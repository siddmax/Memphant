# MemSyco PMU v2 Unscoped Calibration Design

## Goal

Qualify the repaired empty-scope preference arbitration on new non-official data without changing or reusing the retired v1 confirmation or official track.

## Design

Extend the existing Personalized Memory Use generator with two new immutable splits: `development_v2` and `confirmation_v2`. Each split contains six topic-disjoint polarity-twin families, producing 12 cases. Every explicit preference uses the unscoped form `I prefer <value>.`; every distractor is positively worded but objectively unsuccessful.

The existing development and confirmation bytes and hashes remain unchanged. All four calibration splits must be pairwise topic-disjoint and the combined v2 cases must have zero exact hashes, suspicious row matches, and normalized five-gram overlaps with the official 300 rows.

Run complete arms sequentially. Development runs RawDialogue, MemPhant, and diagnostic episode-only. Freeze only after RawDialogue and MemPhant reach 12/12 for answer accuracy and preference use, all MemPhant packet proofs pass, and provenance is complete. Open `confirmation_v2` once: RawDialogue first, then MemPhant, then episode-only. Any candidate change retires the opened pack.

## Boundaries

- The v1 official track remains retired and untouched.
- No official rerun, new retrieval behavior, learned gate, graph, decay, dependency, API, or schema.
- No commit, reset, clean, rebase, push, `STATUS.md` change, or paused-cutover change.
- A future SOTA claim still requires a new independent benchmark version or holdout.

