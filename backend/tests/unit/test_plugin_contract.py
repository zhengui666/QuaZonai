from __future__ import annotations

import os
import subprocess
import sys
import time
from datetime import UTC, datetime
from pathlib import Path

import pytest
from fastapi import HTTPException
from pydantic import ValidationError

from plugins.contract import Capability, DescriptorSnapshot
from quant_runtime.contracts import (
    ArchiveManifestDescriptor,
    ArchiveShardDescriptor,
    CatalogDescriptor,
)
from runners import nautilus_remote_runtime as runtime


def test_descriptor_snapshot_is_structural() -> None:
    descriptor = DescriptorSnapshot(
        plugin_id="parquet_l2",
        version="1.0.0",
        capabilities={Capability.HISTORICAL_IMPORT},
        compatibility_key="polymarket-l2-v1",
        requires_python=">=3.14,<3.15",
        requires_qf=">=0.1,<0.2",
        public_config_schema={"type": "object", "properties": {}},
        secret_config_schema={"type": "object", "properties": {}},
    )
    assert descriptor.plugin_id == "parquet_l2"
    assert descriptor.capabilities == frozenset({Capability.HISTORICAL_IMPORT})


def test_required_secret_must_exist_in_schema() -> None:
    with pytest.raises(ValidationError):
        DescriptorSnapshot(
            plugin_id="research_feed",
            version="1.0.0",
            capabilities={Capability.RESEARCH_TOOL},
            compatibility_key="research-v1",
            requires_python=">=3.14,<3.15",
            requires_qf=">=0.1,<0.2",
            secret_config_schema={"type": "object", "properties": {}},
            required_secret_names=("api_key",),
        )


def _catalog_descriptor() -> CatalogDescriptor:
    return CatalogDescriptor(
        catalog_uri="catalog://pmxt-test",
        provider="pmxt",
        source_license="pmxt-public",
        nautilus_data_type="QuoteTick",
        instrument_scope=["PMXT-ASSET.PMXT"],
        row_count=1,
        schema_revision="test",
        quality_result={},
        point_in_time_result={},
    )


def test_staged_output_rejects_files_outside_catalog_data(tmp_path: Path) -> None:
    (tmp_path / "unexpected.txt").write_text("unexpected", encoding="utf-8")

    with pytest.raises(HTTPException, match="unexpected file"):
        runtime._validate_staged_output(tmp_path, _catalog_descriptor())


def test_plugin_child_caps_each_protocol_stream(tmp_path: Path) -> None:
    command = [
        sys.executable,
        "-c",
        "import sys; sys.stdout.buffer.write(b'x' * (9 * 1024 * 1024)); sys.stdout.flush()",
    ]

    with pytest.raises(runtime._PluginChildLimit, match="output"):
        runtime._run_plugin_child(
            command,
            {},
            cwd=tmp_path,
            env=os.environ.copy(),
            timeout_seconds=5,
        )


def test_plugin_child_input_is_deadline_bound(tmp_path: Path) -> None:
    command = [sys.executable, "-c", "import time; time.sleep(2)"]
    started = time.monotonic()

    with pytest.raises(subprocess.TimeoutExpired):
        runtime._run_plugin_child(
            command,
            {"payload": "x" * (2 * 1024 * 1024)},
            cwd=tmp_path,
            env=os.environ.copy(),
            timeout_seconds=0,
        )

    assert time.monotonic() - started < 1.5


def _manifest_shard(size_bytes: int | None) -> ArchiveShardDescriptor:
    start = datetime(2026, 1, 1, tzinfo=UTC)
    return ArchiveShardDescriptor(
        shard_key="2026-01-01T00:00:00Z",
        source_url="https://example.test/archive.parquet",
        coverage_start=start,
        coverage_end=datetime(2026, 1, 1, 1, tzinfo=UTC),
        size_bytes=size_bytes,
        state="AVAILABLE",
        observed_at=start,
    )


def _manifest_descriptor(
    shard: ArchiveShardDescriptor,
    total_bytes: int,
) -> ArchiveManifestDescriptor:
    start = shard.coverage_start
    return ArchiveManifestDescriptor(
        manifest_uri="manifest://pmxt-test",
        provider="pmxt",
        source_license="pmxt-public",
        source_spec={"kind": "plugin", "config": {"venue": "polymarket_v2"}},
        coverage_start=start,
        coverage_end=shard.coverage_end,
        scanned_until=start,
        shard_count=1,
        total_bytes=total_bytes,
        missing_shard_count=0,
        probe_error_count=0,
        schema_revision="test",
        point_in_time_result={},
        shards=[shard],
    )


def test_manifest_descriptor_requires_exact_available_total_bytes() -> None:
    with pytest.raises(ValidationError, match="total_bytes"):
        _manifest_descriptor(_manifest_shard(10), total_bytes=9)


def test_manifest_descriptor_rejects_unknown_available_size() -> None:
    with pytest.raises(ValidationError, match="sizes must be known"):
        _manifest_descriptor(_manifest_shard(None), total_bytes=0)
