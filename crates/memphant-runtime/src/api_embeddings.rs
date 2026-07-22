//! Hosted-API embedding providers (R0-T2), compiled UNCONDITIONALLY — no cargo
//! feature gates them, so the docs-gate harness (T3) can swap any arm in purely
//! via `MEMPHANT_EMBEDDINGS` / `--embed-model` without a rebuild. All are
//! synchronous ([`ureq`], no tokio/reqwest) and implement
//! [`memphant_core::EmbeddingProvider`]: `embed` = document side, `embed_query`
//! = query side.
//!
//! Seven arms across four provider shapes:
//! - [`VoyageEmbedding`] — `voyage-4` / `voyage-4-lite` / `voyage-4-large` /
//!   `voyage-code-3` (`/v1/embeddings`, asymmetric `input_type: document|query`,
//!   1024d default).
//! - [`VoyageContextualizedEmbedding`] — `voyage-context-4`
//!   (`/v1/contextualizedembeddings`, one document = a list of chunks that share
//!   context, 1024d default).
//! - [`GeminiEmbedding`] — `gemini-embedding-001`
//!   (`:batchEmbedContents`, asymmetric `taskType: RETRIEVAL_DOCUMENT|QUERY`,
//!   3072d native — no MRL truncation).
//! - [`OpenAiEmbedding`] — `text-embedding-3-small` (`/v1/embeddings`, 1536d,
//!   SYMMETRIC — no query/document split, deliberately the Syndai-parity
//!   diagnostic control).
//!
//! Dims are DECLARED per model and asserted against every live response (a
//! mismatch fails fast — the embedding profile is keyed on id+dims and a silent
//! width change would corrupt the vector channel). Every declared default was
//! pinned by a live probe on 2026-07-11 (see the crate's R0-T2 report): all five
//! voyage models default to 1024, gemini to 3072, openai to 1536.
//!
//! Keys are read from the environment at CONSTRUCTION (never logged); a missing
//! var yields a clear error naming it. Construction does NOT hit the network —
//! it only reads the key and builds a connection-pooled agent — so
//! [`crate::embedder_from_id`] stays cheap.

use std::thread::sleep;
use std::time::Duration;

use memphant_core::{EmbedError, EmbeddingProvider};
use serde::{Deserialize, Serialize};
use ureq::Agent;
use ureq::http::HeaderMap;

// ---- Endpoints -------------------------------------------------------------

const VOYAGE_EMBED_URL: &str = "https://api.voyageai.com/v1/embeddings";
const VOYAGE_CONTEXT_URL: &str = "https://api.voyageai.com/v1/contextualizedembeddings";
const GEMINI_EMBED_URL: &str = "https://generativelanguage.googleapis.com/v1beta/models/gemini-embedding-001:batchEmbedContents";
const GEMINI2_EMBED_URL: &str =
    "https://generativelanguage.googleapis.com/v1beta/models/gemini-embedding-2:batchEmbedContents";
const OPENAI_EMBED_URL: &str = "https://api.openai.com/v1/embeddings";
const JINA_EMBED_URL: &str = "https://api.jina.ai/v1/embeddings";

// ---- Declared dims (pinned via live probe 2026-07-11) ----------------------

/// Every voyage arm (`voyage-4`, `voyage-4-lite`, `voyage-4-large`,
/// `voyage-code-3`, `voyage-context-4`) defaults to 1024 when
/// `output_dimension` is omitted.
pub const VOYAGE_DIMS: usize = 1024;
/// `gemini-embedding-001` native output width (no MRL truncation requested).
pub const GEMINI_DIMS: usize = 3072;
/// `gemini-embedding-2` native output width (MRL 128-3072; default full).
pub const GEMINI2_DIMS: usize = 3072;
/// `text-embedding-3-small` output width.
pub const OPENAI_DIMS: usize = 1536;
/// `jina-embeddings-v5-text-small` output width (live-probed 2026-07-22).
pub const JINA_DIMS: usize = 1024;

// ---- Stable provider ids (key the embedding profile) -----------------------

pub const GEMINI_ID: &str = "gemini-embedding-001";
pub const GEMINI2_ID: &str = "gemini-embedding-2";
pub const OPENAI_ID: &str = "openai-text-embedding-3-small";
pub const VOYAGE_CONTEXT_ID: &str = "voyage-context-4";
pub const JINA_ID: &str = "jina-v5-small";

const GEMINI_MODEL_PATH: &str = "models/gemini-embedding-001";
const GEMINI2_MODEL_PATH: &str = "models/gemini-embedding-2";
const JINA_MODEL: &str = "jina-embeddings-v5-text-small";

// ---- Env var names ---------------------------------------------------------

const VOYAGE_KEY_VAR: &str = "VOYAGE_API_KEY";
const GEMINI_KEY_VAR: &str = "GEMINI_API_KEY";
const OPENAI_KEY_VAR: &str = "OPENAI_API_KEY";
const JINA_KEY_VAR: &str = "JINA_API_KEY";

// ---- Per-request batch caps ------------------------------------------------

const VOYAGE_BATCH: usize = 64;
const GEMINI_BATCH: usize = 100;
const OPENAI_BATCH: usize = 256;
const JINA_BATCH: usize = 128;

// ---- Retry / timeout policy ------------------------------------------------

