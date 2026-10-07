//! Controlled original-JSON/native-parser agreement, not a network acquisition.
use super::*;
use contracts::Id;
use nautilus_model::{
    identifiers::{InstrumentId, Symbol},
    instruments::CurrencyPair,
    types::{Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
const START: u64 = 86_400_000;
fn bundle() -> FrozenSpotCandleSourceV1 {
    let instrument = InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(InstrumentId::from("BTC-SPOT.HYPERLIQUID"))
            .raw_symbol(Symbol::new("@142"))
            .base_currency(Currency::BTC())
            .quote_currency(Currency::USDC())
            .price_precision(2)
            .size_precision(4)
            .price_increment(Price::from("0.01"))
            .size_increment(Quantity::from("0.0001"))
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap(),
    );
    let mut bundle = FrozenSpotCandleSourceV1 {
        schema_version: SchemaV1,
        capture_id: Id::new(),
        method: SpotCashSourceMethodV1::HyperliquidPublicRestCandleSnapshot,
        network: SpotCashSourceNetworkV1::Mainnet,
        native_version: "0.63.0".into(),
        historical_availability: SpotCashHistoricalAvailabilityV1::Unverified,
        instrument_definition: serde_json::to_value(instrument).unwrap(),
        instrument_request_started_ns: count(1).unwrap(),
        instrument_received_ns: count(2).unwrap(),
        bar_type: "BTC-SPOT.HYPERLIQUID-1-MINUTE-LAST-EXTERNAL".into(),
        responses: vec![],
        selected_rows: vec![],
    };
    for index in 0..3 {
        let open = START + index * MINUTE_MS;
        let close = open + MINUTE_MS;
        bundle.responses.push(FrozenSpotCandleResponseV1 {
            sequence: count(index + 1).unwrap(), coin: "@142".into(), interval: "1m".into(),
            requested_start_ms: count(open).unwrap(), requested_end_ms: count(close - 1).unwrap(),
            request_started_ns: count(nanos(close).unwrap() + 100_000_000).unwrap(),
            received_ns: count(nanos(close).unwrap() + 500_000_000).unwrap(),
            response: serde_json::json!([{"t":open,"T":close-1,"s":"@142","i":"1m","o":"100.00","h":"100.00","l":"100.00","c":"100.00","v":"100.0000","n":1}]),
        });
        bundle.selected_rows.push(FrozenSpotCandleRowV1 {
            response_sequence: count(index + 1).unwrap(),
            source_row_index: DbCounter::ZERO,
        });
    }
    bundle
}
fn catalog(bundle: &FrozenSpotCandleSourceV1) -> (tempfile::TempDir, NativeBarSelectionV1) {
    let (instrument, rows) = source_rows(bundle).unwrap();
    let root = tempfile::tempdir().unwrap();
    let native =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    native.write_instruments(vec![instrument]).unwrap();
    native
        .write_to_parquet(
            &rows.values().map(|row| row.native).collect::<Vec<_>>(),
            None,
            None,
            None,
        )
        .unwrap();
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: vec![bundle.bar_type.clone()],
        event_start_ns: count(nanos(START).unwrap()).unwrap(),
        event_end_ns: count(nanos(START + 3 * MINUTE_MS).unwrap()).unwrap(),
        decision_cutoff_ns: bundle.responses.last().unwrap().received_ns,
        maximum_rows: 3,
    };
    write_bundle(root.path(), bundle).unwrap();
    (root, selection)
}
#[test]
fn offline_catalog_roundtrip_preserves_existing_source_bundle() {
    let original = bundle();
    // This remains a controlled fixture, not newly collected market data or
    // a claim of third-party provenance. All native I/O is local to the fixture.
    let (root, selection) = catalog(&original);
    let proof = load_closed_rows(root.path(), &selection).unwrap();
    assert_eq!(proof.rows.len(), 3);
    assert_eq!(
        proof.evidence.historical_availability,
        SpotCashHistoricalAvailabilityV1::Unverified
    );
    assert_eq!(
        serde_json::to_value(read_bundle(root.path()).unwrap()).unwrap(),
        serde_json::to_value(&original).unwrap()
    );
    assert_eq!(load_closed_rows(root.path(), &selection).unwrap().rows.len(), 3);
}

