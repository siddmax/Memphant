# P1-T6 pre-execution amendment 3

Date: 2026-07-20

The second execution root, `run-f7c6c0f7`, and the bounded diagnostic root,
`diagnostic-dee83e37`, are infrastructure-invalid. Both are preserved without
replay. Their append-only invalidation proofs establish that no reader route,
judge route, Deep generation, official score, or settled provider spend was
produced. The diagnostic root reproduced the first deterministic worker
failure after 139 of 670 retained resources:

`conflict: contextual chunk span does not match its source body`

The failure had three root causes:

1. episode and resource contextual chunkers trimmed the stored chunk body but
   retained the original untrimmed byte span, violating exact citation
   reconstruction whenever the selected source window had edge whitespace;
2. worker drain treated one zero-completion tick as proof of an empty queue,
   even when a failed job was delayed for retry; and
3. campaign output paths remained relative while the official harness changed
   its working directory, so worker and official logs could be written outside
   the immutable row root.

The repair preserves each chunk body byte-for-byte with its source span; adds
cross-backend fleet-pending counts; makes drain wait until queued and running
jobs are absent; rejects any newly dead-lettered job; anchors all campaign
paths before changing working directory; and archives worker stdout and stderr
inside the row. Regression tests cover both chunkers, multibyte/source-span
reconstruction, the in-memory and Postgres worker-queue contracts, delayed
retry/dead-letter drain decisions, scratch-only worker-binary isolation, and
absolute campaign paths. The full Rust/Python gate and the real Postgres plus
worker-binary scratch gate pass at this repair.

A fresh paid benchmark root is still unauthorized by this amendment alone.
Before that root is created, a separate scratch-database proof must run the
pinned 500-trajectory Fast case through the packaged server, CLI, adapter, and
real worker with no reader or judge endpoint configured. It must archive a
single successful `drain completed=670` result, zero worker failures, exact
binary hashes, an empty post-drain pending queue, and scratch-database cleanup.
Only after that proof is committed may a fresh T6 campaign root be initialized.
The campaign adapter independently repeats the same exact-count gate before
any external reader call, so paid dispatch remains impossible unless all 670
resources compile successfully in the actual row.
