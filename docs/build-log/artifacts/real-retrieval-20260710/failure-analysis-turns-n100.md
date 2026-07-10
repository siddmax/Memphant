# MemPhant weak-strata failure analysis — turns granularity, rerank-off (n=100, QA 0.560)

Scope: every INCORRECT question in temporal-reasoning (16), single-session-preference (4), and
multi-session (16) = 36 failures. Data joined by `question_id` across
`scaled-reader-turns-rerank-off.json` (reply/correct), `scaled-lme-s-turns-rerank-off.json`
(hit_at_5/10, first_answer_rank), and `reader-evidence-scaled-turns-rerank-off.jsonl` (the exact
packed evidence the reader saw). All quotes below are verbatim from the packed evidence.

## Operationalization of the modes
- **A retrieval miss** = `first_answer_rank = None` / `hit_at_10 = False` — gold session absent from top-10.
- **B pack drop** = `hit_at_10 = True` (gold session provenance-hit) BUT the answer-bearing text is not in
  the packed turn-windows the reader saw (turns-granularity dropped the answer window or a sibling sub-session).
- **C reader miss** = answer text IS in a packed turn, reader still wrong.
- **D temporal/numeric composition** = the operands/dated events ARE in the pack and the answer is derivable,
  but requires arithmetic/ordering across items and the reply got it wrong.
- **E judge artifact** = reply semantically correct, scored wrong.

## 1. Mode-count table per stratum

| Stratum | n_wrong | A retrieval-miss | B pack-drop | C reader-miss | D composition | E judge |
|---|---|---|---|---|---|---|
| temporal-reasoning | 16 | 6 | 8 | 0 | 2 | 0 |
| multi-session | 16 | 3 | 11 | 0 | 2 | 0 |
| single-session-preference | 4 | 3 | 0 | 1 | 0 | 0 |
| **TOTAL (36)** | **36** | **12** | **19** | **1** | **4** | **0** |

Headline: **B (pack drop) = 19/36 (53%)** is the dominant mode, A = 12 (33%), D = 4 (11%), C = 1, E = 0.
24 of 36 failures are `hit_at_10 = True` ("gold present, still wrong"); only 12 are true retrieval misses.

## 2. Per-question classification

### multi-session (16)
| id | mode | justification (verbatim evidence) |
|---|---|---|
| 09ba9854 | D | taxi "$60" (r4) and train "$10" (r1) both packed; gold $50 = 60−10; reader "I don't know." — arithmetic not performed |
| 2318644b | D | Maui resort "over $300 per night" (r7) and "hostel in Tokyo that cost around $30 per night" (r10) both packed; gold $270 = 300−30; abstained |
| 73d42213 | B | packed windows show clinic "available morning slots ... (9:00 am - 12:00 pm)" but not the user's actual arrival; arrival-time turn not in the packed windows of answer_1881e7db |
| a96c20ee | B | gold answer_ef84b994_1 packed only turns 1-4 & 9-12 (5-8 dropped); "Harvard" in NO packed turn — pack has "presented ... at my first research conference over the summer" with no university |
| bb7c3b45 | B | all of answer_de64539a_1 packed shows only "$200 at the outlet mall"; original price / "$300 saved" absent (unretrieved sibling sub-session); abstained |
| d905b33f | B | only 1 window packed; "originally priced at $30" present but the discounted price / "20%" absent from pack |
| 60159905 | A | rank None — no dinner-party-count evidence retrieved |
| 8e91e7d9 | A | rank None — sibling-count evidence not retrieved |
| c18a7dc8 | A | rank None — graduation-age evidence not retrieved |
| 67e0d0f2 | B | pack shows only "completed 12 courses on Coursera" (r4); other-platform counts summing to gold 20 not packed; reader answered 12 (all it saw) |
| 81507db6 | B | only 1 of 3 ceremonies packed ("Rachel's graduation ceremony", r10); reader answered 1 |
| bf659f65 | B | only 2 of 3 purchases packed (Whiskey Wanderers EP "Midnight Sky"; Billie Eilish "Happier Than Ever"); reader answered 2 |
| e3038f8c | B | pack shows only "12 rare figurines" (r3) + "5 rare books" (r7); gold total 99 needs categories not packed; reader summed visible items to 22 |
| gpt4_194be4b3 | B | packed windows of answer_3826dc55 cover ukulele-care/pedals/violin-apps, NOT the 4 ownership statements (Strat/Yamaha/Pearl/Korg); abstained |
| gpt4_2f8be40d | B | only Emily/Sarah wedding packed (r8, rank 8, hit5 False); Rachel/Mike & Jen/Tom weddings not retrieved; reader answered 2 (counted a sister/cousin distractor) |
| gpt4_ab202e7f | B | only toaster replacement packed (r4 "got rid of the old toaster"); other 4 items (faucet/mat/coffee maker/shelves) not in pack; reader answered 1 |

