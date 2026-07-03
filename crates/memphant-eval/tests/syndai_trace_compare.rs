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

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
