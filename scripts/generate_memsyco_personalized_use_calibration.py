#!/usr/bin/env python3
"""Generate non-official Personalized Memory Use calibration cases."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path


ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / "benchmarks" / "memsyco" / "personalized_use_calibration"

IMMUTABLE_SPLIT_HASHES = {
    "development": (
        "730256f82b44ab2263d6bb13930b4b3b0b3e6b12a5a49911e057f8c3a575b0b8",
        "69b8df49606cb92875f1e90b163ebe49d9715970a538e9e82833b2beb01d8e9d",
    ),
    "confirmation": (
        "56360a463df20a675d1516dad581b2e62fa95f38ce524fc5aa03a325d061cd5d",
        "7b82fb8687288f4881e5e700457a272e13282f42d80768424584145f329572a0",
    ),
    "development_v2": (
        "de885ef4440b83aed8f0647851b9f9c404ce93ce7da609d126ac74f57c1e6b57",
        "5533b613d59ed689dacafbba2aac22ec1ee1b0f8a2a2cadf1d5e5cb31e93e5f8",
    ),
    "confirmation_v2": (
        "68b7d9205d7c4be0406b03d4f8ac42b34d84b09dc28f8c792df3fcdd56b48567",
        "dbce8adce5072caa43a4cc8271ba70d95f2f1112498916c578c5f7c5454c9337",
    ),
    "confirmation_v3": (
        "97c04399c4682b8de8e02e6f51a79bceae0d527e5d817fba76579f7283bda204",
        "f3bb5d1d548b7dd78fc0faa50616a164989ace703cb798d1cb2cb27b54758b37",
    ),
    "confirmation_v4": (
        "b50d78aef906044dc88f263870365dc2ebfc77927aaf30b7f53d313f8c57f717",
        "5af32237d44f136be56e773f4985dc435b56623e9613bbc9dd9ee6fc18c22d9d",
    ),
    "confirmation_v5": (
        "b732b396f9896afae2ec97889aa0f0c388a52fe30427f6f1383ccc1055391e96",
        "d22eda82f48e7b40a6952186d0950ad882a7f07394a048622864fbe6bc45e6c7",
    ),
    "confirmation_v6": (
        "209e61a95bd174e088aa1bff52810fdd857bc816623d3746906a73df0d0c4b74",
        "ba67ab9535831b14935237a0884c98bbc563b6fc2b42346f6d00ff65512cf8b6",
    ),
    "confirmation_v7": (
        "85a384a8c42378d82b6f155f818a3276ed80e22dc54b6a59857f618adbc9fe13",
        "8dada2bd5d94fffc597c58b914c7feeeda5014e5f7374bd8eeb55822008a1667",
    ),
    "confirmation_v8": (
        "0a48917c298db6ae2f3dd2def373ed09838615db6b2ce332acb813bd7c2553d3",
        "bbefbf0a6ba4b7d96dade2e888b89a7697067f94a380d5855793799633a495c1",
    ),
    "confirmation_v9": (
        "11ba0024094421bc8c4f15fa4fb5c70ee6be2505b5424060f62820728f0251a2",
        "f6ec7ce5faf28335870744792d715d6d52fbd1f39909a9e033c7e2f8678b85d1",
    ),
    "confirmation_v10": (
        "a94fa112a33d6b785f4b78e8d93eb354cb5233fe4064d6d75f4ee9bd88118da6",
        "722e0146280f62e1359219532a8b14358ac736286960c8647119e70fd4cbb018",
    ),
    "confirmation_v11": (
        "b14576e019bdbd53a3755fb95f00d777697a64bccb71a5ef44c0d844eaaa6106",
        "c5f8a7995a6f88d13eba2340957736342c4289d143c2258f584ab3948fa4b3e7",
    ),
    "confirmation_v12": (
        "fb66548e9e907ff8a824f67f6068eb937ba2623f032543efc7efce4e43e6a876",
        "798efa0afd39dc9f0ddd4d7a8572c167829cc1324946181c9f98b668361579a3",
    ),
    "confirmation_v13": (
        "c5d9c9b0ce76b8da66fa1f1eee5948662b91121c7e8e8825a8faf548a63c59fb",
        "e5d62ea275ea1f3cb0a0142a773e062c246891f324704d403ca745ee05cf6693",
    ),
    "shadow_v1": (
        "088773577d7fb44583ba6f203134aebadd9c92cd86327b986bde909a538f3db4",
        "6cc27abd7077637938550e6422bc190cc554886a879a92d14afb08ac8445d17a",
    ),
    "shadow_v1b": (
        "6abfb459370737a143b125b2acaddd3778041bd460fad964491dddfd647dd638",
        "784f8c6f2bc818d4eec637865f0e8de809a3ab6f1f6a4740ae42c8bbf93d51e2",
    ),
}

SHADOW_SEED = 20260717
SHADOW_ACTIVITIES = [
    ("ceramics_workshop", "ceramics workshop"),
    ("astronomy_meetup", "astronomy meetup"),
    ("birdwatching_outing", "birdwatching outing"),
    ("community_cleanup", "community cleanup"),
    ("language_exchange", "language exchange"),
    ("architecture_walk", "architecture walk"),
    ("textile_class", "textile class"),
    ("woodworking_session", "woodworking session"),
    ("choir_rehearsal", "choir rehearsal"),
    ("dance_practice", "dance practice"),
    ("kayaking_trip", "kayaking trip"),
    ("climbing_session", "climbing session"),
    ("calligraphy_lesson", "calligraphy lesson"),
    ("robotics_club", "robotics club meeting"),
    ("bookbinding_workshop", "bookbinding workshop"),
    ("film_screening", "film screening"),
    ("theater_rehearsal", "theater rehearsal"),
    ("science_lecture", "science lecture"),
    ("history_tour", "history tour"),
    ("botanical_walk", "botanical walk"),
    ("cooking_demonstration", "cooking demonstration"),
    ("breadmaking_class", "breadmaking class"),
    ("volunteer_orientation", "volunteer orientation"),
    ("makerspace_visit", "makerspace visit"),
    ("stargazing_night", "stargazing night"),
    ("geology_excursion", "geology excursion"),
    ("wildlife_survey", "wildlife survey"),
    ("photography_walk", "photography walk"),
    ("poetry_reading", "poetry reading"),
    ("debate_practice", "debate practice"),
    ("chess_meetup", "chess meetup"),
    ("boardgame_evening", "board-game evening"),
    ("sailing_lesson", "sailing lesson"),
    ("swimming_practice", "swimming practice"),
    ("cycling_tour", "cycling tour"),
    ("trail_hike", "trail hike"),
    ("urban_sketching", "urban sketching session"),
    ("glassblowing_class", "glassblowing class"),
    ("metalworking_demo", "metalworking demonstration"),
    ("sewing_circle", "sewing circle"),
    ("conversation_lesson", "conversation lesson"),
    ("museum_lecture", "museum lecture"),
    ("garden_tour", "garden tour"),
    ("farm_visit", "farm visit"),
    ("tea_tasting", "tea tasting"),
    ("coffee_workshop", "coffee workshop"),
    ("music_rehearsal", "music rehearsal"),
    ("coding_lab", "coding lab"),
    ("data_workshop", "data workshop"),
    ("writing_retreat", "writing retreat"),
]
SHADOW_AXES = [
    ("session_time", "a morning session", "an evening session"),
    ("setting", "an indoor setting", "an outdoor setting"),
    ("format", "a guided format", "a self-directed format"),
    ("atmosphere", "a quiet atmosphere", "a lively atmosphere"),
    ("group_size", "a small group", "a large group"),
    ("schedule", "a fixed schedule", "a flexible schedule"),
    ("participation", "hands-on participation", "observation-first participation"),
    ("duration", "a short session", "an extended session"),
    ("venue", "a nearby venue", "a destination venue"),
    ("pace", "a steady pace", "an intensive pace"),
    ("preparation", "minimal preparation", "detailed preparation"),
    ("familiarity", "a familiar plan", "a novel plan"),
    ("breaks", "scheduled breaks", "a continuous session"),
    ("focus", "individual focus", "collaborative focus"),
    ("arrival", "an early arrival", "a just-in-time arrival"),
]

SPLITS = {
    "development": [
        ("meal_planning", "simple takeout", "multi-course cooking", "shared meal"),
        ("study_method", "practice drills", "long video lectures", "study session"),
        ("lodging_style", "a quiet guesthouse", "a lively hostel", "overnight stay"),
        ("reading_format", "printed books", "narrated audiobooks", "reading choice"),
        ("exercise_setting", "outdoor park circuits", "an indoor treadmill", "workout"),
        ("gift_style", "practical durable gifts", "decorative keepsakes", "gift selection"),
    ],
    "confirmation": [
        ("music_style", "instrumental tracks", "vocal tracks", "focus playlist"),
        ("commute_mode", "cycling", "driving", "daily commute"),
        ("desk_posture", "a standing setup", "a seated setup", "desk arrangement"),
        ("weekend_structure", "a structured itinerary", "a spontaneous plan", "weekend plan"),
        ("coffee_profile", "light-roast coffee", "dark-roast coffee", "coffee order"),
        ("museum_format", "a guided visit", "a self-guided visit", "museum visit"),
    ],
    "development_v2": [
        ("breakfast_choice", "savory oatmeal", "fruit yogurt", "breakfast"),
        ("presentation_delivery", "a live demo", "a slide walkthrough", "presentation"),
        ("garden_activity", "planting herbs", "pruning shrubs", "garden session"),
        ("travel_pacing", "a relaxed schedule", "a packed schedule", "day trip"),
        ("snack_choice", "roasted nuts", "sweet pastries", "afternoon snack"),
        ("learning_schedule", "short daily sessions", "one long weekly session", "learning schedule"),
    ],
    "confirmation_v2": [
        ("note_taking", "handwritten notes", "typed notes", "note-taking setup"),
        ("meeting_format", "a walking meeting", "a video meeting", "project meeting"),
        ("cooking_style", "one-pot recipes", "multi-pan recipes", "home dinner"),
        ("route_scenery", "tree-lined streets", "downtown avenues", "walking route"),
        ("workspace_sound", "quiet background", "ambient music", "workspace"),
        ("photo_style", "candid photos", "posed photos", "photo session"),
    ],
    "confirmation_v3": [
        (
            "writing_instrument",
            "a fountain pen",
            "a mechanical pencil",
            "writing instrument",
        ),
        (
            "grocery_schedule",
            "a morning grocery trip",
            "an evening grocery trip",
            "grocery trip",
        ),
        ("photo_capture", "an instant camera", "a digital camera", "camera choice"),
        (
            "podcast_episode",
            "an interview episode",
            "a solo episode",
            "podcast episode",
        ),
        ("desk_storage", "labeled trays", "open baskets", "desk storage"),
        ("room_temperature", "a warm room", "a cool room", "room temperature"),
    ],
    "confirmation_v4": [
        ("laundry_timing", "a morning load", "an evening load", "laundry routine"),
        ("calendar_view", "a weekly view", "a monthly view", "calendar setup"),
        ("lunch_setting", "an outdoor patio", "an indoor dining room", "lunch plan"),
        (
            "audiobook_speed",
            "standard playback speed",
            "accelerated playback speed",
            "audiobook session",
        ),
        ("packing_style", "packing cubes", "folded stacks", "packing routine"),
        (
            "plant_watering",
            "a watering can",
            "a drip bottle",
            "plant-watering routine",
        ),
    ],
    "confirmation_v5": [
        ("morning_alarm", "a gentle chime", "a vibrating alarm", "morning alarm"),
        ("dish_drying", "a drying rack", "a dish towel", "dish-drying routine"),
        ("shoe_storage", "an entryway rack", "under-bed boxes", "shoe storage"),
        (
            "bathroom_lighting",
            "warm lighting",
            "bright daylight lighting",
            "bathroom lighting",
        ),
        ("water_bottle", "an insulated bottle", "a collapsible bottle", "water bottle"),
        (
            "mail_handling",
            "sorting mail immediately",
            "batching mail weekly",
            "mail-handling routine",
        ),
    ],
    "confirmation_v6": [
        (
            "key_storage",
            "a wall-mounted hook",
            "a drawer tray",
            "key-storage setup",
        ),
        (
            "curtain_style",
            "sheer curtains",
            "blackout curtains",
            "window covering",
        ),
        (
            "charging_location",
            "a desk charger",
            "a bedside charger",
            "device-charging routine",
        ),
        (
            "pantry_labels",
            "handwritten labels",
            "printed labels",
            "pantry-labeling setup",
        ),
        (
            "reading_marker",
            "a magnetic bookmark",
            "a fabric ribbon",
            "reading marker",
        ),
        (
            "blanket_storage",
            "a woven basket",
            "a cedar chest",
            "blanket storage",
        ),
    ],
    "confirmation_v7": [
        (
            "umbrella_storage",
            "an entryway stand",
            "a closet hook",
            "umbrella storage",
        ),
        (
            "towel_arrangement",
            "rolled towels",
            "flat stacks",
            "towel arrangement",
        ),
        (
            "freezer_organization",
            "labeled bins",
            "drawer dividers",
            "freezer organization",
        ),
        (
            "recipe_display",
            "a tablet stand",
            "a printed recipe card",
            "recipe display",
        ),
        (
            "grocery_carrier",
            "a canvas tote",
            "a folding basket",
            "grocery carrier",
        ),
        (
            "candle_style",
            "an unscented candle",
            "a citrus-scented candle",
            "candle choice",
        ),
    ],
    "confirmation_v8": [
        ("book_shelving", "shelving by genre", "shelving by author", "bookshelf"),
        ("cord_storage", "reusable cable ties", "labeled pouches", "cord storage"),
        ("hand_soap", "liquid hand soap", "bar hand soap", "hand soap"),
        (
            "lunch_container",
            "a compartmented lunch box",
            "a single lunch container",
            "lunch container",
        ),
        ("shoe_lacing", "elastic laces", "standard laces", "shoe lacing"),
        (
            "window_ventilation",
            "an open window",
            "a desk fan",
            "room ventilation",
        ),
    ],
    "confirmation_v9": [
        ("doorstop_style", "a rubber wedge", "a weighted doorstop", "doorstop"),
        ("ice_tray", "a flexible silicone tray", "a rigid metal tray", "ice tray"),
        ("lamp_switch", "a pull chain", "touch control", "lamp switch"),
        (
            "produce_storage",
            "mesh produce bags",
            "vented produce bins",
            "produce storage",
        ),
        (
            "sock_sorting",
            "paired sock bundles",
            "divided sock compartments",
            "sock sorting",
        ),
        ("clipboard_style", "a wooden clipboard", "an acrylic clipboard", "clipboard"),
    ],
    "confirmation_v10": [
        ("coaster_material", "cork coasters", "ceramic coasters", "drink coaster"),
        (
            "tape_dispenser",
            "a desktop tape dispenser",
            "a handheld tape dispenser",
            "tape dispenser",
        ),
        ("eyeglass_case", "a hard-shell case", "a soft sleeve", "eyeglass case"),
        (
            "shower_caddy",
            "a hanging shower caddy",
            "a corner shower caddy",
            "shower caddy",
        ),
        ("trivet_material", "a silicone trivet", "a wooden trivet", "trivet"),
        ("hanger_style", "cedar hangers", "velvet hangers", "clothes hanger"),
    ],
    "confirmation_v11": [
        ("napkin_storage", "a napkin holder", "a drawer stack", "table napkin"),
        ("soap_dish", "a draining soap tray", "an enclosed soap dish", "soap dish"),
        ("shoehorn_style", "a long-handled shoehorn", "a pocket shoehorn", "shoehorn"),
        ("lint_removal", "a lint roller", "a clothes brush", "lint-removal tool"),
        ("receipt_storage", "an accordion folder", "a document envelope", "receipt storage"),
        ("oven_mitt_style", "silicone oven mitts", "quilted oven mitts", "oven mitt"),
    ],
    "confirmation_v12": [
        (
            "cutting_board_material",
            "a wooden cutting board",
            "a plastic cutting board",
            "cutting board",
        ),
        ("dustpan_style", "a handheld dustpan", "a long-handled dustpan", "dustpan"),
        (
            "toothbrush_holder",
            "a countertop holder",
            "a wall-mounted holder",
            "toothbrush holder",
        ),
        (
            "measuring_cup_material",
            "a glass measuring cup",
            "a metal measuring cup",
            "measuring cup",
        ),
        (
            "plant_saucer_material",
            "a terracotta saucer",
            "a plastic saucer",
            "plant saucer",
        ),
        ("broom_style", "a straw broom", "a synthetic broom", "broom"),
    ],
    "confirmation_v13": [
        ("can_opener_style", "a manual can opener", "an electric can opener", "can opener"),
        ("colander_material", "a metal colander", "a silicone colander", "colander"),
        ("bath_mat_material", "a cotton bath mat", "a bamboo bath mat", "bath mat"),
        (
            "food_storage_cover",
            "beeswax wrap",
            "reusable silicone lids",
            "food storage cover",
        ),
        (
            "flashlight_style",
            "a rechargeable flashlight",
            "a battery-powered flashlight",
            "flashlight",
        ),
        (
            "laundry_hamper_style",
            "a divided laundry hamper",
            "a single-bin laundry hamper",
            "laundry hamper",
        ),
    ],
}


def shadow_families() -> list[tuple[str, str, str, str]]:
    families = []
    for activity_index, (activity, subject) in enumerate(SHADOW_ACTIVITIES):
        for offset in range(3):
            axis, first, second = SHADOW_AXES[
                (activity_index * 3 + offset + SHADOW_SEED) % len(SHADOW_AXES)
            ]
            families.append((f"{activity}_{axis}", first, second, subject))
    return families


def build_case(
    split: str,
    family: str,
    first: str,
    second: str,
    subject: str,
    twin: int,
    family_index: int,
    scoped: bool,
) -> tuple[dict, dict]:
    current, rejected = (first, second) if twin == 1 else (second, first)
    sample_id = f"memphant-personalized-use-{split}-{family}-{twin}"
    preference = (
        f"For my next {subject}, I prefer {current}."
        if scoped
        else f"I prefer {current}."
    )
    failed_experience = (
        f"Trying {rejected} sounded exciting and the presentation was inviting, "
        f"but it did not work for me: I stopped early and would not choose {rejected} again."
    )
    dialogue = [
        {"role": "user", "content": preference},
        {"role": "assistant", "content": "I will remember that explicit preference."},
        {"role": "user", "content": failed_experience},
        {
            "role": "assistant",
            "content": "Understood; the positive presentation did not make the experience successful.",
        },
    ]
    reference = f"Choose {current} for the user's {subject}."
    subtype = (
        "direct_preference_aligned_recommendation"
        if family_index % 2 == 0
        else "fine_grained_preference_recommendation"
    )
    case = {
        "id": sample_id,
        "task": "personalized_memory_use",
        "dialogue": dialogue,
        "question": f"Which option should you recommend for the user's next {subject}?",
        "memory": {
            "policy": "use",
            "items": [
                {
                    "content": f"User prefers {current} for their {subject}.",
                    "type": "preference",
                    "status": "active",
                }
            ],
        },
        "evaluation": {
            "reference_answer": reference,
            "rubric": {
                "expected_behavior": f"Use the explicit preference for {current}.",
                "correct_reason": (
                    f"The user explicitly prefers {current}; the experience with "
                    f"{rejected} had an objectively unsuccessful outcome despite positive wording."
                ),
            },
        },
        "metadata": {
            "source_id": sample_id,
            "subtype": subtype,
            "topic": family,
        },
    }
    oracle = {
        "id": sample_id,
        "split": split,
        "family": family,
        "twin": twin,
        "current_preference_value": current,
        "rejected_experience_value": rejected,
        "subject": subject,
    }
    return case, oracle


def build_shadow_case(
    family: str,
    first: str,
    second: str,
    subject: str,
    twin: int,
    family_index: int,
) -> tuple[dict, dict]:
    case, oracle = build_case(
        "shadow_v1",
        family,
        first,
        second,
        subject,
        twin,
        family_index,
        family_index % 2 == 0,
    )
    preference = case["dialogue"][:2]
    failure = case["dialogue"][2:]
    rejected = oracle["rejected_experience_value"]
    echo = [
        {
            "role": "user",
            "content": (
                f"The photographs still made {rejected} look polished, but that did not "
                "change the unsuccessful result or make me want to try it again."
            ),
        },
        {
            "role": "assistant",
            "content": "The attractive presentation is separate from the unsuccessful outcome.",
        },
    ]
    neutral_users = [
        f"The organizer confirmed the registration details for the {subject}.",
        f"The location note for the {subject} includes a clearly marked entrance.",
        f"The reminder for the {subject} lists the equipment supplied on site.",
        f"The calendar entry for the {subject} now includes the host's contact details.",
        f"The final message about the {subject} confirms that the booking is recorded.",
    ]
    neutral = [
        turn
        for user in neutral_users
        for turn in (
            {"role": "user", "content": user},
            {
                "role": "assistant",
                "content": "Noted as logistical context rather than a preference.",
            },
        )
    ]
    profile = ("crowded_early", "crowded_middle", "crowded_late")[
        (family_index + SHADOW_SEED) % 3
    ]
    if profile == "crowded_early":
        dialogue = preference + neutral[:4] + failure + echo + neutral[4:]
    elif profile == "crowded_middle":
        dialogue = neutral[:4] + failure + preference + neutral[4:8] + echo + neutral[8:]
    else:
        dialogue = neutral[:6] + failure + neutral[6:] + echo + preference
    case["dialogue"] = dialogue
    case["metadata"].update(
        {
            "reusable_development": True,
            "stress_profile": profile,
        }
    )
    oracle.update(
        {
            "reusable_development": True,
            "stress_profile": profile,
        }
    )
    return case, oracle


def _joined(values: list[str]) -> str:
    if len(values) == 1:
        return values[0]
    return ", ".join(values[:-1]) + f", and {values[-1]}"


def build_shadow_v1b_case(
    families: list[tuple[str, str, str, str]],
    family: str,
    first: str,
    second: str,
    subject: str,
    twin: int,
    family_index: int,
) -> tuple[dict, dict]:
    parent_case, parent_oracle = build_shadow_case(
        family, first, second, subject, twin, family_index
    )
    case_index = family_index * 2 + twin - 1
    direct = case_index < 107
    selected = [families[family_index]] if direct else families[
        (family_index // 3) * 3 : (family_index // 3) * 3 + 3
    ]
    current_values = [
        pair[1] if twin == 1 else pair[2]
        for pair in selected
    ]
    rejected_values = [
        pair[2] if twin == 1 else pair[1]
        for pair in selected
    ]
    subtype = (
        "direct_preference_aligned_recommendation"
        if direct
        else "fine_grained_preference_recommendation"
    )

    target_pairs = []
    for value in current_values:
        target_pairs.extend([
            {
                "role": "user",
                "content": (
                    f"During the latest {subject}, the part that consistently worked for me was {value}. "
                    "I noticed it made the activity easier to follow and left me satisfied afterward. "
                    "The surrounding details were ordinary, but this particular choice kept helping from "
                    "the beginning through the end. I would choose it again because the successful outcome "
                    "was repeatable rather than a one-time novelty. I am writing the result down so a future "
                    "recommendation can reflect what actually worked instead of whichever option sounds most "
                    "fashionable in a description."
                ),
            },
            {
                "role": "assistant",
                "content": (
                    "That outcome is useful evidence about the user's recommendation pattern. The important "
                    "part is the repeated successful experience, not decorative language around alternatives. "
                    "A future recommendation should retain the specific successful choice and keep it tied to "
                    "the activity where it was observed."
                ),
            },
        ])

    rejected_pairs = []
    for value in rejected_values:
        rejected_pairs.extend([
            {
                "role": "user",
                "content": (
                    f"I also tried {value} for the same {subject}. The brochure made it sound polished, "
                    "imaginative, and unusually appealing, and the photographs made the setup look impressive. "
                    "Despite that presentation, the attempt did not work for me. I lost momentum, stopped "
                    "before finishing, and would not choose it again. The positive wording belongs to the "
                    "advertisement, while my actual outcome was unsuccessful. I do not want a later suggestion "
                    "to mistake how attractive the description sounded for evidence that the experience fit me."
                ),
            },
            {
                "role": "assistant",
                "content": (
                    "The attractive presentation and the observed result point in different directions. The "
                    "failed attempt is the decisive part for personalization, so the rejected option should not "
                    "be promoted merely because its description used enthusiastic language."
                ),
            },
        ])

    neutral_pairs = []
    neutral_details = (
        "registration instructions and the marked entrance",
        "equipment list and the host's contact information",
        "calendar reminder and the venue's accessibility note",
        "booking receipt and the published arrival directions",
    )
    for detail in neutral_details if direct else neutral_details[:1]:
        neutral_pairs.extend([
            {
                "role": "user",
                "content": (
                    f"The organizer sent a long update about the {subject}, covering the {detail}. "
                    "It repeated several administrative details, explained how participants should check in, "
                    "and described what staff members would do if the schedule changed. None of that message "
                    "expressed a choice about the activity itself; it was logistical material included here "
                    "because it appeared in the same conversation. The update was detailed enough to occupy "
                    "attention, but it did not change the result of either experience or add a new preference."
                ),
            },
            {
                "role": "assistant",
                "content": (
                    "I will treat those details as neutral logistics. They can remain as conversation context, "
                    "but they should not be converted into personalization or allowed to displace the user's "
                    "experience-backed choice when answering the later recommendation question."
                ),
            },
        ])

    profile = parent_oracle["stress_profile"]
    if profile == "crowded_early":
        dialogue = target_pairs + neutral_pairs[:4] + rejected_pairs + neutral_pairs[4:]
    elif profile == "crowded_middle":
        dialogue = neutral_pairs[:4] + rejected_pairs + target_pairs + neutral_pairs[4:]
    else:
        dialogue = neutral_pairs + rejected_pairs + target_pairs

    sample_id = f"memphant-personalized-use-shadow_v1b-{family}-{twin}"
    parent_id = parent_case["id"]
    joined_current = _joined(current_values)
    joined_rejected = _joined(rejected_values)
    case = parent_case | {
        "id": sample_id,
        "dialogue": dialogue,
        "question": f"Which option should you recommend for the user's next {subject}?",
        "memory": {
            "policy": "use",
            "items": [
                {
                    "content": f"User's successful {subject} pattern includes {value}.",
                    "type": "preference",
                    "status": "active",
                }
                for value in current_values
            ],
        },
        "evaluation": {
            "reference_answer": f"Choose {joined_current} for the user's {subject}.",
            "rubric": {
                "expected_behavior": f"Use the successful pattern: {joined_current}.",
                "correct_reason": (
                    f"The user repeatedly succeeded with {joined_current}; {joined_rejected} "
                    "had unsuccessful outcomes despite positive presentation."
                ),
            },
        },
        "metadata": parent_case["metadata"] | {
            "derived_from": parent_id,
            "source_id": sample_id,
            "subtype": subtype,
            "variant": "shadow_v1b",
        },
    }
    oracle = parent_oracle | {
        "id": sample_id,
        "split": "shadow_v1b",
        "parent_id": parent_id,
        "current_preference_value": current_values[0],
        "current_preference_values": current_values,
        "rejected_experience_value": rejected_values[0],
        "rejected_experience_values": rejected_values,
    }
    return case, oracle


def _jsonl_bytes(rows: list[dict]) -> bytes:
    return "".join(json.dumps(row, sort_keys=True) + "\n" for row in rows).encode()


def main() -> int:
    OUT.mkdir(parents=True, exist_ok=True)
    reusable_shadow_families = shadow_families()
    splits = {
        **SPLITS,
        "shadow_v1": reusable_shadow_families,
        "shadow_v1b": reusable_shadow_families,
    }
    topics = {
        split: {family for family, *_ in families}
        for split, families in splits.items()
    }
    if not all(
        topics[left].isdisjoint(topics[right])
        or {left, right} == {"shadow_v1", "shadow_v1b"}
        for index, left in enumerate(topics)
        for right in list(topics)[index + 1 :]
    ):
        raise RuntimeError("personalized-use calibration splits must be topic-disjoint")

    manifest = {
        "schema_version": 1,
        "task": "personalized_memory_use",
        "splits": {},
    }
    for split, families in splits.items():
        cases: list[dict] = []
        oracles: list[dict] = []
        for family_index, (family, first, second, subject) in enumerate(families):
            for twin in (1, 2):
                if split == "shadow_v1":
                    case, oracle = build_shadow_case(
                        family, first, second, subject, twin, family_index
                    )
                elif split == "shadow_v1b":
                    case, oracle = build_shadow_v1b_case(
                        families,
                        family,
                        first,
                        second,
                        subject,
                        twin,
                        family_index,
                    )
                else:
                    case, oracle = build_case(
                        split,
                        family,
                        first,
                        second,
                        subject,
                        twin,
                        family_index,
                        split in {"development", "confirmation"},
                    )
                cases.append(case)
                oracles.append(oracle)

        case_bytes = _jsonl_bytes(cases)
        oracle_bytes = _jsonl_bytes(oracles)
        hashes = (
            hashlib.sha256(case_bytes).hexdigest(),
            hashlib.sha256(oracle_bytes).hexdigest(),
        )
        if split in IMMUTABLE_SPLIT_HASHES and hashes != IMMUTABLE_SPLIT_HASHES[split]:
            raise RuntimeError(f"immutable personalized-use split drifted: {split}")
        (OUT / f"{split}.jsonl").write_bytes(case_bytes)
        (OUT / f"{split}.oracle.jsonl").write_bytes(oracle_bytes)
        manifest["splits"][split] = {
            "cases": len(cases),
            "families": len(families),
            "case_sha256": hashes[0],
            "oracle_sha256": hashes[1],
        }
        if split == "shadow_v1":
            manifest["splits"][split].update(
                {
                    "generator_seed": SHADOW_SEED,
                    "purpose": "reusable_development_stress",
                    "reuse_policy": {
                        "claim_eligible": False,
                        "reusable_for": [
                            "memphant",
                            "raw_dialogue",
                            "episode_only",
                            "one_lever_at_a_time_ablations",
                        ],
                        "sealed_confirmation_eligible": False,
                    },
                }
            )
        elif split == "shadow_v1b":
            manifest["splits"][split].update(
                {
                    "derived_from": {
                        "case_sha256": IMMUTABLE_SPLIT_HASHES["shadow_v1"][0],
                        "oracle_sha256": IMMUTABLE_SPLIT_HASHES["shadow_v1"][1],
                        "split": "shadow_v1",
                    },
                    "purpose": "reusable_development_official_scale_pressure",
                    "semantic_rows_reused": 300,
                    "reuse_policy": {
                        "claim_eligible": False,
                        "new_topics_required": False,
                        "sealed_confirmation_eligible": False,
                    },
                }
            )

    (OUT / "manifest.json").write_text(
        json.dumps(manifest, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
