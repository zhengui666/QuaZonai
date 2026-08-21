from __future__ import annotations

import base64

from settings import Settings, SettingsError


def test_master_key_requires_exactly_32_decoded_bytes(settings: Settings) -> None:
    assert settings.master_key_configured is True

    invalid = Settings(
        **{
            **{field: getattr(settings, field) for field in settings.__dataclass_fields__},
            "master_key": base64.b64encode(b"short").decode("ascii"),
        }
    )
    assert invalid.master_key_configured is False


def test_database_scheme_rejects_remote_style_unknown_driver(settings: Settings) -> None:
    invalid = Settings(
        **{
            **{field: getattr(settings, field) for field in settings.__dataclass_fields__},
            "database_url": "mysql://localhost/quazonai",
        }
    )
    try:
        invalid.validate_database_scheme()
    except SettingsError as exc:
        assert "postgresql+psycopg" in str(exc)
    else:
        raise AssertionError("unsupported database scheme should fail")
