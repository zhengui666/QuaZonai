//! Original batch observation clocks: native behavior, never historical PIT evidence.
use contracts::{science::NativeBarSelectionV1, DbCounter, SchemaV1};
use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_common::logging::logger::LoggerConfig;
use nautilus_model::{
    data::{Bar, BarType, Data},
    enums::{AccountType, BookType, OmsType},
    instruments::{stubs::audusd_sim, Instrument, InstrumentAny},
    types::{Money, Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use std::str::FromStr;

#[test]
fn upstream_catalog_and_engine_preserve_an_original_equal_time_observation_batch() {
    let directory = tempfile::tempdir().unwrap();
    let instrument = InstrumentAny::CurrencyPair(audusd_sim());
    let kind = BarType::from_str(&format!("{}-1-MINUTE-LAST-EXTERNAL", instrument.id())).unwrap();
    let received = 300_000_000_000_u64;
    let bars = (1..=4)
        .map(|i| {
            Bar::new_checked(
                kind,
                Price::from("0.65000"),
                Price::from("0.66000"),
                Price::from("0.64000"),
                Price::from("0.65500"),
                Quantity::from("100000"),
                (i * 60_000_000_000_u64).into(),
                received.into(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let mut catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(2),
        None,
        None,
    )
    .unwrap();
    catalog.write_instruments(vec![instrument.clone()]).unwrap();
    catalog.write_to_parquet(&bars, None, None, None).unwrap();
    let readback = catalog
        .query::<Bar>(Some(vec![kind.to_string()]), None, None, None, None, true)
        .unwrap()
        .map(|value| match value.unwrap() {
            Data::Bar(bar) => bar,
            _ => panic!("wrong native record type"),
        })
        .collect::<Vec<_>>();
    assert_eq!(readback, bars);
    assert!(readback.iter().all(|bar| bar.ts_init.as_u64() == received));
    let mut engine = BacktestEngine::new(BacktestEngineConfig {
        logging: LoggerConfig::builder()
            .bypass_logging(true)
            .build()
            .unwrap(),
        shutdown_on_error: true,
        bypass_logging: true,
        ..Default::default()
    })
    .unwrap();
    engine
        .add_venue(
            SimulatedVenueConfig::builder()
                .venue(instrument.venue())
                .oms_type(OmsType::Netting)
                .account_type(AccountType::Cash)
                .book_type(BookType::L1_MBP)
                .starting_balances(vec![Money::from("1000 USD")])
                .bar_execution(true)
                .build()
                .unwrap(),
        )
        .unwrap();
    engine.add_instrument(&instrument).unwrap();
    engine
        .add_data(
            readback.into_iter().map(Data::Bar).collect(),
            None,
            true,
            true,
        )
        .unwrap();
    engine.run(None, None, None, false).unwrap();
    assert_eq!(engine.iteration(), 4);
    // The QZ catalog accepts the same honest receipt batch without backdating it.
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: vec![kind.to_string()],
        event_start_ns: DbCounter::ZERO,
        event_end_ns: DbCounter::new(received).unwrap(),
        decision_cutoff_ns: DbCounter::new(received).unwrap(),
        maximum_rows: 4,
    };
    let actual = job::catalog::load_catalog(directory.path(), &selection).unwrap();
    assert_eq!(actual.series[0].bars, bars);
    assert_eq!(actual.rows, 4);
}
