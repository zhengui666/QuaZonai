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


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="QuaZonai research workbench CLI")
    parser.add_argument("--endpoint", default=_endpoint())
    commands = parser.add_subparsers(dest="resource", required=True)

    commands.add_parser("status")
    commands.add_parser("readiness")

    idea = commands.add_parser("idea")
    idea_actions = idea.add_subparsers(dest="action", required=True)
    preview = idea_actions.add_parser("preview")
    preview.add_argument("--text", required=True)

    research = commands.add_parser("research")
    research_actions = research.add_subparsers(dest="action", required=True)
    research_actions.add_parser("list")
    show = research_actions.add_parser("show")
    show.add_argument("id")
    start = research_actions.add_parser("start")
    start.add_argument("--idea", required=True)
    start.add_argument(
        "--overlap-action",
        choices=["recommended", "new-program", "independent-program"],
        default="recommended",
    )
    create = research_actions.add_parser("create")
    create.add_argument("idea")
    create.add_argument(
        "--overlap-action",
        choices=["recommended", "new-program", "independent-program"],
        default="recommended",
    )
    for action in ("pause", "resume", "archive", "restore"):
        item = research_actions.add_parser(action)
        item.add_argument("id")
        item.add_argument("--reason")
    missions = research_actions.add_parser("missions")
    missions.add_argument("id")
    activity = research_actions.add_parser("activity")
    activity.add_argument("id")

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
    data_create.add_argument("name")
    data_create.add_argument("--provider")
    data_create.add_argument("--fields", default="")

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
        return client.request(
            "POST",
            "/api/v1/ideas/preview",
            json_body={"idea": args.text},
        )
    if args.resource == "research":
        if args.action == "list":
            return client.request("GET", "/api/v1/research-programs")
        if args.action == "show":
            return client.request("GET", f"/api/v1/research-programs/{args.id}")
        if args.action in {"create", "start"}:
            idea_text = args.idea
            return client.request(
                "POST",
                "/api/v1/research-programs",
                json_body={
                    "idea": idea_text,
                    "answers": {},
                    "overlap_action": args.overlap_action,
                },
                headers=_headers(),
            )
        if args.action in {"pause", "resume", "archive", "restore"}:
            return client.request(
                "POST",
                f"/api/v1/research-programs/{args.id}/{args.action}",
                json_body={"reason": args.reason},
                headers=_headers(),
            )
        return client.request("GET", f"/api/v1/research-programs/{args.id}/{args.action}")
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
        fields = [item.strip() for item in args.fields.split(",") if item.strip()]
        return client.request(
            "POST",
            "/api/v1/data-sources",
            json_body={"name": args.name, "provider": args.provider, "fields": fields},
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
            _print(execute(client, args))
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
