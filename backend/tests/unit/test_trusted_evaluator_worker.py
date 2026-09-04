from __future__ import annotations

from dataclasses import replace
import os
from pathlib import Path
import sys
import time
from uuid import uuid4

import pytest
from sqlalchemy import create_engine, delete, select
from sqlalchemy.orm import Session, sessionmaker

from db.models import (
    AlphaCalibrationVersion,
    AlphaDiscoveryEvaluation,
    AlphaDiscoveryEvaluationGate,
    AlphaDiscoveryEvaluationMetric,
    AlphaEvaluationAssignment,
    AlphaEvaluationAssignmentDatasetRevision,
    AlphaEvaluationEpisode,
    AlphaEvaluationForecast,
    AlphaEvaluationGate,
    AlphaEvaluationMetric,
    AlphaEvaluationResult,
    AlphaModel,
    AlphaQualification,
    AlphaSignalArtifact,
    Base,
    DatasetRevision,
    Disclosure,
    EvidenceExposure,
    GovernedDataSource,
    Job,
    NautilusCatalogBinding,
    PromotionPolicyGate,
)
from errors import QfError
from jobs import enqueue_job
from research_engine.trusted_evaluator_service import (
    run_trusted_evaluator,
    terminalize_trusted_evaluator_failure,
)
from runners.finite_worker import run_once
from settings import Settings
from test_trusted_alpha_evaluation_persistence import _seed_assignment


_BACKEND_ROOT = Path(__file__).parents[2]


def _worker_settings(settings: Settings, tmp_path: Path, command: Path) -> tuple[Settings, object]:
    database_path = tmp_path / "trusted-evaluator.sqlite"
    database_url = f"sqlite+pysqlite:///{database_path}"
    engine = create_engine(database_url)
    Base.metadata.create_all(engine)
    return (
        replace(
            settings,
            database_url=database_url,
            alembic_url=database_url,
            trusted_evaluator_command=command,
            mission_job_timeout_seconds=5,
        ),
        engine,
    )


def _configure_child_environment(
    monkeypatch: pytest.MonkeyPatch,
    *,
    database_url: str,
    command: Path,
) -> None:
    previous_pythonpath = os.environ.get("PYTHONPATH")
    monkeypatch.setenv(
        "PYTHONPATH",
        str(_BACKEND_ROOT / "src")
        if not previous_pythonpath
        else f"{_BACKEND_ROOT / 'src'}{os.pathsep}{previous_pythonpath}",
    )
    monkeypatch.setenv("QUAZONAI_ENV", "test")
    monkeypatch.setenv("QUAZONAI_DATABASE_URL", database_url)
    monkeypatch.setenv("QUAZONAI_ALEMBIC_URL", database_url)
    monkeypatch.setenv("QUAZONAI_TRUSTED_EVALUATOR_COMMAND", str(command))
    monkeypatch.setenv("QUAZONAI_SECRET_PROBE", "must-not-reach-evaluator")


def _catalog(session: Session, dataset: DatasetRevision, *, sealed: bool) -> None:
    session.add(
        NautilusCatalogBinding(
            dataset_revision_id=dataset.id,
            catalog_uri=f"catalog://trusted-worker/{dataset.partition.casefold()}",
            provider="fixture-provider",
            source_license="fixture-license",
            nautilus_data_type="QuoteTick",
            instrument_scope=["US:TEST"],
            event_time_range={},
            available_time_range={},
            schema_revision="v1",
            quality_state="VALID",
            quality_result={},
            point_in_time_state="VALID",
            point_in_time_result={},
            sealed=sealed,
        )
    )


