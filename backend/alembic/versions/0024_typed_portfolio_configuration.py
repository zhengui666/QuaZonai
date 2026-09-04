"""Make V1 portfolio configuration explicit and immutable.

Revision ID: 0024_typed_portfolio_configuration
Revises: 0023_trusted_alpha_evaluation
"""

from __future__ import annotations

import sqlalchemy as sa
from alembic import op


revision = "0024_typed_portfolio_configuration"
down_revision = "0023_trusted_alpha_evaluation"
branch_labels = None
depends_on = None


_V1_TUPLE = (
    "(policy_family IS NULL AND universe_version_id IS NULL "
    "AND eligible_alpha_role IS NULL AND minimum_weight IS NULL "
    "AND maximum_weight IS NULL AND gross_exposure_limit IS NULL "
    "AND net_exposure_target IS NULL AND cash_reserve IS NULL "
    "AND turnover_limit IS NULL AND variance_limit IS NULL "
    "AND risk_aversion IS NULL AND cost_aversion IS NULL "
    "AND uncertainty_aversion IS NULL AND commission_rate IS NULL "
    "AND half_spread_rate IS NULL AND slippage_rate IS NULL "
    "AND impact_rate IS NULL AND impact_breakpoint IS NULL AND state IS NULL) "
    "OR (policy_family = 'LONG_ONLY_MEAN_VARIANCE_V1' "
    "AND objective = 'MAXIMIZE_NET_RETURN' "
    "AND universe_version_id IS NOT NULL "
    "AND eligible_alpha_role = 'PRIMARY_ALPHA' "
    "AND minimum_weight IS NOT NULL AND maximum_weight IS NOT NULL "
    "AND gross_exposure_limit IS NOT NULL AND net_exposure_target IS NOT NULL "
    "AND cash_reserve IS NOT NULL AND turnover_limit IS NOT NULL "
    "AND variance_limit IS NOT NULL AND risk_aversion IS NOT NULL "
    "AND cost_aversion IS NOT NULL AND uncertainty_aversion IS NOT NULL "
    "AND commission_rate IS NOT NULL AND half_spread_rate IS NOT NULL "
    "AND slippage_rate IS NOT NULL AND impact_rate IS NOT NULL "
    "AND impact_breakpoint IS NOT NULL AND state IN ('ACTIVE', 'RETIRED') "
    "AND minimum_weight >= 0 AND maximum_weight > 0 AND maximum_weight <= 1 "
    "AND minimum_weight <= maximum_weight "
    "AND minimum_weight * minimum_alpha_count <= 1 "
    "AND maximum_weight * minimum_alpha_count >= 1 "
    "AND gross_exposure_limit = 1 AND net_exposure_target = 1 "
    "AND cash_reserve = 0 AND turnover_limit >= 1 AND turnover_limit <= 2 "
    "AND variance_limit > 0 AND risk_aversion >= 0 AND cost_aversion >= 0 "
    "AND uncertainty_aversion >= 0 AND commission_rate >= 0 AND commission_rate <= 1 "
    "AND half_spread_rate >= 0 AND half_spread_rate <= 1 "
    "AND slippage_rate >= 0 AND slippage_rate <= 1 "
    "AND impact_rate >= 0 AND impact_rate <= 1 "
    "AND impact_breakpoint >= 0 AND impact_breakpoint <= 1)"
)
_CAPITAL_V1 = (
    "configuration_contract_version IS NULL OR "
    "(configuration_contract_version = 'CAPITAL_CONTEXT_V1' "
    "AND source_type = 'ADMIN' AND source_downstream_system_id IS NULL "
    "AND deployable_capital > 0 AND observed_at < valid_until)"
)


def _mandate_columns() -> tuple[sa.Column[object], ...]:
    numeric = sa.Numeric(20, 8)
    return (
        sa.Column("policy_family", sa.String(length=40)),
        sa.Column("universe_version_id", sa.Uuid()),
        sa.Column("eligible_alpha_role", sa.String(length=40)),
        sa.Column("minimum_weight", numeric),
        sa.Column("maximum_weight", numeric),
        sa.Column("gross_exposure_limit", numeric),
        sa.Column("net_exposure_target", numeric),
        sa.Column("cash_reserve", numeric),
        sa.Column("turnover_limit", numeric),
        sa.Column("variance_limit", numeric),
        sa.Column("risk_aversion", numeric),
        sa.Column("cost_aversion", numeric),
        sa.Column("uncertainty_aversion", numeric),
        sa.Column("commission_rate", numeric),
        sa.Column("half_spread_rate", numeric),
        sa.Column("slippage_rate", numeric),
        sa.Column("impact_rate", numeric),
        sa.Column("impact_breakpoint", numeric),
        sa.Column("state", sa.String(length=20)),
    )


