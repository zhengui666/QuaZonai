from __future__ import annotations

import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]


def read(path: str) -> str:
    return (ROOT / path).read_text(encoding="utf-8")


def write(path: str, text: str) -> None:
    (ROOT / path).write_text(text, encoding="utf-8")


def replace(path: str, old: str, new: str, *, required: bool = True) -> None:
    text = read(path)
    if old not in text:
        if required:
            raise RuntimeError(f"expected text not found in {path}: {old[:120]!r}")
        return
    write(path, text.replace(old, new))


def sub(path: str, pattern: str, replacement: str, *, count: int = 0, required: bool = True) -> None:
    text = read(path)
    updated, changes = re.subn(pattern, replacement, text, count=count, flags=re.MULTILINE | re.DOTALL)
    if changes == 0 and required:
        raise RuntimeError(f"expected pattern not found in {path}: {pattern[:120]!r}")
    write(path, updated)


def remove_keyword_lines(path: Path, names: tuple[str, ...]) -> None:
    text = path.read_text(encoding="utf-8")
    lines = [
        line
        for line in text.splitlines(keepends=True)
        if not any(f"{name}=" in line for name in names)
    ]
    path.write_text("".join(lines), encoding="utf-8")


# ---------------------------------------------------------------------------
# Settings contract: TOTP-only, fixed operator identity, legacy fail-closed.
# ---------------------------------------------------------------------------
settings_path = "backend/src/settings.py"
for line in (
    "MAX_OPERATOR_USERNAME_CHARACTERS = 200\n",
    "MIN_OPERATOR_PASSWORD_CHARACTERS = 12\n",
    "MAX_OPERATOR_PASSWORD_CHARACTERS = 4096\n",
):
    replace(settings_path, line, "")
sub(
    settings_path,
    r"\ndef _validate_utf8_text\(value: str, \*, name: str\) -> None:\n.*?\n\ndef validate_machine_api_token",
    "\n\ndef validate_machine_api_token",
)
replace(settings_path, "    operator_username: str | None = None\n", "")
replace(settings_path, "    operator_password: str | None = None\n", "")
replace(
    settings_path,
    '        operator_auth_enabled = _env_bool("QUAZONAI_AUTH_ENABLED", False)\n',
    '        operator_auth_enabled = _env_bool("QUAZONAI_AUTH_ENABLED", False)\n'
    '        legacy_auth_variables = tuple(\n'
    '            name\n'
    '            for name in ("QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD")\n'
    '            if _optional_raw_env(name) is not None\n'
    '        )\n'
    '        if operator_auth_enabled and legacy_auth_variables:\n'
    '            raise SettingsError(\n'
    '                "Operator authentication no longer supports the deprecated "\n'
    '                "username/password variables; remove: " + ", ".join(legacy_auth_variables)\n'
    '            )\n',
)
replace(settings_path, '            operator_username=_optional_raw_env("QUAZONAI_AUTH_USERNAME"),\n', "")
replace(settings_path, '            operator_password=_optional_raw_env("QUAZONAI_AUTH_PASSWORD"),\n', "")
sub(
    settings_path,
    r"        fields = \{\n            \"QUAZONAI_AUTH_USERNAME\": self\.operator_username,\n            \"QUAZONAI_AUTH_PASSWORD\": self\.operator_password,\n            \"QUAZONAI_AUTH_TOTP_SECRET\": self\.operator_totp_secret,\n            \"QUAZONAI_AUTH_COOKIE_KEY\": self\.auth_cookie_key,\n            \"QUAZONAI_API_TOKEN\": self\.api_token,\n            \"QUAZONAI_AUTH_PUBLIC_ORIGIN\": self\.auth_public_origin,\n        \}\n        missing = \[name for name, value in fields\.items\(\) if not value\]\n        if missing:\n            raise SettingsError\(\n                \"Operator authentication is enabled but incomplete; missing: \"\n                \+ \", \"\.join\(missing\)\n            \)\n\n        assert self\.operator_username is not None\n        assert self\.operator_password is not None\n        assert self\.operator_totp_secret is not None\n        assert self\.api_token is not None\n        assert self\.auth_public_origin is not None\n\n        _validate_utf8_text\(self\.operator_username, name=\"QUAZONAI_AUTH_USERNAME\"\)\n        _validate_utf8_text\(self\.operator_password, name=\"QUAZONAI_AUTH_PASSWORD\"\)\n        for name, credential in \(\n            \(\"QUAZONAI_AUTH_USERNAME\", self\.operator_username\),\n            \(\"QUAZONAI_AUTH_PASSWORD\", self\.operator_password\),\n        \):\n            if \"\\\\r\" in credential or \"\\\\n\" in credential:\n                raise SettingsError\(\n                    f\"\{name\} must not contain carriage returns or line feeds\"\n                \)\n        if len\(self\.operator_username\) > MAX_OPERATOR_USERNAME_CHARACTERS:\n            raise SettingsError\(\n                f\"QUAZONAI_AUTH_USERNAME must contain at most \"\n                f\"\{MAX_OPERATOR_USERNAME_CHARACTERS\} characters\"\n            \)\n        if not \(\n            MIN_OPERATOR_PASSWORD_CHARACTERS\n            <= len\(self\.operator_password\)\n            <= MAX_OPERATOR_PASSWORD_CHARACTERS\n        \):\n            raise SettingsError\(\n                \"QUAZONAI_AUTH_PASSWORD must contain between \"\n                f\"\{MIN_OPERATOR_PASSWORD_CHARACTERS\} and \"\n                f\"\{MAX_OPERATOR_PASSWORD_CHARACTERS\} characters\"\n            \)\n        validate_machine_api_token\(self\.api_token\)",
    '        fields = {\n'
    '            "QUAZONAI_AUTH_TOTP_SECRET": self.operator_totp_secret,\n'
    '            "QUAZONAI_AUTH_COOKIE_KEY": self.auth_cookie_key,\n'
    '            "QUAZONAI_API_TOKEN": self.api_token,\n'
    '            "QUAZONAI_AUTH_PUBLIC_ORIGIN": self.auth_public_origin,\n'
    '        }\n'
    '        missing = [name for name, value in fields.items() if not value]\n'
    '        if missing:\n'
    '            raise SettingsError(\n'
    '                "Operator authentication is enabled but incomplete; missing: "\n'
    '                + ", ".join(missing)\n'
    '            )\n\n'
    '        assert self.operator_totp_secret is not None\n'
    '        assert self.api_token is not None\n'
    '        assert self.auth_public_origin is not None\n\n'
    '        validate_machine_api_token(self.api_token)',
)


