"""Reserve and recover typed Candidate Package builds.

Revision ID: 0027_candidate_package_build
Revises: 0026_portfolio_assembly_input
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "0027_candidate_package_build"
down_revision = "0026_portfolio_assembly_input"
branch_labels = None
depends_on = None


_PACKAGE_STATES = "'LEGACY_NON_EXECUTABLE', 'STALE', 'BUILDING', 'AVAILABLE'"


def _require_compatible_history() -> None:
    """Fail closed rather than rewriting immutable historical Package facts."""
    bind = op.get_bind()
    if bind.execute(
        sa.text(
            "SELECT 1 FROM candidate_packages "
            f"WHERE state IS NULL OR state NOT IN ({_PACKAGE_STATES}) "
            "OR revision IS NULL OR revision <= 0 LIMIT 1"
        )
    ).first() is not None:
        raise RuntimeError("CANDIDATE_PACKAGE_LEGACY_STATE_CONFLICT")
    if bind.execute(
        sa.text(
            "SELECT 1 FROM candidate_packages "
            "GROUP BY candidate_id, revision HAVING count(*) > 1 LIMIT 1"
        )
    ).first() is not None:
        raise RuntimeError("CANDIDATE_PACKAGE_LEGACY_IDENTITY_CONFLICT")
    if bind.execute(
        sa.text(
            "SELECT 1 FROM candidate_packages WHERE state = 'BUILDING' "
            "GROUP BY candidate_id HAVING count(*) > 1 LIMIT 1"
        )
    ).first() is not None:
        raise RuntimeError("CANDIDATE_PACKAGE_LEGACY_BUILD_CONFLICT")
    if bind.execute(
        sa.text(
            "SELECT 1 FROM jobs WHERE kind = 'CANDIDATE_PACKAGE_BUILD' "
            "AND resource_type = 'portfolio_candidate' AND state IN ('READY', 'LEASED') "
            "GROUP BY resource_id HAVING count(*) > 1 LIMIT 1"
        )
    ).first() is not None:
        raise RuntimeError("CANDIDATE_PACKAGE_ACTIVE_JOB_CONFLICT")


def _add_constraints() -> None:
    bind = op.get_bind()
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("candidate_packages", recreate="always") as batch:
            batch.create_unique_constraint(
                "uq_candidate_package_candidate_revision", ["candidate_id", "revision"]
            )
            batch.create_check_constraint("ck_candidate_package_revision", "revision > 0")
            batch.create_check_constraint(
                "ck_candidate_package_state", f"state IN ({_PACKAGE_STATES})"
            )
        op.create_index(
            "uq_candidate_package_building_candidate",
            "candidate_packages",
            ["candidate_id"],
            unique=True,
            sqlite_where=sa.text("state = 'BUILDING'"),
            postgresql_where=sa.text("state = 'BUILDING'"),
        )
        op.create_index(
            "uq_candidate_package_build_job_active",
            "jobs",
            ["resource_id"],
            unique=True,
            sqlite_where=sa.text(
                "kind = 'CANDIDATE_PACKAGE_BUILD' "
                "AND resource_type = 'portfolio_candidate' "
                "AND state IN ('READY', 'LEASED')"
            ),
            postgresql_where=sa.text(
                "kind = 'CANDIDATE_PACKAGE_BUILD' "
                "AND resource_type = 'portfolio_candidate' "
                "AND state IN ('READY', 'LEASED')"
            ),
        )
        return

    op.create_unique_constraint(
        "uq_candidate_package_candidate_revision",
        "candidate_packages",
        ["candidate_id", "revision"],
    )
    op.create_check_constraint("ck_candidate_package_revision", "candidate_packages", "revision > 0")
    op.create_check_constraint(
        "ck_candidate_package_state", "candidate_packages", f"state IN ({_PACKAGE_STATES})"
    )
    op.create_index(
        "uq_candidate_package_building_candidate",
        "candidate_packages",
        ["candidate_id"],
        unique=True,
        sqlite_where=sa.text("state = 'BUILDING'"),
        postgresql_where=sa.text("state = 'BUILDING'"),
    )
    op.create_index(
        "uq_candidate_package_build_job_active",
        "jobs",
        ["resource_id"],
        unique=True,
        sqlite_where=sa.text(
            "kind = 'CANDIDATE_PACKAGE_BUILD' "
            "AND resource_type = 'portfolio_candidate' "
            "AND state IN ('READY', 'LEASED')"
        ),
        postgresql_where=sa.text(
            "kind = 'CANDIDATE_PACKAGE_BUILD' "
            "AND resource_type = 'portfolio_candidate' "
            "AND state IN ('READY', 'LEASED')"
        ),
    )


def upgrade() -> None:
    _require_compatible_history()
    _add_constraints()


def _require_no_typed_package_facts() -> None:
    bind = op.get_bind()
    if bind.execute(
        sa.text(
            "SELECT 1 FROM candidate_packages AS package "
            "JOIN portfolio_candidates AS candidate ON candidate.id = package.candidate_id "
            "WHERE candidate.assembly_input_id IS NOT NULL LIMIT 1"
        )
    ).first() is not None:
        raise RuntimeError("CANDIDATE_PACKAGE_BUILD_DOWNGRADE_BLOCKED")
    if bind.execute(
        sa.text(
            "SELECT 1 FROM jobs WHERE kind = 'CANDIDATE_PACKAGE_BUILD' "
            "AND resource_type = 'portfolio_candidate' "
            "AND state IN ('READY', 'LEASED') LIMIT 1"
        )
    ).first() is not None:
        raise RuntimeError("CANDIDATE_PACKAGE_BUILD_DOWNGRADE_BLOCKED")


def downgrade() -> None:
    _require_no_typed_package_facts()
    bind = op.get_bind()
    op.drop_index("uq_candidate_package_build_job_active", table_name="jobs")
    op.drop_index("uq_candidate_package_building_candidate", table_name="candidate_packages")
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("candidate_packages", recreate="always") as batch:
            batch.drop_constraint("ck_candidate_package_state", type_="check")
            batch.drop_constraint("ck_candidate_package_revision", type_="check")
            batch.drop_constraint("uq_candidate_package_candidate_revision", type_="unique")
        return
    op.drop_constraint("ck_candidate_package_state", "candidate_packages", type_="check")
    op.drop_constraint("ck_candidate_package_revision", "candidate_packages", type_="check")
    op.drop_constraint(
        "uq_candidate_package_candidate_revision", "candidate_packages", type_="unique"
    )
