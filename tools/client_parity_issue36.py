#!/usr/bin/env python3
"""Enforce the Web/iPhone/iPad operator capability contract for Issue #36."""

from __future__ import annotations

import json
import os
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
CANONICAL_REGISTRY = ROOT / "contracts" / "client-capabilities.yaml"
FALLBACK_REGISTRY = ROOT / "contracts" / "client-capabilities.issue36.yaml"
REQUIRED_DOMAINS = {
    "home",
    "idea",
    "research",
    "alpha",
    "portfolio",
    "approval",
    "handoff",
    "administration",
    "sync",
    "authentication",
}
REQUIRED_LOCALES = ("en", "zh-Hans", "zh-Hant", "ja", "ko", "es", "ar")
FORBIDDEN_NATIVE_SOURCE = (
    "WKWebView",
    "QUAZONAI_API_TOKEN",
    "allowsAnyHTTPSCertificate",
    "serverTrust",
    "trustAllCertificate",
)


def fail(message: str) -> None:
    raise SystemExit(f"client-parity: {message}")


def load_json_object(path: Path) -> dict[str, Any]:
    try:
        payload = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot read {path.relative_to(ROOT)}: {exc}")
    if not isinstance(payload, dict):
        fail(f"{path.relative_to(ROOT)} root must be an object")
    return payload


def load_registry() -> dict[str, Any]:
    if CANONICAL_REGISTRY.is_file():
        payload = load_json_object(CANONICAL_REGISTRY)
        include = payload.get("include")
        if include is not None:
            if not isinstance(include, str) or Path(include).name != include:
                fail("canonical registry include must name one sibling file")
            return load_json_object(CANONICAL_REGISTRY.parent / include)
        return payload
    return load_json_object(FALLBACK_REGISTRY)


def live_openapi() -> dict[str, Any]:
    os.environ.setdefault("QUAZONAI_ENV", "test")
    os.environ.setdefault("QUAZONAI_DATABASE_URL", "sqlite+pysqlite:///:memory:")
    os.environ.setdefault("QUAZONAI_ALEMBIC_URL", "sqlite+pysqlite:///:memory:")
    sys.path.insert(0, str(ROOT / "backend" / "src"))
    from main import create_app

    document = create_app().openapi()
    if not isinstance(document, dict):
        fail("FastAPI did not produce an OpenAPI document")
    return document


def resolve_schema(schema: dict[str, Any], document: dict[str, Any]) -> dict[str, Any]:
    reference = schema.get("$ref")
    if not isinstance(reference, str):
        return schema
    prefix = "#/components/schemas/"
    if not reference.startswith(prefix):
        fail(f"unsupported OpenAPI reference: {reference}")
    name = reference.removeprefix(prefix)
    resolved = document.get("components", {}).get("schemas", {}).get(name)
    if not isinstance(resolved, dict):
        fail(f"missing OpenAPI schema: {name}")
    return resolved


def check_mobile_login_schema(document: dict[str, Any]) -> None:
    operation = document.get("paths", {}).get("/api/v1/auth/mobile/login", {}).get("post")
    if not isinstance(operation, dict):
        fail("mobile login operation is absent from live OpenAPI")
    body_schema = (
        operation.get("requestBody", {})
        .get("content", {})
        .get("application/json", {})
        .get("schema")
    )
    if not isinstance(body_schema, dict):
        fail("mobile login has no JSON request schema")
    resolved = resolve_schema(body_schema, document)
    properties = resolved.get("properties", {})
    if not isinstance(properties, dict):
        fail("mobile login properties are malformed")
    required = set(resolved.get("required", []))
    if "totp_code" not in properties or "totp_code" not in required:
        fail("mobile login must require totp_code")
    forbidden = {"username", "password"} & set(properties)
    if forbidden:
        fail(f"mobile login contains forbidden fields: {sorted(forbidden)}")
    if resolved.get("additionalProperties") is not False:
        fail("mobile login must reject unknown fields")


def parse_api_operation(value: object) -> tuple[str, str]:
    if not isinstance(value, str):
        fail("api_operation must be a string")
    method, separator, path = value.partition(" ")
    method = method.lower()
    if not separator or method not in {"get", "post", "put", "patch", "delete"}:
        fail(f"invalid api_operation: {value!r}")
    if not path.startswith("/api/v1/"):
        fail(f"operator capability is outside /api/v1: {value!r}")
    return method, path


