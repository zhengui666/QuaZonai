from pathlib import Path

p = Path('apps/job/src/bin/polymarket-history.rs')
s = p.read_text()
old = '''    archive.trades.sort_by_key(|r| (r.ts_init, r.ts_event));
    archive.quotes.sort_by_key(|r| (r.ts_init, r.ts_event));
    archive.deltas.sort_by_key(|r| (r.ts_init, r.ts_event));
    archive.bars.sort_by_key(|r| (r.ts_init, r.ts_event));
    archive.closes.sort_by_key(|r| (r.ts_init, r.ts_event));
    if !archive.trades.is_empty() {
        catalog.write_to_parquet(&archive.trades, None, None, None)?;
    }
    if !archive.quotes.is_empty() {
        catalog.write_to_parquet(&archive.quotes, None, None, None)?;
    }
    if !archive.deltas.is_empty() {
        catalog.write_to_parquet(&archive.deltas, None, None, None)?;
    }
    if !archive.bars.is_empty() {
        catalog.write_to_parquet(&archive.bars, None, None, None)?;
    }
    if !archive.closes.is_empty() {
        catalog.write_to_parquet(&archive.closes, None, None, None)?;
    }'''
new = '''    // Native Parquet metadata belongs to one instrument (one BarType for bars).
    // Stable ordering preserves the source order of simultaneous book updates.
    archive.trades.sort_by_key(|r| (r.instrument_id, r.ts_init, r.ts_event));
    archive.quotes.sort_by_key(|r| (r.instrument_id, r.ts_init, r.ts_event));
    archive.deltas.sort_by_key(|r| (r.instrument_id, r.ts_init, r.ts_event));
    archive.bars.sort_by_key(|r| (r.bar_type, r.ts_init, r.ts_event));
    archive.closes.sort_by_key(|r| (r.instrument_id, r.ts_init, r.ts_event));
    for rows in archive.trades.chunk_by(|a, b| a.instrument_id == b.instrument_id) {
        catalog.write_to_parquet(rows, None, None, None)?;
    }
    for rows in archive.quotes.chunk_by(|a, b| a.instrument_id == b.instrument_id) {
        catalog.write_to_parquet(rows, None, None, None)?;
    }
    for rows in archive.deltas.chunk_by(|a, b| a.instrument_id == b.instrument_id) {
        catalog.write_to_parquet(rows, None, None, None)?;
    }
    for rows in archive.bars.chunk_by(|a, b| a.bar_type == b.bar_type) {
        catalog.write_to_parquet(rows, None, None, None)?;
    }
    for rows in archive.closes.chunk_by(|a, b| a.instrument_id == b.instrument_id) {
        catalog.write_to_parquet(rows, None, None, None)?;
    }'''
assert s.count(old) == 1
s = s.replace(old, new)
needle = '''    #[test]
    fn invalid_or_duplicate_records_do_not_publish()'''
addition = '''    #[test]
    fn multiple_outcomes_and_bar_types_round_trip_in_distinct_native_partitions() {
        use nautilus_model::{
            data::{BarType, BookOrder},
            enums::{BookAction, OrderSide, RecordFlag},
        };
        let mut input = archive();
        let second_id = InstrumentId::from_str("test-condition-987654321098765432109876543210.POLYMARKET").unwrap();
        let mut definition = serde_json::to_value(&input.instruments[0]).unwrap();
        definition["BinaryOption"]["id"] = second_id.to_string().into();
        definition["BinaryOption"]["raw_symbol"] = "987654321098765432109876543210".into();
        input.instruments.push(serde_json::from_value(definition).unwrap());
        let original_trades = input.trades.clone();
        for mut trade in original_trades {
            trade.instrument_id = second_id;
            input.trades.push(trade);
        }
        for id in input.instruments.iter().map(Instrument::id) {
            input.quotes.push(QuoteTick::new(
                id, Price::from("0.4000"), Price::from("0.4500"),
                Quantity::from("1.000000"), Quantity::from("2.000000"),
                30_u64.into(), 31_u64.into(),
            ));
            input.deltas.push(OrderBookDelta::new(
                id, BookAction::Add,
                BookOrder::new(OrderSide::Buy, Price::from("0.4000"), Quantity::from("1.000000"), 1),
                RecordFlag::F_LAST as u8, 1, 40_u64.into(), 41_u64.into(),
            ));
            for minutes in [1, 5] {
                input.bars.push(Bar::new_checked(
                    BarType::from_str(&format!("{id}-{minutes}-MINUTE-LAST-EXTERNAL")).unwrap(),
                    Price::from("0.4000"), Price::from("0.4500"), Price::from("0.3500"), Price::from("0.4200"),
                    Quantity::from("10.000000"), 50_u64.into(), 51_u64.into(),
                ).unwrap());
            }
            input.closes.push(InstrumentClose::new(
                id, Price::from("0.5000"), InstrumentCloseType::ContractExpired,
                1000_u64.into(), 1200_u64.into(),
            ));
        }
        let ids = input.instruments.iter().map(Instrument::id).collect::<Vec<_>>();
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("multiple-outcomes");
        let report = import(input, &output).unwrap();
        assert_eq!((report.instruments, report.trades, report.quotes, report.deltas, report.bars, report.closes), (2, 4, 2, 2, 4, 2));
        let root = output.join("catalog");
        let mut catalog = ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, None, None, None).unwrap();
        for id in ids {
            macro_rules! rows {
                ($kind:ty, $key:expr) => {
                    catalog.query::<$kind>(Some(vec![$key]), None, None, None, None, true)
                        .unwrap().collect::<Result<Vec<_>, _>>().unwrap()
                };
            }
            assert_eq!(rows!(TradeTick, id.to_string()).len(), 2);
            assert_eq!(rows!(QuoteTick, id.to_string()).len(), 1);
            assert_eq!(rows!(OrderBookDelta, id.to_string()).len(), 1);
            assert_eq!(rows!(InstrumentClose, id.to_string()).len(), 1);
            for minutes in [1, 5] {
                assert_eq!(rows!(Bar, format!("{id}-{minutes}-MINUTE-LAST-EXTERNAL")).len(), 1);
            }
        }
    }

'''
assert s.count(needle) == 1
s = s.replace(needle, addition + needle)
p.write_text(s)

p = Path('apps/job/tests/polymarket.rs')
s = p.read_text()
s = s.replace('format!("{error:?}")', 'format!("{error:?}; domain={:?}", error.downcast_ref::<domain::DomainError>())')
needle = '''    assert!(request.members.len() >= 2);'''
assert s.count(needle) == 1
s = s.replace(needle, '''    domain::execution::portfolio_study_cutoffs(&request).expect("synthetic study input contract");
''' + needle)
p.write_text(s)