const MAX_ATTEMPTS: u32 = 5;
const BACKOFF_BASE_MS: u64 = 500;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const GLOBAL_TIMEOUT: Duration = Duration::from_secs(120);
/// Response-body read cap (a trusted vendor could still stream unboundedly).
const RESPONSE_BODY_LIMIT: u64 = 64 * 1024 * 1024;
/// Error-body read cap — the snippet folded into an [`EmbedError`].
const ERROR_SNIPPET_LIMIT: u64 = 4096;

// ---- Pure helpers (unit-tested without any network) ------------------------

/// Splits `len` items into contiguous `[start, end)` batches of at most
/// `batch_size`. Empty input → no batches; preserves order.
fn batch_ranges(len: usize, batch_size: usize) -> Vec<std::ops::Range<usize>> {
    debug_assert!(batch_size > 0, "batch_size must be positive");
    let mut ranges = Vec::new();
    let mut start = 0;
    while start < len {
        let end = (start + batch_size).min(len);
        ranges.push(start..end);
        start = end;
    }
    ranges
}

/// Exponential backoff for retry `attempt` (0-based): `500ms · 2^attempt`.
/// Sequence: 500ms, 1s, 2s, 4s, 8s, …
fn backoff_delay(attempt: u32) -> Duration {
    Duration::from_millis(BACKOFF_BASE_MS.saturating_mul(1_u64 << attempt.min(20)))
}

/// Reads a key from the environment, erroring with the missing var name when it
/// is unset or blank. Never logs the value.
fn require_key(var: &str) -> Result<String, EmbedError> {
    match std::env::var(var) {
        Ok(value) if !value.trim().is_empty() => Ok(value),
        _ => Err(EmbedError::Unavailable(format!(
            "{var} is not set (required to construct this API embedding provider)"
        ))),
    }
}

/// `Retry-After` in integer seconds, when present and parseable. HTTP-date
/// forms are ignored (we fall back to exponential backoff for those).
fn parse_retry_after(headers: &HeaderMap) -> Option<Duration> {
    headers
        .get("retry-after")?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
        .map(Duration::from_secs)
}

/// Whether an HTTP status is worth retrying (429 or any 5xx).
fn is_retryable_status(code: u16) -> bool {
    code == 429 || (500..600).contains(&code)
}

// ---- Wire types (shared shapes across providers) ---------------------------

/// One `{embedding, index}` entry — voyage-standard AND openai `data[]` share
/// this exact shape (extra fields like `object`/`text` are ignored).
#[derive(Debug, Deserialize)]
struct IndexedEmbedding {
    embedding: Vec<f32>,
    index: usize,
}

/// `{"data": [{embedding, index}, ...]}` — voyage-standard + openai.
#[derive(Debug, Deserialize)]
struct DataEnvelope {
    data: Vec<IndexedEmbedding>,
}

/// `{"embeddings": [{values}, ...]}` — gemini `:batchEmbedContents`.
#[derive(Debug, Deserialize)]
struct GeminiResponse {
    embeddings: Vec<GeminiValues>,
}

#[derive(Debug, Deserialize)]
struct GeminiValues {
    values: Vec<f32>,
}

/// `{"data": [{data: [{embedding, index}], index}, ...]}` — voyage
/// contextualized: one outer entry per input document, one inner entry per
/// chunk of that document.
#[derive(Debug, Deserialize)]
struct ContextResponse {
    data: Vec<ContextDocument>,
}

#[derive(Debug, Deserialize)]
struct ContextDocument {
    data: Vec<IndexedEmbedding>,
    index: usize,
}

/// Orders `data` by its `index`, asserting the indices are exactly `0..len`
/// (defensive against an out-of-order or gapped response — the alignment
/// between a vector and its source unit body must be exact).
fn order_indexed(mut data: Vec<IndexedEmbedding>) -> Result<Vec<Vec<f32>>, EmbedError> {
    data.sort_by_key(|entry| entry.index);
    for (expected, entry) in data.iter().enumerate() {
        if entry.index != expected {
            return Err(EmbedError::Unavailable(format!(
                "embedding response has non-contiguous indices (expected {expected}, got {})",
                entry.index
            )));
        }
    }
    Ok(data.into_iter().map(|entry| entry.embedding).collect())
}

/// Flattens a contextualized response back to one vector per chunk in input
/// order: documents sorted by outer index, chunks within each by inner index.
/// Correct for both the document path (one document, many chunks → many
/// vectors) and the query path (many single-chunk documents → many vectors).
fn flatten_context(documents: Vec<ContextDocument>) -> Result<Vec<Vec<f32>>, EmbedError> {
    let mut documents = documents;
    documents.sort_by_key(|document| document.index);
    for (expected, document) in documents.iter().enumerate() {
        if document.index != expected {
            return Err(EmbedError::Unavailable(format!(
                "contextualized response has non-contiguous document indices (expected {expected}, got {})",
                document.index
            )));
        }
    }
    let mut out = Vec::new();
    for document in documents {
        out.extend(order_indexed(document.data)?);
    }
    Ok(out)
}