def check_test_reference(value: object, *, field: str) -> None:
    if not isinstance(value, str):
        fail(f"{field} must be a file#symbol reference")
    path_text, separator, symbol = value.partition("#")
    path = ROOT / path_text
    if not separator or not symbol:
        fail(f"{field} must include #symbol: {value}")
    if not path.is_file():
        fail(f"{field} file does not exist: {path_text}")
    if symbol not in path.read_text(encoding="utf-8"):
        fail(f"{field} symbol {symbol!r} missing from {path_text}")


def check_web_routes(capabilities: list[dict[str, Any]]) -> None:
    app_source = (ROOT / "frontend" / "src" / "app" / "App.tsx").read_text(encoding="utf-8")
    routes = {"/"}
    aliases = {
        "/alphas": "/alpha",
        "/alphas/:id": "/alpha/:id",
        "/approvals": "/approval",
        "/handoffs": "/handoff",
    }
    for match in re.finditer(r'<Route\s+path="([^"]+)"', app_source):
        route = "/" + match.group(1).lstrip("/")
        if route == "/*":
            continue
        routes.add(aliases.get(route, route))
    covered = {str(item.get("web_route")) for item in capabilities}
    missing = sorted(routes - covered)
    if missing:
        fail(f"Web routes lack native capability coverage: {missing}")


def localization_keys(path: Path) -> set[str]:
    text = path.read_text(encoding="utf-8")
    return set(re.findall(r'^\s*"([^"]+)"\s*=', text, flags=re.MULTILINE))


def check_localizations() -> None:
    resources = ROOT / "ios" / "Issue36Resources"
    english = resources / "en.lproj" / "Localizable.strings"
    if not english.is_file():
        fail("missing English iOS localization")
    canonical_keys = localization_keys(english)
    for locale in REQUIRED_LOCALES:
        path = resources / f"{locale}.lproj" / "Localizable.strings"
        if not path.is_file():
            fail(f"missing iOS localization: {locale}")
        keys = localization_keys(path)
        if keys != canonical_keys:
            missing = sorted(canonical_keys - keys)
            extra = sorted(keys - canonical_keys)
            fail(f"{locale} localization key drift; missing={missing}, extra={extra}")
    if not (resources / "PrivacyInfo.xcprivacy").is_file():
        fail("missing iOS PrivacyInfo.xcprivacy")
    if not (resources / "Assets.xcassets" / "AppIcon.appiconset" / "Contents.json").is_file():
        fail("missing iOS app icon catalog")


def check_native_security() -> None:
    selected = [
        ROOT / "ios" / "Issue36App" / "QuaZonaiApp.swift",
        ROOT / "ios" / "Issue36Final" / "Core.swift",
        ROOT / "ios" / "Issue36Final" / "SharedViews.swift",
        ROOT / "ios" / "Issue36Final" / "RootView.swift",
        ROOT / "ios" / "Issue36App" / "ResearchViews.swift",
        ROOT / "ios" / "Issue36App" / "MarketViews.swift",
        ROOT / "ios" / "Issue36Final" / "AdministrationViews.swift",
    ]
    missing = [str(path.relative_to(ROOT)) for path in selected if not path.is_file()]
    if missing:
        fail(f"selected Swift sources are missing: {missing}")
    joined = "\n".join(path.read_text(encoding="utf-8") for path in selected)
    for marker in FORBIDDEN_NATIVE_SOURCE:
        if marker in joined:
            fail(f"forbidden native source marker: {marker}")
    required_markers = {
        "NavigationSplitView": "iPad adaptive shell",
        "TabView": "iPhone shell",
        "LocalAuthentication": "biometric protection",
        "SwiftData": "offline cache",
        "text/event-stream": "SSE client",
        "Idempotency-Key": "mutation idempotency",
        "SecureField": "secret input protection",
        "rightToLeft": "Arabic RTL",
        "if method.isMutation && !network.isOnline": "offline mutation prohibition",
    }
    for marker, capability in required_markers.items():
        if marker not in joined:
            fail(f"missing {capability} marker ({marker})")

    login_match = re.search(
        r'let body: JSONValue = \.object\(\[(.*?)\]\)\s*\n\s*let response',
        joined,
        flags=re.DOTALL,
    )
    if login_match is None:
        fail("native TOTP login request could not be located")
    login_body = login_match.group(1)
    if '"totp_code"' not in login_body:
        fail("native login request does not contain totp_code")
    if re.search(r'"(?:username|password)"\s*:', login_body, flags=re.IGNORECASE):
        fail("native login request contains username/password")

    keychain_source = (ROOT / "ios" / "Issue36Final" / "Core.swift").read_text(encoding="utf-8")
    service_match = re.search(r'private let service = "([^"]+)"', keychain_source)
    if service_match is None or "mobile-refresh" not in service_match.group(1):
        fail("Keychain service is not restricted to mobile refresh credentials")
    if "codex-api" in service_match.group(1).casefold():
        fail("Codex API key must not be persisted in Keychain")


