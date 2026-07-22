//! Hosted rerankers behind MemPhant's existing cross-reranker seam:
//! Voyage `rerank-2.5` and Cohere `rerank-v3.5` (both API-only, not
//! self-hostable — reference/cost arms alongside the local fastembed path).

use std::time::Duration;

use memphant_core::{CrossReranker, CrossRerankerConfig};
use serde::{Deserialize, Serialize};
use ureq::Agent;

const URL: &str = "https://api.voyageai.com/v1/rerank";
const MODEL: &str = "rerank-2.5";
const MAX_LENGTH: usize = 32_000;
const CONNECT_TIMEOUT: Duration = Duration::from_millis(500);
// The release local baseline leaves roughly 330-390ms of non-reranker p95
// inside the 1.5s recall ceiling, so the hosted call gets the remaining
// bounded budget by default. `MEMPHANT_RERANK_TIMEOUT_MS` overrides it (0 =
// unbounded) for offline accuracy benchmarking where the hot-path budget does
// not apply. This is still a hard global timeout with no retry.
const DEFAULT_GLOBAL_TIMEOUT_MS: u64 = 1_500;
const RESPONSE_BODY_LIMIT: u64 = 1024 * 1024;
const ERROR_BODY_LIMIT: u64 = 4096;

/// The hosted-reranker global timeout, from `MEMPHANT_RERANK_TIMEOUT_MS`
/// (default 1500 ms; `0` disables the timeout for offline benchmarking).
fn global_timeout() -> Option<Duration> {
    match std::env::var("MEMPHANT_RERANK_TIMEOUT_MS")
        .ok()
        .and_then(|value| value.trim().parse::<u64>().ok())
    {
        Some(0) => None,
        Some(ms) => Some(Duration::from_millis(ms)),
        None => Some(Duration::from_millis(DEFAULT_GLOBAL_TIMEOUT_MS)),
    }
}

pub struct VoyageReranker {
    agent: Agent,
    api_key: String,
    candidate_limit: usize,
}

impl VoyageReranker {
    pub fn new(candidate_limit: usize) -> Result<Self, String> {
        let api_key = std::env::var("VOYAGE_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| {
                "VOYAGE_API_KEY is not set (required for voyage rerank-2.5)".to_string()
            })?;
        let config = Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(global_timeout())
            .http_status_as_error(false)
            .build();
        Ok(Self {
            agent: config.into(),
            api_key,
            candidate_limit,
        })
    }
}

#[derive(Serialize)]
struct Request<'a> {
    query: &'a str,
    documents: &'a [&'a str],
    model: &'static str,
    return_documents: bool,
    truncation: bool,
}

#[derive(Deserialize)]
struct Response {
    data: Vec<ResultItem>,
}

#[derive(Deserialize)]
struct ResultItem {
    index: usize,
    relevance_score: f32,
}

/// Reorder a hosted reranker's scored results into input order. Shared by the
/// Voyage and Cohere arms, so error messages name the calling `provider` — a
/// hardcoded label would mis-attribute a Cohere fault to Voyage (a real debugging
/// wrong-turn), since both arms funnel through here.
fn scores_in_input_order(
    provider: &str,
    items: Vec<ResultItem>,
    count: usize,
) -> Result<Vec<f32>, String> {
    if items.len() != count {
        return Err(format!(
            "{provider} reranker returned {} results for {count} documents",
            items.len()
        ));
    }
    let mut scores = vec![None; count];
    for item in items {
        if item.index >= count || scores[item.index].is_some() {
            return Err(format!(
                "{provider} reranker returned invalid or duplicate index {}",
                item.index
            ));
        }
        if !item.relevance_score.is_finite() {
            return Err(format!(
                "{provider} reranker returned non-finite score at index {}",
                item.index
            ));
        }
        scores[item.index] = Some(item.relevance_score);
    }
    scores
        .into_iter()
        .enumerate()
        .map(|(index, score)| {
            score.ok_or_else(|| format!("{provider} reranker omitted index {index}"))
        })
        .collect()
}

impl CrossReranker for VoyageReranker {
    fn config(&self) -> CrossRerankerConfig {
        CrossRerankerConfig {
            provider: "voyage".to_string(),
            model: MODEL.to_string(),
            candidate_limit: self.candidate_limit,
            max_length: MAX_LENGTH,
            batch_size: None,
        }
    }