#[test]
fn official_native_open_receipt_and_source_close_remain_distinct() {
    let original = bundle();
    let (root, selection) = catalog(&original);
    let proof = load_closed_rows(root.path(), &selection).unwrap();
    assert_eq!(proof.rows.len(), 3);
    for row in &proof.rows {
        assert_eq!(row.event_time, NativeSpotBarEventTimeV1::Open);
        assert_eq!(row.native.ts_event.as_u64(), row.bar_open_ns.get());
        assert_eq!(
            row.bar_close_ns.get() - row.bar_open_ns.get(),
            60_000_000_000
        );
        assert_eq!(
            row.native.ts_init.as_u64() - row.bar_close_ns.get(),
            500_000_000
        );
    }
    assert_eq!(
        proof.evidence.historical_availability,
        SpotCashHistoricalAvailabilityV1::Unverified
    );
}
#[test]
fn frozen_source_mutations_cannot_self_attest_against_unchanged_parquet() {
    let original = bundle();
    let (root, selection) = catalog(&original);
    for case in 0..8 {
        let mut changed = original.clone();
        match case {
            0 => changed.responses[0].response[0]["c"] = serde_json::json!("99.00"),
            1 => changed.responses[0].response[0]["T"] = serde_json::json!(START + MINUTE_MS),
            2 => changed.responses[0].response[0]["s"] = serde_json::json!("@143"),
            3 => changed.responses[0].received_ns = count(nanos(START).unwrap()).unwrap(),
            4 => changed.selected_rows[0].source_row_index = count(9).unwrap(),
            5 => changed.selected_rows[1] = changed.selected_rows[0].clone(),
            6 => {
                changed.selected_rows.remove(0);
            }
            7 => {
                changed.instrument_definition["CurrencyPair"]["price_precision"] =
                    serde_json::json!(3)
            }
            _ => unreachable!(),
        }
        assert!(
            verified(&changed, root.path(), &selection).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn selection_preserves_original_json_and_can_never_become_verified_pit() {
    let original = bundle();
    let (root, mut selection) = catalog(&original);
    selection.event_start_ns = count(nanos(START + MINUTE_MS).unwrap()).unwrap();
    selection.maximum_rows = 2;
    let destination = tempfile::tempdir().unwrap();
    let actual = catalog::load_catalog(root.path(), &selection).unwrap();
    let native = ParquetDataCatalog::from_uri(
        destination.path().to_str().unwrap(),
        None,
        Some(16),
        None,
        None,
    )
    .unwrap();
    native
        .write_instruments(vec![actual.series[0].instrument.clone()])
        .unwrap();
    native
        .write_to_parquet(&actual.series[0].bars, None, None, None)
        .unwrap();
    preserve_for_selection(root.path(), destination.path(), &selection).unwrap();
    let reduced = read_bundle(destination.path()).unwrap();
    assert_eq!(
        serde_json::to_value(&reduced.responses).unwrap(),
        serde_json::to_value(&original.responses[1..]).unwrap()
    );
    assert_eq!(reduced.selected_rows.len(), 2);
    assert_eq!(
        load_closed_rows(destination.path(), &selection)
            .unwrap()
            .rows
            .len(),
        2
    );
    let mut encoded = serde_json::to_value(&reduced).unwrap();
    encoded["historical_availability"] = serde_json::json!("VERIFIED");
    assert!(serde_json::from_value::<FrozenSpotCandleSourceV1>(encoded).is_err());
}
#[test]
fn closed_row_rewrites_and_perpetual_api_symbols_are_rejected() {
    let mut changed = bundle();
    let mut repeated = changed.responses[0].response[0].clone();
    repeated["c"] = serde_json::json!("99.00");
    changed.responses[1].requested_start_ms = count(START).unwrap();
    changed.responses[1]
        .response
        .as_array_mut()
        .unwrap()
        .push(repeated);
    assert!(source_rows(&changed).is_err());
    let mut changed = bundle();
    changed.instrument_definition["CurrencyPair"]["raw_symbol"] = serde_json::json!("BTC");
    assert!(source_rows(&changed).is_err());
}

#[test]
fn malformed_ohlcv_is_an_error_before_the_official_unchecked_bar_constructor() {
    let mut source = bundle();
    source.responses[0].response[0]["c"] = serde_json::json!("99.00");
    let result = std::panic::catch_unwind(|| source_rows(&source));
    assert!(result.is_ok(), "source validation must not panic");
    assert!(
        result
            .unwrap()
            .unwrap_err()
            .to_string()
            .contains("SPOT_SOURCE_OHLCV_INVALID")
    );
}

#[test]
fn omitting_a_row_from_both_selector_and_parquet_cannot_hide_original_evidence() {
    let original = bundle();
    let (instrument, rows) = source_rows(&original).unwrap();
    let mut reduced = original.clone();
    reduced.selected_rows.remove(1);
    let root = tempfile::tempdir().unwrap();
    let native =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    native.write_instruments(vec![instrument]).unwrap();
    let selected = reduced
        .selected_rows
        .iter()
        .map(|key| rows[key].native)
        .collect::<Vec<_>>();
    native
        .write_to_parquet(&selected, None, None, None)
        .unwrap();
    write_bundle(root.path(), &reduced).unwrap();
    let selection = NativeBarSelectionV1 {
        schema_version: SchemaV1,
        bar_types: vec![original.bar_type],
        event_start_ns: count(nanos(START).unwrap()).unwrap(),
        event_end_ns: count(nanos(START + 3 * MINUTE_MS).unwrap()).unwrap(),
        decision_cutoff_ns: original.responses.last().unwrap().received_ns,
        maximum_rows: 3,
    };
    let error = load_closed_rows(root.path(), &selection)
        .err()
        .expect("omitted original row must fail");
    assert!(
        error
            .to_string()
            .contains("SPOT_SOURCE_ELIGIBLE_ROW_OMITTED")
    );
}
