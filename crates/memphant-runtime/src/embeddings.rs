//! Real embedding + reranking providers behind the `fastembed` cargo feature.
//!
//! Embeddings: local models via fastembed/onnxruntime. Four measured arms (W8,
//! R0-T1) coexist because the store keys every embedding by profile id =
//! hash(embedder id + dims), so they never mix: `bge-small-en-v1.5` (384d, the
//! default, unchanged), `bge-base-en-v1.5` (768d), `modernbert-embed-large`
//! (1024d), and `embeddinggemma-300m` (768d). The default build stays Noop —
//! no model download in CI or tests.
//!
//! Query/document prefixes (R0-T1): some models are trained with distinct
//! textual prefixes for queries vs documents. [`prefix_text`] is the pure,
//! unit-testable seam for that; [`FastEmbedProvider`] applies it inside
//! `embed`/`embed_query` so call sites never have to know about it. Verified
//! against fastembed 5.17.2's source
//! (`~/.cargo/registry/.../fastembed-5.17.2/src/`): it does NOT apply any of
//! these prefixes internally for these models (no `search_query`,
//! `search_document`, `task: search`, or `title: none` strings anywhere in
//! its source), so `prefix_text` never double-prefixes.
//!
//! Reranking (W8): a local cross-encoder ([`FastEmbedCrossReranker`]) over
//! fastembed's `TextRerank`, implementing the core [`CrossReranker`] seam. The
//! default reranker model is `BAAI/bge-reranker-base` (fastembed's default,
//! ~1.1 GB ONNX download on first use). Like the embedder it downloads lazily
//! into the local fastembed cache and never in the default/CI build.

use std::sync::Mutex;

