from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8")


def replace_required(path: str, old: str, new: str, *, count: int = 1) -> None:
    text = read(path)
    actual = text.count(old)
    if actual < count:
        raise RuntimeError(
            f"required text missing in {path}: expected at least {count}, found {actual}: {old[:120]!r}"
        )
    write(path, text.replace(old, new, count))


# Remove stale direct Settings fields from every remaining auth test helper.
stale_fields = (
    '        "operator_username": "operator",\n'
    '        "operator_password": "correct horse battery staple",\n'
)
for test_path in (
    "backend/tests/unit/test_settings.py",
    "backend/tests/unit/test_operator_auth_configuration_hardening.py",
    "backend/tests/unit/test_operator_auth_review_regressions.py",
):
    replace_required(test_path, stale_fields, "")

# Username/password text validation no longer belongs to the supported Settings contract.
configuration_tests = read(
    "backend/tests/unit/test_operator_auth_configuration_hardening.py"
)
obsolete_pattern = re.compile(
    r"\n\n@pytest\.mark\.parametrize\(\n"
    r"    \(\"field\", \"value\"\),\n"
    r"    \[\n"
    r"        \(\"operator_username\", \"operator\\nname\"\),\n"
    r"        \(\"operator_username\", \"operator\\rname\"\),\n"
    r"        \(\"operator_password\", \"correct horse\\nbattery staple\"\),\n"
    r"        \(\"operator_password\", \"correct horse\\rbattery staple\"\),\n"
    r"    \],\n"
    r"\)\n"
    r"def test_enabled_auth_rejects_browser_unrepresentable_line_breaks\(\n"
    r"    settings: Settings,\n"
    r"    field: str,\n"
    r"    value: str,\n"
    r"\) -> None:\n"
    r"    configured = replace\(_enabled_auth\(settings\), \*\*\{field: value\}\)\n\n"
    r"    with pytest\.raises\(SettingsError, match=\"must not contain carriage returns or line feeds\"\):\n"
    r"        configured\.validate_operator_auth\(\)\n"
)
configuration_tests, removed = obsolete_pattern.subn("", configuration_tests, count=1)
if removed != 1:
    raise RuntimeError("failed to remove obsolete username/password text-validation test")
write(
    "backend/tests/unit/test_operator_auth_configuration_hardening.py",
    configuration_tests,
)

# Keep the source-of-truth bootstrap list complete after removing the combined legacy line.
replace_required(
    "DESIGN.md",
    "QUAZONAI_AUTH_ENABLED\nQUAZONAI_AUTH_COOKIE_KEY / QUAZONAI_API_TOKEN / QUAZONAI_AUTH_PUBLIC_ORIGIN",
    "QUAZONAI_AUTH_ENABLED\nQUAZONAI_AUTH_TOTP_SECRET\nQUAZONAI_AUTH_COOKIE_KEY / QUAZONAI_API_TOKEN / QUAZONAI_AUTH_PUBLIC_ORIGIN",
)

# Re-state the CLI boundary without implying that the retired password remains a login factor.
replace_required(
    "CLI.md",
    "- `QUAZONAI_AUTH_ENABLED=true` 时，CLI 从环境读取 `QUAZONAI_API_TOKEN` 并以 `Authorization: Bearer` 调用 Operator API；认证关闭时该 token 不是必需项；\n- machine token 不授予 downstream-owned Handoff claim/accept/reject/package/feedback 权限；",
    "- `QUAZONAI_AUTH_ENABLED=true` 时，CLI 从环境读取 `QUAZONAI_API_TOKEN` 并以 `Authorization: Bearer` 调用 Operator API；认证关闭时该 token 不是必需项；\n- CLI 不读取或存储浏览器 TOTP setup secret、session/trusted-browser cookie，也不使用已废弃的 `QUAZONAI_AUTH_USERNAME` / `QUAZONAI_AUTH_PASSWORD`；\n- machine token 不授予 downstream-owned Handoff claim/accept/reject/package/feedback 权限；",
)
replace_required(
    "CLI.md",
    "- CLI 不读取/存储 browser TOTP setup secret、TOTP secret 或 browser cookies；",
    "- CLI 不读取/存储 browser TOTP setup secret、session/trusted-browser cookies 或已废弃的 browser username/password；",
)

# User-facing backoff and failure wording must match the 30-second TOTP-only contract.
replace_required(
    "OPERATIONS.md",
    "连续失败登录会触发 1–5 秒的短退避，但不会形成持久账户锁定；被限制的请求仍显示统一的无效凭据错误。",
    "连续失败登录会触发最长 30 秒的有界短退避，但不会形成持久账户锁定；被限制的请求仍显示统一的认证失败。",
)
replace_required(
    "OPERATIONS.md",
    "Operator Authentication 启用时，CLI/automation 不使用 Web cookie、密码或 TOTP，",
    "Operator Authentication 启用时，CLI/automation 不使用 Web cookie、浏览器 TOTP 或已废弃的用户名/密码，",
)

# Fail the one-shot migration if any direct construction still targets removed Settings fields.
residuals: list[str] = []
for path in sorted((ROOT / "backend/tests").rglob("*.py")):
    text = path.read_text(encoding="utf-8")
    for match in re.finditer(r"\boperator_(?:username|password)\s*=", text):
        line = text.count("\n", 0, match.start()) + 1
        residuals.append(f"{path.relative_to(ROOT)}:{line}")
for path in sorted((ROOT / "backend/src").rglob("*.py")):
    text = path.read_text(encoding="utf-8")
    for forbidden in ("settings.operator_username", "settings.operator_password"):
        if forbidden in text:
            residuals.append(f"{path.relative_to(ROOT)} contains {forbidden}")
if residuals:
    raise RuntimeError("residual active username/password settings references: " + ", ".join(residuals))

print("Issue 37 residual auth contract cleanup completed.")
