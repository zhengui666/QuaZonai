from __future__ import annotations

import importlib.util
from datetime import UTC, datetime, timedelta
from pathlib import Path
from uuid import uuid4

import pytest
from alembic.migration import MigrationContext
from alembic.operations import Operations
from sqlalchemy import Column, DateTime, Integer, MetaData, String, Table, Uuid, inspect
from sqlalchemy.exc import IntegrityError
from sqlalchemy.orm import Session

from db.models import (
    AlphaCalibrationVersion,
    AlphaEvaluationEpisode,
    AlphaModel,
    AlphaModelVersion,
    AlphaQualification,
    AlphaSignalArtifact,
    Base,
    DataQualityResult,
    DatasetRevision,
    FeaturePipelineVersion,
    GovernedDataSource,
    MarketUniverseVersion,
    QuantRuntimeRun,
    ResearchBranch,
    ResearchCharter,
    ResearchMission,
    ResearchProgram,
)


def _now() -> datetime:
    return datetime.now(UTC)


def _seed_research(session: Session) -> tuple[ResearchProgram, ResearchBranch, ResearchMission]:
    now = _now()
    charter = ResearchCharter(
        original_idea_text="Test a point-in-time Alpha signal.",
        research_question="Can this signal survive the sealed evaluation?",
        market_scope=[],
        universe_version_ids=[],
        prediction_horizon="1D",
        allowed_data_domains=[],
        explicit_exclusions=[],
        material_assumptions=[],
        system_assumptions=[],
        clarification_transcript=[],
        created_at=now,
    )
    session.add(charter)
    session.flush()
    program = ResearchProgram(charter_id=charter.id, title="alpha persistence")
    session.add(program)
    session.flush()
    branch = ResearchBranch(
        program_id=program.id,
        derivation_type="ROOT",
        hypothesis="The signal is point-in-time valid.",
        changed_assumptions=[],
        preserved_constraints=[],
        state="ACTIVE",
        revision_no=1,
        created_at=now,
    )
    session.add(branch)
    session.flush()
    mission = ResearchMission(
        program_id=program.id,
        branch_id=branch.id,
        type="ALPHA_DISCOVERY",
        role="ALPHA_RESEARCHER",
        state="PLANNED",
        objective="Produce a bounded Alpha artifact.",
        contract_version="1",
        input_snapshot={},
        capability_snapshot={},
        runtime_snapshot={},
        prompt_version="1",
        max_turns=2,
        max_tool_calls=2,
        attempt=1,
        revision=1,
    )
    session.add(mission)
    session.flush()
    return program, branch, mission


