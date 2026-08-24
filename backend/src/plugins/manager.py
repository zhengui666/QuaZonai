"""Research/data plugin release lifecycle operations owned by the control plane."""

from __future__ import annotations

from datetime import UTC, datetime
from uuid import UUID

from sqlalchemy import select
from sqlalchemy.orm import Session

from db.models import PluginRelease
from errors import QfError

_ALLOWED_CAPABILITIES = {"HISTORICAL_IMPORT", "LIVE_DATA", "RESEARCH_TOOL"}


def _require_research_data_release(release: PluginRelease) -> None:
    raw = release.descriptor_snapshot.get("capabilities", [])
    capabilities = {str(item) for item in raw} if isinstance(raw, list) else set()
    unsupported = sorted(capabilities - _ALLOWED_CAPABILITIES)
    if unsupported:
        raise QfError(
            "PLUGIN_CAPABILITY_FORBIDDEN",
            "Plugin declares capabilities outside the QuaZonai research/data boundary.",
            422,
            {"unsupported_capabilities": unsupported},
        )


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
    _require_research_data_release(release)

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
