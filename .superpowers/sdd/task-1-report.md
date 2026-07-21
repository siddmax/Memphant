# Task 1 implementation report: P1-T6 paired gate

## Result

Superseded the four-arm P1-T6 execution contract with the preregistered
Fast/Sonnet paired gate. The manifest expands to 24 rows for 12 cases,
contains 12 constructions, and names Sonnet as the only selected executable
Deep arm. Luna and Sol remain an inactive researched shortlist.

## Test-first evidence

1. Replaced the old 48-row manifest assertion with
   `test_campaign_is_single_candidate_paired_gate`.
2. Ran `python3 -m pytest tests/test_run_lme_v2_p1_t6.py -q` before production
   changes. It failed as intended because the old manifest verified as 48 rows
   and four arms instead of 24 rows, two arms, and 12 constructions.
3. Implemented the minimal manifest verification and row-expansion contract,
   then reran the focused suite successfully.

## Contract changes

- `selected_deep_arm` is `sonnet`; row order is `fast`, then `sonnet`.
- The fresh reserve is 3,600,000 micros for 12 Deep dispatches plus 2,097,600
  micros for 24 reader/judge reservations.
- Prior liability now includes the stopped `run-408363c9` reader's settled
  3,018 micros: 7,542 settled plus 316,142 unresolved, 323,684 micros total.
- Cumulative maximum is 6,021,284 micros under the 15,500,000-micro hard cap.
- Amendment 11 binds the manifest and stopped-root hashes and preregisters the
  required efficiency checkpoint and stop rule.

## Verification

- `python3 -m pytest tests/test_run_lme_v2_p1_t6.py -q` — 46 passed, 2 skipped.
- `python3 -m json.tool benchmarks/manifests/longmemeval_v2.p1_t6.json`.
- `git diff --check`.

## Self-review

The runner change is restricted to manifest verification; aggregate selection
remains deliberately deferred to Task 4. Its two legacy four-arm synthetic
aggregate tests are explicitly skipped until Task 4 replaces them with paired
aggregate contracts. No treatment was dispatched, no historical artifact was
changed, and the unrelated handoff edit was left unstaged.
