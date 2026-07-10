//! Worker binary smoke test (plan addendum W1-d): runs the real compiled
//! `memphant-worker` binary as a subprocess with `MEMPHANT_WORKER_ONCE=1`
//! against a live, migrated Postgres database and asserts it exits 0 and
//! prints the "once completed=" line. Before this test, the worker binary's
//! entrypoint (tick loop, `MEMPHANT_WORKER_ONCE` exit path) had zero
//! automated coverage of any kind — only manual exercise via
//! `scripts/e2e_probe.sh`.
//!
//! Gated exactly like `pg_store_contract.rs`: `#[ignore]`, reads
//! `MEMPHANT_TEST_DATABASE_URL` (the test translates that into the
//! `DATABASE_URL` the worker binary itself reads via
//! `memphant_runtime::build_store`). Run with:
//!   MEMPHANT_TEST_DATABASE_URL=postgres://memphant:memphant@localhost:5432/memphant \
//!     cargo test -p memphant-worker -- --ignored --test-threads=1

use std::process::Command;

fn db_url() -> String {
    std::env::var("MEMPHANT_TEST_DATABASE_URL")
        .expect("MEMPHANT_TEST_DATABASE_URL must point at a migrated Postgres")
}

#[test]
#[ignore = "requires MEMPHANT_TEST_DATABASE_URL"]
fn worker_once_tick_exits_zero_and_prints_completed_line() {
    let output = Command::new(env!("CARGO_BIN_EXE_memphant-worker"))
        .env("DATABASE_URL", db_url())
        .env("MEMPHANT_WORKER_ONCE", "1")
        .output()
        .expect("memphant-worker binary runs");

    assert!(
        output.status.success(),
        "memphant-worker --once must exit 0; stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("memphant-worker: once completed="),
        "stdout must report the once-tick completion line, got: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}
