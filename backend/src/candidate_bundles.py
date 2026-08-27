"""Nautilus-native Candidate Bundle construction.

The bundle packages the exact strategy artifact validated by the Search Ledger;
it does not invent a second micro-runtime or translate target weights into a
QuaZonai-owned order model.
"""

from __future__ import annotations

import base64
from dataclasses import dataclass
from datetime import UTC, datetime
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
from typing import Any
from uuid import UUID, uuid4
import zipfile

from errors import QfError
from quant_runtime.contracts import PINNED_NAUTILUS_VERSION, StrategyArtifact

BUNDLE_CONTRACT_VERSION = "2"


def _json_bytes(value: Any) -> bytes:
    return json.dumps(
        value,
        ensure_ascii=False,
        indent=2,
        sort_keys=True,
        separators=(",", ": "),
        default=str,
    ).encode("utf-8") + b"\n"


def _safe_path(path: str) -> PurePosixPath:
    candidate = PurePosixPath(path.replace("\\", "/"))
    if candidate.is_absolute() or ".." in candidate.parts or not candidate.parts:
        raise QfError(
            "CANDIDATE_STRATEGY_ARTIFACT_INVALID",
            "Strategy source contains an unsafe path.",
            422,
            {"path": path},
        )
    return candidate


def _record_hash(payload: bytes) -> str:
    digest = base64.urlsafe_b64encode(hashlib.sha256(payload).digest()).rstrip(b"=").decode()
    return f"sha256={digest}"


def _strategy_wheel(artifact: StrategyArtifact, *, candidate_id: UUID) -> bytes:
    if artifact.kind != "SOURCE_BUNDLE":
        raise QfError(
            "CANDIDATE_STRATEGY_ARTIFACT_INVALID",
            "Candidate promotion requires the exact source-bundle strategy from research evidence.",
            422,
        )
    distribution = "quazonai_candidate_strategy"
    version = f"0.0.{candidate_id.int % 1_000_000}"
    dist_info = f"{distribution}-{version}.dist-info"
    files: dict[str, bytes] = {}
    for path, source in artifact.source_files.items():
        safe = _safe_path(path)
        if safe.suffix != ".py":
            raise QfError(
                "CANDIDATE_STRATEGY_ARTIFACT_INVALID",
                "Only Python source modules are accepted in the strategy wheel.",
                422,
                {"path": path},
            )
        files[str(safe)] = source.encode("utf-8")
    strategy_module = artifact.strategy_path.split(":", 1)[0].replace(".", "/") + ".py"
    config_module = artifact.config_path.split(":", 1)[0].replace(".", "/") + ".py"
    if strategy_module not in files or config_module not in files:
        raise QfError(
            "CANDIDATE_STRATEGY_ARTIFACT_INVALID",
            "Strategy and config import paths must resolve inside the supplied source bundle.",
            422,
        )

    metadata = (
        "Metadata-Version: 2.3\n"
        f"Name: {distribution}\n"
        f"Version: {version}\n"
        "Summary: Sealed QuaZonai Nautilus strategy artifact\n"
        f"Requires-Dist: nautilus_trader (=={PINNED_NAUTILUS_VERSION})\n"
        "\n"
    ).encode()
    wheel_metadata = (
        "Wheel-Version: 1.0\n"
        "Generator: QuaZonai Candidate Bundle v2\n"
        "Root-Is-Purelib: true\n"
        "Tag: py3-none-any\n"
        "\n"
    ).encode()
    files[f"{dist_info}/METADATA"] = metadata
    files[f"{dist_info}/WHEEL"] = wheel_metadata

    records = [f"{path},{_record_hash(payload)},{len(payload)}" for path, payload in sorted(files.items())]
    records.append(f"{dist_info}/RECORD,,")
    files[f"{dist_info}/RECORD"] = ("\n".join(records) + "\n").encode()

    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path, payload in sorted(files.items()):
            archive.writestr(path, payload)
    return stream.getvalue()


def _candidate_value(candidate: Any, name: str, default: Any = None) -> Any:
    return getattr(candidate, name, default)


def _member_payload(candidate: Any) -> list[dict[str, Any]]:
    members = list(_candidate_value(candidate, "members", []) or [])
    return [
        {
            "alpha_id": str(getattr(member, "alpha_id", "")),
            "weight": str(getattr(member, "weight", "")),
        }
        for member in sorted(members, key=lambda item: str(getattr(item, "alpha_id", "")))
    ]


@dataclass(slots=True)
class BuiltCandidateBundle:
    archive_bytes: bytes
    manifest: dict[str, Any]
    validation_summary: dict[str, Any]
    package_sha256: str

    @property
    def package_bytes(self) -> bytes:
        return self.archive_bytes

    @property
    def content_hash(self) -> str:
        return self.package_sha256

    @property
    def sha256(self) -> str:
        return self.package_sha256

    @property
    def size_bytes(self) -> int:
        return len(self.archive_bytes)


# Existing API/database naming remains an implementation detail while the wire
# contract is Candidate Bundle v2.
BuiltCandidatePackage = BuiltCandidateBundle


