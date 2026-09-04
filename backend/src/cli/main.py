"""Local human CLI for the loopback QuaZonai Research Intelligence API."""

from __future__ import annotations

import argparse
import json
import os
import sys
from collections.abc import Sequence
from typing import Any
from uuid import uuid4

from cli.client import ApiClient, CliClientError

DEFAULT_ENDPOINT = "http://127.0.0.1:8000"
EXIT_OK = 0
EXIT_USAGE = 2
EXIT_UNAVAILABLE = 10
EXIT_CONFLICT = 20
EXIT_FAILURE = 1


def _endpoint() -> str:
    return os.environ.get("QUAZONAI_API_ENDPOINT", DEFAULT_ENDPOINT)


def _headers() -> dict[str, str]:
    return {"Idempotency-Key": str(uuid4())}


def _print(value: Any) -> None:
    print(json.dumps(value, ensure_ascii=False, indent=2, default=str))


def _answer_pair(value: str) -> tuple[str, str]:
    key, separator, answer = value.partition("=")
    if not separator or not key or not answer:
        raise argparse.ArgumentTypeError("--answer must be KEY=VALUE")
    return key, answer


def _json_object(value: str) -> dict[str, Any]:
    try:
        parsed = json.loads(value)
    except json.JSONDecodeError as exc:
        raise argparse.ArgumentTypeError("--json must be a JSON object") from exc
    if not isinstance(parsed, dict):
        raise argparse.ArgumentTypeError("--json must be a JSON object")
    return parsed


def _safe_output(value: Any) -> Any:
    if isinstance(value, dict):
        return {
            key: "<redacted>" if key == "service_token" else _safe_output(item)
            for key, item in value.items()
        }
    if isinstance(value, list):
        return [_safe_output(item) for item in value]
    return value


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="QuaZonai research workbench CLI")
    parser.add_argument("--endpoint", default=_endpoint())
    commands = parser.add_subparsers(dest="resource", required=True)

    commands.add_parser("status")
    commands.add_parser("readiness")

    idea = commands.add_parser("idea")
    idea_actions = idea.add_subparsers(dest="action", required=True)
    draft_create = idea_actions.add_parser("create")
    draft_create.add_argument("--text", required=True)
    draft_show = idea_actions.add_parser("show")
    draft_show.add_argument("id")
    draft_answer = idea_actions.add_parser("answer")
    draft_answer.add_argument("id")
    draft_answer.add_argument("--expected-revision", type=int, required=True)
    draft_answer.add_argument(
        "--answer",
        dest="answers",
        action="append",
        type=_answer_pair,
        required=True,
    )
    draft_start = idea_actions.add_parser("start")
    draft_start.add_argument("id")
    draft_start.add_argument("--expected-revision", type=int, required=True)
    draft_start.add_argument("--title")
    draft_start.add_argument("--universe-version-id", dest="universe_version_ids", action="append")

    research = commands.add_parser("research")
    research_actions = research.add_subparsers(dest="action", required=True)
    research_actions.add_parser("list")
    show = research_actions.add_parser("show")
    show.add_argument("id")
    for action in ("pause", "resume", "archive", "wake"):
        item = research_actions.add_parser(action)
        item.add_argument("id")
        item.add_argument("--expected-revision", type=int, required=True)
        item.add_argument("--reason")
    cycles = research_actions.add_parser("cycles")
    cycles.add_argument("id")
    graph = research_actions.add_parser("graph")
    graph.add_argument("id")

    mission = commands.add_parser("mission")
    mission_actions = mission.add_subparsers(dest="action", required=True)
    for action in ("show", "turns", "artifacts"):
        item = mission_actions.add_parser(action)
        item.add_argument("id")

    alpha = commands.add_parser("alpha")
    alpha_actions = alpha.add_subparsers(dest="action", required=True)
    alpha_actions.add_parser("list")
    alpha_show = alpha_actions.add_parser("show")
    alpha_show.add_argument("id")

    portfolio = commands.add_parser("portfolio")
    portfolio_actions = portfolio.add_subparsers(dest="action", required=True)
    portfolio_actions.add_parser("mandates")
    portfolio_actions.add_parser("programs")
    candidate = portfolio_actions.add_parser("candidate")
    candidate.add_argument("id")

    approval = commands.add_parser("approval")
    approval_actions = approval.add_subparsers(dest="action", required=True)
    approval_actions.add_parser("list")
    approval_show = approval_actions.add_parser("show")
    approval_show.add_argument("id")
    approve = approval_actions.add_parser("approve")
    approve.add_argument("id")
    approve.add_argument("--downstream", dest="downstream_id", required=True)
    approve.add_argument("--expected-state", default="PENDING")
    reject = approval_actions.add_parser("reject")
    reject.add_argument("id")
    reject.add_argument("--reason", dest="reason_code", required=True)
    reject.add_argument("--note")
    reject.add_argument("--expected-state", default="PENDING")

    handoff = commands.add_parser("handoff")
    handoff_actions = handoff.add_subparsers(dest="action", required=True)
    handoff_actions.add_parser("list")
    revoke = handoff_actions.add_parser("revoke")
    revoke.add_argument("id")
    revoke.add_argument("--reason", dest="reason_code", required=True)

    data = commands.add_parser("data-source")
    data_actions = data.add_subparsers(dest="action", required=True)
    data_actions.add_parser("list")
    data_create = data_actions.add_parser("create")
    data_create.add_argument("--json", dest="payload", type=_json_object, required=True)
    data_preflight = data_actions.add_parser("preflight")
    data_preflight.add_argument("id")

    universe = commands.add_parser("universe")
    universe_actions = universe.add_subparsers(dest="action", required=True)
    universe_create = universe_actions.add_parser("create")
    universe_create.add_argument("--json", dest="payload", type=_json_object, required=True)
    universe_version = universe_actions.add_parser("version")
    universe_version.add_argument("id")
    universe_version.add_argument("--json", dest="payload", type=_json_object, required=True)

    dataset = commands.add_parser("dataset")
    dataset_actions = dataset.add_subparsers(dest="action", required=True)
    dataset_materialize = dataset_actions.add_parser("materialize")
    dataset_materialize.add_argument("--json", dest="payload", type=_json_object, required=True)
    dataset_status = dataset_actions.add_parser("status")
    dataset_status.add_argument("id")

    for resource in (
        "evaluation-dataset-selection",
        "evaluation-design-version",
        "promotion-policy-version",
    ):
        configuration = commands.add_parser(resource)
        configuration_actions = configuration.add_subparsers(dest="action", required=True)
        configuration_actions.add_parser("list")
        configuration_create = configuration_actions.add_parser("create")
        configuration_create.add_argument("--json", dest="payload", type=_json_object, required=True)

    mandate = commands.add_parser("mandate")
    mandate_actions = mandate.add_subparsers(dest="action", required=True)
    mandate_create = mandate_actions.add_parser("create")
    mandate_create.add_argument("--json", dest="payload", type=_json_object, required=True)
    mandate_version = mandate_actions.add_parser("version")
    mandate_version.add_argument("id")
    mandate_version.add_argument("--json", dest="payload", type=_json_object, required=True)

    downstream = commands.add_parser("downstream")
    downstream_actions = downstream.add_subparsers(dest="action", required=True)
    downstream_register = downstream_actions.add_parser("register")
    downstream_register.add_argument("--json", dest="payload", type=_json_object, required=True)

    commands.add_parser("datasets")
    commands.add_parser("universes")
    commands.add_parser("downstreams")
    return parser