/// Fails fast when any returned vector's width is not the declared dims — the
/// embedding profile is keyed on id+dims, so a silent width drift would corrupt
/// the vector channel.
fn assert_dims(vectors: &[Vec<f32>], expected: usize, provider: &str) -> Result<(), EmbedError> {
    for vector in vectors {
        if vector.len() != expected {
            return Err(EmbedError::Unavailable(format!(
                "{provider} returned a {}-dim vector, declared {expected}",
                vector.len()
            )));
        }
    }
    Ok(())
}

// ---- Shared HTTP client (retry + backoff + timeouts) -----------------------

/// A connection-pooled [`ureq`] agent with the shared retry/timeout policy.
/// `http_status_as_error(false)` so 4xx/5xx come back as an inspectable
/// response (status + `Retry-After` + body snippet) rather than an opaque error.
struct ApiHttp {
    agent: Agent,
}

impl ApiHttp {
    fn new() -> Self {
        let config = Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(Some(GLOBAL_TIMEOUT))
            .http_status_as_error(false)
            .build();
        Self {
            agent: config.into(),
        }
    }

    /// POSTs `body` as JSON with the given headers, retrying on 429/5xx and
    /// transport errors (exponential backoff, honoring `Retry-After`), then
    /// deserializes a 2xx body into `T`. `provider` names the arm in errors.
    fn post_json<B, T>(
        &self,
        url: &str,
        headers: &[(&str, &str)],
        body: &B,
        provider: &str,
    ) -> Result<T, EmbedError>
    where
        B: Serialize,
        T: for<'de> Deserialize<'de>,
    {
        let mut attempt: u32 = 0;
        loop {
            let mut request = self.agent.post(url);
            for (name, value) in headers {
                request = request.header(*name, *value);
            }
            match request.send_json(body) {
                Ok(mut response) => {
                    let code = response.status().as_u16();
                    if (200..300).contains(&code) {
                        return response
                            .body_mut()
                            .with_config()
                            .limit(RESPONSE_BODY_LIMIT)
                            .read_json::<T>()
                            .map_err(|error| {
                                EmbedError::Unavailable(format!(
                                    "{provider} response decode failed: {error}"
                                ))
                            });
                    }
                    // Non-2xx: capture Retry-After before the body borrow.
                    let retry_after = parse_retry_after(response.headers());
                    let snippet = read_snippet(&mut response);
                    if is_retryable_status(code) && attempt + 1 < MAX_ATTEMPTS {
                        sleep(retry_after.unwrap_or_else(|| backoff_delay(attempt)));
                        attempt += 1;
                        continue;
                    }
                    return Err(EmbedError::Unavailable(format!(
                        "{provider} HTTP {code}: {snippet}"
                    )));
                }
                Err(error) => {
                    // Transport/IO error (connect, TLS, timeout): retry.
                    if attempt + 1 < MAX_ATTEMPTS {
                        sleep(backoff_delay(attempt));
                        attempt += 1;
                        continue;
                    }
                    return Err(EmbedError::Unavailable(format!(
                        "{provider} transport error after {MAX_ATTEMPTS} attempts: {error}"
                    )));
                }
            }
        }
    }
}

/// Reads a bounded snippet of an error response body for the [`EmbedError`]
/// message; never fails the caller (a body-read error becomes a placeholder).
fn read_snippet(response: &mut ureq::http::Response<ureq::Body>) -> String {
    match response
        .body_mut()
        .with_config()
        .limit(ERROR_SNIPPET_LIMIT)
        .read_to_string()
    {
        Ok(body) => {
            let trimmed = body.trim();
            let snippet: String = trimmed.chars().take(500).collect();
            if snippet.is_empty() {
                "<empty body>".to_string()
            } else {
                snippet
            }
        }
        Err(_) => "<unreadable body>".to_string(),
    }
}

// ---- Request wire types ----------------------------------------------------

#[derive(Serialize)]
struct VoyageRequest<'a> {
    model: &'a str,
    input: &'a [String],
    input_type: &'a str,
}

#[derive(Serialize)]
struct VoyageContextRequest<'a> {
    model: &'a str,
    inputs: &'a [Vec<String>],
    input_type: &'a str,
}

#[derive(Serialize)]
struct OpenAiRequest<'a> {
    model: &'a str,
    input: &'a [String],
}

/// Jina is OpenAI-envelope-compatible with an extra asymmetric `task` field
/// (`retrieval.passage` / `retrieval.query`).
#[derive(Serialize)]
struct JinaRequest<'a> {
    model: &'a str,
    input: &'a [String],
    task: &'a str,
}

#[derive(Serialize)]
struct GeminiBatchRequest {
    requests: Vec<GeminiSingleRequest>,
}

fn gemini_single_request(
    model: &'static str,
    text: &str,
    task_type: &'static str,
) -> GeminiSingleRequest {
    GeminiSingleRequest {
        model,
        content: GeminiContent {
            parts: vec![GeminiPart {
                text: text.to_string(),
            }],
        },
        task_type,
    }
}

#[derive(Serialize)]
struct GeminiSingleRequest {
    model: &'static str,
    content: GeminiContent,
    #[serde(rename = "taskType")]
    task_type: &'static str,
}

#[derive(Serialize)]
struct GeminiContent {
    parts: Vec<GeminiPart>,
}

#[derive(Serialize)]
struct GeminiPart {
    text: String,
}

// ---- Voyage (standard) -----------------------------------------------------

