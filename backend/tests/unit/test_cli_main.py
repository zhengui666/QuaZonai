from __future__ import annotations

import json
from typing import Any

import pytest

from cli.main import _print, _safe_output, build_parser, execute


class Recorder:
    def __init__(self) -> None:
        self.calls: list[dict[str, Any]] = []

    def request(self, method: str, path: str, **kwargs: Any) -> dict[str, str]:
        self.calls.append({"method": method, "path": path, **kwargs})
        return {"path": path}


def _run(client: Recorder, *argv: str) -> dict[str, str]:
    return execute(client, build_parser().parse_args(argv))  # type: ignore[arg-type]


def test_draft_commands_follow_the_only_program_creation_path() -> None:
    client = Recorder()

    _run(client, "idea", "create", "--text", "Research liquid equities with PIT data")
    _run(
        client,
        "idea",
        "answer",
        "draft-1",
        "--expected-revision",
        "1",
        "--answer",
        "market_scope=US equities",
        "--answer",
        "horizon=one day",
    )
    _run(client, "idea", "start", "draft-1", "--expected-revision", "2", "--title", "PIT")

    assert client.calls[0]["path"] == "/api/v1/idea-drafts"
    assert client.calls[0]["json_body"] == {
        "original_idea_text": "Research liquid equities with PIT data"
    }
    assert client.calls[1]["path"] == "/api/v1/idea-drafts/draft-1/answers"
    assert client.calls[1]["json_body"] == {
        "answers": {"market_scope": "US equities", "horizon": "one day"},
        "expected_revision": 1,
    }
    assert client.calls[2]["path"] == "/api/v1/idea-drafts/draft-1/start"
    assert client.calls[2]["json_body"] == {"expected_revision": 2, "title": "PIT"}
    assert all("Idempotency-Key" in call["headers"] for call in client.calls)


def test_program_and_mission_reads_use_current_graph_routes() -> None:
    client = Recorder()

    _run(client, "research", "cycles", "program-1")
    _run(client, "research", "graph", "program-1")
    _run(client, "research", "wake", "program-1", "--expected-revision", "3")
    _run(client, "mission", "artifacts", "mission-1")

    assert [(call["method"], call["path"]) for call in client.calls] == [
        ("GET", "/api/v1/research-programs/program-1/cycles"),
        ("GET", "/api/v1/research-programs/program-1/mission-graph"),
        ("POST", "/api/v1/research-programs/program-1/wake"),
        ("GET", "/api/v1/missions/mission-1/artifacts"),
    ]
    assert client.calls[2]["json_body"] == {"expected_revision": 3}


def test_configuration_commands_use_canonical_routes() -> None:
    client = Recorder()

    _run(client, "universe", "create", "--json", '{"universe_key":"US"}')
    _run(client, "universe", "version", "universe-1", "--json", '{"name":"US v2"}')
    _run(client, "data-source", "create", "--json", '{"name":"bars"}')
    _run(client, "data-source", "preflight", "source-1")
    _run(client, "dataset", "materialize", "--json", '{"origin":"vendor"}')
    _run(client, "dataset", "status", "operation-1")
    _run(client, "mandate", "create", "--json", '{"key":"core"}')
    _run(client, "mandate", "version", "mandate-1", "--json", '{"objective":"RETURN"}')
    _run(client, "downstream", "register", "--json", '{"name":"paper"}')

    assert [(call["method"], call["path"]) for call in client.calls] == [
        ("POST", "/api/v1/universes"),
        ("POST", "/api/v1/universes/universe-1/versions"),
        ("POST", "/api/v1/data-sources"),
        ("POST", "/api/v1/data-sources/source-1/preflight"),
        ("POST", "/api/v1/datasets/materializations"),
        ("GET", "/api/v1/operations/operation-1"),
        ("POST", "/api/v1/portfolio-mandates"),
        ("POST", "/api/v1/portfolio-mandates/mandate-1/versions"),
        ("POST", "/api/v1/downstream-systems"),
    ]
    assert [call.get("json_body") for call in client.calls] == [
        {"universe_key": "US"},
        {"name": "US v2"},
        {"name": "bars"},
        {},
        {"origin": "vendor"},
        None,
        {"key": "core"},
        {"objective": "RETURN"},
        {"name": "paper"},
    ]
    assert all(
        "Idempotency-Key" in call["headers"]
        for call in client.calls
        if call["method"] == "POST"
    )


