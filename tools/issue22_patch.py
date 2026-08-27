from __future__ import annotations

from pathlib import Path


def replace_once(path: str, old: str, new: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    if text.count(old) != 1:
        raise RuntimeError(f"{path}: expected exactly one target for {old[:120]!r}")
    target.write_text(text.replace(old, new, 1), encoding="utf-8")


def replace_between(path: str, start: str, end: str, replacement: str) -> None:
    target = Path(path)
    text = target.read_text(encoding="utf-8")
    start_index = text.index(start)
    end_index = text.index(end, start_index)
    target.write_text(text[:start_index] + replacement + text[end_index:], encoding="utf-8")


contract_old = '''class CatalogIngestRequest(StrictModel):
    protocol_version: str = QUANT_RUNTIME_PROTOCOL_VERSION
    request_id: UUID = Field(default_factory=uuid4)
    catalog_key: str = Field(
        min_length=1,
        max_length=128,
        pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$",
    )
    provider: str = Field(min_length=1, max_length=200)
    source: str = Field(min_length=1, max_length=500)
    source_license: str | None = Field(default=None, max_length=500)
    instrument_id: str = Field(min_length=3, max_length=200)
    nautilus_data_type: Literal["QuoteTick"] = "QuoteTick"
    rows: list[QuoteRow] = Field(min_length=2, max_length=1_000_000)
'''
contract_new = '''class InstrumentQuoteBatch(StrictModel):
    instrument_id: str = Field(min_length=3, max_length=200)
    instrument_type: str = Field(
        min_length=1,
        max_length=80,
        pattern=r"^[A-Za-z][A-Za-z0-9_]*$",
    )
    instrument_definition: dict[str, Any]
    rows: list[QuoteRow] = Field(min_length=2, max_length=1_000_000)


class CatalogIngestRequest(StrictModel):
    protocol_version: str = QUANT_RUNTIME_PROTOCOL_VERSION
    request_id: UUID = Field(default_factory=uuid4)
    catalog_key: str = Field(
        min_length=1,
        max_length=128,
        pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$",
    )
    provider: str = Field(min_length=1, max_length=200)
    source: str = Field(min_length=1, max_length=500)
    source_license: str | None = Field(default=None, max_length=500)
    nautilus_data_type: Literal["QuoteTick"] = "QuoteTick"
    instruments: list[InstrumentQuoteBatch] = Field(min_length=1, max_length=10_000)

    @model_validator(mode="after")
    def validate_instrument_batches(self) -> CatalogIngestRequest:
        ids = [item.instrument_id for item in self.instruments]
        if len(ids) != len(set(ids)):
            raise ValueError("catalog ingest instrument_ids must be unique")
        if sum(len(item.rows) for item in self.instruments) > 1_000_000:
            raise ValueError("catalog ingest is limited to 1,000,000 QuoteTicks per revision")
        return self
'''
replace_once("backend/src/quant_runtime/contracts.py", contract_old, contract_new)

runtime_old = '''class CatalogIngestRequest(StrictModel):
    protocol_version: str = "1"
    request_id: UUID = Field(default_factory=uuid4)
    catalog_key: str = Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
    provider: str = Field(min_length=1, max_length=200)
    source: str = Field(min_length=1, max_length=500)
    source_license: str | None = Field(default=None, max_length=500)
    instrument_id: str = Field(min_length=3, max_length=200)
    nautilus_data_type: Literal["QuoteTick"] = "QuoteTick"
    rows: list[QuoteRow] = Field(min_length=2, max_length=1_000_000)
'''
runtime_new = '''class InstrumentQuoteBatch(StrictModel):
    instrument_id: str = Field(min_length=3, max_length=200)
    instrument_type: str = Field(
        min_length=1,
        max_length=80,
        pattern=r"^[A-Za-z][A-Za-z0-9_]*$",
    )
    instrument_definition: dict[str, Any]
    rows: list[QuoteRow] = Field(min_length=2, max_length=1_000_000)


class CatalogIngestRequest(StrictModel):
    protocol_version: str = "1"
    request_id: UUID = Field(default_factory=uuid4)
    catalog_key: str = Field(min_length=1, max_length=128, pattern=r"^[A-Za-z0-9][A-Za-z0-9._-]*$")
    provider: str = Field(min_length=1, max_length=200)
    source: str = Field(min_length=1, max_length=500)
    source_license: str | None = Field(default=None, max_length=500)
    nautilus_data_type: Literal["QuoteTick"] = "QuoteTick"
    instruments: list[InstrumentQuoteBatch] = Field(min_length=1, max_length=10_000)

    @model_validator(mode="after")
    def validate_instrument_batches(self) -> CatalogIngestRequest:
        ids = [item.instrument_id for item in self.instruments]
        if len(ids) != len(set(ids)):
            raise ValueError("catalog ingest instrument_ids must be unique")
        if sum(len(item.rows) for item in self.instruments) > 1_000_000:
            raise ValueError("catalog ingest is limited to 1,000,000 QuoteTicks per revision")
        return self
'''
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/models.py", runtime_old, runtime_new
)

replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    "from nautilus_trader.model.data import QuoteTick\n",
    "from nautilus_trader.model.data import QuoteTick\nfrom nautilus_trader.model.identifiers import InstrumentId\n",
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    "from nautilus_trader.test_kit.providers import TestInstrumentProvider\n",
    "",
)

ingest_section = '''    @staticmethod
    def _instrument_from_definition(batch: Any) -> Any:
        try:
            instrument_module = importlib.import_module("nautilus_trader.model.instruments")
            instrument_class = getattr(instrument_module, batch.instrument_type)
            from_dict = getattr(instrument_class, "from_dict")
            instrument = from_dict(dict(batch.instrument_definition))
        except (AttributeError, ImportError, KeyError, TypeError, ValueError) as exc:
            raise GatewayContractError(
                f"invalid governed Nautilus instrument definition for {batch.instrument_id!r}"
            ) from exc
        if str(instrument.id) != batch.instrument_id:
            raise GatewayContractError(
                "governed instrument definition id does not match the requested instrument_id"
            )
        return instrument

    @staticmethod
    def _catalog_instruments(catalog: ParquetDataCatalog, requested_ids: list[str]) -> list[Any]:
        available = {str(item.id): item for item in catalog.instruments()}
        missing = sorted(set(requested_ids).difference(available))
        if missing:
            raise GatewayContractError(
                f"governed catalog is missing instrument definitions: {missing!r}"
            )
        return [available[item] for item in requested_ids]

    @staticmethod
    def _canonical_ingest_request(request: CatalogIngestRequest) -> dict[str, Any]:
        canonical = request.model_dump(mode="json", exclude={"request_id"})
        batches = list(canonical["instruments"])
        for batch in batches:
            batch["rows"] = sorted(
                batch["rows"],
                key=lambda row: (str(row["timestamp"]), str(row["available_at"])),
            )
        canonical["instruments"] = sorted(batches, key=lambda item: item["instrument_id"])
        return canonical

    def _existing_ingest_result(
        self,
        *,
        catalog_path: Path,
        canonical_request: dict[str, Any],
    ) -> dict[str, Any] | None:
        manifest_path = catalog_path / "quazonai-catalog-manifest.json"
        request_path = catalog_path / "quazonai-ingest-request.json"
        if not catalog_path.exists():
            return None
        if not manifest_path.exists() or not request_path.exists():
            raise GatewayContractError(
                "catalog key is immutable and contains an incomplete prior ingest"
            )
        existing_request = json.loads(request_path.read_text(encoding="utf-8"))
        if existing_request != canonical_request:
            raise GatewayContractError(
                "catalog key is immutable and already bound to another ingest contract"
            )
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        return self._manifest_result(manifest)

    @staticmethod
    def _batch_ticks(batch: Any) -> tuple[Any, list[QuoteTick], dict[str, Any]]:
        instrument = NautilusGatewayEngine._instrument_from_definition(batch)
        rows = sorted(batch.rows, key=lambda row: row.timestamp)
        event_times = [row.timestamp.astimezone(UTC) for row in rows]
        if len(set(event_times)) != len(event_times):
            raise GatewayContractError(
                f"event timestamps must be unique for {batch.instrument_id}"
            )
        try:
            frame = pd.DataFrame(
                {
                    "bid_price": [float(row.bid_price) for row in rows],
                    "ask_price": [float(row.ask_price) for row in rows],
                    "bid_size": [float(row.volume or "1000000") for row in rows],
                    "ask_size": [float(row.volume or "1000000") for row in rows],
                },
                index=pd.DatetimeIndex(event_times, name="timestamp"),
            )
        except ValueError as exc:
            raise GatewayContractError("quote prices and sizes must be numeric") from exc
        if not frame.index.is_monotonic_increasing or frame.index.has_duplicates:
            raise GatewayContractError(
                "event timestamps must be unique and monotonically increasing"
            )
        if (frame["bid_price"] > frame["ask_price"]).any():
            raise GatewayContractError("bid price cannot exceed ask price")

        wrangled = QuoteTickDataWrangler(instrument).process(frame)
        ticks = [
            QuoteTick(
                tick.instrument_id,
                tick.bid_price,
                tick.ask_price,
                tick.bid_size,
                tick.ask_size,
                tick.ts_event,
                int(pd.Timestamp(row.available_at).value),
            )
            for tick, row in zip(wrangled, rows, strict=True)
        ]
        if any(tick.ts_init < tick.ts_event for tick in ticks):
            raise GatewayContractError("availability timestamp cannot precede event timestamp")
        availability_times = [row.available_at.astimezone(UTC) for row in rows]
        record = {
            "instrument_type": batch.instrument_type,
            "row_count": len(ticks),
            "event_time_start": min(event_times).isoformat(),
            "event_time_end": max(event_times).isoformat(),
            "available_time_start": min(availability_times).isoformat(),
            "available_time_end": max(availability_times).isoformat(),
        }
        return instrument, ticks, record

    def ingest(self, request: CatalogIngestRequest) -> dict[str, Any]:
        if request.protocol_version != PROTOCOL_VERSION:
            raise GatewayContractError("unsupported protocol version")
        catalog_path = self._catalog_path(request.catalog_key)
        canonical_request = self._canonical_ingest_request(request)
        existing = self._existing_ingest_result(
            catalog_path=catalog_path,
            canonical_request=canonical_request,
        )
        if existing is not None:
            return existing

        lock_path = (self._catalog_root / f".{request.catalog_key}.ingest.lock").resolve()
        if lock_path.parent != self._catalog_root:
            raise GatewayContractError("catalog lock escaped the configured root")
        try:
            lock_fd = os.open(lock_path, os.O_CREAT | os.O_EXCL | os.O_WRONLY, 0o600)
        except FileExistsError as exc:
            raise GatewayContractError("catalog ingest is already in progress") from exc
        staging_path: Path | None = None
        try:
            existing = self._existing_ingest_result(
                catalog_path=catalog_path,
                canonical_request=canonical_request,
            )
            if existing is not None:
                return existing

            staging_path = Path(
                tempfile.mkdtemp(prefix=f".{request.catalog_key}.staging-", dir=self._catalog_root)
            )
            catalog = ParquetDataCatalog(path=str(staging_path))
            instrument_records: dict[str, dict[str, Any]] = {}
            for batch in sorted(request.instruments, key=lambda item: item.instrument_id):
                instrument, ticks, record = self._batch_ticks(batch)
                catalog.write_data([instrument])
                catalog.write_data(ticks)
                instrument_records[str(instrument.id)] = {
                    "provider": request.provider,
                    "source": request.source,
                    "source_license": request.source_license,
                    **record,
                }

            instrument_scope = sorted(str(item.id) for item in catalog.instruments())
            expected_scope = sorted(item.instrument_id for item in request.instruments)
            if instrument_scope != expected_scope:
                raise GatewayContractError(
                    "persisted Nautilus instrument scope differs from the governed ingest contract"
                )
            all_records = list(instrument_records.values())
            event_start = min(_parse_time(item["event_time_start"]) for item in all_records)
            event_end = max(_parse_time(item["event_time_end"]) for item in all_records)
            available_start = min(
                _parse_time(item["available_time_start"]) for item in all_records
            )
            available_end = max(
                _parse_time(item["available_time_end"]) for item in all_records
            )
            manifest = {
                "protocol_version": PROTOCOL_VERSION,
                "runtime_version": nautilus_version,
                "catalog_key": request.catalog_key,
                "nautilus_data_type": request.nautilus_data_type,
                "instrument_scope": instrument_scope,
                "instruments": instrument_records,
                "event_time_start": event_start.isoformat(),
                "event_time_end": event_end.isoformat(),
                "available_time_start": available_start.isoformat(),
                "available_time_end": available_end.isoformat(),
                "row_count": sum(int(item["row_count"]) for item in all_records),
                "schema_revision": QUOTE_TICK_SCHEMA_REVISION,
                "quality_result": {
                    "state": "VALID",
                    "duplicate_timestamps": 0,
                    "crossed_quotes": 0,
                    "sorted": True,
                },
                "point_in_time_result": {
                    "state": "VALID",
                    "replay_order": "TS_INIT",
                    "event_time_preserved": True,
                    "availability_time_preserved": True,
                },
                "ingested_at": _utc_now().isoformat(),
            }
            (staging_path / "quazonai-catalog-manifest.json").write_text(
                json.dumps(manifest, indent=2), encoding="utf-8"
            )
            (staging_path / "quazonai-ingest-request.json").write_text(
                json.dumps(canonical_request, indent=2, sort_keys=True),
                encoding="utf-8",
            )
            os.rename(staging_path, catalog_path)
            staging_path = None
            return self._manifest_result(manifest)
        finally:
            os.close(lock_fd)
            lock_path.unlink(missing_ok=True)
            if staging_path is not None and staging_path.exists():
                shutil.rmtree(staging_path, ignore_errors=True)

'''
replace_between(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    "    @staticmethod\n    def _instrument(requested_id: str) -> Any:\n",
    "    def validate_catalog(self, request: CatalogValidationRequest) -> dict[str, Any]:\n",
    ingest_section,
)

