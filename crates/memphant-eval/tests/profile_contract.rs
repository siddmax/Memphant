use std::fs;
use std::path::Path;

use memphant_eval::run_profile_file;

fn repo_root() -> &'static Path {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("workspace root")
}

#[test]
fn wsi_profile_archives_dormant_advanced_lever_decisions() {
    let archive_dir = tempfile::tempdir().expect("tempdir");
    let archive_path = archive_dir.path().join("wsi-profile.json");
    let report = run_profile_file(
        &repo_root().join("examples/evals/wsi-profile.yaml"),
        "rungs-0-3-baseline",
        Some(archive_path.clone()),
    )
    .expect("profile should pass");

    assert!(report.activated_levers.is_empty());
    assert!(
        report
            .dormant_levers
            .iter()
            .any(|item| item == "L4 exhaustive recall behavior")
    );
    assert_eq!(report.archived_path.as_ref(), Some(&archive_path));
    assert!(archive_path.is_file());
    let archived: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&archive_path).expect("read archive"))
            .expect("archive json");
    assert_eq!(
        archived["archived_path"],
        archive_path.display().to_string()
    );
}

#[test]
fn activated_profile_decision_requires_after_trace_evidence() {
    let source = fs::read_to_string(repo_root().join("examples/evals/wsi-profile.yaml"))
        .expect("read fixture");
    let bad = source.replacen(
        "status: dormant\n    gate_met: false",
        "status: activated\n    gate_met: true",
        1,
    );
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad-profile.yaml");
    fs::write(&path, bad).expect("write fixture");

    let error = run_profile_file(&path, "rungs-0-3-baseline", None)
        .expect_err("activated lever without after trace should fail");

    assert!(error.to_string().contains("missing_after_trace"));
}

#[test]
fn rung4_profile_archives_contextual_chunk_promotion_with_public_sampled_axes() {
    let archive_dir = tempfile::tempdir().expect("tempdir");
    let archive_path = archive_dir.path().join("rung4-profile.json");
    let report = run_profile_file(
        &repo_root().join("examples/evals/rung4-contextual-chunks-profile.yaml"),
        "rungs-0-3-baseline",
        Some(archive_path.clone()),
    )
    .expect("rung4 profile should pass");

    let decision = report
        .rung_decisions
        .iter()
        .find(|decision| decision.rung == 4)
        .expect("rung 4 decision");
    assert_eq!(decision.item, "contextual chunks");
    assert_eq!(decision.status, "promoted");
    assert!(decision.gate_met);
    assert_eq!(decision.axes, ["long_horizon", "scale"]);
    assert!(decision.delta_vs_baseline > 0.0);
    assert!(decision.ci[0] > 0.0);
    assert!(decision.p95_ms > 0.0);
    assert!(decision.cost_per_1k_recalls_usd >= 0.0);

    let archived: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&archive_path).expect("read archive"))
            .expect("archive json");
    assert_eq!(archived["rung_decisions"][0]["rung"], 4);
    assert_eq!(
        archived["rung_decisions"][0]["benchmark_sample_refs"][0],
        "hf:xiaowu0162/longmemeval-v2@2026-05-17/questions.jsonl"
    );
}

#[test]
fn rung4_promotion_requires_lme_and_beam_axes() {
    let source =
        fs::read_to_string(repo_root().join("examples/evals/rung4-contextual-chunks-profile.yaml"))
            .expect("read fixture");
    let bad = source.replace("axes: [long_horizon, scale]", "axes: [long_horizon]");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad-rung4-profile.yaml");
    fs::write(&path, bad).expect("write fixture");

    let error = run_profile_file(&path, "rungs-0-3-baseline", None)
        .expect_err("rung4 promotion without BEAM axis should fail");

    assert!(
        error
            .to_string()
            .contains("rung_decision:4:missing_scale_axis")
    );
}
