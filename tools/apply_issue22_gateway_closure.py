from __future__ import annotations

from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def read(relative: str) -> str:
    return (ROOT / relative).read_text(encoding="utf-8")


def write(relative: str, content: str) -> None:
    (ROOT / relative).write_text(content, encoding="utf-8")


def replace_once(relative: str, old: str, new: str) -> None:
    content = read(relative)
    count = content.count(old)
    if count != 1:
        raise RuntimeError(
            f"{relative}: expected one replacement, found {count}: {old[:120]!r}"
        )
    write(relative, content.replace(old, new, 1))


# Gateway-side protocol parity.
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/models.py",
    '''class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


class ExperimentMode(StrEnum):
''',
    '''class StrictModel(BaseModel):
    model_config = ConfigDict(extra="forbid")


def _require_aware_datetime(value: datetime, *, field_name: str) -> None:
    if value.tzinfo is None or value.utcoffset() is None:
        raise ValueError(f"{field_name} must be timezone-aware")


class ExperimentMode(StrEnum):
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/models.py",
    '''    @model_validator(mode="after")
    def validate_availability(self) -> QuoteRow:
        if self.available_at < self.timestamp:
            raise ValueError("available_at cannot precede the market event timestamp")
        return self
''',
    '''    @model_validator(mode="after")
    def validate_availability(self) -> QuoteRow:
        _require_aware_datetime(self.timestamp, field_name="timestamp")
        _require_aware_datetime(self.available_at, field_name="available_at")
        if self.available_at < self.timestamp:
            raise ValueError("available_at cannot precede the market event timestamp")
        return self
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/models.py",
    '''    @model_validator(mode="after")
    def reject_unapplied_configuration(self) -> BacktestExperimentRequest:
        if self.data_config:
            raise ValueError(
                "data_config is reserved until protocol v1 explicitly applies its fields; use the "
                "top-level catalog/instrument/time contract instead"
            )
        if self.risk_config:
            raise ValueError(
                "risk_config is reserved until protocol v1 explicitly applies a Nautilus RiskEngine "
                "configuration"
            )
        return self
''',
    '''    @model_validator(mode="after")
    def validate_v1_configuration(self) -> BacktestExperimentRequest:
        if self.start_time is not None:
            _require_aware_datetime(self.start_time, field_name="start_time")
        if self.end_time is not None:
            _require_aware_datetime(self.end_time, field_name="end_time")
        if (
            self.start_time is not None
            and self.end_time is not None
            and self.start_time >= self.end_time
        ):
            raise ValueError("start_time must precede end_time")
        if self.data_config:
            raise ValueError(
                "data_config is reserved until protocol v1 explicitly applies its fields; use the "
                "top-level catalog/instrument/time contract instead"
            )
        if self.risk_config:
            raise ValueError(
                "risk_config is reserved until protocol v1 explicitly applies a Nautilus RiskEngine "
                "configuration"
            )
        return self
''',
)


# Remote gateway: immutable catalog keys, explicit schema IDs and disposable
# child processes for every user-supplied source/wheel import.
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''import base64
import hashlib
import importlib
import io
import json
import sys
import tempfile
import zipfile
''',
    '''import base64
import importlib
import io
import json
import os
import shutil
import subprocess
import sys
import tempfile
import zipfile
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''CANDIDATE_BUNDLE_CONTRACT = "QUAZONAI_NAUTILUS_CANDIDATE_BUNDLE"
CANDIDATE_BUNDLE_CONTRACT_VERSION = "2"
''',
    '''CANDIDATE_BUNDLE_CONTRACT = "QUAZONAI_NAUTILUS_CANDIDATE_BUNDLE"
CANDIDATE_BUNDLE_CONTRACT_VERSION = "2"
QUOTE_TICK_SCHEMA_REVISION = "nautilus.quote_tick.v2"
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''def _parse_time(value: str | datetime) -> datetime:
    parsed = value if isinstance(value, datetime) else datetime.fromisoformat(value)
    if parsed.tzinfo is None:
        raise GatewayContractError("catalog timestamps must be timezone-aware")
    return parsed.astimezone(UTC)


