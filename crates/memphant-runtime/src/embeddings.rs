//! Real embedding + reranking providers behind the `fastembed` cargo feature.
//!
//! Embeddings: local models via fastembed/onnxruntime. Five measured arms (W8,
//! R0-T1) coexist because the store keys every embedding by profile id =
//! hash(embedder id + dims), so they never mix: `bge-small-en-v1.5` (384d, the
//! default, unchanged), `bge-base-en-v1.5` (768d), `bge-m3` (1024d),
//! `modernbert-embed-large` (1024d), and `embeddinggemma-300m` (768d). The default build stays Noop —
//! no model download in CI or tests.
//!
//! A sixth arm, Qwen3-Embedding-0.6B ([`Qwen3Provider`], R0-T1b), lives
//! behind the SEPARATE `qwen3` cargo feature: fastembed's candle backend
//! (safetensors download, not the ort/ONNX path the four arms above use).
//! `fastembed::Qwen3TextEmbedding` is its own public type, not a
//! `TextEmbedding`/`EmbeddingModel` variant, so it does not fit
//! [`FastEmbedModel`] and is wired up separately, additive to (never
//! replacing) the `fastembed` feature.
//!
//! Query/document prefixes (R0-T1): some models are trained with distinct
//! textual prefixes for queries vs documents. [`prefix_text`] is the pure,
//! unit-testable seam for the five `FastEmbedModel` arms; [`FastEmbedProvider`]
//! applies it inside `embed`/`embed_query` so call sites never have to know
//! about it. Verified against fastembed 5.17.2's source
//! (`~/.cargo/registry/.../fastembed-5.17.2/src/`): it does NOT apply any of
//! these prefixes internally for these models (no `search_query`,
//! `search_document`, `task: search`, or `title: none` strings anywhere in
//! its source), so `prefix_text` never double-prefixes. Qwen3 gets its own
//! sibling pure fn, [`qwen3_query_instruction`], since it isn't
//! `FastEmbedModel`-typed; verified the same way against
//! `fastembed-5.17.2/src/models/qwen3.rs`'s `Qwen3TextEmbedding::embed`,
//! which tokenizes input texts as-is with no added instruction wrapper.
//!
//! Reranking (W8): a local cross-encoder ([`FastEmbedCrossReranker`]) over
//! fastembed's `TextRerank`, implementing the core [`CrossReranker`] seam. The
//! default reranker model is `BAAI/bge-reranker-base` (fastembed's default,
//! ~1.1 GB ONNX download on first use). Like the embedder it downloads lazily
//! into the local fastembed cache and never in the default/CI build.

use std::sync::Mutex;

#[cfg(feature = "qwen3")]
use candle_core::{DType, Device};
#[cfg(feature = "qwen3")]
use fastembed::Qwen3TextEmbedding;
use fastembed::{
    EmbeddingModel, InitOptions, RerankInitOptions, RerankResult, RerankerModel, TextEmbedding,
    TextRerank,
};
use memphant_core::{CrossReranker, CrossRerankerConfig, EmbedError, EmbeddingProvider};

/// Legacy aliases for the default (small) embedder identity, kept so existing
/// call sites and reports stay byte-identical.
pub const FASTEMBED_MODEL_ID: &str = FastEmbedModel::SMALL_ID;
pub const FASTEMBED_DIMENSIONS: usize = FastEmbedModel::SMALL_DIMENSIONS;

/// The reranker model id recorded in provenance. `BAAI/bge-reranker-base`, a
/// ~278M-param XLM-RoBERTa cross-encoder; fastembed's default reranker.
pub const FASTEMBED_RERANKER_ID: &str = "fastembed:bge-reranker-base";

/// Which fastembed embedding model an arm selects. The id/dims are pure (no
/// model load), so `embedding_profile_for` derives a distinct profile per arm
/// and the bench can record provenance without touching onnxruntime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum FastEmbedModel {
    /// `bge-small-en-v1.5` (384d) — the shipped default, unchanged.
    #[default]
    BgeSmallEnV15,
    /// `bge-base-en-v1.5` (768d) — the W8 measured upgrade arm.
    BgeBaseEnV15,
    /// `BAAI/bge-m3` (1024d) — the multilingual MemSyco retrieval arm.
    BgeM3,
    /// `lightonai/modernbert-embed-large` (1024d) — nomic-style
    /// `search_query:`/`search_document:` prefixing (R0-T1).
    ModernBertEmbedLarge,
    /// `onnx-community/embeddinggemma-300m-ONNX` (768d) — Google's documented
    /// `task: ... | query: ` / `title: ... | text: ` prompt prefixes (R0-T1).
    EmbeddingGemma300M,
}

