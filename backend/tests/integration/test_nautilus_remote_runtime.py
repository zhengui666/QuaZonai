from __future__ import annotations

import json
import os
import zipfile
from datetime import UTC, datetime
from pathlib import Path

import httpx
import pytest

from quant_runtime.config import RemoteNautilusConfig
from quant_runtime.remote import NautilusQuantRuntime


pytestmark = [
    pytest.mark.nautilus,
    pytest.mark.skipif(
        not os.environ.get("QUAZONAI_NAUTILUS_RUNTIME_URL"),
        reason="real remote NautilusTrader runtime is not configured",
    ),
]


def _bundle(path: Path) -> None:
    now = datetime.now(UTC).isoformat()
    manifest = {
        "candidate_bundle_contract_version": "1",
        "package_kind": "TARGET_PORTFOLIO_FRAME",
        "candidate_id": "candidate-1",
        "candidate_package_id": "package-1",
        "candidate_package_revision": 1,
        "target_portfolio_frame": "validation/target-portfolio-frame.json",
    }
    frame = {
        "schema_version": "1",
        "portfolio_candidate_id": "candidate-1",
        "portfolio_state": "READY",
        "universe_version_id": "universe-1",
        "as_of_time": now,
        "effective_from": now,
        "effective_until": None,
        "rows": [
            {"instrument_id": "A", "target_weight": 0.5, "confidence": 0.5},
            {"instrument_id": "B", "target_weight": 0.5, "confidence": 0.5},
        ],
    }
    with zipfile.ZipFile(path, "w") as archive:
        archive.writestr("manifest.json", json.dumps(manifest))
        archive.writestr("validation/target-portfolio-frame.json", json.dumps(frame))


def test_remote_runtime_only_accepts_target_portfolio_conformance(tmp_path: Path) -> None:
    config = RemoteNautilusConfig.from_env(required=True)
    assert config is not None
    package = tmp_path / "candidate.zip"
    _bundle(package)

    runtime = NautilusQuantRuntime(config)
    assert runtime.capabilities().supported_modes == []
    assert runtime.verify_candidate(package)["valid"] is True

    response = httpx.post(
        f"{config.base_url}/v1/runs",
        headers={
            "Authorization": f"Bearer {config.service_token}",
            "X-QuaZonai-Quant-Contract": config.contract_version,
        },
        json={},
    )
    assert response.status_code == 404
