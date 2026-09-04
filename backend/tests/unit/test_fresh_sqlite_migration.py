"""Exercise the Alembic history against a genuinely empty SQLite database."""

from __future__ import annotations

from pathlib import Path

import pytest
from alembic import command
from alembic.config import Config
from alembic.script import ScriptDirectory
from sqlalchemy import create_engine, inspect, text
from sqlalchemy.exc import IntegrityError


_BACKEND_ROOT = Path(__file__).parents[2]


def test_fresh_sqlite_database_reaches_head_with_current_schema(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    database_url = f"sqlite+pysqlite:///{tmp_path / 'fresh.sqlite'}"
    monkeypatch.setenv("QUAZONAI_ENV", "test")
    monkeypatch.setenv("QUAZONAI_DATABASE_URL", database_url)
    monkeypatch.setenv("QUAZONAI_ALEMBIC_URL", database_url)

    config = Config(str(_BACKEND_ROOT / "alembic.ini"))
    command.upgrade(config, "head")

    engine = create_engine(database_url)
    try:
        with engine.begin() as connection:
            assert connection.execute(text("SELECT version_num FROM alembic_version")).scalar_one() == (
                ScriptDirectory.from_config(config).get_current_head()
            )

            tables = set(inspect(connection).get_table_names())
            assert {
                "degradation_observations",
                "idea_drafts",
                "research_cycles",
                "research_wake_events",
                "portfolio_mandate_versions",
                "portfolio_evaluation_assignments",
                "portfolio_evaluation_episodes",
                "portfolio_evaluation_metrics",
                "portfolio_evaluation_gates",
                "portfolio_evaluation_disclosures",
                "promotion_evaluations",
                "promotion_gate_results",
                "feedback_contract_versions",
                "downstream_connection_versions",
                "feedback_packages",
                "forward_evidence_metrics",
            } <= tables
            assert "current_cycle_id" in {
                column["name"] for column in inspect(connection).get_columns("research_programs")
            }
            assert {
                "connector_key",
                "field_schema",
                "license_classification",
                "availability_semantics",
            } <= {
                column["name"]
                for column in inspect(connection).get_columns("governed_data_sources")
            }
            assert "materialization_request" in {
                column["name"] for column in inspect(connection).get_columns("dataset_revisions")
            }
            assert "revision" in {
                column["name"] for column in inspect(connection).get_columns("candidate_packages")
            }
            assert "revision" in {
                column["name"] for column in inspect(connection).get_columns("downstream_systems")
            }
            assert {
                "policy_family",
                "universe_version_id",
                "minimum_weight",
                "impact_breakpoint",
                "state",
            } <= {
                column["name"]
                for column in inspect(connection).get_columns("portfolio_mandate_versions")
            }
            assert "configuration_contract_version" in {
                column["name"] for column in inspect(connection).get_columns("capital_context_versions")
            }
            assert {"candidate_package_id", "candidate_package_revision"} <= {
                column["name"] for column in inspect(connection).get_columns("approval_snapshots")
            }
            assert {
                "promotion_evaluation_id",
                "promotion_purpose",
                "downstream_connection_version_id",
                "feedback_contract_version_id",
                "preflight_receipt_id",
                "paper_to_live_policy_version_id",
            } <= {
                column["name"] for column in inspect(connection).get_columns("approval_snapshots")
            }
            assert {
                "policy_contract_version",
                "paper_connection_version_id",
                "paper_feedback_contract_version_id",
                "paper_preflight_receipt_id",
                "live_connection_version_id",
                "live_feedback_contract_version_id",
                "live_preflight_receipt_id",
                "paper_to_live_policy_version_id",
            } <= {
                column["name"]
                for column in inspect(connection).get_columns("promotion_policy_versions")
            }
            promotion_columns = {
                column["name"] for column in inspect(connection).get_columns("promotion_evaluations")
            }
            assert "paper_to_live_policy_version_id" in promotion_columns
            promotion_fks = inspect(connection).get_foreign_keys("promotion_evaluations")
            assert any(
                foreign_key["constrained_columns"]
                == [
                    "downstream_connection_version_id",
                    "downstream_system_id",
                    "feedback_contract_version_id",
                ]
                for foreign_key in promotion_fks
            )
            approval_fks = inspect(connection).get_foreign_keys("approval_snapshots")
            assert any(
                foreign_key["constrained_columns"]
                == [
                    "promotion_evaluation_id",
                    "promotion_purpose",
                    "candidate_id",
                    "candidate_package_id",
                    "candidate_package_revision",
                    "downstream_system_id",
                    "downstream_connection_version_id",
                    "feedback_contract_version_id",
                    "preflight_receipt_id",
                    "paper_to_live_policy_version_id",
                ]
                for foreign_key in approval_fks
            )
            with pytest.raises(IntegrityError):
                connection.execute(
                    text(
                        "INSERT INTO promotion_policy_versions "
                        "(id, version_no, purpose, mode, policy_contract_version, state) "
                        "VALUES (:id, 1, 'PORTFOLIO_TO_PAPER', 'MANUAL_APPROVAL', "
                        "'PROMOTION_POLICY_V1', 'ACTIVE')"
                    ),
                    {"id": "d1e4d18f49c642fba58ab0c8d0b2b701"},
                )
            connection.execute(
                text(
                    "INSERT INTO promotion_policy_versions "
                    "(id, version_no, purpose, mode, paper_downstream_system_id, state) "
                    "VALUES (:id, 2, 'PORTFOLIO_TO_PAPER', 'MANUAL_APPROVAL', :paper, 'ACTIVE')"
                ),
                {
                    "id": "f06aa139ae2a4b66ab0efbc0345daed4",
                    "paper": "f06aa139ae2a4b66ab0efbc0345daed5",
                },
            )
            agent_turn_kind_check = next(
                check["sqltext"]
                for check in inspect(connection).get_check_constraints("agent_turns")
                if check["name"] == "ck_agent_turn_kind"
            )
            assert "VALIDATE" in agent_turn_kind_check
            assert "EXECUTE" in agent_turn_kind_check

            draft_id = "b75d4f6ac1ea4a6f9ee3a15f5e4a6dd3"
            connection.execute(
                text("INSERT INTO idea_drafts (id, original_idea_text) VALUES (:id, :idea)"),
                {"id": draft_id, "idea": "fresh migration smoke"},
            )
            assert connection.execute(
                text(
                    "SELECT state, clarification_round, revision "
                    "FROM idea_drafts WHERE id = :id"
                ),
                {"id": draft_id},
            ).one() == ("DRAFT", 0, 1)
    finally:
        engine.dispose()


def test_trusted_production_chain_empty_downgrade_and_v1_guard(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    database_url = f"sqlite+pysqlite:///{tmp_path / 'trusted-production.sqlite'}"
    monkeypatch.setenv("QUAZONAI_ENV", "test")
    monkeypatch.setenv("QUAZONAI_DATABASE_URL", database_url)
    monkeypatch.setenv("QUAZONAI_ALEMBIC_URL", database_url)
    config = Config(str(_BACKEND_ROOT / "alembic.ini"))

    command.upgrade(config, "head")
    command.downgrade(config, "0027_candidate_package_build")
    engine = create_engine(database_url)
    try:
        assert "promotion_evaluations" not in set(inspect(engine).get_table_names())
    finally:
        engine.dispose()

    command.upgrade(config, "head")
    engine = create_engine(database_url)
    try:
        with engine.begin() as connection:
            connection.execute(
                text(
                    "INSERT INTO jobs (id, kind, resource_type, resource_id, state, payload, attempt) "
                    "VALUES (:id, 'PORTFOLIO_INPUT_EVALUATION', "
                    "'portfolio_input_evaluation_assignment', :resource_id, 'SUCCEEDED', '{}', 0)"
                ),
                {
                    "id": "a1e5b2d94d6749c4a0a0e9cc5b9be010",
                    "resource_id": "a1e5b2d94d6749c4a0a0e9cc5b9be011",
                },
            )
    finally:
        engine.dispose()

    with pytest.raises(RuntimeError, match="TRUSTED_PRODUCTION_CHAIN_DOWNGRADE_BLOCKED"):
        command.downgrade(config, "0027_candidate_package_build")

    engine = create_engine(database_url)
    try:
        with engine.begin() as connection:
            connection.execute(
                text("DELETE FROM jobs WHERE id = :id"),
                {"id": "a1e5b2d94d6749c4a0a0e9cc5b9be010"},
            )
        with engine.begin() as connection:
            connection.execute(
                text(
                    "INSERT INTO promotion_policy_versions "
                    "(id, version_no, purpose, mode, policy_contract_version, "
                    "paper_downstream_system_id, paper_connection_version_id, "
                    "paper_feedback_contract_version_id, paper_preflight_receipt_id, "
                    "live_downstream_system_id, live_connection_version_id, "
                    "live_feedback_contract_version_id, live_preflight_receipt_id, state) "
                    "VALUES (:id, 1, 'PAPER_TO_LIVE', 'AUTO_HANDOFF', 'PROMOTION_POLICY_V1', "
                    ":paper_system, :paper_connection, :paper_contract, :paper_receipt, "
                    ":live_system, :live_connection, :live_contract, :live_receipt, 'ACTIVE')"
                ),
                {
                    "id": "a1e5b2d94d6749c4a0a0e9cc5b9be001",
                    "paper_system": "a1e5b2d94d6749c4a0a0e9cc5b9be002",
                    "paper_connection": "a1e5b2d94d6749c4a0a0e9cc5b9be003",
                    "paper_contract": "a1e5b2d94d6749c4a0a0e9cc5b9be004",
                    "paper_receipt": "a1e5b2d94d6749c4a0a0e9cc5b9be005",
                    "live_system": "a1e5b2d94d6749c4a0a0e9cc5b9be006",
                    "live_connection": "a1e5b2d94d6749c4a0a0e9cc5b9be007",
                    "live_contract": "a1e5b2d94d6749c4a0a0e9cc5b9be008",
                    "live_receipt": "a1e5b2d94d6749c4a0a0e9cc5b9be009",
                },
            )
    finally:
        engine.dispose()

    with pytest.raises(RuntimeError, match="TRUSTED_PRODUCTION_CHAIN_DOWNGRADE_BLOCKED"):
        command.downgrade(config, "0027_candidate_package_build")


def test_trusted_production_chain_sqlite_rejects_cross_handoff_feedback_contract(
    tmp_path: Path, monkeypatch: pytest.MonkeyPatch
) -> None:
    database_url = f"sqlite+pysqlite:///{tmp_path / 'trusted-feedback.sqlite'}"
    monkeypatch.setenv("QUAZONAI_ENV", "test")
    monkeypatch.setenv("QUAZONAI_DATABASE_URL", database_url)
    monkeypatch.setenv("QUAZONAI_ALEMBIC_URL", database_url)
    config = Config(str(_BACKEND_ROOT / "alembic.ini"))
    command.upgrade(config, "head")

    engine = create_engine(database_url)
    try:
        with engine.begin() as connection:
            connection.execute(
                text(
                    "INSERT INTO handoff_offers "
                    "(id, approval_id, candidate_package_id, candidate_id, purpose, "
                    "downstream_system_id, state, feedback_contract_snapshot) "
                    "VALUES (:id, :approval, :package, :candidate, 'PAPER', :downstream, "
                    "'AVAILABLE', '{}')"
                ),
                {
                    "id": "b2e5b2d94d6749c4a0a0e9cc5b9be001",
                    "approval": "b2e5b2d94d6749c4a0a0e9cc5b9be002",
                    "package": "b2e5b2d94d6749c4a0a0e9cc5b9be003",
                    "candidate": "b2e5b2d94d6749c4a0a0e9cc5b9be004",
                    "downstream": "b2e5b2d94d6749c4a0a0e9cc5b9be005",
                },
            )
        with engine.connect() as connection:
            connection.execute(text("PRAGMA foreign_keys = ON"))
            connection.commit()
            assert connection.execute(text("PRAGMA foreign_keys")).scalar_one() == 1
            with pytest.raises(IntegrityError):
                connection.execute(
                    text(
                        "INSERT INTO feedback_packages "
                        "(id, handoff_offer_id, feedback_contract_version_id, state, "
                        "observation_start, observation_end, sample_size, summary_json) "
                        "VALUES (:id, :handoff, :contract, 'COMPLETE', CURRENT_TIMESTAMP, "
                        "CURRENT_TIMESTAMP, 1, '{}')"
                    ),
                    {
                        "id": "b2e5b2d94d6749c4a0a0e9cc5b9be006",
                        "handoff": "b2e5b2d94d6749c4a0a0e9cc5b9be001",
                        "contract": "b2e5b2d94d6749c4a0a0e9cc5b9be007",
                    },
                )
    finally:
        engine.dispose()