def _queued_discovery(session: Session) -> AlphaDiscoveryEvaluation:
    """Reuse the narrow persistence fixture, removing only its completed facts."""
    facts = _seed_assignment(session)
    assignment = facts["assignment"]
    episode = facts["episode"]
    result = facts["result"]
    discovery = facts["discovery"]
    calibration = facts["calibration"]
    assert isinstance(assignment, AlphaEvaluationAssignment)
    assert isinstance(episode, AlphaEvaluationEpisode)
    assert isinstance(result, AlphaEvaluationResult)
    assert isinstance(discovery, AlphaDiscoveryEvaluation)
    assert isinstance(calibration, AlphaCalibrationVersion)

    session.execute(delete(EvidenceExposure).where(EvidenceExposure.episode_id == episode.id))
    session.execute(delete(Disclosure).where(Disclosure.episode_id == episode.id))
    session.execute(delete(AlphaEvaluationForecast).where(AlphaEvaluationForecast.result_id == result.id))
    session.execute(
        delete(AlphaQualification).where(AlphaQualification.evaluation_result_id == result.id)
    )
    session.execute(
        delete(AlphaSignalArtifact).where(AlphaSignalArtifact.evaluation_result_id == result.id)
    )
    session.execute(delete(AlphaEvaluationGate).where(AlphaEvaluationGate.result_id == result.id))
    session.execute(delete(AlphaEvaluationMetric).where(AlphaEvaluationMetric.result_id == result.id))
    session.execute(delete(AlphaEvaluationResult).where(AlphaEvaluationResult.id == result.id))
    session.execute(delete(AlphaEvaluationEpisode).where(AlphaEvaluationEpisode.id == episode.id))
    session.execute(
        delete(AlphaEvaluationAssignmentDatasetRevision).where(
            AlphaEvaluationAssignmentDatasetRevision.assignment_id == assignment.id
        )
    )
    session.execute(delete(AlphaEvaluationAssignment).where(AlphaEvaluationAssignment.id == assignment.id))
    session.execute(delete(AlphaCalibrationVersion).where(AlphaCalibrationVersion.id == calibration.id))
    session.execute(
        delete(AlphaDiscoveryEvaluationGate).where(
            AlphaDiscoveryEvaluationGate.discovery_evaluation_id == discovery.id
        )
    )
    session.execute(
        delete(AlphaDiscoveryEvaluationMetric).where(
            AlphaDiscoveryEvaluationMetric.discovery_evaluation_id == discovery.id
        )
    )
    session.flush()

    model = facts["model"]
    mission = facts["mission"]
    design = facts["design"]
    datasets = facts["datasets"]
    assert isinstance(model, object)
    assert isinstance(mission, object)
    assert isinstance(design, object)
    assert isinstance(datasets, list)
    discovery.state = "QUEUED"
    discovery.outcome_code = None
    discovery.private_result_ref = None
    discovery.evaluated_at = None
    discovery.completed_at = None
    model.state = "DRAFT"  # type: ignore[attr-defined]
    mission.state = "AWAITING_VALIDATION"  # type: ignore[attr-defined]
    mission.finished_at = None  # type: ignore[attr-defined]
    mission.error_code = None  # type: ignore[attr-defined]
    design.qualification_metric_code = "NET_RETURN"  # type: ignore[attr-defined]
    alpha = session.get(AlphaModel, model.alpha_model_id)  # type: ignore[attr-defined]
    assert alpha is not None
    alpha.state = "RESEARCHING"
    policy_gate = session.scalar(select(PromotionPolicyGate).limit(1))
    assert policy_gate is not None
    policy_gate.metric_code = "NET_RETURN"
    source = GovernedDataSource(
        name=f"trusted-worker-source-{uuid4()}",
        connector_key="fixture",
        state="ACTIVE",
        preflight_state="READY",
        universe_scope=[],
        fields=[],
        field_schema={},
        availability_semantics={},
        public_config={},
    )
    session.add(source)
    session.flush()
    for dataset in datasets:
        assert isinstance(dataset, DatasetRevision)
        dataset.data_source_id = source.id
        dataset.data_class = "VENDOR"
        dataset.promotability = "PROMOTABLE"
        dataset.quality_state = "VALID"
        dataset.point_in_time_state = "VALID"
        _catalog(session, dataset, sealed=dataset.partition == "SEALED")
    session.flush()
    return discovery