/// The four standard voyage arms. `id()` == the wire model name for these.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VoyageModel {
    Voyage4,
    Voyage4Lite,
    Voyage4Large,
    VoyageCode3,
}

impl VoyageModel {
    /// The wire model name AND the stable provider id (they coincide for voyage
    /// standard arms).
    pub fn id(self) -> &'static str {
        match self {
            Self::Voyage4 => "voyage-4",
            Self::Voyage4Lite => "voyage-4-lite",
            Self::Voyage4Large => "voyage-4-large",
            Self::VoyageCode3 => "voyage-code-3",
        }
    }
}

/// `voyage-4` / `voyage-4-lite` / `voyage-code-3` via `/v1/embeddings`.
/// Asymmetric: documents use `input_type: "document"`, queries `"query"`.
pub struct VoyageEmbedding {
    http: ApiHttp,
    api_key: String,
    model: VoyageModel,
}

impl VoyageEmbedding {
    pub fn new(model: VoyageModel) -> Result<Self, EmbedError> {
        Ok(Self {
            http: ApiHttp::new(),
            api_key: require_key(VOYAGE_KEY_VAR)?,
            model,
        })
    }

    fn embed_with_input_type(
        &self,
        texts: &[String],
        input_type: &str,
    ) -> Result<Vec<Vec<f32>>, EmbedError> {
        let bearer = format!("Bearer {}", self.api_key);
        let mut out = Vec::with_capacity(texts.len());
        for range in batch_ranges(texts.len(), VOYAGE_BATCH) {
            let batch = &texts[range];
            let request = VoyageRequest {
                model: self.model.id(),
                input: batch,
                input_type,
            };
            let response: DataEnvelope = self.http.post_json(
                VOYAGE_EMBED_URL,
                &[
                    ("Authorization", bearer.as_str()),
                    ("Content-Type", "application/json"),
                ],
                &request,
                self.model.id(),
            )?;
            let vectors = order_indexed(response.data)?;
            expect_batch_len(vectors.len(), batch.len(), self.model.id())?;
            assert_dims(&vectors, VOYAGE_DIMS, self.model.id())?;
            out.extend(vectors);
        }
        Ok(out)
    }
}

impl EmbeddingProvider for VoyageEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.embed_with_input_type(texts, "document")
    }

    fn embed_query(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.embed_with_input_type(texts, "query")
    }

    fn dimensions(&self) -> usize {
        VOYAGE_DIMS
    }

    fn id(&self) -> &str {
        self.model.id()
    }
}

// ---- Voyage (contextualized) -----------------------------------------------

/// `voyage-context-4` via `/v1/contextualizedembeddings`.
///
/// GROUPING (R0-T2, verified at the reflect/index call-site
/// `memphant-core::reflect_recorded`): one `embed()` call's bodies always
/// belong to ONE source episode — a reflect job compiles candidates from
/// exactly one episode (or one resource), and the write-through embeds only
/// that job's new units. So the whole document batch is sent as a SINGLE
/// contextualized document (`inputs = [bodies]`), letting the model condition
/// each chunk's embedding on its siblings — the entire point of the
/// contextualized arm. (An episode compiling to more than [`VOYAGE_BATCH`]
/// units is split into successive ≤64-chunk documents, preserving order; in
/// practice unit counts are far below that cap.)
///
/// `embed_query` treats each query as its own single-chunk document with
/// `input_type: "query"`.
pub struct VoyageContextualizedEmbedding {
    http: ApiHttp,
    api_key: String,
}

impl VoyageContextualizedEmbedding {
    pub fn new() -> Result<Self, EmbedError> {
        Ok(Self {
            http: ApiHttp::new(),
            api_key: require_key(VOYAGE_KEY_VAR)?,
        })
    }

    fn post_context(
        &self,
        inputs: &[Vec<String>],
        input_type: &str,
    ) -> Result<Vec<Vec<f32>>, EmbedError> {
        let bearer = format!("Bearer {}", self.api_key);
        let request = VoyageContextRequest {
            model: VOYAGE_CONTEXT_ID,
            inputs,
            input_type,
        };
        let response: ContextResponse = self.http.post_json(
            VOYAGE_CONTEXT_URL,
            &[
                ("Authorization", bearer.as_str()),
                ("Content-Type", "application/json"),
            ],
            &request,
            VOYAGE_CONTEXT_ID,
        )?;
        let vectors = flatten_context(response.data)?;
        assert_dims(&vectors, VOYAGE_DIMS, VOYAGE_CONTEXT_ID)?;
        Ok(vectors)
    }
}

impl EmbeddingProvider for VoyageContextualizedEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        // Each ≤64-chunk slice is one contextualized document, in order.
        let mut out = Vec::with_capacity(texts.len());
        for range in batch_ranges(texts.len(), VOYAGE_BATCH) {
            let document = texts[range].to_vec();
            let chunk_count = document.len();
            let inputs = [document];
            let vectors = self.post_context(&inputs, "document")?;
            expect_batch_len(vectors.len(), chunk_count, VOYAGE_CONTEXT_ID)?;
            out.extend(vectors);
        }
        Ok(out)
    }

    fn embed_query(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        // Each query is an independent single-chunk document.
        let mut out = Vec::with_capacity(texts.len());
        for range in batch_ranges(texts.len(), VOYAGE_BATCH) {
            let batch = &texts[range];
            let inputs: Vec<Vec<String>> = batch.iter().map(|text| vec![text.clone()]).collect();
            let vectors = self.post_context(&inputs, "query")?;
            expect_batch_len(vectors.len(), batch.len(), VOYAGE_CONTEXT_ID)?;
            out.extend(vectors);
        }
        Ok(out)
    }

    fn dimensions(&self) -> usize {
        VOYAGE_DIMS
    }

    fn id(&self) -> &str {
        VOYAGE_CONTEXT_ID
    }
}

