//! Fixed-pool tools for the P1 retrieval benchmark: `embed-pool` embeds every
//! unique pool chunk (and optionally the queries) through the production
//! embedder seam (`embedder_from_id`); `rerank-pool` scores retrieved
//! candidates through the production reranker seam (`build_cross_reranker`,
//! `MEMPHANT_RERANKER` env). Vector rows append as resume-safe JSONL keyed by
//! sha256 of the exact text (queries under `sha256("q:" + qid)`), matching the
//! Python harness in docs/build-log/artifacts/p1-retrieval-bench/harness.py.
//! Plan: docs/superpowers/plans/2026-07-22-p1-retrieval-pipeline-bench-plan.md

use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::fs::OpenOptions;
use std::io::Write as _;
use std::time::Instant;

use serde::Deserialize;
use sha2::{Digest, Sha256};

fn sha256_hex(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

#[derive(Deserialize)]
struct Pool {
    questions: Vec<PoolQuestion>,
}

#[derive(Deserialize)]
struct PoolQuestion {
    qid: String,
    question: String,
    docs: Vec<PoolDoc>,
}

#[derive(Deserialize)]
struct PoolDoc {
    doc_id: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    chunks: Vec<String>,
}

/// Hashes already present in an embed-pool JSONL; tolerates a crash-truncated
/// final line by skipping unparseable rows.
fn existing_hashes(path: &str) -> HashSet<String> {
    let mut seen = HashSet::new();
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            if let Ok(row) = serde_json::from_str::<serde_json::Value>(line)
                && let Some(h) = row.get("hash").and_then(|v| v.as_str())
            {
                seen.insert(h.to_string());
            }
        }
    }
    seen
}

fn percentile(sorted_ms: &[f64], p: f64) -> f64 {
    if sorted_ms.is_empty() {
        return 0.0;
    }
    let idx = ((sorted_ms.len() as f64 - 1.0) * p).round() as usize;
    sorted_ms[idx]
}

