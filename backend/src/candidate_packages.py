"""Build immutable Nautilus-native Candidate Bundle artifacts."""

from __future__ import annotations

import base64
import json
import os
import shutil
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from uuid import uuid4

from db.models import ApprovalSnapshot, DownstreamSystem, PortfolioCandidate
from errors import QfError
from quant_runtime import NAUTILUS_RUNTIME_NAME, PINNED_NAUTILUS_VERSION
from settings import Settings

_SECRET_KEY_FRAGMENTS = ("credential", "password", "private_key", "secret", "api_key", "token")
_REQUIRED_FILES = {
    "manifest.json",
    "requirements.lock",
    "strategy/strategy.whl",
    "strategy/strategy-config.json",
    "strategy/actor-config.json",
    "data/requirements.json",
    "data/instrument-scope.json",
    "runtime/nautilus-version.json",
    "runtime/backtest-run-config.json",
    "runtime/venue-config.json",
    "runtime/risk-config.json",
    "runtime/live-node-template.json",
    "validation/expected-orders.json",
    "validation/expected-positions.json",
    "validation/expected-statistics.json",
    "evidence/discovery-summary.json",
    "evidence/sealed-summary.json",
    "evidence/robustness-summary.json",
    "lineage.json",
}


@dataclass(frozen=True, slots=True)
class BuiltCandidatePackage:
    manifest: dict[str, Any]
    relative_path: str
    operator_summary: dict[str, Any]


def _json_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True, default=str).encode("utf-8")


def _contains_secret_key(value: Any) -> bool:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).casefold()
            if any(fragment in normalized for fragment in _SECRET_KEY_FRAGMENTS):
                return True
            if _contains_secret_key(item):
                return True
    elif isinstance(value, (list, tuple)):
        return any(_contains_secret_key(item) for item in value)
    return False


def _write_descriptor_wheel(path: Path, strategy: dict[str, Any]) -> None:
    """Freeze an importable Nautilus strategy descriptor into a valid pure-Python wheel."""
    distribution = "quazonai_nautilus_candidate_strategy"
    version = "1.0.0"
    dist_info = f"{distribution}-{version}.dist-info"
    module_path = f"{distribution}/__init__.py"
    metadata_path = f"{dist_info}/METADATA"
    wheel_path = f"{dist_info}/WHEEL"
    record_path = f"{dist_info}/RECORD"
    source = (
        '"""Frozen Nautilus strategy descriptor for one approved Candidate."""\n\n'
        f"STRATEGY_PATH = {strategy.get('strategy_path')!r}\n"
        f"CONFIG_PATH = {strategy.get('config_path')!r}\n"
        f"CONFIG = {strategy.get('config', {})!r}\n"
    )
    metadata = (
        "Metadata-Version: 2.4\n"
        "Name: quazonai-nautilus-candidate-strategy\n"
        f"Version: {version}\n"
        "Summary: Frozen NautilusTrader strategy descriptor\n"
        f"Requires-Dist: nautilus-trader=={PINNED_NAUTILUS_VERSION}\n"
    )
    wheel = "Wheel-Version: 1.0\nGenerator: QuaZonai\nRoot-Is-Purelib: true\nTag: py3-none-any\n"
    record = "\n".join(
        [
            f"{module_path},,",
            f"{metadata_path},,",
            f"{wheel_path},,",
            f"{record_path},,",
            "",
        ]
    )
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr(module_path, source)
        archive.writestr(metadata_path, metadata)
        archive.writestr(wheel_path, wheel)
        archive.writestr(record_path, record)


def _write_strategy_wheel(path: Path, strategy: dict[str, Any]) -> None:
    encoded = strategy.get("wheel_base64")
    if encoded:
        try:
            payload = base64.b64decode(str(encoded), validate=True)
        except ValueError as exc:
            raise QfError(
                "CANDIDATE_BUNDLE_STRATEGY_INVALID",
                "The frozen strategy wheel is not valid base64.",
                422,
            ) from exc
        path.write_bytes(payload)
        try:
            with zipfile.ZipFile(path) as archive:
                if not any(name.endswith(".dist-info/WHEEL") for name in archive.namelist()):
                    raise QfError(
                        "CANDIDATE_BUNDLE_STRATEGY_INVALID",
                        "The frozen strategy artifact is not a valid wheel.",
                        422,
                    )
        except zipfile.BadZipFile as exc:
            raise QfError(
                "CANDIDATE_BUNDLE_STRATEGY_INVALID",
                "The frozen strategy artifact is not a valid wheel.",
                422,
            ) from exc
        return
    _write_descriptor_wheel(path, strategy)


