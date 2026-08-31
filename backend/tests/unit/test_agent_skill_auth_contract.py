from __future__ import annotations

from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parents[3]
DESIGN_PATH = REPO_ROOT / "DESIGN.md"
OPERATIONS_PATH = REPO_ROOT / "OPERATIONS.md"
CLI_PATH = REPO_ROOT / "CLI.md"
README_PATH = REPO_ROOT / "README.md"
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


def _combined_governed_authentication_docs() -> str:
    return "\n".join(
        path.read_text(encoding="utf-8")
        for path in (
            DESIGN_PATH,
            OPERATIONS_PATH,
            CLI_PATH,
            README_PATH,
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


def test_portable_skill_directly_forbids_browser_secret_access() -> None:
    skill = SKILL_PATH.read_text(encoding="utf-8")
    prohibition = (
        "never request, read, infer, capture, copy, print, or store the browser "
        "TOTP setup secret, one-time authenticator code, session cookie, or "
        "trusted-browser cookie"
    )
    authentication_reference = (
        "Read [references/authentication.md](references/authentication.md) before "
        "diagnosing an authentication failure or credential boundary."
    )

    assert skill.count(prohibition) == 1
    assert "those credentials are outside the Skill and CLI boundary" in skill
    assert skill.count(authentication_reference) == 1


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


def test_governed_docs_keep_the_browser_login_contract_totp_only() -> None:
    design = DESIGN_PATH.read_text(encoding="utf-8")
    combined = _combined_governed_authentication_docs()

    assert (
        "只输入 Google Authenticator-compatible 6 位 TOTP 动态码，"
        "不展示、缓存或提交 username/password"
    ) in design
    forbidden_current_contracts = (
        "输入单 Operator username、password 和 6 位 authenticator code",
        "Normal browser sign-in requires the `.env` username/password",
        "Operator 2FA and trusted browsers",
        "password + RFC 6238 TOTP browser login",
        "complete username/TOTP/cookie-key/machine-token/public-origin",
    )
    for stale_contract in forbidden_current_contracts:
        assert stale_contract not in combined
