"""Mission workspace bridge for bounded, evidence-driven Nautilus research rounds."""

from __future__ import annotations

import json
import os
import stat
from pathlib import Path
from typing import Any
from uuid import UUID

from pydantic import ValidationError
from sqlalchemy import select

from db.models import (
    DatasetRevision,
    GovernedDataSource,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
    SearchLedgerEntry,
)
from db.session import create_database_engine, create_session_factory
from errors import QfError
from quant_runtime.contracts import BacktestExperimentRequest, ExperimentMode
from quant_runtime.data_scope import dataset_revision_domains, normalize_data_domain
from quant_runtime.ledger import (
    ExperimentCoordinator,
    write_evidence,
    write_workspace_json,
)
from settings import Settings

MAX_EXPERIMENTS_PER_ROUND = 20
CATALOG_URI_PREFIX = "nautilus-catalog://"



def prepare_experiment_workspace(
    settings: Settings,
    *,
    workspace: Path,
    mission_id: UUID,
) -> int:
    """Publish only governed Discovery datasets and the executable contract to a Mission.

    The files contain no database credentials or remote-runtime token. The child
    Agent can propose source-bundle strategies and experiment requests, while the
    parent worker remains the only process able to validate and execute them.
    """
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    try:
        with factory() as session:
            mission = session.get(ResearchMission, mission_id)
            program = session.get(ResearchProgram, mission.program_id) if mission is not None else None
            charter = session.get(ResearchCharter, program.charter_id) if program is not None else None
            if mission is None or program is None or charter is None:
                raise QfError(
                    "MISSION_CHARTER_MISSING",
                    "Mission cannot publish datasets without its frozen Research Charter.",
                    422,
                )
            allowed_universe_ids: set[UUID] = set()
            for raw_id in charter.universe_version_ids or []:
                try:
                    allowed_universe_ids.add(UUID(str(raw_id)))
                except ValueError:
                    continue
            allowed_domains = {
                normalize_data_domain(value) for value in (charter.allowed_data_domains or []) if value
            }
            revisions = []
            if allowed_universe_ids and allowed_domains:
                revisions = list(
                    session.scalars(
                        select(DatasetRevision)
                        .where(
                            DatasetRevision.partition == "DISCOVERY",
                            DatasetRevision.quality_state == "VALID",
                            DatasetRevision.point_in_time_state == "VALID",
                            DatasetRevision.catalog_uri.is_not(None),
                            DatasetRevision.universe_version_id.in_(allowed_universe_ids),
                        )
                        .order_by(DatasetRevision.created_at.desc())
                    )
                )
            source_ids = {item.data_source_id for item in revisions if item.data_source_id is not None}
            sources = {
                item.id: item
                for item in session.scalars(
                    select(GovernedDataSource).where(GovernedDataSource.id.in_(source_ids))
                )
            } if source_ids else {}
            datasets: list[dict[str, Any]] = []
            for revision in revisions:
                source = (
                    sources.get(revision.data_source_id)
                    if revision.data_source_id is not None
                    else None
                )
                if (
                    source is None
                    or source.state != "ACTIVE"
                    or source.preflight_state != "READY"
                ):
                    continue
                domains = dataset_revision_domains(revision, source)
                if not domains.intersection(allowed_domains):
                    continue
                catalog_uri = revision.catalog_uri or ""
                if not catalog_uri.startswith(CATALOG_URI_PREFIX):
                    continue
                catalog_key = catalog_uri.removeprefix(CATALOG_URI_PREFIX)
                if not catalog_key or not revision.instrument_scope:
                    continue
                datasets.append(
                    {
                        "dataset_revision_id": str(revision.id),
                        "universe_version_id": str(revision.universe_version_id),
                        "data_domains": sorted(domains),
                        "catalog_key": catalog_key,
                        "catalog_uri": catalog_uri,
                        "provider_name": revision.provider_name,
                        "source_license": revision.source_license,
                        "nautilus_data_type": revision.nautilus_data_type,
                        "instrument_scope": revision.instrument_scope,
                        "event_start": revision.event_start,
                        "event_end": revision.event_end,
                        "available_start": revision.available_start,
                        "available_end": revision.available_end,
                        "row_count": revision.row_count,
                        "schema_revision": revision.schema_revision,
                        "quality_result": revision.quality_result,
                        "point_in_time_result": revision.point_in_time_result,
                    }
                )
    finally:
        engine.dispose()

    (workspace / "DATASETS.json").write_text(
        json.dumps(
            {"policy": "GOVERNED_DISCOVERY_ONLY", "datasets": datasets},
            ensure_ascii=False,
            indent=2,
            default=str,
        ),
        encoding="utf-8",
    )
    (workspace / "EXPERIMENT_CONTRACT.schema.json").write_text(
        json.dumps(
            BacktestExperimentRequest.model_json_schema(),
            ensure_ascii=False,
            indent=2,
        ),
        encoding="utf-8",
    )
    experiments_root = workspace / "experiments"
    experiments_root.mkdir(parents=True, exist_ok=True)
    (workspace / "NAUTILUS_EXPERIMENTS.md").write_text(
        """# Governed Nautilus experiment interface

QuaZonai Core does **not** execute your Python directly and does not expose database or
runtime credentials. The parent Mission worker validates JSON contracts in `experiments/`
and sends accepted requests to the separately deployed NautilusTrader runtime.

## Required workflow

1. Read `DATASETS.json`. You may use only a listed `dataset_revision_id`, its exact
   `catalog_key`, and instruments within that revision's `instrument_scope`.
2. Read `EXPERIMENT_CONTRACT.schema.json`.
3. For quantitative claims, write one or more `experiments/<experiment-uuid>.json` files.
   A round may contain at most 20 contracts.
4. Mission experiments must use `mode` `DISCOVERY` or `PORTFOLIO` and must provide a
   `strategy.kind` of `SOURCE_BUNDLE`. Put the complete Python strategy/config source in
   `strategy.source_files`; paths must be relative and traversal-free.
5. `strategy.strategy_path` and `strategy.config_path` must resolve inside that source
   bundle. The strategy must be a real NautilusTrader `Strategy` and its config a real
   `StrategyConfig`. Pin only the validated `nautilus_trader==1.231.0` runtime dependency.
6. `data_config` and `risk_config` are reserved and must remain empty until an explicit
   contract version applies them. Do not claim assumptions the remote runtime ignores.
7. Do not invent fills, PnL, positions, statistics, or DatasetRevision metadata. The parent
   worker executes accepted contracts and writes canonical results to `evidence/*.json`.
8. The exact successful `StrategyArtifact` is immutable research lineage and is what a
   promoted Candidate Bundle must reuse for downstream paper/live Nautilus runtimes.

If `DATASETS.json` contains no usable dataset, document the blocked evidence requirement in
`RESULT.md`; do not fabricate a backtest.
""",
        encoding="utf-8",
    )
    return len(datasets)


