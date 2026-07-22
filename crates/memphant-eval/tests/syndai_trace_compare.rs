use std::fs;
use std::path::{Path, PathBuf};

use memphant_eval::{EvalRunOptions, run_syndai_trace_compare_file};

#[test]
fn syndai_file_memory_trace_compare_passes_and_archives() {
    let temp = tempfile::tempdir().unwrap();
    let report = run_syndai_trace_compare_file(
        &repo_root().join("examples/syndai/file-memory-trace-compare.yaml"),
        EvalRunOptions {
            archive_traces: true,
            archive_dir: Some(temp.path().to_path_buf()),
            ..EvalRunOptions::default()
        },
    )
    .expect("trace compare");

    assert!(report.passed);
    assert_eq!(report.surface, "agent_file_memory");
    assert_eq!(report.answer_bearing_recall, 1.0);
    assert!(report.forbidden_returned.is_empty());
    assert!(report.trace_id.is_some());

    let archive = report.archived_trace_path.expect("archive path");
    let archive_json: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(archive).unwrap()).unwrap();
    assert_eq!(archive_json["surface"], "agent_file_memory");
    assert_eq!(archive_json["case_id"], "syndai_agent_file_memory_001");
}

#[test]
fn syndai_coding_continuity_fixture_families_pass() {
    // The four spec-28 coding-continuity families (28-syndai-code-contract §4),
    // executable through the same syndai-trace-compare lane as the file-memory
    // surface: each fixture wraps a golden case and must pass end-to-end.
    for (file, id) in [
        (
            "arch-decision-honored-trace-compare.yaml",
            "syndai_arch_decision_honored_001",
        ),
        (
            "compaction-rehydrate-trace-compare.yaml",
            "syndai_compaction_rehydrate_001",
        ),
        (
            "cross-agent-transfer-trace-compare.yaml",
            "syndai_cross_agent_transfer_001",
        ),
        (
            "task-plus-semantic-composite-trace-compare.yaml",
            "syndai_task_plus_semantic_composite_001",
        ),
    ] {
        let report = run_syndai_trace_compare_file(
            &repo_root().join("examples/syndai").join(file),
            EvalRunOptions::default(),
        )
        .unwrap_or_else(|error| panic!("{file}: {error}"));

        assert!(
            report.passed,
            "{file}: missing={:?} forbidden={:?} other={:?}",
            report.missing_answer_bearing, report.forbidden_returned, report.other_mismatches
        );
        assert_eq!(report.id, id);
        assert_eq!(report.surface, "coding_continuity");
        assert_eq!(report.answer_bearing_recall, 1.0);
        assert!(report.trace_id.is_some());
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
