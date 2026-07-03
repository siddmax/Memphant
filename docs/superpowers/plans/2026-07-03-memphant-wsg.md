# MemPhant WS-G Public UI, Docs, and Launch Surface Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans. WS-G is a public-surface slice: build only API-backed inspectability surfaces, not a decorative SaaS shell.

**Goal:** Complete WS-G by adding a repo-owned public launch surface with docs, dashboard, trace explorer, memory inspector, API key/usage, eval run, and compiled export views. The surface must read only API-shaped data, pass route/accessibility Playwright checks, and link every visible memory/citation item to a trace or citation path.

---

## Scope Notes

- `29` WS-G requires docs site, trace explorer, memory inspector, API key/usage pages, eval run viewer, and compiled export viewer.
- `19` says the trace explorer is the signature UX; the dashboard must stay dense and operational.
- `23` requires status labels with text, semantic headings, table captions, focus states, and no color-only trust labels.
- This slice creates a static local launch surface backed by fixture JSON shaped like public API responses. Hosted auth/billing/control-plane work remains WS-H.

## Architecture

- Add a self-contained `web/` package:
  - static files under `web/public/`;
  - a tiny Node static server with route fallback;
  - fixture data under an API-shaped path (`/api/fixture/launch-surface.json`);
  - Playwright route/a11y/no-DB tests.
- The UI fetches only `/api/...` fixture data. It does not import Rust internals, read SQL, or query MemPhant DB tables.
- Trace, memory, eval, export, dashboard, API-key, and citation routes share the same fixture so trace/citation links remain consistent.

## Tasks

### Task 1: Public Launch Surface

**Files:**
- Create: `web/package.json`
- Create: `web/package-lock.json`
- Create: `web/playwright.config.mjs`
- Create: `web/serve.mjs`
- Create: `web/public/index.html`
- Create: `web/public/styles.css`
- Create: `web/public/app.js`
- Create: `web/public/api/fixture/launch-surface.json`

- [x] Implement route rendering for home, docs, dashboard, traces, memory, API keys, evals, exports, and citations.
- [x] Use `tokens.css`-equivalent CSS variables from `23` and keep trust labels text+icon.
- [x] Make every visible memory/citation item link to `/traces/...` or `/citations/...`.

### Task 2: Playwright Gates

**Files:**
- Create: `web/tests/launch-surface.spec.js`

- [x] Route coverage for all WS-G pages.
- [x] Accessibility basics: semantic headings, table captions, keyboard focus, copyable IDs, status labels not color-only.
- [x] Public surface never requests DB/SQL paths and fetches only API-shaped launch data.

### Task 3: Proof and Status

**Files:**
- Create: `docs/build-log/2026-07-03-wsg-progress.md`
- Modify: `docs/superpowers/specs/memphant/STATUS.md`
- Mirror: `/Users/sidsharma/Syndai/.wt/codex-memphant-cross-repo/docs/superpowers/specs/memphant/STATUS.md`

- [x] Run web Playwright gates.
- [x] Run MemPhant repo gates.
- [x] Mark WS-G complete only after route/accessibility/DB-boundary checks pass.
