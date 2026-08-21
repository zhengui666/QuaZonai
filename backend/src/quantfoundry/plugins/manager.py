"""Plugin release lifecycle operations owned by the control plane."""

from __future__ import annotations

from datetime import UTC, datetime
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from quantfoundry.db.models import PluginRelease
from quantfoundry.errors import QfError


def activate_release(session: Session, release_id: UUID) -> PluginRelease:
    release = session.execute(
        select(PluginRelease).where(PluginRelease.id == release_id).with_for_update()
    ).scalar_one_or_none()
    if release is None:
        raise QfError("PLUGIN_UNKNOWN", "Plugin release does not exist.", 404)
    if release.state not in {"STAGED", "INACTIVE"}:
        raise QfError(
            "PLUGIN_INVALID_STATE",
            "Only STAGED or INACTIVE plugin releases can be activated.",
            409,
            {"state": release.state},
        )

    other_defaults = list(
        session.scalars(
            select(PluginRelease)
            .where(
                PluginRelease.plugin_id == release.plugin_id,
                PluginRelease.id != release.id,
                PluginRelease.is_default.is_(True),
            )
            .with_for_update()
        )
    )
    for previous in other_defaults:
        previous.is_default = False
        if previous.state == "ACTIVE":
            previous.state = "DRAINING"

    # Release the database-enforced default slot before assigning it to the new version.
    # This preserves the invariant on databases that check the partial unique index row-by-row.
    if other_defaults:
        session.flush()

    release.state = "ACTIVE"
    release.is_default = True
    release.activated_at = datetime.now(UTC)
    session.flush()
    return release


def deactivate_release(session: Session, release_id: UUID) -> PluginRelease:
    release = session.execute(
        select(PluginRelease).where(PluginRelease.id == release_id).with_for_update()
    ).scalar_one_or_none()
    if release is None:
        raise QfError("PLUGIN_UNKNOWN", "Plugin release does not exist.", 404)
    if release.state != "ACTIVE":
        raise QfError(
            "PLUGIN_INVALID_STATE",
            "Only ACTIVE plugin releases can begin draining.",
            409,
            {"state": release.state},
        )
    release.state = "DRAINING"
    release.is_default = False
    session.flush()
    return release
