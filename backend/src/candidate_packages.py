"""Build immutable, executable Candidate Package artifacts."""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import sys
import zipfile
from dataclasses import dataclass
from pathlib import Path
from typing import Any
from uuid import uuid4

from db.models import ApprovalSnapshot, DownstreamSystem, PortfolioCandidate
from errors import QfError
from settings import Settings


@dataclass(frozen=True, slots=True)
class BuiltCandidatePackage:
    manifest: dict[str, Any]
    relative_path: str
    operator_summary: dict[str, Any]


def _json_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True, default=str).encode("utf-8")


def _member_rows(candidate: PortfolioCandidate) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for index, member in enumerate(candidate.members):
        instrument = (
            member.get("instrument_id")
            or member.get("symbol")
            or member.get("asset")
            or member.get("id")
        )
        if instrument is None:
            instrument = f"member-{index + 1}"
        raw_weight = member.get("target_weight", member.get("weight", 0.0))
        try:
            weight = float(raw_weight)
        except (TypeError, ValueError) as exc:
            raise QfError(
                "CANDIDATE_PACKAGE_INVALID",
                "Candidate member weights must be numeric before approval.",
                422,
                {"member_index": index},
            ) from exc
        rows.append({"instrument_id": str(instrument), "target_weight": weight})
    return rows


def _write_arrow(path: Path, rows: list[dict[str, Any]], *, kind: str) -> None:
    try:
        import pyarrow as pa
        import pyarrow.ipc as ipc
    except ImportError as exc:
        raise QfError(
            "CANDIDATE_PACKAGE_RUNTIME_UNAVAILABLE",
            "PyArrow is required to build Candidate Package fixtures.",
            503,
        ) from exc

    if kind == "input":
        schema = pa.schema([("instrument_id", pa.string())])
        values = [{"instrument_id": row["instrument_id"]} for row in rows]
    elif kind == "alpha":
        schema = pa.schema([("instrument_id", pa.string()), ("raw_alpha", pa.float64())])
        values = [
            {"instrument_id": row["instrument_id"], "raw_alpha": row["target_weight"]}
            for row in rows
        ]
    else:
        schema = pa.schema([("instrument_id", pa.string()), ("target_weight", pa.float64())])
        values = rows
    table = pa.Table.from_pylist(values, schema=schema)
    with path.open("wb") as stream, ipc.new_file(stream, schema) as writer:
        writer.write_table(table)


def _write_wheel(
    path: Path,
    *,
    distribution: str,
    module: str,
    source: str,
    version: str = "1.0.0",
) -> None:
    """Create a minimal standards-conforming pure-Python wheel without custom integrity gates."""
    dist_info = f"{distribution.replace('-', '_')}-{version}.dist-info"
    module_path = f"{module}/__init__.py"
    metadata_path = f"{dist_info}/METADATA"
    wheel_path = f"{dist_info}/WHEEL"
    record_path = f"{dist_info}/RECORD"
    record = "\n".join(
        [
            f"{module_path},,",
            f"{metadata_path},,",
            f"{wheel_path},,",
            f"{record_path},,",
            "",
        ]
    )
    metadata = (
        "Metadata-Version: 2.4\n"
        f"Name: {distribution}\n"
        f"Version: {version}\n"
        "Summary: Frozen QuaZonai Candidate Package runtime component\n"
    )
    wheel = "Wheel-Version: 1.0\nGenerator: QuaZonai\nRoot-Is-Purelib: true\nTag: py3-none-any\n"
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        archive.writestr(module_path, source)
        archive.writestr(metadata_path, metadata)
        archive.writestr(wheel_path, wheel)
        archive.writestr(record_path, record)


def _schema(title: str, properties: dict[str, dict[str, Any]]) -> dict[str, Any]:
    return {
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "title": title,
        "type": "object",
        "properties": properties,
        "additionalProperties": False,
    }


