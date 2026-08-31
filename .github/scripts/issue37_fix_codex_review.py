from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8")


def ensure_replace(path: str, old: str, new: str) -> None:
    text = read(path)
    if new in text:
        return
    if old not in text:
        raise RuntimeError(f"migration anchor missing in {path}: {old[:160]!r}")
    write(path, text.replace(old, new, 1))


# Compose must preserve upgrade fail-closed behavior without forwarding either old secret.
ensure_replace(
    "backend/src/settings.py",
    '_ALLOWED_ENVIRONMENTS = frozenset({"development", "production", "test"})\n',
    '_ALLOWED_ENVIRONMENTS = frozenset({"development", "production", "test"})\n'
    '_LEGACY_OPERATOR_AUTH_ENV_MARKERS = (\n'
    '    ("QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT"),\n'
    '    ("QUAZONAI_AUTH_PASSWORD", "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT"),\n'
    ')\n',
)
ensure_replace(
    "backend/src/settings.py",
    '        legacy_auth_variables = tuple(\n'
    '            name\n'
    '            for name in ("QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD")\n'
    '            if _optional_raw_env(name) is not None\n'
    '        )\n',
    '        legacy_auth_variables = tuple(\n'
    '            legacy_name\n'
    '            for legacy_name, presence_marker in _LEGACY_OPERATOR_AUTH_ENV_MARKERS\n'
    '            if (\n'
    '                _optional_raw_env(legacy_name) is not None\n'
    '                or _optional_env(presence_marker) is not None\n'
    '            )\n'
    '        )\n',
)
ensure_replace(
    "compose.yml",
    '      QUAZONAI_AUTH_ENABLED: ${QUAZONAI_AUTH_ENABLED:-false}\n'
    '      QUAZONAI_AUTH_TOTP_SECRET: ${QUAZONAI_AUTH_TOTP_SECRET:-}\n',
    '      QUAZONAI_AUTH_ENABLED: ${QUAZONAI_AUTH_ENABLED:-false}\n'
    '      # Convert deprecated non-empty secrets to names-only presence markers.\n'
    '      # The legacy values themselves never enter the API container.\n'
    '      QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT: "${QUAZONAI_AUTH_USERNAME:+true}"\n'
    '      QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT: "${QUAZONAI_AUTH_PASSWORD:+true}"\n'
    '      QUAZONAI_AUTH_TOTP_SECRET: ${QUAZONAI_AUTH_TOTP_SECRET:-}\n',
)
ensure_replace(
    "backend/src/runners/research_missions.py",
    '        "QUAZONAI_AUTH_USERNAME": "",\n'
    '        "QUAZONAI_AUTH_PASSWORD": "",\n'
    '        "QUAZONAI_AUTH_TOTP_SECRET": "",\n',
    '        "QUAZONAI_AUTH_USERNAME": "",\n'
    '        "QUAZONAI_AUTH_PASSWORD": "",\n'
    '        "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT": "",\n'
    '        "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT": "",\n'
    '        "QUAZONAI_AUTH_TOTP_SECRET": "",\n',
)

# Unit coverage for names-only marker detection and dormant disabled-auth behavior.
totp_path = "backend/tests/unit/test_operator_auth_totp_only_contract.py"
totp = read(totp_path)
marker_test_name = "test_enabled_auth_rejects_compose_names_only_legacy_markers"
if marker_test_name not in totp:
    anchor = (
        '\n\n@pytest.mark.parametrize("legacy_name", ["QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD"])\n'
        'def test_empty_legacy_browser_credentials_do_not_enter_settings(\n'
    )
    addition = '''\n\n@pytest.mark.parametrize(\n    ("legacy_name", "presence_marker"),\n    [\n        ("QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT"),\n        ("QUAZONAI_AUTH_PASSWORD", "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT"),\n    ],\n)\ndef test_enabled_auth_rejects_compose_names_only_legacy_markers(\n    monkeypatch: pytest.MonkeyPatch,\n    legacy_name: str,\n    presence_marker: str,\n) -> None:\n    _configure_enabled_env(monkeypatch)\n    monkeypatch.delenv(legacy_name, raising=False)\n    monkeypatch.setenv(presence_marker, "true")\n\n    with pytest.raises(SettingsError) as raised:\n        Settings.from_env()\n\n    message = str(raised.value)\n    assert legacy_name in message\n    assert presence_marker not in message\n\n\n@pytest.mark.parametrize("legacy_name", ["QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD"])\ndef test_empty_legacy_browser_credentials_do_not_enter_settings(\n'''
    if anchor not in totp:
        raise RuntimeError("TOTP marker-test insertion anchor missing")
    totp = totp.replace(anchor, addition, 1)