def test_point_in_time_alpha_facts_persist_as_normalized_rows(engine) -> None:
    now = _now()
    with Session(engine) as session:
        program, branch, mission = _seed_research(session)
        universe = MarketUniverseVersion(
            universe_key="US_EQUITIES",
            version_no=1,
            name="US Equities",
            state="ACTIVE",
            spec_json={},
            created_at=now,
        )
        source = GovernedDataSource(
            name="Vendor ticks",
            provider="vendor",
            state="ACTIVE",
            universe_scope=[],
            fields=[],
            preflight_state="READY",
            public_config={},
        )
        session.add_all([universe, source])
        session.flush()
        discovery_dataset = DatasetRevision(
            data_source_id=source.id,
            universe_version_id=universe.id,
            universe_name=universe.name,
            revision_no=1,
            data_class="VENDOR",
            origin="vendor-materialization",
            ingested_at=now,
            promotability="PROMOTABLE",
            schema_version="1",
            event_start=now,
            event_end=now + timedelta(minutes=1),
            available_start=now,
            available_end=now + timedelta(minutes=1),
            row_count=2,
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="DISCOVERY",
            created_at=now,
        )
        sealed_dataset = DatasetRevision(
            data_source_id=source.id,
            universe_version_id=universe.id,
            universe_name=universe.name,
            revision_no=2,
            data_class="VENDOR",
            origin="vendor-materialization",
            ingested_at=now,
            promotability="PROMOTABLE",
            schema_version="1",
            event_start=now,
            event_end=now + timedelta(minutes=1),
            available_start=now,
            available_end=now + timedelta(minutes=1),
            row_count=2,
            quality_state="VALID",
            point_in_time_state="VALID",
            partition="SEALED",
            created_at=now,
        )
        session.add_all([discovery_dataset, sealed_dataset])
        session.flush()
        quality = DataQualityResult(
            dataset_revision_id=discovery_dataset.id,
            check_kind="QUALITY",
            revision_no=1,
            state="VALID",
            summary={"coverage_ratio": 1.0},
            checker_version="1",
        )
        point_in_time = DataQualityResult(
            dataset_revision_id=discovery_dataset.id,
            check_kind="POINT_IN_TIME",
            revision_no=1,
            state="VALID",
            summary={"available_after_event": True},
            checker_version="1",
        )
        session.add_all([quality, point_in_time])
        session.flush()
        discovery_dataset.quality_result_id = quality.id
        discovery_dataset.point_in_time_result_id = point_in_time.id

        feature = FeaturePipelineVersion(
            pipeline_key="momentum",
            version_no=1,
            universe_version_id=universe.id,
            artifact_uri="features/momentum-v1.arrow",
            input_schema={"price": "float"},
            output_schema={"momentum": "float"},
            point_in_time_policy_version_id=uuid4(),
        )
        session.add(feature)
        session.flush()
        alpha = AlphaModel(
            alpha_key="momentum-alpha",
            name="Momentum Alpha",
            family="MOMENTUM",
            description="A bounded point-in-time momentum signal.",
            owner_program_id=program.id,
            state="RESEARCHING",
        )
        session.add(alpha)
        session.flush()
        version = AlphaModelVersion(
            alpha_model_id=alpha.id,
            version_no=1,
            source_mission_id=mission.id,
            universe_version_id=universe.id,
            feature_pipeline_version_id=feature.id,
            horizon="1D",
            mode="RELATIVE_SCORE",
            artifact_uri="alphas/momentum-v1.whl",
            entrypoint="momentum:Alpha",
            parameters={},
            input_contract={},
            output_contract={},
            state="VALIDATED",
        )
        session.add(version)
        session.flush()
        run = QuantRuntimeRun(
            program_id=program.id,
            branch_id=branch.id,
            mission_id=mission.id,
            mode="DISCOVERY",
            state="SUCCEEDED",
            experiment_key="momentum-v1",
            family="MOMENTUM",
            catalog_uri="catalogs/discovery",
            runtime_name="NautilusTrader",
            strategy_artifact={},
            parameters={},
            evidence={},
        )
        session.add(run)
        session.flush()
        signal = AlphaSignalArtifact(
            alpha_model_version_id=version.id,
            dataset_revision_id=discovery_dataset.id,
            run_id=run.id,
            mode="RELATIVE_SCORE",
            artifact_uri="signals/momentum-v1.arrow",
            row_count=2,
            event_start=now,
            event_end=now + timedelta(minutes=1),
            available_start=now,
            available_end=now + timedelta(minutes=1),
            schema_version="1",
        )
        calibration = AlphaCalibrationVersion(
            alpha_model_version_id=version.id,
            version_no=1,
            method="ISOTONIC",
            training_dataset_revision_ids=[str(discovery_dataset.id)],
            artifact_uri="calibrations/momentum-v1.arrow",
            parameters={},
            metrics={},
            state="VALIDATED",
        )
        session.add_all([signal, calibration])
        session.flush()
        episode = AlphaEvaluationEpisode(
            program_id=program.id,
            branch_id=branch.id,
            alpha_model_version_id=version.id,
            discovery_run_ids=[str(run.id)],
            validation_run_ids=[],
            sealed_run_id=run.id,
            sealed_dataset_revision_id=sealed_dataset.id,
            promotion_policy_version_id=uuid4(),
            state="COMPLETED",
            result="PASS",
            gate_results={"point_in_time": "PASS"},
            multiple_testing_summary={},
            disclosure={},
        )
        session.add(episode)
        session.flush()
        qualification = AlphaQualification(
            program_id=program.id,
            alpha_model_id=alpha.id,
            alpha_model_version_id=version.id,
            calibration_version_id=calibration.id,
            universe_version_id=universe.id,
            universe=universe.name,
            horizon="1D",
            role="PRIMARY_ALPHA",
            state="ACTIVE",
            name=alpha.name,
            scope_json={},
            evaluation_episode_id=episode.id,
            degradation_state="HEALTHY",
            qualification_metrics={"net_sharpe": 1.2},
            lineage=[],
        )
        session.add(qualification)
        session.flush()
        alpha.current_qualified_version_id = qualification.id
        session.commit()

        assert discovery_dataset.quality_result_id == quality.id
        assert discovery_dataset.point_in_time_result_id == point_in_time.id
        assert qualification.metrics == {"net_sharpe": 1.2}
        assert qualification.qualification_metrics == {"net_sharpe": 1.2}
        assert signal.run_id == run.id
        assert episode.alpha_model_version_id == version.id

        duplicate = AlphaModelVersion(
            alpha_model_id=alpha.id,
            version_no=1,
            source_mission_id=mission.id,
            universe_version_id=universe.id,
            horizon="1D",
            mode="RELATIVE_SCORE",
            artifact_uri="alphas/duplicate.whl",
            entrypoint="duplicate:Alpha",
            parameters={},
            input_contract={},
            output_contract={},
            state="VALIDATED",
        )
        session.add(duplicate)
        with pytest.raises(IntegrityError):
            session.flush()