validate_section = '''    def validate_catalog(self, request: CatalogValidationRequest) -> dict[str, Any]:
        if request.protocol_version != PROTOCOL_VERSION:
            raise GatewayContractError("unsupported protocol version")
        catalog_path = self._catalog_path(request.catalog_key)
        manifest_path = catalog_path / "quazonai-catalog-manifest.json"
        findings: list[dict[str, Any]] = []
        if not manifest_path.exists():
            return {
                "protocol_version": PROTOCOL_VERSION,
                "runtime_version": nautilus_version,
                "catalog_key": request.catalog_key,
                "valid": False,
                "instrument_scope": [],
                "row_count": 0,
                "findings": [{"code": "CATALOG_MANIFEST_MISSING"}],
            }
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        catalog = ParquetDataCatalog(path=str(catalog_path))
        instruments = catalog.instruments()
        scope = sorted(str(instrument.id) for instrument in instruments)
        requested = set(request.instrument_ids)
        if requested and not requested.issubset(scope):
            findings.append(
                {
                    "code": "INSTRUMENT_SCOPE_MISMATCH",
                    "missing": sorted(requested.difference(scope)),
                }
            )
        if set(manifest.get("instrument_scope", [])) != set(scope):
            findings.append({"code": "CATALOG_MANIFEST_SCOPE_MISMATCH"})
        if (
            request.nautilus_data_type
            and request.nautilus_data_type != manifest.get("nautilus_data_type")
        ):
            findings.append(
                {
                    "code": "DATA_TYPE_MISMATCH",
                    "expected": request.nautilus_data_type,
                    "actual": manifest.get("nautilus_data_type"),
                }
            )

        ticks: list[Any] = []
        if scope:
            try:
                ticks = list(catalog.query_quote_ticks(identifiers=scope))
            except (OSError, RuntimeError, TypeError, ValueError) as exc:
                findings.append({"code": "CATALOG_DATA_QUERY_FAILED", "detail": str(exc)})
        if not ticks:
            findings.append({"code": "CATALOG_QUOTE_DATA_MISSING"})

        actual_row_count = len(ticks)
        actual_event_start: datetime | None = None
        actual_event_end: datetime | None = None
        actual_available_start: datetime | None = None
        actual_available_end: datetime | None = None
        if ticks:
            tick_scope = sorted({str(tick.instrument_id) for tick in ticks})
            if tick_scope != scope:
                findings.append(
                    {
                        "code": "CATALOG_DATA_SCOPE_MISMATCH",
                        "instrument_scope": scope,
                        "quote_scope": tick_scope,
                    }
                )
            event_values = [int(tick.ts_event) for tick in ticks]
            init_values = [int(tick.ts_init) for tick in ticks]
            actual_event_start = datetime.fromtimestamp(min(event_values) / 1_000_000_000, UTC)
            actual_event_end = datetime.fromtimestamp(max(event_values) / 1_000_000_000, UTC)
            actual_available_start = datetime.fromtimestamp(min(init_values) / 1_000_000_000, UTC)
            actual_available_end = datetime.fromtimestamp(max(init_values) / 1_000_000_000, UTC)
            if any(init < event for init, event in zip(init_values, event_values, strict=True)):
                findings.append({"code": "CATALOG_POINT_IN_TIME_ORDER_INVALID"})

        if actual_row_count != int(manifest.get("row_count", -1)):
            findings.append(
                {
                    "code": "CATALOG_MANIFEST_ROW_COUNT_MISMATCH",
                    "manifest": manifest.get("row_count"),
                    "actual": actual_row_count,
                }
            )
        bounds = {
            "event_time_start": actual_event_start,
            "event_time_end": actual_event_end,
            "available_time_start": actual_available_start,
            "available_time_end": actual_available_end,
        }
        for key, actual in bounds.items():
            if actual is None:
                continue
            try:
                expected = _parse_time(manifest[key])
            except (GatewayContractError, KeyError, TypeError, ValueError):
                findings.append({"code": "CATALOG_MANIFEST_TIME_INVALID", "field": key})
                continue
            if actual != expected:
                findings.append(
                    {
                        "code": "CATALOG_MANIFEST_TIME_MISMATCH",
                        "field": key,
                        "manifest": expected.isoformat(),
                        "actual": actual.isoformat(),
                    }
                )

        return {
            "protocol_version": PROTOCOL_VERSION,
            "runtime_version": nautilus_version,
            "catalog_key": request.catalog_key,
            "valid": not findings and bool(instruments) and bool(ticks),
            "instrument_scope": scope,
            "row_count": actual_row_count,
            "event_time_start": actual_event_start,
            "event_time_end": actual_event_end,
            "available_time_start": actual_available_start,
            "available_time_end": actual_available_end,
            "findings": findings,
        }

'''
replace_between(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    "    def validate_catalog(self, request: CatalogValidationRequest) -> dict[str, Any]:\n",
    "    @contextmanager\n",
    validate_section,
)

replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''        instruments = [
            NautilusGatewayEngine._instrument(item).id for item in request.instrument_ids
        ]
''',
    '''        instruments = [InstrumentId.from_str(item) for item in request.instrument_ids]
''',
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''        catalog_path = self._catalog_path(request.catalog_key)
        instruments = [self._instrument(item) for item in request.instrument_ids]
''',
    '''        catalog_path = self._catalog_path(request.catalog_key)
        catalog = ParquetDataCatalog(path=str(catalog_path))
        instruments = self._catalog_instruments(catalog, request.instrument_ids)
''',
)

sealed_helpers = '''def _finite_metric(value: Any) -> float | None:
    try:
        result = float(value)
    except (TypeError, ValueError):
        return None
    if result != result or result in {float("inf"), float("-inf")}:
        return None
    return result


def _metric_by_fragment(value: Any, fragment: str) -> float | None:
    normalized_fragment = fragment.casefold()
    if isinstance(value, dict):
        for key, item in value.items():
            if normalized_fragment in str(key).casefold():
                metric = _finite_metric(item)
                if metric is not None:
                    return metric
            nested = _metric_by_fragment(item, fragment)
            if nested is not None:
                return nested
    elif isinstance(value, list):
        for item in value:
            nested = _metric_by_fragment(item, fragment)
            if nested is not None:
                return nested
    return None


def _pnl_totals(value: Any) -> dict[str, float]:
    if not isinstance(value, dict):
        return {}
    totals: dict[str, float] = {}
    for currency, stats in value.items():
        metric = _metric_by_fragment(stats, "pnl (total)")
        if metric is None and not isinstance(stats, dict):
            metric = _finite_metric(stats)
        if metric is not None:
            totals[str(currency)] = metric
    if not totals:
        metric = _metric_by_fragment(value, "pnl (total)")
        if metric is not None:
            totals["BASE"] = metric
    return totals


def _sealed_performance_disclosure(raw: dict[str, Any]) -> dict[str, Any]:
    statistics = raw.get("statistics") if isinstance(raw.get("statistics"), dict) else {}
    returns = statistics.get("returns") if isinstance(statistics, dict) else {}
    general = statistics.get("general") if isinstance(statistics, dict) else {}
    pnl_totals = _pnl_totals(raw.get("pnl"))
    sharpe = _metric_by_fragment(returns, "sharpe ratio")
    max_drawdown = _metric_by_fragment(returns, "max drawdown")
    profit_factor = _metric_by_fragment(raw.get("pnl"), "profit factor")
    if profit_factor is None:
        profit_factor = _metric_by_fragment(general, "profit factor")

    trade_evidence = bool(raw.get("orders") and raw.get("fills") and raw.get("positions"))
    pnl_pass = bool(pnl_totals) and all(value > 0.0 for value in pnl_totals.values())
    sharpe_pass = sharpe is None or sharpe >= 0.0
    drawdown_pass = max_drawdown is None or max_drawdown >= -0.50
    profit_factor_pass = profit_factor is None or profit_factor >= 1.0
    passed = trade_evidence and pnl_pass and sharpe_pass and drawdown_pass and profit_factor_pass

    quality_score = 0.0
    if passed:
        quality_score = 0.60
        if sharpe is not None and sharpe >= 1.0:
            quality_score += 0.10
        if max_drawdown is not None and max_drawdown >= -0.10:
            quality_score += 0.05
        if profit_factor is not None and profit_factor >= 1.50:
            quality_score += 0.05
        quality_score = min(0.80, quality_score)

    return {
        "passed": passed,
        "quality_score": quality_score,
        "performance": {
            "pnl_totals": pnl_totals,
            "sharpe_ratio": sharpe,
            "max_drawdown": max_drawdown,
            "profit_factor": profit_factor,
        },
        "order_count": len(raw.get("orders") or []),
        "fill_count": len(raw.get("fills") or []),
        "position_count": len(raw.get("positions") or []),
        "policy_checks": {
            "transaction_evidence": trade_evidence,
            "positive_total_pnl": pnl_pass,
            "non_negative_sharpe_when_available": sharpe_pass,
            "max_drawdown_floor": drawdown_pass,
            "profit_factor_floor_when_available": profit_factor_pass,
        },
        "policy": "SEALED_PERFORMANCE_RISK_V1",
    }


'''
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    "class GatewayContractError(ValueError):\n    pass\n\n\n",
    "class GatewayContractError(ValueError):\n    pass\n\n\n" + sealed_helpers,
)
replace_once(
    "nautilus_runtime/src/quazonai_nautilus_gateway/engine.py",
    '''        statistics = raw["statistics"]
        disclosure = {
            "passed": bool(raw["fills"] and raw["positions"]),
            "statistics": statistics,
            "pnl_summary": raw["pnl"],
            "order_count": len(raw["orders"]),
            "fill_count": len(raw["fills"]),
            "position_count": len(raw["positions"]),
            "policy": "AGGREGATES_ONLY_V1",
        }
''',
    '''        disclosure = _sealed_performance_disclosure(raw)
''',
)

# Remove the obsolete opaque payload now; the maintenance workflow deletes this patch itself.
Path("tools/issue22_payload.b64").unlink(missing_ok=True)
