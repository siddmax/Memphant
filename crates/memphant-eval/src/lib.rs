use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};

use memphant_core::{InMemoryStore, MemoryStore, forget_memory, recall};
use memphant_types::{
    ActorId, ENGINE_VERSION, ForgetRequest, ForgetSelector, MemoryEdgeKind, MemoryKind, NewEpisode,
    NewMemoryEdge, NewMemoryUnit, RecallDropReason, RecallMode, RecallRequest, ScopeId,
    StoredMemoryUnit, TRACE_SCHEMA_VERSION, TenantId, TrustLevel, UnitId, UnitState,
};
use schemars::schema_for;
use serde::{Deserialize, Serialize};

pub const EVAL_RUNNER_NAME: &str = "memphant-eval";

#[derive(Debug, Clone, Default)]
pub struct EvalRunOptions {
    pub archive_traces: bool,
    pub archive_dir: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvalReport {
    pub eval_id: String,
    pub total_cases: usize,
    pub passed_cases: usize,
    pub case_results: Vec<EvalCaseResult>,
    pub archived_trace_path: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct EvalCaseResult {
    pub id: String,
    pub passed: bool,
    pub trace_id: Option<String>,
    pub missing_units: Vec<String>,
    pub forbidden_present: Vec<String>,
    pub missing_citations: Vec<String>,
    pub missing_trace_stages: Vec<String>,
    pub dropped_mismatches: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoldenVerifyReport {
    pub verified_cases: usize,
    pub case_results: Vec<GoldenVerifyCaseResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GoldenVerifyCaseResult {
    pub id: String,
    pub load_bearing: bool,
    pub second_author_confirmed: bool,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ManifestReport {
    pub categories: BTreeMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityReport {
    pub id: String,
    pub passed: bool,
    pub covered_lanes: Vec<String>,
    pub lane_results: Vec<SecurityLaneResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SecurityLaneResult {
    pub id: String,
    pub kind: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpsReport {
    pub id: String,
    pub passed: bool,
    pub covered_checks: Vec<String>,
    pub check_results: Vec<OpsCheckResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OpsCheckResult {
    pub id: String,
    pub kind: String,
    pub passed: bool,
    pub detail: String,
}

#[derive(Debug, thiserror::Error)]
pub enum EvalError {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("YAML parse error at {path}: {source}")]
    Yaml {
        path: PathBuf,
        source: yaml_serde::Error,
    },
    #[error("JSON parse error at {path}: {source}")]
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("manifest mismatch: {0}")]
    Manifest(String),
    #[error("eval failed: {0}")]
    Failed(String),
    #[error("core error: {0}")]
    Core(String),
}

pub type EvalResult<T> = Result<T, EvalError>;

#[derive(Debug, Deserialize)]
struct EvalSuite {
    id: String,
    manifest: Option<PathBuf>,
    cases: Vec<PathBuf>,
}

#[derive(Debug, Clone, Deserialize)]
struct GoldenCase {
    id: String,
    #[serde(default)]
    second_author_confirmed: bool,
    query: String,
    #[serde(default)]
    k: Option<usize>,
    #[serde(default)]
    budget_tokens: Option<usize>,
    #[serde(default)]
    include_beliefs: bool,
    seed: GoldenSeed,
    expect: GoldenExpect,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct GoldenSeed {
    #[serde(default)]
    units: Vec<GoldenUnit>,
    #[serde(default)]
    edges: Vec<GoldenEdge>,
}

#[derive(Debug, Clone, Deserialize)]
struct GoldenUnit {
    name: String,
    #[serde(default = "primary_name")]
    tenant: String,
    #[serde(default = "primary_name")]
    scope: String,
    episode_body: String,
    kind: MemoryKind,
    state: UnitState,
    subject_key: Option<String>,
    body: String,
    trust_level: TrustLevel,
    #[serde(default)]
    deletion_generation: Option<u64>,
}

#[derive(Debug, Clone, Deserialize)]
struct GoldenEdge {
    src: String,
    dst: String,
    kind: MemoryEdgeKind,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct GoldenExpect {
    #[serde(default)]
    answer_bearing_ids: Vec<String>,
    #[serde(default)]
    top_k_contains: Vec<String>,
    #[serde(default)]
    forbidden_units: Vec<String>,
    #[serde(default)]
    citations_include: Vec<String>,
    #[serde(default)]
    trace_stages_include: Vec<String>,
    #[serde(default)]
    dropped: Vec<GoldenDropped>,
    #[serde(default)]
    high_risk_suppressed: Vec<String>,
    #[serde(default)]
    invalidated_units: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct GoldenDropped {
    unit: String,
    reason: RecallDropReason,
}

#[derive(Debug, Deserialize)]
struct SecuritySuite {
    id: String,
    lanes: Vec<SecurityLane>,
}

#[derive(Debug, Deserialize)]
struct SecurityLane {
    id: String,
    kind: String,
    #[serde(default)]
    query: String,
    #[serde(default)]
    raw_selector: Option<String>,
    #[serde(default)]
    expect_rejected: bool,
    #[serde(default)]
    high_risk_action: Option<String>,
    #[serde(default)]
    seed: GoldenSeed,
    #[serde(default)]
    forget: Option<ForgetFixture>,
    #[serde(default)]
    expect: GoldenExpect,
}

#[derive(Debug, Deserialize)]
struct ForgetFixture {
    unit: String,
    reason: String,
}

#[derive(Debug, Deserialize)]
struct OpsSuite {
    id: String,
    checks: Vec<OpsCheck>,
}

#[derive(Debug, Deserialize)]
struct OpsCheck {
    id: String,
    kind: String,
    #[serde(default)]
    min_age_seconds: Option<u64>,
    #[serde(default)]
    live_blob_hashes: Vec<String>,
    #[serde(default)]
    tombstoned_blob_hashes: Vec<String>,
    #[serde(default)]
    ledger_blob_hashes: Vec<String>,
    #[serde(default)]
    expect_collect: Vec<String>,
    #[serde(default)]
    deletion_generation_bumped: bool,
    #[serde(default)]
    readback_paths: BTreeMap<String, String>,
    #[serde(default)]
    dead_ratio: Option<f64>,
    #[serde(default)]
    threshold: Option<f64>,
    #[serde(default)]
    tombstone_age_hours: Option<u64>,
    #[serde(default)]
    max_tombstone_age_hours: Option<u64>,
    #[serde(default)]
    expect_reindex_required: Option<bool>,
}

#[derive(Clone)]
struct SeedContext {
    store: InMemoryStore,
    tenant_id: TenantId,
    scope_id: ScopeId,
    actor_id: ActorId,
    named_units: HashMap<String, UnitId>,
    unit_records: HashMap<String, StoredMemoryUnit>,
}

pub fn run_eval_file(path: &Path, options: EvalRunOptions) -> EvalResult<EvalReport> {
    let suite: EvalSuite = read_yaml(path)?;
    let base = path.parent().unwrap_or_else(|| Path::new("."));
    if let Some(manifest) = suite.manifest.as_ref() {
        validate_manifest(&base.join(manifest), &base.join("golden"))?;
    }

    let runtime = runtime()?;
    let mut case_results = Vec::new();
    for case_path in &suite.cases {
        let case: GoldenCase = read_yaml(&base.join(case_path))?;
        let result = runtime.block_on(run_golden_case(&case, &BTreeSet::new()));
        case_results.push(result);
    }

    let passed_cases = case_results.iter().filter(|case| case.passed).count();
    let mut report = EvalReport {
        eval_id: suite.id,
        total_cases: case_results.len(),
        passed_cases,
        case_results,
        archived_trace_path: None,
    };

    if options.archive_traces {
        let archive_dir = options
            .archive_dir
            .unwrap_or_else(|| PathBuf::from("docs/build-log/artifacts"));
        fs::create_dir_all(&archive_dir).map_err(|source| EvalError::Io {
            path: archive_dir.clone(),
            source,
        })?;
        let archive_path = archive_dir.join(format!("{}-traces.json", report.eval_id));
        let archive = serde_json::json!({
            "eval_id": report.eval_id,
            "runner": EVAL_RUNNER_NAME,
            "trace_schema_version": TRACE_SCHEMA_VERSION,
            "metrics": {
                "total_cases": report.total_cases,
                "passed_cases": report.passed_cases,
            },
            "case_results": report.case_results,
        });
        write_json(&archive_path, &archive)?;
        report.archived_trace_path = Some(archive_path);
    }

    Ok(report)
}

pub fn verify_golden_file(path: &Path) -> EvalResult<GoldenVerifyReport> {
    let suite_path = if path.is_dir() {
        path.join("golden.yaml")
    } else {
        path.to_path_buf()
    };
    let suite: EvalSuite = read_yaml(&suite_path)?;
    let base = suite_path.parent().unwrap_or_else(|| Path::new("."));
    let runtime = runtime()?;
    let mut case_results = Vec::new();

    for case_path in &suite.cases {
        let case: GoldenCase = read_yaml(&base.join(case_path))?;
        let answer_bearing: BTreeSet<_> = case.expect.answer_bearing_ids.iter().cloned().collect();
        let normal = runtime.block_on(run_golden_case(&case, &BTreeSet::new()));
        let masked = runtime.block_on(run_golden_case(&case, &answer_bearing));
        let load_bearing = normal.passed
            && !masked.passed
            && case.second_author_confirmed
            && case
                .expect
                .top_k_contains
                .iter()
                .all(|unit| answer_bearing.contains(unit));
        let reason = if load_bearing {
            "masked answer-bearing units break assertions".to_string()
        } else if !case.second_author_confirmed {
            "second author confirmation is missing".to_string()
        } else if !normal.passed {
            "unmasked case does not pass".to_string()
        } else {
            "masked run still satisfies assertions or top_k lacks answer-bearing ids".to_string()
        };
        case_results.push(GoldenVerifyCaseResult {
            id: case.id,
            load_bearing,
            second_author_confirmed: case.second_author_confirmed,
            reason,
        });
    }

    Ok(GoldenVerifyReport {
        verified_cases: case_results.len(),
        case_results,
    })
}

pub fn validate_manifest(manifest_path: &Path, lane_root: &Path) -> EvalResult<ManifestReport> {
    let categories: BTreeMap<String, Vec<String>> = read_yaml(manifest_path)?;
    let declared: BTreeSet<_> = categories
        .values()
        .flat_map(|case_ids| case_ids.iter().cloned())
        .collect();
    let mut files = BTreeSet::new();
    for entry in fs::read_dir(lane_root).map_err(|source| EvalError::Io {
        path: lane_root.to_path_buf(),
        source,
    })? {
        let entry = entry.map_err(|source| EvalError::Io {
            path: lane_root.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        let is_yaml = path
            .extension()
            .and_then(|ext| ext.to_str())
            .is_some_and(|ext| matches!(ext, "yaml" | "yml"));
        if is_yaml && let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) {
            files.insert(stem.to_string());
        }
    }

    let missing: Vec<_> = declared.difference(&files).cloned().collect();
    let orphan: Vec<_> = files.difference(&declared).cloned().collect();
    if !missing.is_empty() || !orphan.is_empty() {
        return Err(EvalError::Manifest(format!(
            "missing={missing:?} orphan={orphan:?}"
        )));
    }
    Ok(ManifestReport { categories })
}

pub fn run_security_file(path: &Path) -> EvalResult<SecurityReport> {
    let suite: SecuritySuite = read_yaml(path)?;
    let runtime = runtime()?;
    let mut lane_results = Vec::new();
    for lane in &suite.lanes {
        let result = runtime.block_on(run_security_lane(lane));
        lane_results.push(result);
    }

    let required = required_security_lanes();
    let present: BTreeSet<_> = lane_results.iter().map(|lane| lane.kind.clone()).collect();
    let missing: Vec<_> = required
        .iter()
        .filter(|kind| !present.contains(**kind))
        .map(|kind| (*kind).to_string())
        .collect();
    if !missing.is_empty() {
        return Err(EvalError::Failed(format!(
            "missing required security lanes: {missing:?}"
        )));
    }
    let passed = lane_results.iter().all(|lane| lane.passed);
    Ok(SecurityReport {
        id: suite.id,
        passed,
        covered_lanes: required.iter().map(|lane| (*lane).to_string()).collect(),
        lane_results,
    })
}

pub fn run_ops_file(path: &Path) -> EvalResult<OpsReport> {
    let suite: OpsSuite = read_yaml(path)?;
    let mut check_results = Vec::new();
    for check in &suite.checks {
        check_results.push(run_ops_check(check));
    }
    let required = required_ops_checks();
    let present: BTreeSet<_> = check_results
        .iter()
        .map(|check| check.kind.clone())
        .collect();
    let missing: Vec<_> = required
        .iter()
        .filter(|kind| !present.contains(**kind))
        .map(|kind| (*kind).to_string())
        .collect();
    if !missing.is_empty() {
        return Err(EvalError::Failed(format!(
            "missing required ops checks: {missing:?}"
        )));
    }
    let passed = check_results.iter().all(|check| check.passed);
    Ok(OpsReport {
        id: suite.id,
        passed,
        covered_checks: required.iter().map(|check| (*check).to_string()).collect(),
        check_results,
    })
}

pub fn generate_trace_schema() -> serde_json::Value {
    serde_json::to_value(schema_for!(memphant_types::RetrievalTrace))
        .expect("RetrievalTrace schema serializes")
}

async fn run_golden_case(case: &GoldenCase, masked_units: &BTreeSet<String>) -> EvalCaseResult {
    match run_golden_case_inner(case, masked_units).await {
        Ok(result) => result,
        Err(error) => EvalCaseResult {
            id: case.id.clone(),
            passed: false,
            trace_id: None,
            missing_units: Vec::new(),
            forbidden_present: Vec::new(),
            missing_citations: Vec::new(),
            missing_trace_stages: Vec::new(),
            dropped_mismatches: Vec::new(),
            error: Some(error.to_string()),
        },
    }
}

async fn run_golden_case_inner(
    case: &GoldenCase,
    masked_units: &BTreeSet<String>,
) -> EvalResult<EvalCaseResult> {
    let context = seed_store(&case.seed, masked_units).await?;
    let response = recall(
        &context.store,
        RecallRequest {
            tenant_id: context.tenant_id,
            scope_id: context.scope_id,
            actor_id: context.actor_id,
            allowed_scope_ids: vec![context.scope_id],
            query: case.query.clone(),
            k: case.k.unwrap_or(8),
            budget_tokens: case.budget_tokens.unwrap_or(256),
            mode: RecallMode::Fast,
            include_beliefs: case.include_beliefs,
            engine_version: ENGINE_VERSION.to_string(),
        },
    )
    .await
    .map_err(|error| EvalError::Core(error.to_string()))?;

    let trace = context
        .store
        .trace_by_id(response.trace_id)
        .ok_or_else(|| EvalError::Failed(format!("{} missing trace", case.id)))?;
    let mut required_units = case.expect.top_k_contains.clone();
    for answer in &case.expect.answer_bearing_ids {
        if !required_units.contains(answer) {
            required_units.push(answer.clone());
        }
    }
    let missing_units = required_units
        .iter()
        .filter(|name| {
            context
                .named_units
                .get(*name)
                .is_none_or(|unit_id| !response.candidate_whitelist.contains(unit_id))
        })
        .cloned()
        .collect::<Vec<_>>();
    let forbidden_present = case
        .expect
        .forbidden_units
        .iter()
        .filter(|name| {
            context
                .named_units
                .get(*name)
                .is_some_and(|unit_id| response.candidate_whitelist.contains(unit_id))
        })
        .cloned()
        .collect::<Vec<_>>();
    let missing_citations = case
        .expect
        .citations_include
        .iter()
        .filter(|name| {
            context.named_units.get(*name).is_none_or(|unit_id| {
                !response
                    .citations
                    .iter()
                    .any(|citation| citation.unit_id == *unit_id)
            })
        })
        .cloned()
        .collect::<Vec<_>>();
    let trace_stages: BTreeSet<_> = trace
        .channel_runs
        .iter()
        .map(|stage| stage.stage.as_str())
        .collect();
    let missing_trace_stages = case
        .expect
        .trace_stages_include
        .iter()
        .filter(|stage| !trace_stages.contains(stage.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    let dropped_mismatches = case
        .expect
        .dropped
        .iter()
        .filter(|expected| {
            context
                .named_units
                .get(&expected.unit)
                .is_none_or(|unit_id| {
                    !trace
                        .dropped_items
                        .iter()
                        .any(|item| item.unit_id == *unit_id && item.reason == expected.reason)
                })
        })
        .map(|expected| format!("{}:{:?}", expected.unit, expected.reason))
        .collect::<Vec<_>>();

    let passed = missing_units.is_empty()
        && forbidden_present.is_empty()
        && missing_citations.is_empty()
        && missing_trace_stages.is_empty()
        && dropped_mismatches.is_empty();

    Ok(EvalCaseResult {
        id: case.id.clone(),
        passed,
        trace_id: Some(response.trace_id.as_uuid().to_string()),
        missing_units,
        forbidden_present,
        missing_citations,
        missing_trace_stages,
        dropped_mismatches,
        error: None,
    })
}

async fn run_security_lane(lane: &SecurityLane) -> SecurityLaneResult {
    let outcome = match lane.kind.as_str() {
        "poisoning" | "tenant_leakage" => run_fixture_security_lane(lane).await,
        "query_filter_injection" => run_selector_injection_lane(lane),
        "high_risk_action_suppression" => run_high_risk_lane(lane).await,
        "deletion_completeness" => run_deletion_lane(lane).await,
        other => Err(EvalError::Failed(format!("unknown security lane {other}"))),
    };
    match outcome {
        Ok(detail) => SecurityLaneResult {
            id: lane.id.clone(),
            kind: lane.kind.clone(),
            passed: true,
            detail,
        },
        Err(error) => SecurityLaneResult {
            id: lane.id.clone(),
            kind: lane.kind.clone(),
            passed: false,
            detail: error.to_string(),
        },
    }
}

async fn run_fixture_security_lane(lane: &SecurityLane) -> EvalResult<String> {
    let case = GoldenCase {
        id: lane.id.clone(),
        second_author_confirmed: true,
        query: lane.query.clone(),
        k: None,
        budget_tokens: None,
        include_beliefs: false,
        seed: lane.seed.clone(),
        expect: lane.expect.clone(),
    };
    let result = run_golden_case_inner(&case, &BTreeSet::new()).await?;
    if result.passed {
        Ok("fixture assertions passed".to_string())
    } else {
        Err(EvalError::Failed(format!("{result:?}")))
    }
}

fn run_selector_injection_lane(lane: &SecurityLane) -> EvalResult<String> {
    let rejected = lane
        .raw_selector
        .as_deref()
        .is_some_and(selector_requires_parameterization);
    if rejected == lane.expect_rejected {
        Ok("raw selector rejected before query construction".to_string())
    } else {
        Err(EvalError::Failed(
            "selector injection expectation mismatch".to_string(),
        ))
    }
}

async fn run_high_risk_lane(lane: &SecurityLane) -> EvalResult<String> {
    let context = seed_store(&lane.seed, &BTreeSet::new()).await?;
    let response = recall(
        &context.store,
        RecallRequest {
            tenant_id: context.tenant_id,
            scope_id: context.scope_id,
            actor_id: context.actor_id,
            allowed_scope_ids: vec![context.scope_id],
            query: lane.query.clone(),
            k: 8,
            budget_tokens: 256,
            mode: RecallMode::Fast,
            include_beliefs: true,
            engine_version: ENGINE_VERSION.to_string(),
        },
    )
    .await
    .map_err(|error| EvalError::Core(error.to_string()))?;

    for required in &lane.expect.top_k_contains {
        let Some(id) = context.named_units.get(required) else {
            return Err(EvalError::Failed(format!(
                "unknown expected unit {required}"
            )));
        };
        if !response.candidate_whitelist.contains(id) {
            return Err(EvalError::Failed(format!(
                "missing verified high-risk context {required}"
            )));
        }
    }

    for suppressed in &lane.expect.high_risk_suppressed {
        let Some(unit) = context.unit_records.get(suppressed) else {
            return Err(EvalError::Failed(format!(
                "unknown suppressed unit {suppressed}"
            )));
        };
        if high_risk_action_eligible(unit) {
            return Err(EvalError::Failed(format!(
                "low-trust unit {suppressed} remained high-risk eligible"
            )));
        }
    }
    Ok(format!(
        "{} low-trust memories suppressed for {}",
        lane.expect.high_risk_suppressed.len(),
        lane.high_risk_action
            .as_deref()
            .unwrap_or("high-risk action")
    ))
}

async fn run_deletion_lane(lane: &SecurityLane) -> EvalResult<String> {
    let context = seed_store(&lane.seed, &BTreeSet::new()).await?;
    let forget = lane
        .forget
        .as_ref()
        .ok_or_else(|| EvalError::Failed("deletion lane missing forget fixture".to_string()))?;
    let unit_id = *context
        .named_units
        .get(&forget.unit)
        .ok_or_else(|| EvalError::Failed(format!("unknown forget unit {}", forget.unit)))?;
    let result = forget_memory(
        &context.store,
        ForgetRequest {
            tenant_id: context.tenant_id,
            scope_id: context.scope_id,
            actor_id: context.actor_id,
            selector: ForgetSelector {
                memory_unit_id: Some(unit_id),
                scope_id: None,
            },
            reason: forget.reason.clone(),
        },
    )
    .await
    .map_err(|error| EvalError::Core(error.to_string()))?;

    for expected in &lane.expect.invalidated_units {
        let expected_id = context
            .named_units
            .get(expected)
            .ok_or_else(|| EvalError::Failed(format!("unknown invalidated unit {expected}")))?;
        if !result.invalidated_units.contains(expected_id) {
            return Err(EvalError::Failed(format!(
                "forget did not invalidate {expected}"
            )));
        }
    }

    let case = GoldenCase {
        id: lane.id.clone(),
        second_author_confirmed: true,
        query: lane.query.clone(),
        k: None,
        budget_tokens: None,
        include_beliefs: false,
        seed: GoldenSeed::default(),
        expect: lane.expect.clone(),
    };
    let response = recall(
        &context.store,
        RecallRequest {
            tenant_id: context.tenant_id,
            scope_id: context.scope_id,
            actor_id: context.actor_id,
            allowed_scope_ids: vec![context.scope_id],
            query: case.query,
            k: 8,
            budget_tokens: 256,
            mode: RecallMode::Fast,
            include_beliefs: false,
            engine_version: ENGINE_VERSION.to_string(),
        },
    )
    .await
    .map_err(|error| EvalError::Core(error.to_string()))?;
    for forbidden in &lane.expect.forbidden_units {
        let forbidden_id = context
            .named_units
            .get(forbidden)
            .ok_or_else(|| EvalError::Failed(format!("unknown forbidden unit {forbidden}")))?;
        if response.candidate_whitelist.contains(forbidden_id) {
            return Err(EvalError::Failed(format!(
                "deleted unit {forbidden} returned after forget"
            )));
        }
    }
    Ok(format!(
        "deletion_generation={} verification={}",
        result.deletion_generation, result.verification
    ))
}

fn run_ops_check(check: &OpsCheck) -> OpsCheckResult {
    let outcome = match check.kind.as_str() {
        "blob_gc" => run_blob_gc_check(check),
        "deletion_saga_readback" => run_deletion_saga_check(check),
        "reindex_compaction_sla" => run_reindex_sla_check(check),
        other => Err(EvalError::Failed(format!("unknown ops check {other}"))),
    };
    match outcome {
        Ok(detail) => OpsCheckResult {
            id: check.id.clone(),
            kind: check.kind.clone(),
            passed: true,
            detail,
        },
        Err(error) => OpsCheckResult {
            id: check.id.clone(),
            kind: check.kind.clone(),
            passed: false,
            detail: error.to_string(),
        },
    }
}

fn run_blob_gc_check(check: &OpsCheck) -> EvalResult<String> {
    if check.min_age_seconds.unwrap_or_default() == 0 {
        return Err(EvalError::Failed(
            "blob GC check must set a positive MIN_AGE".to_string(),
        ));
    }
    let live: BTreeSet<_> = check.live_blob_hashes.iter().cloned().collect();
    let ledger: BTreeSet<_> = check.ledger_blob_hashes.iter().cloned().collect();
    let tombstoned: BTreeSet<_> = check.tombstoned_blob_hashes.iter().cloned().collect();
    let collect: BTreeSet<_> = tombstoned
        .intersection(&ledger)
        .filter(|hash| !live.contains(*hash))
        .cloned()
        .collect();
    let expected: BTreeSet<_> = check.expect_collect.iter().cloned().collect();
    if collect == expected {
        Ok(format!("collect={collect:?}"))
    } else {
        Err(EvalError::Failed(format!(
            "blob collect mismatch expected={expected:?} actual={collect:?}"
        )))
    }
}

fn run_deletion_saga_check(check: &OpsCheck) -> EvalResult<String> {
    if !check.deletion_generation_bumped {
        return Err(EvalError::Failed(
            "deletion generation did not bump".to_string(),
        ));
    }
    let bad_paths: Vec<_> = check
        .readback_paths
        .iter()
        .filter(|(_, state)| !matches!(state.as_str(), "clear" | "gc_pending"))
        .map(|(path, state)| format!("{path}:{state}"))
        .collect();
    if bad_paths.is_empty() {
        Ok("all deletion saga read-backs are clear or GC-pending".to_string())
    } else {
        Err(EvalError::Failed(format!(
            "uncleared deletion paths: {bad_paths:?}"
        )))
    }
}

fn run_reindex_sla_check(check: &OpsCheck) -> EvalResult<String> {
    let dead_ratio = check
        .dead_ratio
        .ok_or_else(|| EvalError::Failed("dead_ratio missing".to_string()))?;
    let threshold = check
        .threshold
        .ok_or_else(|| EvalError::Failed("threshold missing".to_string()))?;
    let age = check.tombstone_age_hours.unwrap_or_default();
    let max_age = check.max_tombstone_age_hours.unwrap_or(u64::MAX);
    let required = dead_ratio >= threshold || age >= max_age;
    if Some(required) == check.expect_reindex_required {
        Ok(format!(
            "dead_ratio={dead_ratio} threshold={threshold} tombstone_age_hours={age}"
        ))
    } else {
        Err(EvalError::Failed(format!(
            "reindex expectation mismatch expected={:?} actual={required}",
            check.expect_reindex_required
        )))
    }
}

async fn seed_store(seed: &GoldenSeed, masked_units: &BTreeSet<String>) -> EvalResult<SeedContext> {
    let store = InMemoryStore::default();
    let tenant_id = TenantId::from_u128(90_000);
    let other_tenant_id = TenantId::from_u128(90_001);
    let scope_id = ScopeId::from_u128(90_010);
    let denied_scope_id = ScopeId::from_u128(90_011);
    let actor_id = ActorId::from_u128(90_020);
    let mut named_units = HashMap::new();
    let mut unit_records = HashMap::new();

    let mut tx = store.begin().await;
    for unit in &seed.units {
        if masked_units.contains(&unit.name) {
            continue;
        }
        let unit_tenant_id = if unit.tenant == "other" {
            other_tenant_id
        } else {
            tenant_id
        };
        let unit_scope_id = if unit.scope == "denied" {
            denied_scope_id
        } else {
            scope_id
        };
        let episode = store
            .stage_episode(
                &mut tx,
                NewEpisode {
                    tenant_id: unit_tenant_id,
                    scope_id: unit_scope_id,
                    actor_id,
                    source_kind: "fixture".to_string(),
                    source_trust: unit.trust_level,
                    dedup_key: format!("{}:{}", unit.name, unit.episode_body),
                    body: unit.episode_body.clone(),
                },
            )
            .await
            .map_err(|error| EvalError::Core(error.to_string()))?;
        let unit_id = store
            .stage_memory_unit(
                &mut tx,
                NewMemoryUnit {
                    tenant_id: unit_tenant_id,
                    scope_id: unit_scope_id,
                    kind: unit.kind,
                    state: unit.state,
                    subject_key: unit.subject_key.clone(),
                    body: unit.body.clone(),
                    trust_level: unit.trust_level,
                    churn_class: None,
                    freshness_due: false,
                    actor_id: Some(actor_id),
                    source_kind: Some("fixture".to_string()),
                    source_episode_id: Some(episode.episode_id),
                    source_resource_id: None,
                    deletion_generation: unit.deletion_generation,
                },
            )
            .await
            .map_err(|error| EvalError::Core(error.to_string()))?;
        named_units.insert(unit.name.clone(), unit_id);
    }
    for edge in &seed.edges {
        if masked_units.contains(&edge.src) || masked_units.contains(&edge.dst) {
            continue;
        }
        let src_id = *named_units
            .get(&edge.src)
            .ok_or_else(|| EvalError::Failed(format!("unknown edge src {}", edge.src)))?;
        let dst_id = *named_units
            .get(&edge.dst)
            .ok_or_else(|| EvalError::Failed(format!("unknown edge dst {}", edge.dst)))?;
        store
            .stage_memory_edge(
                &mut tx,
                NewMemoryEdge {
                    tenant_id,
                    scope_id,
                    src_id,
                    dst_id,
                    kind: edge.kind,
                },
            )
            .await
            .map_err(|error| EvalError::Core(error.to_string()))?;
    }
    store
        .commit(tx)
        .await
        .map_err(|error| EvalError::Core(error.to_string()))?;

    for (name, id) in &named_units {
        if let Some(unit) = store
            .memory_units(tenant_id)
            .into_iter()
            .chain(store.memory_units(other_tenant_id))
            .find(|unit| unit.id == *id)
        {
            unit_records.insert(name.clone(), unit);
        }
    }

    Ok(SeedContext {
        store,
        tenant_id,
        scope_id,
        actor_id,
        named_units,
        unit_records,
    })
}

fn selector_requires_parameterization(selector: &str) -> bool {
    let normalized = selector.to_ascii_lowercase();
    normalized.contains(" or ")
        || normalized.contains("--")
        || normalized.contains(';')
        || normalized.contains("/*")
        || normalized.contains("*/")
        || normalized.contains('=')
}

fn high_risk_action_eligible(unit: &StoredMemoryUnit) -> bool {
    matches!(
        unit.trust_level,
        TrustLevel::TrustedUser | TrustLevel::TrustedSystem | TrustLevel::VerifiedTool
    ) && unit.kind != MemoryKind::Belief
        && matches!(unit.state, UnitState::Active | UnitState::Validated)
        && unit.deletion_generation.is_none()
}

fn required_security_lanes() -> [&'static str; 5] {
    [
        "poisoning",
        "query_filter_injection",
        "high_risk_action_suppression",
        "tenant_leakage",
        "deletion_completeness",
    ]
}

fn required_ops_checks() -> [&'static str; 3] {
    [
        "blob_gc",
        "deletion_saga_readback",
        "reindex_compaction_sla",
    ]
}

fn primary_name() -> String {
    "primary".to_string()
}

fn read_yaml<T>(path: &Path) -> EvalResult<T>
where
    T: for<'de> Deserialize<'de>,
{
    let content = fs::read_to_string(path).map_err(|source| EvalError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    yaml_serde::from_str(&content).map_err(|source| EvalError::Yaml {
        path: path.to_path_buf(),
        source,
    })
}

fn write_json<T>(path: &Path, value: &T) -> EvalResult<()>
where
    T: Serialize,
{
    let json = serde_json::to_vec_pretty(value).map_err(|source| EvalError::Json {
        path: path.to_path_buf(),
        source,
    })?;
    fs::write(path, [json, b"\n".to_vec()].concat()).map_err(|source| EvalError::Io {
        path: path.to_path_buf(),
        source,
    })
}

fn runtime() -> EvalResult<tokio::runtime::Runtime> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|source| EvalError::Failed(format!("failed to create runtime: {source}")))
}