    fn rerank(&self, query: &str, docs: &[&str]) -> Result<Vec<f32>, String> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }
        let body = Request {
            query,
            documents: docs,
            model: MODEL,
            return_documents: false,
            truncation: false,
        };
        let mut response = self
            .agent
            .post(URL)
            .header("authorization", &format!("Bearer {}", self.api_key))
            .send_json(&body)
            .map_err(|error| format!("voyage reranker transport error: {error}"))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let snippet = response
                .body_mut()
                .with_config()
                .limit(ERROR_BODY_LIMIT)
                .read_to_string()
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(format!(
                "voyage reranker HTTP {status}: {}",
                snippet.trim().chars().take(500).collect::<String>()
            ));
        }
        let decoded = response
            .body_mut()
            .with_config()
            .limit(RESPONSE_BODY_LIMIT)
            .read_json::<Response>()
            .map_err(|error| format!("voyage reranker response decode failed: {error}"))?;
        scores_in_input_order("voyage", decoded.data, docs.len())
    }
}

// --- Cohere rerank (v2 endpoint) ------------------------------------------

const COHERE_URL: &str = "https://api.cohere.com/v2/rerank";
// Fast/cheap tier by default ($0.001/search, ~600ms). `MEMPHANT_COHERE_MODEL`
// overrides — e.g. `rerank-v4.0-pro` for the accuracy-max arm ($0.0025/search).
const COHERE_DEFAULT_MODEL: &str = "rerank-v3.5";
// Cohere's per-doc context; query+doc are truncated to this. Sent explicitly so
// the arm is comparable to the local `max_length` knob.
const COHERE_MAX_LENGTH: usize = 4_096;

pub struct CohereReranker {
    agent: Agent,
    api_key: String,
    model: String,
    candidate_limit: usize,
}

impl CohereReranker {
    pub fn new(candidate_limit: usize) -> Result<Self, String> {
        let api_key = std::env::var("COHERE_API_KEY")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .ok_or_else(|| "COHERE_API_KEY is not set (required for cohere rerank)".to_string())?;
        let model = std::env::var("MEMPHANT_COHERE_MODEL")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| COHERE_DEFAULT_MODEL.to_string());
        let config = Agent::config_builder()
            .timeout_connect(Some(CONNECT_TIMEOUT))
            .timeout_global(global_timeout())
            .http_status_as_error(false)
            .build();
        Ok(Self {
            agent: config.into(),
            api_key,
            model,
            candidate_limit,
        })
    }
}

#[derive(Serialize)]
struct CohereRequest<'a> {
    query: &'a str,
    documents: &'a [&'a str],
    model: &'a str,
    top_n: usize,
    max_tokens_per_doc: usize,
}

#[derive(Deserialize)]
struct CohereResponse {
    results: Vec<CohereResultItem>,
}

#[derive(Deserialize)]
struct CohereResultItem {
    index: usize,
    relevance_score: f32,
}

impl CrossReranker for CohereReranker {
    fn config(&self) -> CrossRerankerConfig {
        CrossRerankerConfig {
            provider: "cohere".to_string(),
            model: self.model.clone(),
            candidate_limit: self.candidate_limit,
            max_length: COHERE_MAX_LENGTH,
            batch_size: None,
        }
    }

