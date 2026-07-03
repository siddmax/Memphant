# 2026-07-03 WS-G Public UI, Docs, and Launch Surface Progress

## Scope

WS-G public launch-surface exit packet:

- Docs site entry surface.
- Developer dashboard with API key, usage, recall, and trace-link summaries.
- Trace explorer with stage timing, candidates, dropped memories, policy filters, final context, citations, and raw JSON.
- Memory inspector with correction/forget affordances and evidence links.
- API keys and usage page.
- Eval run viewer with accuracy, CI, latency, cost, source status, trace archive, and security result.
- Compiled memory export viewer with lock-verification status and evidence links.

## Artifacts

- WS-G plan: `docs/superpowers/plans/2026-07-03-memphant-wsg.md`
- Web package: `web/package.json`
- Static launch surface: `web/public/index.html`, `web/public/app.js`, `web/public/styles.css`
- API-shaped fixture: `web/public/api/fixture/launch-surface.json`
- Local route server: `web/serve.mjs`
- Playwright config: `web/playwright.config.mjs`
- Playwright gate: `web/tests/launch-surface.spec.js`

## Implementation Notes

- The surface is self-contained under `web/` and uses a local Node static server with route fallback. It does not import Rust internals or query any database.
- The UI fetches only `/api/fixture/launch-surface.json`, which is shaped like public API data: traces, memory units, citations, eval runs, exports, API keys, and usage.
- Trust/status labels are text plus a visible marker and `aria-label`; table views include captions; trace IDs are copyable; keyboard focus states are visible.
- Every visible memory/export row links to either `/traces/...` or `/citations/...`.
- Benchmark copy avoids unsupported bare SOTA claims and shows source status, CI, latency, cost, security result, and trace archive.

## Verification

WS-G web gate:

```text
npm test
PASS: 6 passed
Coverage: routes, trace explorer, evidence links, accessibility basics, no DB/SQL requests, benchmark claim guard.
```

Repo gates:

```text
cargo fmt --check
PASS

cargo clippy --all-targets --all-features -- -D warnings
PASS

cargo test --all-targets --all-features
PASS

cargo test --doc
PASS

python3 -m pytest tests
PASS: 19 passed

python3 scripts/check_spec_drift.py
PASS: spec_drift=clean public=/Users/sidsharma/Memphant/docs/superpowers/specs/memphant private=/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant
```

## Status

WS-G exit packet is complete. Next workstream: WS-H BYOC, Hosted Packaging, and Deployment.