class NautilusGatewayEngine:
''',
    '''def _parse_time(value: str | datetime) -> datetime:
    parsed = value if isinstance(value, datetime) else datetime.fromisoformat(value)
    if parsed.tzinfo is None or parsed.utcoffset() is None:
        raise GatewayContractError("catalog timestamps must be timezone-aware")
    return parsed.astimezone(UTC)


def _sanitized_child_environment() -> dict[str, str]:
    allowed = {
        "PATH",
        "HOME",
        "LANG",
        "LC_ALL",
        "TZ",
        "TMPDIR",
        "PYTHONUTF8",
        "PYTHONHOME",
        "VIRTUAL_ENV",
        "CONDA_PREFIX",
        "LD_LIBRARY_PATH",
        "DYLD_LIBRARY_PATH",
        "SSL_CERT_FILE",
    }
    result = {key: value for key, value in os.environ.items() if key in allowed}
    result["QUAZONAI_NAUTILUS_ISOLATED_CHILD"] = "1"
    return result


class NautilusGatewayEngine:
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''    @staticmethod
    def _instrument(requested_id: str) -> Any:
''',
    '''    @staticmethod
    def _manifest_result(manifest: dict[str, Any]) -> dict[str, Any]:
        return {
            "protocol_version": manifest["protocol_version"],
            "runtime_version": manifest["runtime_version"],
            "catalog_key": manifest["catalog_key"],
            "catalog_uri": f"nautilus-catalog://{manifest['catalog_key']}",
            "nautilus_data_type": manifest["nautilus_data_type"],
            "instrument_scope": manifest["instrument_scope"],
            "event_time_start": _parse_time(manifest["event_time_start"]),
            "event_time_end": _parse_time(manifest["event_time_end"]),
            "available_time_start": _parse_time(manifest["available_time_start"]),
            "available_time_end": _parse_time(manifest["available_time_end"]),
            "row_count": int(manifest["row_count"]),
            "schema_revision": manifest["schema_revision"],
            "quality_result": manifest["quality_result"],
            "point_in_time_result": manifest["point_in_time_result"],
            "ingested_at": _parse_time(manifest["ingested_at"]),
        }

    def _isolated_operation(
        self,
        operation: str,
        payload: dict[str, Any],
        *,
        catalog_key: str | None = None,
    ) -> dict[str, Any]:
        with tempfile.TemporaryDirectory(
            prefix="qz-isolated-", dir=self._artifact_root
        ) as directory:
            workspace = Path(directory)
            child_root = workspace / "runtime"
            (child_root / "catalogs").mkdir(parents=True)
            if catalog_key is not None:
                source_catalog = self._catalog_path(catalog_key)
                if not source_catalog.is_dir():
                    raise GatewayContractError("selected catalog is unavailable")
                shutil.copytree(
                    source_catalog,
                    child_root / "catalogs" / catalog_key,
                )
            input_path = workspace / "input.json"
            output_path = workspace / "output.json"
            input_path.write_text(json.dumps(_jsonable(payload)), encoding="utf-8")
            completed = subprocess.run(
                [
                    sys.executable,
                    "-I",
                    "-m",
                    "quazonai_nautilus_gateway.isolated_runner",
                    operation,
                    str(child_root),
                    str(input_path),
                    str(output_path),
                ],
                cwd=workspace,
                env=_sanitized_child_environment(),
                stdin=subprocess.DEVNULL,
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                text=True,
                timeout=900,
                check=False,
            )
            if completed.returncode != 0 or not output_path.is_file():
                raise GatewayContractError("isolated Nautilus operation failed")
            try:
                result = json.loads(output_path.read_text(encoding="utf-8"))
            except (json.JSONDecodeError, UnicodeDecodeError) as exc:
                raise GatewayContractError("isolated Nautilus result is invalid") from exc
            if not isinstance(result, dict):
                raise GatewayContractError("isolated Nautilus result is invalid")
            return result

    @staticmethod
    def _instrument(requested_id: str) -> Any:
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''        instrument = self._instrument(request.instrument_id)
        rows = sorted(request.rows, key=lambda row: row.timestamp)