def _validate_bundle(staging: Path, manifest: dict[str, Any]) -> None:
    actual = {
        path.relative_to(staging).as_posix()
        for path in staging.rglob("*")
        if path.is_file()
    }
    missing = sorted(_REQUIRED_FILES - actual)
    if missing:
        raise QfError(
            "CANDIDATE_BUNDLE_INCOMPLETE",
            "Candidate Bundle is missing required Nautilus-native files.",
            422,
            {"missing": missing},
        )
    if manifest.get("runtime", {}).get("name") != NAUTILUS_RUNTIME_NAME:
        raise QfError(
            "CANDIDATE_BUNDLE_RUNTIME_INVALID",
            "Candidate Bundle runtime must be NautilusTrader.",
            422,
        )
    if manifest.get("runtime", {}).get("version") != PINNED_NAUTILUS_VERSION:
        raise QfError(
            "CANDIDATE_BUNDLE_RUNTIME_INVALID",
            "Candidate Bundle runtime version does not match the pinned version.",
            422,
        )
    if _contains_secret_key(manifest):
        raise QfError(
            "CANDIDATE_BUNDLE_SECRET_FORBIDDEN",
            "Candidate Bundle manifest contains a secret-bearing field.",
            422,
        )


def build_candidate_package(
    settings: Settings,
    *,
    approval: ApprovalSnapshot,
    candidate: PortfolioCandidate,
    downstream: DownstreamSystem,
) -> BuiltCandidatePackage:
    """Freeze one approved Candidate for Nautilus Backtest/Paper/Live reuse."""
    package_id = uuid4()
    staging = settings.package_root / "staging" / str(package_id)
    final_root = settings.package_root / str(package_id)
    archive_name = "candidate-bundle.zip"
    metrics = dict(candidate.metrics or {})
    strategy = dict(metrics.get("strategy_artifact") or {})
    evidence = dict(metrics.get("quant_evidence") or {})
    if _contains_secret_key(strategy) or _contains_secret_key(evidence):
        raise QfError(
            "CANDIDATE_BUNDLE_SECRET_FORBIDDEN",
            "Candidate runtime inputs must not contain broker or provider secrets.",
            422,
        )
    staging.mkdir(parents=True, exist_ok=False)
    try:
        for directory in (
            "strategy",
            "data/custom-data-schemas",
            "runtime",
            "validation/fixture-catalog",
            "evidence",
        ):
            (staging / directory).mkdir(parents=True, exist_ok=True)

        _write_strategy_wheel(staging / "strategy" / "strategy.whl", strategy)
        (staging / "requirements.lock").write_text(
            f"nautilus_trader=={PINNED_NAUTILUS_VERSION}\n",
            encoding="utf-8",
        )
        strategy_config = {
            "strategy_path": strategy.get("strategy_path"),
            "config_path": strategy.get("config_path"),
            "config": strategy.get("config", {}),
            "artifact_id": strategy.get("artifact_id"),
        }
        (staging / "strategy" / "strategy-config.json").write_bytes(
            _json_bytes(strategy_config)
        )
        (staging / "strategy" / "actor-config.json").write_bytes(_json_bytes([]))

        discovery = dict(evidence.get("discovery") or {})
        sealed = dict(evidence.get("sealed") or {})
        catalog_uri = discovery.get("catalog_uri") or sealed.get("catalog_uri")
        data_requirements = {
            "catalog_uri": catalog_uri,
            "nautilus_data_type": "QuoteTick",
            "dataset_revision_id": (
                candidate.metrics.get("dataset_revision_id") if candidate.metrics else None
            ),
        }
        (staging / "data" / "requirements.json").write_bytes(_json_bytes(data_requirements))
        instrument_scope = [
            str(member.get("instrument_id") or member.get("symbol"))
            for member in candidate.members
            if member.get("instrument_id") or member.get("symbol")
        ]
        (staging / "data" / "instrument-scope.json").write_bytes(
            _json_bytes(instrument_scope)
        )
        (staging / "data" / "custom-data-schemas" / ".keep").write_text(
            "No custom data schemas are required by this Candidate.\n",
            encoding="utf-8",
        )

        runtime = {"name": NAUTILUS_RUNTIME_NAME, "version": PINNED_NAUTILUS_VERSION}
        (staging / "runtime" / "nautilus-version.json").write_bytes(_json_bytes(runtime))
        (staging / "runtime" / "backtest-run-config.json").write_bytes(
            _json_bytes(
                {
                    "strategy": strategy_config,
                    "catalog_uri": catalog_uri,
                    "candidate_id": str(candidate.id),
                }
            )
        )
        (staging / "runtime" / "venue-config.json").write_bytes(
            _json_bytes({"source": "approved quant experiment contract"})
        )
        (staging / "runtime" / "risk-config.json").write_bytes(
            _json_bytes(
                {
                    "owner": "NAUTILUS_TRADER_RISK_ENGINE",
                    "research_promotion_owner": "QUAZONAI",
                }
            )
        )
        (staging / "runtime" / "live-node-template.json").write_bytes(
            _json_bytes(
                {
                    "template_only": True,
                    "broker_credentials": "MUST_BE_PROVIDED_BY_DOWNSTREAM_RUNTIME",
                    "quazonai_controls_runtime": False,
                }
            )
        )

        reports = dict(discovery.get("reports") or {})
        (staging / "validation" / "expected-orders.json").write_bytes(
            _json_bytes(reports.get("orders", []))
        )
        (staging / "validation" / "expected-positions.json").write_bytes(
            _json_bytes(reports.get("positions", []))
        )
        (staging / "validation" / "expected-statistics.json").write_bytes(
            _json_bytes(discovery.get("statistics", {}))
        )
        (staging / "validation" / "fixture-catalog" / "README.md").write_text(
            "The downstream verifier supplies a catalog satisfying data/requirements.json.\n",
            encoding="utf-8",
        )

        (staging / "evidence" / "discovery-summary.json").write_bytes(
            _json_bytes(discovery)
        )
        (staging / "evidence" / "sealed-summary.json").write_bytes(_json_bytes(sealed))
        (staging / "evidence" / "robustness-summary.json").write_bytes(
            _json_bytes(metrics.get("robustness_summary", {}))
        )
        approval_summary = {
            "approval_id": str(approval.id),
            "candidate_id": str(candidate.id),
            "purpose": approval.purpose,
            "evidence_summary": approval.evidence_summary,
            "risk_summary": approval.risk_summary,
            "cost_summary": approval.cost_summary,
            "capacity_summary": approval.capacity_summary,
        }
        lineage = {
            "candidate_id": str(candidate.id),
            "candidate_family_id": (
                str(candidate.candidate_family_id) if candidate.candidate_family_id else None
            ),
            "portfolio_program_id": str(candidate.portfolio_program_id),
            "evaluation_episode_id": (
                str(candidate.evaluation_episode_id) if candidate.evaluation_episode_id else None
            ),
            "strategy_artifact_id": strategy.get("artifact_id"),
        }
        (staging / "lineage.json").write_bytes(_json_bytes(lineage))

        manifest = {
            "package_contract_version": downstream.package_contract_version,
            "bundle_kind": "NAUTILUS_NATIVE_CANDIDATE",
            "candidate_id": str(candidate.id),
            "approval_id": str(approval.id),
            "purpose": approval.purpose,
            "runtime": runtime,
            "strategy": "strategy/strategy.whl",
            "data_requirements": "data/requirements.json",
            "validation": {
                "orders": "validation/expected-orders.json",
                "positions": "validation/expected-positions.json",
                "statistics": "validation/expected-statistics.json",
            },
            "evidence": {
                "discovery": "evidence/discovery-summary.json",
                "sealed": "evidence/sealed-summary.json",
                "robustness": "evidence/robustness-summary.json",
            },
            "lineage": "lineage.json",
            "required_files": sorted(_REQUIRED_FILES),
            "contains_broker_credentials": False,
        }
        (staging / "manifest.json").write_bytes(_json_bytes(manifest))
        _validate_bundle(staging, manifest)

        archive_path = staging / archive_name
        with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(staging.rglob("*")):
                if path.is_file() and path != archive_path:
                    archive.write(path, path.relative_to(staging).as_posix())
        final_root.parent.mkdir(parents=True, exist_ok=True)
        os.replace(staging, final_root)
        return BuiltCandidatePackage(
            manifest=manifest,
            relative_path=(Path(str(package_id)) / archive_name).as_posix(),
            operator_summary=approval_summary,
        )
    except Exception:
        shutil.rmtree(staging, ignore_errors=True)
        raise


def resolve_package_archive(settings: Settings, relative_path: str) -> Path:
    root = settings.package_root.resolve()
    candidate = (root / relative_path).resolve()
    if not candidate.is_relative_to(root):
        raise QfError("CANDIDATE_PACKAGE_PATH_INVALID", "Candidate Bundle path is invalid.", 500)
    if not candidate.is_file():
        raise QfError("CANDIDATE_PACKAGE_MISSING", "Candidate Bundle artifact is missing.", 500)
    return candidate
