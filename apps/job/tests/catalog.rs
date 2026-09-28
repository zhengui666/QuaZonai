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

fn definition(original: &InstrumentAny, tick: &str, event: u64, available: u64) -> InstrumentAny {
    let mut value = serde_json::to_value(original).unwrap();
    let payload = value.as_object_mut().unwrap().values_mut().next().unwrap();
    payload["price_increment"] = tick.into();
    payload["ts_event"] = event.into();
    payload["ts_init"] = available.into();
    serde_json::from_value(value).unwrap()
}

#[test]
fn parquet_tick_history_keeps_late_rows_controls_after_event_end_and_original_baselines() {
    let directory = tempfile::tempdir().unwrap();
    let (original, mut bars, mut selection) = fixture();
    let original = definition(&original, "0.00100", 0, 0);
    let update = definition(&original, "0.00010", 210_000_000_000, 210_000_000_001);
    bars[0].ts_init = 200_000_000_000_u64.into();
    bars[1].ts_init = 230_000_000_000_u64.into();
    bars[1].close = Price::from("0.65550");
    bars[2].ts_init = 260_000_000_000_u64.into();
    bars[3].ts_init = 280_000_000_000_u64.into();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(2),
        None,
        None,
    )
    .unwrap();
    catalog
        .write_instruments(vec![original.clone(), update.clone()])
        .unwrap();
    catalog.write_to_parquet(&bars, None, None, None).unwrap();
    selection.event_end_ns = DbCounter::new(150_000_000_000).unwrap();
    let result = load_catalog(directory.path(), &selection).unwrap();
    assert_eq!(result.rows, 2);
    assert_eq!(result.series[0].bars, bars[..2]);
    assert_eq!(
        serde_json::to_value(&result.series[0].instrument_updates).unwrap(),
        serde_json::to_value([&update]).unwrap()
    );
    assert_eq!(
        result.series[0]
            .instrument_at(200_000_000_000)
            .unwrap()
            .price_increment(),
        original.price_increment()
    );
    assert_eq!(
        result.series[0]
            .instrument_at(230_000_000_000)
            .unwrap()
            .price_increment(),
        update.price_increment()
    );
    selection.decision_cutoff_ns = DbCounter::new(205_000_000_000).unwrap();
    let before = load_catalog(directory.path(), &selection).unwrap();
    assert_eq!(before.series[0].bars, bars[..1]);
    assert!(before.series[0].instrument_updates.is_empty());
    selection.event_start_ns = DbCounter::new(225_000_000_000).unwrap();
    selection.event_end_ns = DbCounter::new(300_000_000_000).unwrap();
    selection.decision_cutoff_ns = selection.event_end_ns;
    let later = load_catalog(directory.path(), &selection).unwrap();
    assert_eq!(later.series[0].bars, bars[3..]);
    assert_eq!(
        serde_json::to_value(&later.series[0].instrument).unwrap(),
        serde_json::to_value(&update).unwrap()
    );
    assert!(later.series[0].instrument_updates.is_empty());
}

#[test]
fn parquet_boundary_update_keeps_its_baseline_and_rejects_a_tied_bar() {
    let boundary = 60_000_000_000;
    for (earlier_definition, delay) in [(true, 0), (true, 1), (false, 0)] {
        let directory = tempfile::tempdir().unwrap();
        let (original, mut bars, mut selection) = fixture();
        let update = definition(&original, "0.00010", boundary - 1, boundary);
        bars[0].ts_init = (boundary + delay).into();
        selection.event_start_ns = DbCounter::new(boundary).unwrap();
        let catalog = ParquetDataCatalog::from_uri(
            directory.path().to_str().unwrap(),
            None,
            Some(2),
            None,
            None,
        )
        .unwrap();
        let definitions = if earlier_definition {
            vec![original.clone(), update.clone()]
        } else {
            vec![update.clone()]
        };
        catalog.write_instruments(definitions).unwrap();
        catalog.write_to_parquet(&bars, None, None, None).unwrap();
        let result = load_catalog(directory.path(), &selection);
        if earlier_definition && delay == 0 {
            assert!(result
                .err()
                .unwrap()
                .to_string()
                .contains("CATALOG_AMBIGUOUS_INSTRUMENT_UPDATE"));
            continue;
        }
        let data = result.unwrap();
        assert_eq!(data.series[0].bars, bars);
        let expected_baseline = if earlier_definition {
            &original
        } else {
            &update
        };
        assert_eq!(
            serde_json::to_value(&data.series[0].instrument).unwrap(),
            serde_json::to_value(expected_baseline).unwrap()
        );
        assert_eq!(
            data.series[0].instrument_updates.len(),
            usize::from(earlier_definition)
        );
        assert_eq!(
            serde_json::to_value(data.series[0].instrument_at(boundary).unwrap()).unwrap(),
            serde_json::to_value(&update).unwrap()
        );
    }
}

