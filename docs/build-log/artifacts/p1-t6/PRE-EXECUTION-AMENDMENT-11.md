# P1-T6 pre-execution amendment 11

Date: 2026-07-20

## Superseding decision

This amendment supersedes the four-arm execution contract before any new Deep
treatment output. The only executable Deep arm is Sonnet:
`anthropic/claude-sonnet-5-20260630` on Azure, bound to config hash
`22730027f29f7daa15b7b8905878ce6d9f45ee49491db415960f431da72bcf75`.

The gate is exactly 12 Fast/Sonnet pairs: 12 constructions, 24 answer rows,
and at most 12 Deep dispatches. Luna and Sol remain inactive researched
shortlist metadata. They require a fresh answer-blind amendment and a new
output root; they are neither fallback nor a ranking arm in this root.

The stopped `run-408363c9` root is diagnostic-only and is never replayed. Its
invalidation proof SHA-256 is
`7e360eadceead985dbc729a935cbf8d276abde27a8b016c262b49c961a210bad`.
The frozen paired manifest SHA-256 is
`0cbc32d51b8ec665d9c1b4ac7dcdc8dec1f975ad63b830501846b797e9bdda6a`.

## Cumulative hard-cap contract

The stopped root's settled reader cost of 3,018 micro-dollars is added to
prior liability. Prior liability is therefore 7,542 settled micros plus
316,142 unresolved upper-bound micros, or 323,684 micros total. Fresh
reservations are 3,600,000 micros for 12 bounded Deep dispatches and
2,097,600 micros for 24 reader/judge reservations, or 5,697,600 micros.
The cumulative maximum is 6,021,284 micros, below the tightened 6,250,000
micro-dollar hard ceiling, with 228,716 micros headroom. The runner must
carry all settled and unsettled prior liability before every reservation and
fail closed at that ceiling.

## Pre-dispatch efficiency checkpoint

Before each paid or large-compute step, archive a checkpoint with these
fields:

- necessity: the Fast/Sonnet paired question being resolved;
- reusable work: one immutable pre-query construction cloned into both arms;
- expected information gain: the registered paired Sonnet-minus-Fast score;
- maxima: 12 constructions, 24 answer rows, 12 Deep dispatches, and
  6,021,284 micros cumulative worst-case liability;
- stop predicate: any failed pair, cap/infra/security/write failure, missing
  proof, or Sonnet infeasibility stops T6 without Luna or Sol.

This amendment authorizes no treatment output by itself. It is not a
promotion, ledger update, product-default change, public claim, or authority
to replay any historical row.
