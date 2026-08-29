"""Build immutable Nautilus-native Candidate Bundle artifacts."""

from __future__ import annotations

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
from quant_runtime.config import PINNED_NAUTILUS_VERSION, RemoteNautilusConfig
from quant_runtime.contracts import RunEvidence, StrategyArtifact
from quant_runtime.remote import NautilusQuantRuntime
from settings import Settings


_FORBIDDEN_SECRET_FIELDS = {
    "api_key",
    "apikey",
    "private_key",
    "secret_key",
    "service_token",
    "access_token",
    "refresh_token",
    "account_password",
    "wallet_seed",
    "broker_url",
}


@dataclass(frozen=True, slots=True)
class BuiltCandidatePackage:
    manifest: dict[str, Any]
    relative_path: str
    operator_summary: dict[str, Any]


def _json_bytes(value: Any) -> bytes:
    return json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True, default=str).encode(
        "utf-8"
    )


def _write_json(path: Path, value: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(_json_bytes(value))


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
            raise QfError(
                "CANDIDATE_BUNDLE_INVALID",
                "Every Candidate member must identify a Nautilus instrument.",
                422,
                {"member_index": index},
            )
        try:
            weight = float(str(member.get("target_weight", member.get("weight"))))
        except (TypeError, ValueError) as exc:
            raise QfError(
                "CANDIDATE_BUNDLE_INVALID",
                "Candidate target weights must be numeric.",
                422,
                {"member_index": index},
            ) from exc
        rows.append(
            {
                "instrument_id": str(instrument),
                "target_weight": weight,
                "alpha_qualification_id": member.get("alpha_qualification_id"),
            }
        )
    if not rows:
        raise QfError(
            "CANDIDATE_BUNDLE_INVALID",
            "A Nautilus Candidate Bundle requires at least one target instrument.",
            422,
        )
    return rows


def _reject_secret_fields(value: object, path: str = "$") -> None:
    if isinstance(value, dict):
        for key, item in value.items():
            normalized = str(key).casefold()
            if normalized in _FORBIDDEN_SECRET_FIELDS or normalized.endswith("_secret"):
                raise QfError(
                    "CANDIDATE_BUNDLE_CONTAINS_SECRET",
                    "Candidate Bundle data contains a forbidden execution credential field.",
                    422,
                    {"path": f"{path}.{key}"},
                )
            _reject_secret_fields(item, f"{path}.{key}")
    elif isinstance(value, list):
        for index, item in enumerate(value):
            _reject_secret_fields(item, f"{path}[{index}]")


def _without_runtime_account_data(value: object) -> object:
    """Keep validation reports useful without exporting runtime account data."""
    if isinstance(value, dict):
        return {
            str(key): _without_runtime_account_data(item)
            for key, item in value.items()
            if (
                str(key).casefold() not in {"account", "account_id"}
                and not str(key).casefold().startswith("account.")
            )
        }
    if isinstance(value, list):
        return [_without_runtime_account_data(item) for item in value]
    return value


def _runtime_payload(
    candidate: PortfolioCandidate,
) -> tuple[StrategyArtifact, RunEvidence, dict[str, Any]]:
    raw = candidate.metrics.get("nautilus")
    if not isinstance(raw, dict):
        raise QfError(
            "CANDIDATE_BUNDLE_EVIDENCE_MISSING",
            "Candidate is not backed by Nautilus runtime evidence.",
            422,
        )
    try:
        artifact = StrategyArtifact.model_validate(raw["strategy_artifact"])
        portfolio_evidence = RunEvidence.model_validate(raw["portfolio_evidence"])
    except (KeyError, ValueError) as exc:
        raise QfError(
            "CANDIDATE_BUNDLE_EVIDENCE_INVALID",
            "Candidate Nautilus evidence is incomplete or invalid.",
            422,
        ) from exc
    if portfolio_evidence.state != "SUCCEEDED" or portfolio_evidence.mode != "PORTFOLIO":
        raise QfError(
            "CANDIDATE_BUNDLE_EVIDENCE_INVALID",
            "Candidate requires a successful Nautilus PORTFOLIO simulation.",
            422,
        )
    if portfolio_evidence.nautilus_version != PINNED_NAUTILUS_VERSION:
        raise QfError(
            "CANDIDATE_BUNDLE_RUNTIME_MISMATCH",
            "Candidate evidence was not produced by the pinned NautilusTrader version.",
            422,
            {
                "expected": PINNED_NAUTILUS_VERSION,
                "actual": portfolio_evidence.nautilus_version,
            },
        )
    _reject_secret_fields(raw)
    return artifact, portfolio_evidence, raw


def _requirements(artifact: StrategyArtifact) -> list[str]:
    result = [f"nautilus-trader=={PINNED_NAUTILUS_VERSION}"]
    for requirement in artifact.requirements:
        clean = requirement.strip()
        if not clean:
            continue
        lower = clean.casefold()
        if lower.startswith("nautilus-trader") and clean != result[0]:
            raise QfError(
                "CANDIDATE_BUNDLE_RUNTIME_MISMATCH",
                "Strategy artifact requests a NautilusTrader version other than the pinned version.",
                422,
                {"requirement": clean},
            )
        if "://" in clean or clean.startswith(("git+", "-e ", "--editable")):
            raise QfError(
                "CANDIDATE_BUNDLE_REQUIREMENT_INVALID",
                "Candidate Bundle requirements must be pinned package requirements, not URLs.",
                422,
                {"requirement": clean},
            )
        if clean not in result:
            result.append(clean)
    return result


def _write_strategy_wheel(path: Path, artifact: StrategyArtifact) -> None:
    distribution = "quazonai_nautilus_candidate"
    version = "1.0.0"
    dist_info = f"{distribution}-{version}.dist-info"
    metadata_path = f"{dist_info}/METADATA"
    wheel_path = f"{dist_info}/WHEEL"
    record_path = f"{dist_info}/RECORD"
    source_paths = sorted(artifact.source_files)
    record_lines = [f"{name},," for name in source_paths]
    record_lines.extend([metadata_path + ",,", wheel_path + ",,", record_path + ",,", ""])
    metadata = (
        "Metadata-Version: 2.4\n"
        "Name: quazonai-nautilus-candidate\n"
        f"Version: {version}\n"
        "Summary: Frozen NautilusTrader strategy artifact for one approved Candidate\n"
        f"Requires-Dist: nautilus-trader (=={PINNED_NAUTILUS_VERSION})\n"
    )
    wheel = "Wheel-Version: 1.0\nGenerator: QuaZonai\nRoot-Is-Purelib: true\nTag: py3-none-any\n"
    path.parent.mkdir(parents=True, exist_ok=True)
    with zipfile.ZipFile(path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
        for source_path in source_paths:
            archive.writestr(source_path, artifact.source_files[source_path])
        archive.writestr(metadata_path, metadata)
        archive.writestr(wheel_path, wheel)
        archive.writestr(record_path, "\n".join(record_lines))


def _bundle_manifest(
    *,
    approval: ApprovalSnapshot,
    candidate: PortfolioCandidate,
    downstream: DownstreamSystem,
    artifact: StrategyArtifact,
    evidence: RunEvidence,
) -> dict[str, Any]:
    return {
        "candidate_bundle_contract_version": downstream.package_contract_version,
        "candidate_id": str(candidate.id),
        "approval_id": str(approval.id),
        "purpose": approval.purpose,
        "canonical_runtime": {
            "name": "NautilusTrader",
            "version": PINNED_NAUTILUS_VERSION,
            "quant_contract_version": evidence.contract_version,
        },
        "strategy": {
            "wheel": "strategy/strategy.whl",
            "strategy_config": "strategy/strategy-config.json",
            "actor_config": "strategy/actor-config.json",
            "strategy_path": artifact.strategy_path,
            "config_path": artifact.config_path,
        },
        "data": {
            "requirements": "data/requirements.json",
            "instrument_scope": "data/instrument-scope.json",
            "custom_data_schemas": "data/custom-data-schemas/",
        },
        "runtime": {
            "nautilus_version": "runtime/nautilus-version.json",
            "backtest_run_config": "runtime/backtest-run-config.json",
            "venue_config": "runtime/venue-config.json",
            "risk_config": "runtime/risk-config.json",
            "live_node_template": "runtime/live-node-template.json",
        },
        "validation": {
            "fixture_catalog": "validation/fixture-catalog/",
            "expected_orders": "validation/expected-orders.json",
            "expected_positions": "validation/expected-positions.json",
            "expected_statistics": "validation/expected-statistics.json",
        },
        "evidence": {
            "discovery": "evidence/discovery-summary.json",
            "sealed": "evidence/sealed-summary.json",
            "robustness": "evidence/robustness-summary.json",
        },
        "lineage": "lineage.json",
        "requirements_lock": "requirements.lock",
        "same_strategy_artifact_for_backtest_paper_live": True,
        "execution_secret_material": "excluded",
    }


def build_candidate_package(
    settings: Settings,
    *,
    approval: ApprovalSnapshot,
    candidate: PortfolioCandidate,
    downstream: DownstreamSystem,
) -> BuiltCandidatePackage:
    """Freeze an approved Candidate into a Nautilus-native, remotely verified bundle."""
    rows = _member_rows(candidate)
    artifact, portfolio_evidence, runtime_data = _runtime_payload(candidate)
    requirements = _requirements(artifact)
    package_id = uuid4()
    staging = settings.package_root / "staging" / str(package_id)
    final_root = settings.package_root / str(package_id)
    archive_name = "candidate-bundle.zip"
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

        _write_strategy_wheel(staging / "strategy" / "strategy.whl", artifact)
        _write_json(
            staging / "strategy" / "strategy-config.json",
            {
                "strategy_path": artifact.strategy_path,
                "config_path": artifact.config_path,
                "config": artifact.config,
            },
        )
        _write_json(staging / "strategy" / "actor-config.json", {"actors": []})
        (staging / "requirements.lock").write_text(
            "\n".join(requirements) + "\n",
            encoding="utf-8",
        )

        _write_json(
            staging / "data" / "requirements.json",
            {
                "catalog_uri": portfolio_evidence.catalog_uri,
                "nautilus_data_types": ["Bar"],
                "point_in_time_required": True,
                "license_governance": "QuaZonai Dataset Revision",
            },
        )
        _write_json(staging / "data" / "instrument-scope.json", {"instruments": rows})
        (staging / "data" / "custom-data-schemas" / ".keep").write_text(
            "",
            encoding="utf-8",
        )

        _write_json(
            staging / "runtime" / "nautilus-version.json",
            {
                "package": "nautilus-trader",
                "version": PINNED_NAUTILUS_VERSION,
                "quant_contract_version": portfolio_evidence.contract_version,
            },
        )
        _write_json(
            staging / "runtime" / "backtest-run-config.json",
            {
                "strategy_path": artifact.strategy_path,
                "config_path": artifact.config_path,
                "strategy_config": artifact.config,
                "catalog_uri": portfolio_evidence.catalog_uri,
            },
        )
        _write_json(
            staging / "runtime" / "venue-config.json",
            {
                "source": "frozen remote Nautilus PORTFOLIO run",
                "mode": "simulation",
                "venue_semantics_owned_by": "NautilusTrader",
            },
        )
        _write_json(
            staging / "runtime" / "risk-config.json",
            {
                "execution_risk_engine": "NautilusTrader RiskEngine",
                "research_promotion_risk": "QuaZonai governance",
            },
        )
        _write_json(
            staging / "runtime" / "live-node-template.json",
            {
                "strategy_wheel": "strategy/strategy.whl",
                "strategy_config": "strategy/strategy-config.json",
                "runtime": f"nautilus-trader=={PINNED_NAUTILUS_VERSION}",
                "environment": "PAPER_OR_LIVE_DOWNSTREAM",
                "secret_source": "downstream-owned secret store",
                "control_owner": "downstream Nautilus runtime",
            },
        )

        _write_json(
            staging / "validation" / "fixture-catalog" / "catalog-descriptor.json",
            {
                "catalog_uri": portfolio_evidence.catalog_uri,
                "purpose": "candidate conformance fixture reference",
            },
        )
        _write_json(
            staging / "validation" / "expected-orders.json",
            _without_runtime_account_data(portfolio_evidence.orders),
        )
        _write_json(
            staging / "validation" / "expected-positions.json",
            _without_runtime_account_data(portfolio_evidence.positions),
        )
        _write_json(
            staging / "validation" / "expected-statistics.json",
            _without_runtime_account_data(portfolio_evidence.statistics),
        )

        _write_json(
            staging / "evidence" / "discovery-summary.json",
            {
                "run_id": runtime_data.get("discovery_run_id"),
                "source": "remote Nautilus discovery evidence",
            },
        )
        _write_json(
            staging / "evidence" / "sealed-summary.json",
            {
                "run_id": runtime_data.get("sealed_run_id"),
                "statistics": candidate.metrics.get("sealed_statistics", {}),
                "source": "independent remote Nautilus sealed evaluator",
            },
        )
        _write_json(
            staging / "evidence" / "robustness-summary.json",
            {
                "portfolio_run_id": runtime_data.get("portfolio_run_id"),
                "transaction_level_simulation": True,
                "orders": len(portfolio_evidence.orders),
                "fills": len(portfolio_evidence.fills),
                "positions": len(portfolio_evidence.positions),
            },
        )

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
        lineage = {
            "candidate_id": str(candidate.id),
            "candidate_family_id": (
                str(candidate.candidate_family_id) if candidate.candidate_family_id else None
            ),
            "portfolio_program_id": str(candidate.portfolio_program_id),
            "mandate_version_id": (
                str(candidate.mandate_version_id) if candidate.mandate_version_id else None
            ),
            "capital_context_version_id": (
                str(candidate.capital_context_version_id)
                if candidate.capital_context_version_id
                else None
            ),
            "evaluation_episode_id": (
                str(candidate.evaluation_episode_id) if candidate.evaluation_episode_id else None
            ),
            "discovery_run_id": runtime_data.get("discovery_run_id"),
            "sealed_run_id": runtime_data.get("sealed_run_id"),
            "portfolio_run_id": runtime_data.get("portfolio_run_id"),
            "policy_version": candidate.policy_version,
            "risk_model_version": candidate.risk_model_version,
            "cost_model_version": candidate.cost_model_version,
            "capacity_model_version": candidate.capacity_model_version,
            "constraint_set_version": candidate.constraint_set_version,
            "rebalance_policy_version": candidate.rebalance_policy_version,
        }
        _write_json(staging / "lineage.json", lineage)

        manifest = _bundle_manifest(
            approval=approval,
            candidate=candidate,
            downstream=downstream,
            artifact=artifact,
            evidence=portfolio_evidence,
        )
        _reject_secret_fields(manifest)
        _write_json(staging / "manifest.json", manifest)

        archive_path = staging / archive_name
        with zipfile.ZipFile(archive_path, "w", compression=zipfile.ZIP_DEFLATED) as archive:
            for path in sorted(staging.rglob("*")):
                if path.is_file() and path != archive_path:
                    archive.write(path, path.relative_to(staging).as_posix())

        remote_config = RemoteNautilusConfig.from_env(
            required=settings.environment == "production"
        )
        verification: dict[str, Any] | None = None
        if remote_config is not None:
            verification = NautilusQuantRuntime(remote_config).verify_candidate(archive_path)

        final_root.parent.mkdir(parents=True, exist_ok=True)
        os.replace(staging, final_root)
        return BuiltCandidatePackage(
            manifest=manifest,
            relative_path=(Path(str(package_id)) / archive_name).as_posix(),
            operator_summary={
                **approval_summary,
                "remote_nautilus_conformance": verification,
            },
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