def _load_contract(path: Path, *, root: Path) -> BacktestExperimentRequest:
    if path.is_symlink():
        raise QfError(
            "MISSION_WORKSPACE_PATH_UNSAFE",
            "Mission experiment contracts cannot be symlinks.",
            422,
            {"path": path.name},
        )
    if path.parent.resolve(strict=True) != root.resolve(strict=True):
        raise QfError(
            "MISSION_WORKSPACE_PATH_UNSAFE",
            "Mission experiment contract escaped its controlled directory.",
            422,
            {"path": path.name},
        )
    try:
        raw: Any = json.loads(path.read_text(encoding="utf-8"))
        return BacktestExperimentRequest.model_validate(raw)
    except (OSError, ValueError, ValidationError) as exc:
        raise QfError(
            "EXPERIMENT_CONTRACT_INVALID",
            "Mission experiment contract is invalid.",
            422,
            {"path": path.name, "detail": str(exc)[-2000:]},
        ) from exc


def _controlled_experiment_root(workspace: Path) -> Path | None:
    real_workspace = workspace.resolve(strict=True)
    contracts_root = real_workspace / "experiments"
    try:
        info = os.lstat(contracts_root)
    except FileNotFoundError:
        return None
    if stat.S_ISLNK(info.st_mode) or not stat.S_ISDIR(info.st_mode):
        raise QfError(
            "MISSION_WORKSPACE_PATH_UNSAFE",
            "Mission experiments path must be a real directory, not a link.",
            422,
        )
    return contracts_root