# ---------------------------------------------------------------------------
# Authentication primitives: fixed subject, TOTP-only login, cookie v3.
# ---------------------------------------------------------------------------
auth_core = "backend/src/operator_auth.py"
replace(auth_core, 'STREAM_ADMISSION_GENERATION_STATE_ATTRIBUTE = "operator_auth_stream_generation"\n', 'STREAM_ADMISSION_GENERATION_STATE_ATTRIBUTE = "operator_auth_stream_generation"\nOPERATOR_SUBJECT = "local-operator"\n')
replace(auth_core, "COOKIE_VERSION = 2\n", "COOKIE_VERSION = 3\n")
replace(auth_core, "LOGIN_MAX_BACKOFF_SECONDS = 5.0\n", "LOGIN_MAX_BACKOFF_SECONDS = 30.0\n")
replace(
    auth_core,
    '    username: str\n    cookie_generation: int | None\n    cookie_issuance_epoch: str | None\n    browser_epoch: str | None\n\n\n@dataclass(frozen=True, slots=True)\nclass CookieIssuance:',
    '    subject: str\n    cookie_generation: int | None\n    cookie_issuance_epoch: str | None\n    browser_epoch: str | None\n\n\n@dataclass(frozen=True, slots=True)\nclass CookieIssuance:',
)
replace(auth_core, "password + TOTP", "TOTP", required=False)
replace(auth_core, "password/TOTP", "TOTP", required=False)
replace(auth_core, "password or TOTP", "TOTP", required=False)
replace(auth_core, "password/TOTP verification", "TOTP verification", required=False)
replace(auth_core, "        assert settings.operator_username is not None\n", "", required=False)
replace(auth_core, '        "sub": settings.operator_username,\n', '        "sub": OPERATOR_SUBJECT,\n')
replace(auth_core, '        username = payload.get("sub")\n', '        subject = payload.get("sub")\n')
replace(auth_core, '        if not isinstance(username, str) or not isinstance(expires_at, int):\n', '        if not isinstance(subject, str) or not isinstance(expires_at, int):\n')
replace(
    auth_core,
    '        if settings.operator_username is None or not _constant_time_text_equal(\n            username, settings.operator_username\n        ):\n            return None\n',
    '        if not _constant_time_text_equal(subject, OPERATOR_SUBJECT):\n            return None\n',
)
replace(auth_core, '            username=username,\n', '            subject=subject,\n')
sub(
    auth_core,
    r"def authenticate_login\(\n    settings: Settings,\n    runtime: OperatorAuthRuntime,\n    \*,\n    username: str,\n    password: str,\n    totp_code: str,\n\) -> bool:\n.*?\n\ndef authenticate_machine",
    'def authenticate_totp_login(\n'
    '    settings: Settings,\n'
    '    runtime: OperatorAuthRuntime,\n'
    '    *,\n'
    '    totp_code: str,\n'
    ') -> bool:\n'
    '    """Validate TOTP and atomically consume the accepted RFC 6238 time step."""\n'
    '    if not settings.auth_enabled:\n'
    '        return False\n'
    '    assert settings.operator_totp_secret is not None\n\n'
    '    matched_step = _matching_totp_step(settings, totp_code)\n'
    '    if matched_step is None:\n'
    '        return False\n'
    '    step, current_step = matched_step\n'
    '    return runtime.consume_totp_step(step, current_step=current_step)\n\n\n'
    'def authenticate_machine',
)
replace(auth_core, '    assert settings.operator_username is not None\n    return OperatorIdentity(username=settings.operator_username, source="machine")\n', '    return OperatorIdentity(username=OPERATOR_SUBJECT, source="machine")\n')
replace(auth_core, '        return OperatorIdentity(username=session.username, source="session")\n', '        return OperatorIdentity(username=session.subject, source="session")\n')
replace(auth_core, '            username=trusted_browser.username,\n', '            username=trusted_browser.subject,\n')