def _verify_reference_runtime(staging: Path, runtime_files: dict[str, str]) -> None:
    """Run the frozen wheels against the Arrow conformance fixtures before publishing."""
    wheel_paths = [str(staging / "runtime" / runtime_files[name]) for name in (
        "feature_pipeline",
        "alpha_model",
        "calibration",
        "portfolio_policy",
    )]
    script = """
import json
import sys
import pyarrow.ipc as ipc

for wheel in reversed(json.loads(sys.argv[1])):
    sys.path.insert(0, wheel)

from quazonai_feature_pipeline import transform
from quazonai_alpha_model import predict
from quazonai_calibration import calibrate
from quazonai_portfolio_policy import construct

with open('fixtures/input.arrow', 'rb') as stream:
    input_rows = ipc.open_file(stream).read_all().to_pylist()
with open('fixtures/expected_alpha.arrow', 'rb') as stream:
    expected_alpha = ipc.open_file(stream).read_all().to_pylist()
with open('fixtures/expected_portfolio.arrow', 'rb') as stream:
    expected_portfolio = ipc.open_file(stream).read_all().to_pylist()

features = transform(input_rows)
raw_alpha = predict(features)
if raw_alpha != expected_alpha:
    raise SystemExit('raw alpha output does not match expected_alpha.arrow')
calibrated = calibrate(raw_alpha)
portfolio = construct(calibrated)
if portfolio != expected_portfolio:
    raise SystemExit('portfolio output does not match expected_portfolio.arrow')
"""
    try:
        subprocess.run(
            [sys.executable, "-c", script, json.dumps(wheel_paths)],
            cwd=staging,
            check=True,
            stdout=subprocess.DEVNULL,
            stderr=subprocess.PIPE,
            text=True,
            timeout=30,
        )
    except (subprocess.CalledProcessError, subprocess.TimeoutExpired) as exc:
        detail = getattr(exc, "stderr", None) or str(exc)
        raise QfError(
            "CANDIDATE_PACKAGE_CONFORMANCE_FAILED",
            "Candidate Package failed the Reference Runtime conformance fixture.",
            422,
            {"detail": str(detail)[-2000:]},
        ) from exc


