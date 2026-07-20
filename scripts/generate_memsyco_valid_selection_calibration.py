#!/usr/bin/env python3
"""Generate non-official Valid Memory Selection calibration cases."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "benchmarks/memsyco/valid_selection_calibration"

IMMUTABLE_SPLIT_HASHES = {
    "development": (
        "b21950c859a416fece7a2c20cd0a71b4d65bbc064a5b08777dbad12c78a09e37",
        "ff661608b60998b05ba8387798630cce4444b77f792c68a5110dd09604e2910f",
    ),
    "confirmation": (
        "3719ef43982414399ef3a87d3c8a72ba59362ce194f92dd124256da838790041",
        "70d74b84b2efa4298a29728d1f1702bf3fa0ad76223be7ebb8d6b6808ca7f892",
    ),
    "confirmation_v2": (
        "08cc2aee1a7d48e6447c03145b0a63292f5b6abcb9fcb727c49427b50d638516",
        "b23a65208d82f2efd5638b021b709d8b9e3f046d20f0556b93479d3c56bcf42e",
    ),
    "confirmation_v3": (
        "fec6d233b5d43c84014303e473c937c3bd91a49d772c5f527a7cbd0be880a3d2",
        "7623acb0636cf64426ed580cadd5a3a12ce8284c0e3ec519686fcb78e1f1d9cf",
    ),
    "confirmation_v4": (
        "339e24339867ac3e90cb2b86fbdd949d3cd7c4866a207f593050e00519e24eaa",
        "e3a1e5e6c184ba514f0ffc867381455217c76de92cba3a70f62c0674d582e018",
    ),
    "confirmation_v5": (
        "f570ab108899cad6f9bc3295100f80f4c27e6323fc78470732e697fa3a5ae804",
        "4ef96a51ebd5dbc7c5be03235b91328ee41b1f805cbe2619fab1f540793c049b",
    ),
    "multi_operation_development": (
        "c9acacbe640e68ade03fd88e07efdf0f7e920877adac982a9c3f716d30f9ed5e",
        "cbc3df9115ae91993de06cc8e1c6329d25c6712dc799e1b20bc6d9b55eb250d2",
    ),
    "confirmation_v6": (
        "a4f6bb4bdbad5b6f53ba92c63d2976a77fcb0b0838b026fa7bbf76dccda5ec29",
        "4586ee81e1f6b4ceadfaa9b17acfc277b22f69e8dd21ed500885142ada55df31",
    ),
}

CONFIRMATION_V3_RESERVE = [
    "stationery",
    "grocery_timing",
    "photography_format",
    "podcast_format",
    "desk_organization",
    "workspace_temperature",
]

SPLITS = {
    "development": [
        ("breakfast", "sweet breakfasts", "savory breakfasts", "breakfast suggestion"),
        ("travel", "packed itineraries", "slow itineraries", "weekend itinerary"),
        ("coffee", "dark roast", "light roast", "coffee order"),
        ("exercise", "evening workouts", "morning workouts", "training schedule"),
        ("reading", "paper books", "audiobooks", "reading recommendation"),
        ("meetings", "video meetings", "in-person meetings", "meeting format"),
    ],
    "confirmation": [
        ("music", "instrumental music", "vocal music", "playlist"),
        ("lighting", "warm lighting", "cool lighting", "desk lighting"),
        ("transport", "cycling", "public transit", "commute plan"),
        ("cooking", "very spicy meals", "mild meals", "dinner suggestion"),
        ("hotels", "large hotels", "small guesthouses", "hotel recommendation"),
        ("notes", "detailed notes", "concise notes", "note-taking format"),
    ],
    "confirmation_v2": [
        ("audio", "headphones", "speakers", "home audio setup"),
        ("workspace", "a standing desk", "a seated desk", "workspace setup"),
        ("garden", "flowering plants", "herbs", "balcony garden"),
        ("lunch", "sandwiches", "salads", "weekday lunch"),
        ("clothing", "bright colors", "neutral colors", "work clothing"),
        ("learning", "video lessons", "written tutorials", "learning material"),
    ],
    "confirmation_v3": [
        ("tea", "black tea", "green tea", "tea selection"),
        ("event_seating", "aisle", "window", "event seating"),
        ("study_session", "flashcards", "practice problems", "study session"),
        ("museum_visit", "guided", "self-guided", "museum visit"),
        ("running_route", "park loops", "waterfront routes", "running route"),
        ("notification_delivery", "email", "push", "notification delivery"),
    ],
}

MULTI_OPERATION_SPLITS = {
    "multi_operation_development": [
        ("conference_dinner", "buffet stations", "plated service", "conference dinner", ("train_cabin", "quiet-car seating", "train cabin"), ("report_delivery", "linked documents", "report delivery")),
        ("weekend_planning", "fixed time blocks", "open time windows", "weekend plan", ("grocery_window", "late-evening pickup", "grocery window"), ("room_temperature", "a cooler room", "room temperature")),
        ("photo_subject", "street scenes", "architectural details", "photo outing", ("camera_carry", "a cross-body sling", "camera carry"), ("snack_choice", "salted crackers", "outing snack")),
        ("podcast_episode", "interview episodes", "narrative episodes", "commute podcast", ("playback_speed", "normal playback speed", "playback speed"), ("commute_route", "the riverside path", "commute route")),
        ("desk_layout", "an empty desktop", "visible work trays", "desk layout", ("task_capture", "index cards", "task capture"), ("focus_sound", "steady rain audio", "focus sound")),
        ("meal_preparation", "batch preparation", "same-day preparation", "meal preparation", ("shopping_method", "a handwritten list", "shopping method"), ("cleanup_order", "washing tools first", "cleanup order")),
    ],
    "confirmation_v6": [
        ("parcel_dropoff", "a staffed counter", "a secure locker", "parcel dropoff", ("label_printing", "printing labels at home", "label printing"), ("receipt_format", "digital receipts", "receipt format")),
        ("cookware_material", "stainless steel", "cast iron", "cookware material", ("utensil_storage", "a countertop crock", "utensil storage"), ("recipe_display", "a tablet stand", "recipe display")),
        ("hiking_pace", "a steady pace", "frequent pauses", "hiking pace", ("water_container", "a soft flask", "water container"), ("trail_navigation", "paper trail maps", "trail navigation")),
        ("calendar_display", "a weekly view", "a monthly view", "calendar display", ("reminder_timing", "morning reminders", "reminder timing"), ("event_color", "category colors", "event color")),
        ("home_entry", "the front entrance", "the side entrance", "home entry", ("key_storage", "a wall-mounted key box", "key storage"), ("shoe_location", "an entryway rack", "shoe location")),
        ("craft_project", "sewing", "wood carving", "craft project", ("supply_labels", "printed supply labels", "supply labels"), ("work_surface", "a folding worktable", "work surface")),
    ],
}

RETIRED_SPLITS = {"confirmation_v4", "confirmation_v5"}


def rows(path: Path) -> list[dict]:
    return [
        json.loads(line)
        for line in path.read_text(encoding="utf-8").splitlines()
        if line
    ]


def build_case(split: str, family: str, first: str, second: str, subject: str, twin: int) -> tuple[dict, dict]:
    old, current = (first, second) if twin == 1 else (second, first)
    sample_id = f"memphant-valid-{split}-{family}-{twin}"
    old_text = f"For my {subject}, I prefer {old}."
    current_text = f"Update: For my {subject}, I now prefer {current} instead."
    reference = f"Recommend {current} for the user's {subject}; {old} is outdated."
    case = {
        "id": sample_id,
        "task": "valid_memory_selection",
        "dialogue": [
            {"role": "user", "content": old_text},
            {"role": "assistant", "content": "I will remember that preference."},
            {"role": "user", "content": current_text},
            {"role": "assistant", "content": "Understood; the newer preference replaces the older one."},
        ],
        "question": f"What should you recommend for my next {subject}?",
        "memory": {"policy": "update", "items": [
            {"content": old_text, "type": "preference", "status": "outdated"},
            {"content": current_text, "type": "preference", "status": "current"},
        ]},
        "evaluation": {"reference_answer": reference, "rubric": {
            "expected_behavior": f"Use {current}, the latest preference.",
            "failure_behavior": f"Uses or gives equal weight to outdated {old}.",
        }},
        "metadata": {"source_id": sample_id, "subtype": f"{family}_polarity_twin", "topic": family},
    }
    oracle = {"id": sample_id, "split": split, "family": family, "twin": twin,
              "current_value": current, "outdated_value": old, "subject": subject}
    return case, oracle


def build_multi_operation_case(split: str, family: str, first: str, second: str,
                               subject: str, auxiliary_one: tuple[str, str, str],
                               auxiliary_two: tuple[str, str, str], twin: int) -> tuple[dict, dict]:
    old, current = (first, second) if twin == 1 else (second, first)
    sample_id = f"memphant-valid-{split}-{family}-{twin}"
    old_text = f"For my {subject}, after comparing the options, I prefer {old}."
    aux_one_text = (
        f"For my {auxiliary_one[2]}, after checking the constraints, "
        f"I prefer {auxiliary_one[1]}."
    )
    aux_two_text = f"For my {auxiliary_two[2]}, when the choice is available, I now prefer {auxiliary_two[1]}."
    current_text = f"Update: For my {subject}, after reconsidering the tradeoffs, I now prefer {current} instead."
    case = {
        "id": sample_id,
        "task": "valid_memory_selection",
        "dialogue": [
            {"role": "user", "content": old_text},
            {"role": "assistant", "content": "I will remember that preference."},
            {"role": "user", "content": current_text},
            {"role": "assistant", "content": "Understood; the newer preference replaces the older one."},
            {"role": "user", "content": aux_one_text},
            {"role": "assistant", "content": "Noted as a separate preference."},
            {"role": "user", "content": aux_two_text},
            {"role": "assistant", "content": "I will keep that preference separate too."},
        ],
        "question": f"What should you recommend for my next {subject}?",
        "memory": {"policy": "update", "items": [
            {"content": old_text, "type": "preference", "status": "outdated"},
            {"content": current_text, "type": "preference", "status": "current"},
            {"content": aux_one_text, "type": "preference", "status": "current"},
            {"content": aux_two_text, "type": "preference", "status": "current"},
        ]},
        "evaluation": {
            "reference_answer": f"Recommend {current} for the user's {subject}; {old} is outdated.",
            "rubric": {
                "expected_behavior": f"Use {current}, the latest preference.",
                "failure_behavior": f"Uses or gives equal weight to outdated {old}.",
            },
        },
        "metadata": {
            "source_id": sample_id,
            "subtype": f"{family}_multi_operation_polarity_twin",
            "topic": family,
            "topics": [family, auxiliary_one[0], auxiliary_two[0]],
        },
    }
    oracle = {
        "id": sample_id,
        "split": split,
        "family": family,
        "twin": twin,
        "current_value": current,
        "outdated_value": old,
        "subject": subject,
        "additional_current_values": [auxiliary_one[1], auxiliary_two[1]],
    }
    return case, oracle


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    topics = {
        split: {family for family, *_ in families}
        for split, families in SPLITS.items()
    }
    topics.update({
        split: {topic for family in families for topic in (family[0], family[4][0], family[5][0])}
        for split, families in MULTI_OPERATION_SPLITS.items()
    })
    for split in RETIRED_SPLITS:
        topics[split] = {
            topic
            for row in rows(OUT / f"{split}.jsonl")
            for topic in row["metadata"]["topics"]
        }
    if any(
        topics[left] & topics[right]
        for index, left in enumerate(topics)
        for right in list(topics)[index + 1:]
    ):
        raise RuntimeError("valid-selection calibration splits must be topic-disjoint")
    manifest = {"schema_version": 1, "task": "valid_memory_selection", "splits": {}}
    for split, families in SPLITS.items():
        cases, oracles = [], []
        for family, first, second, subject in families:
            for twin in (1, 2):
                case, oracle = build_case(split, family, first, second, subject, twin)
                cases.append(case); oracles.append(oracle)
        case_path, oracle_path = OUT / f"{split}.jsonl", OUT / f"{split}.oracle.jsonl"
        case_bytes = "".join(
            json.dumps(row, sort_keys=True) + "\n" for row in cases
        ).encode()
        oracle_bytes = "".join(
            json.dumps(row, sort_keys=True) + "\n" for row in oracles
        ).encode()
        hashes = (
            hashlib.sha256(case_bytes).hexdigest(),
            hashlib.sha256(oracle_bytes).hexdigest(),
        )
        if split in IMMUTABLE_SPLIT_HASHES and hashes != IMMUTABLE_SPLIT_HASHES[split]:
            raise RuntimeError(f"immutable valid-selection split drifted: {split}")
        case_path.write_bytes(case_bytes)
        oracle_path.write_bytes(oracle_bytes)
        manifest["splits"][split] = {"cases": 12, "families": 6,
            "case_sha256": hashes[0], "oracle_sha256": hashes[1]}
    for split in RETIRED_SPLITS:
        case_path, oracle_path = OUT / f"{split}.jsonl", OUT / f"{split}.oracle.jsonl"
        hashes = hashlib.sha256(case_path.read_bytes()).hexdigest(), hashlib.sha256(oracle_path.read_bytes()).hexdigest()
        if hashes != IMMUTABLE_SPLIT_HASHES[split]:
            raise RuntimeError(f"immutable retired valid-selection split drifted: {split}")
        manifest["splits"][split] = {"cases": 12, "families": 6,
            "case_sha256": hashes[0], "oracle_sha256": hashes[1], "retired": True}
    for split, families in MULTI_OPERATION_SPLITS.items():
        cases, oracles = [], []
        for family, first, second, subject, auxiliary_one, auxiliary_two in families:
            for twin in (1, 2):
                case, oracle = build_multi_operation_case(
                    split, family, first, second, subject, auxiliary_one, auxiliary_two, twin
                )
                cases.append(case); oracles.append(oracle)
        case_path, oracle_path = OUT / f"{split}.jsonl", OUT / f"{split}.oracle.jsonl"
        case_bytes = "".join(json.dumps(row, sort_keys=True) + "\n" for row in cases).encode()
        oracle_bytes = "".join(json.dumps(row, sort_keys=True) + "\n" for row in oracles).encode()
        hashes = hashlib.sha256(case_bytes).hexdigest(), hashlib.sha256(oracle_bytes).hexdigest()
        if (
            split in IMMUTABLE_SPLIT_HASHES
            and hashes != IMMUTABLE_SPLIT_HASHES[split]
        ):
            raise RuntimeError(f"immutable valid-selection split drifted: {split}")
        case_path.write_bytes(case_bytes)
        oracle_path.write_bytes(oracle_bytes)
        manifest["splits"][split] = {"cases": 12, "families": 6,
            "case_sha256": hashes[0], "oracle_sha256": hashes[1]}
    (OUT / "manifest.json").write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