def check_project_selection() -> None:
    project = ROOT / "ios" / "project.issue36.final.yml"
    if not project.is_file():
        fail("missing final XcodeGen project")
    source = project.read_text(encoding="utf-8")
    for required in (
        "SWIFT_VERSION: \"6.0\"",
        "iOS: \"18.0\"",
        "TARGETED_DEVICE_FAMILY: \"1,2\"",
        "Issue36Final/Core.swift",
        "Issue36Tests",
        "Issue36UITests",
        "Issue36Packages/QuaZonaiAPI",
    ):
        if required not in source:
            fail(f"XcodeGen project lacks {required}")


def main() -> None:
    registry = load_registry()
    capabilities = registry.get("capabilities")
    if not isinstance(capabilities, list) or not capabilities:
        fail("registry has no capabilities")
    if registry.get("clients") != ["web", "iphone", "ipad"]:
        fail("registry clients must be web, iphone, ipad")

    document = live_openapi()
    operation_ids: set[str] = set()
    seen_ids: set[str] = set()
    domains: set[str] = set()
    for raw in capabilities:
        if not isinstance(raw, dict):
            fail("capability entry must be an object")
        capability_id = raw.get("id")
        if not isinstance(capability_id, str) or capability_id in seen_ids:
            fail(f"invalid or duplicate capability id: {capability_id!r}")
        seen_ids.add(capability_id)
        domain = raw.get("domain")
        if not isinstance(domain, str):
            fail(f"{capability_id}: missing domain")
        domains.add(domain)
        if raw.get("kind") not in {"read", "mutation", "sensitive_mutation"}:
            fail(f"{capability_id}: invalid kind")
        method, path = parse_api_operation(raw.get("api_operation"))
        operation = document.get("paths", {}).get(path, {}).get(method)
        if not isinstance(operation, dict):
            fail(f"{capability_id}: live OpenAPI lacks {method.upper()} {path}")
        operation_id = operation.get("operationId")
        if not isinstance(operation_id, str) or not operation_id:
            fail(f"{capability_id}: operationId is missing")
        if operation_id in operation_ids:
            fail(f"duplicate capability operationId: {operation_id}")
        operation_ids.add(operation_id)
        web_test = raw.get("web_test")
        if not isinstance(web_test, str) or not (ROOT / web_test).is_file():
            fail(f"{capability_id}: missing Web test {web_test!r}")
        check_test_reference(raw.get("iphone_test"), field=f"{capability_id}.iphone_test")
        check_test_reference(raw.get("ipad_test"), field=f"{capability_id}.ipad_test")
        for field in ("required_fields", "required_states", "required_errors"):
            if not isinstance(raw.get(field), list):
                fail(f"{capability_id}: {field} must be a list")
        if not isinstance(raw.get("offline_readable"), bool):
            fail(f"{capability_id}: offline_readable must be boolean")

    all_operation_ids = [
        operation.get("operationId")
        for methods in document.get("paths", {}).values()
        if isinstance(methods, dict)
        for method, operation in methods.items()
        if method in {"get", "post", "put", "patch", "delete"}
        and isinstance(operation, dict)
    ]
    if any(not isinstance(value, str) or not value for value in all_operation_ids):
        fail("one or more live OpenAPI operations lack operationId")
    if len(all_operation_ids) != len(set(all_operation_ids)):
        fail("live OpenAPI contains duplicate operationIds")

    missing_domains = sorted(REQUIRED_DOMAINS - domains)
    if missing_domains:
        fail(f"registry lacks domains: {missing_domains}")
    check_mobile_login_schema(document)
    check_web_routes(capabilities)
    check_localizations()
    check_native_security()
    check_project_selection()
    print(
        f"client-parity: {len(capabilities)} capabilities, "
        f"{len(operation_ids)} live operations, Web/iPhone/iPad coverage OK"
    )


if __name__ == "__main__":
    main()
