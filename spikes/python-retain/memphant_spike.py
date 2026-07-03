from __future__ import annotations

import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any


@dataclass(frozen=True)
class ExtractionRule:
    contains: str
    subject: str
    value: str


@dataclass(frozen=True)
class ExtractionPolicy:
    compiler_version: str
    rules: tuple[ExtractionRule, ...]


@dataclass(frozen=True)
class Episode:
    body: str


@dataclass(frozen=True)
class GoldenCase:
    case_id: str
    episodes: tuple[Episode, ...]
    expected: tuple[str, ...]


def load_policy(path: Path) -> ExtractionPolicy:
    raw = json.loads(path.read_text(encoding="utf-8"))
    return ExtractionPolicy(
        compiler_version=raw["compiler_version"],
        rules=tuple(ExtractionRule(**rule) for rule in raw["rules"]),
    )


def load_golden(path: Path) -> tuple[GoldenCase, ...]:
    cases: list[GoldenCase] = []
    for line in path.read_text(encoding="utf-8").splitlines():
        raw: dict[str, Any] = json.loads(line)
        cases.append(
            GoldenCase(
                case_id=raw["id"],
                episodes=tuple(Episode(**episode) for episode in raw["episodes"]),
                expected=tuple(raw["expected"]),
            )
        )
    return tuple(cases)


def retain_episode(policy: ExtractionPolicy, episode: Episode) -> tuple[str, ...]:
    body = episode.body.lower()
    extracted = [
        f"{rule.subject}:{rule.value}"
        for rule in policy.rules
        if rule.contains.lower() in body
    ]
    return tuple(extracted)


def run_golden(
    policy: ExtractionPolicy, cases: tuple[GoldenCase, ...]
) -> dict[str, list[str]]:
    results: dict[str, list[str]] = {}
    for case in cases:
        extracted: list[str] = []
        for episode in case.episodes:
            extracted.extend(retain_episode(policy, episode))
        if tuple(extracted) != case.expected:
            raise AssertionError(
                f"{case.case_id}: expected {case.expected}, extracted {tuple(extracted)}"
            )
        results[case.case_id] = extracted
    return results