# API contract is small enough to write explicitly.
write(
    "backend/src/api/auth.py",
    '''"""Single-operator browser authentication endpoints."""\n\nfrom __future__ import annotations\n\nfrom fastapi import APIRouter, Request, Response, status\nfrom pydantic import BaseModel, ConfigDict, Field\n\nfrom errors import QfError\nfrom operator_auth import (\n    OPERATOR_SUBJECT,\n    OperatorAuthRuntime,\n    authenticate_browser,\n    authenticate_totp_login,\n    browser_cookie_epoch,\n    has_valid_trusted_browser,\n    login_source_key,\n    require_same_origin,\n)\nfrom settings import Settings\n\nrouter = APIRouter(prefix="/api/v1/auth", tags=["auth"])\n\n\nclass LoginInput(BaseModel):\n    model_config = ConfigDict(extra="forbid")\n\n    totp_code: str = Field(min_length=6, max_length=6, pattern=r"^[0-9]{6}$")\n    trust_browser: bool = False\n\n\nclass SessionView(BaseModel):\n    model_config = ConfigDict(extra="forbid")\n\n    authenticated: bool\n    username: str\n    trusted_browser: bool\n    auth_enabled: bool\n\n\ndef _prevent_auth_response_caching(response: Response) -> None:\n    response.headers["Cache-Control"] = "no-store"\n    response.headers["Pragma"] = "no-cache"\n\n\ndef _invalid_authentication() -> QfError:\n    return QfError(\n        "AUTH_INVALID",\n        "Operator authentication failed.",\n        401,\n    )\n\n\n@router.post("/login", response_model=SessionView)\ndef login(payload: LoginInput, request: Request, response: Response) -> SessionView:\n    settings: Settings = request.app.state.settings\n    _prevent_auth_response_caching(response)\n    require_same_origin(request, settings)\n    if not settings.auth_enabled:\n        return SessionView(\n            authenticated=True,\n            username=OPERATOR_SUBJECT,\n            trusted_browser=False,\n            auth_enabled=False,\n        )\n\n    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime\n    # Snapshot before TOTP verification. A logout that completes while the\n    # code is being checked must prevent this request from clearing its\n    # barrier or minting a replacement browser session.\n    login_cookie_issuance = runtime.cookie_issuance()\n    login_browser_epoch = browser_cookie_epoch(request, settings)\n    source = login_source_key(request, settings)\n    if not runtime.login_limiter.allow_attempt(source):\n        raise _invalid_authentication()\n    if not authenticate_totp_login(settings, runtime, totp_code=payload.totp_code):\n        runtime.login_limiter.record_failure(source)\n        raise _invalid_authentication()\n    if not runtime.complete_login_if_current(\n        response,\n        settings,\n        cookie_issuance=login_cookie_issuance,\n        browser_epoch=login_browser_epoch,\n        trust_browser=payload.trust_browser,\n    ):\n        raise _invalid_authentication()\n    runtime.login_limiter.record_success(source)\n    return SessionView(\n        authenticated=True,\n        username=OPERATOR_SUBJECT,\n        trusted_browser=payload.trust_browser,\n        auth_enabled=True,\n    )\n\n\n@router.get("/session", response_model=SessionView)\ndef session(request: Request, response: Response) -> SessionView:\n    settings: Settings = request.app.state.settings\n    _prevent_auth_response_caching(response)\n    if not settings.auth_enabled:\n        return SessionView(\n            authenticated=True,\n            username=OPERATOR_SUBJECT,\n            trusted_browser=False,\n            auth_enabled=False,\n        )\n    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime\n    renewal_cookie_issuance = runtime.cookie_issuance()\n    renewal_browser_epoch = browser_cookie_epoch(request, settings)\n    identity = authenticate_browser(request, settings)\n    if identity is None:\n        raise QfError(\n            "AUTH_REQUIRED",\n            "Operator authentication is required.",\n            401,\n        )\n    if identity.renew_session:\n        if not runtime.renew_session_if_current(\n            response,\n            settings,\n            cookie_issuance=renewal_cookie_issuance,\n            browser_epoch=renewal_browser_epoch,\n        ):\n            raise QfError(\n                "AUTH_REQUIRED",\n                "Operator authentication is required.",\n                401,\n            )\n    return SessionView(\n        authenticated=True,\n        username=identity.username,\n        trusted_browser=has_valid_trusted_browser(request, settings),\n        auth_enabled=True,\n    )\n\n\n@router.post("/logout", status_code=status.HTTP_204_NO_CONTENT)\ndef logout(request: Request, response: Response) -> None:\n    settings: Settings = request.app.state.settings\n    _prevent_auth_response_caching(response)\n    require_same_origin(request, settings)\n    browser_identity = authenticate_browser(request, settings)\n    runtime: OperatorAuthRuntime = request.app.state.operator_auth_runtime\n    runtime.complete_logout(\n        response,\n        settings,\n        revoke_streams=browser_identity is not None,\n    )\n''',
)

replace("backend/src/errors.py", "Login failures must not echo password or TOTP material or reveal which\n            # authentication factor failed validation.", "Login failures must not echo submitted TOTP or legacy authentication material.\n            # Every schema/format failure intentionally has the same public shape.")
replace("backend/src/errors.py", 'message="Invalid operator credentials.",', 'message="Operator authentication failed.",')


# ---------------------------------------------------------------------------
# Frontend login surface and localization.
# ---------------------------------------------------------------------------
front = "frontend/src/auth/AuthGate.tsx"
replace(front, "  const [username, setUsername] = useState('');\n", "")
replace(front, "  const [password, setPassword] = useState('');\n", "")
replace(front, "          username,\n          password,\n", "")
sub(front, r"            <label>\n              <span>\{t\('auth\.username'\)\}</span>\n.*?            </label>\n", "")
sub(front, r"            <label>\n              <span>\{t\('auth\.password'\)\}</span>\n.*?            </label>\n", "")
replace(front, '              <input\n                autoComplete="one-time-code"', '              <input\n                autoComplete="one-time-code"\n                autoFocus')

