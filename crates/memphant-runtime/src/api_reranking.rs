//! Voyage `rerank-2.5` behind MemPhant's existing cross-reranker seam.

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
// bounded budget. This is still a hard global timeout with no retry.
const GLOBAL_TIMEOUT: Duration = Duration::from_millis(1_500);
const RESPONSE_BODY_LIMIT: u64 = 1024 * 1024;
const ERROR_BODY_LIMIT: u64 = 4096;

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
            .timeout_global(Some(GLOBAL_TIMEOUT))
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

fn scores_in_input_order(items: Vec<ResultItem>, count: usize) -> Result<Vec<f32>, String> {
    if items.len() != count {
        return Err(format!(
            "voyage reranker returned {} results for {count} documents",
            items.len()
        ));
    }
    let mut scores = vec![None; count];
    for item in items {
        if item.index >= count || scores[item.index].is_some() {
            return Err(format!(
                "voyage reranker returned invalid or duplicate index {}",
                item.index
            ));
        }
        if !item.relevance_score.is_finite() {
            return Err(format!(
                "voyage reranker returned non-finite score at index {}",
                item.index
            ));
        }
        scores[item.index] = Some(item.relevance_score);
    }
    scores
        .into_iter()
        .enumerate()
        .map(|(index, score)| score.ok_or_else(|| format!("voyage reranker omitted index {index}")))
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
        scores_in_input_order(decoded.data, docs.len())
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
    fn response_scores_are_strictly_scattered_by_index() {
        let scores = scores_in_input_order(
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
        assert!(scores_in_input_order(vec![], 1).is_err());
        assert!(
            scores_in_input_order(
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