def _load_migration() -> object:
    path = (
        Path(__file__).resolve().parents[2]
        / "alembic"
        / "versions"
        / "0017_data_pit_and_true_alpha.py"
    )
    spec = importlib.util.spec_from_file_location("migration_0017", path)
    assert spec and spec.loader
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def test_0017_migration_preserves_legacy_rows_and_adds_alpha_tables(engine) -> None:
    legacy = MetaData()
    sources = Table("governed_data_sources", legacy, Column("id", Uuid, primary_key=True))
    universes = Table("market_universe_versions", legacy, Column("id", Uuid, primary_key=True))
    programs = Table("research_programs", legacy, Column("id", Uuid, primary_key=True))
    branches = Table("research_branches", legacy, Column("id", Uuid, primary_key=True))
    missions = Table("research_missions", legacy, Column("id", Uuid, primary_key=True))
    Table("quant_runtime_runs", legacy, Column("id", Uuid, primary_key=True))
    datasets = Table(
        "dataset_revisions",
        legacy,
        Column("id", Uuid, primary_key=True),
        Column("data_source_id", Uuid),
        Column("universe_version_id", Uuid),
        Column("universe_name", String(200)),
        Column("revision_no", Integer, nullable=False),
        Column("schema_version", String(100)),
        Column("event_start", DateTime(timezone=True)),
        Column("event_end", DateTime(timezone=True)),
        Column("available_start", DateTime(timezone=True)),
        Column("available_end", DateTime(timezone=True)),
        Column("row_count", Integer),
        Column("quality_state", String(40), nullable=False),
        Column("point_in_time_state", String(40), nullable=False),
        Column("partition", String(40), nullable=False),
        Column("created_at", DateTime(timezone=True), nullable=False),
    )
    qualifications = Table(
        "alpha_qualifications",
        legacy,
        Column("id", Uuid, primary_key=True),
        Column("program_id", Uuid),
        Column("alpha_model_version_id", Uuid),
        Column("calibration_version_id", Uuid),
        Column("universe_version_id", Uuid),
        Column("universe", String(200)),
        Column("horizon", String(100)),
        Column("role", String(100), nullable=False),
        Column("state", String(40), nullable=False),
        Column("name", String(240)),
        Column("scope_json", String, nullable=False),
        Column("evaluation_episode_id", Uuid),
        Column("degradation_state", String(40), nullable=False),
        Column("metrics", String, nullable=False),
        Column("lineage", String, nullable=False),
        Column("created_at", DateTime(timezone=True), nullable=False),
    )

    Base.metadata.drop_all(engine)
    legacy.create_all(engine)
    now = _now()
    source_id, universe_id, program_id, branch_id, mission_id, dataset_id, qualification_id = (
        uuid4(),
        uuid4(),
        uuid4(),
        uuid4(),
        uuid4(),
        uuid4(),
        uuid4(),
    )
    with engine.begin() as connection:
        connection.execute(sources.insert().values(id=source_id))
        connection.execute(universes.insert().values(id=universe_id))
        connection.execute(programs.insert().values(id=program_id))
        connection.execute(branches.insert().values(id=branch_id))
        connection.execute(missions.insert().values(id=mission_id))
        connection.execute(
            datasets.insert().values(
                id=dataset_id,
                data_source_id=source_id,
                universe_version_id=universe_id,
                universe_name="Legacy universe",
                revision_no=1,
                quality_state="VALID",
                point_in_time_state="VALID",
                partition="DISCOVERY",
                created_at=now,
            )
        )
        connection.execute(
            qualifications.insert().values(
                id=qualification_id,
                program_id=program_id,
                role="PRIMARY_ALPHA",
                state="ACTIVE",
                scope_json="{}",
                degradation_state="HEALTHY",
                metrics="{}",
                lineage="[]",
                created_at=now,
            )
        )
        context = MigrationContext.configure(connection)
        with Operations.context(context):
            _load_migration().upgrade()

        inspector = inspect(connection)
        assert {
            "data_quality_results",
            "feature_pipeline_versions",
            "alpha_models",
            "alpha_model_versions",
            "alpha_signal_artifacts",
            "alpha_calibration_versions",
            "alpha_evaluation_episodes",
        }.issubset(inspector.get_table_names())
        dataset_columns = {column["name"] for column in inspector.get_columns("dataset_revisions")}
        assert {
            "data_class",
            "origin",
            "ingested_at",
            "promotability",
            "quality_result_id",
            "point_in_time_result_id",
        }.issubset(dataset_columns)
        qualification_columns = {
            column["name"] for column in inspector.get_columns("alpha_qualifications")
        }
        assert {"alpha_model_id", "updated_at"}.issubset(qualification_columns)
        preserved = connection.execute(
            datasets.select().where(datasets.c.id == dataset_id)
        ).mappings().one()
        assert preserved["universe_name"] == "Legacy universe"
        assert preserved["revision_no"] == 1
        assert preserved["partition"] == "DISCOVERY"
