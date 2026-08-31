from __future__ import annotations

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
            f"required text missing in {path}: expected at least {count}, found {actual}: {old[:160]!r}"
        )
    write(path, text.replace(old, new, count))


# Preserve fail-closed upgrade detection in Compose without passing either legacy secret.
replace_required(
    "backend/src/settings.py",
    '_ALLOWED_ENVIRONMENTS = frozenset({"development", "production", "test"})\n',
    '_ALLOWED_ENVIRONMENTS = frozenset({"development", "production", "test"})\n'
    '_LEGACY_OPERATOR_AUTH_ENV_MARKERS = (\n'
    '    ("QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT"),\n'
    '    ("QUAZONAI_AUTH_PASSWORD", "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT"),\n'
    ')\n',
)
replace_required(
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
replace_required(
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
replace_required(
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

# Lock the marker behavior and ensure only legacy variable names reach error output.
totp_contract = read("backend/tests/unit/test_operator_auth_totp_only_contract.py")
marker_test_anchor = (
    '\n\n@pytest.mark.parametrize("legacy_name", ["QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD"])\n'
    'def test_empty_legacy_browser_credentials_do_not_enter_settings(\n'
)
marker_test = '''\n\n@pytest.mark.parametrize(\n    ("legacy_name", "presence_marker"),\n    [\n        ("QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT"),\n        ("QUAZONAI_AUTH_PASSWORD", "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT"),\n    ],\n)\ndef test_enabled_auth_rejects_compose_names_only_legacy_markers(\n    monkeypatch: pytest.MonkeyPatch,\n    legacy_name: str,\n    presence_marker: str,\n) -> None:\n    _configure_enabled_env(monkeypatch)\n    monkeypatch.delenv(legacy_name, raising=False)\n    monkeypatch.setenv(presence_marker, "true")\n\n    with pytest.raises(SettingsError) as raised:\n        Settings.from_env()\n\n    message = str(raised.value)\n    assert legacy_name in message\n    assert presence_marker not in message\n\n\n@pytest.mark.parametrize("legacy_name", ["QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD"])\ndef test_empty_legacy_browser_credentials_do_not_enter_settings(\n'''
if marker_test_anchor not in totp_contract:
    raise RuntimeError("TOTP contract insertion anchor missing")
totp_contract = totp_contract.replace(marker_test_anchor, marker_test, 1)
totp_contract = totp_contract.replace(
    '    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", "legacy-password")\n\n'
    '    configured = Settings.from_env()\n',
    '    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", "legacy-password")\n'
    '    monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT", "true")\n'
    '    monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT", "true")\n\n'
    '    configured = Settings.from_env()\n',
    1,
)
write("backend/tests/unit/test_operator_auth_totp_only_contract.py", totp_contract)

# Prove Compose carries only boolean presence and never maps the raw legacy keys/values.
review_tests = read("backend/tests/unit/test_operator_auth_review_regressions.py")
review_anchor = '\n\ndef test_compose_disables_uvicorn_proxy_header_rewriting() -> None:\n'
review_insert = '''\n\ndef test_compose_preserves_names_only_legacy_auth_detection() -> None:\n    compose = (REPO_ROOT / "compose.yml").read_text(encoding="utf-8")\n    api_environment = compose.split("\\n  api:", maxsplit=1)[1].split(\n        "\\n    extra_hosts:", maxsplit=1\n    )[0]\n\n    assert "\\n      QUAZONAI_AUTH_USERNAME:" not in api_environment\n    assert "\\n      QUAZONAI_AUTH_PASSWORD:" not in api_environment\n    assert (\n        'QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT: "${QUAZONAI_AUTH_USERNAME:+true}"'\n        in api_environment\n    )\n    assert (\n        'QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT: "${QUAZONAI_AUTH_PASSWORD:+true}"'\n        in api_environment\n    )\n\n\ndef test_design_operator_login_is_totp_only() -> None:\n    design = (REPO_ROOT / "DESIGN.md").read_text(encoding="utf-8")\n\n    assert "输入单 Operator username、password 和 6 位 authenticator code" not in design\n    assert "输入 Google Authenticator-compatible 6 位动态码" in design\n\n\ndef test_compose_disables_uvicorn_proxy_header_rewriting() -> None:\n'''
if review_anchor not in review_tests:
    raise RuntimeError("review regression insertion anchor missing")
write(
    "backend/tests/unit/test_operator_auth_review_regressions.py",
    review_tests.replace(review_anchor, review_insert, 1),
)

# The Mission child must scrub both raw legacy variables and names-only markers.
secret_tests = read("backend/tests/unit/test_operator_auth_secret_isolation.py")
secret_tests = secret_tests.replace(
    '    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", legacy_password)\n',
    '    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", legacy_password)\n'
    '    monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT", "true")\n'
    '    monkeypatch.setenv("QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT", "true")\n',
    1,
)
secret_tests = secret_tests.replace(
    '        "QUAZONAI_AUTH_PASSWORD",\n'
    '        "QUAZONAI_AUTH_TOTP_SECRET",\n',
    '        "QUAZONAI_AUTH_PASSWORD",\n'
    '        "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT",\n'
    '        "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT",\n'
    '        "QUAZONAI_AUTH_TOTP_SECRET",\n',
    1,
)
write("backend/tests/unit/test_operator_auth_secret_isolation.py", secret_tests)

# Correct the remaining fact-source drift and document the Compose bridge precisely.
replace_required(
    "DESIGN.md",
    '旧浏览器 username/password 环境变量已退出受支持配置。认证启用时只要检测到任一非空旧变量，API 必须在启动阶段 fail closed，并且错误只指出变量名而不输出其值；这些旧值不得进入 `Settings`、日志、API、Cookie 或登录验证逻辑。认证关闭时仍保持 direct access，旧变量仅作为 dormant process environment 被忽略。\n',
    '旧浏览器 username/password 环境变量已退出受支持配置。认证启用时只要检测到任一非空旧变量，API 必须在启动阶段 fail closed，并且错误只指出变量名而不输出其值；这些旧值不得进入 `Settings`、日志、API、Cookie 或登录验证逻辑。Compose 只把旧变量的非空状态转换为内部 names-only presence marker，绝不把旧值注入 API container；API 将 marker 映射回对应旧变量名并执行同一 fail-closed 错误。认证关闭时仍保持 direct access，旧变量与 presence marker 都视为 dormant process environment。\n',
)
replace_required(
    "DESIGN.md",
    '- 输入单 Operator username、password 和 6 位 authenticator code；\n',
    '- 输入 Google Authenticator-compatible 6 位动态码；\n',
)

# Sanity-check the migration before committing.
settings_text = read("backend/src/settings.py")
for marker in (
    "QUAZONAI_AUTH_LEGACY_USERNAME_PRESENT",
    "QUAZONAI_AUTH_LEGACY_PASSWORD_PRESENT",
):
    if marker not in settings_text:
        raise RuntimeError(f"missing Settings marker detection: {marker}")
if "输入单 Operator username、password" in read("DESIGN.md"):
    raise RuntimeError("stale password-based login instruction remains in DESIGN.md")

print("Applied Issue 37 Codex review fixes.")
