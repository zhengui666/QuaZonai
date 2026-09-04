"""Persist the required visible Mission validation and execution turns.

Revision ID: 0019_visible_mission_turns
Revises: 0018_configuration_facts
"""

from __future__ import annotations

from alembic import op


revision = "0019_visible_mission_turns"
down_revision = "0018_configuration_facts"
branch_labels = None
depends_on = None

_NEW_KIND_CHECK = (
    "kind IN ('PLAN', 'IMPLEMENT', 'VALIDATE', 'EXECUTE', 'REPAIR', 'REVIEW', "
    "'REPLAN', 'DIAGNOSE')"
)
_OLD_KIND_CHECK = "kind IN ('PLAN', 'IMPLEMENT', 'REPAIR', 'REVIEW', 'REPLAN', 'DIAGNOSE')"


def _replace_kind_check(expression: str) -> None:
    if op.get_bind().dialect.name == "sqlite":
        with op.batch_alter_table("agent_turns", recreate="always") as batch:
            batch.drop_constraint("ck_agent_turn_kind", type_="check")
            batch.create_check_constraint("ck_agent_turn_kind", expression)
        return
    op.drop_constraint("ck_agent_turn_kind", "agent_turns", type_="check")
    op.create_check_constraint("ck_agent_turn_kind", "agent_turns", expression)


def upgrade() -> None:
    _replace_kind_check(_NEW_KIND_CHECK)


def downgrade() -> None:
    _replace_kind_check(_OLD_KIND_CHECK)