def execute(client: ApiClient, args: argparse.Namespace) -> Any:
    if args.resource == "status":
        return client.request("GET", "/api/v1/system/health")
    if args.resource == "readiness":
        return client.request("GET", "/api/v1/readiness")
    if args.resource == "idea":
        if args.action == "create":
            return client.request(
                "POST",
                "/api/v1/idea-drafts",
                json_body={"original_idea_text": args.text},
                headers=_headers(),
            )
        if args.action == "show":
            return client.request("GET", f"/api/v1/idea-drafts/{args.id}")
        if args.action == "answer":
            return client.request(
                "POST",
                f"/api/v1/idea-drafts/{args.id}/answers",
                json_body={
                    "answers": dict(args.answers),
                    "expected_revision": args.expected_revision,
                },
                headers=_headers(),
            )
        return client.request(
            "POST",
            f"/api/v1/idea-drafts/{args.id}/start",
            json_body={
                "expected_revision": args.expected_revision,
                **({"title": args.title} if args.title else {}),
                **({"universe_version_ids": args.universe_version_ids} if args.universe_version_ids else {}),
            },
            headers=_headers(),
        )
    if args.resource == "research":
        if args.action == "list":
            return client.request("GET", "/api/v1/research-programs")
        if args.action == "show":
            return client.request("GET", f"/api/v1/research-programs/{args.id}")
        if args.action in {"pause", "resume", "archive", "wake"}:
            return client.request(
                "POST",
                f"/api/v1/research-programs/{args.id}/{args.action}",
                json_body={
                    "expected_revision": args.expected_revision,
                    **({"reason": args.reason} if args.reason else {}),
                },
                headers=_headers(),
            )
        if args.action == "cycles":
            return client.request("GET", f"/api/v1/research-programs/{args.id}/cycles")
        return client.request("GET", f"/api/v1/research-programs/{args.id}/mission-graph")
    if args.resource == "mission":
        suffix = "" if args.action == "show" else f"/{args.action}"
        return client.request("GET", f"/api/v1/missions/{args.id}{suffix}")
    if args.resource == "alpha":
        if args.action == "list":
            return client.request("GET", "/api/v1/alpha-library")
        return client.request("GET", f"/api/v1/alpha-library/{args.id}")
    if args.resource == "portfolio":
        if args.action == "mandates":
            return client.request("GET", "/api/v1/portfolio-mandates")
        if args.action == "programs":
            return client.request("GET", "/api/v1/portfolio-programs")
        return client.request("GET", f"/api/v1/portfolio-candidates/{args.id}")
    if args.resource == "approval":
        if args.action == "list":
            return client.request("GET", "/api/v1/approvals")
        if args.action == "show":
            return client.request("GET", f"/api/v1/approvals/{args.id}")
        if args.action == "approve":
            body = {
                "downstream_system_id": args.downstream_id,
                "expected_state": args.expected_state,
            }
        else:
            body = {
                "reason_code": args.reason_code,
                "note": args.note,
                "expected_state": args.expected_state,
            }
        return client.request(
            "POST",
            f"/api/v1/approvals/{args.id}/{args.action}",
            json_body=body,
            headers=_headers(),
        )
    if args.resource == "handoff":
        if args.action == "list":
            return client.request("GET", "/api/v1/handoffs")
        return client.request(
            "POST",
            f"/api/v1/handoffs/{args.id}/revoke",
            json_body={"reason_code": args.reason_code},
            headers=_headers(),
        )
    if args.resource == "data-source":
        if args.action == "list":
            return client.request("GET", "/api/v1/data-sources")
        if args.action == "preflight":
            return client.request(
                "POST",
                f"/api/v1/data-sources/{args.id}/preflight",
                json_body={},
                headers=_headers(),
            )
        return client.request(
            "POST",
            "/api/v1/data-sources",
            json_body=args.payload,
            headers=_headers(),
        )
    if args.resource == "universe":
        path = "/api/v1/universes"
        if args.action == "version":
            path = f"{path}/{args.id}/versions"
        return client.request("POST", path, json_body=args.payload, headers=_headers())
    if args.resource == "dataset":
        if args.action == "status":
            return client.request("GET", f"/api/v1/operations/{args.id}")
        return client.request(
            "POST",
            "/api/v1/datasets/materializations",
            json_body=args.payload,
            headers=_headers(),
        )
    trusted_configuration_paths = {
        "evaluation-dataset-selection": "/api/v1/evaluation-dataset-selections",
        "evaluation-design-version": "/api/v1/evaluation-design-versions",
        "promotion-policy-version": "/api/v1/promotion-policy-versions",
    }
    if args.resource in trusted_configuration_paths:
        path = trusted_configuration_paths[args.resource]
        if args.action == "list":
            return client.request("GET", path)
        return client.request("POST", path, json_body=args.payload, headers=_headers())
    if args.resource == "mandate":
        path = "/api/v1/portfolio-mandates"
        if args.action == "version":
            path = f"{path}/{args.id}/versions"
        return client.request("POST", path, json_body=args.payload, headers=_headers())
    if args.resource == "downstream":
        return client.request(
            "POST",
            "/api/v1/downstream-systems",
            json_body=args.payload,
            headers=_headers(),
        )
    if args.resource == "datasets":
        return client.request("GET", "/api/v1/datasets")
    if args.resource == "universes":
        return client.request("GET", "/api/v1/universes")
    return client.request("GET", "/api/v1/downstream-systems")


def main(argv: Sequence[str] | None = None) -> int:
    try:
        args = build_parser().parse_args(argv)
        with ApiClient(args.endpoint) as client:
            _print(_safe_output(execute(client, args)))
        return EXIT_OK
    except json.JSONDecodeError as exc:
        print(f"Invalid JSON: {exc}", file=sys.stderr)
        return EXIT_USAGE
    except CliClientError as exc:
        print(str(exc), file=sys.stderr)
        if exc.status_code == 409:
            return EXIT_CONFLICT
        if exc.status_code is not None and exc.status_code >= 500:
            return EXIT_UNAVAILABLE
        return EXIT_FAILURE
    except OSError as exc:
        print(str(exc), file=sys.stderr)
        return EXIT_FAILURE


if __name__ == "__main__":
    raise SystemExit(main())
