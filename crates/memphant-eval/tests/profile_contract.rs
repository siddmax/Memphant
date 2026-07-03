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

#[test]
fn rung5_profile_archives_temporal_validity_promotion() {
    let archive_dir = tempfile::tempdir().expect("tempdir");
    let archive_path = archive_dir.path().join("rung5-profile.json");
    let report = run_profile_file(
        &repo_root().join("examples/evals/rung5-temporal-validity-profile.yaml"),
        "rungs-0-4-baseline",
        Some(archive_path.clone()),
    )
    .expect("rung5 profile should pass");

    let decision = report
        .rung_decisions
        .iter()
        .find(|decision| decision.rung == 5)
        .expect("rung 5 decision");
    assert_eq!(decision.item, "temporal validity");
    assert_eq!(decision.status, "promoted");
    assert_eq!(decision.axes, ["outcome", "interactive"]);
    assert!(decision.delta_vs_baseline > 0.0);
    assert!(decision.ci[0] > 0.0);

    let archived: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&archive_path).expect("read archive"))
            .expect("archive json");
    assert_eq!(archived["rung_decisions"][0]["rung"], 5);
}

#[test]
fn rung5_promotion_requires_state_style_axis() {
    let source =
        fs::read_to_string(repo_root().join("examples/evals/rung5-temporal-validity-profile.yaml"))
            .expect("read fixture");
    let bad = source.replace("axes: [outcome, interactive]", "axes: [outcome]");
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad-rung5-profile.yaml");
    fs::write(&path, bad).expect("write fixture");

    let error = run_profile_file(&path, "rungs-0-4-baseline", None)
        .expect_err("rung5 promotion without STATE-style axis should fail");

    assert!(
        error
            .to_string()
            .contains("rung_decision:5:missing_interactive_axis")
    );
}

#[test]
fn rung6_profile_archives_edge_expansion_promotion_with_controls() {
    let archive_dir = tempfile::tempdir().expect("tempdir");
    let archive_path = archive_dir.path().join("rung6-profile.json");
    let report = run_profile_file(
        &repo_root().join("examples/evals/rung6-edge-expansion-profile.yaml"),
        "rungs-0-5-baseline",
        Some(archive_path.clone()),
    )
    .expect("rung6 profile should pass");

    let decision = report
        .rung_decisions
        .iter()
        .find(|decision| decision.rung == 6)
        .expect("rung 6 decision");
    assert_eq!(decision.item, "edge expansion");
    assert_eq!(decision.status, "promoted");
    assert_eq!(decision.axes, ["outcome", "long_horizon", "interactive"]);
    assert!(decision.delta_vs_baseline >= 0.03);
    assert!(decision.ci[0] > 0.0);
    assert!(
        decision
            .benchmark_sample_refs
            .iter()
            .any(|sample| sample.contains("no-edges"))
    );
    assert!(
        decision
            .benchmark_sample_refs
            .iter()
            .any(|sample| sample.contains("filesystem-control"))
    );

    let archived: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&archive_path).expect("read archive"))
            .expect("archive json");
    assert_eq!(archived["rung_decisions"][0]["rung"], 6);
}

#[test]
fn rung6_promotion_requires_no_edges_and_filesystem_controls() {
    let source =
        fs::read_to_string(repo_root().join("examples/evals/rung6-edge-expansion-profile.yaml"))
            .expect("read fixture");
    let bad = source
        .replace(
            "      - no-edges:benchmarks/rung6-no-edges-sampled.yaml\n",
            "",
        )
        .replace(
            "      - filesystem-control:benchmarks/rung6-filesystem-control-sampled.yaml\n",
            "",
        );
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad-rung6-profile.yaml");
    fs::write(&path, bad).expect("write fixture");

    let error = run_profile_file(&path, "rungs-0-5-baseline", None)
        .expect_err("rung6 promotion without controls should fail");

    let text = error.to_string();
    assert!(text.contains("rung_decision:6:missing_no_edges_control"));
    assert!(text.contains("rung_decision:6:missing_filesystem_control"));
}

#[test]
fn rung7_profile_archives_packing_abstention_promotion() {
    let archive_dir = tempfile::tempdir().expect("tempdir");
    let archive_path = archive_dir.path().join("rung7-profile.json");
    let report = run_profile_file(
        &repo_root().join("examples/evals/rung7-packing-abstention-profile.yaml"),
        "rungs-0-6-baseline",
        Some(archive_path.clone()),
    )
    .expect("rung7 profile should pass");

    let decision = report
        .rung_decisions
        .iter()
        .find(|decision| decision.rung == 7)
        .expect("rung 7 decision");
    assert_eq!(decision.item, "packing+abstention");
    assert_eq!(decision.status, "promoted");
    assert_eq!(decision.axes, ["outcome", "restraint"]);
    assert!(decision.delta_vs_baseline > 0.0);
    assert!(decision.ci[0] > 0.0);
    assert!(
        decision
            .benchmark_sample_refs
            .iter()
            .any(|sample| sample.contains("packing_abstention_buried_deploy"))
    );
    assert!(
        decision
            .benchmark_sample_refs
            .iter()
            .any(|sample| sample.contains("packing_abstention_contradiction"))
    );

    let archived: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&archive_path).expect("read archive"))
            .expect("archive json");
    assert_eq!(archived["rung_decisions"][0]["rung"], 7);
}

#[test]
fn rung7_promotion_requires_packing_and_abstention_samples() {
    let source = fs::read_to_string(
        repo_root().join("examples/evals/rung7-packing-abstention-profile.yaml"),
    )
    .expect("read fixture");
    let bad = source
        .replace(
            "      - memphant:examples/evals/golden/packing_abstention_buried_deploy.yaml\n",
            "",
        )
        .replace(
            "      - memphant:examples/evals/golden/packing_abstention_contradiction.yaml\n",
            "",
        );
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("bad-rung7-profile.yaml");
    fs::write(&path, bad).expect("write fixture");

    let error = run_profile_file(&path, "rungs-0-6-baseline", None)
        .expect_err("rung7 promotion without samples should fail");

    let text = error.to_string();
    assert!(text.contains("rung_decision:7:missing_packing_sample"));
    assert!(text.contains("rung_decision:7:missing_abstention_sample"));
}