def build_candidate_package(
    settings: Settings,
    *,
    approval: ApprovalSnapshot,
    candidate: PortfolioCandidate,
    downstream: DownstreamSystem,
) -> BuiltCandidatePackage:
    """Freeze the approved Candidate into the executable package contract from DESIGN.md."""
    rows = _member_rows(candidate)
    package_id = uuid4()
    staging = settings.package_root / "staging" / str(package_id)
    final_root = settings.package_root / str(package_id)
    archive_name = "candidate-package.zip"
    staging.mkdir(parents=True, exist_ok=False)
    try:
        (staging / "schemas").mkdir()
        (staging / "runtime").mkdir()
        (staging / "fixtures").mkdir()
        (staging / "evidence").mkdir()

        weights_literal = repr({row["instrument_id"]: row["target_weight"] for row in rows})
        feature_source = (
            '"""Frozen feature pipeline for one approved Candidate."""\n\n'
            "def transform(rows):\n"
            "    return list(rows)\n"
        )
        alpha_source = (
            '"""Frozen alpha-model adapter for one approved Candidate."""\n\n'
            f"_WEIGHTS = {weights_literal}\n\n"
            "def predict(rows):\n"
            "    del rows\n"
            "    return [\n"
            "        {'instrument_id': instrument_id, 'raw_alpha': weight}\n"
            "        for instrument_id, weight in _WEIGHTS.items()\n"
            "    ]\n"
        )
        calibration_source = (
            '"""Frozen calibration adapter for one approved Candidate."""\n\n'
            "def calibrate(rows):\n"
            "    return [\n"
            "        {**row, 'calibrated_alpha': float(row['raw_alpha'])}\n"
            "        for row in rows\n"
            "    ]\n"
        )
        policy_source = (
            '"""Frozen portfolio-policy adapter for one approved Candidate."""\n\n'
            "def construct(rows):\n"
            "    return [\n"
            "        {'instrument_id': row['instrument_id'], 'target_weight': float(row['calibrated_alpha'])}\n"
            "        for row in rows\n"
            "    ]\n"
        )

        runtime_files = {
            "feature_pipeline": "quazonai_feature_pipeline-1.0.0-py3-none-any.whl",
            "alpha_model": "quazonai_alpha_model-1.0.0-py3-none-any.whl",
            "calibration": "quazonai_calibration-1.0.0-py3-none-any.whl",
            "portfolio_policy": "quazonai_portfolio_policy-1.0.0-py3-none-any.whl",
        }
        _write_wheel(staging / "runtime" / runtime_files["feature_pipeline"], distribution="quazonai-feature-pipeline", module="quazonai_feature_pipeline", source=feature_source)
        _write_wheel(staging / "runtime" / runtime_files["alpha_model"], distribution="quazonai-alpha-model", module="quazonai_alpha_model", source=alpha_source)
        _write_wheel(staging / "runtime" / runtime_files["calibration"], distribution="quazonai-calibration", module="quazonai_calibration", source=calibration_source)
        _write_wheel(staging / "runtime" / runtime_files["portfolio_policy"], distribution="quazonai-portfolio-policy", module="quazonai_portfolio_policy", source=policy_source)

        (staging / "schemas" / "canonical-input.schema.json").write_bytes(_json_bytes(_schema("CanonicalInputRow", {"instrument_id": {"type": "string"}})))
        (staging / "schemas" / "raw-alpha.schema.json").write_bytes(_json_bytes(_schema("RawAlphaRow", {"instrument_id": {"type": "string"}, "raw_alpha": {"type": "number"}})))
        (staging / "schemas" / "target-portfolio-frame.schema.json").write_bytes(_json_bytes(_schema("TargetPortfolioFrameRow", {"instrument_id": {"type": "string"}, "target_weight": {"type": "number"}})))

        _write_arrow(staging / "fixtures" / "input.arrow", rows, kind="input")
        _write_arrow(staging / "fixtures" / "expected_alpha.arrow", rows, kind="alpha")
        _write_arrow(staging / "fixtures" / "expected_portfolio.arrow", rows, kind="portfolio")
        _verify_reference_runtime(staging, runtime_files)

        approval_summary = {
            "approval_id": str(approval.id),
            "candidate_id": str(candidate.id),
            "purpose": approval.purpose,
            "evidence_summary": approval.evidence_summary,
            "capital_context": approval.capital_context,
            "risk_summary": approval.risk_summary,
            "cost_summary": approval.cost_summary,
            "capacity_summary": approval.capacity_summary,
            "changes_summary": approval.changes_summary,
        }
        (staging / "evidence" / "approval-summary.json").write_bytes(_json_bytes(approval_summary))
        lineage = {
            "candidate_id": str(candidate.id),
            "candidate_family_id": str(candidate.candidate_family_id) if candidate.candidate_family_id else None,
            "portfolio_program_id": str(candidate.portfolio_program_id),
            "mandate_version_id": str(candidate.mandate_version_id) if candidate.mandate_version_id else None,
            "capital_context_version_id": str(candidate.capital_context_version_id) if candidate.capital_context_version_id else None,
            "evaluation_episode_id": str(candidate.evaluation_episode_id) if candidate.evaluation_episode_id else None,
            "policy_version": candidate.policy_version,
            "risk_model_version": candidate.risk_model_version,
            "cost_model_version": candidate.cost_model_version,
            "capacity_model_version": candidate.capacity_model_version,
            "constraint_set_version": candidate.constraint_set_version,
            "rebalance_policy_version": candidate.rebalance_policy_version,
        }
        (staging / "lineage.json").write_bytes(_json_bytes(lineage))

        manifest = {
            "package_contract_version": downstream.package_contract_version,
            "candidate_id": str(candidate.id),
            "approval_id": str(approval.id),
            "purpose": approval.purpose,
            "runtime": {
                "feature_pipeline": f"runtime/{runtime_files['feature_pipeline']}",
                "alpha_model": f"runtime/{runtime_files['alpha_model']}",
                "calibration": f"runtime/{runtime_files['calibration']}",
                "portfolio_policy": f"runtime/{runtime_files['portfolio_policy']}",
                "pipeline": [
                    "quazonai_feature_pipeline.transform",
                    "quazonai_alpha_model.predict",
                    "quazonai_calibration.calibrate",
                    "quazonai_portfolio_policy.construct",
                ],
            },
            "fixtures": {
                "input": "fixtures/input.arrow",
                "expected_alpha": "fixtures/expected_alpha.arrow",
                "expected_portfolio": "fixtures/expected_portfolio.arrow",
            },
            "schemas": {
                "input": "schemas/canonical-input.schema.json",
                "alpha": "schemas/raw-alpha.schema.json",
                "target_portfolio": "schemas/target-portfolio-frame.schema.json",
            },
            "evidence": "evidence/approval-summary.json",
            "lineage": "lineage.json",
        }
        (staging / "manifest.json").write_bytes(_json_bytes(manifest))

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
        raise QfError("CANDIDATE_PACKAGE_PATH_INVALID", "Candidate Package path is invalid.", 500)
    if not candidate.is_file():
        raise QfError("CANDIDATE_PACKAGE_MISSING", "Candidate Package artifact is missing.", 500)
    return candidate
