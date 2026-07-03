use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

#[test]
fn compile_exports_markdown_and_verify_detects_stale_export() {
    let root = repo_root();
    let out = tempfile::tempdir().unwrap();
    let source = root.join("examples/evals/compiled-memory-source.json");

    let compile = Command::new(env!("CARGO_BIN_EXE_memphant-cli"))
        .current_dir(&root)
        .args(["compile", "--scope", "project:checkout", "--out"])
        .arg(out.path())
        .arg("--source")
        .arg(&source)
        .output()
        .expect("run compile");
    assert!(
        compile.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&compile.stdout),
        String::from_utf8_lossy(&compile.stderr)
    );

    assert!(out.path().join("index.md").is_file());
    assert!(out.path().join("mem_checkout_taipei.md").is_file());
    let metadata_path = out.path().join("memphant-export.json");
    assert!(metadata_path.is_file());

    let verify = Command::new(env!("CARGO_BIN_EXE_memphant-cli"))
        .current_dir(&root)
        .args(["verify", "--lock", "memphant.lock", "--export"])
        .arg(out.path())
        .output()
        .expect("run verify export");
    assert!(
        verify.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&verify.stdout),
        String::from_utf8_lossy(&verify.stderr)
    );
    assert!(String::from_utf8_lossy(&verify.stdout).contains("export=clean"));

    let mut metadata: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&metadata_path).unwrap()).unwrap();
    metadata["source_hash"] = serde_json::Value::String("drifted-source".to_string());
    fs::write(
        &metadata_path,
        serde_json::to_vec_pretty(&metadata).unwrap(),
    )
    .unwrap();

    let stale = Command::new(env!("CARGO_BIN_EXE_memphant-cli"))
        .current_dir(&root)
        .args(["verify", "--lock", "memphant.lock", "--export"])
        .arg(out.path())
        .output()
        .expect("run stale verify");
    assert!(!stale.status.success());
    assert!(String::from_utf8_lossy(&stale.stderr).contains("export_source_hash"));
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
