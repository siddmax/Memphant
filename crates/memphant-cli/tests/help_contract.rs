use std::process::{Command, Output};

#[test]
fn compile_help_documents_the_complete_safe_workflow() {
    let output = help("compile");
    assert_success(&output);

    let help = String::from_utf8(output.stdout).expect("compile help is UTF-8");
    assert!(help.contains("Usage: memphant compile"));
    assert_context_flags(&help);
    for output in ["MEMORY.md", "units/", "inbox/", "memphant-export.json"] {
        assert!(help.contains(output), "missing output {output}:\n{help}");
    }
    assert_environment(&help);
    assert!(help.contains("Refuses to overwrite a dirty projection"));
    assert!(help.contains("Next: edit units/*.md or add inbox/*.md, then run `memphant sync`"));
}

#[test]
fn sync_help_documents_dry_run_apply_and_recovery() {
    let output = help("sync");
    assert_success(&output);

    let help = String::from_utf8(output.stdout).expect("sync help is UTF-8");
    assert!(help.contains("Usage: memphant sync"));
    assert_context_flags(&help);
    assert!(help.contains("--apply"));
    assert!(help.contains("Default: dry-run"));
    assert!(help.contains("JSON plan to stdout"));
    assert_environment(&help);
    assert!(help.contains("Review the plan, then rerun the same command with --apply"));
    assert!(help.contains(
        "outcome_unknown: the request may have committed; do not retry a different plan"
    ));
    assert!(
        help.contains("After apply: run `memphant verify --lock memphant.lock --export <DIR>`")
    );
    assert!(
        help.contains("First create the binary contract with `memphant lock --out memphant.lock`")
    );
}

#[test]
fn readme_quickstart_creates_the_binary_lock_before_verification() {
    let readme = include_str!("../../../README.md");
    let lock = readme
        .find("memphant lock --out memphant.lock")
        .expect("quickstart creates the binary contract lock");
    let verify = readme
        .find("memphant verify --lock memphant.lock --export")
        .expect("quickstart verifies the projection and lock");
    assert!(lock < verify, "lock creation must precede verification");
}

fn help(command: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_memphant-cli"))
        .args([command, "--help"])
        .env_remove("MEMPHANT_URL")
        .env_remove("MEMPHANT_API_KEY")
        .env_remove("MEMPHANT_HTTP_TIMEOUT_MS")
        .output()
        .expect("run memphant-cli")
}

fn assert_success(output: &Output) {
    assert!(
        output.status.success(),
        "stdout={} stderr={}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(output.stderr.is_empty());
}

fn assert_context_flags(help: &str) {
    for flag in [
        "--subject-id <UUID>",
        "--scope <UUID>",
        "--actor <UUID>",
        "--agent-node <UUID>",
        "--subject-generation <N>",
        "--out <DIR>",
    ] {
        assert!(help.contains(flag), "missing context flag {flag}:\n{help}");
    }
}

fn assert_environment(help: &str) {
    for variable in [
        "MEMPHANT_URL",
        "MEMPHANT_API_KEY",
        "MEMPHANT_HTTP_TIMEOUT_MS",
    ] {
        assert!(
            help.contains(variable),
            "missing environment variable {variable}:\n{help}"
        );
    }
}
