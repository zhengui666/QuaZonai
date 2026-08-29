from __future__ import annotations

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    count = text.count(old)
    if count != 1:
        raise RuntimeError(f"{path}: expected one match, found {count}: {old[:120]!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


engine = "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py"

# An isolated strategy process is a capability boundary inside the same Gateway,
# not a new data publisher. Preserve the immutable Gateway + catalog release identity.
replace_once(
    engine,
    '''            child_storage_root = child_catalog_root / "data"\n            child_storage_root.mkdir(parents=True)\n            if catalog_key is not None:\n                source_catalog = self._catalog_path(catalog_key)\n                child_storage_id = UUID("00000000-0000-0000-0000-000000000001")\n''',
    '''            child_storage_root = child_catalog_root / "data"\n            child_storage_root.mkdir(parents=True)\n            (child_root / ".gateway-instance-id").write_text(\n                f"{self._gateway_instance_id}\\n", encoding="ascii"\n            )\n            if catalog_key is not None:\n                source_catalog = self._catalog_path(catalog_key)\n                record = self._find_catalog_record(self._load_catalog_registry(), catalog_key)\n                if record is None or record.state != "READY":\n                    raise GatewayContractError("selected catalog is unavailable")\n                child_storage_id = record.storage_id\n''',
)

# Raw engine APIs use UUID values consistently; FastAPI/Pydantic serializes them on the wire.
replace_once(
    engine,
    '''            "gateway_instance_id": str(self._gateway_instance_id),\n            "catalog_release_id": str(record.storage_id),\n            "valid": not findings and bool(instruments) and bool(ticks),\n''',
    '''            "gateway_instance_id": self._gateway_instance_id,\n            "catalog_release_id": record.storage_id,\n            "valid": not findings and bool(instruments) and bool(ticks),\n''',
)

# Keep the namespace-profile test aligned with the sandbox command contract.
replace_once(
    "nautilus_runtime/tests/test_sandbox_namespace_profile.py",
    '''    command = gateway_engine._source_bundle_sandbox_command(\n        operation="backtest",\n        workspace=tmp_path,\n    )\n''',
    '''    command = gateway_engine._source_bundle_sandbox_command(\n        operation="backtest",\n        workspace=tmp_path,\n        data_root=tmp_path / "gateway-data",\n    )\n''',
)

# Governed ingest fixture now carries the immutable Gateway/catalog release identity.
replace_once(
    "backend/tests/integration/test_domain_api.py",
    '''    def fake_remote(ingest_request):\n        assert ingest_request.catalog_key == "fx-discovery-v1"\n        ingested = SimpleNamespace(\n            catalog_key="fx-discovery-v1",\n            catalog_uri="nautilus-catalog://fx-discovery-v1",\n''',
    '''    def fake_remote(ingest_request):\n        assert ingest_request.catalog_key == "fx-discovery-v1"\n        gateway_instance_id = uuid4()\n        catalog_release_id = uuid4()\n        ingested = SimpleNamespace(\n            catalog_key="fx-discovery-v1",\n            catalog_uri="nautilus-catalog://fx-discovery-v1",\n            gateway_instance_id=gateway_instance_id,\n            catalog_release_id=catalog_release_id,\n''',
)
replace_once(
    "backend/tests/integration/test_domain_api.py",
    '''        validated = SimpleNamespace(\n            valid=True,\n            catalog_key="fx-discovery-v1",\n            instrument_scope=["EUR/USD.SIM"],\n''',
    '''        validated = SimpleNamespace(\n            valid=True,\n            catalog_key="fx-discovery-v1",\n            gateway_instance_id=gateway_instance_id,\n            catalog_release_id=catalog_release_id,\n            instrument_scope=["EUR/USD.SIM"],\n''',
)