if 'monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT", "true")' not in totp:
    anchor = (
        '    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", "legacy-password")\n\n'
        '    configured = Settings.from_env()\n'
    )
    replacement = (
        '    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", "legacy-password")\n'
        '    monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT", "true")\n'
        '    monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT", "true")\n\n'
        '    configured = Settings.from_env()\n'
    )
    if anchor not in totp:
        raise RuntimeError("disabled-auth marker insertion anchor missing")
    totp = totp.replace(anchor, replacement, 1)
write(totp_path, totp)

# Static bridge proof: Compose maps presence only, never legacy variable keys or values.
review_path = "backend/tests/unit/test_operator_auth_review_regressions.py"
review = read(review_path)
if "test_compose_preserves_names_only_legacy_auth_detection" not in review:
    anchor = '\n\ndef test_compose_disables_uvicorn_proxy_header_rewriting() -> None:\n'
    addition = '''\n\ndef test_compose_preserves_names_only_legacy_auth_detection() -> None:\n    compose = (REPO_ROOT / "compose.yml").read_text(encoding="utf-8")\n    api_environment = compose.split("\\n  api:", maxsplit=1)[1].split(\n        "\\n    extra_hosts:", maxsplit=1\n    )[0]\n\n    assert "\\n      QUAZONAI_AUTH_USERNAME:" not in api_environment\n    assert "\\n      QUAZONAI_AUTH_PASSWORD:" not in api_environment\n    assert (\n        'QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT: "${QUAZONAI_AUTH_USERNAME:+true}"'\n        in api_environment\n    )\n    assert (\n        'QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT: "${QUAZONAI_AUTH_PASSWORD:+true}"'\n        in api_environment\n    )\n\n\ndef test_compose_disables_uvicorn_proxy_header_rewriting() -> None:\n'''
    if anchor not in review:
        raise RuntimeError("Compose regression-test insertion anchor missing")
    review = review.replace(anchor, addition, 1)
write(review_path, review)

# Names-only markers are still Core auth metadata and must not reach Mission children.
secret_path = "backend/tests/unit/test_operator_auth_secret_isolation.py"
secret = read(secret_path)
if 'monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT", "true")' not in secret:
    secret = secret.replace(
        '    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", legacy_password)\n',
        '    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", legacy_password)\n'
        '    monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT", "true")\n'
        '    monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT", "true")\n',
        1,
    )
if '        "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT",\n' not in secret:
    secret = secret.replace(
        '        "QUAZONAI_AUTH_PASSWORD",\n'
        '        "QUAZONAI_AUTH_TOTP_SECRET",\n',
        '        "QUAZONAI_AUTH_PASSWORD",\n'
        '        "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT",\n'
        '        "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT",\n'
        '        "QUAZONAI_AUTH_TOTP_SECRET",\n',
        1,
    )
write(secret_path, secret)

# Keep the fact source explicit about the value-free Compose bridge.
ensure_replace(
    "DESIGN.md",
    '旧浏览器 username/password 环境变量已退出受支持配置。认证启用时只要检测到任一非空旧变量，API 必须在启动阶段 fail closed，并且错误只指出变量名而不输出其值；这些旧值不得进入 `Settings`、日志、API、Cookie 或登录验证逻辑。认证关闭时仍保持 direct access，旧变量仅作为 dormant process environment 被忽略。\n',
    '旧浏览器 username/password 环境变量已退出受支持配置。认证启用时只要检测到任一非空旧变量，API 必须在启动阶段 fail closed，并且错误只指出变量名而不输出其值；这些旧值不得进入 `Settings`、日志、API、Cookie 或登录验证逻辑。Compose 只把旧变量的非空状态转换为内部 names-only presence marker，绝不把旧值注入 API container；API 将 marker 映射回对应旧变量名并执行同一 fail-closed 错误。认证关闭时仍保持 direct access，旧变量与 presence marker 都视为 dormant process environment。\n',
)

# Final invariants.
for path, needle in (
    ("backend/src/settings.py", "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT"),
    ("compose.yml", '${QUAZONAI_AUTH_USERNAME:+true}'),
    ("backend/src/runners/research_missions.py", "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT"),
):
    if needle not in read(path):
        raise RuntimeError(f"missing final marker contract in {path}: {needle}")
if "输入单 Operator username、password" in read("DESIGN.md"):
    raise RuntimeError("stale password-based login instruction remains in DESIGN.md")

print("Applied Issue 37 Compose legacy-guard review fix.")