def _add_typed_mandate_configuration() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("portfolio_mandate_versions", recreate="always") as batch:
            for column in _mandate_columns():
                batch.add_column(column)
            batch.create_foreign_key(
                "fk_portfolio_mandate_version_universe",
                "market_universe_versions",
                ["universe_version_id"],
                ["id"],
                ondelete="RESTRICT",
            )
            batch.create_check_constraint("ck_portfolio_mandate_v1_complete", _V1_TUPLE)
        return
    for column in _mandate_columns():
        op.add_column("portfolio_mandate_versions", column)
    op.create_foreign_key(
        "fk_portfolio_mandate_version_universe",
        "portfolio_mandate_versions",
        "market_universe_versions",
        ["universe_version_id"],
        ["id"],
        ondelete="RESTRICT",
    )
    op.create_check_constraint(
        "ck_portfolio_mandate_v1_complete", "portfolio_mandate_versions", _V1_TUPLE
    )


def _add_capital_contract() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("capital_context_versions", recreate="always") as batch:
            batch.add_column(sa.Column("configuration_contract_version", sa.String(length=40)))
            batch.create_check_constraint("ck_capital_context_v1_complete", _CAPITAL_V1)
        return
    op.add_column(
        "capital_context_versions",
        sa.Column("configuration_contract_version", sa.String(length=40)),
    )
    op.create_check_constraint(
        "ck_capital_context_v1_complete", "capital_context_versions", _CAPITAL_V1
    )


def _widen_alembic_version_for_long_revision() -> None:
    """PostgreSQL's default Alembic version column is too short for this revision ID."""
    if op.get_bind().dialect.name == "postgresql":
        op.execute(sa.text("ALTER TABLE alembic_version ALTER COLUMN version_num TYPE VARCHAR(64)"))


def upgrade() -> None:
    _widen_alembic_version_for_long_revision()
    _add_typed_mandate_configuration()
    _add_capital_contract()


def _require_legacy_only_for_downgrade() -> None:
    bind = op.get_bind()
    if bind.execute(
        sa.text("SELECT 1 FROM portfolio_mandate_versions WHERE policy_family IS NOT NULL LIMIT 1")
    ).first() is not None or bind.execute(
        sa.text(
            "SELECT 1 FROM capital_context_versions "
            "WHERE configuration_contract_version IS NOT NULL LIMIT 1"
        )
    ).first() is not None:
        raise RuntimeError("TYPED_PORTFOLIO_CONFIGURATION_DOWNGRADE_BLOCKED")


def _drop_typed_mandate_configuration() -> None:
    columns = tuple(column.name for column in _mandate_columns())
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("portfolio_mandate_versions", recreate="always") as batch:
            batch.drop_constraint("fk_portfolio_mandate_version_universe", type_="foreignkey")
            batch.drop_constraint("ck_portfolio_mandate_v1_complete", type_="check")
            for column in reversed(columns):
                batch.drop_column(column)
        return
    op.drop_constraint(
        "fk_portfolio_mandate_version_universe",
        "portfolio_mandate_versions",
        type_="foreignkey",
    )
    op.drop_constraint(
        "ck_portfolio_mandate_v1_complete", "portfolio_mandate_versions", type_="check"
    )
    for column in reversed(columns):
        op.drop_column("portfolio_mandate_versions", column)


def _drop_capital_contract() -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("capital_context_versions", recreate="always") as batch:
            batch.drop_constraint("ck_capital_context_v1_complete", type_="check")
            batch.drop_column("configuration_contract_version")
        return
    op.drop_constraint(
        "ck_capital_context_v1_complete", "capital_context_versions", type_="check"
    )
    op.drop_column("capital_context_versions", "configuration_contract_version")


def downgrade() -> None:
    _require_legacy_only_for_downgrade()
    _drop_capital_contract()
    _drop_typed_mandate_configuration()
