from __future__ import annotations

import pytest

from cli.client import CliClientError, validate_loopback_endpoint


def test_loopback_endpoint_is_accepted() -> None:
    assert validate_loopback_endpoint("http://127.0.0.1:8000/") == "http://127.0.0.1:8000"


def test_remote_endpoint_is_rejected() -> None:
    with pytest.raises(CliClientError, match="REMOTE_API_ENDPOINT_FORBIDDEN"):
        validate_loopback_endpoint("https://quazonai.example.com")
