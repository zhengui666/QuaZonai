from __future__ import annotations

import pytest
from pydantic import ValidationError

from plugins.contract import Capability, DescriptorSnapshot


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