impl FastEmbedModel {
    const SMALL_ID: &'static str = "fastembed:bge-small-en-v1.5";
    const SMALL_DIMENSIONS: usize = 384;
    const BASE_ID: &'static str = "fastembed:bge-base-en-v1.5";
    const BASE_DIMENSIONS: usize = 768;
    const BGE_M3_ID: &'static str = "fastembed:bge-m3";
    const BGE_M3_DIMENSIONS: usize = 1024;
    const MODERNBERT_ID: &'static str = "fastembed:modernbert-embed-large";
    const MODERNBERT_DIMENSIONS: usize = 1024;
    const GEMMA_ID: &'static str = "fastembed:embeddinggemma-300m";
    const GEMMA_DIMENSIONS: usize = 768;

    /// The provider identity (`id()`), keyed into the embedding profile.
    pub fn id(self) -> &'static str {
        match self {
            Self::BgeSmallEnV15 => Self::SMALL_ID,
            Self::BgeBaseEnV15 => Self::BASE_ID,
            Self::BgeM3 => Self::BGE_M3_ID,
            Self::ModernBertEmbedLarge => Self::MODERNBERT_ID,
            Self::EmbeddingGemma300M => Self::GEMMA_ID,
        }
    }

    /// The embedding dimensionality, keyed into the embedding profile.
    pub fn dimensions(self) -> usize {
        match self {
            Self::BgeSmallEnV15 => Self::SMALL_DIMENSIONS,
            Self::BgeBaseEnV15 => Self::BASE_DIMENSIONS,
            Self::BgeM3 => Self::BGE_M3_DIMENSIONS,
            Self::ModernBertEmbedLarge => Self::MODERNBERT_DIMENSIONS,
            Self::EmbeddingGemma300M => Self::GEMMA_DIMENSIONS,
        }
    }

    /// The fastembed model enum this arm loads.
    fn model(self) -> EmbeddingModel {
        match self {
            Self::BgeSmallEnV15 => EmbeddingModel::BGESmallENV15,
            Self::BgeBaseEnV15 => EmbeddingModel::BGEBaseENV15,
            Self::BgeM3 => EmbeddingModel::BGEM3,
            Self::ModernBertEmbedLarge => EmbeddingModel::ModernBertEmbedLarge,
            Self::EmbeddingGemma300M => EmbeddingModel::EmbeddingGemma300M,
        }
    }

    /// Parses the bench `--embed-model` selector
    /// (`small` | `base` | `bge-m3` | `modernbert` | `gemma`).
    pub fn parse(selector: &str) -> Option<Self> {
        match selector {
            "small" => Some(Self::BgeSmallEnV15),
            "base" => Some(Self::BgeBaseEnV15),
            "bge-m3" => Some(Self::BgeM3),
            "modernbert" => Some(Self::ModernBertEmbedLarge),
            "gemma" => Some(Self::EmbeddingGemma300M),
            _ => None,
        }
    }
}

/// Distinguishes a recall-time QUERY text from an index-time DOCUMENT text —
/// the two sides of the per-model prefix convention in [`prefix_text`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextKind {
    Query,
    Document,
}

/// Applies a fastembed model's query/document prefix convention to `text`.
/// Pure (no model load, no I/O) so it is unit-testable without downloading
/// any ONNX weights.
///
/// - bge-small, bge-base: NO prefix for either kind — byte-identical to
///   today. Binding baseline-integrity guarantee (pre-registered in the R0-T1
///   brief).
/// - modernbert-embed-large: nomic-style `"search_query: "` (query) /
///   `"search_document: "` (document), per the lightonai/modernbert-embed-large
///   model card ("The model is trained similarly to Nomic Embed and REQUIRES
///   prefixes to be added to the input").
/// - embeddinggemma-300m: Google's documented prompts, `"task: search result
///   | query: "` (query) / `"title: none | text: "` (document), per the
///   EmbeddingGemma model card's default search-task prompt templates.
///
/// fastembed 5.17.2 does not apply any of these prefixes internally (checked
/// against its source — see the module docs), so this never double-prefixes.
pub fn prefix_text(model: FastEmbedModel, kind: TextKind, text: &str) -> String {
    match model {
        FastEmbedModel::BgeSmallEnV15 | FastEmbedModel::BgeBaseEnV15 | FastEmbedModel::BgeM3 => {
            text.to_string()
        }
        FastEmbedModel::ModernBertEmbedLarge => {
            let prefix = match kind {
                TextKind::Query => "search_query: ",
                TextKind::Document => "search_document: ",
            };
            format!("{prefix}{text}")
        }
        FastEmbedModel::EmbeddingGemma300M => {
            let prefix = match kind {
                TextKind::Query => "task: search result | query: ",
                TextKind::Document => "title: none | text: ",
            };
            format!("{prefix}{text}")
        }
    }
}

pub struct FastEmbedProvider {
    // fastembed's `embed` takes `&mut self` (onnx session state); the provider
    // trait is `&self`, so a mutex serializes embedding calls.
    session: Mutex<TextEmbedding>,
    arm: FastEmbedModel,
    id: &'static str,
    dimensions: usize,
}

