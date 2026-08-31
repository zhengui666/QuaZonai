from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8")


def replace_required(path: str, old: str, new: str, *, count: int = -1) -> None:
    text = read(path)
    if old not in text:
        raise RuntimeError(f"required text missing in {path}: {old[:120]!r}")
    write(path, text.replace(old, new, count))


# Backend cleanup: remove obsolete password/username tests and the duplicate subject constant.
settings_tests = read("backend/tests/unit/test_settings.py")
settings_tests, removed = re.subn(
    r"\n\ndef test_enabled_auth_rejects_password_longer_than_login_schema\(.*?\n\n@pytest\.mark\.parametrize\(\n    \"origin\",",
    '\n\n@pytest.mark.parametrize(\n    "origin",',
    settings_tests,
    count=1,
    flags=re.DOTALL,
)
if removed != 1:
    raise RuntimeError("failed to remove obsolete username/password settings tests")
write("backend/tests/unit/test_settings.py", settings_tests)

replace_required(
    "backend/src/operator_auth.py",
    'OPERATOR_SUBJECT = "local-operator"\nOPERATOR_SUBJECT = "local-operator"\n',
    'OPERATOR_SUBJECT = "local-operator"\n',
    count=1,
)

# Frontend tests must await the asynchronous session bootstrap before querying the TOTP input.
front_tests = read("frontend/src/auth/AuthGate.test.tsx")
old_query = "await user.type(screen.getByLabelText('Authenticator code'), '123456');"
if front_tests.count(old_query) < 2:
    raise RuntimeError("expected two synchronous authenticator queries")
front_tests = front_tests.replace(
    old_query,
    "await user.type(await screen.findByLabelText('Authenticator code'), '123456');",
)
front_tests = front_tests.replace(
    "const totp = screen.getByLabelText('رمز المصادقة');",
    "const totp = await screen.findByLabelText('رمز المصادقة');",
    1,
)

# Explicitly prove the six-digit submission gate and duplicate-submit suppression.
marker = "  it('enters the login gate only after logout succeeds', async () => {"
if marker not in front_tests:
    raise RuntimeError("AuthGate insertion marker missing")
extra_frontend_tests = '''  it('requires a complete six-digit authenticator code before submitting', async () => {\n    const fetchMock = vi.fn(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401));\n    vi.stubGlobal('fetch', fetchMock);\n    const user = userEvent.setup();\n\n    renderAuthGate(<div>Workbench ready</div>);\n\n    const totp = await screen.findByLabelText('Authenticator code');\n    const signIn = screen.getByRole('button', { name: 'Sign in' });\n    expect(signIn).toBeDisabled();\n    await user.type(totp, '12345');\n    expect(signIn).toBeDisabled();\n    expect(fetchMock).toHaveBeenCalledTimes(1);\n    await user.type(totp, '6');\n    expect(signIn).toBeEnabled();\n  });\n\n  it('disables the login controls while one submission is pending', async () => {\n    const login = deferredResponse();\n    const fetchMock = vi.fn()\n      .mockImplementationOnce(() => jsonResponse({ error: { code: 'AUTH_REQUIRED' } }, 401))\n      .mockImplementationOnce(() => login.promise);\n    vi.stubGlobal('fetch', fetchMock);\n    const user = userEvent.setup();\n\n    renderAuthGate(<div>Workbench ready</div>);\n\n    const totp = await screen.findByLabelText('Authenticator code');\n    await user.type(totp, '123456');\n    const signIn = screen.getByRole('button', { name: 'Sign in' });\n    await user.click(signIn);\n\n    expect(signIn).toBeDisabled();\n    expect(totp).toBeDisabled();\n    expect(fetchMock).toHaveBeenCalledTimes(2);\n\n    await act(async () => {\n      login.resolve(await jsonResponse({\n        authenticated: true,\n        username: 'local-operator',\n        trusted_browser: false,\n        auth_enabled: true,\n      }));\n      await Promise.resolve();\n    });\n  });\n\n'''
front_tests = front_tests.replace(marker, extra_frontend_tests + marker, 1)
write("frontend/src/auth/AuthGate.test.tsx", front_tests)

