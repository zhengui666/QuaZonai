#!/usr/bin/env python3
"""Fail CI when a declared operator capability is not represented on all clients."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "contracts" / "client-capabilities.yaml"
IOS_PROJECT = ROOT / "ios" / "project.yml"
FORBIDDEN = ("web_only", "ios_later", "desktop_required", "not_supported_on_mobile", "请去 Web 完成")
REQUIRED_KEYS = {
    "id",
    "domain",
    "kind",
    "api_operation",
    "web_route",
    "web_test",
    "iphone_test",
    "ipad_test",
    "required_fields",
    "required_states",
    "required_errors",
    "offline_readable",
}
NATIVE_REFERENCE = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)\.(test[A-Za-z0-9_]*)$")
TEST_CLASS = re.compile(r"\b(?:final\s+)?class\s+([A-Za-z_][A-Za-z0-9_]*)\s*:\s*XCTestCase\b")
TEST_METHOD = re.compile(r"\bfunc\s+(test[A-Za-z0-9_]*)\s*\(")


def repository_file(raw: object, *, capability_id: str, field: str) -> Path:
    if not isinstance(raw, str) or not raw.strip():
        raise SystemExit(f"{capability_id} missing {field}")
    candidate = (ROOT / raw).resolve()
    try:
        candidate.relative_to(ROOT)
    except ValueError as exc:
        raise SystemExit(f"{capability_id} {field} escapes the repository") from exc
    if not candidate.is_file():
        raise SystemExit(f"{capability_id} {field} does not resolve to a file: {raw}")
    return candidate


def native_test_index() -> dict[str, set[str]]:
    manifest = IOS_PROJECT.read_text(encoding="utf-8")
    for source in ("Tests", "UITests"):
        if f"path: {source}" not in manifest:
            raise SystemExit(f"ios/project.yml does not include the {source} source directory")

    index: dict[str, set[str]] = {}
    for root in (ROOT / "ios" / "Tests", ROOT / "ios" / "UITests"):
        if not root.is_dir():
            raise SystemExit(f"missing native test source directory: {root.relative_to(ROOT)}")
        for path in root.rglob("*.swift"):
            text = path.read_text(encoding="utf-8")
            classes = TEST_CLASS.findall(text)
            methods = set(TEST_METHOD.findall(text))
            for class_name in classes:
                index.setdefault(class_name, set()).update(methods)
    if not index:
        raise SystemExit("no XCTestCase symbols were found in native test sources")
    return index


def require_native_reference(
    raw: object,
    *,
    capability_id: str,
    field: str,
    test_index: dict[str, set[str]],
) -> None:
    if not isinstance(raw, str):
        raise SystemExit(f"{capability_id} missing {field}")
    match = NATIVE_REFERENCE.fullmatch(raw)
    if match is None:
        raise SystemExit(f"{capability_id} has invalid {field} reference: {raw!r}")
    class_name, method_name = match.groups()
    if method_name not in test_index.get(class_name, set()):
        raise SystemExit(f"{capability_id} {field} does not resolve to XCTest symbol {raw}")


def main() -> int:
    payload = json.loads(REGISTRY.read_text(encoding="utf-8"))
    capabilities = payload.get("capabilities")
    if not isinstance(capabilities, list) or not capabilities:
        raise SystemExit("client capability registry must be non-empty")
    test_index = native_test_index()
    ids: set[str] = set()
    operations: set[str] = set()
    for capability in capabilities:
        if not isinstance(capability, dict) or REQUIRED_KEYS - capability.keys():
            raise SystemExit(f"incomplete capability: {capability!r}")
        capability_id = str(capability["id"])
        if capability_id in ids:
            raise SystemExit(f"duplicate capability id: {capability_id}")
        ids.add(capability_id)
        if capability["kind"] not in {"read", "mutation", "sensitive_mutation"}:
            raise SystemExit(f"invalid kind for {capability_id}")
        route = capability["web_route"]
        if not isinstance(route, str) or not route.startswith("/"):
            raise SystemExit(f"{capability_id} has invalid web_route")
        repository_file(capability["web_test"], capability_id=capability_id, field="web_test")
        require_native_reference(
            capability["iphone_test"],
            capability_id=capability_id,
            field="iphone_test",
            test_index=test_index,
        )
        require_native_reference(
            capability["ipad_test"],
            capability_id=capability_id,
            field="ipad_test",
            test_index=test_index,
        )
        operation = str(capability["api_operation"])
        if not re.match(r"^(GET|POST|PUT|PATCH|DELETE) /api/v1/", operation):
            raise SystemExit(f"invalid api_operation for {capability_id}: {operation}")
        operations.add(operation)
        if capability["kind"] != "read" and capability["offline_readable"]:
            raise SystemExit(f"mutation cannot be offline-writable: {capability_id}")

    ios_root = ROOT / "ios"
    frontend_root = ROOT / "frontend" / "src"
    searchable = []
    for root in (ios_root, frontend_root):
        if root.exists():
            searchable.extend(path for path in root.rglob("*") if path.is_file())
    for path in searchable:
        if path.suffix.lower() not in {".swift", ".ts", ".tsx", ".json", ".md"}:
            continue
        text = path.read_text(encoding="utf-8", errors="ignore")
        for marker in FORBIDDEN:
            if marker in text:
                raise SystemExit(f"forbidden parity escape marker {marker!r} in {path.relative_to(ROOT)}")

    required_domains = {
        "home",
        "idea",
        "research",
        "alpha",
        "portfolio",
        "approval",
        "handoff",
        "administration",
        "sync",
    }
    domains = {str(item["domain"]) for item in capabilities}
    missing_domains = required_domains - domains
    if missing_domains:
        raise SystemExit(f"missing client domains: {sorted(missing_domains)}")
    symbol_count = sum(len(methods) for methods in test_index.values())
    print(
        f"client parity registry: {len(ids)} capabilities across {len(domains)} domains; "
        f"resolved {symbol_count} native test symbols"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
