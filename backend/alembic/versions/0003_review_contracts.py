"""Add review-enforced autonomous research and handoff contract fields.

Revision ID: 0003_review_contracts
Revises: 0002_research_boundary
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op

revision = "0003_review_contracts"
down_revision = "0002_research_boundary"
branch_labels = None
depends_on = None


def _columns(bind: sa.Connection, table: str) -> set[str]:
    return {column["name"] for column in sa.inspect(bind).get_columns(table)}


def _tables(bind: sa.Connection) -> set[str]:
    return set(sa.inspect(bind).get_table_names())


def _json_type(bind: sa.Connection) -> sa.types.TypeEngine[object]:
    if bind.dialect.name == "postgresql":
        from sqlalchemy.dialects.postgresql import JSONB

        return JSONB()
    return sa.JSON()


def upgrade() -> None:
    bind = op.get_bind()
    tables = _tables(bind)

    if "idea_contributions" not in tables:
        op.create_table(
            "idea_contributions",
            sa.Column("id", sa.Uuid(), primary_key=True, nullable=False),
            sa.Column(
                "program_id",
                sa.Uuid(),
                sa.ForeignKey("research_programs.id", ondelete="CASCADE"),
                nullable=False,
            ),
            sa.Column("idea_text", sa.Text(), nullable=False),
            sa.Column("action", sa.String(length=80), nullable=False),
            sa.Column("created_at", sa.DateTime(timezone=True), nullable=False),
        )
        op.create_index(
            "ix_idea_contribution_program",
            "idea_contributions",
            ["program_id", "created_at"],
        )

    program_columns = _columns(bind, "research_programs")
    if "source_program_id" not in program_columns:
        op.add_column("research_programs", sa.Column("source_program_id", sa.Uuid(), nullable=True))
        op.create_foreign_key(
            "fk_research_program_source_program",
            "research_programs",
            "research_programs",
            ["source_program_id"],
            ["id"],
            ondelete="RESTRICT",
        )
    if "relationship_type" not in program_columns:
        op.add_column(
            "research_programs",
            sa.Column("relationship_type", sa.String(length=80), nullable=True),
        )
    if "evidence_inherited_from_program_id" not in program_columns:
        op.add_column(
            "research_programs",
            sa.Column("evidence_inherited_from_program_id", sa.Uuid(), nullable=True),
        )
        op.create_foreign_key(
            "fk_research_program_evidence_parent",
            "research_programs",
            "research_programs",
            ["evidence_inherited_from_program_id"],
            ["id"],
            ondelete="RESTRICT",
        )

    mission_columns = _columns(bind, "research_missions")
    if "codex_thread_id" not in mission_columns:
        op.add_column(
            "research_missions",
            sa.Column("codex_thread_id", sa.String(length=200), nullable=True),
        )
    if "workspace_path" not in mission_columns:
        op.add_column("research_missions", sa.Column("workspace_path", sa.Text(), nullable=True))

    downstream_columns = _columns(bind, "downstream_systems")
    if "service_token_ciphertext" not in downstream_columns:
        op.add_column(
            "downstream_systems",
            sa.Column("service_token_ciphertext", sa.LargeBinary(), nullable=True),
        )
    if "service_token_nonce" not in downstream_columns:
        op.add_column(
            "downstream_systems",
            sa.Column("service_token_nonce", sa.LargeBinary(), nullable=True),
        )
    if "service_token_key_version" not in downstream_columns:
        op.add_column(
            "downstream_systems",
            sa.Column("service_token_key_version", sa.Integer(), nullable=True),
        )

    # ``0001_initial`` intentionally creates current Base.metadata for fresh
    # development databases.  Before Issue #22 that table was named
    # candidate_packages; on a fresh Nautilus-first database it is already
    # candidate_bundles.  Keep this historical migration idempotent for both
    # shapes without re-introducing a runtime compatibility layer.
    package_table = "candidate_packages" if "candidate_packages" in tables else "candidate_bundles"
    package_offer_column = (
        "candidate_package_id" if package_table == "candidate_packages" else "candidate_bundle_id"
    )
    package_columns = _columns(bind, package_table)
    if "state" not in package_columns:
        op.add_column(
            package_table,
            sa.Column(
                "state",
                sa.String(length=40),
                nullable=False,
                server_default="LEGACY_NON_EXECUTABLE",
            ),
        )
    if "manifest_json" not in package_columns:
        op.add_column(
            package_table,
            sa.Column(
                "manifest_json",
                _json_type(bind),
                nullable=False,
                server_default=sa.text("'{}'"),
            ),
        )
    if "relative_path" not in package_columns:
        op.add_column(
            package_table,
            sa.Column("relative_path", sa.Text(), nullable=False, server_default=""),
        )
        if package_table == "candidate_packages":
            op.execute(
                "UPDATE handoff_offers SET state = 'REVOKED', "
                "stale_reason = 'Legacy Candidate Package is not executable under the current contract' "
                f"WHERE {package_offer_column} IN (SELECT id FROM {package_table}) "
                "AND state IN ('APPROVED','PUBLISHING','AVAILABLE')"
            )

    handoff_columns = _columns(bind, "handoff_offers")
    if "feedback_contract_snapshot" not in handoff_columns:
        op.add_column(
            "handoff_offers",
            sa.Column(
                "feedback_contract_snapshot",
                _json_type(bind),
                nullable=False,
                server_default=sa.text("'{}'"),
            ),
        )

    evidence_columns = _columns(bind, "forward_evidence_episodes")
    added_observation = False
    if "observation_start" not in evidence_columns:
        op.add_column(
            "forward_evidence_episodes",
            sa.Column("observation_start", sa.DateTime(timezone=True), nullable=True),
        )
        added_observation = True
    if "observation_end" not in evidence_columns:
        op.add_column(
            "forward_evidence_episodes",
            sa.Column("observation_end", sa.DateTime(timezone=True), nullable=True),
        )
        added_observation = True
    if "sample_size" not in evidence_columns:
        op.add_column(
            "forward_evidence_episodes",
            sa.Column("sample_size", sa.Integer(), nullable=True),
        )
        added_observation = True
    if added_observation:
        op.execute(
            "UPDATE forward_evidence_episodes SET state = 'LEGACY_UNVALIDATED', "
            "observation_start = COALESCE(observation_start, created_at), "
            "observation_end = COALESCE(observation_end, created_at), "
            "sample_size = COALESCE(sample_size, 0)"
        )
        op.alter_column("forward_evidence_episodes", "observation_start", nullable=False)
        op.alter_column("forward_evidence_episodes", "observation_end", nullable=False)
        op.alter_column("forward_evidence_episodes", "sample_size", nullable=False)


def downgrade() -> None:
    raise RuntimeError(
        "0003_review_contracts is intentionally irreversible because it establishes "
        "credential, package, lineage, and validated-forward-evidence contracts."
    )