# Add focused backend contract tests for the migration invariants that are easy to regress.
write(
    "backend/tests/unit/test_operator_auth_totp_only_contract.py",
    '''from __future__ import annotations\n\nimport base64\nimport json\nimport secrets\nimport time\nfrom dataclasses import replace\n\nimport pytest\nfrom cryptography.hazmat.primitives.ciphers.aead import AESGCM\n\nfrom api.auth import LoginInput\nfrom operator_auth import (\n    COOKIE_NONCE_BYTES,\n    COOKIE_VERSION,\n    OPERATOR_SUBJECT,\n    _read_cookie,\n    _urlsafe_encode,\n    authenticate_machine,\n)\nfrom settings import Settings, SettingsError\n\n\ndef _enabled_settings(settings: Settings) -> Settings:\n    return replace(\n        settings,\n        operator_auth_enabled=True,\n        operator_totp_secret="JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",\n        auth_cookie_key=base64.b64encode(b"b" * 32).decode("ascii"),\n        api_token="machine-token-" + "x" * 32,\n        auth_public_origin="https://quazonai.example.com",\n    )\n\n\ndef _configure_enabled_env(monkeypatch: pytest.MonkeyPatch) -> None:\n    monkeypatch.setenv("QUAZONAI_ENV", "test")\n    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "true")\n    monkeypatch.setenv(\n        "QUAZONAI_AUTH_TOTP_SECRET",\n        "JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",\n    )\n    monkeypatch.setenv(\n        "QUAZONAI_AUTH_COOKIE_KEY",\n        base64.b64encode(b"b" * 32).decode("ascii"),\n    )\n    monkeypatch.setenv("QUAZONAI_API_TOKEN", "machine-token-" + "x" * 32)\n    monkeypatch.setenv("QUAZONAI_AUTH_PUBLIC_ORIGIN", "https://quazonai.example.com")\n\n\ndef test_login_schema_is_totp_only_and_forbids_legacy_fields() -> None:\n    assert set(LoginInput.model_fields) == {"totp_code", "trust_browser"}\n    assert LoginInput(totp_code="123456").trust_browser is False\n\n    with pytest.raises(ValueError):\n        LoginInput.model_validate(\n            {\n                "username": "legacy",\n                "password": "legacy",\n                "totp_code": "123456",\n            }\n        )\n\n\n@pytest.mark.parametrize("legacy_name", ["QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD"])\ndef test_enabled_auth_rejects_nonempty_legacy_browser_credentials_without_echo(\n    monkeypatch: pytest.MonkeyPatch,\n    legacy_name: str,\n) -> None:\n    _configure_enabled_env(monkeypatch)\n    legacy_value = "legacy-secret-must-not-appear"\n    monkeypatch.setenv(legacy_name, legacy_value)\n\n    with pytest.raises(SettingsError) as raised:\n        Settings.from_env()\n\n    message = str(raised.value)\n    assert legacy_name in message\n    assert legacy_value not in message\n\n\n@pytest.mark.parametrize("legacy_name", ["QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD"])\ndef test_empty_legacy_browser_credentials_do_not_enter_settings(\n    monkeypatch: pytest.MonkeyPatch,\n    legacy_name: str,\n) -> None:\n    _configure_enabled_env(monkeypatch)\n    monkeypatch.setenv(legacy_name, "")\n\n    configured = Settings.from_env()\n    configured.validate_operator_auth()\n\n    assert not hasattr(configured, "operator_username")\n    assert not hasattr(configured, "operator_password")\n    assert "operator_username" not in repr(configured)\n    assert "operator_password" not in repr(configured)\n\n\ndef test_disabled_auth_keeps_legacy_values_dormant(\n    monkeypatch: pytest.MonkeyPatch,\n) -> None:\n    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "false")\n    monkeypatch.setenv("QUAZONAI_AUTH_USERNAME", "legacy-user")\n    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", "legacy-password")\n\n    configured = Settings.from_env()\n\n    assert configured.auth_enabled is False\n    assert not hasattr(configured, "operator_username")\n    assert not hasattr(configured, "operator_password")\n\n\ndef test_machine_authentication_uses_fixed_operator_subject(settings: Settings) -> None:\n    configured = _enabled_settings(settings)\n    assert configured.api_token is not None\n\n    identity = authenticate_machine(configured, f"Bearer {configured.api_token}")\n\n    assert identity is not None\n    assert identity.username == OPERATOR_SUBJECT == "local-operator"\n\n\ndef test_cookie_version_three_rejects_a_version_two_session(settings: Settings) -> None:\n    configured = _enabled_settings(settings)\n    assert COOKIE_VERSION == 3\n    issued_at = int(time.time())\n    payload = json.dumps(\n        {\n            "v": 2,\n            "kind": "session",\n            "sub": OPERATOR_SUBJECT,\n            "iat": issued_at,\n            "exp": issued_at + 300,\n        },\n        separators=(",", ":"),\n        sort_keys=True,\n    ).encode("utf-8")\n    nonce = secrets.token_bytes(COOKIE_NONCE_BYTES)\n    old_aad = b"quazonai|operator-auth|cookie=session|version=2"\n    ciphertext = AESGCM(configured.auth_cookie_key_bytes()).encrypt(nonce, payload, old_aad)\n    old_cookie = _urlsafe_encode(nonce + ciphertext)\n\n    assert _read_cookie(configured, old_cookie, kind="session") is None\n''',
)

