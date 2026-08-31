#!/usr/bin/env python3
"""Fail CI when a declared operator capability is not represented on all clients."""

from __future__ import annotations

import json
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
REGISTRY = ROOT / "contracts" / "client-capabilities.yaml"
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


def main() -> int:
    payload = json.loads(REGISTRY.read_text(encoding="utf-8"))
    capabilities = payload.get("capabilities")
    if not isinstance(capabilities, list) or not capabilities:
        raise SystemExit("client capability registry must be non-empty")
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
        for key in ("web_route", "web_test", "iphone_test", "ipad_test"):
            if not capability[key]:
                raise SystemExit(f"{capability_id} missing {key}")
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
    print(f"client parity registry: {len(ids)} capabilities across {len(domains)} domains")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
