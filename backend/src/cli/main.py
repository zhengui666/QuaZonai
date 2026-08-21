"""Local human CLI for the loopback QuaZonai Core API."""

from __future__ import annotations

import argparse
import getpass
import json
import os
import sys
from collections.abc import Sequence
from pathlib import Path
from typing import Any

from cli.client import ApiClient, CliClientError
from cli.output import render_json, render_table

DEFAULT_ENDPOINT = "http://127.0.0.1:8000"
EXIT_OK = 0
EXIT_USAGE = 2
EXIT_UNAVAILABLE = 10
EXIT_CONFLICT = 20
EXIT_FAILURE = 1


def _endpoint() -> str:
    return os.environ.get("QUAZONAI_API_ENDPOINT", DEFAULT_ENDPOINT)


def _json_object(value: str | None, *, default: dict[str, Any] | None = None) -> dict[str, Any]:
    if value is None:
        return dict(default or {})
    if value == "-":
        raw = sys.stdin.read()
    elif value.startswith("@"):
        raw = Path(value[1:]).read_text(encoding="utf-8")
    else:
        raw = value
    parsed = json.loads(raw)
    if not isinstance(parsed, dict):
        raise CliClientError("JSON payload must be an object")
    return parsed


def _confirm(expected: str, supplied: str | None, message: str) -> None:
    if supplied == expected:
        return
    if not sys.stdin.isatty():
        raise CliClientError(f"Confirmation required: --confirm {expected}")
    entered = input(f"{message}\nType {expected} to continue: ").strip()
    if entered != expected:
        raise CliClientError("Confirmation did not match; no operation was performed")


def _secret_values(client: ApiClient, release_id: str, from_stdin: bool) -> dict[str, str]:
    if from_stdin:
        value = _json_object("-")
        if any(not isinstance(item, str) for item in value.values()):
            raise CliClientError("Secret JSON values must all be strings")
        return {key: str(item) for key, item in value.items()}
    release = client.request("GET", f"/api/v1/plugin-releases/{release_id}")
    descriptor = release.get("descriptor_snapshot") or {}
    schema = descriptor.get("secret_config_schema") or {}
    properties = schema.get("properties") or {}
    required = set(descriptor.get("required_secret_names") or [])
    values: dict[str, str] = {}
    for field_name in sorted(properties):
        suffix = " (required)" if field_name in required else ""
        value = getpass.getpass(f"{field_name}{suffix}: ")
        if value:
            values[field_name] = value
    missing = sorted(required - set(values))
    if missing:
        raise CliClientError("Missing required secret fields: " + ", ".join(missing))
    return values


def _render(value: Any, output: str) -> str:
    return render_json(value) if output == "json" else render_table(value)


def _resource_command(
    client: ApiClient,
    args: argparse.Namespace,
    base_path: str,
) -> Any:
    if args.action == "list":
        return client.request("GET", base_path)
    if args.action == "show":
        return client.request("GET", f"{base_path}/{args.id}")
    if args.action == "create":
        return client.request("POST", base_path, json_body=_json_object(args.json))
    if args.action == "update":
        return client.request("PUT", f"{base_path}/{args.id}", json_body=_json_object(args.json))
    raise CliClientError("Unsupported resource action")


