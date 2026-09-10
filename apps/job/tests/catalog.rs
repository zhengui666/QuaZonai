//! Actual native Parquet catalog round trips. Synthetic fixtures are never delivery evidence.
use contracts::{science::NativeBarSelectionV1, DbCounter, SchemaV1};
use job::catalog::{load_catalog, validate_native};
use nautilus_model::{
    data::{Bar, BarType},
    instruments::{stubs::audusd_sim, Instrument, InstrumentAny},
    types::{Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use std::str::FromStr;

fn fixture() -> (InstrumentAny, Vec<Bar>, NativeBarSelectionV1) {
    let instrument = InstrumentAny::CurrencyPair(audusd_sim());
    let kind = BarType::from_str(&format!("{}-1-MINUTE-LAST-EXTERNAL", instrument.id())).unwrap();
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
                (i * 60_000_000_000_u64 + 1).into(),
            )
            .unwrap()
        })
        .collect();
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: vec![kind.to_string()],
        event_start_ns: DbCounter::ZERO,
        event_end_ns: DbCounter::new(300_000_000_000).unwrap(),
        decision_cutoff_ns: DbCounter::new(300_000_000_000).unwrap(),
        maximum_rows: 4,
    };
    (instrument, bars, selection)
}

#[test]
fn native_catalog_round_trip_preserves_identity_values_and_times() {
    let directory = tempfile::tempdir().unwrap();
    let (instrument, bars, selection) = fixture();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(2),
        None,
        None,
    )
    .unwrap();
    catalog.write_instruments(vec![instrument.clone()]).unwrap();
    catalog.write_to_parquet(&bars, None, None, None).unwrap();
    let result = load_catalog(directory.path(), &selection).unwrap();
    assert_eq!(result.rows, 4);
    assert_eq!(result.series.len(), 1);
    assert_eq!(result.series[0].instrument.id(), instrument.id());
    assert_eq!(result.series[0].bars, bars);
    let mut limited = selection.clone();
    limited.maximum_rows = 3;
    assert!(load_catalog(directory.path(), &limited).is_err());
    let mut before = selection;
    before.event_end_ns = DbCounter::new(240_000_000_000).unwrap();
    before.maximum_rows = 3;
    let result = load_catalog(directory.path(), &before).unwrap();
    assert_eq!(result.rows, 3, "event interval is half-open");
}

#[test]
fn native_event_pushdown_keeps_late_available_rows_and_counts_only_the_requested_slice() {
    let directory = tempfile::tempdir().unwrap();
    let (instrument, mut bars, mut selection) = fixture();
    // The first two event rows were available only after the requested interval
    // ended. They are still valid for the later decision cutoff.
    bars[0].ts_init = 200_000_000_000_u64.into();
    bars[1].ts_init = 220_000_000_000_u64.into();
    bars[2].ts_init = 260_000_000_000_u64.into();
    bars[3].ts_init = 320_000_000_000_u64.into();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(2),
        None,
        None,
    )
    .unwrap();
    catalog.write_instruments(vec![instrument]).unwrap();
    catalog.write_to_parquet(&bars, None, None, None).unwrap();
    selection.event_end_ns = DbCounter::new(150_000_000_000).unwrap();
    selection.maximum_rows = 2;
    let selected = load_catalog(directory.path(), &selection).unwrap();
    assert_eq!(selected.series[0].bars, bars[..2]);
    assert_eq!(selected.rows, 2);
    selection.maximum_rows = 1;
    assert!(load_catalog(directory.path(), &selection).is_err());
    selection.maximum_rows = 2;
    selection.decision_cutoff_ns = DbCounter::new(210_000_000_000).unwrap();
    let earlier = load_catalog(directory.path(), &selection).unwrap();
    assert_eq!(earlier.series[0].bars, bars[..1]);
    // Broadening only the event interval still does not make the fourth row,
    // which was available after the decision cutoff, visible.
    selection.event_end_ns = DbCounter::new(300_000_000_000).unwrap();
    selection.decision_cutoff_ns = selection.event_end_ns;
    selection.maximum_rows = 3;
    let full_visible = load_catalog(directory.path(), &selection).unwrap();
    assert_eq!(full_visible.series[0].bars, bars[..3]);
}

#[test]
fn time_identity_duplicates_and_missing_definitions_fail_closed() {
    let (instrument, bars, selection) = fixture();
    assert!(validate_native(vec![], bars.clone(), &selection).is_err());
    assert!(validate_native(vec![instrument.clone(); 2], bars.clone(), &selection).is_err());
    let mut duplicated = bars.clone();
    duplicated[1] = duplicated[0];
    assert!(validate_native(vec![instrument.clone()], duplicated, &selection).is_err());
    let mut future = bars.clone();
    future[0].ts_init = 400_000_000_000_u64.into();
    assert!(validate_native(vec![instrument.clone()], future, &selection).is_err());
    let mut impossible = bars.clone();
    impossible[0].ts_init = 1_u64.into();
    assert!(validate_native(vec![instrument.clone()], impossible, &selection).is_err());
    let mut unknown = bars;
    unknown[0].bar_type = BarType::from_str("EUR/USD.SIM-1-MINUTE-LAST-EXTERNAL").unwrap();
    assert!(validate_native(vec![instrument], unknown, &selection).is_err());
}

#[test]
fn unrequested_aggregation_duplicate_assets_and_oversized_queries_are_rejected() {
    let (instrument, bars, selection) = fixture();
    for kind in [
        format!("{}-1-MINUTE-MID-EXTERNAL", instrument.id()),
        format!("{}-1-MINUTE-LAST-INTERNAL", instrument.id()),
        format!("{}-100-TICK-LAST-EXTERNAL", instrument.id()),
    ] {
        let mut request = selection.clone();
        request.bar_types = vec![kind];
        assert!(validate_native(vec![instrument.clone()], bars.clone(), &request).is_err());
    }
    let mut duplicate = selection.clone();
    duplicate.bar_types.push(duplicate.bar_types[0].clone());
    assert!(validate_native(vec![instrument.clone()], bars.clone(), &duplicate).is_err());
    let mut large = selection;
    large.maximum_rows = 1_000_001;
    assert!(validate_native(vec![instrument], bars, &large).is_err());
}

#[test]
fn malformed_prices_are_not_normalized_into_acceptable_market_data() {
    let (instrument, bars, selection) = fixture();
    let mut malformed = bars.clone();
    malformed[0].high = Price::from("0.64000");
    assert!(validate_native(vec![instrument.clone()], malformed, &selection).is_err());
    let mut precision = bars;
    precision[0].close = Price::from("0.655001");
    assert!(validate_native(vec![instrument], precision, &selection).is_err());
}

#[cfg(unix)]
#[test]
fn catalog_root_symlink_is_not_an_authorized_mount() {
    let directory = tempfile::tempdir().unwrap();
    let target = directory.path().join("target");
    std::fs::create_dir(&target).unwrap();
    let link = directory.path().join("catalog");
    std::os::unix::fs::symlink(target, &link).unwrap();
    assert!(load_catalog(&link, &fixture().2).is_err());
}
