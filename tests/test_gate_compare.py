from __future__ import annotations

import importlib.util
import hashlib
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def _load_run_reader():
    spec = importlib.util.spec_from_file_location(
        "run_reader_gate_contract", ROOT / "scripts" / "run_reader.py"
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _load_gate_compare():
    spec = importlib.util.spec_from_file_location(
        "gate_compare_contract", ROOT / "scripts" / "gate_compare.py"
    )
    module = importlib.util.module_from_spec(spec)
    assert spec.loader is not None
    spec.loader.exec_module(module)
    return module


def _reader_report(
    ids=("q1", "q2"), fingerprint="eval-v1", *, evidence_sha="e" * 64,
    retrieval_sha="d" * 64,
):
    rows = [
        {
            "question_id": qid,
            "question_type": "multi-session",
            "question": f"question-{qid}",
            "question_date": None,
            "is_abstention": False,
            "gold_answer": f"answer-{qid}",
            "gold_answers": [f"answer-{qid}"],
            "correct": True,
        }
        for qid in ids
    ]
    evaluator = {
        "engine": "codex",
        "reader_model_id": "reader-v1",
        "judge_model_id": "judge-v1",
        "reasoning_effort": "medium",
        "evaluator_source_sha256": "f" * 64,
        "source_evidence_sha256": evidence_sha,
        "retrieval_report_sha256": retrieval_sha,
    }
    evaluator["sha256"] = hashlib.sha256(
        json.dumps(evaluator, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    if fingerprint != "eval-v1":
        evaluator["sha256"] = fingerprint
    report = {
        "engine": "codex",
        "reader_model_id": "reader-v1",
        "judge_model_id": "judge-v1",
        "reasoning_effort": "medium",
        "expected_n": len(rows),
        "source_expected_n": len(rows),
        "evaluated_expected_n": len(rows),
        "smoke_only": False,
        "promotion_ineligible": False,
        "complete": True,
        "aborted": None,
        "errors": {"reader": 0, "parse": 0, "judge": 0},
        "evaluator_fingerprint": evaluator,
        "source_evidence_sha256": evidence_sha,
        "evaluated_evidence_sha256": evidence_sha,
        "retrieval_report_sha256": retrieval_sha,
        "question_set_sha256": "questions-v1",
        "per_question": rows,
    }
    report["reader_report_sha256"] = hashlib.sha256(
        json.dumps(report, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    return report


def _provenance_report(ids=("q1", "q2"), runtime="runtime-v1"):
    source_tree = {
        "git_commit": "3" * 40,
        "tracked_tree_sha256": "1" * 64,
    }
    source_tree["sha256"] = hashlib.sha256(
        json.dumps(source_tree, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    database = {
        "schema_sha256": "4" * 64,
        "extensions_and_migrations_sha256": "5" * 64,
    }
    database["sha256"] = hashlib.sha256(
        json.dumps(database, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    migration_files = {"001.sql": "6" * 64}
    generation = {
        "source_tree": source_tree,
        "files": {"runner": "2" * 64},
        "database": database,
        "migration_sources": {
            "files": migration_files,
            "sha256": hashlib.sha256(
                json.dumps(migration_files, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest(),
        },
    }
    generation["sha256"] = hashlib.sha256(
        json.dumps(generation, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    runtime_config = {
        "runtime": runtime,
        "generation_identity": generation,
        "k": 10,
        "budget_tokens": 8192,
        "evidence_packer": {
            "name": "test",
            "sha256": hashlib.sha256(b'{"name":"test"}').hexdigest(),
        },
        "golden_revision": "sha256:" + "a" * 64,
        "corpus_revision": "sha256:" + "b" * 64,
    }
    rows = [
        {
            "question_id": qid,
            "hit_at_5": True,
            "hit_at_10": True,
            "degraded": False,
            "fallback": False,
            "skipped": False,
            "failure": "none",
            "recall_e2e_ms": 10,
            "packed_items": 1,
            "evidence_packer_sha256": hashlib.sha256(b'{"name":"test"}').hexdigest(),
            "evidence_budget_tokens": 8192,
            "evidence_packed_tokens": 10,
            "evidence_truncated_items": 0,
            "evidence_dropped_items": 0,
        }
        for qid in ids
    ]
    report = {
        "runtime_config_fingerprint": hashlib.sha256(
            json.dumps(runtime_config, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest(),
        "runtime_config": runtime_config,
        "golden_sha256": "a" * 64,
        "golden_revision": "sha256:" + "a" * 64,
        "corpus_revision": "sha256:" + "b" * 64,
        "expected_n": len(ids),
        "golden_count": len(ids),
        "evidence_sha256": "c" * 64,
        "degraded_count": 0,
        "fallback_count": 0,
        "skipped_count": 0,
        "reranker_failure_count": 0,
        "recall_at_5": 1.0,
        "recall_at_10": 1.0,
        "recall_e2e_ms_p50": 10.0,
        "recall_e2e_ms_p95": 10.0,
        "recall_e2e_p95_ceiling_ms": 1500,
        "recall_e2e_p95_within_ceiling": True,
        "per_question": rows,
    }
    report["provenance_sha256"] = hashlib.sha256(
        json.dumps(report, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    return report


def _with_negative(provenance: dict, *, eligible: bool = True) -> dict:
    provenance["negative"] = {
        "negative_case_count": 10,
        "negative_forbidden_hit_count": 0,
        "negative_forbidden_hit_rate": 0.0,
        "negative_unsupported_count": 0,
        "negative_promotion_eligible": eligible,
        "negative_evidence_sha256": "9" * 64,
        "negative_per_case": [
            {
                "case_id": f"neg-{index}",
                "case_kind": f"kind-{index}",
                "supported": True,
                "unsupported_reason": None,
                "forbidden_hits": 0,
                "passed": True,
            }
            for index in range(10)
        ],
    }
    provenance["provenance_sha256"] = hashlib.sha256(
        json.dumps(
            {key: value for key, value in provenance.items() if key != "provenance_sha256"},
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ).hexdigest()
    return provenance


def _negative_reader_report(provenance: dict, provenance_bytes: bytes) -> dict:
    rows = provenance["negative"]["negative_per_case"]
    report = _reader_report(
        tuple(row["case_id"] for row in rows),
        evidence_sha=provenance["negative"]["negative_evidence_sha256"],
        retrieval_sha=hashlib.sha256(provenance_bytes).hexdigest(),
    )
    for result, case in zip(report["per_question"], rows):
        result.update(
            question_type=case["case_kind"],
            is_abstention=True,
            gold_answer="ABSTAIN",
            gold_answers=["ABSTAIN"],
            correct=True,
            judge_method="abstention_exact",
        )
    report["reader_report_sha256"] = hashlib.sha256(
        json.dumps(
            {key: value for key, value in report.items() if key != "reader_report_sha256"},
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ).hexdigest()
    return report


def test_negative_admission_binds_nested_evidence_and_exact_abstention() -> None:
    comparator = _load_gate_compare()
    mem_prov = _with_negative(_provenance_report(runtime="mem"))
    syn_prov = _with_negative(_provenance_report(runtime="syn"))
    mem_bytes = (json.dumps(mem_prov) + "\n").encode()
    syn_bytes = (json.dumps(syn_prov) + "\n").encode()
    mem_reader = _negative_reader_report(mem_prov, mem_bytes)
    syn_reader = _negative_reader_report(syn_prov, syn_bytes)

    summary = comparator._validate_negative_admission(
        mem_prov, syn_prov, mem_bytes, syn_bytes, mem_reader, syn_reader
    )
    assert summary["memphant"]["joint_pass_count"] == 10
    assert summary["syndai"]["joint_pass_count"] == 10

    syn_prov["negative"]["negative_per_case"][0].update(forbidden_hits=1, passed=False)
    syn_prov["negative"].update(
        negative_forbidden_hit_count=1,
        negative_forbidden_hit_rate=0.1,
        negative_promotion_eligible=False,
    )
    syn_reader["per_question"][0]["correct"] = False
    syn_reader["reader_report_sha256"] = hashlib.sha256(
        json.dumps(
            {key: value for key, value in syn_reader.items() if key != "reader_report_sha256"},
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ).hexdigest()
    summary = comparator._validate_negative_admission(
        mem_prov, syn_prov, mem_bytes, syn_bytes, mem_reader, syn_reader
    )
    assert summary["memphant"]["joint_pass_count"] == 10
    assert summary["syndai"]["joint_pass_count"] == 9
    assert summary["candidate_minus_incumbent"]["joint_pass_count"] == 1

    mem_prov["negative"]["negative_per_case"][0].update(
        supported=False,
        unsupported_reason="missing_contract",
        passed=False,
    )
    mem_prov["negative"].update(
        negative_unsupported_count=1,
        negative_promotion_eligible=False,
    )
    try:
        comparator._validate_negative_admission(
            mem_prov, syn_prov, mem_bytes, syn_bytes, mem_reader, syn_reader
        )
        raise AssertionError("expected unsupported negative rejection")
    except ValueError as error:
        assert "unsupported" in str(error)


def test_reader_pairing_rejects_unequal_duplicate_and_mismatched_evaluator() -> None:
    reader = _load_run_reader()
    for left, right in [
        (_reader_report(), _reader_report(("q1",))),
        (_reader_report(("q1", "q1")), _reader_report()),
        (_reader_report(), _reader_report(fingerprint="eval-v2")),
    ]:
        try:
            reader.validate_and_pair_reports(left, right, "reader")
            raise AssertionError("expected invalid report pair")
        except ValueError:
            pass


def test_reader_pairing_rejects_non_bool_incomplete_or_erroring_reports() -> None:
    reader = _load_run_reader()
    invalid = _reader_report()
    invalid["per_question"][0]["correct"] = 1
    incomplete = _reader_report()
    incomplete["complete"] = False
    erroring = _reader_report()
    erroring["errors"]["parse"] = 1
    missing_fingerprint = _reader_report()
    del missing_fingerprint["evaluator_fingerprint"]
    missing_evaluation_input = _reader_report()
    for row in missing_evaluation_input["per_question"]:
        del row["question_type"]
    for report in (
        invalid,
        incomplete,
        erroring,
        missing_fingerprint,
        missing_evaluation_input,
    ):
        try:
            reader.validate_and_pair_reports(report, _reader_report(), "reader")
            raise AssertionError("expected invalid reader report")
        except ValueError:
            pass


def test_reader_pairing_recomputes_fingerprint_and_matches_immutable_inputs() -> None:
    reader = _load_run_reader()
    tampered_fingerprint = _reader_report()
    tampered_fingerprint["evaluator_fingerprint"]["judge_model_id"] = "judge-v2"
    mismatched_type = _reader_report()
    mismatched_type["per_question"][0]["question_type"] = "knowledge-update"
    mismatched_abstention = _reader_report()
    mismatched_abstention["per_question"][0]["is_abstention"] = True
    mismatched_gold = _reader_report()
    mismatched_gold["per_question"][0]["gold_answer"] = "different"
    mismatched_exact_golds = _reader_report()
    mismatched_exact_golds["per_question"][0]["gold_answers"] = ["different"]
    for invalid in (
        tampered_fingerprint,
        mismatched_type,
        mismatched_abstention,
        mismatched_gold,
        mismatched_exact_golds,
    ):
        try:
            reader.validate_and_pair_reports(invalid, _reader_report(), "reader")
            raise AssertionError("expected immutable input mismatch")
        except ValueError:
            pass


def test_provenance_pairing_allows_distinct_runtime_fingerprints_only() -> None:
    reader = _load_run_reader()
    pairs = reader.validate_and_pair_reports(
        _provenance_report(runtime="memphant-v1"),
        _provenance_report(runtime="syndai-v1"),
        "provenance",
    )
    assert [qid for qid, _, _ in pairs] == ["q1", "q2"]
    invalid = _provenance_report(runtime="")
    invalid["runtime_config_fingerprint"] = ""
    try:
        reader.validate_and_pair_reports(invalid, _provenance_report(), "provenance")
        raise AssertionError("expected missing immutable runtime fingerprint")
    except ValueError:
        pass


def test_gate_compare_consumes_both_runner_schema_and_binds_reader_artifacts(
    tmp_path: Path, monkeypatch
) -> None:
    comparator = _load_gate_compare()
    golden = tmp_path / "golden.jsonl"
    golden.write_text(
        "".join(
            json.dumps({"question_id": qid, "provenance": [{"file": file}]}) + "\n"
            for qid, file in (("q1", "docs/a.md"), ("q2", "docs/b.md"))
        )
    )
    golden_sha = hashlib.sha256(golden.read_bytes()).hexdigest()

    reports = []
    paths = []
    for name in ("memphant", "syndai"):
        report = _with_negative(_provenance_report(runtime=name))
        if name == "syndai":
            report["negative"]["negative_per_case"][0].update(
                forbidden_hits=1, passed=False
            )
            report["negative"].update(
                negative_forbidden_hit_count=1,
                negative_forbidden_hit_rate=0.1,
                negative_promotion_eligible=False,
            )
        report["golden_sha256"] = golden_sha
        report["golden_revision"] = "sha256:" + golden_sha
        report["runtime_config"]["golden_revision"] = "sha256:" + golden_sha
        report["runtime_config_fingerprint"] = hashlib.sha256(
            json.dumps(report["runtime_config"], sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        report["provenance_sha256"] = hashlib.sha256(
            json.dumps(
                {key: value for key, value in report.items() if key != "provenance_sha256"},
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
        ).hexdigest()
        path = tmp_path / f"{name}-provenance.json"
        path.write_text(json.dumps(report, indent=2) + "\n")
        reports.append(report)
        paths.append(path)

    reader_paths = []
    for index, (name, report, provenance_path) in enumerate(
        zip(("memphant", "syndai"), reports, paths)
    ):
        retrieval_sha = hashlib.sha256(provenance_path.read_bytes()).hexdigest()
        reader = _reader_report(
            evidence_sha=report["evidence_sha256"], retrieval_sha=retrieval_sha
        )
        if index:
            for row in reader["per_question"]:
                row["correct"] = False
            reader["reader_report_sha256"] = hashlib.sha256(
                json.dumps(
                    {key: value for key, value in reader.items() if key != "reader_report_sha256"},
                    sort_keys=True,
                    separators=(",", ":"),
                ).encode()
            ).hexdigest()
        path = tmp_path / f"{name}-reader.json"
        path.write_text(json.dumps(reader) + "\n")
        reader_paths.append(path)

    negative_reader_paths = []
    for index, (name, report, provenance_path) in enumerate(
        zip(("memphant", "syndai"), reports, paths)
    ):
        negative_reader = _negative_reader_report(report, provenance_path.read_bytes())
        if index:
            negative_reader["per_question"][0]["correct"] = False
            negative_reader["reader_report_sha256"] = hashlib.sha256(
                json.dumps(
                    {
                        key: value
                        for key, value in negative_reader.items()
                        if key != "reader_report_sha256"
                    },
                    sort_keys=True,
                    separators=(",", ":"),
                ).encode()
            ).hexdigest()
        path = tmp_path / f"{name}-negative-reader.json"
        path.write_text(json.dumps(negative_reader) + "\n")
        negative_reader_paths.append(path)

    out = tmp_path / "comparison.json"
    monkeypatch.setattr(
        sys,
        "argv",
        [
            "gate_compare.py",
            "--memphant-provenance", str(paths[0]),
            "--syndai-provenance", str(paths[1]),
            "--memphant-reader", str(reader_paths[0]),
            "--syndai-reader", str(reader_paths[1]),
            "--memphant-negative-reader", str(negative_reader_paths[0]),
            "--syndai-negative-reader", str(negative_reader_paths[1]),
            "--golden", str(golden),
            "--out", str(out),
        ],
    )
    assert comparator.main() == 0
    result = json.loads(out.read_text())
    assert result["retrieval"]["cluster_source"] == "pinned_golden.provenance[].file"
    assert result["retrieval"]["n_source_document_clusters"] == 2
    assert result["qa"]["delta_qa_accuracy"]["mean"] == 1.0
    assert result["negative"]["n_paired"] == 10
    assert result["negative"]["memphant"]["joint_pass_count"] == 10
    assert result["negative"]["syndai"]["joint_pass_count"] == 9
    assert result["negative"]["candidate_minus_incumbent"]["joint_pass_count"] == 1

    mem_negative = json.loads(negative_reader_paths[0].read_text())
    mem_negative["per_question"][0]["correct"] = False
    mem_negative["reader_report_sha256"] = hashlib.sha256(
        json.dumps(
            {
                key: value
                for key, value in mem_negative.items()
                if key != "reader_report_sha256"
            },
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ).hexdigest()
    negative_reader_paths[0].write_text(json.dumps(mem_negative) + "\n")
    assert comparator.main() == 0
    assert json.loads(out.read_text())["verdict"]["decision"].startswith("HOLD:")
    restored_mem_negative = _negative_reader_report(reports[0], paths[0].read_bytes())
    negative_reader_paths[0].write_text(json.dumps(restored_mem_negative) + "\n")

    full_argv = list(sys.argv)
    monkeypatch.setattr(
        sys,
        "argv",
        [
            value
            for index, value in enumerate(full_argv)
            if full_argv[index - 1] not in {
                "--memphant-negative-reader",
                "--syndai-negative-reader",
            }
            and value not in {"--memphant-negative-reader", "--syndai-negative-reader"}
        ],
    )
    assert comparator.main() == 1
    assert "negative reader reports are required" in json.loads(out.read_text())["verdict"]["decision"]
    monkeypatch.setattr(sys, "argv", full_argv)

    for report, path in zip(reports, paths):
        legacy = {key: value for key, value in report.items() if key != "negative"}
        legacy["provenance_sha256"] = hashlib.sha256(
            json.dumps(
                {key: value for key, value in legacy.items() if key != "provenance_sha256"},
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
        ).hexdigest()
        path.write_text(json.dumps(legacy, indent=2) + "\n")
    assert comparator.main() == 1
    assert "negative provenance reports are required" in json.loads(out.read_text())["verdict"]["decision"]
    for report, path in zip(reports, paths):
        path.write_text(json.dumps(report, indent=2) + "\n")

    tampered = json.loads(reader_paths[0].read_text())
    tampered["retrieval_report_sha256"] = "0" * 64
    reader_paths[0].write_text(json.dumps(tampered))
    assert comparator.main() == 1
    assert "retrieval_report_sha256" in json.loads(out.read_text())["verdict"]["decision"]


def test_strict_provenance_rejects_tampered_health_aggregate_and_latency() -> None:
    reader = _load_run_reader()
    for mutate in (
        lambda report: report.update(fallback_count=1),
        lambda report: report.update(recall_at_10=0.5),
        lambda report: report["per_question"][0].update(recall_e2e_ms=-1),
    ):
        report = _provenance_report()
        mutate(report)
        report["provenance_sha256"] = hashlib.sha256(
            json.dumps(
                {key: value for key, value in report.items() if key != "provenance_sha256"},
                sort_keys=True,
                separators=(",", ":"),
            ).encode()
        ).hexdigest()
        try:
            reader.validate_and_pair_reports(report, _provenance_report(), "provenance")
            raise AssertionError("expected strict provenance rejection")
        except ValueError:
            pass


def test_gate_compare_rejects_mismatched_k_budget_or_packer() -> None:
    comparator = _load_gate_compare()
    left = _provenance_report(runtime="memphant")
    for field, value in (
        ("k", 5),
        ("budget_tokens", 4096),
        ("evidence_packer", {"name": "other", "sha256": "0" * 64}),
    ):
        right = _provenance_report(runtime="syndai")
        right["runtime_config"][field] = value
        try:
            comparator._validate_shared_evidence_contract(left, right)
            raise AssertionError("expected shared evidence contract mismatch")
        except ValueError as error:
            assert field in str(error)