def execute(client: ApiClient, args: argparse.Namespace) -> Any:
    command = args.command
    action = getattr(args, "action", None)
    if command == "status":
        return client.request("GET", "/api/v1/system/health")

    if command == "plugin":
        if action == "list":
            return client.request("GET", "/api/v1/plugins")
        if action == "show":
            return client.request("GET", f"/api/v1/plugin-releases/{args.id}")
        if action == "install":
            primary = Path(args.primary)
            dependencies = [Path(item) for item in args.dependency]
            for path in [primary, *dependencies]:
                if not path.is_file() or path.suffix != ".whl":
                    raise CliClientError(f"Plugin input is not a wheel file: {path}")
            return client.upload_plugin(primary, dependencies)
        if action == "prewarm":
            return client.request(
                "POST",
                "/api/v1/plugin-runtime-bundles/prewarm",
                json_body=_json_object(args.json),
            )
        if action == "impact":
            return client.request("GET", f"/api/v1/plugin-releases/{args.id}/impact")
        if action in {"activate", "deactivate"}:
            return client.request("POST", f"/api/v1/plugin-releases/{args.id}/{action}")
        if action == "remove":
            if args.force:
                _confirm(
                    args.id,
                    args.confirm,
                    "Force removal stops affected Deployments but does not liquidate positions.",
                )
            return client.request(
                "DELETE",
                f"/api/v1/plugin-releases/{args.id}",
                params={"force": str(bool(args.force)).lower()},
            )

    if command == "credential":
        if action == "list":
            return client.request("GET", "/api/v1/credential-sets")
        if action == "show":
            return client.request("GET", f"/api/v1/credential-sets/{args.id}")
        if action == "create":
            secrets = _secret_values(client, args.plugin_release_id, args.secret_stdin)
            return client.request(
                "POST",
                "/api/v1/credential-sets",
                json_body={
                    "plugin_release_id": args.plugin_release_id,
                    "name": args.name,
                    "public_config": _json_object(args.public_json),
                    "secrets": secrets,
                },
            )
        if action == "update":
            current = client.request("GET", f"/api/v1/credential-sets/{args.id}")
            secrets = _secret_values(
                client,
                str(current["plugin_release_id"]),
                args.secret_stdin,
            )
            return client.request(
                "PUT",
                f"/api/v1/credential-sets/{args.id}",
                json_body={
                    "public_config": _json_object(
                        args.public_json,
                        default=current["public_config"],
                    ),
                    "secrets": secrets,
                },
            )

    if command == "data-source":
        if action == "preflight":
            return client.request("POST", f"/api/v1/data-sources/{args.id}/preflight")
        return _resource_command(client, args, "/api/v1/data-sources")

    if command == "execution-connection":
        if action == "preflight":
            return client.request(
                "POST", f"/api/v1/execution-connections/{args.id}/preflight"
            )
        return _resource_command(client, args, "/api/v1/execution-connections")

    if command == "dataset":
        if action == "list":
            return client.request("GET", "/api/v1/catalog-datasets")
        if action == "show":
            return client.request("GET", f"/api/v1/catalog-datasets/{args.id}")
        if action == "import":
            parquet = Path(args.file)
            if not parquet.is_file() or parquet.suffix != ".parquet":
                raise CliClientError("Dataset input must be an existing .parquet file")
            return client.upload_dataset(
                args.source_id,
                parquet,
                instrument_id=args.instrument_id,
                source_label=args.source_label,
                metadata_json=json.dumps(_json_object(args.metadata_json), separators=(",", ":")),
            )

    if command == "strategy":
        if action == "list":
            return client.request("GET", "/api/v1/strategies")
        if action == "show":
            return client.request("GET", f"/api/v1/strategies/{args.id}")
        if action == "create":
            return client.request("POST", "/api/v1/strategies", json_body={"name": args.name})
        if action == "version":
            source = Path(args.file)
            if not source.is_file() or source.suffix != ".py":
                raise CliClientError("Strategy input must be an existing .py file")
            return client.upload_strategy(
                args.strategy_id,
                source,
                json.dumps(_json_object(args.config_json), separators=(",", ":")),
            )

    if command == "research":
        if action == "list":
            return client.request("GET", "/api/v1/research-cases")
        if action == "show":
            return client.request("GET", f"/api/v1/research-cases/{args.id}")
        if action == "create":
            return client.request(
                "POST",
                "/api/v1/research-cases",
                json_body={
                    "title": args.title,
                    "strategy_version_id": args.strategy_version_id,
                },
            )
        if action == "section":
            markdown = (
                Path(args.markdown_file).read_text(encoding="utf-8")
                if args.markdown_file
                else sys.stdin.read()
            )
            return client.request(
                "POST",
                f"/api/v1/research-cases/{args.research_id}/sections",
                json_body={"section": args.section, "markdown": markdown},
            )
        if action == "activate":
            return client.request(
                "POST", f"/api/v1/research-cases/{args.research_id}/activate"
            )

    if command == "experiment":
        if action == "show":
            return client.request("GET", f"/api/v1/experiments/{args.id}")
        if action == "create":
            return client.request(
                "POST",
                f"/api/v1/research-cases/{args.research_id}/experiments",
                json_body=_json_object(args.json),
            )
        if action == "start":
            return client.request("POST", f"/api/v1/experiments/{args.id}/start")

    if command == "run":
        if action == "list":
            params = {"experiment_id": args.experiment_id} if args.experiment_id else None
            return client.request("GET", "/api/v1/runs", params=params)
        if action == "show":
            return client.request("GET", f"/api/v1/runs/{args.id}")
        if action == "reports":
            return client.request("GET", f"/api/v1/runs/{args.id}/reports")

    if command == "approval":
        if action == "list":
            return client.request("GET", "/api/v1/approvals")
        if action == "show":
            return client.request("GET", f"/api/v1/approvals/{args.id}")
        if action in {"approve", "reject"}:
            _confirm(
                args.id,
                args.confirm,
                "This local human decision may change the real-capital Deployment lifecycle.",
            )
            return client.request(
                "POST",
                f"/api/v1/approvals/{args.id}/{action}",
                json_body={"reason": args.reason},
            )

    if command == "deployment":
        if action == "list":
            return client.request("GET", "/api/v1/deployments")
        if action == "show":
            return client.request("GET", f"/api/v1/deployments/{args.id}")
        if action == "create":
            return client.request("POST", "/api/v1/deployments", json_body=_json_object(args.json))
        if action == "stop":
            _confirm(
                args.id,
                args.confirm,
                "Stop cancels open orders and blocks new trading; it does not liquidate positions.",
            )
            return client.request("POST", f"/api/v1/deployments/{args.id}/stop")
        if action == "restart":
            return client.request("POST", f"/api/v1/deployments/{args.id}/restart")

    if command == "risk" and action == "list":
        return client.request("GET", "/api/v1/risk-accounts")

    if command == "universe":
        if action == "list":
            return client.request(
                "GET", f"/api/v1/deployments/{args.deployment_id}/universe-revisions"
            )
        if action == "create":
            return client.request(
                "POST",
                f"/api/v1/deployments/{args.deployment_id}/universe-revisions",
                json_body=_json_object(args.json),
            )

    if command == "event" and action == "list":
        return client.request(
            "GET",
            "/api/v1/events",
            params={"after_id": args.after_id, "limit": args.limit},
        )

    raise CliClientError("Unsupported command")