### temporal-reasoning (16)
| id | mode | justification (verbatim evidence) |
|---|---|---|
| 0bc8ad93 | B | pack holds a DISTRACTOR "behind-the-scenes tour of the Science Museum with your friend, the chemistry professor" dated 2022/10/22 (~4.5mo, not 2mo); the actual "two months ago" alone-visit turn is not packed; abstained |
| af082822 | A | rank None — Nordstrom friends-and-family sale event not retrieved |
| b29f3365 | B | amp "two weeks ago" packed (r4/r7) but the guitar-lesson START anchor needed for gold "Four weeks" not in any packed window |
| cc6d1ec1 | B | bird-watching activity packed (birder's meetup, field guide, backyard bird calls) but the "workshop" event and start-date anchor are NOT packed; "two months" not derivable |
| gpt4_1d80365e | B | trip START packed ("just started my solo camping trip to Yosemite", 05/15) but END/duration anchor absent; only vague "Spending a few days at Yosemite"; abstained |
| gpt4_21adecb5 | A | rank None — undergrad-completion & thesis-submission dates not retrieved |
| gpt4_2312f94c | B | both devices packed but all in same-day (2023/03/15) sessions labelled "new"; no acquisition-order info; reader guessed "Dell XPS 13" (gold Samsung) |
| gpt4_2f56ae70 | D | Disney+ "free trial last month" (r1) and "Amazon Prime Video with HBO add-on" (r4, session 2023/05/26 23:40 — AFTER the 00:18 question time) both packed; reader ignored the question's as-of date and picked the post-question service |
| gpt4_468eb063 | A | rank None — "met Emma" event not retrieved |
| gpt4_59149c78 | A | rank None — art-event location not retrieved; reader confabulated "City Art Museum" (gold Metropolitan Museum) |
| gpt4_65aabe59 | B | thermostat & mesh both packed but in same-day sessions with no setup-order statement; abstained |
| gpt4_7f6b06db | A | rank None — the three trips not retrieved to order |
| gpt4_8279ba03 | A | rank None — "kitchen appliance bought 10 days ago" (smoker) not retrieved |
| gpt4_88806d6e | B | "friends Mark and Sarah, who I met on a beach trip about a month ago" packed; Tom appears only as vague "I met someone named Tom at a previous event" with no date; order not derivable; abstained |
| gpt4_d31cdae3 | B | both trips packed only as vague relative refs — "Grand Canyon before with my family on a road trip across the American Southwest" vs "solo trip to Europe last summer"; no hard dates to order; abstained |
| gpt4_e414231f | D | mountain bike "just fixed" 03/15 (Wed) & road bike maintenance 03/19 (Sun) both packed; question date 03/21, "past weekend" = 03/19 → road bike; reader mis-anchored and answered "Mountain bike" |

### single-session-preference (4)
| id | mode | justification |
|---|---|---|
| 32260d93 | A | rank None — comedy-special preference evidence not retrieved |
| 38146c39 | A | rank None — turbinado-sugar preference evidence not retrieved |
| 75832dbd | A | rank None — AI-in-healthcare preference evidence not retrieved |
| 57f827a0 | C | "mid-century modern walnut dresser project" preference IS packed (r4 answer_1bde8d3b); reader abstained on the open-ended "rearranging furniture ... any tips?" instead of personalizing |

## 3. Top lever recommendations (ranked by expected QA gain)

### Lever 1 — Session-complete packing (attacks 19 B; primary mover of both weak strata)
Mechanism: turns-granularity retrieves each turn-window independently, so the answer window (or a sibling
sub-session `answer_X_2`) of an ALREADY-retrieved gold session frequently ranks >10 and is dropped from the
10-item pack. Verified: a96c20ee packs answer_ef84b994_1 turns 1-4 & 9-12 but the Harvard-naming turns 5-8 are
dropped; d905b33f packs only 1 of 3 windows; the enumeration/counting cases pack only a subset of the answer
turns. Fix: when any window of a logical gold session (`answer_X_*`) enters top-k, gather ALL its windows and
sibling sub-sessions into the pack (neighbor-window / session-gather expansion), within budget.
- Moves: multi-session (11 B) and temporal (8 B).
- Estimated gain: the single-fact same-session-window B cases (a96c20ee, d905b33f, 73d42213, b29f3365,
  gpt4_1d80365e, gpt4_65aabe59, gpt4_88806d6e, gpt4_2312f94c, gpt4_d31cdae3, 67e0d0f2) are directly
  addressable; realistic recovery ~7–10 questions → overall QA 0.56 → ~0.63–0.66, temporal 0.41 → ~0.50,
  multi-session 0.41 → ~0.53. Counting cases that need a *different* session (weddings/ceremonies) are only
  partly helped and spill into Lever 2's recall.
- Caveat: expansion competes for pack budget — run jointly with the k/budget sweep (open task #4) so gathered
  windows don't evict other answers.

### Lever 2 — Query-date extraction + date-windowed recall (attacks 6 temporal A + de-noises temporal B/D)
Mechanism: temporal A failures ("how many weeks ago…", "how many months between X and Y", "bought 10 days
ago", "art event two weeks ago") get rank=None because vector similarity ignores dates. Extract the target
date/interval from the question relative to `question_date`, then filter/boost retrieval to sessions whose
event-date falls in that window; additionally drop sessions dated AFTER the question (fixes gpt4_2f56ae70's
post-question Amazon session) and disambiguate distractors (0bc8ad93's 4.5-month-old museum visit vs the
2-month one).
- Moves: temporal only.
- Estimated gain: ~4 of 6 temporal A + the 2 temporal D + 1–2 de-noised B → +4 to +6 temporal questions →
  temporal 0.41 → ~0.56–0.63. Needs a per-session event-date signal (a proxy from the session date header is
  adequate).

### Lever 3 — Reader prompt: cut preference over-abstention + enumerate-then-compute scaffolding (attacks 1 C, 4 D; multiplies Lever 1)
Mechanism: 26/36 failures are "I don't know." Two cheap prompt changes: (a) for open-ended preference
"tips/suggestions" questions, instruct the reader to synthesize from retrieved user context rather than
abstain (fixes 57f827a0); (b) for "how many / how much more / total" questions, instruct explicit
enumerate-then-compute over ALL packed items (fixes the D arithmetic cases 09ba9854 & 2318644b and makes the
counting cases answerable once Lever 1 supplies the items).
- Moves: preference (n=6, so +1–2 → 0.33 → ~0.50–0.67) and multi-session composition/counting.
- Estimated gain: +2 to +4 alone; strongly synergistic — Lever 1 without Lever 3 leaves the arithmetic/counting
  cases reader-limited. Prompt-only, so ship it alongside Lever 1.

Practical order: do **Lever 1 + Lever 3 together** (packing + reader unlock, biggest combined swing), then **Lever 2** for the temporal-specific recall gap.

## 4. Surprises
1. **Recall is not the bottleneck; packing is.** Retrieval recall@10 = 0.83 yet QA = 0.56. In the 3 weak
   strata, 24/36 failures have the gold session in top-10 and still fail — driven by turn-window drop (B), not
   retrieval miss. Session-level provenance "hit" masks the dropped answer window.
2. **Zero judge artifacts (E = 0).** Every containment/llm_judge "incorrect" is genuinely wrong; no
   semantically-correct reply was mis-scored, so the judge is neither inflating nor deflating these strata —
   the failures are real. (Structural note: the 4 wrong preference questions all fell to `containment` because
   the reply was "I don't know"; the 2 correct ones used `llm_judge` on real answers. Preference's long
   descriptive golds are only scorable via llm_judge, but since the reader abstained this caused no false
   negative here — it would matter the moment the reader stops abstaining.)
3. **The reader is well-calibrated, not hallucinating.** 72% of failures are honest abstentions when the pack
   lacks the answer. Good news: fixing pack completeness (Lever 1) converts abstentions to correct answers with
   low hallucination risk. The only specific-wrong answers are counting undercounts (of what's visible) and 2
   temporal date-anchoring slips.
4. **"Temporal reasoning" is mostly a retrieval/packing problem, not a reasoning problem.** Only 2/16 temporal
   failures are genuine reader composition (D); the rest are retrieval miss (A=6) + dropped date-anchor (B=8).
   A query-date recall lever will beat a reader-reasoning lever for this stratum.
5. **The w=2 ablation corroborates the diagnosis.** Smaller windows improved multi-session (0.41→0.52,
   more scattered answer-turns surface = coverage) but hurt temporal (0.41→0.37) and preference (0.33→0.17,
   coherent context lost). Multi-session is coverage-limited (Lever 1); temporal/preference need coherent +
   date-anchored context (Levers 2/3). No single window size wins all three — session-aware expansion + date
   filtering is the correct shape.