// ---- Gemini ----------------------------------------------------------------

/// `gemini-embedding-001` via `:batchEmbedContents`. Asymmetric:
/// `taskType: RETRIEVAL_DOCUMENT` for documents, `RETRIEVAL_QUERY` for queries.
pub struct GeminiEmbedding {
    http: ApiHttp,
    api_key: String,
    url: &'static str,
    model_path: &'static str,
    id: &'static str,
    dims: usize,
}

impl GeminiEmbedding {
    pub fn new() -> Result<Self, EmbedError> {
        Self::with_model(GEMINI_EMBED_URL, GEMINI_MODEL_PATH, GEMINI_ID, GEMINI_DIMS)
    }

    /// `gemini-embedding-2` (GA 2026-04; same batch wire shape, newer space).
    pub fn new_v2() -> Result<Self, EmbedError> {
        Self::with_model(
            GEMINI2_EMBED_URL,
            GEMINI2_MODEL_PATH,
            GEMINI2_ID,
            GEMINI2_DIMS,
        )
    }

    fn with_model(
        url: &'static str,
        model_path: &'static str,
        id: &'static str,
        dims: usize,
    ) -> Result<Self, EmbedError> {
        Ok(Self {
            http: ApiHttp::new(),
            api_key: require_key(GEMINI_KEY_VAR)?,
            url,
            model_path,
            id,
            dims,
        })
    }

    fn embed_with_task(
        &self,
        texts: &[String],
        task_type: &'static str,
    ) -> Result<Vec<Vec<f32>>, EmbedError> {
        let mut out = Vec::with_capacity(texts.len());
        for range in batch_ranges(texts.len(), GEMINI_BATCH) {
            let batch = &texts[range];
            let requests = batch
                .iter()
                .map(|text| gemini_single_request(self.model_path, text, task_type))
                .collect();
            let request = GeminiBatchRequest { requests };
            let response: GeminiResponse = self.http.post_json(
                self.url,
                &[
                    ("x-goog-api-key", self.api_key.as_str()),
                    ("Content-Type", "application/json"),
                ],
                &request,
                self.id,
            )?;
            // Gemini's `embeddings[]` is order-preserving (no index field).
            let vectors: Vec<Vec<f32>> = response
                .embeddings
                .into_iter()
                .map(|item| item.values)
                .collect();
            expect_batch_len(vectors.len(), batch.len(), self.id)?;
            assert_dims(&vectors, self.dims, self.id)?;
            out.extend(vectors);
        }
        Ok(out)
    }
}

impl EmbeddingProvider for GeminiEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.embed_with_task(texts, "RETRIEVAL_DOCUMENT")
    }

    fn embed_query(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.embed_with_task(texts, "RETRIEVAL_QUERY")
    }

    fn dimensions(&self) -> usize {
        self.dims
    }

    fn id(&self) -> &str {
        self.id
    }
}

// ---- Jina ------------------------------------------------------------------

/// `jina-embeddings-v5-text-small` via the OpenAI-compatible `/v1/embeddings`
/// with Jina's asymmetric `task` field.
pub struct JinaEmbedding {
    http: ApiHttp,
    api_key: String,
}

impl JinaEmbedding {
    pub fn new() -> Result<Self, EmbedError> {
        Ok(Self {
            http: ApiHttp::new(),
            api_key: require_key(JINA_KEY_VAR)?,
        })
    }

    fn embed_with_task(&self, texts: &[String], task: &str) -> Result<Vec<Vec<f32>>, EmbedError> {
        let bearer = format!("Bearer {}", self.api_key);
        let mut out = Vec::with_capacity(texts.len());
        for range in batch_ranges(texts.len(), JINA_BATCH) {
            let batch = &texts[range];
            let request = JinaRequest {
                model: JINA_MODEL,
                input: batch,
                task,
            };
            let response: DataEnvelope = self.http.post_json(
                JINA_EMBED_URL,
                &[
                    ("Authorization", bearer.as_str()),
                    ("Content-Type", "application/json"),
                ],
                &request,
                JINA_ID,
            )?;
            let vectors = order_indexed(response.data)?;
            expect_batch_len(vectors.len(), batch.len(), JINA_ID)?;
            assert_dims(&vectors, JINA_DIMS, JINA_ID)?;
            out.extend(vectors);
        }
        Ok(out)
    }
}

impl EmbeddingProvider for JinaEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.embed_with_task(texts, "retrieval.passage")
    }

    fn embed_query(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        self.embed_with_task(texts, "retrieval.query")
    }

    fn dimensions(&self) -> usize {
        JINA_DIMS
    }

    fn id(&self) -> &str {
        JINA_ID
    }
}

