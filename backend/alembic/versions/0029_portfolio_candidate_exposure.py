"""Allow immutable evidence exposure to cross into Portfolio Candidates.

Revision ID: 0029_portfolio_candidate_exposure
Revises: 0028_trusted_production_chain
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "0029_portfolio_candidate_exposure"
down_revision = "0028_trusted_production_chain"
branch_labels = None
depends_on = None


_SUBJECT_TYPES = (
    "subject_type IN ('PROGRAM', 'BRANCH', 'MISSION', 'ALPHA_MODEL', "
    "'ALPHA_QUALIFICATION', 'PORTFOLIO_CANDIDATE')"
)
_LEGACY_SUBJECT_TYPES = (
    "subject_type IN ('PROGRAM', 'BRANCH', 'MISSION', 'ALPHA_MODEL', "
    "'ALPHA_QUALIFICATION')"
)


def upgrade() -> None:
    bind = op.get_bind()
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("evidence_exposures", recreate="always") as batch:
            batch.drop_constraint("ck_evidence_exposure_subject_type", type_="check")
            batch.create_check_constraint("ck_evidence_exposure_subject_type", _SUBJECT_TYPES)
        return
    op.drop_constraint("ck_evidence_exposure_subject_type", "evidence_exposures", type_="check")
    op.create_check_constraint(
        "ck_evidence_exposure_subject_type", "evidence_exposures", _SUBJECT_TYPES
    )


def downgrade() -> None:
    bind = op.get_bind()
    if bind.execute(
        sa.text(
            "SELECT 1 FROM evidence_exposures "
            "WHERE subject_type = 'PORTFOLIO_CANDIDATE' LIMIT 1"
        )
    ).first() is not None:
        raise RuntimeError("PORTFOLIO_CANDIDATE_EXPOSURE_DOWNGRADE_BLOCKED")
    if bind.dialect.name == "sqlite":
        with op.batch_alter_table("evidence_exposures", recreate="always") as batch:
            batch.drop_constraint("ck_evidence_exposure_subject_type", type_="check")
            batch.create_check_constraint(
                "ck_evidence_exposure_subject_type", _LEGACY_SUBJECT_TYPES
            )
        return
    op.drop_constraint("ck_evidence_exposure_subject_type", "evidence_exposures", type_="check")
    op.create_check_constraint(
        "ck_evidence_exposure_subject_type", "evidence_exposures", _LEGACY_SUBJECT_TYPES
    )