messages = "frontend/src/i18n/messages.ts"
sub(messages, r"  'auth\.loginDescription': m\(.*?\),\n", "  'auth.loginDescription': m('Enter the {digits}-digit code from your Google Authenticator-compatible app.', '请输入 Google Authenticator 兼容验证器中的 {digits} 位动态码。', '請輸入 Google Authenticator 相容驗證器中的 {digits} 位動態碼。', 'Google Authenticator 互換アプリの {digits} 桁コードを入力してください。', 'Google Authenticator 호환 앱의 {digits}자리 코드를 입력하세요.', 'Introduzca el código de {digits} dígitos de su aplicación compatible con Google Authenticator.', 'أدخل الرمز المكوّن من {digits} أرقام من تطبيق متوافق مع Google Authenticator.'),\n")
sub(messages, r"  'auth\.username': m\(.*?\),\n", "")
sub(messages, r"  'auth\.password': m\(.*?\),\n", "")
sub(messages, r"  'auth\.trustBrowserDescription': m\(.*?\),\n", "  'auth.trustBrowserDescription': m('Future visits can sign in without another authenticator code until this device trust expires.', '在此设备信任到期前，后续访问无需再次输入验证器动态码。', '在此裝置信任到期前，後續造訪無需再次輸入驗證器動態碼。', 'このデバイスの信頼が失効するまで、次回以降は認証コードを再入力せずにサインインできます。', '이 기기의 신뢰가 만료될 때까지 이후 방문에서는 인증 앱 코드를 다시 입력하지 않고 로그인할 수 있습니다.', 'En futuras visitas podrá iniciar sesión sin volver a introducir el código del autenticador hasta que caduque la confianza de este dispositivo.', 'يمكن تسجيل الدخول في الزيارات اللاحقة دون إعادة إدخال رمز المصادقة حتى تنتهي صلاحية الثقة بهذا الجهاز.'),\n")


# ---------------------------------------------------------------------------
# Deployment surfaces.
# ---------------------------------------------------------------------------
for path in (".env.example", "compose.yml"):
    text = read(path)
    text = re.sub(r"^.*QUAZONAI_AUTH_USERNAME.*\n", "", text, flags=re.MULTILINE)
    text = re.sub(r"^.*QUAZONAI_AUTH_PASSWORD.*\n", "", text, flags=re.MULTILINE)
    write(path, text)
replace(
    ".env.example",
    "# When enabled, every credential/origin value below is required. TOTP is\n",
    "# When enabled, the TOTP/cookie-key/API-token/public-origin values below are required. TOTP is\n",
)

write(
    "frontend/e2e/operator-auth.spec.ts",
    '''import { execFileSync } from 'node:child_process';\nimport { expect, test } from '@playwright/test';\n\nconst authEnabled = process.env.QUAZONAI_E2E_AUTH_ENABLED === 'true';\nconst totpSecret = process.env.QUAZONAI_E2E_AUTH_TOTP_SECRET ?? '';\n\nfunction currentTotpCode(): string {\n  if (!totpSecret) throw new Error('QUAZONAI_E2E_AUTH_TOTP_SECRET is required');\n  return execFileSync(\n    'python',\n    [\n      '-c',\n      'import os, pyotp; print(pyotp.TOTP(os.environ["QUAZONAI_E2E_AUTH_TOTP_SECRET"]).now())',\n    ],\n    {\n      encoding: 'utf8',\n      env: { ...process.env, QUAZONAI_E2E_AUTH_TOTP_SECRET: totpSecret },\n    },\n  ).trim();\n}\n\ntest.describe('single-operator authentication', () => {\n  test.skip(!authEnabled, 'Runs only in the dedicated auth-enabled browser workflow.');\n  test.describe.configure({ retries: 0 });\n\n  test('TOTP-only login, trusted-browser restore, and logout revocation', async ({\n    page,\n    context,\n  }) => {\n    await page.goto('/');\n\n    await expect(page.getByLabel('Username', { exact: true })).toHaveCount(0);\n    await expect(page.getByLabel('Password', { exact: true })).toHaveCount(0);\n    const totp = page.getByLabel('Authenticator code', { exact: true });\n    await expect(totp).toBeFocused();\n    await totp.fill(currentTotpCode());\n    await page.getByRole('checkbox', { name: /^Trust this browser/ }).check();\n    await page.getByRole('button', { name: 'Sign in', exact: true }).click();\n\n    await expect(page.getByText('Dashboard', { exact: true }).first()).toBeVisible();\n\n    const authenticatedCookies = await context.cookies();\n    const session = authenticatedCookies.find((cookie) => cookie.name === 'quazonai_session');\n    const trusted = authenticatedCookies.find((cookie) => cookie.name === 'quazonai_trusted_browser');\n    expect(session).toBeDefined();\n    expect(trusted).toBeDefined();\n    expect(session?.httpOnly).toBe(true);\n    expect(trusted?.httpOnly).toBe(true);\n    expect(session?.sameSite).toBe('Strict');\n    expect(trusted?.sameSite).toBe('Strict');\n\n    await context.clearCookies({ name: 'quazonai_session' });\n    await page.reload();\n    await expect(page.getByText('Dashboard', { exact: true }).first()).toBeVisible();\n    expect((await context.cookies()).some((cookie) => cookie.name === 'quazonai_session')).toBe(true);\n\n    await page.getByRole('button', { name: /sign out|log out/i }).click();\n    await expect(page.getByLabel('Authenticator code', { exact: true })).toBeVisible();\n    await expect(page.getByLabel('Username', { exact: true })).toHaveCount(0);\n    await expect(page.getByLabel('Password', { exact: true })).toHaveCount(0);\n\n    const loggedOutCookies = await context.cookies();\n    expect(loggedOutCookies.some((cookie) => cookie.name === 'quazonai_session')).toBe(false);\n    expect(loggedOutCookies.some((cookie) => cookie.name === 'quazonai_trusted_browser')).toBe(false);\n  });\n});\n''',
)


