"""Optimization, backtest, and single-Holdout finite job implementations."""

from __future__ import annotations

import argparse
import importlib.util
import json
import os
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from datetime import UTC, datetime
from pathlib import Path
from types import ModuleType
from typing import Any
from uuid import UUID, uuid4

from sqlalchemy import select, text

from db.models import Experiment, Job, ResearchCase, Run, StrategyVersion
from db.session import create_database_engine, create_session_factory
from errors import QfError
from events import append_event
from jobs import enqueue_job
from optimization import TrialPoint, select_compromise
from settings import Settings

TRIAL_COUNT = 100
POPULATION_SIZE = 20
MAX_PARALLEL_PROCESSES = 4


def _load_module(path: Path, module_name: str) -> ModuleType:
    specification = importlib.util.spec_from_file_location(module_name, path)
    if specification is None or specification.loader is None:
        raise QfError("STRATEGY_FILE_INVALID", "Strategy source could not be loaded.", 422)
    module = importlib.util.module_from_spec(specification)
    sys.modules[module_name] = module
    specification.loader.exec_module(module)
    return module


def _backtest_subprocess(
    settings: Settings,
    *,
    experiment_id: UUID,
    parameters: dict[str, Any],
    phase: str,
) -> dict[str, Any]:
    command = [
        sys.executable,
        "-m",
        "quazonai.runners.backtest",
        "--experiment-id",
        str(experiment_id),
        "--parameters-json",
        json.dumps(parameters, separators=(",", ":"), default=str),
        "--phase",
        phase,
    ]
    try:
        result = subprocess.run(
            command,
            check=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            timeout=settings.backtest_timeout_seconds,
            env=os.environ.copy(),
        )
    except subprocess.TimeoutExpired as exc:
        raise QfError("BACKTEST_FAILED", "Backtest process exceeded its time limit.", 503) from exc
    except subprocess.CalledProcessError as exc:
        raise QfError(
            "BACKTEST_FAILED",
            "Backtest process failed.",
            422,
            {"exit_code": exc.returncode},
        ) from exc
    try:
        payload = json.loads(result.stdout)
    except json.JSONDecodeError as exc:
        raise QfError("BACKTEST_FAILED", "Backtest returned invalid output.", 500) from exc
    if not isinstance(payload, dict):
        raise QfError("BACKTEST_FAILED", "Backtest result must be an object.", 500)
    return payload


def _run_trial_with_retry(
    settings: Settings,
    *,
    experiment_id: UUID,
    parameters: dict[str, Any],
) -> dict[str, Any]:
    first_error: Exception | None = None
    for _ in range(2):
        try:
            return _backtest_subprocess(
                settings,
                experiment_id=experiment_id,
                parameters=parameters,
                phase="train",
            )
        except Exception as exc:  # noqa: BLE001 - exact same trial receives one retry
            if first_error is None:
                first_error = exc
    assert first_error is not None
    raise first_error


def _create_study(settings: Settings, experiment: Experiment, run_id: UUID) -> Any:
    try:
        import optuna
    except ImportError as exc:
        raise QfError(
            "RESEARCH_RUNTIME_UNAVAILABLE",
            "Optuna is not installed in the finite-worker runtime.",
            503,
        ) from exc

    directions = [str(item).lower() for item in experiment.objective_directions]
    study_name = f"quazonai-{experiment.id}-{run_id}"
    if settings.database_url.startswith("postgresql"):
        engine = create_database_engine(settings)
        with engine.begin() as connection:
            connection.execute(text("CREATE SCHEMA IF NOT EXISTS optuna"))
        storage = optuna.storages.RDBStorage(
            url=settings.database_url,
            engine_kwargs={"connect_args": {"options": "-csearch_path=optuna"}},
        )
    else:
        storage = None
    sampler = optuna.samplers.NSGAIISampler(
        population_size=POPULATION_SIZE,
        seed=experiment.seed,
    )
    return optuna.create_study(
        study_name=study_name,
        directions=directions,
        sampler=sampler,
        storage=storage,
        load_if_exists=False,
    )


def _load_strategy_for_suggestions(
    settings: Settings,
    strategy: StrategyVersion,
) -> tuple[ModuleType, Path, str]:
    root = settings.import_root / "optimization" / f"{strategy.id}-{uuid4()}"
    root.mkdir(parents=True, exist_ok=False)
    module_name = f"qf_optimization_strategy_{strategy.id.hex}"
    source = root / f"{module_name}.py"
    source.write_text(strategy.source_text, encoding="utf-8")
    sys.path.insert(0, str(root))
    return _load_module(source, module_name), root, module_name


def _cleanup_strategy(root: Path, module_name: str) -> None:
    try:
        sys.path.remove(str(root))
    except ValueError:
        pass
    sys.modules.pop(module_name, None)
    shutil.rmtree(root, ignore_errors=True)