# DESIGN.md is the fact source: remove current username/password claims and make the
# exact TOTP-only wire/config/cookie migration contract explicit.
design = read("DESIGN.md")
design_replacements = {
    "设为 `true` 时 username/password、TOTP secret、独立 32-byte cookie encryption key、machine API token、public origin 与 bounded TTL 必须全部格式合法": "设为 `true` 时 TOTP secret、独立 32-byte cookie encryption key、machine API token、public origin 与 bounded TTL 必须全部格式合法",
    "正常浏览器登录要求 `username + TOTP`。": "正常浏览器登录只要求 `TOTP`。",
    "密码、TOTP setup secret、cookie key 与 API token 都是启动级 secret": "TOTP setup secret、cookie key 与 API token 都是启动级 secret",
    "在没有密码和 TOTP 的情况下为该浏览器恢复登录": "无需再次输入 TOTP 即可为该浏览器恢复登录",
    "连续失败指数退避但最大 5 秒": "连续失败指数退避但最大 30 秒",
}
for old, new in design_replacements.items():
    if old in design:
        design = design.replace(old, new)
wire_anchor = "- 正常浏览器登录只要求 `TOTP`。TOTP 使用 RFC 6238 兼容 Google Authenticator 的标准 30 秒、6 位配置；允许有限 clock-skew window，不自研 OTP/HMAC 协议；"
wire_contract = wire_anchor + "\n- `POST /api/v1/auth/login` 请求体只允许 `totp_code` 与可选 `trust_browser`（默认 `false`）；`totp_code` 必须最终是恰好 6 个 ASCII 数字，Schema `extra=forbid`，旧 username/password 或任意其他字段不得被静默接受；缺失、格式错误、错误、重放或被限流统一返回 `401 / AUTH_INVALID` 与通用失败文案，不回显请求体或动态码；"
if wire_anchor in design and "请求体只允许 `totp_code`" not in design:
    design = design.replace(wire_anchor, wire_contract, 1)
legacy_anchor = "Operator Authentication 是部署/访问边界，不是新的业务用户、tenant 或 RBAC Domain。V1 只有一个固定 Operator，身份 subject 为 `local-operator`，不得由客户端或环境变量覆盖。浏览器登录是 TOTP-only 单因素认证；Machine API Token 是独立的自动化凭据，不是浏览器登录因子。"
legacy_contract = legacy_anchor + "\n\n旧浏览器 username/password 环境变量已退出受支持配置。认证启用时只要检测到任一非空旧变量，API 必须在启动阶段 fail closed，并且错误只指出变量名而不输出其值；这些旧值不得进入 `Settings`、日志、API、Cookie 或登录验证逻辑。认证关闭时仍保持 direct access，旧变量仅作为 dormant process environment 被忽略。"
if legacy_anchor in design and "旧浏览器 username/password 环境变量已退出" not in design:
    design = design.replace(legacy_anchor, legacy_contract, 1)
cookie_anchor = "- 轮换 cookie key：所有 browser credential 失效，全部重新登录；"
if cookie_anchor in design and "Cookie schema version 从 `2` 升级为 `3`" not in design:
    design = design.replace(
        cookie_anchor,
        cookie_anchor + "\n- 本次 TOTP-only 迁移将 Cookie schema version 从 `2` 升级为 `3`；升级前的 session/trusted-browser cookie 一律 fail closed，部署升级后需要重新输入一次当前 TOTP；无需数据库迁移，也无需在 TOTP setup key 不变时重新绑定验证器；",
        1,
    )
write("DESIGN.md", design)

# OPERATIONS.md: user-facing setup, upgrade and security language.
ops = read("OPERATIONS.md")
ops = ops.replace(
    "TOTP secret、Operator password、cookie key 和 machine API token 都属于启动级 secret",
    "TOTP secret、cookie key 和 machine API token 都属于启动级 secret",
)
ops = ops.replace(
    "`.env` 只负责启动级基础设施与 Operator access：运行环境、PostgreSQL、master key、`QUAZONAI_AUTH_ENABLED`、Operator username/TOTP、browser cookie key、CLI machine token、public origin、存储根目录和 HTTP port。",
    "`.env` 只负责启动级基础设施与 Operator access：运行环境、PostgreSQL、master key、`QUAZONAI_AUTH_ENABLED`、Operator TOTP、browser cookie key、CLI machine token、public origin、存储根目录和 HTTP port。",
)
risk_sentence = "TOTP-only 是单因素登录，抗在线猜测能力弱于密码 + TOTP；若把 Web/API 暴露到公网，仍必须使用 HTTPS、窄化可信代理 CIDR，并优先叠加部署侧网络访问控制。"
ops_anchor = "认证启用后的 Web 登录输入只有 Google Authenticator-compatible 6-digit TOTP。登录页不再展示或提交用户名/密码；`Trust this browser` 行为保持不变。"
if ops_anchor in ops and risk_sentence not in ops:
    ops = ops.replace(ops_anchor, ops_anchor + "\n\n" + risk_sentence, 1)