# ---------------------------------------------------------------------------
# Backend tests: remove deleted Settings fields, migrate login helpers.
# ---------------------------------------------------------------------------
for path in (ROOT / "backend/tests").rglob("*.py"):
    remove_keyword_lines(path, ("operator_username", "operator_password"))

# Settings tests no longer validate deleted username/password values.
sub(
    "backend/tests/unit/test_settings.py",
    r"\n\ndef test_enabled_auth_rejects_password_longer_than_login_schema\(.*?\n\ndef test_enabled_auth_rejects_invalid_origin_host_or_port",
    "\n\ndef test_enabled_auth_rejects_invalid_origin_host_or_port",
)

# Configuration hardening: remove obsolete browser credential tests and prove legacy env fail-closed.
sub(
    "backend/tests/unit/test_operator_auth_configuration_hardening.py",
    r"\n\n@pytest\.mark\.parametrize\(\n    \(\"field\", \"value\"\),\n    \[\n        \(\"operator_username\".*?\n\ndef test_enabled_auth_accepts_header_safe_bearer_tokens",
    '''\n\n@pytest.mark.parametrize("legacy_name", ["QUAZONAI_AUTH_USERNAME", "QUAZONAI_AUTH_PASSWORD"])\ndef test_enabled_auth_rejects_nonempty_legacy_browser_credentials(\n    monkeypatch: pytest.MonkeyPatch,\n    legacy_name: str,\n) -> None:\n    legacy_value = "must-not-appear-in-error"\n    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "true")\n    monkeypatch.setenv(legacy_name, legacy_value)\n\n    with pytest.raises(SettingsError) as raised:\n        Settings.from_env()\n\n    assert legacy_name in str(raised.value)\n    assert legacy_value not in str(raised.value)\n\n\ndef test_disabled_auth_ignores_legacy_browser_credentials(monkeypatch: pytest.MonkeyPatch) -> None:\n    monkeypatch.setenv("QUAZONAI_AUTH_ENABLED", "false")\n    monkeypatch.setenv("QUAZONAI_AUTH_USERNAME", "legacy-user")\n    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", "legacy-password")\n\n    configured = Settings.from_env()\n\n    assert configured.auth_enabled is False\n    assert not hasattr(configured, "operator_username")\n    assert not hasattr(configured, "operator_password")\n\n\ndef test_enabled_auth_accepts_header_safe_bearer_tokens''',
)
replace("backend/tests/unit/test_operator_auth_configuration_hardening.py", '    monkeypatch.setenv("QUAZONAI_AUTH_USERNAME", "operator")\n', "", required=False)
replace("backend/tests/unit/test_operator_auth_configuration_hardening.py", '    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", "correct horse battery staple")\n', "", required=False)

# Review regression helper already lost deleted Settings keyword lines.

# Secret isolation keeps explicit clearing of both legacy names without storing their values in Settings.
write(
    "backend/tests/unit/test_operator_auth_secret_isolation.py",
    '''from __future__ import annotations\n\nimport base64\nfrom dataclasses import replace\nfrom pathlib import Path\n\nimport pytest\n\nfrom runners.research_missions import _codex_launch_configuration\nfrom settings import Settings\n\n\ndef test_codex_child_environment_scrubs_operator_auth_configuration(\n    settings: Settings,\n    tmp_path: Path,\n    monkeypatch: pytest.MonkeyPatch,\n) -> None:\n    legacy_username = "operator-secret-name"\n    legacy_password = "operator-secret-password"\n    monkeypatch.setenv("QUAZONAI_AUTH_USERNAME", legacy_username)\n    monkeypatch.setenv("QUAZONAI_AUTH_PASSWORD", legacy_password)\n    configured = replace(\n        settings,\n        operator_auth_enabled=True,\n        operator_totp_secret="JBSWY3DPEHPK3PXPJBSWY3DPEHPK3PXP",\n        auth_cookie_key=base64.b64encode(b"c" * 32).decode("ascii"),\n        api_token="operator-machine-token-" + "x" * 32,\n        auth_public_origin="https://quazonai.example.test",\n    )\n\n    config, provider_id = _codex_launch_configuration(configured, tmp_path)\n\n    assert provider_id is None\n    assert config.env is not None\n    scrubbed_names = (\n        "QUAZONAI_AUTH_ENABLED",\n        "QUAZONAI_AUTH_USERNAME",\n        "QUAZONAI_AUTH_PASSWORD",\n        "QUAZONAI_AUTH_TOTP_SECRET",\n        "QUAZONAI_AUTH_COOKIE_KEY",\n        "QUAZONAI_API_TOKEN",\n        "QUAZONAI_AUTH_PUBLIC_ORIGIN",\n        "QUAZONAI_AUTH_SESSION_TTL_SECONDS",\n        "QUAZONAI_AUTH_TRUSTED_BROWSER_TTL_DAYS",\n        "QUAZONAI_AUTH_TRUSTED_PROXY_CIDRS",\n    )\n    for name in scrubbed_names:\n        assert config.env[name] == ""\n\n    serialized = repr(config.env) + repr(config.config_overrides)\n    assert legacy_username not in serialized\n    assert legacy_password not in serialized\n    assert configured.operator_totp_secret not in serialized\n    assert configured.auth_cookie_key not in serialized\n    assert configured.api_token not in serialized\n    assert configured.auth_public_origin not in serialized\n''',
)