def build_candidate_bundle(
    settings: Any,
    *,
    candidate: Any,
    approval_snapshot: Any | None = None,
    package_id: UUID | None = None,
    **_: Any,
) -> BuiltCandidateBundle:
    del settings  # Core settings never supply broker/runtime credentials to a bundle.
    candidate_id = _candidate_value(candidate, "id")
    if candidate_id is None:
        raise QfError("CANDIDATE_INVALID", "Candidate id is required.", 422)
    package_id = package_id or uuid4()
    metrics = dict(_candidate_value(candidate, "metrics", {}) or {})
    runtime = metrics.get("nautilus")
    if not isinstance(runtime, dict):
        raise QfError(
            "CANDIDATE_NAUTILUS_EVIDENCE_MISSING",
            "Candidate must reference a successful Nautilus portfolio-simulation experiment.",
            422,
        )
    try:
        artifact = StrategyArtifact.model_validate(runtime["strategy_artifact"])
    except (KeyError, ValueError) as exc:
        raise QfError(
            "CANDIDATE_STRATEGY_ARTIFACT_INVALID",
            "Candidate strategy artifact is missing or invalid.",
            422,
        ) from exc
    evidence = runtime.get("evidence")
    required_evidence = {"experiment_id", "orders", "fills", "positions", "pnl", "statistics"}
    if not isinstance(evidence, dict) or not required_evidence.issubset(evidence):
        raise QfError(
            "CANDIDATE_NAUTILUS_EVIDENCE_INCOMPLETE",
            "Candidate evidence must include the canonical Nautilus transaction record.",
            422,
            {"required": sorted(required_evidence)},
        )
    simulation_experiment_id = _candidate_value(candidate, "simulation_experiment_id")
    if simulation_experiment_id is None:
        simulation_experiment_id = evidence.get("experiment_id")
    if not simulation_experiment_id:
        raise QfError(
            "CANDIDATE_SIMULATION_LINEAGE_MISSING",
            "Candidate must retain its portfolio-simulation experiment id.",
            422,
        )

    wheel = _strategy_wheel(artifact, candidate_id=candidate_id)
    strategy_config = dict(artifact.config)
    target_weights = _member_payload(candidate)
    approval = {
        "approval_snapshot_id": str(getattr(approval_snapshot, "id", "")) or None,
        "policy_version": getattr(approval_snapshot, "policy_version", None),
        "approved_at": getattr(approval_snapshot, "created_at", None),
        "evidence_summary": getattr(approval_snapshot, "evidence_summary", None),
    }
    manifest: dict[str, Any] = {
        "contract": "QUAZONAI_NAUTILUS_CANDIDATE_BUNDLE",
        "contract_version": BUNDLE_CONTRACT_VERSION,
        "bundle_id": str(package_id),
        "candidate_id": str(candidate_id),
        "program_id": str(_candidate_value(candidate, "program_id", "")),
        "branch_id": str(_candidate_value(candidate, "branch_id", "")),
        "created_at": datetime.now(UTC).isoformat(),
        "runtime": {
            "name": "NAUTILUS_TRADER",
            "version": PINNED_NAUTILUS_VERSION,
            "deployment": "REMOTE_INDEPENDENT_RUNTIME",
            "paper_live_reuse": "SAME_STRATEGY_WHEEL_AND_CONFIG",
        },
        "strategy": {
            "artifact_id": artifact.artifact_id,
            "wheel": "strategy.whl",
            "strategy_path": artifact.strategy_path,
            "config_path": artifact.config_path,
            "config": "strategy-config.json",
        },
        "data_requirements": runtime.get("data_requirements", {}),
        "instrument_scope": runtime.get("instrument_scope", []),
        "backtest_run_config": runtime.get("backtest_run_config", {}),
        "venue_config": runtime.get("venue_config", {}),
        "risk_config": runtime.get("risk_config", {}),
        "live_node_template": runtime.get(
            "live_node_template",
            {
                "strategy_wheel": "strategy.whl",
                "strategy_config": "strategy-config.json",
                "broker_adapter": "DOWNSTREAM_RUNTIME_OWNED",
                "credentials": "INJECT_AT_REMOTE_RUNTIME_ONLY",
            },
        ),
        "target_weights": target_weights,
        "lineage": {
            "portfolio_simulation_experiment_id": str(simulation_experiment_id),
            "dataset_revision_ids": runtime.get("dataset_revision_ids", []),
            "alpha_qualification_ids": runtime.get("alpha_qualification_ids", []),
            "candidate_lineage": _candidate_value(candidate, "lineage", {}),
            "approval": approval,
        },
        "evidence": {
            "canonical_runtime": "NAUTILUS_TRADER",
            "discovery": runtime.get("discovery_evidence", []),
            "sealed": runtime.get("sealed_disclosures", []),
            "portfolio_simulation": "evidence/portfolio-simulation.json",
            "expected_fixture": "validation/expected-evidence.json",
        },
    }
    expected = {
        "experiment_id": evidence["experiment_id"],
        "orders": evidence["orders"],
        "fills": evidence["fills"],
        "positions": evidence["positions"],
        "pnl": evidence["pnl"],
        "statistics": evidence["statistics"],
    }
    files = {
        "MANIFEST.json": _json_bytes(manifest),
        "requirements.lock": f"nautilus_trader=={PINNED_NAUTILUS_VERSION}\n".encode(),
        "strategy.whl": wheel,
        "strategy-config.json": _json_bytes(strategy_config),
        "actor-config.json": _json_bytes(runtime.get("actor_config", {})),
        "data-requirements.json": _json_bytes(manifest["data_requirements"]),
        "instrument-scope.json": _json_bytes(manifest["instrument_scope"]),
        "backtest-run-config.json": _json_bytes(manifest["backtest_run_config"]),
        "venue-config.json": _json_bytes(manifest["venue_config"]),
        "risk-config.json": _json_bytes(manifest["risk_config"]),
        "live-node-template.json": _json_bytes(manifest["live_node_template"]),
        "lineage.json": _json_bytes(manifest["lineage"]),
        "evidence/portfolio-simulation.json": _json_bytes(evidence),
        "validation/expected-evidence.json": _json_bytes(expected),
    }
    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path, payload in sorted(files.items()):
            archive.writestr(path, payload)
    archive_bytes = stream.getvalue()
    validation_summary = validate_candidate_bundle(archive_bytes)
    if not validation_summary["valid"]:
        raise QfError(
            "CANDIDATE_BUNDLE_INVALID",
            "Generated Candidate Bundle failed structural validation.",
            500,
            validation_summary,
        )
    return BuiltCandidateBundle(
        archive_bytes=archive_bytes,
        manifest=manifest,
        validation_summary=validation_summary,
        package_sha256=hashlib.sha256(archive_bytes).hexdigest(),
    )