def execute_workspace_experiments(
    settings: Settings,
    *,
    workspace: Path,
    mission_id: UUID,
    program_id: UUID,
    branch_id: UUID,
    already_executed: set[UUID],
) -> list[UUID]:
    """Execute new Discovery contracts and write structured evidence for the next Codex turn."""
    contracts_root = _controlled_experiment_root(workspace)
    if contracts_root is None:
        return []
    paths = sorted(path for path in contracts_root.iterdir() if path.suffix == ".json")
    if any(path.is_symlink() or not path.is_file() for path in paths):
        raise QfError(
            "MISSION_WORKSPACE_PATH_UNSAFE",
            "Mission experiment entries must be regular JSON files.",
            422,
        )
    if len(paths) > MAX_EXPERIMENTS_PER_ROUND:
        raise QfError(
            "EXPERIMENT_ROUND_LIMIT_EXCEEDED",
            "A Mission round declared too many experiments.",
            422,
            {"limit": MAX_EXPERIMENTS_PER_ROUND, "received": len(paths)},
        )

    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    coordinator = ExperimentCoordinator(factory)
    executed: list[UUID] = []
    try:
        for path in paths:
            try:
                contract = _load_contract(path, root=contracts_root)
            except QfError as exc:
                write_workspace_json(
                    workspace,
                    f"evidence/rejected-{path.stem}.json",
                    {
                        "state": "REJECTED",
                        "contract_file": path.name,
                        "failure_code": exc.code,
                        "failure_message": exc.message,
                    },
                )
                continue
            if contract.experiment_id in already_executed:
                continue
            if contract.mode not in {ExperimentMode.DISCOVERY, ExperimentMode.PORTFOLIO}:
                write_workspace_json(
                    workspace,
                    f"evidence/rejected-{contract.experiment_id}.json",
                    {
                        "experiment_id": str(contract.experiment_id),
                        "state": "REJECTED",
                        "failure_code": "EXPERIMENT_MODE_NOT_PERMITTED",
                        "failure_message": (
                            "Research Missions may submit Discovery or Portfolio simulation only."
                        ),
                    },
                )
                already_executed.add(contract.experiment_id)
                executed.append(contract.experiment_id)
                continue
            if contract.strategy.kind != "SOURCE_BUNDLE":
                write_workspace_json(
                    workspace,
                    f"evidence/rejected-{contract.experiment_id}.json",
                    {
                        "experiment_id": str(contract.experiment_id),
                        "state": "REJECTED",
                        "failure_code": "EXPERIMENT_STRATEGY_ARTIFACT_REQUIRED",
                        "failure_message": (
                            "Research Missions must submit a self-contained SOURCE_BUNDLE strategy artifact."
                        ),
                    },
                )
                already_executed.add(contract.experiment_id)
                executed.append(contract.experiment_id)
                continue
            try:
                entry = coordinator.execute(
                    mission_id=mission_id,
                    program_id=program_id,
                    branch_id=branch_id,
                    request=contract,
                    sealed=False,
                )
            except QfError as exc:
                with factory() as evidence_session:
                    entry = evidence_session.get(SearchLedgerEntry, contract.experiment_id)
                    if entry is not None:
                        evidence_session.expunge(entry)
                if entry is not None:
                    write_evidence(workspace, entry)
                else:
                    write_workspace_json(
                        workspace,
                        f"evidence/rejected-{contract.experiment_id}.json",
                        {
                            "experiment_id": str(contract.experiment_id),
                            "state": "REJECTED",
                            "failure_code": exc.code,
                            "failure_message": exc.message,
                        },
                    )
            else:
                write_evidence(workspace, entry)
            already_executed.add(contract.experiment_id)
            executed.append(contract.experiment_id)
    finally:
        engine.dispose()

    comparison = {
        "experiment_ids": [str(value) for value in sorted(already_executed, key=str)],
        "instruction": (
            "Compare orders, fills, positions, PnL and statistics. Keep failed and rejected "
            "attempts in the Search Ledger; do not delete inconvenient evidence."
        ),
    }
    write_workspace_json(workspace, "evidence/INDEX.json", comparison)
    return executed