# Main integration helpers and login-specific regression cases.
main_test = "backend/tests/integration/test_operator_auth.py"
replace(
    main_test,
    '        json={\n            "username": "operator",\n            "password": "correct horse battery staple",\n            "totp_code": code,\n            "trust_browser": trust_browser,\n        },\n',
    '        json={\n            "totp_code": code,\n            "trust_browser": trust_browser,\n        },\n',
)
replace(main_test, "def test_password_totp_login_sets_strict_http_only_cookies", "def test_totp_login_sets_strict_http_only_cookies")
replace(main_test, '        "username": "operator",\n', '        "username": "local-operator",\n', required=False)
replace(main_test, "Invalid operator credentials.", "Operator authentication failed.", required=False)
sub(
    main_test,
    r"\ndef test_invalid_login_does_not_reveal_failed_factor\(.*?\n\ndef test_unencodable_login_text_returns_generic_auth_failure",
    '''\ndef test_invalid_login_does_not_reveal_failed_totp(settings: Settings, engine: Engine) -> None:\n    secured = _enabled_settings(settings)\n    assert secured.operator_totp_secret is not None\n    client = TestClient(create_app(settings=secured, engine=engine))\n    current = pyotp.TOTP(secured.operator_totp_secret).now()\n    wrong = "000000" if current != "000000" else "000001"\n\n    response = _login(client, secured, totp_code=wrong)\n\n    assert response.status_code == 401\n    assert response.json() == {\n        "error": {\n            "code": "AUTH_INVALID",\n            "message": "Operator authentication failed.",\n            "details": {},\n        }\n    }\n    assert wrong not in response.text\n    assert response.headers["Cache-Control"] == "no-store"\n\n\ndef test_unencodable_login_text_returns_generic_auth_failure''',
)
sub(
    main_test,
    r"\ndef test_unencodable_login_text_returns_generic_auth_failure\(.*?\n\ndef test_invalid_login_shape_does_not_echo_submitted_secrets",
    '''\ndef test_non_ascii_totp_returns_generic_auth_failure(\n    settings: Settings,\n    engine: Engine,\n) -> None:\n    secured = _enabled_settings(settings)\n    client = TestClient(create_app(settings=secured, engine=engine))\n\n    response = client.post(\n        "/api/v1/auth/login",\n        headers={"Origin": "http://testserver"},\n        json={"totp_code": "１２３４５６", "trust_browser": False},\n    )\n\n    assert response.status_code == 401\n    assert response.json()["error"]["code"] == "AUTH_INVALID"\n    assert "１２３４５６" not in response.text\n\n\ndef test_invalid_login_shape_does_not_echo_submitted_secrets''',
)
sub(
    main_test,
    r"\ndef test_invalid_login_shape_does_not_echo_submitted_secrets\(.*?\n\ndef test_browser_mutation_requires_configured_origin",
    '''\ndef test_legacy_login_fields_are_forbidden_and_not_echoed(\n    settings: Settings,\n    engine: Engine,\n) -> None:\n    secured = _enabled_settings(settings)\n    assert secured.operator_totp_secret is not None\n    client = TestClient(create_app(settings=secured, engine=engine))\n    legacy_username = "do-not-echo-legacy-user"\n    legacy_password = "do-not-echo-legacy-password"\n\n    response = client.post(\n        "/api/v1/auth/login",\n        headers={"Origin": "http://testserver"},\n        json={\n            "username": legacy_username,\n            "password": legacy_password,\n            "totp_code": pyotp.TOTP(secured.operator_totp_secret).now(),\n            "trust_browser": False,\n        },\n    )\n\n    assert response.status_code == 401\n    assert response.json()["error"]["code"] == "AUTH_INVALID"\n    assert legacy_username not in response.text\n    assert legacy_password not in response.text\n    assert response.headers["Cache-Control"] == "no-store"\n\n\ndef test_browser_mutation_requires_configured_origin''',
)
replace(main_test, "test_trusted_browser_restores_session_without_password_or_totp", "test_trusted_browser_restores_session_without_another_totp", required=False)

# Credential-confusion and origin helpers have simple login payloads.
for path in (
    "backend/tests/integration/test_operator_auth_credential_confusion.py",
    "backend/tests/integration/test_operator_origin_canonicalization.py",
):
    text = read(path)
    text = text.replace('            "username": "operator",\n', "")
    text = text.replace('            "password": "correct horse battery staple",\n', "")
    write(path, text)

# Replace obsolete Unicode username/password test with wire-contract coverage.
write(
    "backend/tests/integration/test_operator_auth_unicode.py",
    '''from __future__ import annotations\n\nimport base64\nfrom dataclasses import replace\n\nimport pyotp\nimport pytest\nfrom fastapi.testclient import TestClient\nfrom sqlalchemy import Engine\n\nfrom main import create_app\nfrom settings import Settings\n\n\ndef _enabled_settings(settings: Settings) -> Settings:\n    return replace(\n        settings,\n        operator_auth_enabled=True,\n        operator_totp_secret=pyotp.random_base32(),\n        auth_cookie_key=base64.b64encode(b"a" * 32).decode("ascii"),\n        api_token="machine-token-" + "x" * 32,\n        auth_public_origin="http://testserver",\n    )\n\n\n@pytest.mark.parametrize("totp_code", ["１２３４５６", "١٢٣٤٥٦", "12345", "1234567", "12 3456"])\ndef test_login_rejects_non_ascii_or_non_six_digit_totp(\n    settings: Settings,\n    engine: Engine,\n    totp_code: str,\n) -> None:\n    secured = _enabled_settings(settings)\n    client = TestClient(create_app(settings=secured, engine=engine))\n\n    response = client.post(\n        "/api/v1/auth/login",\n        headers={"Origin": "http://testserver"},\n        json={"totp_code": totp_code, "trust_browser": False},\n    )\n\n    assert response.status_code == 401\n    assert response.json()["error"]["code"] == "AUTH_INVALID"\n    assert totp_code not in response.text\n''',
)