def run_optimization(settings: Settings, job_id: UUID) -> None:
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    with factory.begin() as session:
        job = session.get(Job, job_id)
        if job is None:
            raise QfError("JOB_NOT_FOUND", "Optimization job does not exist.", 404)
        run = session.execute(
            select(Run).where(Run.id == job.resource_id).with_for_update()
        ).scalar_one()
        if run.type != "OPTIMIZATION" or run.state not in {"QUEUED", "RUNNING"}:
            raise QfError("RUN_INVALID_STATE", "Run is not a queued optimization.", 409)
        experiment = session.get(Experiment, run.experiment_id)
        if experiment is None:
            raise QfError("EXPERIMENT_UNKNOWN", "Experiment does not exist.", 404)
        strategy = session.get(StrategyVersion, experiment.strategy_version_id)
        if strategy is None:
            raise QfError("STRATEGY_VERSION_UNKNOWN", "Strategy version does not exist.", 404)
        run.state = "RUNNING"
        run.started_at = run.started_at or datetime.now(UTC)
        session.expunge(experiment)
        session.expunge(strategy)
        run_id = run.id

    strategy_module, strategy_root, module_name = _load_strategy_for_suggestions(
        settings, strategy
    )
    try:
        suggest = getattr(strategy_module, "suggest", None)
        if suggest is None or not callable(suggest):
            raise QfError("STRATEGY_FILE_INVALID", "Strategy no longer exports suggest().", 422)
        study = _create_study(settings, experiment, run_id)
        records: list[dict[str, Any]] = []

        for _ in range(TRIAL_COUNT // POPULATION_SIZE):
            batch: list[tuple[Any, dict[str, Any]]] = []
            for _ in range(POPULATION_SIZE):
                trial = study.ask()
                parameters = suggest(trial)
                if not isinstance(parameters, dict):
                    raise QfError(
                        "STRATEGY_FILE_INVALID",
                        "suggest() must return a dictionary of trial parameters.",
                        422,
                    )
                json.dumps(parameters, default=str)
                batch.append((trial, parameters))

            with ThreadPoolExecutor(max_workers=MAX_PARALLEL_PROCESSES) as executor:
                futures = {
                    trial.number: executor.submit(
                        _run_trial_with_retry,
                        settings,
                        experiment_id=experiment.id,
                        parameters=parameters,
                    )
                    for trial, parameters in batch
                }
                results = {number: future.result() for number, future in futures.items()}

            for trial, parameters in sorted(batch, key=lambda item: item[0].number):
                result = results[trial.number]
                values = tuple(float(item) for item in result["objectives"])
                study.tell(trial, values)
                records.append(
                    {
                        "trial_no": trial.number,
                        "parameters": parameters,
                        "objectives": list(values),
                        "summary": result.get("summary", {}),
                    }
                )

        directions = tuple(str(item).lower() for item in experiment.objective_directions)
        selected = select_compromise(
            [
                TrialPoint(
                    trial_no=int(item["trial_no"]),
                    values=tuple(float(value) for value in item["objectives"]),
                )
                for item in records
            ],
            directions,  # type: ignore[arg-type]
        )
        selected_record = next(item for item in records if item["trial_no"] == selected.trial_no)

        with factory.begin() as session:
            run = session.get(Run, run_id)
            experiment_row = session.get(Experiment, experiment.id)
            assert run is not None and experiment_row is not None
            run.state = "SUCCEEDED"
            run.finished_at = datetime.now(UTC)
            run.error_code = None
            run.error_message = None
            run.summary = {
                "trial_count": TRIAL_COUNT,
                "population_size": POPULATION_SIZE,
                "max_parallel_processes": MAX_PARALLEL_PROCESSES,
                "trials": records,
                "selected": selected_record,
            }
            experiment_row.selected_trial_no = selected.trial_no
            experiment_row.optuna_study_name = study.study_name
            holdout = Run(
                experiment_id=experiment.id,
                runtime_bundle_id=experiment.runtime_bundle_id,
                type="HOLDOUT",
                state="QUEUED",
                summary={
                    "selected_trial_no": selected.trial_no,
                    "parameters": selected_record["parameters"],
                },
            )
            session.add(holdout)
            session.flush()
            enqueue_job(
                session,
                kind="HOLDOUT",
                resource_type="run",
                resource_id=holdout.id,
            )
            append_event(
                session,
                kind="OPTIMIZATION_SUCCEEDED",
                aggregate_type="run",
                aggregate_id=run.id,
                payload={
                    "selected_trial_no": selected.trial_no,
                    "holdout_run_id": str(holdout.id),
                },
            )
    except Exception as exc:
        with factory.begin() as session:
            run = session.get(Run, run_id)
            if run is not None and run.state != "SUCCEEDED":
                run.state = "FAILED"
                run.finished_at = datetime.now(UTC)
                run.error_code = getattr(exc, "code", type(exc).__name__)
                run.error_message = str(exc)[-4000:]
                append_event(
                    session,
                    kind="OPTIMIZATION_FAILED",
                    aggregate_type="run",
                    aggregate_id=run.id,
                    payload={"error_code": run.error_code},
                )
        raise
    finally:
        _cleanup_strategy(strategy_root, module_name)


def run_holdout(settings: Settings, job_id: UUID) -> None:
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    with factory.begin() as session:
        job = session.get(Job, job_id)
        if job is None:
            raise QfError("JOB_NOT_FOUND", "Holdout job does not exist.", 404)
        run = session.execute(
            select(Run).where(Run.id == job.resource_id).with_for_update()
        ).scalar_one()
        if run.type != "HOLDOUT" or run.state not in {"QUEUED", "RUNNING"}:
            raise QfError("RUN_INVALID_STATE", "Run is not a queued Holdout.", 409)
        experiment = session.get(Experiment, run.experiment_id)
        if experiment is None:
            raise QfError("EXPERIMENT_UNKNOWN", "Experiment does not exist.", 404)
        other_holdout = session.scalar(
            select(Run.id).where(
                Run.experiment_id == experiment.id,
                Run.type == "HOLDOUT",
                Run.id != run.id,
            )
        )
        if other_holdout is not None:
            raise QfError(
                "HOLDOUT_REQUIRED",
                "An Experiment may have only one Holdout run.",
                409,
            )
        parameters = dict(run.summary.get("parameters") or {})
        run.state = "RUNNING"
        run.started_at = run.started_at or datetime.now(UTC)
        run_id = run.id
        experiment_id = experiment.id
        research_id = experiment.research_id

    try:
        result = _backtest_subprocess(
            settings,
            experiment_id=experiment_id,
            parameters=parameters,
            phase="holdout",
        )
        with factory.begin() as session:
            run = session.get(Run, run_id)
            research = session.execute(
                select(ResearchCase).where(ResearchCase.id == research_id).with_for_update()
            ).scalar_one()
            assert run is not None
            if research.state != "ACTIVE":
                raise QfError(
                    "RESEARCH_INVALID_STATE",
                    "Holdout can only complete while Research is ACTIVE.",
                    409,
                )
            run.state = "SUCCEEDED"
            run.finished_at = datetime.now(UTC)
            run.summary = {**run.summary, "result": result}
            research.state = "REVIEW"
            append_event(
                session,
                kind="HOLDOUT_SUCCEEDED",
                aggregate_type="run",
                aggregate_id=run.id,
                payload={"research_id": str(research.id)},
            )
    except Exception as exc:
        with factory.begin() as session:
            run = session.get(Run, run_id)
            if run is not None:
                run.state = "FAILED"
                run.finished_at = datetime.now(UTC)
                run.error_code = getattr(exc, "code", type(exc).__name__)
                run.error_message = str(exc)[-4000:]
                append_event(
                    session,
                    kind="HOLDOUT_FAILED",
                    aggregate_type="run",
                    aggregate_id=run.id,
                    payload={"error_code": run.error_code},
                )
        raise


def run_single_backtest(settings: Settings, job_id: UUID) -> None:
    engine = create_database_engine(settings)
    factory = create_session_factory(engine)
    with factory.begin() as session:
        job = session.get(Job, job_id)
        if job is None:
            raise QfError("JOB_NOT_FOUND", "Backtest job does not exist.", 404)
        run = session.execute(
            select(Run).where(Run.id == job.resource_id).with_for_update()
        ).scalar_one()
        experiment_id = run.experiment_id
        if experiment_id is None:
            raise QfError("EXPERIMENT_UNKNOWN", "Backtest run has no Experiment.", 422)
        run.state = "RUNNING"
        run.started_at = run.started_at or datetime.now(UTC)
        run_id = run.id
    try:
        result = _backtest_subprocess(
            settings,
            experiment_id=experiment_id,
            parameters={},
            phase="train",
        )
        with factory.begin() as session:
            run = session.get(Run, run_id)
            assert run is not None
            run.state = "SUCCEEDED"
            run.finished_at = datetime.now(UTC)
            run.summary = result
    except Exception as exc:
        with factory.begin() as session:
            run = session.get(Run, run_id)
            if run is not None:
                run.state = "FAILED"
                run.finished_at = datetime.now(UTC)
                run.error_code = getattr(exc, "code", type(exc).__name__)
                run.error_message = str(exc)[-4000:]
        raise


def build_parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description="Run one QuaZonai research job")
    parser.add_argument("action", choices=["optimization", "holdout", "backtest"])
    parser.add_argument("job_id")
    return parser


def main() -> int:
    args = build_parser().parse_args()
    settings = Settings.from_env()
    job_id = UUID(args.job_id)
    if args.action == "optimization":
        run_optimization(settings, job_id)
    elif args.action == "holdout":
        run_holdout(settings, job_id)
    else:
        run_single_backtest(settings, job_id)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
