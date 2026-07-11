//! Real embedding + reranking providers behind the `fastembed` cargo feature.
//!
//! Embeddings: a local bge model via fastembed/onnxruntime. Two measured arms
//! (W8) coexist because the store keys every embedding by profile id =
//! hash(embedder id + dims), so they never mix: `bge-small-en-v1.5` (384d, the
//! default, unchanged) and `bge-base-en-v1.5` (768d). The default build stays
//! Noop — no model download in CI or tests.
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
}

impl FastEmbedModel {
    const SMALL_ID: &'static str = "fastembed:bge-small-en-v1.5";
    const SMALL_DIMENSIONS: usize = 384;
    const BASE_ID: &'static str = "fastembed:bge-base-en-v1.5";
    const BASE_DIMENSIONS: usize = 768;

    /// The provider identity (`id()`), keyed into the embedding profile.
    pub fn id(self) -> &'static str {
        match self {
            Self::BgeSmallEnV15 => Self::SMALL_ID,
            Self::BgeBaseEnV15 => Self::BASE_ID,
        }
    }

    /// The embedding dimensionality, keyed into the embedding profile.
    pub fn dimensions(self) -> usize {
        match self {
            Self::BgeSmallEnV15 => Self::SMALL_DIMENSIONS,
            Self::BgeBaseEnV15 => Self::BASE_DIMENSIONS,
        }
    }

    /// The fastembed model enum this arm loads.
    fn model(self) -> EmbeddingModel {
        match self {
            Self::BgeSmallEnV15 => EmbeddingModel::BGESmallENV15,
            Self::BgeBaseEnV15 => EmbeddingModel::BGEBaseENV15,
        }
    }

    /// Parses the bench `--embed-model` selector (`small` | `base`).
    pub fn parse(selector: &str) -> Option<Self> {
        match selector {
            "small" => Some(Self::BgeSmallEnV15),
            "base" => Some(Self::BgeBaseEnV15),
            _ => None,
        }
    }
}

pub struct FastEmbedProvider {
    // fastembed's `embed` takes `&mut self` (onnx session state); the provider
    // trait is `&self`, so a mutex serializes embedding calls.
    model: Mutex<TextEmbedding>,
    id: &'static str,
    dimensions: usize,
}

impl FastEmbedProvider {
    /// Initializes the default `bge-small-en-v1.5` (downloads the model on first
    /// use into the local fastembed cache; never in the default/CI build).
    pub fn new() -> Result<Self, EmbedError> {
        Self::with_model(FastEmbedModel::default())
    }

    /// Initializes a chosen embedding arm. The two arms coexist in the store via
    /// distinct embedding profiles (id+dims), so ingest and recall must always
    /// select the SAME arm — the caller derives it once and shares it.
    pub fn with_model(model: FastEmbedModel) -> Result<Self, EmbedError> {
        let embedding = TextEmbedding::try_new(InitOptions::new(model.model()))
            .map_err(|error| EmbedError::Unavailable(error.to_string()))?;
        Ok(Self {
            model: Mutex::new(embedding),
            id: model.id(),
            dimensions: model.dimensions(),
        })
    }
}

impl EmbeddingProvider for FastEmbedProvider {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let mut model = self
            .model
            .lock()
            .map_err(|_| EmbedError::Unavailable("embedding model mutex poisoned".to_string()))?;
        model
            .embed(texts, None)
            .map_err(|error| EmbedError::Unavailable(error.to_string()))
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

    #[test]
    fn arm_identity_mapping() {
        assert_eq!(FastEmbedModel::BgeSmallEnV15.id(), FASTEMBED_MODEL_ID);
        assert_eq!(FastEmbedModel::BgeSmallEnV15.dimensions(), 384);
        assert_eq!(
            FastEmbedModel::BgeBaseEnV15.id(),
            "fastembed:bge-base-en-v1.5"
        );
        assert_eq!(FastEmbedModel::BgeBaseEnV15.dimensions(), 768);
    }

    #[test]
    fn selector_parses_both_arms() {
        assert_eq!(
            FastEmbedModel::parse("small"),
            Some(FastEmbedModel::BgeSmallEnV15)
        );
        assert_eq!(
            FastEmbedModel::parse("base"),
            Some(FastEmbedModel::BgeBaseEnV15)
        );
        assert_eq!(FastEmbedModel::parse("bge"), None);
    }

    #[test]
    fn arms_derive_distinct_embedding_profiles() {
        // The whole "coexist cleanly" claim: small and base key different
        // profile ids, so their stored vectors never mix under `<=>`.
        let small = embedding_profile_for(&ArmIdentity(FastEmbedModel::BgeSmallEnV15));
        let base = embedding_profile_for(&ArmIdentity(FastEmbedModel::BgeBaseEnV15));
        assert_ne!(
            small.id, base.id,
            "distinct embedder arms → distinct profiles"
        );
        assert_eq!(small.dimensions, 384);
        assert_eq!(base.dimensions, 768);
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
