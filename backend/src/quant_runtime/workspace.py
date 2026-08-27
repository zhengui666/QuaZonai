"""Mission workspace bridge for bounded, evidence-driven Nautilus research rounds."""

from __future__ import annotations

import json
from pathlib import Path
from typing import Any
from uuid import UUID

from pydantic import ValidationError

from db.session import create_database_engine, create_session_factory
from errors import QfError
from quant_runtime.contracts import BacktestExperimentRequest, ExperimentMode
from quant_runtime.ledger import ExperimentCoordinator, write_evidence
from settings import Settings

MAX_EXPERIMENTS_PER_ROUND = 20


def _load_contract(path: Path) -> BacktestExperimentRequest:
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


def execute_workspace_experiments(
    settings: Settings,
    *,
    workspace: Path,
    mission_id: UUID,
    program_id: UUID,
    branch_id: UUID,
    already_executed: set[UUID],
) -> list[UUID]:
    """Execute new Discovery contracts and write structured evidence for the next Codex turn.

    The Codex child can only create files in its worktree.  The parent worker owns
    DB credentials and the remote runtime token, validates every contract, and
    performs the network call outside the Codex sandbox.
    """
    contracts_root = workspace / "experiments"
    if not contracts_root.exists():
        return []
    paths = sorted(path for path in contracts_root.glob("*.json") if path.is_file())
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
            contract = _load_contract(path)
            if contract.experiment_id in already_executed:
                continue
            if contract.mode not in {ExperimentMode.DISCOVERY, ExperimentMode.PORTFOLIO}:
                raise QfError(
                    "EXPERIMENT_MODE_NOT_PERMITTED",
                    "Research Missions may submit Discovery or Portfolio simulation only.",
                    422,
                    {"mode": contract.mode.value, "path": path.name},
                )
            entry = coordinator.execute(
                mission_id=mission_id,
                program_id=program_id,
                branch_id=branch_id,
                request=contract,
                sealed=False,
            )
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
    evidence_root = workspace / "evidence"
    evidence_root.mkdir(parents=True, exist_ok=True)
    (evidence_root / "INDEX.json").write_text(
        json.dumps(comparison, ensure_ascii=False, indent=2), encoding="utf-8"
    )
    return executed