pub fn embed_pool_command(args: &[String]) -> Result<(), String> {
    let mut pool_path = None;
    let mut embed_model = None;
    let mut out_path = None;
    let mut queries = false;
    let mut query_prefix = String::new();
    let mut batch = 128usize;
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--pool" => pool_path = it.next().cloned(),
            "--embed-model" => embed_model = it.next().cloned(),
            "--out" => out_path = it.next().cloned(),
            "--queries" => queries = true,
            "--query-prefix" => query_prefix = it.next().cloned().unwrap_or_default(),
            "--batch" => {
                batch = it
                    .next()
                    .and_then(|v| v.parse().ok())
                    .ok_or("--batch requires a positive integer")?;
            }
            other => return Err(format!("embed-pool: unknown argument {other}")),
        }
    }
    let pool_path = pool_path.ok_or("embed-pool: --pool is required")?;
    let embed_model = embed_model.ok_or("embed-pool: --embed-model is required")?;
    let out_path = out_path.ok_or("embed-pool: --out is required")?;
    if batch == 0 {
        return Err("--batch must be positive".into());
    }

    let pool: Pool = serde_json::from_str(
        &fs::read_to_string(&pool_path).map_err(|e| format!("read {pool_path}: {e}"))?,
    )
    .map_err(|e| format!("parse {pool_path}: {e}"))?;
    let embedder = memphant_runtime::embedder_from_id(&embed_model)?;

    let seen = existing_hashes(&out_path);
    // Unique chunk texts in first-seen order so runs are deterministic.
    let mut pending: Vec<(String, String)> = Vec::new();
    let mut queued: HashSet<String> = HashSet::new();
    for q in &pool.questions {
        for d in &q.docs {
            for c in &d.chunks {
                let h = sha256_hex(c);
                if !seen.contains(&h) && queued.insert(h.clone()) {
                    pending.push((h, c.clone()));
                }
            }
        }
    }
    let mut out = OpenOptions::new()
        .create(true)
        .append(true)
        .open(&out_path)
        .map_err(|e| format!("open {out_path}: {e}"))?;

    let started = Instant::now();
    let total = pending.len();
    let mut embedded = 0usize;
    let mut approx_chars = 0usize;
    for chunk_batch in pending.chunks(batch) {
        let texts: Vec<String> = chunk_batch.iter().map(|(_, t)| t.clone()).collect();
        approx_chars += texts.iter().map(|t| t.len()).sum::<usize>();
        let vecs = embedder
            .embed(&texts)
            .map_err(|e| format!("embed batch failed after {embedded}/{total}: {e:?}"))?;
        if vecs.len() != texts.len() {
            return Err(format!(
                "embed returned {} vectors for {} texts",
                vecs.len(),
                texts.len()
            ));
        }
        for ((h, _), v) in chunk_batch.iter().zip(&vecs) {
            let row = serde_json::json!({"hash": h, "vec": v});
            writeln!(out, "{row}").map_err(|e| format!("write {out_path}: {e}"))?;
        }
        out.flush().map_err(|e| e.to_string())?;
        embedded += texts.len();
        if embedded % (batch * 8) < batch {
            eprintln!("embed-pool[{embed_model}]: {embedded}/{total} chunks");
        }
    }

    // Queries one at a time: this is the per-query latency a served recall pays.
    let mut query_ms: Vec<f64> = Vec::new();
    if queries {
        for q in &pool.questions {
            let qh = sha256_hex(&format!("q:{}", q.qid));
            if seen.contains(&qh) {
                continue;
            }
            let text = format!("{query_prefix}{}", q.question);
            let t0 = Instant::now();
            let vecs = embedder
                .embed_query(&[text])
                .map_err(|e| format!("embed_query {} failed: {e:?}", q.qid))?;
            query_ms.push(t0.elapsed().as_secs_f64() * 1000.0);
            let row = serde_json::json!({"hash": qh, "vec": vecs[0]});
            writeln!(out, "{row}").map_err(|e| e.to_string())?;
        }
        out.flush().map_err(|e| e.to_string())?;
    }
    query_ms.sort_by(|a, b| a.partial_cmp(b).expect("finite latency"));
    let summary = serde_json::json!({
        "event": "embed_pool",
        "model": embedder.id(),
        "dims": embedder.dimensions(),
        "embedded": embedded,
        "skipped": seen.len(),
        "queries": query_ms.len(),
        "query_ms_p50": percentile(&query_ms, 0.5),
        "query_ms_p95": percentile(&query_ms, 0.95),
        "elapsed_ms": started.elapsed().as_millis() as u64,
        "approx_tokens": approx_chars / 4,
    });
    println!("{summary}");
    Ok(())
}

#[derive(Deserialize)]
struct CandQuestion {
    qid: String,
    question: String,
    docs: Vec<PoolDoc>,
}