use fastembed::{
    EmbeddingModel, InitOptions, RerankInitOptions, RerankerModel, TextEmbedding, TextRerank,
};
use memphant_core::{CrossReranker, EmbedError, EmbeddingProvider};

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
    const MODERNBERT_ID: &'static str = "fastembed:modernbert-embed-large";
    const MODERNBERT_DIMENSIONS: usize = 1024;
    const GEMMA_ID: &'static str = "fastembed:embeddinggemma-300m";
    const GEMMA_DIMENSIONS: usize = 768;

    /// The provider identity (`id()`), keyed into the embedding profile.
    pub fn id(self) -> &'static str {
        match self {
            Self::BgeSmallEnV15 => Self::SMALL_ID,
            Self::BgeBaseEnV15 => Self::BASE_ID,
            Self::ModernBertEmbedLarge => Self::MODERNBERT_ID,
            Self::EmbeddingGemma300M => Self::GEMMA_ID,
        }
    }

    /// The embedding dimensionality, keyed into the embedding profile.
    pub fn dimensions(self) -> usize {
        match self {
            Self::BgeSmallEnV15 => Self::SMALL_DIMENSIONS,
            Self::BgeBaseEnV15 => Self::BASE_DIMENSIONS,
            Self::ModernBertEmbedLarge => Self::MODERNBERT_DIMENSIONS,
            Self::EmbeddingGemma300M => Self::GEMMA_DIMENSIONS,
        }
    }

    /// The fastembed model enum this arm loads.
    fn model(self) -> EmbeddingModel {
        match self {
            Self::BgeSmallEnV15 => EmbeddingModel::BGESmallENV15,
            Self::BgeBaseEnV15 => EmbeddingModel::BGEBaseENV15,
            Self::ModernBertEmbedLarge => EmbeddingModel::ModernBertEmbedLarge,
            Self::EmbeddingGemma300M => EmbeddingModel::EmbeddingGemma300M,
        }
    }

    /// Parses the bench `--embed-model` selector
    /// (`small` | `base` | `modernbert` | `gemma`).
    pub fn parse(selector: &str) -> Option<Self> {
        match selector {
            "small" => Some(Self::BgeSmallEnV15),
            "base" => Some(Self::BgeBaseEnV15),
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
        FastEmbedModel::BgeSmallEnV15 | FastEmbedModel::BgeBaseEnV15 => text.to_string(),
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

/// Cross-encoder reranker (W8) over fastembed's `TextRerank`, implementing the
/// core [`CrossReranker`] seam. `rerank` is `&self` (the trait is object-safe
/// and shared behind an `Arc`), but fastembed's inference takes `&mut self`, so
/// a mutex serializes reranking calls exactly like the embedder.
pub struct FastEmbedCrossReranker {
    model: Mutex<TextRerank>,
}

impl FastEmbedCrossReranker {
    /// Initializes `BAAI/bge-reranker-base` (downloads ~1.1 GB into the local
    /// fastembed cache on first use; never in the default/CI build).
    pub fn new() -> Result<Self, EmbedError> {
        let model = TextRerank::try_new(RerankInitOptions::new(RerankerModel::BGERerankerBase))
            .map_err(|error| EmbedError::Unavailable(error.to_string()))?;
        Ok(Self {
            model: Mutex::new(model),
        })
    }
}

impl CrossReranker for FastEmbedCrossReranker {
    fn rerank(&self, query: &str, docs: &[&str]) -> Vec<f32> {
        if docs.is_empty() {
            return Vec::new();
        }
        let mut model = match self.model.lock() {
            Ok(model) => model,
            Err(_) => {
                eprintln!("memphant: cross-reranker mutex poisoned — skipping rerank");
                return Vec::new();
            }
        };
        // fastembed returns results sorted by score DESC, each carrying its
        // input `index`. The core seam expects one score per doc IN INPUT
        // ORDER, so re-scatter by index. `return_documents = false` (we only
        // need the scores); default batch size.
        match model.rerank(query, docs, false, None) {
            Ok(results) => {
                let mut scores = vec![0.0_f32; docs.len()];
                for result in results {
                    if let Some(slot) = scores.get_mut(result.index) {
                        *slot = result.score;
                    }
                }
                scores
            }
            Err(error) => {
                // A length != docs.len() (here 0) signals "no-op" to the core
                // stage, which then leaves the fused order unchanged.
                eprintln!("memphant: cross-reranker inference failed: {error}");
                Vec::new()
            }
        }
    }
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

    /// All four arms, for tests that need to iterate every variant.
    const ALL_ARMS: [FastEmbedModel; 4] = [
        FastEmbedModel::BgeSmallEnV15,
        FastEmbedModel::BgeBaseEnV15,
        FastEmbedModel::ModernBertEmbedLarge,
        FastEmbedModel::EmbeddingGemma300M,
    ];

    #[test]
    fn arm_identity_mapping() {
        assert_eq!(FastEmbedModel::BgeSmallEnV15.id(), FASTEMBED_MODEL_ID);
        assert_eq!(FastEmbedModel::BgeSmallEnV15.dimensions(), 384);
        assert_eq!(
            FastEmbedModel::BgeBaseEnV15.id(),
            "fastembed:bge-base-en-v1.5"
        );
        assert_eq!(FastEmbedModel::BgeBaseEnV15.dimensions(), 768);
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
        assert_eq!(
            FastEmbedModel::parse("modernbert"),
            Some(FastEmbedModel::ModernBertEmbedLarge)
        );
        assert_eq!(
            FastEmbedModel::parse("gemma"),
            Some(FastEmbedModel::EmbeddingGemma300M)
        );
        assert_eq!(FastEmbedModel::parse("bge"), None);
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
        assert_eq!(profiles[3].dimensions, 768);
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
        for &arm in &[FastEmbedModel::BgeSmallEnV15, FastEmbedModel::BgeBaseEnV15] {
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
        let scores = reranker.rerank(query, &docs);
        let elapsed = started.elapsed();
        eprintln!(
            "rerank smoke: {} docs in {} ms",
            docs.len(),
            elapsed.as_millis()
        );
        assert_eq!(scores.len(), docs.len(), "one score per doc in input order");
        // Determinism: a second call yields byte-identical scores.
        let again = reranker.rerank(query, &docs);
        assert_eq!(scores, again, "fastembed inference is deterministic");
        // The on-topic Paris doc must outscore the cell-biology distractor.
        assert!(
            scores[0] > scores[1],
            "the relevant doc scores above the irrelevant one: {scores:?}"
        );
    }
}
