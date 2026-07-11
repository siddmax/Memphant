//! LongMemEval retrieval-only benchmark lane against the packaged Postgres
//! runtime (`MemoryService<PgStore>`).
//!
//! Per sampled question (stratified by `question_type`, seeded, identical
//! sample for every run at the same seed): create a fresh tenant, ingest each
//! haystack session chronologically as ONE episode (turns concatenated as
//! `role: content`, body prefixed with the session id and date), reflect via
//! the worker claim/complete path, then recall the question and score
//! Recall@5/@10 by provenance: a top-k item hits when its
//! `citation_episode_id` maps back to a session in `answer_session_ids`.
//! Abstention questions (`_abs` in the question id) are scored separately.
//!
//! Honesty header: every report records the dataset sha256, sample seed,
//! `runtime: "postgres"`, `retrieval_only: true` and the exact command line.
//! This lane makes NO reader/QA-accuracy claim by itself.
//!
//! Reader lane: `--emit-qa <path>` additionally writes one JSONL row per
//! question (question, question_date, gold answer, top-k evidence bodies with
//! provenance) so `scripts/run_reader.py` can drive an external reader/judge
//! (`claude -p`) without re-running ingestion. QA accuracy is computed and
//! labeled by that script, never by this lane.
//!
//! Granularity: `--granularity session` (the lane default again — the product
//! path is session ingestion + service-side runtime contextual chunks, see
//! `DEFAULT_GRANULARITY`) ingests each haystack session as ONE episode;
//! `--granularity turns` (still available for the ablation) ingests each
//! session as multiple episodes of up to `--turns-window` consecutive turns
//! (default `DEFAULT_TURNS_WINDOW`=4, same `[session <id>]` prefix), mapping
//! every minted episode back to its session for provenance scoring.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;

