//! Real embedding provider behind the `fastembed` cargo feature: a local
//! bge-small-en-v1.5 model (384 dimensions) via fastembed/onnxruntime. The
//! default build stays Noop — no model download in CI or tests.

use std::sync::Mutex;

use fastembed::{EmbeddingModel, InitOptions, TextEmbedding};
use memphant_core::{EmbedError, EmbeddingProvider};

pub const FASTEMBED_MODEL_ID: &str = "fastembed:bge-small-en-v1.5";
pub const FASTEMBED_DIMENSIONS: usize = 384;

pub struct FastEmbedProvider {
    // fastembed's `embed` takes `&mut self` (onnx session state); the provider
    // trait is `&self`, so a mutex serializes embedding calls.
    model: Mutex<TextEmbedding>,
}

impl FastEmbedProvider {
    /// Initializes bge-small-en-v1.5 (downloads the model on first use into
    /// the local fastembed cache; never in the default/CI build).
    pub fn new() -> Result<Self, EmbedError> {
        let model = TextEmbedding::try_new(InitOptions::new(EmbeddingModel::BGESmallENV15))
            .map_err(|error| EmbedError::Unavailable(error.to_string()))?;
        Ok(Self {
            model: Mutex::new(model),
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
        FASTEMBED_DIMENSIONS
    }

    fn id(&self) -> &str {
        FASTEMBED_MODEL_ID
    }
}
