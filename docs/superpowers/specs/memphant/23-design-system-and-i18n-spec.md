# MemPhant - Design System and i18n Spec

## 0. Brand

MemPhant can nod to elephant memory in name and voice. The product UI should feel like infrastructure: precise, calm, inspectable.

## 1. Visual Principles

- dense but readable
- evidence before decoration
- neutral colors with strong status contrast
- tables built for scanning
- code blocks first-class
- no mascot-heavy debugging UI

### 1.1 Design Tokens

The canonical token set (OKLCH for perceptual consistency; ships as `tokens.css` when the dashboard is built). Status tokens carry a **min contrast ratio ≥ 4.5:1 on the surface background** (WCAG AA) because trust state is safety-relevant and must never be color-only (§3):

```css
/* neutral scale (infrastructure calm) */
--mp-bg:        oklch(0.99 0 0);     --mp-surface: oklch(0.97 0.005 250);
--mp-border:    oklch(0.90 0.01 250); --mp-text:    oklch(0.25 0.02 250);
--mp-text-muted: oklch(0.50 0.02 250);
/* status (paired with text labels + icons, never color alone) */
--mp-trusted:     oklch(0.55 0.13 150);  /* green  — ≥4.5:1 */
--mp-provisional: oklch(0.65 0.14 85);   /* amber  — belief/low-confidence */
--mp-low-trust:   oklch(0.60 0.15 50);   /* orange */
--mp-quarantined: oklch(0.55 0.20 25);   /* red    — excluded from recall */
--mp-stale:       oklch(0.55 0.02 250);  /* gray   — superseded, still citable */
--mp-degraded:    oklch(0.60 0.16 60);   /* recall served in consolidation-lag fallback */
/* type scale (1.2 ratio, mono for evidence) */
--mp-font-mono: ui-monospace, "JetBrains Mono", monospace;
--mp-fs-xs: 0.75rem; --mp-fs-sm: 0.875rem; --mp-fs-md: 1rem; --mp-fs-lg: 1.25rem;
/* spacing (4px base) */
--mp-sp-1: 4px; --mp-sp-2: 8px; --mp-sp-3: 12px; --mp-sp-4: 16px; --mp-sp-6: 24px;
```

Tokens are the design frozen-interface: components (`19` §10) reference tokens, never raw hex, so a theme change is one file.

## 2. Components

Required:

- install command block
- API key card
- trace timeline
- candidate table
- memory unit drawer
- citation chip
- trust badge
- eval scorecard
- benchmark config panel
- deletion status row
- comparison table

Component rules:

- trust badges always include text labels
- citation chips open evidence drawers
- trace timelines use stable columns so rows do not jump during loading
- scorecards show cost/latency next to accuracy
- destructive actions require explicit confirmation and policy text
- memory text is wrapped and searchable, never squeezed into tiny cards

## 3. Status Labels

Use text plus color:

```text
trusted
provisional
low trust
quarantined
forgotten
expired
```

Never color-only.

## 4. i18n

Docs and UI should be i18n-ready, not translated at launch.

Rules:

- no string concatenation in UI
- ISO timestamps with locale formatting at render
- docs examples in English first
- memory content language is opaque user data

Translate when:

- non-English users appear in support
- enterprise deal requires it
- docs traffic justifies it

## 5. Accessibility

Minimum launch gate:

- keyboard navigation
- focus states
- semantic headings
- table captions
- copy buttons with labels
- high contrast status badges
- reduced-motion safe UI

## 6. Localization Risks

Memory systems often store multilingual user content. UI localization must not:

- translate user memory content by default
- change quote hashes
- alter citation spans
- make trust labels ambiguous
- localize machine-readable IDs

Translation belongs to labels/help text, not evidence.
