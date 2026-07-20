# P1-T6 pre-execution amendment

Before any reader, judge, or Deep output and before any paid request, review found that the preregistered selection digest `ffe151038e3dc54c8132b58a2d39575db9ee37d0ead8f873afda67a6e35c2bea` had no serialization definition and could not be reproduced from the frozen rows using ordinary canonical encodings.

The frozen IDs and answer-blind selection algorithm are unchanged. The replacement contract hashes UTF-8 canonical JSON produced with `json.dumps(rows, sort_keys=True, ensure_ascii=True, separators=(",", ":"))`; rows are sorted by `(domain, ability, id)` and contain exactly `domain`, `ability`, `question_type`, and `id`. Its digest is `d7762dbaffff7acfe779162d4993c8c09ef0440e3c1a25e0d3408127d73e25fa`.

The same pre-output review replaced floating candidate aliases with dated dispatch slugs, pinned the official reader to one DeepInfra BF16 route through a fail-closed loopback request policy, and replaced the floating official judge alias with `gpt-5.2-2025-12-11`. No benchmark output was observed and no billable call was made before this amendment.