pub fn rerank_pool_command(args: &[String]) -> Result<(), String> {
    let mut cands_path = None;
    let mut out_path = None;
    let mut granularity = "chunk".to_string();
    let mut it = args.iter();
    while let Some(arg) = it.next() {
        match arg.as_str() {
            "--candidates" => cands_path = it.next().cloned(),
            "--out" => out_path = it.next().cloned(),
            "--granularity" => granularity = it.next().cloned().unwrap_or_default(),
            other => return Err(format!("rerank-pool: unknown argument {other}")),
        }
    }
    let cands_path = cands_path.ok_or("rerank-pool: --candidates is required")?;
    let out_path = out_path.ok_or("rerank-pool: --out is required")?;
    if granularity != "chunk" && granularity != "doc" {
        return Err(format!(
            "rerank-pool: --granularity must be chunk|doc, got {granularity}"
        ));
    }

    let cands: Vec<CandQuestion> = serde_json::from_str(
        &fs::read_to_string(&cands_path).map_err(|e| format!("read {cands_path}: {e}"))?,
    )
    .map_err(|e| format!("parse {cands_path}: {e}"))?;
    let reranker = memphant_runtime::build_cross_reranker()?;
    let config = reranker.config();
    eprintln!(
        "rerank-pool: provider={} model={} granularity={granularity}",
        config.provider, config.model
    );

    // Bench-only throttle between questions (default 0), so a hosted reranker's
    // per-minute token/rate limit isn't tripped by a tight 72-question loop.
    let sleep_ms: u64 = std::env::var("POOL_RERANK_SLEEP_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut rows = Vec::new();
    for (ci, c) in cands.iter().enumerate() {
        if sleep_ms > 0 && ci > 0 {
            std::thread::sleep(std::time::Duration::from_millis(sleep_ms));
        }
        // Flatten to (owner doc index, text). Doc granularity = whole text;
        // chunk granularity = each chunk, falling back to text when chunkless.
        let mut owner: Vec<usize> = Vec::new();
        let mut texts: Vec<&str> = Vec::new();
        for (di, d) in c.docs.iter().enumerate() {
            if granularity == "chunk" && !d.chunks.is_empty() {
                for ch in &d.chunks {
                    owner.push(di);
                    texts.push(ch.as_str());
                }
            } else {
                owner.push(di);
                texts.push(d.text.as_str());
            }
        }
        // Bench resilience: retry a per-question rerank failure (e.g. a hosted
        // 429) with exponential backoff instead of aborting the whole run.
        let t0 = Instant::now();
        let mut attempt = 0u32;
        let scores = loop {
            match reranker.rerank(&c.question, &texts) {
                Ok(scores) => break scores,
                Err(e) if attempt < 5 => {
                    let wait = 2_u64.saturating_pow(attempt).saturating_mul(2000).min(30_000);
                    eprintln!(
                        "rerank-pool: {} attempt {} failed ({e}); backing off {wait}ms",
                        c.qid, attempt
                    );
                    std::thread::sleep(std::time::Duration::from_millis(wait));
                    attempt += 1;
                }
                Err(e) => return Err(format!("rerank {} failed after retries: {e}", c.qid)),
            }
        };
        let elapsed_ms = t0.elapsed().as_secs_f64() * 1000.0;
        if scores.len() != texts.len() {
            return Err(format!(
                "rerank {} returned {} scores for {} docs",
                c.qid,
                scores.len(),
                texts.len()
            ));
        }
        let mut doc_scores: Vec<f32> = vec![f32::MIN; c.docs.len()];
        for (s, di) in scores.iter().zip(&owner) {
            if *s > doc_scores[*di] {
                doc_scores[*di] = *s;
            }
        }
        let scores_map: BTreeMap<&str, f32> = c
            .docs
            .iter()
            .zip(&doc_scores)
            .map(|(d, s)| (d.doc_id.as_str(), *s))
            .collect();
        rows.push(serde_json::json!({
            "qid": c.qid,
            "scores": scores_map,
            "docs_scored": texts.len(),
            "elapsed_ms": elapsed_ms,
        }));
        eprintln!(
            "rerank-pool: {} scored {} texts in {:.0}ms",
            c.qid,
            texts.len(),
            elapsed_ms
        );
    }
    fs::write(
        &out_path,
        serde_json::to_string(&rows).map_err(|e| e.to_string())?,
    )
    .map_err(|e| format!("write {out_path}: {e}"))?;
    let mut lats: Vec<f64> = rows
        .iter()
        .map(|r| r["elapsed_ms"].as_f64().expect("recorded above"))
        .collect();
    lats.sort_by(|a, b| a.partial_cmp(b).expect("finite latency"));
    println!(
        "{}",
        serde_json::json!({
            "event": "rerank_pool",
            "provider": config.provider,
            "model": config.model,
            "granularity": granularity,
            "questions": rows.len(),
            "lat_ms_p50": percentile(&lats, 0.5),
            "lat_ms_p95": percentile(&lats, 0.95),
        })
    );
    Ok(())
}
