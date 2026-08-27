"""Build immutable Nautilus-native Candidate Bundle v2 artifacts.

QuaZonai freezes the exact Nautilus strategy artifact and research evidence that
passed governance.  The bundle is a downstream handoff artifact; it never
contains broker credentials or a QuaZonai-owned execution shim.
"""

from __future__ import annotations

import base64
from collections.abc import Mapping
from dataclasses import dataclass
from datetime import UTC, datetime
import hashlib
import io
import json
import os
from pathlib import Path, PurePosixPath
import tempfile
from typing import Any
from uuid import UUID, uuid4
import zipfile

from errors import QfError
from quant_runtime.contracts import (
    PINNED_NAUTILUS_VERSION,
    CandidateVerificationRequest,
    StrategyArtifact,
)

BUNDLE_CONTRACT_VERSION = "2"
BUNDLE_FILENAME = "candidate-bundle.zip"


def _json_safe(value: Any) -> Any:
    """Normalize bundle metadata for both archive JSON and JSONB persistence."""
    if isinstance(value, datetime):
        return value.isoformat()
    if isinstance(value, UUID):
        return str(value)
    if isinstance(value, Mapping):
        return {str(key): _json_safe(child) for key, child in value.items()}
    if isinstance(value, (list, tuple)):
        return [_json_safe(child) for child in value]
    return value


def _json_bytes(value: Any) -> bytes:
    return (
        json.dumps(
            _json_safe(value),
            ensure_ascii=False,
            indent=2,
            sort_keys=True,
            separators=(",", ": "),
        ).encode("utf-8")
        + b"\n"
    )


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
    """Return the PEP 427 RECORD digest required by the wheel format."""
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

    files[f"{dist_info}/METADATA"] = (
        "Metadata-Version: 2.3\n"
        f"Name: {distribution}\n"
        f"Version: {version}\n"
        "Summary: Sealed QuaZonai Nautilus strategy artifact\n"
        f"Requires-Dist: nautilus_trader (=={PINNED_NAUTILUS_VERSION})\n"
        "\n"
    ).encode()
    files[f"{dist_info}/WHEEL"] = (
        "Wheel-Version: 1.0\n"
        "Generator: QuaZonai Candidate Bundle v2\n"
        "Root-Is-Purelib: true\n"
        "Tag: py3-none-any\n"
        "\n"
    ).encode()
    records = [
        f"{path},{_record_hash(payload)},{len(payload)}" for path, payload in sorted(files.items())
    ]
    records.append(f"{dist_info}/RECORD,,")
    files[f"{dist_info}/RECORD"] = ("\n".join(records) + "\n").encode()

    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path, payload in sorted(files.items()):
            archive.writestr(path, payload)
    return stream.getvalue()


def _value(item: Any, name: str, default: Any = None) -> Any:
    if isinstance(item, Mapping):
        return item.get(name, default)
    return getattr(item, name, default)


def _first(item: Any, names: tuple[str, ...], default: Any = None) -> Any:
    for name in names:
        value = _value(item, name)
        if value is not None:
            return value
    return default


def _member_payload(candidate: Any) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for index, member in enumerate(list(_value(candidate, "members", []) or [])):
        alpha_id = _first(member, ("alpha_qualification_id", "alpha_id"))
        if alpha_id in (None, ""):
            raise QfError(
                "CANDIDATE_MEMBER_LINEAGE_MISSING",
                "Every Candidate member must retain its Alpha Qualification id.",
                422,
                {"member_index": index},
            )
        raw_instruments = _value(member, "instrument_ids")
        if raw_instruments is None:
            singular = _first(member, ("instrument_id", "symbol", "asset", "id"))
            instruments = [singular] if singular not in (None, "") else []
        elif isinstance(raw_instruments, (list, tuple, set)):
            instruments = list(raw_instruments)
        else:
            raise QfError(
                "CANDIDATE_MEMBER_INSTRUMENTS_INVALID",
                "Candidate member instrument_ids must be a list.",
                422,
                {"member_index": index},
            )
        normalized_instruments = list(
            dict.fromkeys(
                str(value).strip()
                for value in instruments
                if value is not None and str(value).strip()
            )
        )
        if not normalized_instruments:
            raise QfError(
                "CANDIDATE_MEMBER_INSTRUMENTS_MISSING",
                "Every Candidate member must retain at least one governed instrument.",
                422,
                {"member_index": index},
            )
        target_weight = str(_first(member, ("target_weight", "weight"), "0"))
        for instrument_id in normalized_instruments:
            rows.append(
                {
                    "alpha_qualification_id": str(alpha_id),
                    "instrument_id": instrument_id,
                    "target_weight": target_weight,
                }
            )
    return sorted(
        rows,
        key=lambda row: (
            row["instrument_id"],
            row["alpha_qualification_id"],
        ),
    )