// ---- OpenAI ----------------------------------------------------------------

/// `text-embedding-3-small` via `/v1/embeddings`. SYMMETRIC — no query/document
/// distinction: `embed_query` intentionally falls through to `embed` (the
/// trait default), making this the Syndai-parity diagnostic control (Syndai
/// embeds symmetric).
pub struct OpenAiEmbedding {
    http: ApiHttp,
    api_key: String,
}

impl OpenAiEmbedding {
    pub fn new() -> Result<Self, EmbedError> {
        Ok(Self {
            http: ApiHttp::new(),
            api_key: require_key(OPENAI_KEY_VAR)?,
        })
    }
}

impl EmbeddingProvider for OpenAiEmbedding {
    fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, EmbedError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }
        let bearer = format!("Bearer {}", self.api_key);
        let mut out = Vec::with_capacity(texts.len());
        for range in batch_ranges(texts.len(), OPENAI_BATCH) {
            let batch = &texts[range];
            let request = OpenAiRequest {
                model: "text-embedding-3-small",
                input: batch,
            };
            let response: DataEnvelope = self.http.post_json(
                OPENAI_EMBED_URL,
                &[
                    ("Authorization", bearer.as_str()),
                    ("Content-Type", "application/json"),
                ],
                &request,
                OPENAI_ID,
            )?;
            let vectors = order_indexed(response.data)?;
            expect_batch_len(vectors.len(), batch.len(), OPENAI_ID)?;
            assert_dims(&vectors, OPENAI_DIMS, OPENAI_ID)?;
            out.extend(vectors);
        }
        Ok(out)
    }

    // `embed_query` deliberately NOT overridden: symmetric model.

    fn dimensions(&self) -> usize {
        OPENAI_DIMS
    }

    fn id(&self) -> &str {
        OPENAI_ID
    }
}