# Hardening helper: TOTP-only payload and renamed monkeypatch hook.
hardening = "backend/tests/integration/test_operator_auth_hardening.py"
replace(
    hardening,
    "    *,\n    password: str,\n    totp_code: str | None = None,\n",
    "    *,\n    totp_code: str | None = None,\n",
)
replace(hardening, '        "username": "operator",\n        "password": password,\n', "")
replace(
    hardening,
    '    return {\n        "totp_code": totp_code or pyotp.TOTP(settings.operator_totp_secret).now(),\n        "trust_browser": trust_browser,\n    }\n',
    '    return {\n        "totp_code": totp_code or pyotp.TOTP(settings.operator_totp_secret).now(),\n        "trust_browser": trust_browser,\n    }\n\n\ndef _wrong_totp(settings: Settings) -> str:\n    assert settings.operator_totp_secret is not None\n    current = pyotp.TOTP(settings.operator_totp_secret).now()\n    return "000000" if current != "000000" else "000001"\n',
)
replace(hardening, 'password="wrong-password-value"', 'totp_code=_wrong_totp(secured)', required=False)
text = read(hardening)
text = re.sub(r"\n\s*password=\"correct horse battery staple\",", "", text)
text = text.replace("authenticate_login", "authenticate_totp_login")
text = re.sub(r"\n\s*username: str,", "", text)
text = re.sub(r"\n\s*password: str,", "", text)
text = re.sub(r"\n\s*username=username,", "", text)
text = re.sub(r"\n\s*password=password,", "", text)
text = text.replace("maximum_backoff_seconds=5.0", "maximum_backoff_seconds=30.0")
text = text.replace("now[0] += 5.01", "now[0] += 30.01")
text = text.replace("now[0] += 4.99", "now[0] += 29.99")
write(hardening, text)

# Fixed subject should be reflected by every auth-oriented frontend fixture.
front_test = "frontend/src/auth/AuthGate.test.tsx"
text = read(front_test).replace("username: 'operator'", "username: 'local-operator'")
text = re.sub(r"\n\s*await user\.type\(await screen\.findByLabelText\('Username'\), 'operator'\);", "", text)
text = re.sub(r"\n\s*await user\.type\(screen\.getByLabelText\('Password'\), .*?\);", "", text)
text = re.sub(r"\n\s*await user\.type\(await screen\.findByLabelText\('اسم المستخدم'\), 'operator'\);", "", text)
text = re.sub(r"\n\s*await user\.type\(screen\.getByLabelText\('كلمة المرور'\), .*?\);", "", text)
text = text.replace("shows password, authenticator code, and trusted-browser option when anonymous", "shows only authenticator code and trusted-browser option when anonymous")
text = text.replace("    expect(screen.getByLabelText('Username')).toBeInTheDocument();\n    expect(screen.getByLabelText('Password')).toBeInTheDocument();\n    expect(screen.getByLabelText('Authenticator code')).toBeInTheDocument();", "    expect(screen.queryByLabelText('Username')).not.toBeInTheDocument();\n    expect(screen.queryByLabelText('Password')).not.toBeInTheDocument();\n    expect(screen.getByLabelText('Authenticator code')).toHaveFocus();")
text = text.replace("    expect(screen.getByLabelText('اسم المستخدم')).toHaveAttribute('dir', 'auto');\n    expect(screen.getByLabelText('كلمة المرور')).toHaveAttribute('dir', 'ltr');\n", "")
text = text.replace("      username: 'operator',\n      password: 'correct horse battery staple',\n", "")
write(front_test, text)

# Skill contract now describes the browser TOTP secret rather than a removed password.
replace("backend/tests/unit/test_agent_skill_auth_contract.py", '    assert "Operator password" in combined\n', '    assert "TOTP setup secret" in combined\n', required=False)


# ---------------------------------------------------------------------------
# Documentation: remove active username/password credentials and describe TOTP-only risk.
# ---------------------------------------------------------------------------
doc_paths = (
    "DESIGN.md",
    "OPERATIONS.md",
    "CLI.md",
    "README.md",
    "skills/quazonai/SKILL.md",
    "skills/quazonai/references/authentication.md",
    "skills/quazonai/references/cli-reference.md",
    "skills/quazonai/references/workflows.md",
)
for path in doc_paths:
    text = read(path)
    text = re.sub(r"^.*QUAZONAI_AUTH_USERNAME.*\n", "", text, flags=re.MULTILINE)
    text = re.sub(r"^.*QUAZONAI_AUTH_PASSWORD.*\n", "", text, flags=re.MULTILINE)
    replacements = {
        "password + Google Authenticator-compatible TOTP": "Google Authenticator-compatible TOTP",
        "password + TOTP": "TOTP",
        "Password + TOTP": "TOTP",
        "password/TOTP": "TOTP",
        "password or authenticator code": "authenticator code",
        "password and authenticator code": "authenticator code",
        "password and TOTP": "TOTP",
        "Operator password/TOTP/cookie key/machine token/public origin": "Operator TOTP setup secret/cookie key/machine token/public origin",
        "Operator password, TOTP setup secret": "Operator TOTP setup secret",
        "password/TOTP 恢复": "TOTP 恢复",
        "password + 动态码": "动态码",
        "密码 + 动态码": "动态码",
        "密码和兼容 Google Authenticator": "兼容 Google Authenticator",
        "双因素认证": "TOTP Operator Authentication",
        "multi-factor": "TOTP-only",
    }
    for old, new in replacements.items():
        text = text.replace(old, new)
    write(path, text)