def build_candidate_package(
    settings: Any,
    candidate: Any | None = None,
    approval_snapshot: Any | None = None,
    package_id: UUID | None = None,
    *args: Any,
    **kwargs: Any,
) -> BuiltCandidateBundle:
    """Compatibility entry point with Candidate Bundle v2 semantics."""
    if candidate is None and args:
        candidate = args[0]
        args = args[1:]
    if approval_snapshot is None and args:
        approval_snapshot = args[0]
        args = args[1:]
    if package_id is None and args:
        package_id = args[0]
    candidate = candidate or kwargs.pop("portfolio_candidate", None)
    approval_snapshot = approval_snapshot or kwargs.pop("approval", None)
    package_id = package_id or kwargs.pop("candidate_package_id", None)
    if candidate is None:
        raise TypeError("candidate is required")
    return build_candidate_bundle(
        settings,
        candidate=candidate,
        approval_snapshot=approval_snapshot,
        package_id=package_id,
        **kwargs,
    )


def validate_candidate_bundle(archive_bytes: bytes) -> dict[str, Any]:
    required = {
        "MANIFEST.json",
        "requirements.lock",
        "strategy.whl",
        "strategy-config.json",
        "actor-config.json",
        "data-requirements.json",
        "instrument-scope.json",
        "backtest-run-config.json",
        "venue-config.json",
        "risk-config.json",
        "live-node-template.json",
        "lineage.json",
        "evidence/portfolio-simulation.json",
        "validation/expected-evidence.json",
    }
    findings: list[dict[str, Any]] = []
    try:
        with zipfile.ZipFile(io.BytesIO(archive_bytes)) as archive:
            names = set(archive.namelist())
            unsafe = sorted(
                name
                for name in names
                if PurePosixPath(name).is_absolute() or ".." in PurePosixPath(name).parts
            )
            if unsafe:
                findings.append({"code": "UNSAFE_ARCHIVE_PATHS", "paths": unsafe})
            missing = sorted(required.difference(names))
            if missing:
                findings.append({"code": "BUNDLE_FILES_MISSING", "paths": missing})
            if "MANIFEST.json" in names:
                manifest = json.loads(archive.read("MANIFEST.json"))
                if manifest.get("contract_version") != BUNDLE_CONTRACT_VERSION:
                    findings.append({"code": "CONTRACT_VERSION_INVALID"})
                if manifest.get("runtime", {}).get("version") != PINNED_NAUTILUS_VERSION:
                    findings.append({"code": "NAUTILUS_VERSION_INVALID"})
                encoded = json.dumps(manifest, sort_keys=True).lower()
                for secret_key in ("password", "private_key", "api_key", "broker_token"):
                    if f'"{secret_key}"' in encoded:
                        findings.append({"code": "LIVE_SECRET_FIELD_PRESENT", "field": secret_key})
    except (zipfile.BadZipFile, ValueError) as exc:
        findings.append({"code": "BUNDLE_ARCHIVE_INVALID", "detail": str(exc)})
    return {
        "valid": not findings,
        "contract": "QUAZONAI_NAUTILUS_CANDIDATE_BUNDLE",
        "contract_version": BUNDLE_CONTRACT_VERSION,
        "findings": findings,
    }


# Historical import name; all generated bytes follow Candidate Bundle v2.
validate_candidate_package = validate_candidate_bundle
