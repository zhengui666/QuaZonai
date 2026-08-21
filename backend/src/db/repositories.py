"""Small repository helpers used by the P0/P1 control plane."""

from __future__ import annotations

from collections import defaultdict
from typing import Any
from uuid import UUID

from sqlalchemy import Select, select
from sqlalchemy.orm import Session

from db.models import PluginRelease


def plugin_release_query() -> Select[tuple[PluginRelease]]:
    return select(PluginRelease).order_by(
        PluginRelease.plugin_id.asc(),
        PluginRelease.created_at.desc(),
    )


def list_plugin_releases(session: Session) -> list[PluginRelease]:
    return list(session.scalars(plugin_release_query()))


def get_plugin_release(session: Session, release_id: UUID) -> PluginRelease | None:
    return session.get(PluginRelease, release_id)


def plugin_catalog(session: Session) -> list[dict[str, Any]]:
    grouped: dict[str, list[PluginRelease]] = defaultdict(list)
    for release in list_plugin_releases(session):
        grouped[release.plugin_id].append(release)

    result: list[dict[str, Any]] = []
    for plugin_id in sorted(grouped):
        releases = grouped[plugin_id]
        active = next(
            (item for item in releases if item.state == "ACTIVE" and item.is_default),
            None,
        )
        result.append(
            {
                "plugin_id": plugin_id,
                "active_release_id": str(active.id) if active else None,
                "capabilities": (
                    active.descriptor_snapshot.get("capabilities", []) if active else []
                ),
                "releases": [
                    {
                        "id": str(item.id),
                        "version": item.version,
                        "state": item.state,
                        "is_default": item.is_default,
                    }
                    for item in releases
                ],
            }
        )
    return result