def test_exhausted_discovery_retry_closes_assignment_without_fake_result(engine) -> None:  # type: ignore[no-untyped-def]
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        discovery = _queued_discovery(session)
        discovery.state = "RUNNING"
        discovery_id = discovery.id
        assert terminalize_trusted_evaluator_failure(
            session,
            kind="DISCOVERY_EVALUATION",
            resource_id=discovery_id,
            outcome_code="RETRIES_EXHAUSTED",
        )

    with factory() as session:
        discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
        assert discovery is not None
        assert discovery.state == "FAILED"
        assert discovery.private_result_ref is None
        assert discovery.evaluated_at is None
        assert discovery.outcome_code == "RETRIES_EXHAUSTED"


def _write_evaluator(path: Path, body: str) -> Path:
    command = path / "trusted-evaluator.py"
    command.write_text(f"#!{sys.executable}\n{body}", encoding="utf-8")
    command.chmod(0o700)
    return command


_VALID_EVALUATOR = r'''
import json
import os
import sys
import uuid

descriptor = json.load(open(sys.argv[1], encoding="utf-8"))
if os.stat(sys.argv[1]).st_mode & 0o777 != 0o600 or os.stat(os.getcwd()).st_mode & 0o777 != 0o700:
    raise SystemExit(90)
if any(name in os.environ for name in (
    "QUAZONAI_DATABASE_URL", "QUAZONAI_SECRET_PROBE", "CODEX_HOME", "QUAZONAI_AUTH_COOKIE_KEY"
)):
    raise SystemExit(91)
if any(token in json.dumps(descriptor).lower() for token in ("raw", "artifact_uri", "storage_uri")):
    raise SystemExit(92)
codes = [
    "ANNUALIZED_VOLATILITY", "COVERAGE", "HIT_RATE", "IC_MEAN", "MAX_DRAWDOWN",
    "NET_RETURN", "OBSERVATION_COUNT", "RANK_IC_MEAN", "SHARPE_RATIO", "TRIAL_ADJUSTED_SHARPE",
]
gates = [
    "CALIBRATION_VALID", "EVIDENCE_VALID", "POINT_IN_TIME_VALID", "POLICY_VALID", "STATISTICAL_VALID",
]
def metrics(phase):
    return [
        {
            "phase": phase,
            "code": code,
            "status": "NOT_AVAILABLE" if code == "TRIAL_ADJUSTED_SHARPE" else "AVAILABLE",
            "value": None if code == "TRIAL_ADJUSTED_SHARPE" else (0.02 if code == "NET_RETURN" else 1.0),
        }
        for code in codes
    ]
def gate_rows():
    return [{"code": code, "status": "PASS", "reason_code": None} for code in gates]
if descriptor["kind"] == "DISCOVERY_EVALUATION":
    result = {
        "kind": "DISCOVERY_EVALUATION",
        "status": "VALID",
        "private_result_id": str(uuid.uuid4()),
        "evaluated_at": "2026-09-03T00:00:00Z",
        "metrics": metrics("DISCOVERY"),
        "gates": gate_rows(),
        "calibration": {
            "method": "ISOTONIC",
            "training_dataset": descriptor["discovery_dataset"],
            "private_artifact_ref": str(uuid.uuid4()),
        },
    }
elif descriptor["kind"] == "ALPHA_EVALUATION":
    result = {
        "kind": "ALPHA_EVALUATION",
        "status": "PASS",
        "private_result_id": str(uuid.uuid4()),
        "evaluated_at": "2026-09-03T00:00:00Z",
        "metrics": metrics("SEALED"),
        "gates": gate_rows(),
        "disclosure": {"classification": "QUALIFIED", "reason_code": None},
        "signal": {
            "row_count": 1,
            "event_start": "2026-09-03T00:00:00Z",
            "event_end": "2026-09-03T00:00:00Z",
            "available_start": "2026-09-03T00:00:00Z",
            "available_end": "2026-09-03T00:00:00Z",
        },
        "forecasts": [{
            "instrument_id": "US:TEST",
            "as_of_time": "2026-09-03T00:00:00Z",
            "effective_from": "2026-09-03T00:00:00Z",
            "effective_until": "2026-09-04T00:00:00Z",
            "expected_return": 0.02,
            "uncertainty": 0.01,
            "confidence": 0.8,
            "max_trade_notional": 10000,
            "max_position_notional": 50000,
            "max_participation_rate": 0.1,
            "days_to_liquidate": 2,
            "stressed_capacity_notional": 20000,
        }],
    }
else:
    raise SystemExit(93)
print(json.dumps(result, separators=(",", ":")))
'''