write("OPERATIONS.md", ops)

# README: align Quick Start and current product terminology with the fact source.
readme = read("README.md")
readme = readme.replace(
    "The browser login then requires username, password, and the current 6-digit authenticator code.",
    "The browser login then requires only the current 6-digit authenticator code.",
)
readme = readme.replace("### Operator 2FA and trusted browsers", "### TOTP Operator Authentication and trusted browsers")
readme = readme.replace(
    "Normal browser sign-in requires the `.env` username/password plus an RFC 6238 TOTP code compatible with Google Authenticator.",
    "Normal browser sign-in requires only an RFC 6238 TOTP code compatible with Google Authenticator. The operator identity is fixed to `local-operator`; browser username/password are not login factors or supported settings.",
)
readme = readme.replace(
    "restores a new session without asking for either the password or TOTP code.",
    "restores a new session without asking for another TOTP code.",
)
readme = readme.replace(
    "The CLI never reads the browser cookie, Operator password, or TOTP setup secret",
    "The CLI never reads the browser cookie or TOTP setup secret",
)
readme_anchor = "Normal browser sign-in requires only an RFC 6238 TOTP code compatible with Google Authenticator. The operator identity is fixed to `local-operator`; browser username/password are not login factors or supported settings."
readme_risk = "TOTP-only is single-factor authentication and is weaker against online guessing than password + TOTP. Internet-facing deployments should still use HTTPS, narrowly scoped trusted-proxy configuration, and deployment-level network access controls. Non-empty deprecated browser username/password environment settings fail startup closed when authentication is enabled. This migration also invalidates pre-v3 session/trusted-browser cookies, so upgraded browsers must enter one current TOTP once; no database migration or authenticator re-binding is required when the TOTP setup secret is unchanged."
if readme_anchor in readme and readme_risk not in readme:
    readme = readme.replace(readme_anchor, readme_anchor + "\n\n" + readme_risk, 1)
write("README.md", readme)

# CLI/Skill surfaces must describe machine-token auth independently from browser TOTP.
for path in (
    "CLI.md",
    "skills/quazonai/SKILL.md",
    "skills/quazonai/references/authentication.md",
    "skills/quazonai/references/cli-reference.md",
    "skills/quazonai/references/workflows.md",
):
    text = read(path)
    text = text.replace("Operator password", "browser TOTP setup secret")
    text = text.replace("password + TOTP", "TOTP")
    text = text.replace("password/TOTP", "TOTP")
    write(path, text)

skill_test = read("backend/tests/unit/test_agent_skill_auth_contract.py")
skill_test = skill_test.replace(
    '    assert "TOTP setup secret" in combined\n    assert "TOTP setup secret" in combined\n',
    '    assert "TOTP setup secret" in combined\n    assert "Operator password" not in combined\n',
)
write("backend/tests/unit/test_agent_skill_auth_contract.py", skill_test)

# Final contract hygiene checks before CI receives the commit.
source = read("backend/src/operator_auth.py")
assert source.count('OPERATOR_SUBJECT = "local-operator"') == 1
assert "operator_username" not in source
assert "operator_password" not in source

for path in (".env.example", "compose.yml", ".github/workflows/operator-auth-e2e.yml"):
    text = read(path)
    assert "QUAZONAI_AUTH_USERNAME" not in text
    assert "QUAZONAI_AUTH_PASSWORD" not in text

for path in (
    "frontend/src/auth/AuthGate.tsx",
    "frontend/e2e/operator-auth.spec.ts",
):
    text = read(path)
    assert "QUAZONAI_E2E_AUTH_USERNAME" not in text
    assert "QUAZONAI_E2E_AUTH_PASSWORD" not in text

assert "requires username, password" not in read("README.md")
assert "Operator 2FA" not in read("README.md")
assert "username + TOTP" not in read("DESIGN.md")
assert "Operator username/TOTP" not in read("OPERATIONS.md")

print("Issue #37 final contract cleanup applied")