#[test]
fn tick_history_rejects_missing_baseline_ambiguous_ties_and_crossing_invalid_ohlc() {
    let (original, bars, selection) = fixture();
    let update = definition(
        &original,
        "0.00010",
        119_000_000_000,
        bars[1].ts_init.as_u64(),
    );
    let error = validate_native(
        vec![original.clone(), update.clone()],
        bars.clone(),
        &selection,
    )
    .err()
    .unwrap();
    assert!(error
        .to_string()
        .contains("CATALOG_AMBIGUOUS_INSTRUMENT_UPDATE"));
    assert!(validate_native(vec![update], bars.clone(), &selection).is_err());
    let update = definition(&original, "0.00010", 100_000_000_000, 110_000_000_000);
    let mut crossing = bars.clone();
    crossing[1].open = Price::from("0.65001");
    assert!(validate_native(vec![original.clone(), update.clone()], crossing, &selection).is_err());
    let mut bounds = serde_json::to_value(&update).unwrap();
    bounds["CurrencyPair"]["max_price"] = "0.65000".into();
    let bounds = serde_json::from_value(bounds).unwrap();
    assert!(validate_native(vec![original, bounds], bars, &selection).is_err());
}

#[test]
fn original_static_definition_after_empty_selection_start_is_not_backdated_or_rejected() {
    let (original, bars, selection) = fixture();
    let original = definition(&original, "0.00001", 10_000_000_000, 20_000_000_000);
    let data = validate_native(vec![original.clone()], bars.clone(), &selection).unwrap();
    assert_eq!(data.series[0].instrument.ts_init(), original.ts_init());
    assert!(data.series[0].instrument_at(19_999_999_999).is_err());
    assert_eq!(data.rows, bars.len());
    let unavailable = definition(
        &original,
        "0.00001",
        10_000_000_000,
        bars[0].ts_init.as_u64() + 1,
    );
    assert!(validate_native(vec![unavailable], bars, &selection).is_err());
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

#[test]
fn compressed_native_catalog_cannot_expand_past_the_decoded_row_limit() {
    let directory = tempfile::tempdir().unwrap();
    let (instrument, original, mut selection) = fixture();
    let bars: Vec<_> = (1..=50_000_u64)
        .map(|timestamp| {
            let mut bar = original[0];
            bar.ts_event = timestamp.into();
            bar.ts_init = (timestamp + 1).into();
            bar
        })
        .collect();
    let catalog = ParquetDataCatalog::from_uri(
        directory.path().to_str().unwrap(),
        None,
        Some(4096),
        None,
        None,
    )
    .unwrap();
    assert_eq!(format!("{:?}", catalog.compression), "SNAPPY");
    catalog.write_instruments(vec![instrument]).unwrap();
    let path = directory
        .path()
        .join(catalog.write_to_parquet(&bars, None, None, None).unwrap());
    let native_directory = catalog.make_path(
        <nautilus_model::data::Bar as nautilus_persistence::backend::catalog::CatalogPathPrefix>::path_prefix(),
        Some(&bars[0].bar_type.to_string()),
    ).unwrap();
    let files = catalog.list_parquet_files(&native_directory).unwrap();
    assert_eq!(files.len(), 1);
    assert_eq!(directory.path().join(&files[0]), path);
    let bytes = std::fs::read(&path).unwrap();
    assert!(
        bytes.len() * 4 < std::mem::size_of_val(bars.as_slice()),
        "fixture must actually expand substantially after native decoding"
    );
    selection.maximum_rows = 3;
    let error = load_catalog(directory.path(), &selection).err().unwrap();
    assert_eq!(error.to_string(), "CATALOG_ROW_LIMIT");
    assert_eq!(std::fs::read(path).unwrap(), bytes);
}

#[test]
fn malformed_native_parquet_is_rejected_without_rewriting_the_input() {
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
    catalog.write_instruments(vec![instrument]).unwrap();
    let path = directory
        .path()
        .join(catalog.write_to_parquet(&bars, None, None, None).unwrap());
    let original = std::fs::read(&path).unwrap();
    assert_eq!(load_catalog(directory.path(), &selection).unwrap().rows, 4);
    let footer = original.len() - 8;
    let metadata_len =
        u32::from_le_bytes(original[footer..footer + 4].try_into().unwrap()) as usize;
    let data_end = footer - metadata_len;
    assert!(data_end > 4);
    for variant in [
        "truncated_footer",
        "oversized_metadata",
        "invalid_compressed_pages",
    ] {
        let mut corrupted = original.clone();
        match variant {
            "truncated_footer" => corrupted.truncate(footer),
            "oversized_metadata" => {
                corrupted[footer..footer + 4].copy_from_slice(&u32::MAX.to_le_bytes())
            }
            _ => corrupted[4..data_end].fill(0xff),
        }
        std::fs::write(&path, &corrupted).unwrap();
        let error = load_catalog(directory.path(), &selection)
            .err()
            .expect(variant);
        if variant == "oversized_metadata" {
            assert_eq!(error.to_string(), "CATALOG_PARQUET_FOOTER_INVALID");
        }
        assert_eq!(std::fs::read(&path).unwrap(), corrupted);
    }
    std::fs::write(path, original).unwrap();
    let recovered = load_catalog(directory.path(), &selection).unwrap();
    assert_eq!(recovered.series[0].bars, bars);
}