use memphant_core::service::MemoryService;
use memphant_core::{DEFAULT_CANDIDATE_POOL_SIZE, EmbeddingProvider, NoopEmbedding, SystemClock};
use memphant_store_postgres::PgStore;
use memphant_types::{
    ActorId, RecallHttpRequest, RecallMode, RetainEpisodeHttpRequest, ScopeId, TenantId, TrustLevel,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const BOOTSTRAP_RESAMPLES: usize = 1000;
/// Default turn-window size for `--granularity turns`, overridable via
/// `--turns-window`.
pub const DEFAULT_TURNS_WINDOW: usize = 4;
/// Default packing token budget threaded to the recall call, overridable via
/// `--budget-tokens`.
pub const DEFAULT_BUDGET_TOKENS: usize = 8192;
/// Lane default ingestion granularity, back to "session" as of the 2026-07-10
/// rung 4 promotion: the lane now measures the PRODUCT path (session ingestion
/// plus service-side runtime contextual chunks, default-on). The earlier
/// same-day "turns" promotion is SUPERSEDED by the runtime embodiment: runtime
/// chunks tie client-side turns windowing (ΔQA +0.000 [−0.080, +0.080] ns)
/// while needing no caller-side windowing, so the product path is measured
/// directly. Proof:
/// `docs/build-log/artifacts/real-retrieval-20260710/scaled-reader-or-session-chunkpack-rerank-off.json`.
/// `--granularity turns` stays available for the ablation. The serde
/// `default_granularity` below also reads "session", but for an independent
/// reason (pre-granularity REPORTS were session runs), not this lane default.
pub const DEFAULT_GRANULARITY: &str = "session";

#[derive(Debug, Clone, Deserialize)]
pub struct LmeQuestion {
    pub question_id: String,
    pub question_type: String,
    pub question: String,
    /// Gold answer (string or number in the published dataset).
    #[serde(default)]
    pub answer: serde_json::Value,
    #[serde(default)]
    pub question_date: Option<String>,
    pub haystack_session_ids: Vec<String>,
    pub haystack_dates: Vec<String>,
    pub haystack_sessions: Vec<Vec<LmeTurn>>,
    pub answer_session_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct LmeTurn {
    pub role: String,
    pub content: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchLmeOptions {
    pub database_url: String,
    pub data_path: String,
    pub sample: usize,
    pub seed: u64,
    pub k: usize,
    /// One of: vector, edge_expansion, rerank, query_decomposition,
    /// procedure_recall, decay, packing.
    pub disable: Option<String>,
    pub mode: RecallMode,
    /// Baseline report path for paired per-question deltas.
    pub baseline: Option<String>,
    /// Ingestion granularity: "session" (one episode per haystack session)
    /// or "turns" (episodes of up to `turns_window` consecutive turns).
    pub granularity: String,
    /// Turn-window size for `--granularity turns` (no-op for "session").
    pub turns_window: usize,
    /// Packing token budget threaded to the recall call.
    pub budget_tokens: usize,
    /// Vector-channel candidate-pool size (`--pool`) threaded to the recall
    /// service via `with_candidate_pool_size`. Default
    /// `DEFAULT_CANDIDATE_POOL_SIZE` (32) reproduces today's ranking.
    pub pool: usize,
    /// W4 sibling-gather packing lever (`--sibling-gather`, default off) threaded
    /// via `with_sibling_gather_enabled`. The measurement-campaign flag; off
    /// reproduces today's packing.
    pub sibling_gather: bool,
    /// W4 per-session diversity quota (`--session-quota <n>`, default off =
    /// `None`) threaded via `with_session_quota`.
    pub session_quota: Option<usize>,
    /// W5 temporal-grounding flag (`--temporal-grounding`, default off) threaded
    /// via `with_temporal_grounding_enabled` to BOTH the ingest service (so
    /// `valid_from` and chunk headers are date-grounded at reflect) and the
    /// recall service (query-date windowing + dated packs).
    pub temporal_grounding: bool,
    /// Rung 4 runtime contextual-chunk write path opt-in flag
    /// (`--runtime-chunks`, default true = the product path). The EFFECTIVE
    /// state also depends on `--disable runtime_chunks`, which forces the
    /// chunks-off control arm; see `runtime_chunks_enabled` in `run_bench_lme`.
    pub runtime_chunks: bool,
    /// When set, write one QA-evidence JSONL row per question to this path
    /// (question + gold answer + top-k evidence bodies) for the external
    /// reader/judge in `scripts/run_reader.py`.
    pub emit_qa: Option<String>,
    pub command: String,
}

/// One top-k evidence item handed to the external reader.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaEvidenceItem {
    pub rank: usize,
    /// Haystack session this item's citation maps back to, when known.
    pub session_id: Option<String>,
    pub body: String,
}

/// One QA-evidence JSONL row (input contract of `scripts/run_reader.py`).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QaEvidenceRow {
    pub question_id: String,
    pub question_type: String,
    pub is_abstention: bool,
    pub question: String,
    pub question_date: Option<String>,
    pub gold_answer: serde_json::Value,
    pub abstained: bool,
    pub granularity: String,
    pub k: usize,
    pub evidence: Vec<QaEvidenceItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuestionResult {
    pub question_id: String,
    pub question_type: String,
    pub is_abstention: bool,
    /// None for abstention questions (scored separately).
    pub hit_at_5: Option<bool>,
    pub hit_at_10: Option<bool>,
    /// Some(...) only for abstention questions: correct when recall abstained
    /// or returned no answer-session item.
    pub abstention_correct: Option<bool>,
    /// 1-based rank of the first answer-bearing item, if any.
    pub first_answer_rank: Option<usize>,
    pub returned_items: usize,
    pub degraded: bool,
    pub ingested_sessions: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StratumMetrics {
    pub question_type: String,
    pub n: usize,
    pub n_scored: usize,
    pub recall_at_5: Option<f64>,
    pub recall_at_10: Option<f64>,
    pub abstention_n: usize,
    pub abstention_correct: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeltaCi {
    pub mean: f64,
    pub ci95_low: f64,
    pub ci95_high: f64,
    /// True when the bootstrap 95% CI excludes zero.
    pub ci_excludes_zero: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedComparison {
    pub baseline_path: String,
    pub n_paired: usize,
    pub delta_recall_at_5: DeltaCi,
    pub delta_recall_at_10: DeltaCi,
    pub bootstrap_resamples: usize,
    pub bootstrap_seed: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchLmeReport {
    pub benchmark: String,
    pub dataset_path: String,
    pub dataset_sha256: String,
    pub dataset_questions: usize,
    pub sample_seed: u64,
    pub sample_n: usize,
    pub k: usize,
    pub runtime: String,
    pub retrieval_only: bool,
    pub embeddings: String,
    pub ingestion: String,
    pub reflect: String,
    /// "session" or "turns" (defaults to "session" for pre-granularity reports).
    #[serde(default = "default_granularity")]
    pub granularity: String,
    /// Turn-window size used for `--granularity turns` (defaults to 4 for
    /// pre-flag reports — see `default_turns_window`).
    #[serde(default = "default_turns_window")]
    pub turns_window: usize,
    /// Packing token budget threaded to the recall call (defaults to 8192
    /// for pre-flag reports — see `default_budget_tokens`).
    #[serde(default = "default_budget_tokens")]
    pub budget_tokens: usize,
    /// Vector-channel candidate-pool size (`--pool`) used for this run. Defaults
    /// to `DEFAULT_CANDIDATE_POOL_SIZE` (32) — the historical vector KNN fan-out
    /// — for pre-flag reports, via `default_candidate_pool_size`.
    #[serde(default = "default_candidate_pool_size")]
    pub candidate_pool_size: usize,
    /// Whether the W4 sibling-gather packing lever was on for this run. The serde
    /// default is `false`: every report written before the lever existed was a
    /// sibling-gather-off run, so an absent field ⇒ off.
    #[serde(default)]
    pub sibling_gather: bool,
    /// The W4 per-session diversity quota used for this run (`--session-quota`),
    /// or `None` when off. Serde default `None` for pre-flag reports.
    #[serde(default)]
    pub session_quota: Option<usize>,
    /// Whether W5 temporal grounding (`--temporal-grounding`) was on for this
    /// run. Serde default `false`: every report written before the flag existed
    /// was a temporal-grounding-off run, so an absent field ⇒ off.
    #[serde(default)]
    pub temporal_grounding: bool,
    /// Whether the rung 4 runtime contextual-chunk write path was enabled for
    /// this run — records the EFFECTIVE state (default-on since the 2026-07-10
    /// promotion; `--disable runtime_chunks` records false). The serde default
    /// STAYS false: every pre-promotion report was a chunks-off run unless it
    /// recorded otherwise, so an absent field ⇒ false, never following the lane
    /// default.
    #[serde(default)]
    pub runtime_chunks: bool,
    pub mode: String,
    pub disabled: Option<String>,
    pub command: String,
    pub generated_at_unix: u64,
    pub overall: StratumMetrics,
    pub strata: Vec<StratumMetrics>,
    pub per_question: Vec<QuestionResult>,
    pub paired_vs_baseline: Option<PairedComparison>,
}

fn default_granularity() -> String {
    "session".to_string()
}

/// Parsing default for pre-flag reports (no `turns_window` field). Unlike
/// `default_granularity`, this genuinely coincides with the lane default
/// (`DEFAULT_TURNS_WINDOW`): every report ever written before this field
/// existed actually used window 4.
fn default_turns_window() -> usize {
    DEFAULT_TURNS_WINDOW
}

/// Parsing default for pre-flag reports (no `budget_tokens` field). As with
/// `default_turns_window`, this coincides with the lane default
/// (`DEFAULT_BUDGET_TOKENS`): every report ever written before this field
/// existed actually used budget 8192.
fn default_budget_tokens() -> usize {
    DEFAULT_BUDGET_TOKENS
}

/// Parsing default for pre-flag reports (no `candidate_pool_size` field). As
/// with `default_budget_tokens`, this coincides with the historical value every
/// such report used: the vector KNN fan-out of `DEFAULT_CANDIDATE_POOL_SIZE`
/// (32).
fn default_candidate_pool_size() -> usize {
    DEFAULT_CANDIDATE_POOL_SIZE
}

/// Deterministic splitmix64 PRNG — no external randomness anywhere in the
/// lane, so a (seed, sample) pair always names the same question set.
pub struct SplitMix64 {
    state: u64,
}

impl SplitMix64 {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    pub fn next_below(&mut self, bound: usize) -> usize {
        (self.next_u64() % bound.max(1) as u64) as usize
    }
}

fn stratum_seed(seed: u64, stratum: &str) -> u64 {
    let mut hash: u64 = 0xcbf29ce484222325;
    for byte in stratum.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    seed ^ hash
}

/// Stratified, seeded sample: proportional allocation (largest remainder,
/// minimum one per stratum when the budget allows), then a seeded
/// Fisher-Yates shuffle inside each stratum sorted by question id.
pub fn stratified_sample_ids(
    questions: &[(String, String)],
    sample: usize,
    seed: u64,
) -> Vec<String> {
    let mut strata: Vec<(String, Vec<String>)> = Vec::new();
    for (id, stratum) in questions {
        match strata.iter_mut().find(|(name, _)| name == stratum) {
            Some((_, ids)) => ids.push(id.clone()),
            None => strata.push((stratum.clone(), vec![id.clone()])),
        }
    }
    strata.sort_by(|left, right| left.0.cmp(&right.0));
    for (_, ids) in &mut strata {
        ids.sort();
    }
    let total = questions.len();
    let sample = sample.min(total);

    // Largest-remainder proportional allocation.
    let mut allocations: Vec<(usize, f64)> = strata
        .iter()
        .map(|(_, ids)| {
            let exact = sample as f64 * ids.len() as f64 / total as f64;
            (exact.floor() as usize, exact - exact.floor())
        })
        .collect();
    if sample >= strata.len() {
        for (index, (floor, _)) in allocations.iter_mut().enumerate() {
            if *floor == 0 && !strata[index].1.is_empty() {
                *floor = 1;
            }
        }
    }
    let mut assigned: usize = allocations.iter().map(|(floor, _)| *floor).sum();
    let mut order: Vec<usize> = (0..allocations.len()).collect();
    order.sort_by(|a, b| {
        allocations[*b]
            .1
            .partial_cmp(&allocations[*a].1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| strata[*a].0.cmp(&strata[*b].0))
    });
    let mut cursor = 0;
    while assigned < sample {
        let index = order[cursor % order.len()];
        if allocations[index].0 < strata[index].1.len() {
            allocations[index].0 += 1;
            assigned += 1;
        }
        cursor += 1;
        if cursor > order.len() * (sample + 1) {
            break; // every stratum exhausted
        }
    }
    while assigned > sample {
        let index = order[cursor % order.len()];
        if allocations[index].0 > 1 {
            allocations[index].0 -= 1;
            assigned -= 1;
        }
        cursor += 1;
    }

    let mut picked = Vec::new();
    for ((stratum, ids), (count, _)) in strata.iter().zip(allocations.iter()) {
        let mut pool = ids.clone();
        let mut rng = SplitMix64::new(stratum_seed(seed, stratum));
        for index in (1..pool.len()).rev() {
            let swap = rng.next_below(index + 1);
            pool.swap(index, swap);
        }
        picked.extend(pool.into_iter().take((*count).min(ids.len())));
    }
    picked.sort();
    picked
}

/// Pure scorer: `item_sessions` is the recalled items' provenance
/// (rank-ordered session ids, None when an item has no episode citation).
pub fn score_question(
    item_sessions: &[Option<String>],
    answer_session_ids: &[String],
    is_abstention: bool,
    abstained: bool,
) -> (Option<bool>, Option<bool>, Option<bool>, Option<usize>) {
    let first_answer_rank = item_sessions.iter().enumerate().find_map(|(index, item)| {
        item.as_ref()
            .filter(|session| answer_session_ids.iter().any(|answer| answer == *session))
            .map(|_| index + 1)
    });
    if is_abstention {
        let correct = abstained || first_answer_rank.is_none();
        (None, None, Some(correct), first_answer_rank)
    } else {
        let hit5 = first_answer_rank.is_some_and(|rank| rank <= 5);
        let hit10 = first_answer_rank.is_some_and(|rank| rank <= 10);
        (Some(hit5), Some(hit10), None, first_answer_rank)
    }
}

/// Bootstrap 95% CI over per-question paired deltas (seeded, deterministic).
pub fn bootstrap_ci(deltas: &[f64], resamples: usize, seed: u64) -> DeltaCi {
    let n = deltas.len();
    let mean = if n == 0 {
        0.0
    } else {
        deltas.iter().sum::<f64>() / n as f64
    };
    if n == 0 {
        return DeltaCi {
            mean,
            ci95_low: 0.0,
            ci95_high: 0.0,
            ci_excludes_zero: false,
        };
    }
    let mut rng = SplitMix64::new(seed);
    let mut means = Vec::with_capacity(resamples);
    for _ in 0..resamples {
        let mut sum = 0.0;
        for _ in 0..n {
            sum += deltas[rng.next_below(n)];
        }
        means.push(sum / n as f64);
    }
    means.sort_by(|left, right| left.partial_cmp(right).unwrap_or(std::cmp::Ordering::Equal));
    let low = means[((resamples as f64 * 0.025).floor() as usize).min(resamples - 1)];
    let high = means[((resamples as f64 * 0.975).ceil() as usize - 1).min(resamples - 1)];
    DeltaCi {
        mean,
        ci95_low: low,
        ci95_high: high,
        ci_excludes_zero: low > 0.0 || high < 0.0,
    }
}

fn aggregate(name: &str, rows: &[&QuestionResult]) -> StratumMetrics {
    let scored: Vec<_> = rows.iter().filter(|row| !row.is_abstention).collect();
    let abstentions: Vec<_> = rows.iter().filter(|row| row.is_abstention).collect();
    let ratio = |hits: usize, n: usize| (n > 0).then(|| hits as f64 / n as f64);
    StratumMetrics {
        question_type: name.to_string(),
        n: rows.len(),
        n_scored: scored.len(),
        recall_at_5: ratio(
            scored
                .iter()
                .filter(|row| row.hit_at_5 == Some(true))
                .count(),
            scored.len(),
        ),
        recall_at_10: ratio(
            scored
                .iter()
                .filter(|row| row.hit_at_10 == Some(true))
                .count(),
            scored.len(),
        ),
        abstention_n: abstentions.len(),
        abstention_correct: abstentions
            .iter()
            .filter(|row| row.abstention_correct == Some(true))
            .count(),
    }
}

fn paired_comparison(
    current: &[QuestionResult],
    baseline: &BenchLmeReport,
    baseline_path: &str,
    seed: u64,
) -> PairedComparison {
    let baseline_rows: HashMap<&str, &QuestionResult> = baseline
        .per_question
        .iter()
        .map(|row| (row.question_id.as_str(), row))
        .collect();
    let mut deltas5 = Vec::new();
    let mut deltas10 = Vec::new();
    for row in current.iter().filter(|row| !row.is_abstention) {
        let Some(base) = baseline_rows.get(row.question_id.as_str()) else {
            continue;
        };
        let (Some(hit5), Some(hit10), Some(base5), Some(base10)) =
            (row.hit_at_5, row.hit_at_10, base.hit_at_5, base.hit_at_10)
        else {
            continue;
        };
        deltas5.push(f64::from(u8::from(hit5)) - f64::from(u8::from(base5)));
        deltas10.push(f64::from(u8::from(hit10)) - f64::from(u8::from(base10)));
    }
    PairedComparison {
        baseline_path: baseline_path.to_string(),
        n_paired: deltas5.len(),
        delta_recall_at_5: bootstrap_ci(&deltas5, BOOTSTRAP_RESAMPLES, seed),
        delta_recall_at_10: bootstrap_ci(&deltas10, BOOTSTRAP_RESAMPLES, seed),
        bootstrap_resamples: BOOTSTRAP_RESAMPLES,
        bootstrap_seed: seed,
    }
}

fn session_body(session_id: &str, date: &str, turns: &[LmeTurn]) -> String {
    let mut body = format!("[session {session_id}] [date {date}]\n");
    for turn in turns {
        body.push_str(&turn.role);
        body.push_str(": ");
        body.push_str(&turn.content);
        body.push('\n');
    }
    body
}

/// Episode bodies for one haystack session at the requested granularity:
/// "session" yields one body; "turns" yields one body per window of up to
/// `turns_window` consecutive turns, each keeping the `[session <id>]`
/// prefix plus a `[turns a-b]` marker so provenance stays per-session.
pub fn session_bodies(
    granularity: &str,
    turns_window: usize,
    session_id: &str,
    date: &str,
    turns: &[LmeTurn],
) -> Vec<String> {
    if granularity != "turns" {
        return vec![session_body(session_id, date, turns)];
    }
    if turns.is_empty() {
        return vec![session_body(session_id, date, turns)];
    }
    turns
        .chunks(turns_window)
        .enumerate()
        .map(|(window_index, window)| {
            let first = window_index * turns_window + 1;
            let last = window_index * turns_window + window.len();
            let mut body = format!("[session {session_id}] [date {date}] [turns {first}-{last}]\n");
            for turn in window {
                body.push_str(&turn.role);
                body.push_str(": ");
                body.push_str(&turn.content);
                body.push('\n');
            }
            body
        })
        .collect()
}

#[cfg(feature = "fastembed")]
fn build_fastembed() -> Result<Arc<dyn EmbeddingProvider>, String> {
    memphant_runtime::embeddings::FastEmbedProvider::new()
        .map(|provider| Arc::new(provider) as Arc<dyn EmbeddingProvider>)
        .map_err(|error| format!("fastembed initialization failed: {error}"))
}

#[cfg(not(feature = "fastembed"))]
fn build_fastembed() -> Result<Arc<dyn EmbeddingProvider>, String> {
    Err("bench-lme requires a binary built with --features fastembed (real embeddings are part of the benchmark contract)".to_string())
}

fn sha256_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(format!("{:x}", hasher.finalize()))
}

pub fn run_bench_lme(options: &BenchLmeOptions) -> Result<BenchLmeReport, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("tokio runtime: {error}"))?;
    runtime.block_on(run_bench_lme_async(options))
}

async fn run_bench_lme_async(options: &BenchLmeOptions) -> Result<BenchLmeReport, String> {
    let data_path = Path::new(&options.data_path);
    let dataset_sha256 = sha256_file(data_path)?;
    let raw = std::fs::read_to_string(data_path)
        .map_err(|error| format!("read {}: {error}", data_path.display()))?;
    let questions: Vec<LmeQuestion> =
        serde_json::from_str(&raw).map_err(|error| format!("parse dataset: {error}"))?;
    let dataset_questions = questions.len();

    let id_pairs: Vec<(String, String)> = questions
        .iter()
        .map(|question| (question.question_id.clone(), question.question_type.clone()))
        .collect();
    let sampled_ids = stratified_sample_ids(&id_pairs, options.sample, options.seed);
    let by_id: HashMap<&str, &LmeQuestion> = questions
        .iter()
        .map(|question| (question.question_id.as_str(), question))
        .collect();

    let store = Arc::new(
        PgStore::connect(&options.database_url)
            .await
            .map_err(|error| format!("postgres connect: {error}"))?,
    );
    let embedder = build_fastembed()?;
    // Effective runtime contextual-chunk state: default-on (the product path)
    // unless the control arm explicitly disables it. `--disable runtime_chunks`
    // forces the builder off; `--runtime-chunks` is redundant with the default
    // but kept as an explicit opt-in. The report records THIS effective value.
    let runtime_chunks_enabled =
        options.runtime_chunks && options.disable.as_deref() != Some("runtime_chunks");
    let ingest_service = MemoryService::new(
        Arc::clone(&store),
        Arc::new(SystemClock),
        Arc::clone(&embedder),
    )
    .with_contextual_chunks_write_enabled(runtime_chunks_enabled)
    // W5: the ingest path grounds `valid_from` + dates chunk headers at reflect,
    // so the flag must be set here as well as on the recall service.
    .with_temporal_grounding_enabled(options.temporal_grounding);
    let vector_disabled = options.disable.as_deref() == Some("vector");
    // Vector ablation: same store/units, but the recall-side service embeds
    // with Noop so `query_vec` is None and the vector channel is honestly off.
    let recall_service = if vector_disabled {
        MemoryService::new(
            Arc::clone(&store),
            Arc::new(SystemClock),
            Arc::new(NoopEmbedding),
        )
    } else {
        ingest_service.clone()
    }
    // W3 candidate-pool knob (`--pool`): widens the recall vector-channel KNN
    // fan-out for the rerank pool. Recall-time only; ingestion is unaffected.
    .with_candidate_pool_size(options.pool)
    // W4 packing levers (`--sibling-gather` / `--session-quota`): recall-time
    // only; both default off so the campaign measures each independently.
    .with_sibling_gather_enabled(options.sibling_gather)
    .with_session_quota(options.session_quota)
    // W5 temporal grounding: query-date windowing + dated packs at recall. Set
    // explicitly here too so the vector-disabled fresh recall service (which is
    // not a clone of `ingest_service`) also carries the flag.
    .with_temporal_grounding_enabled(options.temporal_grounding);

    if options.granularity != "session" && options.granularity != "turns" {
        return Err(format!(
            "unknown --granularity: {} (known: session, turns)",
            options.granularity
        ));
    }

    let disable = options.disable.as_deref();
    if let Some(stage) = disable {
        let known = [
            "vector",
            "edge_expansion",
            "rerank",
            "query_decomposition",
            "procedure_recall",
            "decay",
            "packing",
            "runtime_chunks",
        ];
        if !known.contains(&stage) {
            return Err(format!(
                "unknown --disable stage: {stage} (known: {known:?})"
            ));
        }
    }

    // Unique per-run slug nonce: every run ingests into FRESH tenants (fresh
    // tenant per question), so repeated runs never collide or share state.
    let run_nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0);

    let mut per_question = Vec::new();
    let mut qa_rows: Vec<QaEvidenceRow> = Vec::new();
    for (index, question_id) in sampled_ids.iter().enumerate() {
        let question = by_id
            .get(question_id.as_str())
            .ok_or_else(|| format!("sampled id missing from dataset: {question_id}"))?;
        eprintln!(
            "bench-lme [{}/{}] {} ({}) sessions={}",
            index + 1,
            sampled_ids.len(),
            question.question_id,
            question.question_type,
            question.haystack_sessions.len()
        );

        let tenant_uuid = store
            .create_tenant(&format!(
                "lme-{run_nonce}-{}-{}",
                options.seed, question.question_id
            ))
            .await
            .map_err(|error| format!("create_tenant: {error}"))?;
        let tenant = TenantId::from_u128(tenant_uuid.as_u128());
        let scope = ScopeId::new();
        let actor = ActorId::new();

        // Chronological ingestion: one episode per haystack session.
        let mut order: Vec<usize> = (0..question.haystack_sessions.len()).collect();
        order.sort_by(|left, right| {
            question.haystack_dates[*left]
                .cmp(&question.haystack_dates[*right])
                .then(left.cmp(right))
        });
        let mut episode_sessions = HashMap::new();
        for session_index in order {
            let session_id = &question.haystack_session_ids[session_index];
            let bodies = session_bodies(
                &options.granularity,
                options.turns_window,
                session_id,
                &question.haystack_dates[session_index],
                &question.haystack_sessions[session_index],
            );
            for body in bodies {
                let response = ingest_service
                    .retain(
                        tenant,
                        RetainEpisodeHttpRequest {
                            tenant_id: tenant,
                            scope_id: scope,
                            actor_id: actor,
                            source_kind: "user".to_string(),
                            source_trust: TrustLevel::TrustedUser,
                            subject_hint: Some(format!("session {session_id}")),
                            subject: None,
                            predicate: None,
                            body: Some(body),
                            resource: None,
                            unit: None,
                            compiler_version: None,
                        },
                    )
                    .await
                    .map_err(|error| format!("retain {session_id}: {error}"))?;
                if let Some(episode_id) = response.episode_id {
                    episode_sessions
                        .entry(episode_id)
                        .or_insert_with(|| session_id.clone());
                }
            }
        }

        // Reflect through the same claim/complete path the worker uses.
        ingest_service
            .reflect(tenant, scope, None)
            .await
            .map_err(|error| format!("reflect: {error}"))?;

        let response = recall_service
            .recall(
                tenant,
                RecallHttpRequest {
                    tenant_id: tenant,
                    scope_id: scope,
                    actor_id: actor,
                    allowed_scope_ids: None,
                    query: question.question.clone(),
                    limit: Some(options.k),
                    budget_tokens: Some(options.budget_tokens),
                    mode: Some(options.mode),
                    include_beliefs: Some(false),
                    edge_expansion_enabled: Some(disable != Some("edge_expansion")),
                    context_packing_abstention_enabled: Some(disable != Some("packing")),
                    rerank_enabled: Some(disable != Some("rerank")),
                    query_decomposition_enabled: Some(disable != Some("query_decomposition")),
                    procedure_recall_enabled: Some(disable != Some("procedure_recall")),
                    decay_enabled: Some(disable != Some("decay")),
                    include_trace: Some(false),
                },
            )
            .await
            .map_err(|error| format!("recall: {error}"))?;

        let item_sessions: Vec<Option<String>> = response
            .items
            .iter()
            .map(|item| {
                item.citation_episode_id
                    .and_then(|episode| episode_sessions.get(&episode).cloned())
            })
            .collect();
        let is_abstention = question.question_id.contains("_abs");
        if options.emit_qa.is_some() {
            qa_rows.push(QaEvidenceRow {
                question_id: question.question_id.clone(),
                question_type: question.question_type.clone(),
                is_abstention,
                question: question.question.clone(),
                question_date: question.question_date.clone(),
                gold_answer: question.answer.clone(),
                abstained: response.abstention,
                granularity: options.granularity.clone(),
                k: options.k,
                evidence: response
                    .items
                    .iter()
                    .zip(item_sessions.iter())
                    .enumerate()
                    .map(|(rank_index, (item, session))| QaEvidenceItem {
                        rank: rank_index + 1,
                        session_id: session.clone(),
                        body: item.body.clone(),
                    })
                    .collect(),
            });
        }
        let (hit_at_5, hit_at_10, abstention_correct, first_answer_rank) = score_question(
            &item_sessions,
            &question.answer_session_ids,
            is_abstention,
            response.abstention,
        );
        per_question.push(QuestionResult {
            question_id: question.question_id.clone(),
            question_type: question.question_type.clone(),
            is_abstention,
            hit_at_5,
            hit_at_10,
            abstention_correct,
            first_answer_rank,
            returned_items: response.items.len(),
            degraded: response.degraded,
            ingested_sessions: question.haystack_sessions.len(),
        });
    }

    let mut stratum_names: Vec<String> = per_question
        .iter()
        .map(|row| row.question_type.clone())
        .collect();
    stratum_names.sort();
    stratum_names.dedup();
    let strata = stratum_names
        .iter()
        .map(|name| {
            let rows: Vec<&QuestionResult> = per_question
                .iter()
                .filter(|row| &row.question_type == name)
                .collect();
            aggregate(name, &rows)
        })
        .collect();
    let overall = aggregate("overall", &per_question.iter().collect::<Vec<_>>());

    if let Some(path) = &options.emit_qa {
        let mut lines = String::new();
        for row in &qa_rows {
            lines.push_str(
                &serde_json::to_string(row)
                    .map_err(|error| format!("serialize qa row: {error}"))?,
            );
            lines.push('\n');
        }
        std::fs::write(path, lines).map_err(|error| format!("write qa jsonl {path}: {error}"))?;
        eprintln!("bench-lme qa evidence rows={} out={path}", qa_rows.len());
    }

    let paired_vs_baseline = match &options.baseline {
        Some(path) => {
            let raw = std::fs::read_to_string(path)
                .map_err(|error| format!("read baseline {path}: {error}"))?;
            let baseline: BenchLmeReport =
                serde_json::from_str(&raw).map_err(|error| format!("parse baseline: {error}"))?;
            if baseline.dataset_sha256 != dataset_sha256 {
                return Err("baseline dataset sha256 differs from current dataset".to_string());
            }
            if baseline.sample_seed != options.seed || baseline.sample_n != options.sample {
                return Err(
                    "baseline sample seed/size differs — deltas would be unpaired".to_string(),
                );
            }
            Some(paired_comparison(
                &per_question,
                &baseline,
                path,
                options.seed,
            ))
        }
        None => None,
    };

    Ok(BenchLmeReport {
        benchmark: "longmemeval_retrieval_only".to_string(),
        dataset_path: options.data_path.clone(),
        dataset_sha256,
        dataset_questions,
        sample_seed: options.seed,
        sample_n: sampled_ids.len(),
        k: options.k,
        runtime: "postgres".to_string(),
        retrieval_only: true,
        embeddings: if vector_disabled {
            format!(
                "{} for ingestion; query vector disabled (query_vec=None)",
                embedder.id()
            )
        } else {
            embedder.id().to_string()
        },
        ingestion: if options.granularity == "turns" {
            format!(
                "episodes of up to {} consecutive turns per haystack session, chronological by haystack_dates; turns concatenated as `role: content`; body prefixed with [session <id>] [date <date>] [turns a-b]",
                options.turns_window
            )
        } else {
            "one episode per haystack session, chronological by haystack_dates; turns concatenated as `role: content`; body prefixed with [session <id>] [date <date>]".to_string()
        },
        reflect: "MemoryService::reflect (worker claim/complete path), synchronous after ingestion"
            .to_string(),
        granularity: options.granularity.clone(),
        turns_window: options.turns_window,
        budget_tokens: options.budget_tokens,
        candidate_pool_size: options.pool,
        sibling_gather: options.sibling_gather,
        session_quota: options.session_quota,
        temporal_grounding: options.temporal_grounding,
        runtime_chunks: runtime_chunks_enabled,
        mode: match options.mode {
            RecallMode::Fast => "fast",
            RecallMode::Balanced => "balanced",
            RecallMode::Exhaustive => "exhaustive",
        }
        .to_string(),
        disabled: options.disable.clone(),
        command: options.command.clone(),
        generated_at_unix: std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_secs())
            .unwrap_or(0),
        overall,
        strata,
        per_question,
        paired_vs_baseline,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture_rows() -> Vec<QuestionResult> {
        // Synthetic 3-question fixture: one hit@5, one hit@10-only, one abstention.
        let (h5a, h10a, absa, ranka) = score_question(
            &[
                Some("s_other".to_string()),
                Some("s_answer".to_string()),
                None,
            ],
            &["s_answer".to_string()],
            false,
            false,
        );
        let (h5b, h10b, absb, rankb) = score_question(
            &[
                None,
                Some("s1".to_string()),
                Some("s2".to_string()),
                Some("s3".to_string()),
                Some("s4".to_string()),
                Some("s5".to_string()),
                Some("s_answer".to_string()),
            ],
            &["s_answer".to_string()],
            false,
            false,
        );
        let (h5c, h10c, absc, rankc) = score_question(
            &[Some("s_noise".to_string())],
            &["answer_missing_abs".to_string()],
            true,
            false,
        );
        vec![
            QuestionResult {
                question_id: "q1".to_string(),
                question_type: "multi-session".to_string(),
                is_abstention: false,
                hit_at_5: h5a,
                hit_at_10: h10a,
                abstention_correct: absa,
                first_answer_rank: ranka,
                returned_items: 3,
                degraded: false,
                ingested_sessions: 3,
            },
            QuestionResult {
                question_id: "q2".to_string(),
                question_type: "multi-session".to_string(),
                is_abstention: false,
                hit_at_5: h5b,
                hit_at_10: h10b,
                abstention_correct: absb,
                first_answer_rank: rankb,
                returned_items: 7,
                degraded: false,
                ingested_sessions: 3,
            },
            QuestionResult {
                question_id: "q3_abs".to_string(),
                question_type: "knowledge-update".to_string(),
                is_abstention: true,
                hit_at_5: h5c,
                hit_at_10: h10c,
                abstention_correct: absc,
                first_answer_rank: rankc,
                returned_items: 1,
                degraded: false,
                ingested_sessions: 2,
            },
        ]
    }

    #[test]
    fn scorer_ranks_hits_and_abstentions() {
        let rows = fixture_rows();
        assert_eq!(rows[0].hit_at_5, Some(true));
        assert_eq!(rows[0].hit_at_10, Some(true));
        assert_eq!(rows[0].first_answer_rank, Some(2));
        assert_eq!(rows[1].hit_at_5, Some(false));
        assert_eq!(rows[1].hit_at_10, Some(true));
        assert_eq!(rows[1].first_answer_rank, Some(7));
        assert_eq!(rows[2].hit_at_5, None);
        assert_eq!(rows[2].abstention_correct, Some(true));
    }

    #[test]
    fn abstention_fails_when_answer_session_returned_without_abstaining() {
        let (_, _, correct, rank) = score_question(
            &[Some("s_answer".to_string())],
            &["s_answer".to_string()],
            true,
            false,
        );
        assert_eq!(correct, Some(false));
        assert_eq!(rank, Some(1));
        // But abstaining is always correct for an abstention question.
        let (_, _, correct, _) = score_question(
            &[Some("s_answer".to_string())],
            &["s_answer".to_string()],
            true,
            true,
        );
        assert_eq!(correct, Some(true));
    }

    #[test]
    fn aggregate_splits_abstentions_from_scored() {
        let rows = fixture_rows();
        let refs: Vec<&QuestionResult> = rows.iter().collect();
        let overall = aggregate("overall", &refs);
        assert_eq!(overall.n, 3);
        assert_eq!(overall.n_scored, 2);
        assert_eq!(overall.recall_at_5, Some(0.5));
        assert_eq!(overall.recall_at_10, Some(1.0));
        assert_eq!(overall.abstention_n, 1);
        assert_eq!(overall.abstention_correct, 1);
    }

    #[test]
    fn bootstrap_ci_is_deterministic_and_brackets_mean() {
        let deltas = vec![1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 1.0, 1.0];
        let first = bootstrap_ci(&deltas, 1000, 20260710);
        let second = bootstrap_ci(&deltas, 1000, 20260710);
        assert_eq!(first.ci95_low, second.ci95_low);
        assert_eq!(first.ci95_high, second.ci95_high);
        assert!(first.ci95_low <= first.mean && first.mean <= first.ci95_high);
        assert!(first.ci_excludes_zero);
        let null = bootstrap_ci(&[0.0, 0.0, 1.0, -1.0], 1000, 7);
        assert!(!null.ci_excludes_zero);
    }

    #[test]
    fn lane_default_granularity_is_session_and_old_reports_parse_as_session() {
        // Back to "session" as of the 2026-07-10 rung 4 promotion: the lane
        // measures the product path (session ingestion + service-side runtime
        // chunks). The earlier same-day "turns" promotion is superseded by the
        // runtime embodiment (ΔQA +0.000 ns tie, no client-side windowing).
        assert_eq!(DEFAULT_GRANULARITY, "session");
        // The serde parsing default is ALSO "session" here, but for an
        // independent reason: reports written before the granularity field
        // existed were session runs. It must never merely track the lane
        // default.
        assert_eq!(default_granularity(), "session");
    }

    #[test]
    fn session_bodies_windows_turns_and_keeps_session_prefix() {
        let turns: Vec<LmeTurn> = (0..9)
            .map(|index| LmeTurn {
                role: if index % 2 == 0 { "user" } else { "assistant" }.to_string(),
                content: format!("turn {index}"),
            })
            .collect();
        let session = session_bodies("session", DEFAULT_TURNS_WINDOW, "s1", "2023/05/30", &turns);
        assert_eq!(session.len(), 1);
        assert!(session[0].starts_with("[session s1] [date 2023/05/30]\n"));

        let windows = session_bodies("turns", DEFAULT_TURNS_WINDOW, "s1", "2023/05/30", &turns);
        // 9 turns at window 4 -> 4 + 4 + 1.
        assert_eq!(windows.len(), 3);
        assert!(windows[0].starts_with("[session s1] [date 2023/05/30] [turns 1-4]\n"));
        assert!(windows[1].starts_with("[session s1] [date 2023/05/30] [turns 5-8]\n"));
        assert!(windows[2].starts_with("[session s1] [date 2023/05/30] [turns 9-9]\n"));
        assert!(windows[2].contains("user: turn 8"));
        // Every turn appears exactly once across windows.
        let joined = windows.join("");
        for index in 0..9 {
            assert_eq!(joined.matches(&format!("turn {index}\n")).count(), 1);
        }
        // Empty sessions still produce one (header-only) episode body.
        assert_eq!(
            session_bodies("turns", DEFAULT_TURNS_WINDOW, "s2", "2023/06/01", &[]).len(),
            1
        );
    }

    #[test]
    fn session_bodies_windows_turns_with_custom_window_size() {
        // Mirrors `session_bodies_windows_turns_and_keeps_session_prefix` but
        // pins a non-default `turns_window` (2) to prove the window size is a
        // real parameter, not a re-read of `DEFAULT_TURNS_WINDOW`.
        let turns: Vec<LmeTurn> = (0..9)
            .map(|index| LmeTurn {
                role: if index % 2 == 0 { "user" } else { "assistant" }.to_string(),
                content: format!("turn {index}"),
            })
            .collect();
        let windows = session_bodies("turns", 2, "s1", "2023/05/30", &turns);
        // 9 turns at window 2 -> 2 + 2 + 2 + 2 + 1.
        assert_eq!(windows.len(), 5);
        assert!(windows[0].starts_with("[session s1] [date 2023/05/30] [turns 1-2]\n"));
        assert!(windows[1].starts_with("[session s1] [date 2023/05/30] [turns 3-4]\n"));
        assert!(windows[2].starts_with("[session s1] [date 2023/05/30] [turns 5-6]\n"));
        assert!(windows[3].starts_with("[session s1] [date 2023/05/30] [turns 7-8]\n"));
        assert!(windows[4].starts_with("[session s1] [date 2023/05/30] [turns 9-9]\n"));
        assert!(windows[4].contains("user: turn 8"));
        // Every turn appears exactly once across windows.
        let joined = windows.join("");
        for index in 0..9 {
            assert_eq!(joined.matches(&format!("turn {index}\n")).count(), 1);
        }
    }

    #[test]
    fn turn_window_and_budget_tokens_defaults_are_pinned() {
        assert_eq!(DEFAULT_TURNS_WINDOW, 4);
        assert_eq!(DEFAULT_BUDGET_TOKENS, 8192);
        // W3: the candidate-pool default is the historical vector KNN fan-out.
        assert_eq!(DEFAULT_CANDIDATE_POOL_SIZE, 32);
    }

    #[test]
    fn pre_flag_report_json_parses_turns_window_and_budget_tokens_as_defaults() {
        // A report written before `turns_window`/`budget_tokens` existed
        // (also missing `granularity`, for the same reason) must still
        // parse, with both new fields defaulting to the values every such
        // report actually used: window 4, budget 8192.
        let json = r#"{
            "benchmark": "longmemeval_retrieval_only",
            "dataset_path": "data.json",
            "dataset_sha256": "abc123",
            "dataset_questions": 10,
            "sample_seed": 20260710,
            "sample_n": 1,
            "k": 10,
            "runtime": "postgres",
            "retrieval_only": true,
            "embeddings": "noop",
            "ingestion": "one episode per haystack session",
            "reflect": "MemoryService::reflect",
            "mode": "fast",
            "disabled": null,
            "command": "bench-lme --sample 1 --seed 20260710",
            "generated_at_unix": 0,
            "overall": {
                "question_type": "overall",
                "n": 0,
                "n_scored": 0,
                "recall_at_5": null,
                "recall_at_10": null,
                "abstention_n": 0,
                "abstention_correct": 0
            },
            "strata": [],
            "per_question": [],
            "paired_vs_baseline": null
        }"#;
        let report: BenchLmeReport = serde_json::from_str(json).expect("pre-flag report parses");
        assert_eq!(report.granularity, "session");
        assert_eq!(report.turns_window, 4);
        assert_eq!(report.budget_tokens, 8192);
        assert_eq!(report.turns_window, DEFAULT_TURNS_WINDOW);
        assert_eq!(report.budget_tokens, DEFAULT_BUDGET_TOKENS);
        // W3: a report written before `candidate_pool_size` existed must parse
        // with the field defaulting to the vector KNN fan-out (32) every such
        // report actually ran.
        assert_eq!(report.candidate_pool_size, 32);
        assert_eq!(report.candidate_pool_size, DEFAULT_CANDIDATE_POOL_SIZE);
        // The runtime_chunks report field serde default STAYS false even after
        // the write path was promoted to default-on: every pre-promotion report
        // was a chunks-off run, so an absent field must parse chunks-OFF and
        // never follow the default-on lane behavior.
        assert!(
            !report.runtime_chunks,
            "absent runtime_chunks must parse false (pre-promotion runs were chunks-off)"
        );
        // W4: a report written before the packing levers existed must parse with
        // both off — sibling_gather false, session_quota absent (None) — since
        // every such report ran today's unrestricted packing.
        assert!(
            !report.sibling_gather,
            "absent sibling_gather must parse false (pre-lever runs were sibling-gather-off)"
        );
        assert_eq!(
            report.session_quota, None,
            "absent session_quota must parse None (pre-lever runs had no quota)"
        );
        // W5: a report written before the temporal-grounding flag existed must
        // parse with it off — every such report ran without content-date
        // grounding, windowing, or dated packs.
        assert!(
            !report.temporal_grounding,
            "absent temporal_grounding must parse false (pre-flag runs were grounding-off)"
        );
    }

    #[test]
    fn stratified_sample_is_deterministic_and_proportional() {
        let mut questions = Vec::new();
        for index in 0..60 {
            questions.push((format!("a{index:03}"), "multi-session".to_string()));
        }
        for index in 0..30 {
            questions.push((format!("b{index:03}"), "knowledge-update".to_string()));
        }
        for index in 0..10 {
            questions.push((
                format!("c{index:03}"),
                "single-session-preference".to_string(),
            ));
        }
        let first = stratified_sample_ids(&questions, 10, 20260710);
        let second = stratified_sample_ids(&questions, 10, 20260710);
        assert_eq!(first, second);
        assert_eq!(first.len(), 10);
        let count = |prefix: &str| first.iter().filter(|id| id.starts_with(prefix)).count();
        assert_eq!(count("a"), 6);
        assert_eq!(count("b"), 3);
        assert_eq!(count("c"), 1);
        let other_seed = stratified_sample_ids(&questions, 10, 1);
        assert_ne!(first, other_seed);
    }
}