def test_fixed_evaluator_runner_persists_discovery_then_alpha_pass(
    settings: Settings,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    command = _write_evaluator(tmp_path, _VALID_EVALUATOR)
    worker_settings, engine = _worker_settings(settings, tmp_path, command)
    assert isinstance(worker_settings, Settings)
    _configure_child_environment(
        monkeypatch,
        database_url=worker_settings.database_url,
        command=command,
    )
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        discovery = _queued_discovery(session)
        discovery_id = discovery.id
        enqueue_job(
            session,
            kind="DISCOVERY_EVALUATION",
            resource_type="alpha_discovery_evaluation",
            resource_id=discovery.id,
            payload={},
        )

    assert run_once(worker_settings, owner="worker-a", factory=factory)[0] is True
    assert run_once(worker_settings, owner="worker-a", factory=factory)[0] is True

    with factory() as session:
        discovery = session.get(AlphaDiscoveryEvaluation, discovery_id)
        result = session.scalar(select(AlphaEvaluationResult))
        signal = session.scalar(select(AlphaSignalArtifact))
        forecast = session.scalar(select(AlphaEvaluationForecast))
        qualification = session.scalar(select(AlphaQualification))
        jobs = list(session.scalars(select(Job).order_by(Job.created_at)))
        assert discovery is not None and discovery.state == "VALID"
        assert result is not None and result.result == "PASS"
        assert signal is not None and signal.run_id is None and signal.evaluation_result_id == result.id
        assert signal.artifact_uri == f"evaluator-private://alpha-result/{result.private_result_ref}/signal"
        assert forecast is not None and forecast.result_id == result.id and forecast.signal_artifact_id == signal.id
        assert qualification is not None and qualification.evaluation_result_id == result.id
        assert [(job.kind, job.state, job.payload) for job in jobs] == [
            ("DISCOVERY_EVALUATION", "SUCCEEDED", {}),
            ("ALPHA_EVALUATION", "SUCCEEDED", {}),
        ]


@pytest.mark.parametrize(
    ("kind", "resource_type"),
    [
        ("DISCOVERY_EVALUATION", "alpha_discovery_evaluation"),
        ("ALPHA_EVALUATION", "alpha_evaluation_assignment"),
    ],
)
def test_payload_bearing_trusted_jobs_fail_before_writing_facts(
    settings: Settings,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    kind: str,
    resource_type: str,
) -> None:
    command = _write_evaluator(tmp_path, _VALID_EVALUATOR)
    worker_settings, engine = _worker_settings(settings, tmp_path, command)
    assert isinstance(worker_settings, Settings)
    _configure_child_environment(
        monkeypatch,
        database_url=worker_settings.database_url,
        command=command,
    )
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        job = enqueue_job(
            session,
            kind=kind,
            resource_type=resource_type,
            resource_id=uuid4(),
            payload={"forged": "must-not-be-read"},
        )
        job_id = job.id

    assert run_once(worker_settings, owner="worker-a", factory=factory)[0] is True
    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None and job.state == "FAILED"
        assert "forged" not in (job.last_error or "")
        assert session.scalar(select(AlphaDiscoveryEvaluation)) is None
        assert session.scalar(select(AlphaEvaluationAssignment)) is None
        assert session.scalar(select(AlphaEvaluationResult)) is None


@pytest.mark.parametrize(
    "body",
    (
        'import json\nprint(json.dumps({"kind": "DISCOVERY_EVALUATION", "unknown": True}))\n',
        'import sys\nsys.stdout.write("{")\n',
    ),
)
def test_malformed_or_unknown_evaluator_output_fails_closed_in_discovery_runner(
    settings: Settings,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
    body: str,
) -> None:
    command = _write_evaluator(tmp_path, body)
    worker_settings, engine = _worker_settings(settings, tmp_path, command)
    assert isinstance(worker_settings, Settings)
    _configure_child_environment(
        monkeypatch,
        database_url=worker_settings.database_url,
        command=command,
    )
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        discovery = _queued_discovery(session)
        job = enqueue_job(
            session,
            kind="DISCOVERY_EVALUATION",
            resource_type="alpha_discovery_evaluation",
            resource_id=discovery.id,
            payload={},
        )
        job_id = job.id

    assert run_once(worker_settings, owner="worker-a", factory=factory)[0] is True
    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None and job.state == "READY"
        assert session.scalar(select(AlphaEvaluationAssignment)) is None
        assert session.scalar(select(AlphaEvaluationResult)) is None


@pytest.mark.skipif(os.name != "posix", reason="forked evaluator regression requires POSIX")
def test_forked_evaluator_stdout_descendant_is_cleared_by_outer_runner_group(
    settings: Settings,
    tmp_path: Path,
    monkeypatch: pytest.MonkeyPatch,
) -> None:
    marker = tmp_path / "forked-evaluator-terminated"
    command = _write_evaluator(
        tmp_path,
        f'''import os
import signal
import sys
import time
from pathlib import Path

marker = Path({str(marker)!r})
if os.fork():
    raise SystemExit(17)

def stop(*_args):
    marker.write_text("terminated", encoding="utf-8")
    raise SystemExit(0)

signal.signal(signal.SIGTERM, stop)
while True:
    time.sleep(0.01)
''',
    )
    worker_settings, engine = _worker_settings(settings, tmp_path, command)
    assert isinstance(worker_settings, Settings)
    _configure_child_environment(
        monkeypatch,
        database_url=worker_settings.database_url,
        command=command,
    )
    factory = sessionmaker(bind=engine, expire_on_commit=False)
    with factory.begin() as session:
        discovery = _queued_discovery(session)
        job = enqueue_job(
            session,
            kind="DISCOVERY_EVALUATION",
            resource_type="alpha_discovery_evaluation",
            resource_id=discovery.id,
            payload={},
        )
        job_id = job.id

    assert run_once(worker_settings, owner="worker-a", factory=factory)[0] is True
    deadline = time.monotonic() + 2
    while not marker.exists() or marker.read_text(encoding="utf-8") != "terminated":
        assert time.monotonic() < deadline
        time.sleep(0.01)
    with factory() as session:
        job = session.get(Job, job_id)
        assert job is not None and job.state == "READY"
        assert session.scalar(select(AlphaEvaluationAssignment)) is None


def test_unavailable_command_and_oversized_stdout_fail_closed(
    settings: Settings,
    tmp_path: Path,
) -> None:
    unavailable = tmp_path / "not-executable"
    unavailable.write_text("ignored", encoding="utf-8")
    with pytest.raises(QfError, match="TRUSTED_EVALUATOR_UNAVAILABLE"):
        run_trusted_evaluator(replace(settings, trusted_evaluator_command=unavailable), {"kind": "X"})

    oversized = _write_evaluator(
        tmp_path,
        'import os\nimport sys\nos.write(sys.stdout.fileno(), b"x" * 1000001)\n',
    )
    with pytest.raises(QfError, match="TRUSTED_EVALUATOR_RESULT_INVALID"):
        run_trusted_evaluator(
            replace(settings, trusted_evaluator_command=oversized),
            {"kind": "ALPHA_EVALUATION"},
        )
