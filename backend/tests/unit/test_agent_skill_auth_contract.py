from __future__ import annotations

from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
SKILL_DIR = REPO_ROOT / "skills" / "quazonai"
SKILL_PATH = SKILL_DIR / "SKILL.md"
AUTHENTICATION_PATH = SKILL_DIR / "references" / "authentication.md"
CLI_REFERENCE_PATH = SKILL_DIR / "references" / "cli-reference.md"
WORKFLOWS_PATH = SKILL_DIR / "references" / "workflows.md"


def _combined_skill_authentication_contract() -> str:
    return "\n".join(
        path.read_text(encoding="utf-8")
        for path in (
            SKILL_PATH,
            AUTHENTICATION_PATH,
            CLI_REFERENCE_PATH,
            WORKFLOWS_PATH,
        )
    )


def test_portable_skill_documents_operator_machine_authentication() -> None:
    skill = SKILL_PATH.read_text(encoding="utf-8")
    authentication = AUTHENTICATION_PATH.read_text(encoding="utf-8")
    reference = CLI_REFERENCE_PATH.read_text(encoding="utf-8")
    workflows = WORKFLOWS_PATH.read_text(encoding="utf-8")
    combined = _combined_skill_authentication_contract()

    assert "QUAZONAI_API_TOKEN" in skill
    assert "QUAZONAI_API_TOKEN" in authentication
    assert "QUAZONAI_API_TOKEN" in reference
    assert "QUAZONAI_API_TOKEN" in workflows
    assert "AUTH_REQUIRED" in combined
    assert 'test -n "${QUAZONAI_API_TOKEN:-}"' in combined
    assert "never print" in combined
    assert "TOTP setup secret" in combined
    assert "TOTP-only" in authentication
    assert "local-operator" in authentication
    assert "Operator password" not in combined
    assert "downstream Handoff service token" in combined


def test_skill_does_not_advertise_browser_credentials_as_cli_authentication() -> None:
    combined = _combined_skill_authentication_contract()

    forbidden_instructions = (
        "export QUAZONAI_AUTH_PASSWORD",
        "export QUAZONAI_AUTH_TOTP_SECRET",
        "--password",
        "--totp",
        "copy browser cookie",
        "password + RFC 6238 TOTP browser login",
        "complete username/TOTP/cookie-key/machine-token/public-origin",
    )
    for instruction in forbidden_instructions:
        assert instruction not in combined
