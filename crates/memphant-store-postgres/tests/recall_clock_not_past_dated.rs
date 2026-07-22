//! Guard against a bitemporal trap that cost real debugging time in C1: a Pg
//! test that pins a **past-dated** `FixedClock` and then calls `recall()`.
//!
//! On Postgres the worker stamps `transaction_from` with real wall-clock
//! `now()` at compile, but recall filters candidates on `transaction_from <=
//! recall_time`, where `recall_time` defaults to the service clock. If that
//! clock is a fixed date in the PAST, `now() (write) <= past (recall)` is false
//! for every freshly-compiled unit, so recall silently returns zero — a green
//! test that proves nothing (the C1 `hot_path_slo_pg.rs` symptom). On
//! `InMemoryStore` the same clock stamps BOTH write and recall, so the window
//! always aligns and the bug is invisible — which is exactly why this guard
//! lives in the Pg test crate.
//!
//! Rule: within this crate's tests, any file that both defines a `FixedClock`
//! and calls `.recall(` must pin every `FixedClock` to a date >= today. The
//! store-testkit's `OLD_CLOCK` (a deliberately-aged supersession fixture) lives
//! in a different crate and is not scanned here.
//!
//! ponytail: substring scan over the test sources, not a Rust parser. It scans
//! sibling test files at their on-disk paths (a test cannot `include_str!` a
//! whole directory). Ceiling: a `.recall(` inside a comment in the same file as
//! a past clock would false-trip; none exist. Upgrade to a real parse only if
//! that ever bites.

use std::fs;
use std::path::Path;

/// Files exempt from the `.recall(` scan: they legitimately pin a past clock but
/// never recall (they verify via direct SQL count / fetch-by-id, which is not
/// bitemporal-windowed). Verified 2026-07-22: each has zero `.recall(` calls.
/// A file only needs listing here if it BOTH past-dates a clock AND we have
/// confirmed its `.recall(`-free status; adding a recall to one then trips the
/// guard, which is the point.
const RECALL_FREE_PAST_CLOCK_FILES: &[&str] = &[
    "reclaim_idempotency.rs",
    "mutation_ledger.rs",
    "role_matrix.rs",
];

/// The floor year a recall test's clock may be pinned to. A clock dated before
/// this is unambiguously the past-dated trap. Coarse by design: real clocks are
/// years apart (2030 vs 2026), so a year check has zero false positives and
/// needs no runtime `now()` (which would make the guard itself non-deterministic).
/// Bump when the repo's "present" moves past it.
const CLOCK_FLOOR_YEAR: i32 = 2026;

fn extract_fixed_clock_years(src: &str) -> Vec<i32> {
    let mut years = Vec::new();
    let mut rest = src;
    while let Some(pos) = rest.find("FixedClock(\"") {
        let after = &rest[pos + "FixedClock(\"".len()..];
        // The year is the first four chars of an RFC3339 date literal.
        if let Some(year) = after.get(..4).and_then(|y| y.parse::<i32>().ok()) {
            years.push(year);
        }
        rest = &after[4..];
    }
    years
}

#[test]
fn pg_recall_tests_never_pin_a_past_dated_clock() {
    let tests_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests");
    let mut scanned_recall_files = 0;
    for entry in fs::read_dir(&tests_dir).expect("read tests dir") {
        let path = entry.expect("dir entry").path();
        if path.extension().and_then(|e| e.to_str()) != Some("rs") {
            continue;
        }
        let name = path.file_name().unwrap().to_str().unwrap().to_string();
        // Never scan this guard file itself (it names FixedClock in prose).
        if name == "recall_clock_not_past_dated.rs" {
            continue;
        }
        let src = fs::read_to_string(&path).expect("read test source");
        let years = extract_fixed_clock_years(&src);
        if years.is_empty() {
            continue;
        }
        let recalls = src.contains(".recall(");
        if !recalls {
            // Past clock without recall is inert; nothing to enforce. But if we
            // listed it as recall-free and it grew a recall, the branch below
            // catches it.
            continue;
        }
        scanned_recall_files += 1;
        assert!(
            !RECALL_FREE_PAST_CLOCK_FILES.contains(&name.as_str()),
            "`{name}` is on the recall-free-past-clock exemption list but now calls \
             `.recall(`. A past-dated FixedClock + recall on Postgres silently \
             returns zero (the worker stamps transaction_from with real now(), \
             recall filters transaction_from <= past recall_time). Future-date its \
             clock (e.g. 2030-01-01) or remove it from the exemption list."
        );
        let earliest = *years.iter().min().unwrap();
        assert!(
            earliest >= CLOCK_FLOOR_YEAR,
            "`{name}` calls `.recall(` with a FixedClock pinned to {earliest} (before \
             the floor year {}). On Postgres the worker stamps transaction_from with \
             real now(), and recall filters transaction_from <= recall_time; a past \
             clock excludes every freshly-compiled unit, so recall returns zero and \
             the test proves nothing. Future-date the clock (e.g. 2030-01-01T00:00:00Z).",
            CLOCK_FLOOR_YEAR
        );
    }
    assert!(
        scanned_recall_files >= 2,
        "expected to scan several Pg recall tests; found {scanned_recall_files} — the \
         scanner is likely broken (guards against a silent pass)."
    );
}