    fn rerank(&self, query: &str, docs: &[&str]) -> Result<Vec<f32>, String> {
        if docs.is_empty() {
            return Ok(Vec::new());
        }
        // top_n = all docs so we get one score per input (we re-scatter by index).
        let body = CohereRequest {
            query,
            documents: docs,
            model: &self.model,
            top_n: docs.len(),
            max_tokens_per_doc: COHERE_MAX_LENGTH,
        };
        let mut response = self
            .agent
            .post(COHERE_URL)
            .header("authorization", &format!("Bearer {}", self.api_key))
            .send_json(&body)
            .map_err(|error| format!("cohere reranker transport error: {error}"))?;
        let status = response.status().as_u16();
        if !(200..300).contains(&status) {
            let snippet = response
                .body_mut()
                .with_config()
                .limit(ERROR_BODY_LIMIT)
                .read_to_string()
                .unwrap_or_else(|_| "<unreadable body>".to_string());
            return Err(format!(
                "cohere reranker HTTP {status}: {}",
                snippet.trim().chars().take(500).collect::<String>()
            ));
        }
        let decoded = response
            .body_mut()
            .with_config()
            .limit(RESPONSE_BODY_LIMIT)
            .read_json::<CohereResponse>()
            .map_err(|error| format!("cohere reranker response decode failed: {error}"))?;
        let items = decoded
            .results
            .into_iter()
            .map(|item| ResultItem {
                index: item.index,
                relevance_score: item.relevance_score,
            })
            .collect();
        scores_in_input_order("cohere", items, docs.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_disables_vendor_truncation_and_document_echo() {
        let docs = ["first", "second"];
        let value = serde_json::to_value(Request {
            query: "question",
            documents: &docs,
            model: MODEL,
            return_documents: false,
            truncation: false,
        })
        .unwrap();
        assert_eq!(value["model"], MODEL);
        assert_eq!(value["truncation"], false);
        assert_eq!(value["return_documents"], false);
        assert_eq!(value["documents"], serde_json::json!(["first", "second"]));
    }

    #[test]
    fn cohere_request_requests_a_score_for_every_document() {
        let docs = ["first", "second", "third"];
        let value = serde_json::to_value(CohereRequest {
            query: "question",
            documents: &docs,
            model: COHERE_DEFAULT_MODEL,
            top_n: docs.len(),
            max_tokens_per_doc: COHERE_MAX_LENGTH,
        })
        .unwrap();
        assert_eq!(value["model"], COHERE_DEFAULT_MODEL);
        // top_n = doc count so we get one score per input to re-scatter by index.
        assert_eq!(value["top_n"], 3);
        assert_eq!(value["max_tokens_per_doc"], COHERE_MAX_LENGTH);
        assert_eq!(
            value["documents"],
            serde_json::json!(["first", "second", "third"])
        );
    }

    #[test]
    fn response_scores_are_strictly_scattered_by_index() {
        let scores = scores_in_input_order(
            "voyage",
            vec![
                ResultItem {
                    index: 1,
                    relevance_score: 0.2,
                },
                ResultItem {
                    index: 0,
                    relevance_score: 0.9,
                },
            ],
            2,
        )
        .unwrap();
        assert_eq!(scores, vec![0.9, 0.2]);
        assert!(scores_in_input_order("voyage", vec![], 1).is_err());
        assert!(
            scores_in_input_order(
                "voyage",
                vec![
                    ResultItem {
                        index: 0,
                        relevance_score: 0.1,
                    },
                    ResultItem {
                        index: 0,
                        relevance_score: 0.2,
                    },
                ],
                2,
            )
            .is_err()
        );
        assert!(
            scores_in_input_order(
                "voyage",
                vec![ResultItem {
                    index: 0,
                    relevance_score: f32::NAN,
                }],
                1,
            )
            .is_err()
        );
    }

    #[test]
    fn error_message_names_the_calling_provider_not_a_hardcoded_one() {
        // A shared helper must attribute a malformed response to the RIGHT
        // provider — a Cohere fault labeled "voyage" sends debugging the wrong way.
        let err = scores_in_input_order("cohere", vec![], 1).unwrap_err();
        assert!(err.contains("cohere"), "error must name the caller: {err}");
        assert!(
            !err.contains("voyage"),
            "must not mis-label as voyage: {err}"
        );
    }

    #[test]
    #[ignore = "requires VOYAGE_API_KEY and makes one paid live request"]
    fn live_voyage_rerank_latency_smoke() {
        let reranker = VoyageReranker::new(8).expect("construct Voyage reranker");
        let phrase = "deployment operations checklist monitoring rollback observability ";
        // Mirrors the measured release-arm candidate length distribution:
        // median ~700 chars, p95 ~5.3k, max ~24k. The answer sits at the end
        // of a max-sized document to exercise truncation=false honestly.
        let mut owned = (0..6)
            .map(|index| format!("{} unrelated document {index}", phrase.repeat(11)))
            .chain((6..7).map(|index| format!("{} unrelated document {index}", phrase.repeat(82))))
            .collect::<Vec<_>>();
        owned.push(format!(
            "{} The canonical tenant isolation rule is transaction-local context with forced row-level security.",
            phrase.repeat(370)
        ));
        let docs = owned.iter().map(String::as_str).collect::<Vec<_>>();
        let mut latencies = Vec::new();
        for sample in 0..10 {
            let started = std::time::Instant::now();
            let scores = reranker
                .rerank(
                    &format!("How is tenant isolation bound in transaction sample {sample}?"),
                    &docs,
                )
                .expect("live Voyage rerank must succeed inside the global timeout");
            assert_eq!(scores.len(), docs.len());
            latencies.push(started.elapsed());
        }
        latencies.sort_unstable();
        let p95 = latencies[9];
        assert!(
            p95 <= Duration::from_millis(1_100),
            "live Voyage rerank p95 exceeds the residual recall budget: {latencies:?}"
        );
        eprintln!("voyage rerank live latencies: {latencies:?}");
    }
}
