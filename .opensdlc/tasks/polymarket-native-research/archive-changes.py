"""Exact native archive changes. Applied by Actions, then removed."""
from pathlib import Path

def replace(path, old, new):
    file = Path(path)
    text = file.read_text()
    if old not in text:
        raise RuntimeError(f"Missing authored context in {path}: {old!r}")
    file.write_text(text.replace(old, new))

p = "apps/job/src/bin/polymarket-history.rs"
replace(p, 'data::{Bar, OrderBookDelta, QuoteTick, TradeTick}', 'data::{Bar, InstrumentClose, OrderBookDelta, QuoteTick, TradeTick}')
replace(p, 'enums::PriceType,', 'enums::{InstrumentCloseType, PriceType},')
replace(p, '    bars: Vec<Bar>,\n', '    bars: Vec<Bar>,\n    #[serde(default)]\n    closes: Vec<InstrumentClose>,\n')
replace(p, '    bars: usize,\n', '    bars: usize,\n    closes: usize,\n')
replace(p, '.and_then(|n| n.checked_add(archive.bars.len()))', '.and_then(|n| n.checked_add(archive.bars.len()))\n        .and_then(|n| n.checked_add(archive.closes.len()))')
replace(p, '    for bar in &archive.bars {', '''    let mut settled = BTreeSet::new();
    for close in &archive.closes {
        ensure!(ids.contains(&close.instrument_id), "UNMAPPED_INSTRUMENT_CLOSE");
        ensure!(close.close_type == InstrumentCloseType::ContractExpired
            && valid_price(close.close_price.as_decimal()), "INVALID_BINARY_CLOSE");
        ensure!(settled.insert(close.instrument_id), "DUPLICATE_BINARY_CLOSE");
        time_order(close.ts_event, close.ts_init)?;
    }
    for bar in &archive.bars {''')
replace(p, '    archive.bars.sort_by_key(|r| (r.ts_init, r.ts_event));', '    archive.bars.sort_by_key(|r| (r.ts_init, r.ts_event));\n    archive.closes.sort_by_key(|r| (r.ts_init, r.ts_event));')
replace(p, '    let report = ImportReport {', '''    if !archive.closes.is_empty() {
        catalog.write_to_parquet(&archive.closes, None, None, None)?;
    }
    let report = ImportReport {''')
replace(p, '        bars: archive.bars.len(),', '        bars: archive.bars.len(),\n        closes: archive.closes.len(),')
replace(p, '        bars: Vec::new(),', '        bars: Vec::new(),\n        closes: Vec::new(),')
replace(p, '    #[test]\n    fn invalid_or_duplicate_records_do_not_publish()', '''    #[test]
    fn source_settlement_events_round_trip_separately_from_trades() {
        let directory = tempfile::tempdir().unwrap();
        let target = directory.path().join("settlement");
        let mut input = archive();
        input.closes = vec![InstrumentClose::new(input.instruments[0].id(), Price::from("0.5000"),
            InstrumentCloseType::ContractExpired, 1000_u64.into(), 1200_u64.into())];
        let expected = input.closes[0];
        let report = import(input, &target).unwrap();
        assert_eq!(report.closes, 1);
        let path = target.join("catalog");
        let mut catalog = ParquetDataCatalog::from_uri(path.to_str().unwrap(), None, None, None, None).unwrap();
        let rows = catalog.query::<InstrumentClose>(None, None, None, None, None, true).unwrap()
            .collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(rows, vec![nautilus_model::data::Data::InstrumentClose(expected)]);
        assert_eq!(report.bars, 0);
        assert_eq!(report.historical_availability, "UNVERIFIED");
    }

    #[test]
    fn invalid_or_duplicate_records_do_not_publish()''')

p = "docs/polymarket-history.md"
replace(p, '| bars | 原生成交 LAST/EXTERNAL Bar 列表，可省略；不是价格观察的占位 OHLCV |', '| bars | 原生成交 LAST/EXTERNAL Bar 列表，可省略；不是价格观察的占位 OHLCV |\n| closes | 原生 InstrumentClose/ContractExpired 列表，可省略；只接受来源记录，不推断赢家 |')
replace(p, '本工具不推断结算、赎回、资金占用，', '本工具保留原 InstrumentClose 的价格与事件/可用时间，不推断结算、赎回、资金占用，')
replace(p, '默认 `job` 二进制和科学镜像不启用\n该特性', '默认 `job` 二进制和科学镜像不启用\n该命令入口')

p = "docs/research/reuse.md"
replace(p, '新增依赖只属于显式启用的操作员数据准备二进制，不改变默认科学\njob 的网络边界。', '原生 PolymarketFeeModel 同时用于科学 job；HTTP 获取入口只属于显式启用的\n操作员数据准备二进制。科学 OCI 仍为 network=none，不改变网络边界。')