# Formal design facts for Issue #37.
design = read("DESIGN.md")
design = design.replace(
    "Operator Authentication 是部署/访问边界，不是新的业务用户、tenant 或 RBAC Domain。V1 只有一个由启动环境配置的 Operator。",
    "Operator Authentication 是部署/访问边界，不是新的业务用户、tenant 或 RBAC Domain。V1 只有一个固定 Operator，身份 subject 为 `local-operator`，不得由客户端或环境变量覆盖。浏览器登录是 TOTP-only 单因素认证；Machine API Token 是独立的自动化凭据，不是浏览器登录因子。",
)
design = design.replace(
    "连续失败指数退避但最大 5 秒",
    "连续失败指数退避但最大 30 秒",
)
design = design.replace(
    "认证失败返回统一错误 envelope，不区分“用户名不存在/密码错误/TOTP 错误”等可用于枚举的细节；",
    "认证失败返回统一错误 envelope；缺失、格式错误、错误、重放或被限流的 TOTP 均返回 `401 / AUTH_INVALID`，不回显请求体或动态码；",
)
anchor = "### 37.1 Operator Authentication\n"
warning = "\n> 安全语义：TOTP-only 不再是 2FA/MFA，抗在线暴力破解能力弱于密码 + TOTP。公网暴露时必须继续使用 HTTPS、窄化可信代理配置，并优先叠加部署侧网络访问控制。认证因子迁移会升级 Cookie version，因此旧 session/trusted-browser cookie fail closed，升级后需要重新输入一次 TOTP；无需数据库迁移，也无需在 TOTP secret 不变时重新绑定验证器。\n"
if warning.strip() not in design:
    design = design.replace(anchor, anchor + warning, 1)
write("DESIGN.md", design)

# Operations: make the user-facing login and upgrade path explicit without restoring legacy credentials.
ops = read("OPERATIONS.md")
ops = re.sub(
    r"认证启用后的 Web 登录输入：\n\n```text\n.*?```",
    "认证启用后的 Web 登录输入只有 Google Authenticator-compatible 6-digit TOTP。登录页不再展示或提交用户名/密码；`Trust this browser` 行为保持不变。",
    ops,
    count=1,
    flags=re.DOTALL,
)
ops = ops.replace("用户不再输入 TOTP。", "用户不再重复输入 TOTP。")
ops = ops.replace("所有浏览器必须重新执行 TOTP 登录。", "所有浏览器必须重新执行 TOTP 登录。")
upgrade = "\n升级到 TOTP-only 认证时：保留现有 TOTP setup key、cookie key、machine API token、public origin 与 TTL 配置；从 `.env`/部署 Secrets 中删除旧浏览器用户名和密码变量后重启 API。旧 session/trusted-browser cookie 会失效，使用当前 TOTP 重新登录并按需重新勾选 `Trust this browser`。无需数据库迁移；TOTP setup key 不变时无需重新绑定 Google Authenticator。\n"
if "升级到 TOTP-only 认证时" not in ops:
    marker = "### 14.2 Operator Authentication\n"
    ops = ops.replace(marker, marker + upgrade, 1)
write("OPERATIONS.md", ops)

# Research Mission children still clear legacy names deliberately; document that intent in code.
replace(
    "backend/src/runners/research_missions.py",
    '        "QUAZONAI_AUTH_USERNAME": "",\n        "QUAZONAI_AUTH_PASSWORD": "",\n',
    '        # Deprecated browser credentials are still scrubbed so stale host secrets\n        # can never leak into Mission-owned child processes.\n        "QUAZONAI_AUTH_USERNAME": "",\n        "QUAZONAI_AUTH_PASSWORD": "",\n',
)

# ---------------------------------------------------------------------------
# Static migration checks. Exact legacy env names are restricted to detection,
# child-process scrubbing and the tests that prove those two boundaries.
# ---------------------------------------------------------------------------
for path in (ROOT / "backend/src").rglob("*.py"):
    text = path.read_text(encoding="utf-8")
    if path.name != "settings.py" and path.name != "research_missions.py":
        if "QUAZONAI_AUTH_USERNAME" in text or "QUAZONAI_AUTH_PASSWORD" in text:
            raise RuntimeError(f"legacy auth env leaked into active source: {path}")
    if "operator_username" in text or "operator_password" in text:
        raise RuntimeError(f"deleted Settings field still referenced: {path}")

for path in (ROOT / "frontend").rglob("*"):
    if not path.is_file() or path.suffix not in {".ts", ".tsx", ".yml", ".yaml"}:
        continue
    text = path.read_text(encoding="utf-8")
    if "QUAZONAI_E2E_AUTH_USERNAME" in text or "QUAZONAI_E2E_AUTH_PASSWORD" in text:
        raise RuntimeError(f"legacy E2E credential remains: {path}")

print("Issue #37 migration applied successfully")