impl FastEmbedProvider {
    /// Initializes the default `bge-small-en-v1.5` (downloads the model on first
    /// use into the local fastembed cache; never in the default/CI build).
    pub fn new() -> Result<Self, EmbedError> {
        Self::with_model(FastEmbedModel::default())
    }

    /// Initializes a chosen embedding arm. The arms coexist in the store via
    /// distinct embedding profiles (id+dims), so ingest and recall must always
    /// select the SAME arm — the caller derives it once and shares it.
    pub fn with_model(model: FastEmbedModel) -> Result<Self, EmbedError> {
        let embedding = TextEmbedding::try_new(InitOptions::new(model.model()))
            .map_err(|error| EmbedError::Unavailable(error.to_string()))?;
        Ok(Self {
            session: Mutex::new(embedding),
            arm: model,
            id: model.id(),
            dimensions: model.dimensions(),
        })
    }

    /// Shared embed path: applies the arm's [`prefix_text`] convention (a
    /// no-op for bge-small/base) to each text for the given `kind`, then runs
    /// onnx inference. Prefixing lives here — inside the provider — so
    /// `embed`/`embed_query` call sites never need to know about it.
    fn embed_kind(&self, texts: &[String], kind: TextKind) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prefixed: Vec<String> = texts
            .iter()
            .map(|text| prefix_text(self.arm, kind, text))
            .collect();
        let mut session = self
            .session
            .lock()
            .map_err(|_| EmbedError::Unavailable("embedding model mutex poisoned".to_string()))?;
        session
            .embed(&prefixed, None)
            .map_err(|error| EmbedError::Unavailable(error.to_string()))
    }
}

impl EmbeddingProvider for FastEmbedProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        self.embed_kind(texts, TextKind::Document)
    }

    fn embed_query(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        self.embed_kind(texts, TextKind::Query)
    }

    fn dimensions(&self) -> usize {
        self.dimensions
    }

    fn id(&self) -> &str {
        self.id
    }
}

/// The Qwen3-Embedding-0.6B (R0-T1b) provider identity, keyed into the
/// embedding profile.
#[cfg(feature = "qwen3")]
pub const QWEN3_MODEL_ID: &str = "fastembed:qwen3-embedding-0.6b";
/// Qwen3-Embedding-0.6B's hidden size — verified against the model card and
/// fastembed's own bundled `qwen3_06b_embed` test, which asserts
/// `emb.len() == model.config().hidden_size`.
#[cfg(feature = "qwen3")]
pub const QWEN3_DIMENSIONS: usize = 1024;
#[cfg(feature = "qwen3")]
const QWEN3_REPO_ID: &str = "Qwen/Qwen3-Embedding-0.6B";
// 8192, not fastembed's 512-token quick-test default: matches the official
// reference-score test in fastembed's own bundled `tests/qwen3.rs`
// (`qwen3_06b_reference_scores`) and covers our long (~1500 char) smoke texts
// with headroom; Qwen3-Embedding supports up to 32k tokens of context.
#[cfg(feature = "qwen3")]
const QWEN3_MAX_LENGTH: usize = 8192;

/// Qwen3's default query instruction for the "web search" task, per the
/// official HF model card's (https://huggingface.co/Qwen/Qwen3-Embedding-0.6B)
/// `get_detailed_instruct` example: `f'Instruct: {task_description}\nQuery:
/// {query}'`. Verified there is NO space between `"Query:"` and the query
/// text — confirmed against both the model card's published text and
/// fastembed's own bundled `qwen3_06b_reference_scores` test (which
/// reproduces the model card's published cosine scores using this exact,
/// space-free template). Pure (no model load), so unit-testable without a
/// download.
///
/// Documents are embedded RAW — no instruction wrapper — matching the model
/// card's asymmetric convention (only queries carry a task instruction).
#[cfg(feature = "qwen3")]
pub fn qwen3_query_instruction(text: &str) -> String {
    format!(
        "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:{text}"
    )
}

/// Local Qwen3-Embedding-0.6B (R0-T1b) via fastembed's `qwen3` cargo feature
/// (candle backend). `fastembed::Qwen3TextEmbedding::embed` takes `&self`
/// (unlike [`FastEmbedProvider`]'s onnx session, which needs `&mut self`), so
/// no interior `Mutex` is needed here.
#[cfg(feature = "qwen3")]
pub struct Qwen3Provider {
    model: Qwen3TextEmbedding,
}

