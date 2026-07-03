use std::fs;
use std::path::PathBuf;
use std::process::Command;

use memphant_types::MemphantLock;

#[test]
fn verify_succeeds_for_current_lock() {
    let output = Command::new(env!("CARGO_BIN_EXE_memphant-cli"))
        .args(["verify", "--lock"])
        .arg(repo_root().join("memphant.lock"))
        .output()
        .expect("run memphant-cli");

    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("verify=clean"));
}

#[test]
fn verify_fails_for_drifted_lock_and_names_mismatch() {
    let drifted =
        std::env::temp_dir().join(format!("memphant-lock-drift-{}.json", std::process::id()));
    let mut lock: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(repo_root().join("memphant.lock")).unwrap())
            .unwrap();
    lock["engine_version"] = serde_json::Value::String("engine-drift".to_string());
    fs::write(&drifted, serde_json::to_vec_pretty(&lock).unwrap()).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_memphant-cli"))
        .args(["verify", "--lock"])
        .arg(&drifted)
        .output()
        .expect("run memphant-cli");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("engine_version"));
    let _ = fs::remove_file(drifted);
}

#[test]
fn lock_out_dash_emits_current_lock_json() {
    let output = Command::new(env!("CARGO_BIN_EXE_memphant-cli"))
        .args(["lock", "--out", "-"])
        .output()
        .expect("run memphant-cli");

    assert!(output.status.success());
    let lock: MemphantLock = serde_json::from_slice(&output.stdout).expect("lock json");
    assert_eq!(lock, MemphantLock::current());
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}