def _actions(parser: argparse.ArgumentParser, names: Sequence[str]) -> Any:
    return parser.add_subparsers(
        dest="action",
        required=True,
        metavar="{" + ",".join(names) + "}",
    )


def _list_show(actions: Any) -> None:
    actions.add_parser("list")
    show = actions.add_parser("show")
    show.add_argument("id")


def _json_resources(actions: Any) -> None:
    _list_show(actions)
    create = actions.add_parser("create")
    create.add_argument("--json", required=True, help="JSON object, @file, or - for stdin")
    update = actions.add_parser("update")
    update.add_argument("id")
    update.add_argument("--json", required=True, help="JSON object, @file, or - for stdin")
    preflight = actions.add_parser("preflight")
    preflight.add_argument("id")


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="qf", description="QuaZonai local operator CLI")
    parser.add_argument("--endpoint", default=_endpoint(), help="Loopback Core API URL")
    parser.add_argument("--output", choices=["table", "json"], default="table")
    commands = parser.add_subparsers(dest="command", required=True)
    commands.add_parser("status")

    plugin = commands.add_parser("plugin")
    actions = _actions(
        plugin,
        ["list", "show", "install", "prewarm", "impact", "activate", "deactivate", "remove"],
    )
    _list_show(actions)
    install = actions.add_parser("install")
    install.add_argument("primary")
    install.add_argument("--dependency", action="append", default=[])
    prewarm = actions.add_parser("prewarm")
    prewarm.add_argument("--json", required=True)
    for name in ("impact", "activate", "deactivate"):
        item = actions.add_parser(name)
        item.add_argument("id")
    remove = actions.add_parser("remove")
    remove.add_argument("id")
    remove.add_argument("--force", action="store_true")
    remove.add_argument("--confirm")

    credential = commands.add_parser("credential")
    actions = _actions(credential, ["list", "show", "create", "update"])
    _list_show(actions)
    create = actions.add_parser("create")
    create.add_argument("--plugin-release-id", required=True)
    create.add_argument("--name", required=True)
    create.add_argument("--public-json", default="{}")
    create.add_argument("--secret-stdin", action="store_true")
    update = actions.add_parser("update")
    update.add_argument("id")
    update.add_argument("--public-json")
    update.add_argument("--secret-stdin", action="store_true")

    for command_name in ("data-source", "execution-connection"):
        resource = commands.add_parser(command_name)
        _json_resources(
            _actions(resource, ["list", "show", "create", "update", "preflight"])
        )

    dataset = commands.add_parser("dataset")
    actions = _actions(dataset, ["list", "show", "import"])
    _list_show(actions)
    imported = actions.add_parser("import")
    imported.add_argument("--source-id", required=True)
    imported.add_argument("--file", required=True)
    imported.add_argument("--instrument-id", required=True)
    imported.add_argument("--source-label", required=True)
    imported.add_argument("--metadata-json", default="{}")

    strategy = commands.add_parser("strategy")
    actions = _actions(strategy, ["list", "show", "create", "version"])
    _list_show(actions)
    create = actions.add_parser("create")
    create.add_argument("--name", required=True)
    version = actions.add_parser("version")
    version.add_argument("--strategy-id", required=True)
    version.add_argument("--file", required=True)
    version.add_argument("--config-json", default="{}")

    research = commands.add_parser("research")
    actions = _actions(research, ["list", "show", "create", "section", "activate"])
    _list_show(actions)
    create = actions.add_parser("create")
    create.add_argument("--title", required=True)
    create.add_argument("--strategy-version-id")
    section = actions.add_parser("section")
    section.add_argument("--research-id", required=True)
    section.add_argument("--section", required=True)
    section.add_argument("--markdown-file")
    activate = actions.add_parser("activate")
    activate.add_argument("--research-id", required=True)

    experiment = commands.add_parser("experiment")
    actions = _actions(experiment, ["show", "create", "start"])
    show = actions.add_parser("show")
    show.add_argument("id")
    create = actions.add_parser("create")
    create.add_argument("--research-id", required=True)
    create.add_argument("--json", required=True)
    start = actions.add_parser("start")
    start.add_argument("id")

    run = commands.add_parser("run")
    actions = _actions(run, ["list", "show", "reports"])
    listed = actions.add_parser("list")
    listed.add_argument("--experiment-id")
    for name in ("show", "reports"):
        item = actions.add_parser(name)
        item.add_argument("id")

    approval = commands.add_parser("approval")
    actions = _actions(approval, ["list", "show", "approve", "reject"])
    _list_show(actions)
    for name in ("approve", "reject"):
        item = actions.add_parser(name)
        item.add_argument("id")
        item.add_argument("--reason", required=True)
        item.add_argument("--confirm")

    deployment = commands.add_parser("deployment")
    actions = _actions(deployment, ["list", "show", "create", "stop", "restart"])
    _list_show(actions)
    create = actions.add_parser("create")
    create.add_argument("--json", required=True)
    stop = actions.add_parser("stop")
    stop.add_argument("id")
    stop.add_argument("--confirm")
    restart = actions.add_parser("restart")
    restart.add_argument("id")

    risk = commands.add_parser("risk")
    _actions(risk, ["list"]).add_parser("list")

    universe = commands.add_parser("universe")
    actions = _actions(universe, ["list", "create"])
    listed = actions.add_parser("list")
    listed.add_argument("--deployment-id", required=True)
    create = actions.add_parser("create")
    create.add_argument("--deployment-id", required=True)
    create.add_argument("--json", required=True)

    event = commands.add_parser("event")
    actions = _actions(event, ["list"])
    listed = actions.add_parser("list")
    listed.add_argument("--after-id", type=int, default=0)
    listed.add_argument("--limit", type=int, default=200)
    return parser


def main(argv: Sequence[str] | None = None) -> int:
    try:
        args = build_parser().parse_args(argv)
        with ApiClient(args.endpoint) as client:
            print(_render(execute(client, args), args.output))
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