#[cfg(feature = "qwen3")]
impl Qwen3Provider {
    /// Loads the model from `Qwen/Qwen3-Embedding-0.6B` on Hugging Face
    /// (downloads ~1.2 GB of safetensors into the local fastembed cache on
    /// first use; never in the default/CI build, since this whole type is
    /// behind the additive `qwen3` feature).
    ///
    /// Fails fast: embeds one throwaway text immediately and asserts the
    /// real output width matches [`QWEN3_DIMENSIONS`], so a dims mismatch
    /// surfaces here at construction time rather than deep in a later
    /// recall/store path.
    pub fn new() -> Result<Self, EmbedError> {
        let device = Device::Cpu;
        let model =
            Qwen3TextEmbedding::from_hf(QWEN3_REPO_ID, &device, DType::F32, QWEN3_MAX_LENGTH)
                .map_err(|error| EmbedError::Unavailable(error.to_string()))?;

        let probe = model
            .embed(&["dimension probe"])
            .map_err(|error| EmbedError::Unavailable(error.to_string()))?;
        let actual_dims = probe.first().map(Vec::len).unwrap_or(0);
        if actual_dims != QWEN3_DIMENSIONS {
            return Err(EmbedError::Unavailable(format!(
                "qwen3 embedding dims mismatch: declared {QWEN3_DIMENSIONS}, model produced {actual_dims}"
            )));
        }

        Ok(Self { model })
    }
}

#[cfg(feature = "qwen3")]
impl EmbeddingProvider for Qwen3Provider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.model
            .embed(texts)
            .map_err(|error| EmbedError::Unavailable(error.to_string()))
    }

    fn embed_query(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let prefixed: Vec<String> = texts
            .iter()
            .map(|text| qwen3_query_instruction(text))
            .collect();
        self.model
            .embed(&prefixed)
            .map_err(|error| EmbedError::Unavailable(error.to_string()))
    }

    fn dimensions(&self) -> usize {
        QWEN3_DIMENSIONS
    }

    fn id(&self) -> &str {
        QWEN3_MODEL_ID
    }
}

/// Cross-encoder reranker (W8) over fastembed's `TextRerank`, implementing the
/// core [`CrossReranker`] seam. `rerank` is `&self` (the trait is object-safe
/// and shared behind an `Arc`), but fastembed's inference takes `&mut self`, so
/// a mutex serializes reranking calls exactly like the embedder.
pub struct FastEmbedCrossReranker {
    model: Mutex<TextRerank>,
    config: CrossRerankerConfig,
}

impl FastEmbedCrossReranker {
    /// Initializes `BAAI/bge-reranker-base` (downloads ~1.1 GB into the local
    /// fastembed cache on first use; never in the default/CI build).
    pub fn new() -> Result<Self, EmbedError> {
        Self::with_config(CrossRerankerConfig {
            provider: "fastembed".to_string(),
            model: FASTEMBED_RERANKER_ID.to_string(),
            candidate_limit: memphant_core::DEFAULT_RECALL_POOL_DEPTH,
            max_length: 512,
            batch_size: Some(256),
        })
    }

    pub fn with_config(config: CrossRerankerConfig) -> Result<Self, EmbedError> {
        let options = RerankInitOptions::new(RerankerModel::BGERerankerBase)
            .with_max_length(config.max_length);
        let model = TextRerank::try_new(options)
            .map_err(|error| EmbedError::Unavailable(error.to_string()))?;
        Ok(Self {
            model: Mutex::new(model),
            config,
        })
    }
}

impl CrossReranker for FastEmbedCrossReranker {
    fn config(&self) -> CrossRerankerConfig {
        self.config.clone()
    }

    fn rerank(&self, query: &str, docs: &[&str]) -> Result<Vec<f32>, String> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }
        let mut model = match self.model.lock() {
            Ok(model) => model,
            Err(_) => {
                return Err("cross-reranker mutex poisoned".to_string());
            }
        };
        // fastembed returns results sorted by score DESC, each carrying its
        // input `index`. The core seam expects one score per doc IN INPUT
        // ORDER, so re-scatter by index. `return_documents = false` (we only
        // need the scores); configured batch size or fastembed's default.
        match model.rerank(query, docs, false, self.config.batch_size) {
            Ok(results) => scatter_rerank_results(results, docs.len()),
            Err(error) => Err(error.to_string()),
        }
    }
}

fn scatter_rerank_results(
    results: Vec<RerankResult>,
    input_count: usize,
) -> Result<Vec<f32>, String> {
    if results.len() != input_count {
        return Err(format!(
            "reranker returned {} results for {input_count} inputs",
            results.len()
        ));
    }
    let mut scores = vec![0.0; input_count];
    let mut seen = vec![false; input_count];
    for result in results {
        let Some(slot) = scores.get_mut(result.index) else {
            return Err(format!(
                "reranker returned out-of-range index {}",
                result.index
            ));
        };
        if seen[result.index] {
            return Err(format!(
                "reranker returned duplicate index {}",
                result.index
            ));
        }
        seen[result.index] = true;
        *slot = result.score;
    }
    if seen.iter().any(|seen| !seen) {
        return Err("reranker omitted an input index".to_string());
    }
    Ok(scores)
}

#[cfg(test)]
mod tests {
    use super::*;
    use memphant_core::embedding_profile_for;