''',
    '''        catalog_path = self._catalog_path(request.catalog_key)
        manifest_path = catalog_path / "quazonai-catalog-manifest.json"
        request_path = catalog_path / "quazonai-ingest-request.json"
        canonical_request = request.model_dump(mode="json")
        if manifest_path.exists():
            if not request_path.exists():
                raise GatewayContractError(
                    "catalog key is immutable and its ingest contract is missing"
                )
            existing_request = json.loads(request_path.read_text(encoding="utf-8"))
            if existing_request != canonical_request:
                raise GatewayContractError(
                    "catalog key is immutable and already bound to another ingest contract"
                )
            manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
            return self._manifest_result(manifest)
        if catalog_path.exists() and any(catalog_path.iterdir()):
            raise GatewayContractError(
                "catalog key is immutable and contains an incomplete prior ingest"
            )

        instrument = self._instrument(request.instrument_id)
        rows = sorted(request.rows, key=lambda row: row.timestamp)
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''        catalog_path = self._catalog_path(request.catalog_key)
        catalog_path.mkdir(parents=True, exist_ok=True)
''',
    '''        catalog_path.mkdir(parents=True, exist_ok=True)
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''        schema_revision = hashlib.sha256(
            b"QuoteTick:ts_event,ts_init,bid_price,ask_price,bid_size,ask_size:v2"
        ).hexdigest()
        manifest_path = catalog_path / "quazonai-catalog-manifest.json"
        existing: dict[str, Any] = {}
        if manifest_path.exists():
            existing = json.loads(manifest_path.read_text(encoding="utf-8"))
        instrument_records = dict(existing.get("instruments", {}))
        instrument_records[str(instrument.id)] = {
''',
    '''        schema_revision = QUOTE_TICK_SCHEMA_REVISION
        instrument_records = {
            str(instrument.id): {
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''            "available_time_end": available_end.isoformat(),
        }
        instrument_scope = sorted(str(item.id) for item in catalog.instruments())
''',
    '''                "available_time_end": available_end.isoformat(),
            }
        }
        instrument_scope = sorted(str(item.id) for item in catalog.instruments())
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''        manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
        return {
''',
    '''        manifest_path.write_text(json.dumps(manifest, indent=2), encoding="utf-8")
        request_path.write_text(
            json.dumps(canonical_request, indent=2, sort_keys=True),
            encoding="utf-8",
        )
        return {
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''    def run_backtest(self, request: BacktestExperimentRequest) -> dict[str, Any]:
        if request.protocol_version != PROTOCOL_VERSION:
            raise GatewayContractError("unsupported protocol version")
''',
    '''    def run_backtest(
        self,
        request: BacktestExperimentRequest,
        *,
        _source_isolated: bool = False,
    ) -> dict[str, Any]:
        if request.protocol_version != PROTOCOL_VERSION:
            raise GatewayContractError("unsupported protocol version")
        if request.strategy.kind == "SOURCE_BUNDLE" and not _source_isolated:
            return self._isolated_operation(
                "backtest",
                {"request": request.model_dump(mode="json")},
                catalog_key=request.catalog_key,
            )
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''        if wheel:
            try:
                self._verify_wheel(wheel, request.manifest)
''',
    '''        if wheel:
            try:
                self._isolated_operation(
                    "verify-wheel",
                    {
                        "wheel_b64": base64.b64encode(wheel).decode("ascii"),
                        "manifest": request.manifest,
                    },
                )
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''    @staticmethod
    def _verify_wheel(wheel: bytes, manifest: dict[str, Any]) -> None:
''',
    '''    @staticmethod
    def _verify_wheel_inline(wheel: bytes, manifest: dict[str, Any]) -> None:
''',
)

isolated_runner = '''"""Disposable child process for untrusted strategy and wheel imports."""

from __future__ import annotations

import base64
import json
import os
from pathlib import Path
import socket
import sys
from typing import Any

from quazonai_nautilus_gateway.engine import NautilusGatewayEngine, _jsonable
from quazonai_nautilus_gateway.models import BacktestExperimentRequest


def _deny_external_network() -> None:
    original_connect = socket.socket.connect
    original_connect_ex = socket.socket.connect_ex

    def guarded_connect(sock: socket.socket, address: Any) -> Any:
        if sock.family in {socket.AF_INET, socket.AF_INET6}:
            raise OSError("network access is disabled in isolated strategy processes")
        return original_connect(sock, address)

    def guarded_connect_ex(sock: socket.socket, address: Any) -> int:
        if sock.family in {socket.AF_INET, socket.AF_INET6}:
            return 101
        return original_connect_ex(sock, address)

    def denied_create_connection(*_: Any, **__: Any) -> Any:
        raise OSError("network access is disabled in isolated strategy processes")

    socket.socket.connect = guarded_connect  # type: ignore[method-assign]
    socket.socket.connect_ex = guarded_connect_ex  # type: ignore[method-assign]
    socket.create_connection = denied_create_connection


def main() -> None:
    if len(sys.argv) != 5:
        raise SystemExit(2)
    operation, root_raw, input_raw, output_raw = sys.argv[1:]
    root = Path(root_raw).resolve()
    input_path = Path(input_raw).resolve()
    output_path = Path(output_raw).resolve()
    if os.getenv("QUAZONAI_NAUTILUS_ISOLATED_CHILD") != "1":
        raise SystemExit(3)
    os.chdir(root)
    _deny_external_network()
    payload = json.loads(input_path.read_text(encoding="utf-8"))
    engine = NautilusGatewayEngine(root)
    if operation == "backtest":
        request = BacktestExperimentRequest.model_validate(payload["request"])
        result = engine.run_backtest(request, _source_isolated=True)
    elif operation == "verify-wheel":
        wheel = base64.b64decode(payload["wheel_b64"], validate=True)
        engine._verify_wheel_inline(wheel, payload["manifest"])
        result = {"verified": True}
    else:
        raise SystemExit(4)
    output_path.write_text(json.dumps(_jsonable(result)), encoding="utf-8")


if __name__ == "__main__":
    main()
'''
write(
    "nautilus_runtime/src/quazonai_nautilus_gateway/isolated_runner.py",
    isolated_runner,
)


# Real runtime tests now prove immutable exact replay and child execution.
replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    '''from pathlib import Path
from uuid import UUID, uuid4

from quazonai_nautilus_gateway.engine import NautilusGatewayEngine
''',
    '''from pathlib import Path
from uuid import UUID, uuid4

import pytest

from quazonai_nautilus_gateway.engine import GatewayContractError, NautilusGatewayEngine
''',
)
replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    '''        instrument_ids=["EUR/USD.SIM", "GBP/USD.SIM"],
''',
    '''        instrument_ids=["EUR/USD.SIM"],
''',
)
replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    '''    first = engine.ingest(
        CatalogIngestRequest(
            catalog_key="integration-fx-quotes",
            provider="CI_GENERATED_CSV_EQUIVALENT",
            source="fixture://deterministic-eur-usd-quotes.csv",
            source_license="CC0-1.0",
            instrument_id="EUR/USD.SIM",
            rows=_rows(),
        )
    )
    assert first["catalog_uri"] == "nautilus-catalog://integration-fx-quotes"
    assert first["row_count"] == 360
    assert first["quality_result"]["state"] == "VALID"
    assert first["point_in_time_result"]["replay_order"] == "TS_INIT"
    assert first["available_time_start"] > first["event_time_start"]

    second = engine.ingest(
        CatalogIngestRequest(
            catalog_key="integration-fx-quotes",
            provider="CI_GENERATED_CSV_EQUIVALENT",
            source="fixture://deterministic-gbp-usd-quotes.csv",
            source_license="CC0-1.0",
            instrument_id="GBP/USD.SIM",
            rows=_rows(base=1.27),
        )
    )
    assert second["row_count"] == 720
    assert second["instrument_scope"] == ["EUR/USD.SIM", "GBP/USD.SIM"]

    validated = engine.validate_catalog(
        CatalogValidationRequest(
            catalog_key="integration-fx-quotes",
            instrument_ids=["EUR/USD.SIM", "GBP/USD.SIM"],
            nautilus_data_type="QuoteTick",
        )
    )
    assert validated["valid"] is True
    assert validated["row_count"] == 720
''',
    '''    ingest_request = CatalogIngestRequest(
        catalog_key="integration-fx-quotes",
        provider="CI_GENERATED_CSV_EQUIVALENT",
        source="fixture://deterministic-eur-usd-quotes.csv",
        source_license="CC0-1.0",
        instrument_id="EUR/USD.SIM",
        rows=_rows(),
    )
    first = engine.ingest(ingest_request)
    assert first["catalog_uri"] == "nautilus-catalog://integration-fx-quotes"
    assert first["row_count"] == 360
    assert first["schema_revision"] == "nautilus.quote_tick.v2"
    assert first["quality_result"]["state"] == "VALID"
    assert first["point_in_time_result"]["replay_order"] == "TS_INIT"
    assert first["available_time_start"] > first["event_time_start"]

    replay = engine.ingest(ingest_request)
    assert replay == first
    with pytest.raises(GatewayContractError, match="immutable"):
        engine.ingest(
            CatalogIngestRequest(
                catalog_key="integration-fx-quotes",
                provider="CI_GENERATED_CSV_EQUIVALENT",
                source="fixture://different-contract.csv",
                source_license="CC0-1.0",
                instrument_id="EUR/USD.SIM",
                rows=_rows(base=1.27),
            )
        )

    validated = engine.validate_catalog(
        CatalogValidationRequest(
            catalog_key="integration-fx-quotes",
            instrument_ids=["EUR/USD.SIM"],
            nautilus_data_type="QuoteTick",
        )
    )
    assert validated["valid"] is True
    assert validated["row_count"] == 360
''',
)
replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    '''    # BacktestResult.total_events counts domain events, not loaded market data;
    # iterations is the direct proof that both 360-row QuoteTick streams ran.
    assert evidence["statistics"]["iterations"] >= 720
''',
    '''    # BacktestResult.total_events counts domain events, not loaded market data;
    # iterations proves the immutable 360-row QuoteTick stream ran in the child.
    assert evidence["statistics"]["iterations"] >= 360
''',
)
replace_once(
    "nautilus_runtime/tests/test_real_backtest.py",
    '''    assert evidence["diagnostics"]["loaded_instrument_count"] == 2
''',
    '''    assert evidence["diagnostics"]["loaded_instrument_count"] == 1
''',
)
runtime_tests = read("nautilus_runtime/tests/test_real_backtest.py")
if "test_protocol_rejects_naive_timestamps" not in runtime_tests:
    runtime_tests = runtime_tests.rstrip() + '''


def test_protocol_rejects_naive_timestamps() -> None:
    naive = datetime(2024, 1, 2)
    with pytest.raises(ValueError, match="timezone-aware"):
        QuoteRow(
            timestamp=naive,
            available_at=naive + timedelta(seconds=2),
            bid_price="1.0000",
            ask_price="1.0001",
        )
'''
    write("nautilus_runtime/tests/test_real_backtest.py", runtime_tests + "\n")