def _approval_summary(approval: Any | None, *, candidate_id: UUID) -> dict[str, Any]:
    if approval is None:
        return {"candidate_id": str(candidate_id)}
    return _json_safe(
        {
            "approval_id": str(_value(approval, "id", "")) or None,
            "candidate_id": str(candidate_id),
            "purpose": _value(approval, "purpose"),
            "state": _value(approval, "state"),
            "valid_until": _value(approval, "valid_until"),
            "evidence_summary": _value(approval, "evidence_summary", {}),
            "capital_context": _value(approval, "capital_context", {}),
            "risk_summary": _value(approval, "risk_summary", {}),
            "cost_summary": _value(approval, "cost_summary", {}),
            "capacity_summary": _value(approval, "capacity_summary", {}),
            "changes_summary": _value(approval, "changes_summary", {}),
        }
    )


def _candidate_lineage(candidate: Any) -> dict[str, Any]:
    fields = (
        "candidate_family_id",
        "portfolio_program_id",
        "mandate_version_id",
        "capital_context_version_id",
        "evaluation_episode_id",
        "policy_version",
        "risk_model_version",
        "cost_model_version",
        "capacity_model_version",
        "constraint_set_version",
        "rebalance_policy_version",
    )
    return _json_safe({name: _value(candidate, name) for name in fields})


def _runtime_payload(candidate: Any) -> tuple[dict[str, Any], StrategyArtifact, dict[str, Any]]:
    metrics = dict(_value(candidate, "metrics", {}) or {})
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
    required = {"experiment_id", "orders", "fills", "positions", "pnl", "statistics"}
    if not isinstance(evidence, dict) or not required.issubset(evidence):
        raise QfError(
            "CANDIDATE_NAUTILUS_EVIDENCE_INCOMPLETE",
            "Candidate evidence must include the canonical Nautilus transaction record.",
            422,
            {"required": sorted(required)},
        )
    return runtime, artifact, _json_safe(evidence)


def _custom_schema_files(raw: Any) -> dict[str, bytes]:
    schemas = raw if isinstance(raw, Mapping) else {}
    files: dict[str, bytes] = {}
    for name, schema in schemas.items():
        safe_name = PurePosixPath(str(name)).name
        if not safe_name.endswith(".json"):
            safe_name += ".json"
        files[f"data/custom-data-schemas/{safe_name}"] = _json_bytes(schema)
    files["data/custom-data-schemas/index.json"] = _json_bytes(
        {"schemas": sorted(path.rsplit("/", 1)[-1] for path in files)}
    )
    return files


def _persist_bundle(settings: Any, bundle_id: UUID, archive_bytes: bytes) -> str:
    relative_path = (Path(str(bundle_id)) / BUNDLE_FILENAME).as_posix()
    package_root = getattr(settings, "package_root", None)
    if package_root is None:
        return relative_path
    root = Path(package_root)
    final_dir = root / str(bundle_id)
    final_dir.mkdir(parents=True, exist_ok=False)
    final_path = final_dir / BUNDLE_FILENAME
    try:
        with tempfile.NamedTemporaryFile(dir=final_dir, delete=False) as stream:
            stream.write(archive_bytes)
            temporary = Path(stream.name)
        os.replace(temporary, final_path)
    except Exception:
        for child in final_dir.glob("*"):
            child.unlink(missing_ok=True)
        final_dir.rmdir()
        raise
    return relative_path


@dataclass(frozen=True, slots=True)
class BuiltCandidateBundle:
    archive_bytes: bytes
    manifest: dict[str, Any]
    validation_summary: dict[str, Any]
    relative_path: str
    operator_summary: dict[str, Any]

    @property
    def size_bytes(self) -> int:
        return len(self.archive_bytes)