    /// A pure adapter reporting a model arm's identity WITHOUT loading onnx, so
    /// the profile-distinctness check needs no model download.
    struct ArmIdentity(FastEmbedModel);
    impl EmbeddingProvider for ArmIdentity {
        fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            Ok(Vec::new())
        }
        fn dimensions(&self) -> usize {
            self.0.dimensions()
        }
        fn id(&self) -> &str {
            self.0.id()
        }
    }

    /// All five arms, for tests that need to iterate every variant.
    const ALL_ARMS: [FastEmbedModel; 5] = [
        FastEmbedModel::BgeSmallEnV15,
        FastEmbedModel::BgeBaseEnV15,
        FastEmbedModel::BgeM3,
        FastEmbedModel::ModernBertEmbedLarge,
        FastEmbedModel::EmbeddingGemma300M,
    ];

    fn rerank_result(index: usize, score: f32) -> fastembed::RerankResult {
        fastembed::RerankResult {
            document: None,
            score,
            index,
        }
    }

    #[test]
    fn rerank_result_scatter_requires_one_unique_in_range_result_per_input() {
        assert_eq!(
            scatter_rerank_results(
                vec![
                    rerank_result(2, 0.3),
                    rerank_result(0, 0.9),
                    rerank_result(1, 0.5)
                ],
                3,
            )
            .expect("valid scatter"),
            vec![0.9, 0.5, 0.3]
        );

        assert!(scatter_rerank_results(vec![rerank_result(0, 1.0)], 2).is_err());
        assert!(
            scatter_rerank_results(vec![rerank_result(0, 1.0), rerank_result(2, 0.5)], 2).is_err()
        );
        assert!(
            scatter_rerank_results(vec![rerank_result(0, 1.0), rerank_result(0, 0.5)], 2).is_err()
        );
    }

    #[test]
    fn arm_identity_mapping() {
        assert_eq!(FastEmbedModel::BgeSmallEnV15.id(), FASTEMBED_MODEL_ID);
        assert_eq!(FastEmbedModel::BgeSmallEnV15.dimensions(), 384);
        assert_eq!(
            FastEmbedModel::BgeBaseEnV15.id(),
            "fastembed:bge-base-en-v1.5"
        );
        assert_eq!(FastEmbedModel::BgeBaseEnV15.dimensions(), 768);
        assert_eq!(FastEmbedModel::BgeM3.id(), "fastembed:bge-m3");
        assert_eq!(FastEmbedModel::BgeM3.dimensions(), 1024);
        assert_eq!(
            FastEmbedModel::ModernBertEmbedLarge.id(),
            "fastembed:modernbert-embed-large"
        );
        assert_eq!(FastEmbedModel::ModernBertEmbedLarge.dimensions(), 1024);
        assert_eq!(
            FastEmbedModel::EmbeddingGemma300M.id(),
            "fastembed:embeddinggemma-300m"
        );
        assert_eq!(FastEmbedModel::EmbeddingGemma300M.dimensions(), 768);
    }

    #[test]
    fn selector_parses_all_arms() {
        assert_eq!(
            FastEmbedModel::parse("small"),
            Some(FastEmbedModel::BgeSmallEnV15)
        );
        assert_eq!(
            FastEmbedModel::parse("base"),
            Some(FastEmbedModel::BgeBaseEnV15)
        );
        assert_eq!(FastEmbedModel::parse("bge-m3"), Some(FastEmbedModel::BgeM3));
        assert_eq!(
            FastEmbedModel::parse("modernbert"),
            Some(FastEmbedModel::ModernBertEmbedLarge)
        );
        assert_eq!(
            FastEmbedModel::parse("gemma"),
            Some(FastEmbedModel::EmbeddingGemma300M)
        );
        assert_eq!(FastEmbedModel::parse("bge"), None);
        // Qwen3-Embedding-0.6B (R0-T1b) is intentionally NOT a `FastEmbedModel`
        // variant — `fastembed::Qwen3TextEmbedding` is a separate public type,
        // not a `TextEmbedding`/`EmbeddingModel` arm — so it must never parse
        // here. The `--embed-model qwen3` selector is handled as a sibling
        // case in the eval crate's CLI, dispatching to `Qwen3Provider`.
        assert_eq!(FastEmbedModel::parse("qwen3"), None);
    }

    #[test]
    fn arms_derive_distinct_embedding_profiles() {
        // The whole "coexist cleanly" claim: every arm keys a different
        // profile id, so their stored vectors never mix under `<=>`.
        let profiles: Vec<_> = ALL_ARMS
            .iter()
            .map(|&arm| embedding_profile_for(&ArmIdentity(arm)))
            .collect();
        for (left_index, left) in profiles.iter().enumerate() {
            for (right_index, right) in profiles.iter().enumerate() {
                if left_index != right_index {
                    assert_ne!(
                        left.id, right.id,
                        "arms {:?} and {:?} must derive distinct profiles",
                        ALL_ARMS[left_index], ALL_ARMS[right_index]
                    );
                }
            }
        }
        assert_eq!(profiles[0].dimensions, 384);
        assert_eq!(profiles[1].dimensions, 768);
        assert_eq!(profiles[2].dimensions, 1024);
        assert_eq!(profiles[3].dimensions, 1024);
        assert_eq!(profiles[4].dimensions, 768);
    }

    /// Binding contract: our declared `dimensions()` for every arm must match
    /// fastembed's own model-info metadata. `list_supported_models()` is a
    /// pure static lookup (no ONNX session, no download), so this runs in the
    /// default test suite.
    #[test]
    fn declared_dims_match_fastembed_metadata() {
        let supported = TextEmbedding::list_supported_models();
        for &arm in &ALL_ARMS {
            let info = supported
                .iter()
                .find(|info| info.model == arm.model())
                .unwrap_or_else(|| panic!("{arm:?} missing from fastembed's supported models"));
            assert_eq!(
                arm.dimensions(),
                info.dim,
                "{arm:?} declared dims must match fastembed's model-info dims"
            );
        }
    }

    #[test]
    fn bge_arms_are_never_prefixed() {
        // Binding baseline-integrity guarantee: bge-small/base text is
        // byte-identical to the input, for either kind.
        for &arm in &[
            FastEmbedModel::BgeSmallEnV15,
            FastEmbedModel::BgeBaseEnV15,
            FastEmbedModel::BgeM3,
        ] {
            assert_eq!(prefix_text(arm, TextKind::Query, "hello"), "hello");
            assert_eq!(prefix_text(arm, TextKind::Document, "hello"), "hello");
        }
    }

    #[test]
    fn modernbert_uses_nomic_style_prefixes() {
        assert_eq!(
            prefix_text(
                FastEmbedModel::ModernBertEmbedLarge,
                TextKind::Query,
                "What is TSNE?"
            ),
            "search_query: What is TSNE?"
        );
        assert_eq!(
            prefix_text(
                FastEmbedModel::ModernBertEmbedLarge,
                TextKind::Document,
                "TSNE is a dimensionality reduction algorithm."
            ),
            "search_document: TSNE is a dimensionality reduction algorithm."
        );
    }

    #[test]
    fn gemma_uses_documented_task_prompts() {
        assert_eq!(
            prefix_text(
                FastEmbedModel::EmbeddingGemma300M,
                TextKind::Query,
                "capital of France"
            ),
            "task: search result | query: capital of France"
        );
        assert_eq!(
            prefix_text(
                FastEmbedModel::EmbeddingGemma300M,
                TextKind::Document,
                "Paris is the capital of France."
            ),
            "title: none | text: Paris is the capital of France."
        );
    }

    #[test]
    fn prefix_text_query_and_document_differ_for_prefixed_arms() {
        // The whole point of the seam: a prefixed arm must NOT embed queries
        // and documents identically, or the query/document distinction is a
        // no-op in practice.
        for &arm in &[
            FastEmbedModel::ModernBertEmbedLarge,
            FastEmbedModel::EmbeddingGemma300M,
        ] {
            assert_ne!(
                prefix_text(arm, TextKind::Query, "same text"),
                prefix_text(arm, TextKind::Document, "same text"),
                "{arm:?} must prefix queries and documents differently"
            );
        }
    }

    /// A pure adapter reporting the Qwen3 arm's identity WITHOUT loading
    /// candle/safetensors, mirroring [`ArmIdentity`] for the four
    /// `FastEmbedModel` arms above.
    #[cfg(feature = "qwen3")]
    struct Qwen3Identity;
    #[cfg(feature = "qwen3")]
    impl EmbeddingProvider for Qwen3Identity {
        fn embed(&self, _texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
            Ok(Vec::new())
        }
        fn dimensions(&self) -> usize {
            QWEN3_DIMENSIONS
        }
        fn id(&self) -> &str {
            QWEN3_MODEL_ID
        }
    }

    #[cfg(feature = "qwen3")]
    #[test]
    fn qwen3_profile_is_distinct_from_all_fastembed_arms() {
        // The R0-T1b arm must key its own profile, distinct from every T1
        // arm, so its stored vectors never mix with theirs under `<=>`.
        let qwen3_profile = embedding_profile_for(&Qwen3Identity);
        assert_eq!(qwen3_profile.dimensions, 1024);
        for &arm in &ALL_ARMS {
            let arm_profile = embedding_profile_for(&ArmIdentity(arm));
            assert_ne!(
                qwen3_profile.id, arm_profile.id,
                "qwen3 and {arm:?} must derive distinct profiles"
            );
        }
    }

    #[cfg(feature = "qwen3")]
    #[test]
    fn qwen3_query_instruction_matches_model_card_exact_string() {
        // Verified against https://huggingface.co/Qwen/Qwen3-Embedding-0.6B's
        // `get_detailed_instruct` example AND fastembed's own bundled
        // `qwen3_06b_reference_scores` test (which reproduces the model
        // card's published cosine scores using this exact template): NO
        // space between "Query:" and the query text.
        assert_eq!(
            qwen3_query_instruction("What is the capital of China?"),
            "Instruct: Given a web search query, retrieve relevant passages that answer the query\nQuery:What is the capital of China?"
        );
    }

    #[cfg(feature = "qwen3")]
    #[test]
    fn qwen3_query_instruction_differs_from_raw_document_text() {
        // Documents stay raw; only queries get the instruction wrapper. The
        // provider's `embed` passes texts through unchanged while
        // `embed_query` applies `qwen3_query_instruction` — this locks in
        // that the two are never byte-identical for the same input.
        let text = "same text";
        assert_ne!(qwen3_query_instruction(text), text);
    }

    /// Env-gated real-model smoke: loads Qwen3-Embedding-0.6B (candle
    /// backend, ~1.2 GB safetensors download) and times two batches — 32
    /// short (~1 sentence) and 8 long (~1500 char) texts — printing
    /// wall-clock and texts/sec for each. `#[ignore]` keeps it out of the
    /// default suite; run with `MEMPHANT_QWEN3_SMOKE=1`. This is the
    /// CPU-viability pre-gate the R0 bakeoff controller reads before running
    /// the full campaign.
    #[cfg(feature = "qwen3")]
    #[test]
    #[ignore = "downloads Qwen3-Embedding-0.6B (~1.2 GB); run with MEMPHANT_QWEN3_SMOKE=1"]
    fn qwen3_latency_smoke_real_model() {
        if std::env::var("MEMPHANT_QWEN3_SMOKE").as_deref() != Ok("1") {
            eprintln!("qwen3 smoke skipped (set MEMPHANT_QWEN3_SMOKE=1 to run)");
            return;
        }
        let provider = Qwen3Provider::new().expect("load qwen3-embedding-0.6b");

        let short_texts: Vec<String> = (0..32)
            .map(|index| format!("Sentence number {index} about a short everyday topic."))
            .collect();

        let long_sentence = "This is a longer passage meant to simulate a realistic memory \
            document body that a user might store, repeating enough context to reach a \
            document-scale length so the smoke test measures throughput on long inputs \
            rather than single sentences. ";
        let mut long_base = String::new();
        while long_base.len() < 1500 {
            long_base.push_str(long_sentence);
        }
        let long_texts: Vec<String> = (0..8)
            .map(|index| format!("[doc {index}] {long_base}"))
            .collect();
        for text in &long_texts {
            assert!(
                text.len() >= 1500,
                "long fixture text too short: {} chars",
                text.len()
            );
        }

        let started = std::time::Instant::now();
        let short_vectors = provider.embed(&short_texts).expect("embed short batch");
        let short_elapsed = started.elapsed();
        assert_eq!(short_vectors.len(), short_texts.len());
        assert_eq!(
            short_vectors[0].len(),
            QWEN3_DIMENSIONS,
            "qwen3 must produce 1024-d vectors"
        );

        let started = std::time::Instant::now();
        let long_vectors = provider.embed(&long_texts).expect("embed long batch");
        let long_elapsed = started.elapsed();
        assert_eq!(long_vectors.len(), long_texts.len());

        let short_tps = short_texts.len() as f64 / short_elapsed.as_secs_f64();
        let long_tps = long_texts.len() as f64 / long_elapsed.as_secs_f64();
        eprintln!(
            "qwen3 smoke: short batch {} texts in {} ms ({short_tps:.2} texts/sec)",
            short_texts.len(),
            short_elapsed.as_millis()
        );
        eprintln!(
            "qwen3 smoke: long batch {} texts in {} ms ({long_tps:.2} texts/sec)",
            long_texts.len(),
            long_elapsed.as_millis()
        );
    }

    /// Env-gated real-model smoke: loads `bge-reranker-base` and reranks a tiny
    /// query/doc set ONLY when `MEMPHANT_RERANK_SMOKE=1`. `#[ignore]` keeps it
    /// out of the default suite (it downloads ~1.1 GB and runs onnx inference).
    #[test]
    #[ignore = "downloads bge-reranker-base (~1.1 GB); run with MEMPHANT_RERANK_SMOKE=1"]
    fn rerank_smoke_real_model() {
        if std::env::var("MEMPHANT_RERANK_SMOKE").as_deref() != Ok("1") {
            eprintln!("rerank smoke skipped (set MEMPHANT_RERANK_SMOKE=1 to run)");
            return;
        }
        let reranker = FastEmbedCrossReranker::new().expect("load bge-reranker-base");
        let query = "What is the capital of France?";
        let docs = [
            "Paris is the capital and most populous city of France.",
            "The mitochondria is the powerhouse of the cell.",
            "France is a country in Western Europe.",
        ];
        let started = std::time::Instant::now();
        let scores = reranker.rerank(query, &docs).expect("rerank");
        let elapsed = started.elapsed();
        eprintln!(
            "rerank smoke: {} docs in {} ms",
            docs.len(),
            elapsed.as_millis()
        );
        assert_eq!(scores.len(), docs.len(), "one score per doc in input order");
        // Determinism: a second call yields byte-identical scores.
        let again = reranker.rerank(query, &docs).expect("rerank again");
        assert_eq!(scores, again, "fastembed inference is deterministic");
        // The on-topic Paris doc must outscore the cell-biology distractor.
        assert!(
            scores[0] > scores[1],
            "the relevant doc scores above the irrelevant one: {scores:?}"
        );
    }

    fn representative_rerank_docs(candidate_count: usize) -> Vec<String> {
        let sentence = "This representative long memory passage carries enough surrounding context to exercise tokenizer truncation and realistic reranker inference rather than a one-line synthetic document. ";
        let mut body = String::new();
        while body.len() < 1_500 {
            body.push_str(sentence);
        }
        (0..candidate_count)
            .map(|index| {
                if index == 0 {
                    format!("Paris is the capital and most populous city of France. {body}")
                } else {
                    format!("Unrelated distractor document {index}. {body}")
                }
            })
            .collect()
    }

    /// R1.5-T1 live smoke: a SINGLE `rerank` call over
    /// `DEFAULT_RECALL_POOL_DEPTH` (64) docs — the exact shape
    /// `cross_rerank_candidates` issues in production (one batched call per
    /// recall, not one call per doc). Same `MEMPHANT_RERANK_SMOKE=1` gate and
    /// `#[ignore]` as `rerank_smoke_real_model`; run once, record the
    /// printed ms against the R1.5 plan's 1.5s p95 viability ceiling (a
    /// single-run number here is reported, not gated — T2 owns the real p95).
    #[test]
    #[ignore = "downloads bge-reranker-base (~1.1 GB); run with MEMPHANT_RERANK_SMOKE=1"]
    fn rerank_smoke_64_docs_pool_depth_latency() {
        if std::env::var("MEMPHANT_RERANK_SMOKE").as_deref() != Ok("1") {
            eprintln!("rerank smoke skipped (set MEMPHANT_RERANK_SMOKE=1 to run)");
            return;
        }
        let reranker = FastEmbedCrossReranker::new().expect("load bge-reranker-base");
        let query = "What is the capital of France?";
        let pool_depth = memphant_core::DEFAULT_RECALL_POOL_DEPTH;
        let docs = representative_rerank_docs(pool_depth);
        let doc_refs: Vec<&str> = docs.iter().map(String::as_str).collect();
        let started = std::time::Instant::now();
        let scores = reranker.rerank(query, &doc_refs).expect("rerank");
        let elapsed = started.elapsed();
        eprintln!(
            "rerank smoke (pool_depth={pool_depth}): {} docs in {} ms",
            doc_refs.len(),
            elapsed.as_millis()
        );
        assert_eq!(
            scores.len(),
            doc_refs.len(),
            "one score per doc in input order"
        );
        assert!(
            scores[0] > scores[1],
            "the relevant doc scores above a distractor: {scores:?}"
        );
    }

    #[test]
    #[ignore = "downloads bge-reranker-base (~1.1 GB); run with MEMPHANT_RERANK_SMOKE=1"]
    fn rerank_real_model_latency_matrix() {
        if std::env::var("MEMPHANT_RERANK_SMOKE").as_deref() != Ok("1") {
            eprintln!("rerank matrix skipped (set MEMPHANT_RERANK_SMOKE=1 to run)");
            return;
        }
        let query = "What is the capital of France?";
        for (candidate_count, max_length) in [(64, 512), (32, 512), (32, 256), (32, 128)] {
            let batch_size = 256;
            let reranker = FastEmbedCrossReranker::with_config(CrossRerankerConfig {
                provider: "fastembed".to_string(),
                model: FASTEMBED_RERANKER_ID.to_string(),
                candidate_limit: candidate_count,
                max_length,
                batch_size: Some(batch_size),
            })
            .expect("load bge-reranker-base");
            let docs = representative_rerank_docs(candidate_count);
            let doc_refs = docs.iter().map(String::as_str).collect::<Vec<_>>();
            let started = std::time::Instant::now();
            let scores = reranker.rerank(query, &doc_refs).expect("rerank arm");
            let elapsed_ms = started.elapsed().as_millis();
            eprintln!(
                "{{\"event\":\"memphant_rerank_latency\",\"candidate_count\":{candidate_count},\"max_length\":{max_length},\"batch_size\":{batch_size},\"elapsed_ms\":{elapsed_ms}}}"
            );
            assert_eq!(scores.len(), candidate_count);
            assert!(scores.iter().all(|score| score.is_finite()));
        }
    }
}
