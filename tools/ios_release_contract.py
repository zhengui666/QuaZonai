#!/usr/bin/env python3
"""Static release, localization, accessibility, and security contract for the native app."""

from __future__ import annotations

import json
import plistlib
import re
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
IOS = ROOT / "ios"
REQUIRED_LOCALES = {"en", "zh-Hans", "zh-Hant", "ja", "ko", "es", "ar"}
REQUIRED_SOURCE_SIGNALS = {
    "SwiftUI": r"\bimport SwiftUI\b",
    "SwiftData cache": r"\b(import SwiftData|@Model\b|ModelContainer\b)",
    "LocalAuthentication": r"\b(import LocalAuthentication|LAContext\b)",
    "Keychain": r"\b(import Security|SecItem(Add|CopyMatching|Update|Delete))\b",
    "Swift Charts": r"\b(import Charts|Chart\s*\{)",
    "OSLog": r"\b(import OSLog|Logger\s*\()",
    "adaptive iPad shell": r"\bNavigationSplitView\b",
    "compact iPhone shell": r"\bTabView\b",
    "secure API key input": r"\bSecureField\b",
    "native concurrency": r"\b(actor|Task\s*\{|async\s+throws|async\s*->)\b",
    "URLSession transport": r"\bURLSession\b",
    "SSE recovery": r"\b(EventStreamActor|text/event-stream|Last-Event-ID|lastEventID)\b",
    "accessibility": r"\.accessibility(Label|Value|Hint|Element|Identifier)\b",
    "scene privacy lifecycle": r"\bscenePhase\b",
}
BANNED = {
    "web shell": r"\bWKWebView\b",
    "machine credential": r"QUAZONAI_API_TOKEN",
    "username login field": r'''(?i)["']username["']\s*:''',
    "password login field": r'''(?i)["']password["']\s*:''',
    "TLS bypass": r"(?i)trustAll|allowInvalidCertificate|challenge\.protectionSpace\.serverTrust",
    "execution control": r"(?i)stop runtime|undeploy|close position|emergency liquidate|submit_order|cancel_order",
}


def fail(message: str) -> None:
    raise SystemExit(f"iOS release contract violation: {message}")


def load_json(path: Path) -> dict[str, object]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        fail(f"cannot parse {path.relative_to(ROOT)}: {exc}")
    if not isinstance(value, dict):
        fail(f"{path.relative_to(ROOT)} must contain an object")
    return value


def production_source() -> str:
    files = [
        path
        for path in IOS.rglob("*.swift")
        if not any(part in {"Tests", "UITests", "XcodeTests", ".build"} for part in path.parts)
    ]
    if not files:
        fail("production Swift sources are missing")
    return "\n".join(path.read_text(encoding="utf-8") for path in files)


def validate_source() -> None:
    text = production_source()
    for label, pattern in REQUIRED_SOURCE_SIGNALS.items():
        if re.search(pattern, text) is None:
            fail(f"missing implementation signal: {label}")
    for label, pattern in BANNED.items():
        if re.search(pattern, text):
            fail(f"forbidden implementation signal: {label}")

    openapi_evidence = (
        "OpenAPIRuntime" in text
        or "OpenAPIURLSession" in text
        or any(IOS.rglob("openapi-generator-config.yaml"))
    )
    if not openapi_evidence:
        fail("Swift OpenAPI generated-client integration is missing")


def validate_localization() -> None:
    catalogs = list((IOS / "Resources").rglob("*.xcstrings"))
    if not catalogs:
        fail("string catalogs are missing")
    parity_catalog = IOS / "Resources/ClientParity.xcstrings"
    catalog = load_json(parity_catalog)
    strings = catalog.get("strings")
    if not isinstance(strings, dict) or not strings:
        fail("ClientParity.xcstrings contains no strings")
    for key, raw in strings.items():
        if not isinstance(raw, dict):
            fail(f"invalid localization entry: {key}")
        localizations = raw.get("localizations")
        if not isinstance(localizations, dict):
            fail(f"missing localizations for {key}")
        missing = REQUIRED_LOCALES - set(localizations)
        if missing:
            fail(f"{key} is missing locales: {sorted(missing)}")
        for locale in REQUIRED_LOCALES:
            unit = localizations[locale]
            if not isinstance(unit, dict):
                fail(f"invalid {locale} localization for {key}")
            string_unit = unit.get("stringUnit")
            if not isinstance(string_unit, dict) or not string_unit.get("value"):
                fail(f"empty {locale} localization for {key}")


def validate_privacy() -> None:
    path = IOS / "Resources/PrivacyInfo.xcprivacy"
    try:
        manifest = plistlib.loads(path.read_bytes())
    except (OSError, plistlib.InvalidFileException) as exc:
        fail(f"invalid PrivacyInfo.xcprivacy: {exc}")
    if manifest.get("NSPrivacyTracking") is not False:
        fail("tracking must be declared false")
    if manifest.get("NSPrivacyTrackingDomains") != []:
        fail("tracking domains must be empty")
    if manifest.get("NSPrivacyCollectedDataTypes") != []:
        fail("collected data types must be empty")


def validate_release_metadata() -> None:
    metadata = load_json(IOS / "AppStore/metadata.json")
    if metadata.get("minimum_os_version") != "18.0":
        fail("App Store minimum OS must be 18.0")
    if metadata.get("supports_iphone") is not True or metadata.get("supports_ipad") is not True:
        fail("metadata must declare both iPhone and iPad support")
    locales = metadata.get("supported_locales")
    if not isinstance(locales, list) or set(locales) != REQUIRED_LOCALES:
        fail("App Store locales do not match the seven-language contract")
    if metadata.get("tracking") is not False or metadata.get("collected_data_types") != []:
        fail("App Store privacy metadata drifted from PrivacyInfo.xcprivacy")
    for required in (
        IOS / "AppStore/PRIVACY.md",
        IOS / "AppStore/AppIcon.svg",
        IOS / "Resources/LaunchScreen.storyboard",
    ):
        if not required.is_file() or required.stat().st_size == 0:
            fail(f"missing release resource: {required.relative_to(ROOT)}")


def validate_project_baseline() -> None:
    candidates = [
        *IOS.rglob("project.pbxproj"),
        *IOS.rglob("project.yml"),
        *IOS.rglob("Package.swift"),
    ]
    if not candidates:
        fail("Xcode project or project generator specification is missing")
    text = "\n".join(path.read_text(encoding="utf-8") for path in candidates)
    if not re.search(r"(IPHONEOS_DEPLOYMENT_TARGET\s*=\s*18(\.0)?|deploymentTarget:\s*18(\.0)?|\.iOS\(\.v18\))", text):
        fail("minimum iOS deployment target is not 18")
    universal = (
        re.search(r'TARGETED_DEVICE_FAMILY\s*=\s*"?1,2"?', text)
        or re.search(r"deviceFamilies:\s*\[[^]]*1[^]]*2[^]]*\]", text)
        or ("supports_iphone" in (IOS / "AppStore/metadata.json").read_text() and "supports_ipad" in (IOS / "AppStore/metadata.json").read_text())
    )
    if not universal:
        fail("Universal iPhone/iPad target declaration is missing")


def main() -> None:
    validate_source()
    validate_localization()
    validate_privacy()
    validate_release_metadata()
    validate_project_baseline()
    print("iOS release contract valid")


if __name__ == "__main__":
    main()
