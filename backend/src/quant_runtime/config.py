"""Bootstrap configuration for an independently deployed NautilusTrader runtime."""

from __future__ import annotations

import os
from dataclasses import dataclass
from typing import Literal
from urllib.parse import urlparse

from errors import QfError

PINNED_NAUTILUS_VERSION = "1.231.0"
CONTRACT_VERSION = "1"
_DEFAULT_TIMEOUT_SECONDS = 120.0
_DEFAULT_POLL_SECONDS = 0.25
RuntimeProfile = Literal["research", "sealed"]


def _optional_env(name: str) -> str | None:
    value = os.environ.get(name)
    if value is None:
        return None
    clean = value.strip()
    return clean or None


def _positive_float(name: str, default: float) -> float:
    raw = _optional_env(name)
    if raw is None:
        return default
    try:
        value = float(raw)
    except ValueError as exc:
        raise QfError(
            "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
            f"{name} must be a positive number.",
            500,
        ) from exc
    if value <= 0:
        raise QfError(
            "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
            f"{name} must be a positive number.",
            500,
        )
    return value


def _bool_env(name: str, default: bool) -> bool:
    raw = _optional_env(name)
    if raw is None:
        return default
    value = raw.casefold()
    if value in {"1", "true", "yes", "on"}:
        return True
    if value in {"0", "false", "no", "off"}:
        return False
    raise QfError(
        "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
        f"{name} must be true or false.",
        500,
    )


def _validate_url(value: str, *, variable_name: str) -> str:
    parsed = urlparse(value)
    if parsed.scheme not in {"http", "https"} or not parsed.netloc:
        raise QfError(
            "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
            f"{variable_name} must be an absolute HTTP(S) URL.",
            500,
        )
    if parsed.username or parsed.password or parsed.query or parsed.fragment:
        raise QfError(
            "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
            f"{variable_name} must not contain credentials, query parameters, or a fragment.",
            500,
        )
    environment = (_optional_env("QUAZONAI_ENV") or "development").casefold()
    if environment == "production" and parsed.scheme != "https":
        raise QfError(
            "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
            f"A production {variable_name} must use HTTPS.",
            500,
        )
    return value.rstrip("/")


@dataclass(frozen=True, slots=True)
class RemoteNautilusConfig:
    """Connection settings for the remote canonical quant runtime.

    The token authenticates QuaZonai to the runtime service. It is not a broker or
    exchange credential and is never exposed to a Codex Mission child.
    """

    base_url: str
    service_token: str | None
    pinned_version: str = PINNED_NAUTILUS_VERSION
    contract_version: str = CONTRACT_VERSION
    timeout_seconds: float = _DEFAULT_TIMEOUT_SECONDS
    poll_seconds: float = _DEFAULT_POLL_SECONDS
    verify_tls: bool = True

    @classmethod
    def from_env(
        cls,
        *,
        required: bool = False,
        profile: RuntimeProfile = "research",
    ) -> RemoteNautilusConfig | None:
        prefix = "" if profile == "research" else "SEALED_"
        url_name = f"QUAZONAI_NAUTILUS_{prefix}RUNTIME_URL"
        token_name = f"QUAZONAI_NAUTILUS_{prefix}RUNTIME_TOKEN"
        raw_url = _optional_env(url_name)
        if raw_url is None:
            if required:
                raise QfError(
                    "NAUTILUS_RUNTIME_NOT_CONFIGURED",
                    f"A remote NautilusTrader {profile} runtime is required.",
                    503,
                )
            return None

        token = _optional_env(token_name)
        environment = (_optional_env("QUAZONAI_ENV") or "development").casefold()
        if environment == "production" and token is None:
            raise QfError(
                "NAUTILUS_RUNTIME_CONFIGURATION_INVALID",
                f"{token_name} is required in production.",
                500,
            )

        pinned = _optional_env("QUAZONAI_NAUTILUS_VERSION") or PINNED_NAUTILUS_VERSION
        contract = _optional_env("QUAZONAI_NAUTILUS_CONTRACT_VERSION") or CONTRACT_VERSION
        return cls(
            base_url=_validate_url(raw_url, variable_name=url_name),
            service_token=token,
            pinned_version=pinned,
            contract_version=contract,
            timeout_seconds=_positive_float(
                "QUAZONAI_NAUTILUS_RUNTIME_TIMEOUT_SECONDS",
                _DEFAULT_TIMEOUT_SECONDS,
            ),
            poll_seconds=_positive_float(
                "QUAZONAI_NAUTILUS_RUNTIME_POLL_SECONDS",
                _DEFAULT_POLL_SECONDS,
            ),
            verify_tls=_bool_env("QUAZONAI_NAUTILUS_RUNTIME_VERIFY_TLS", True),
        )