/// Guards the one-vector-per-input invariant: a response returning a different
/// count than the batch it answered would silently misalign vectors with unit
/// bodies.
fn expect_batch_len(got: usize, expected: usize, provider: &str) -> Result<(), EmbedError> {
    if got != expected {
        return Err(EmbedError::Unavailable(format!(
            "{provider} returned {got} vectors for {expected} inputs"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- New-arm request shapes (P1 bench: jina-v5-small / gemini-embedding-2)

    #[test]
    fn jina_request_serializes_model_input_and_task() {
        let input = vec!["a".to_string(), "b".to_string()];
        let request = JinaRequest {
            model: JINA_MODEL,
            input: &input,
            task: "retrieval.passage",
        };
        let value = serde_json::to_value(&request).expect("serializes");
        assert_eq!(value["model"], "jina-embeddings-v5-text-small");
        assert_eq!(value["task"], "retrieval.passage");
        assert_eq!(value["input"].as_array().expect("array").len(), 2);
    }

    #[test]
    fn gemini_single_request_parameterizes_the_model_path() {
        let request = gemini_single_request(GEMINI2_MODEL_PATH, "hello", "RETRIEVAL_QUERY");
        let value = serde_json::to_value(&request).expect("serializes");
        assert_eq!(value["model"], "models/gemini-embedding-2");
        assert_eq!(value["taskType"], "RETRIEVAL_QUERY");
        assert_eq!(value["content"]["parts"][0]["text"], "hello");
    }

    // ---- Batching splitter --------------------------------------------------

    #[test]
    fn batch_ranges_empty_input_has_no_batches() {
        assert!(batch_ranges(0, 64).is_empty());
    }

    #[test]
    fn batch_ranges_splits_on_cap_and_preserves_order() {
        assert_eq!(batch_ranges(64, 64), vec![0..64]);
        assert_eq!(batch_ranges(65, 64), vec![0..64, 64..65]);
        assert_eq!(batch_ranges(130, 64), vec![0..64, 64..128, 128..130]);
        // Gemini (100) and openai (256) caps.
        assert_eq!(batch_ranges(250, 100), vec![0..100, 100..200, 200..250]);
        assert_eq!(batch_ranges(256, 256), vec![0..256]);
        assert_eq!(batch_ranges(257, 256), vec![0..256, 256..257]);
    }

    // ---- Backoff sequence ---------------------------------------------------

    #[test]
    fn backoff_delay_is_exponential_from_500ms() {
        assert_eq!(backoff_delay(0), Duration::from_millis(500));
        assert_eq!(backoff_delay(1), Duration::from_millis(1000));
        assert_eq!(backoff_delay(2), Duration::from_millis(2000));
        assert_eq!(backoff_delay(3), Duration::from_millis(4000));
        assert_eq!(backoff_delay(4), Duration::from_millis(8000));
    }

    #[test]
    fn retryable_status_covers_429_and_5xx_only() {
        assert!(is_retryable_status(429));
        assert!(is_retryable_status(500));
        assert!(is_retryable_status(503));
        assert!(!is_retryable_status(400));
        assert!(!is_retryable_status(401));
        assert!(!is_retryable_status(404));
        assert!(!is_retryable_status(200));
    }

    // ---- Retry-After parsing ------------------------------------------------

    #[test]
    fn retry_after_parses_integer_seconds_and_ignores_dates() {
        let mut headers = HeaderMap::new();
        headers.insert("retry-after", "7".parse().unwrap());
        assert_eq!(parse_retry_after(&headers), Some(Duration::from_secs(7)));

        let mut date_headers = HeaderMap::new();
        date_headers.insert(
            "retry-after",
            "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap(),
        );
        assert_eq!(parse_retry_after(&date_headers), None);

        assert_eq!(parse_retry_after(&HeaderMap::new()), None);
    }

    // ---- Missing-key construction error ------------------------------------

    #[test]
    fn require_key_names_the_missing_var() {
        let error =
            require_key("MEMPHANT_API_EMBED_DEFINITELY_UNSET_VAR").expect_err("must be unset");
        let message = error.to_string();
        assert!(
            message.contains("MEMPHANT_API_EMBED_DEFINITELY_UNSET_VAR"),
            "error must name the missing var: {message}"
        );
    }

    // ---- Response deserialization (inline fixtures) ------------------------

    #[test]
    fn voyage_and_openai_share_the_data_envelope_shape() {
        // voyage-standard / openai `{"data":[{embedding,index,...}]}`. Scrambled
        // order here proves `order_indexed` sorts by `index`, not array order.
        let json = r#"{
            "object": "list",
            "data": [
                {"object": "embedding", "embedding": [0.4, 0.5], "index": 1, "text": "b"},
                {"object": "embedding", "embedding": [0.1, 0.2], "index": 0, "text": "a"}
            ],
            "model": "voyage-4",
            "usage": {"total_tokens": 3}
        }"#;
        let envelope: DataEnvelope = serde_json::from_str(json).expect("parse voyage/openai");
        let vectors = order_indexed(envelope.data).expect("order");
        assert_eq!(vectors, vec![vec![0.1, 0.2], vec![0.4, 0.5]]);
    }

    #[test]
    fn order_indexed_rejects_gapped_indices() {
        let json = r#"{"data":[
            {"embedding":[0.1],"index":0},
            {"embedding":[0.2],"index":2}
        ]}"#;
        let envelope: DataEnvelope = serde_json::from_str(json).unwrap();
        let error = order_indexed(envelope.data).expect_err("gap must error");
        assert!(error.to_string().contains("non-contiguous"));
    }

    #[test]
    fn gemini_response_deserializes_values_in_order() {
        let json = r#"{
            "embeddings": [
                {"values": [0.1, 0.2, 0.3]},
                {"values": [0.4, 0.5, 0.6]}
            ]
        }"#;
        let response: GeminiResponse = serde_json::from_str(json).expect("parse gemini");
        let vectors: Vec<Vec<f32>> = response
            .embeddings
            .into_iter()
            .map(|item| item.values)
            .collect();
        assert_eq!(vectors, vec![vec![0.1, 0.2, 0.3], vec![0.4, 0.5, 0.6]]);
    }

    #[test]
    fn contextualized_document_path_flattens_one_doc_many_chunks() {
        // `embed(bodies)` sends inputs=[bodies] → one document, N chunks. The
        // inner `data` is scrambled to prove per-chunk index ordering.
        let json = r#"{
            "object": "list",
            "data": [
                {
                    "object": "list",
                    "index": 0,
                    "data": [
                        {"object": "embedding", "embedding": [0.3, 0.3], "index": 1},
                        {"object": "embedding", "embedding": [0.1, 0.1], "index": 0}
                    ]
                }
            ],
            "model": "voyage-context-4",
            "usage": {"total_tokens": 9}
        }"#;
        let response: ContextResponse = serde_json::from_str(json).expect("parse context doc");
        let vectors = flatten_context(response.data).expect("flatten");
        assert_eq!(vectors, vec![vec![0.1, 0.1], vec![0.3, 0.3]]);
    }

    #[test]
    fn contextualized_query_path_flattens_many_single_chunk_docs() {
        // `embed_query(queries)` sends inputs=[[q0],[q1]] → two single-chunk
        // documents. Outer docs scrambled to prove per-document index ordering.
        let json = r#"{
            "data": [
                {"index": 1, "data": [{"embedding": [0.9, 0.9], "index": 0}]},
                {"index": 0, "data": [{"embedding": [0.2, 0.2], "index": 0}]}
            ]
        }"#;
        let response: ContextResponse = serde_json::from_str(json).expect("parse context query");
        let vectors = flatten_context(response.data).expect("flatten");
        assert_eq!(vectors, vec![vec![0.2, 0.2], vec![0.9, 0.9]]);
    }

    #[test]
    fn assert_dims_rejects_wrong_width() {
        let ok = assert_dims(&[vec![0.0; VOYAGE_DIMS]], VOYAGE_DIMS, "voyage-4");
        assert!(ok.is_ok());
        let bad = assert_dims(&[vec![0.0; 3]], VOYAGE_DIMS, "voyage-4");
        assert!(bad.is_err());
    }

    // ---- Live smokes (env-gated, `#[ignore]`) ------------------------------
    //
    // Each hits the real API once. Gated by `MEMPHANT_API_EMBED_SMOKE=1` on top
    // of `#[ignore]` so `cargo test` never touches the network or spends money.
    // Every smoke asserts dims == the DECLARED width (the profile-correctness
    // invariant) and non-zero vectors; the asymmetric arms (voyage/gemini) also
    // assert `embed_query(t) != embed(t)`.

    fn smoke_enabled() -> bool {
        std::env::var("MEMPHANT_API_EMBED_SMOKE").as_deref() == Ok("1")
    }

    fn assert_nonzero(vector: &[f32]) {
        assert!(
            vector.iter().any(|&value| value != 0.0),
            "embedding must not be all-zero"
        );
    }

    #[test]
    #[ignore = "hits the live Voyage API; run with MEMPHANT_API_EMBED_SMOKE=1 + VOYAGE_API_KEY"]
    fn voyage_standard_smoke() {
        if !smoke_enabled() {
            eprintln!("voyage standard smoke skipped (set MEMPHANT_API_EMBED_SMOKE=1)");
            return;
        }
        let provider = VoyageEmbedding::new(VoyageModel::Voyage4).expect("construct voyage-4");
        let texts = vec!["Release region is Taipei.".to_string()];
        let docs = provider.embed(&texts).expect("embed documents");
        let queries = provider.embed_query(&texts).expect("embed queries");
        assert_eq!(docs.len(), 1);
        assert_eq!(docs[0].len(), VOYAGE_DIMS, "declared dims");
        assert_eq!(queries[0].len(), VOYAGE_DIMS);
        assert_nonzero(&docs[0]);
        assert_nonzero(&queries[0]);
        assert_ne!(
            docs[0], queries[0],
            "voyage is asymmetric: document != query embedding"
        );
        eprintln!(
            "voyage-4 smoke OK: dims={} doc_head={:?}",
            docs[0].len(),
            &docs[0][..3]
        );
    }

    #[test]
    #[ignore = "hits the live Voyage API; run with MEMPHANT_API_EMBED_SMOKE=1 + VOYAGE_API_KEY"]
    fn voyage_contextualized_smoke() {
        if !smoke_enabled() {
            eprintln!("voyage contextualized smoke skipped (set MEMPHANT_API_EMBED_SMOKE=1)");
            return;
        }
        let provider = VoyageContextualizedEmbedding::new().expect("construct voyage-context-4");
        // Two chunks of ONE document (the reflect call-site guarantee).
        let texts = vec![
            "The user prefers window seats.".to_string(),
            "They fly out of SFO on Tuesdays.".to_string(),
        ];
        let docs = provider.embed(&texts).expect("embed document chunks");
        let queries = provider.embed_query(&texts).expect("embed queries");
        assert_eq!(docs.len(), 2, "one vector per chunk");
        assert_eq!(queries.len(), 2);
        for vector in docs.iter().chain(queries.iter()) {
            assert_eq!(vector.len(), VOYAGE_DIMS, "declared dims");
            assert_nonzero(vector);
        }
        assert_ne!(
            docs[0], queries[0],
            "voyage-context is asymmetric: document != query embedding"
        );
        eprintln!(
            "voyage-context-4 smoke OK: dims={} chunks={}",
            docs[0].len(),
            docs.len()
        );
    }

    #[test]
    #[ignore = "hits the live Gemini API; run with MEMPHANT_API_EMBED_SMOKE=1 + GEMINI_API_KEY"]
    fn gemini_smoke() {
        if !smoke_enabled() {
            eprintln!("gemini smoke skipped (set MEMPHANT_API_EMBED_SMOKE=1)");
            return;
        }
        let provider = GeminiEmbedding::new().expect("construct gemini-embedding-001");
        let texts = vec!["Release region is Taipei.".to_string()];
        let docs = provider.embed(&texts).expect("embed documents");
        let queries = provider.embed_query(&texts).expect("embed queries");
        assert_eq!(docs[0].len(), GEMINI_DIMS, "declared dims");
        assert_eq!(queries[0].len(), GEMINI_DIMS);
        assert_nonzero(&docs[0]);
        assert_nonzero(&queries[0]);
        assert_ne!(
            docs[0], queries[0],
            "gemini is asymmetric: RETRIEVAL_DOCUMENT != RETRIEVAL_QUERY"
        );
        eprintln!(
            "gemini-embedding-001 smoke OK: dims={} doc_head={:?}",
            docs[0].len(),
            &docs[0][..3]
        );
    }

    #[test]
    #[ignore = "hits the live OpenAI API; run with MEMPHANT_API_EMBED_SMOKE=1 + OPENAI_API_KEY"]
    fn openai_smoke() {
        if !smoke_enabled() {
            eprintln!("openai smoke skipped (set MEMPHANT_API_EMBED_SMOKE=1)");
            return;
        }
        let provider = OpenAiEmbedding::new().expect("construct openai-text-embedding-3-small");
        let texts = vec!["Release region is Taipei.".to_string()];
        let docs = provider.embed(&texts).expect("embed documents");
        // Symmetric: `embed_query` falls through to `embed` (trait default).
        let queries = provider.embed_query(&texts).expect("embed queries");
        assert_eq!(docs[0].len(), OPENAI_DIMS, "declared dims");
        assert_eq!(queries[0].len(), OPENAI_DIMS);
        assert_nonzero(&docs[0]);
        assert_eq!(
            docs[0], queries[0],
            "openai is symmetric: query and document embeddings coincide"
        );
        eprintln!(
            "openai-text-embedding-3-small smoke OK: dims={} doc_head={:?}",
            docs[0].len(),
            &docs[0][..3]
        );
    }
}