def test_trusted_alpha_configuration_commands_are_thin_canonical_transport() -> None:
    client = Recorder()
    payloads = {
        "evaluation-dataset-selection": {
            "universe_version_id": "universe-1",
            "discovery_dataset_revision_id": "discovery-1",
            "validation_dataset_revision_id": "validation-1",
            "sealed_dataset_revision_id": "sealed-1",
            "state": "ENABLED",
        },
        "evaluation-design-version": {
            "universe_version_id": "universe-1",
            "contract_version": "evaluation-design-v1",
            "allowed_model_mode": "CALIBRATED_RETURN",
            "qualification_role": "PRIMARY_ALPHA",
            "walk_forward_folds": 2,
            "annualization_factor": "252",
            "multiple_testing_method": "BENJAMINI_HOCHBERG",
            "multiple_testing_max_trials": 20,
            "qualification_metric_code": "NET_RETURN",
            "qualification_comparator": "MINIMUM",
            "qualification_threshold": "0.02",
            "pass_disclosure_code": "QUALIFIED",
            "failure_disclosure_code": "FAILED",
            "inconclusive_disclosure_code": "INCONCLUSIVE",
            "invalid_disclosure_code": "INVALID",
            "state": "ACTIVE",
        },
        "promotion-policy-version": {
            "purpose": "ALPHA_DISCOVERY_TO_SEALED",
            "mode": "MANUAL_APPROVAL",
            "gates": [{"metric_code": "NET_RETURN", "comparator": "MINIMUM", "threshold": "0.02", "ordinal": 1}],
            "state": "ACTIVE",
        },
    }

    for resource, payload in payloads.items():
        _run(client, resource, "create", "--json", json.dumps(payload))
        _run(client, resource, "list")

    assert [(call["method"], call["path"]) for call in client.calls] == [
        ("POST", "/api/v1/evaluation-dataset-selections"),
        ("GET", "/api/v1/evaluation-dataset-selections"),
        ("POST", "/api/v1/evaluation-design-versions"),
        ("GET", "/api/v1/evaluation-design-versions"),
        ("POST", "/api/v1/promotion-policy-versions"),
        ("GET", "/api/v1/promotion-policy-versions"),
    ]
    assert [call.get("json_body") for call in client.calls] == [
        payloads["evaluation-dataset-selection"],
        None,
        payloads["evaluation-design-version"],
        None,
        payloads["promotion-policy-version"],
        None,
    ]
    assert all(
        "Idempotency-Key" in call["headers"]
        for call in client.calls
        if call["method"] == "POST"
    )


def test_downstream_service_token_is_not_printed(capsys: pytest.CaptureFixture[str]) -> None:
    _print(_safe_output({"service_token": "never-print", "result": {"service_token": "also-never"}}))

    output = capsys.readouterr().out
    assert "never-print" not in output
    assert "also-never" not in output


def test_draft_answer_requires_a_key_value_pair() -> None:
    with pytest.raises(SystemExit):
        build_parser().parse_args(
            ["idea", "answer", "draft-1", "--expected-revision", "1", "--answer", "scope"]
        )


def test_configuration_json_requires_an_object() -> None:
    with pytest.raises(SystemExit):
        build_parser().parse_args(["universe", "create", "--json", "[]"])


@pytest.mark.parametrize(
    "argv",
    [
        ["idea", "preview", "--text", "Research liquid equities"],
        ["research", "start", "--idea", "Research liquid equities"],
        ["research", "create", "Research liquid equities"],
        ["research", "restore", "program-1"],
        ["research", "activity", "program-1"],
        ["research", "missions", "program-1"],
    ],
)
def test_retired_research_commands_are_not_parseable(argv: list[str]) -> None:
    with pytest.raises(SystemExit):
        build_parser().parse_args(argv)
