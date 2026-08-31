from __future__ import annotations

import argparse
import re
import shlex
from pathlib import Path

import pytest

from cli.main import build_parser

REPO_ROOT = Path(__file__).resolve().parents[3]
README_PATH = REPO_ROOT / "README.md"
SKILL_DIR = REPO_ROOT / "skills" / "quazonai"
SKILL_PATH = SKILL_DIR / "SKILL.md"
CLI_REFERENCE_PATH = SKILL_DIR / "references" / "cli-reference.md"
WORKFLOWS_PATH = SKILL_DIR / "references" / "workflows.md"


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


def _skill_documents() -> tuple[Path, ...]:
    return SKILL_PATH, CLI_REFERENCE_PATH, WORKFLOWS_PATH


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
    assert "human-only approve/reject command" in description
    assert len(compatibility) <= 500
    assert len(text.splitlines()) < 500


def test_skill_references_are_relative_and_present() -> None:
    text = SKILL_PATH.read_text(encoding="utf-8")
    reference_links = set(re.findall(r"\]\((references/[^)]+\.md)\)", text))

    assert reference_links == {
        "references/authentication.md",
        "references/cli-reference.md",
        "references/workflows.md",
    }
    for relative_path in reference_links:
        assert (SKILL_DIR / relative_path).is_file()


def test_skill_defers_to_repository_authority_when_available() -> None:
    skill = SKILL_PATH.read_text(encoding="utf-8")
    reference = CLI_REFERENCE_PATH.read_text(encoding="utf-8")

    assert "active working directory" in skill
    assert "git rev-parse --show-toplevel" in skill
    assert "never from this Skill's installation directory" in skill
    assert "${QZ_REPO_ROOT}/AGENTS.md" in skill
    assert "`DESIGN.md` remains the product authority" in skill
    assert "`AGENTS.md` remains the governance authority" in skill
    assert "`--help` output is syntax authority only" in skill
    assert "installed standalone" in skill
    assert "../../AGENTS.md" not in skill
    assert "`DESIGN.md` remains the product authority" in reference
    assert "never overrides product, ownership, authorization, or safety rules" in reference


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
        path.read_text(encoding="utf-8") for path in _skill_documents()
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

    for path in _skill_documents():
        for command in _bash_commands(path.read_text(encoding="utf-8")):
            if not command.startswith("quazonai "):
                continue
            argv = shlex.split(command)[1:]
            if "--help" in argv:
                continue
            parser.parse_args(argv)
            parsed_commands.append(command)

    assert parsed_commands


def test_candidate_decisions_are_prepared_but_never_executed_by_agents() -> None:
    combined = "\n".join(
        path.read_text(encoding="utf-8") for path in _skill_documents()
    )
    bash_commands = [
        command
        for path in _skill_documents()
        for command in _bash_commands(path.read_text(encoding="utf-8"))
    ]

    assert "No Codex or other AI Agent profile may execute them" in combined
    assert "An AI Agent must never execute `approval approve` or `approval reject`" in combined
    assert "Human-only decision" in combined
    assert not any(
        re.match(r"^quazonai approval (approve|reject)\b", command)
        and "--help" not in command
        for command in bash_commands
    )


def test_codex_installation_uses_codex_home_without_nesting() -> None:
    readme = README_PATH.read_text(encoding="utf-8")

    assert 'CODEX_HOME="${CODEX_HOME:-${HOME}/.codex}"' in readme
    assert 'SKILL_DEST="${CODEX_HOME}/skills/quazonai"' in readme
    assert 'mkdir -p "${CODEX_HOME}/skills"' in readme
    assert 'if [ -L "${SKILL_DEST}" ]' in readme
    assert 'elif [ -e "${SKILL_DEST}" ]' in readme
    assert 'ln -s "${SKILL_SOURCE}" "${SKILL_DEST}"' in readme
    assert "ln -sfn" not in readme
    assert 'mkdir -p "${HOME}/.agents/skills"' not in readme


def test_high_risk_argument_shapes_match_documentation() -> None:
    parser = build_parser()

    approve = parser.parse_args(
        [
            "approval",
            "approve",
            "approval-1",
            "--downstream",
            "downstream-1",
        ]
    )
    assert approve.id == "approval-1"
    assert approve.downstream_id == "downstream-1"
    assert approve.expected_state == "PENDING"

    reject = parser.parse_args(
        ["approval", "reject", "approval-1", "--reason", "RISK_LIMIT"]
    )
    assert reject.id == "approval-1"
    assert reject.reason_code == "RISK_LIMIT"
    assert reject.note is None

    revoke = parser.parse_args(
        ["handoff", "revoke", "handoff-1", "--reason", "SUPERSEDED"]
    )
    assert revoke.id == "handoff-1"
    assert revoke.reason_code == "SUPERSEDED"