def build_candidate_bundle(
    settings: Any,
    *,
    candidate: Any,
    approval: Any | None = None,
    downstream: Any | None = None,
    bundle_id: UUID | None = None,
) -> BuiltCandidateBundle:
    candidate_id = _value(candidate, "id")
    if not isinstance(candidate_id, UUID):
        raise QfError("CANDIDATE_INVALID", "Candidate id is required.", 422)
    runtime, artifact, evidence = _runtime_payload(candidate)
    simulation_experiment_id = _value(candidate, "simulation_experiment_id") or evidence.get(
        "experiment_id"
    )
    if not simulation_experiment_id:
        raise QfError(
            "CANDIDATE_SIMULATION_LINEAGE_MISSING",
            "Candidate must retain its portfolio-simulation experiment id.",
            422,
        )

    bundle_id = bundle_id or uuid4()
    target_weights = _member_payload(candidate)
    approval_summary = _approval_summary(approval, candidate_id=candidate_id)
    lineage = {
        "portfolio_simulation_experiment_id": str(simulation_experiment_id),
        "dataset_revision_ids": runtime.get("dataset_revision_ids", []),
        "alpha_qualification_ids": runtime.get("alpha_qualification_ids", []),
        "candidate": _candidate_lineage(candidate),
        "approval": approval_summary,
    }
    strategy_config = dict(artifact.config)
    actor_config = runtime.get("actor_config", {})
    data_requirements = runtime.get("data_requirements", {})
    instrument_scope = runtime.get("instrument_scope", [])
    backtest_run_config = runtime.get("backtest_run_config", {})
    venue_config = runtime.get("venue_config", {})
    risk_config = runtime.get("risk_config", {})
    live_node_template = runtime.get(
        "live_node_template",
        {
            "strategy_wheel": "strategy/strategy.whl",
            "strategy_config": "strategy/strategy-config.json",
            "actor_config": "strategy/actor-config.json",
            "broker_adapter": "DOWNSTREAM_RUNTIME_OWNED",
            "credentials": "INJECT_AT_REMOTE_RUNTIME_ONLY",
        },
    )
    discovery_summary = runtime.get("discovery_summary", runtime.get("discovery_evidence", []))
    sealed_summary = runtime.get("sealed_summary", runtime.get("sealed_disclosures", []))
    robustness_summary = runtime.get(
        "robustness_summary",
        dict(_value(candidate, "metrics", {}) or {}).get("robustness", {}),
    )
    fixture_catalog = runtime.get(
        "fixture_catalog",
        {
            "dataset_revision_ids": runtime.get("dataset_revision_ids", []),
            "catalog_uri": backtest_run_config.get("catalog_uri")
            or backtest_run_config.get("catalog_key"),
            "purpose": "CANDIDATE_CONFORMANCE",
        },
    )

    manifest: dict[str, Any] = _json_safe(
        {
            "contract": "QUAZONAI_NAUTILUS_CANDIDATE_BUNDLE",
            "contract_version": BUNDLE_CONTRACT_VERSION,
            "bundle_id": str(bundle_id),
            "candidate_id": str(candidate_id),
            "created_at": datetime.now(UTC).isoformat(),
            "purpose": _value(approval, "purpose") if approval is not None else None,
            "downstream_system_id": str(_value(downstream, "id", "")) or None,
            "runtime": {
                "name": "NAUTILUS_TRADER",
                "version": PINNED_NAUTILUS_VERSION,
                "deployment": "REMOTE_INDEPENDENT_RUNTIME",
                "paper_live_reuse": "SAME_STRATEGY_WHEEL_AND_CONFIG",
            },
            "strategy": {
                "artifact_id": artifact.artifact_id,
                "wheel": "strategy/strategy.whl",
                "strategy_path": artifact.strategy_path,
                "config_path": artifact.config_path,
                "strategy_config": "strategy/strategy-config.json",
                "actor_config": "strategy/actor-config.json",
            },
            "data": {
                "requirements": "data/requirements.json",
                "instrument_scope": "data/instrument-scope.json",
                "custom_data_schemas": "data/custom-data-schemas/",
            },
            "runtime_config": {
                "nautilus_version": "runtime/nautilus-version.json",
                "backtest_run": "runtime/backtest-run-config.json",
                "venue": "runtime/venue-config.json",
                "risk": "runtime/risk-config.json",
                "live_node_template": "runtime/live-node-template.json",
            },
            "validation": {
                "fixture_catalog": "validation/fixture-catalog/",
                "expected_orders": "validation/expected-orders.json",
                "expected_fills": "validation/expected-fills.json",
                "expected_positions": "validation/expected-positions.json",
                "expected_statistics": "validation/expected-statistics.json",
            },
            "evidence": {
                "discovery": "evidence/discovery-summary.json",
                "sealed": "evidence/sealed-summary.json",
                "robustness": "evidence/robustness-summary.json",
                "portfolio_simulation": "evidence/portfolio-simulation.json",
            },
            "lineage": "lineage.json",
            "target_weights": target_weights,
        }
    )

    files: dict[str, bytes] = {
        "manifest.json": _json_bytes(manifest),
        "requirements.lock": f"nautilus_trader=={PINNED_NAUTILUS_VERSION}\n".encode(),
        "strategy/strategy.whl": _strategy_wheel(artifact, candidate_id=candidate_id),
        "strategy/strategy-config.json": _json_bytes(strategy_config),
        "strategy/actor-config.json": _json_bytes(actor_config),
        "data/requirements.json": _json_bytes(data_requirements),
        "data/instrument-scope.json": _json_bytes(instrument_scope),
        "runtime/nautilus-version.json": _json_bytes(
            {"name": "nautilus_trader", "version": PINNED_NAUTILUS_VERSION}
        ),
        "runtime/backtest-run-config.json": _json_bytes(backtest_run_config),
        "runtime/venue-config.json": _json_bytes(venue_config),
        "runtime/risk-config.json": _json_bytes(risk_config),
        "runtime/live-node-template.json": _json_bytes(live_node_template),
        "validation/fixture-catalog/manifest.json": _json_bytes(fixture_catalog),
        "validation/expected-orders.json": _json_bytes(evidence["orders"]),
        "validation/expected-positions.json": _json_bytes(evidence["positions"]),
        "validation/expected-statistics.json": _json_bytes(evidence["statistics"]),
        "validation/expected-fills.json": _json_bytes(evidence["fills"]),
        "evidence/discovery-summary.json": _json_bytes(discovery_summary),
        "evidence/sealed-summary.json": _json_bytes(sealed_summary),
        "evidence/robustness-summary.json": _json_bytes(robustness_summary),
        "evidence/portfolio-simulation.json": _json_bytes(evidence),
        "lineage.json": _json_bytes(lineage),
    }
    files.update(_custom_schema_files(runtime.get("custom_data_schemas", {})))

    stream = io.BytesIO()
    with zipfile.ZipFile(stream, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for path, payload in sorted(files.items()):
            archive.writestr(path, payload)
    archive_bytes = stream.getvalue()
    validation_summary = _json_safe(validate_candidate_bundle(archive_bytes))
    if not validation_summary["valid"]:
        raise QfError(
            "CANDIDATE_BUNDLE_INVALID",
            "Generated Candidate Bundle failed structural validation.",
            500,
            validation_summary,
        )
    relative_path = _persist_bundle(settings, bundle_id, archive_bytes)
    return BuiltCandidateBundle(
        archive_bytes=archive_bytes,
        manifest=manifest,
        validation_summary=validation_summary,
        relative_path=relative_path,
        operator_summary=approval_summary,
    )


def build_candidate_verification_request(
    built: BuiltCandidateBundle, *, candidate_id: UUID
) -> CandidateVerificationRequest:
    """Extract the exact wheel and deterministic reference replay contract."""
    with zipfile.ZipFile(io.BytesIO(built.archive_bytes)) as archive:
        lineage = json.loads(archive.read("lineage.json"))
        dataset_revision_ids = list(lineage.get("dataset_revision_ids", []))
        if len(dataset_revision_ids) != 1:
            raise QfError(
                "CANDIDATE_CONFORMANCE_LINEAGE_INVALID",
                "Candidate conformance requires exactly one portfolio-simulation Dataset Revision.",
                500,
                {"dataset_revision_ids": dataset_revision_ids},
            )
        fixture = {
            "dataset_revision_id": dataset_revision_ids[0],
            "strategy_config": json.loads(archive.read("strategy/strategy-config.json")),
            "instrument_scope": json.loads(archive.read("data/instrument-scope.json")),
            "backtest_run_config": json.loads(archive.read("runtime/backtest-run-config.json")),
            "venue_config": json.loads(archive.read("runtime/venue-config.json")),
            "risk_config": json.loads(archive.read("runtime/risk-config.json")),
            "orders": json.loads(archive.read("validation/expected-orders.json")),
            "fills": json.loads(archive.read("validation/expected-fills.json")),
            "positions": json.loads(archive.read("validation/expected-positions.json")),
            "statistics": json.loads(archive.read("validation/expected-statistics.json")),
        }
        wheel = archive.read("strategy/strategy.whl")
    return CandidateVerificationRequest(
        candidate_id=candidate_id,
        manifest=built.manifest,
        strategy_wheel_b64=base64.b64encode(wheel).decode("ascii"),
        fixture=fixture,
    )

def _has_secret_value(value: Any, *, key: str = "") -> bool:
    normalized = key.casefold().replace("-", "_")
    secret_key = any(
        marker in normalized
        for marker in (
            "password",
            "private_key",
            "api_key",
            "secret",
            "credential",
            "access_token",
            "auth_token",
            "broker_token",
            "broker_secret",
        )
    )
    if secret_key and value not in (None, "", "INJECT_AT_REMOTE_RUNTIME_ONLY"):
        return True
    if isinstance(value, Mapping):
        return any(
            _has_secret_value(child, key=str(child_key)) for child_key, child in value.items()
        )
    if isinstance(value, list):
        return any(_has_secret_value(child, key=key) for child in value)
    return False


def validate_candidate_bundle(archive_bytes: bytes) -> dict[str, Any]:
    required = {
        "manifest.json",
        "requirements.lock",
        "strategy/strategy.whl",
        "strategy/strategy-config.json",
        "strategy/actor-config.json",
        "data/requirements.json",
        "data/instrument-scope.json",
        "data/custom-data-schemas/index.json",
        "runtime/nautilus-version.json",
        "runtime/backtest-run-config.json",
        "runtime/venue-config.json",
        "runtime/risk-config.json",
        "runtime/live-node-template.json",
        "validation/fixture-catalog/manifest.json",
        "validation/expected-orders.json",
        "validation/expected-fills.json",
        "validation/expected-positions.json",
        "validation/expected-statistics.json",
        "evidence/discovery-summary.json",
        "evidence/sealed-summary.json",
        "evidence/robustness-summary.json",
        "lineage.json",
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
            if "manifest.json" in names:
                manifest = json.loads(archive.read("manifest.json"))
                if manifest.get("contract_version") != BUNDLE_CONTRACT_VERSION:
                    findings.append({"code": "CONTRACT_VERSION_INVALID"})
                if manifest.get("runtime", {}).get("version") != PINNED_NAUTILUS_VERSION:
                    findings.append({"code": "NAUTILUS_VERSION_INVALID"})
                if manifest.get("strategy", {}).get("wheel") != "strategy/strategy.whl":
                    findings.append({"code": "STRATEGY_WHEEL_PATH_INVALID"})
            if "requirements.lock" in names:
                lock = archive.read("requirements.lock").decode("utf-8").strip()
                if lock != f"nautilus_trader=={PINNED_NAUTILUS_VERSION}":
                    findings.append({"code": "NAUTILUS_LOCK_INVALID"})
            for name in sorted(names):
                if not name.endswith(".json"):
                    continue
                payload = json.loads(archive.read(name))
                if _has_secret_value(payload):
                    findings.append({"code": "LIVE_SECRET_PRESENT", "path": name})
    except (json.JSONDecodeError, UnicodeDecodeError, zipfile.BadZipFile) as exc:
        findings.append({"code": "BUNDLE_ARCHIVE_INVALID", "detail": str(exc)})
    return {
        "valid": not findings,
        "contract": "QUAZONAI_NAUTILUS_CANDIDATE_BUNDLE",
        "contract_version": BUNDLE_CONTRACT_VERSION,
        "findings": findings,
    }


def resolve_bundle_archive(settings: Any, relative_path: str) -> Path:
    root = Path(settings.package_root).resolve()
    candidate = (root / relative_path).resolve()
    if not candidate.is_relative_to(root):
        raise QfError("CANDIDATE_BUNDLE_PATH_INVALID", "Candidate Bundle path is invalid.", 500)
    if not candidate.is_file():
        raise QfError("CANDIDATE_BUNDLE_MISSING", "Candidate Bundle artifact is missing.", 500)
    return candidate
