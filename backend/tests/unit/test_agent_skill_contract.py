from __future__ import annotations

import argparse
import re
import shlex
from pathlib import Path

from cli.main import build_parser

REPO_ROOT = Path(__file__).resolve().parents[3]
SKILL_DIR = REPO_ROOT / "skills" / "quazonai"
SKILL_PATH = SKILL_DIR / "SKILL.md"
CLI_REFERENCE_PATH = SKILL_DIR / "references" / "cli-reference.md"


def _frontmatter(text: str) -> str:
    parts = text.split("---", 2)
    assert len(parts) == 3 and parts[0] == "", "SKILL.md must start with YAML frontmatter"
    return parts[1]


def _frontmatter_value(frontmatter: str, field: str) -> str:
    match = re.search(rf"^{re.escape(field)}:\s*(.+)$", frontmatter, flags=re.MULTILINE)
    assert match is not None, f"missing frontmatter field: {field}"
    return match.group(1).strip().strip("\"'")


def _leaf_command_paths(
    parser: argparse.ArgumentParser,
    prefix: tuple[str, ...] = (),
) -> set[str]:
    subparser_actions = [
        action
        for action in parser._actions
        if isinstance(action, argparse._SubParsersAction)
    ]
    if not subparser_actions:
        return {" ".join(prefix)}

    paths: set[str] = set()
    for action in subparser_actions:
        for name, child_parser in action.choices.items():
            paths.update(_leaf_command_paths(child_parser, (*prefix, name)))
    return paths


def _documented_command_paths(text: str) -> set[str]:
    start = "<!-- cli-command-paths:start -->"
    end = "<!-- cli-command-paths:end -->"
    assert start in text and end in text, "CLI reference must contain command markers"
    block = text.split(start, 1)[1].split(end, 1)[0]
    return set(re.findall(r"^\| `([^`]+)` \|", block, flags=re.MULTILINE))


def _bash_commands(text: str) -> list[str]:
    commands: list[str] = []
    for block in re.findall(r"```bash\n(.*?)```", text, flags=re.DOTALL):
        current = ""
        for raw_line in block.splitlines():
            line = raw_line.strip()
            if not line:
                continue
            if line.endswith("\\"):
                current += f"{line[:-1].rstrip()} "
                continue
            current += line
            commands.append(current)
            current = ""
        assert not current, "unterminated shell line continuation in documentation"
    return commands


def test_skill_metadata_is_portable_and_discoverable() -> None:
    text = SKILL_PATH.read_text(encoding="utf-8")
    frontmatter = _frontmatter(text)

    name = _frontmatter_value(frontmatter, "name")
    description = _frontmatter_value(frontmatter, "description")
    compatibility = _frontmatter_value(frontmatter, "compatibility")

    assert name == SKILL_DIR.name
    assert re.fullmatch(r"[a-z0-9]+(?:-[a-z0-9]+)*", name)
    assert 1 <= len(description) <= 1024
    assert "`quazonai` CLI" in description
    assert "Use when" in description
    assert "Do not use" in description
    assert len(compatibility) <= 500
    assert len(text.splitlines()) < 500


def test_skill_references_are_relative_and_present() -> None:
    text = SKILL_PATH.read_text(encoding="utf-8")
    reference_links = set(re.findall(r"\]\((references/[^)]+\.md)\)", text))

    assert reference_links == {
        "references/cli-reference.md",
        "references/workflows.md",
    }
    for relative_path in reference_links:
        assert (SKILL_DIR / relative_path).is_file()

    assert "This skill is self-contained." in text
    assert not re.search(
        r"\]\((?:\.\./)*(?:AGENTS|DESIGN|OPERATIONS|CLI)\.md(?:#[^)]+)?\)",
        text,
    )


def test_documented_command_inventory_matches_argparse_tree() -> None:
    documented = _documented_command_paths(
        CLI_REFERENCE_PATH.read_text(encoding="utf-8")
    )
    implemented = _leaf_command_paths(build_parser())

    assert documented == implemented, (
        f"missing from reference: {sorted(implemented - documented)}; "
        f"not implemented: {sorted(documented - implemented)}"
    )


def test_skill_does_not_advertise_design_only_commands() -> None:
    combined = "\n".join(
        path.read_text(encoding="utf-8")
        for path in (
            SKILL_PATH,
            CLI_REFERENCE_PATH,
            SKILL_DIR / "references" / "workflows.md",
        )
    )
    stale_invocations = {
        "quazonai mandate ",
        "quazonai portfolio list",
        "quazonai portfolio show",
        "quazonai candidate show",
        "quazonai handoff show",
        "quazonai feedback show",
        "quazonai universe list",
        "quazonai dataset list",
        "quazonai downstream list",
        "quazonai codex ",
        "quazonai plugin ",
    }

    for invocation in stale_invocations:
        assert invocation not in combined


def test_all_documented_shell_commands_parse() -> None:
    parser = build_parser()
    parsed_commands: list[str] = []

    for path in (
        SKILL_PATH,
        CLI_REFERENCE_PATH,
        SKILL_DIR / "references" / "workflows.md",
    ):
        for command in _bash_commands(path.read_text(encoding="utf-8")):
            if not command.startswith("quazonai "):
                continue
            argv = shlex.split(command)[1:]
            if "--help" in argv:
                continue
            parser.parse_args(argv)
            parsed_commands.append(command)

    assert parsed_commands


def test_high_risk_argument_shapes_match_documentation() -> None:
    parser = build_parser()

    approve = parser.parse_args(
        ["approval", "approve", "approval-1", "downstream-1"]
    )
    assert approve.id == "approval-1"
    assert approve.downstream_id == "downstream-1"
    assert approve.expected_state == "PENDING"

    reject = parser.parse_args(
        ["approval", "reject", "approval-1", "RISK_LIMIT"]
    )
    assert reject.id == "approval-1"
    assert reject.reason_code == "RISK_LIMIT"
    assert reject.note is None

    revoke = parser.parse_args(
        ["handoff", "revoke", "handoff-1", "SUPERSEDED"]
    )
    assert revoke.id == "handoff-1"
    assert revoke.reason_code == "SUPERSEDED"

    data_source = parser.parse_args(
        [
            "data-source",
            "create",
            "primary-market-data",
            "--provider",
            "example",
            "--fields",
            "symbol,price",
        ]
    )
    assert data_source.name == "primary-market-data"
    assert data_source.provider == "example"
    assert data_source.fields == "symbol,price"

    reference = CLI_REFERENCE_PATH.read_text(encoding="utf-8")
    assert "<APPROVAL_ID> \\\n  <DOWNSTREAM_SYSTEM_ID>" in reference
    assert "quazonai handoff revoke <HANDOFF_ID> <REASON_CODE>" in reference
    assert "quazonai data-source create \\\n  \"<NAME>\"" in reference
    assert "quazonai approval approve <APPROVAL_ID> --downstream" not in reference


def test_global_endpoint_examples_use_argparse_order() -> None:
    combined = "\n".join(
        path.read_text(encoding="utf-8")
        for path in (
            SKILL_PATH,
            CLI_REFERENCE_PATH,
            SKILL_DIR / "references" / "workflows.md",
        )
    )

    assert "quazonai --endpoint http://127.0.0.1:8000 status" in combined
    assert not re.search(r"(?m)^\s*quazonai status --endpoint", combined)
