from __future__ import annotations

from dataclasses import replace

import pytest
from sqlalchemy import Engine

from main import create_app
from settings import Settings, SettingsError


def test_app_startup_rejects_production_without_operator_auth(
    settings: Settings,
    engine: Engine,
) -> None:
    production = replace(
        settings,
        environment="production",
        operator_auth_enabled=False,
    )

    with pytest.raises(SettingsError, match="must be enabled in production"):
        create_app(settings=production, engine=engine)


def test_app_startup_rejects_partial_disabled_auth_configuration(
    settings: Settings,
    engine: Engine,
) -> None:
    partial = replace(
        settings,
        operator_auth_enabled=False,
        operator_username="operator",
    )

    with pytest.raises(SettingsError, match="disabled but authentication values are configured"):
        create_app(settings=partial, engine=engine)
