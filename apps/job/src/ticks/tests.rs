//! Controlled native TradeTick/Wasmi counterexamples; not market-quality evidence.
use super::*;
use nautilus_model::{
    enums::AggressorSide,
    identifiers::{InstrumentId, TradeId},
    types::{Price, Quantity},
};
use serde_json::json;

pub(super) fn request() -> NativeTradeTickFactorEvaluationRequestV1 {
    serde_json::from_value(json!({"schema_version":1,
        "input":{"modality":"TRADE_TICK","instrument_ids":["AUD/USD.SIM"],"event_start_ns":"0","event_end_ns":"80","decision_cutoff_ns":"80","maximum_rows":100,"source_revision":"fixed-source","availability":"EVENT_TIME_PROXY_UNVERIFIED"},
        "sampling":{"kind":"EVENT_TIME_COHORT_V1","maximum_cohort_rows":100},
        "target":{"kind":"FIXED_TRADE_COHORTS","horizon_cohorts":1},
        "parameters":{"fast_period_cohorts":1,"slow_period_cohorts":2},
        "train_end_ns":"30","validation_end_ns":"50"})).unwrap()
}

fn trade(time: u64, id: &str, price: &str, quantity: &str) -> TradeTick {
    TradeTick::new(
        InstrumentId::from("AUD/USD.SIM"),
        Price::from(price),
        Quantity::from(quantity),
        AggressorSide::Buy,
        TradeId::from(id),
        time.into(),
        time.into(),
    )
}

pub(super) fn series() -> BTreeMap<String, Vec<TradeTick>> {
    BTreeMap::from([(
        "AUD/USD.SIM".to_owned(),
        (1..=7)
            .map(|i| trade(i * 10, &i.to_string(), &(10 + i).to_string(), "1"))
            .collect(),
    )])
}

pub(super) fn model(slot: i32) -> Vec<u8> {
    wat::parse_str(format!(r#"(module
        (global $value (mut f64) (f64.const 0))
        (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)
            local.get 0 i32.const {slot} i32.eq
            if local.get 1 global.set $value end)
        (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) global.get $value))"#)).unwrap()
}

#[test]
fn received_cohorts_preserve_late_events_and_use_receipt_clocks_for_features_labels_and_views() {
    let mut request = request();
    request.input.availability = NativeTickAvailabilityV1::ReceivedTimeObserved;
    request.input.maximum_receive_delay_ns = Some(counter(70).unwrap());
    request.sampling.kind = NativeTickSamplingKindV1::ReceivedTimeCohortV1;
    request.target = NativeTickTargetV1::FixedReceivedTime {
        horizon_ns: counter(5).unwrap(),
    };
    let rows = [
        (8, 10, "10"),
        (1, 20, "20"),
        (8, 20, "30"),
        (2, 40, "40"),
        (25, 55, "50"),
        (3, 60, "60"),
    ]
    .into_iter()
    .enumerate()
    .map(|(index, (event, received, price))| {
        let mut row = trade(event, &index.to_string(), price, "1");
        row.ts_init = received.into();
        row
    })
    .collect::<Vec<_>>();
    let reduced = sampled_cohorts(&rows, 100, false, true).unwrap();
    assert_eq!(
        reduced
            .iter()
            .map(|row| (
                row.event,
                row.source_event_start,
                row.source_event_end,
                row.count
            ))
            .collect::<Vec<_>>(),
        vec![
            (10, 8, 9, 1),
            (20, 1, 9, 2),
            (40, 2, 3, 1),
            (55, 25, 26, 1),
            (60, 3, 4, 1)
        ]
    );
    let wasm = wat::parse_str(r#"(module
        (global $event (mut i64) (i64.const 0))
        (global $available (mut i64) (i64.const 0))
        (global $elapsed (mut f64) (f64.const 0))
        (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)
            local.get 0 i32.eqz if local.get 3 global.set $event local.get 4 global.set $available end
            local.get 0 i32.const 6 i32.eq if local.get 1 global.set $elapsed end)
        (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
            local.get 0 global.get $available i64.ne if unreachable end
            local.get 1 global.get $event i64.ne if unreachable end
            global.get $elapsed))"#).unwrap();
    let original = evaluate_series(
        &request,
        BTreeMap::from([("AUD/USD.SIM".into(), rows.clone())]),
        &wasm,
        false,
    )
    .unwrap();
    let points = original.points.as_ref().unwrap();
    assert_eq!(
        points
            .iter()
            .skip(1)
            .map(|point| point.observation.forecast.unwrap())
            .collect::<Vec<_>>(),
        vec![10.0, 20.0, 15.0, 5.0]
    );
    assert_eq!(points[1].observation.event_ns.get(), 20);
    assert_eq!(points[1].observation.available_ns.get(), 20);
    assert_eq!(points[1].observation.label_available_ns.unwrap().get(), 40);
    assert_eq!(points[3].observation.label_available_ns.unwrap().get(), 60);
    assert!((points[3].observation.label_return.unwrap() - 0.2).abs() < 1e-12);
    assert_eq!(points[3].partition, NativeFactorPartitionV1::Heldout);
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("received");
    let mut builder = cohort_cache::Builder::new_received(&output, 100, 0, 0, false, 70).unwrap();
    for row in &rows {
        builder.push(*row).unwrap();
    }
    let manifest = builder.seal("fixed-source", 0, 80).unwrap();
    assert_eq!(manifest.files[0].bytes.get(), 5 * 48);
    assert_eq!(manifest.source_event_start_ns.unwrap().get(), 1);
    assert_eq!(manifest.source_event_end_ns.unwrap().get(), 26);
    builder.publish(manifest, json!({"status":"IMPORTED","availability":"RECEIVED_TIME_OBSERVED", "source_revision":"fixed-source", "event_start_ns":"0","event_end_ns":"80","missing_symbols":{},"rejected_rows":{},"missing_partitions":[],"duplicate_trades":0})).unwrap();
    request.input.representation = Some(NativeTickRepresentationV1::ReceivedTimeCohortCacheV1);
    let result = evaluate(&output.join("catalog"), &request, &wasm, true).unwrap();
    assert_eq!(
        serde_json::to_value(&result.partitions).unwrap(),
        serde_json::to_value(&original.partitions).unwrap()
    );
    assert_eq!(result.methods["features"], RECEIVED_FEATURE_SCHEMA);
    let prepared = temp.path().join("prepared");
    cohort_cache::prepare_views(&output.join("catalog"), &request, &prepared, 0).unwrap();
    let primary = cohort_cache::manifest(&prepared.join("research/catalog")).unwrap();
    assert_eq!(primary.source_event_start_ns.unwrap().get(), 1);
    assert_eq!(primary.source_event_end_ns.unwrap().get(), 9);
    let mut invalid = request.clone();
    invalid.input.maximum_receive_delay_ns = Some(counter(69).unwrap());
    assert_eq!(
        evaluate(&output.join("catalog"), &invalid, &wasm, true)
            .unwrap_err()
            .to_string(),
        "TICK_CACHE_VIEW_MISMATCH"
    );
    let mut venue_ahead = rows[0];
    venue_ahead.ts_event = 11_u64.into();
    let mut ahead_builder =
        cohort_cache::Builder::new_received(&temp.path().join("ahead"), 100, 0, 0, false, 70)
            .unwrap();
    ahead_builder.push(venue_ahead).unwrap();
    let ahead = ahead_builder.seal("fixed-source", 0, 80).unwrap();
    assert_eq!(ahead.files[0].first_event_ns.unwrap().get(), 10);
    assert_eq!(ahead.source_event_start_ns.unwrap().get(), 11);
    assert_eq!(ahead.source_event_end_ns.unwrap().get(), 12);
    let mut too_old = rows[0];
    too_old.ts_init = 79_u64.into();
    let mut invalid_builder =
        cohort_cache::Builder::new_received(&temp.path().join("too-old"), 100, 0, 0, false, 70)
            .unwrap();
    assert_eq!(
        invalid_builder.push(too_old).unwrap_err().to_string(),
        "TICK_RECEIVE_DELAY_OUTSIDE_BOUND"
    );
}

#[test]
fn received_feature_clock_is_derived_without_changing_raw_venue_bounds() {
    let mut request = request();
    request.input.availability = NativeTickAvailabilityV1::ReceivedTimeObserved;
    request.input.maximum_receive_delay_ns = Some(counter(70).unwrap());
    request.sampling.kind = NativeTickSamplingKindV1::ReceivedTimeCohortV1;
    let rows = (1..=7)
        .map(|i| {
            let mut row = trade(
                i * 10 + if i == 7 { 13 } else { 3 },
                &i.to_string(),
                &(10 + i).to_string(),
                "1",
            );
            row.ts_init = (i * 10).into();
            row
        })
        .collect::<Vec<_>>();
    let wasm = wat::parse_str(
        r#"(module
        (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)
            local.get 3 local.get 4 i64.ne if unreachable end)
        (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
            local.get 0 local.get 1 i64.ne if unreachable end f64.const 1))"#,
    )
    .unwrap();
    let original = evaluate_series(
        &request,
        BTreeMap::from([("AUD/USD.SIM".into(), rows.clone())]),
        &wasm,
        false,
    )
    .unwrap();
    assert_eq!(original.coverage.source_event_end_ns.unwrap().get(), 84);
    assert!(original
        .points
        .as_ref()
        .unwrap()
        .iter()
        .all(|point| point.observation.event_ns == point.observation.available_ns));
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("received");
    let mut builder = cohort_cache::Builder::new_received(&output, 100, 0, 0, false, 70).unwrap();
    for row in rows {
        builder.push(row).unwrap();
    }
    let manifest = builder.seal("fixed-source", 0, 80).unwrap();
    builder.publish(manifest, json!({"status":"IMPORTED","availability":"RECEIVED_TIME_OBSERVED","source_revision":"fixed-source","event_start_ns":"0","event_end_ns":"80","missing_symbols":{},"rejected_rows":{},"missing_partitions":[],"duplicate_trades":0})).unwrap();
    request.input.representation = Some(NativeTickRepresentationV1::ReceivedTimeCohortCacheV1);
    let cached = evaluate(&output.join("catalog"), &request, &wasm, true).unwrap();
    assert_eq!(cached.coverage.source_event_end_ns.unwrap().get(), 84);
    assert_eq!(
        serde_json::to_value(&cached.partitions).unwrap(),
        serde_json::to_value(&original.partitions).unwrap()
    );
}

#[test]
fn flow_reduction_preserves_marks_and_distinguishes_balanced_zero_from_unknown_side() {
    let mut rows = vec![
        trade(10, "a", "10", "0.1"),
        trade(10, "b", "20", "0.2"),
        trade(10, "c", "30", "0.3"),
    ];
    rows[2].aggressor_side = AggressorSide::Sell;
    let original = cohorts(&rows, 100).unwrap();
    let balanced = cohorts_with_flow(&rows, 100, true).unwrap();
    assert_eq!(balanced[0].signed_quantity, Some(0.0));
    assert_eq!(balanced[0].vwap.to_bits(), original[0].vwap.to_bits());
    assert_eq!(
        balanced[0].quantity.to_bits(),
        original[0].quantity.to_bits()
    );
    rows.reverse();
    assert_eq!(
        cohorts_with_flow(&rows, 100, true).unwrap()[0].signed_quantity,
        Some(0.0)
    );
    rows.pop();
    assert_eq!(
        cohorts_with_flow(&rows, 100, true).unwrap()[0].signed_quantity,
        Some(-0.1)
    );
    rows[0].aggressor_side = AggressorSide::Buy;
    assert_eq!(
        cohorts_with_flow(&rows, 100, true).unwrap()[0].signed_quantity,
        Some(0.5)
    );
    rows[0].aggressor_side = AggressorSide::NoAggressor;
    assert!(cohorts(&rows, 100).unwrap()[0].signed_quantity.is_none());
    assert_eq!(
        cohorts_with_flow(&rows, 100, true).unwrap_err().to_string(),
        "TICK_FLOW_AGGRESSOR_UNKNOWN"
    );
}

#[test]
fn flow_cache_delivers_explicit_eighth_feature_for_positive_negative_and_balanced_cohorts() {
    let temp = tempfile::tempdir().unwrap();
    let output = temp.path().join("flow");
    let mut rows = series().remove("AUD/USD.SIM").unwrap();
    rows[1].aggressor_side = AggressorSide::Sell;
    rows[4].aggressor_side = AggressorSide::Sell;
    for (event, id, price) in [(40, "balanced-40", "14"), (70, "balanced-70", "17")] {
        let mut sell = trade(event, id, price, "1");
        sell.aggressor_side = AggressorSide::Sell;
        rows.push(sell);
    }
    rows.sort_by_key(|row| row.ts_event);
    let mut builder = cohort_cache::Builder::new_flow(&output, 100, 0, 0).unwrap();
    for row in &rows {
        builder.push(*row).unwrap();
    }
    let manifest = builder.seal("fixed-source", 0, 80).unwrap();
    assert_eq!(manifest.files[0].bytes.get(), 7 * 40);
    builder
        .publish(
            manifest,
            json!({"status":"IMPORTED","availability":"EVENT_TIME_PROXY_UNVERIFIED",
        "source_revision":"fixed-source","event_start_ns":"0","event_end_ns":"80",
        "missing_symbols":{},"rejected_rows":{},"missing_partitions":[],"duplicate_trades":0}),
        )
        .unwrap();
    let mut request = request();
    request.input.representation = Some(NativeTickRepresentationV1::EventTimeCohortFlowCacheV1);
    request.input.maximum_rows = None;
    request.input.maximum_source_rows = Some(counter(9).unwrap());
    let prepared = temp.path().join("prepared");
    cohort_cache::prepare_views(&output.join("catalog"), &request, &prepared, 1_000_000).unwrap();
    let root = prepared.join("independent/catalog");
    let wasm = wat::parse_str(
        r#"(module
        (global $flow (mut f64) (f64.const 99))
        (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64)
            local.get 0 i32.const 7 i32.eq if
                local.get 2 i32.const 0 i32.ne if unreachable end
                local.get 1 global.set $flow end)
        (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
            local.get 4 i32.const 8 i32.ne if unreachable end
            local.get 3 i32.const 3 i32.rem_u i32.const 1 i32.eq
            if (result f64) f64.const -1 else
                local.get 3 i32.const 3 i32.rem_u i32.const 2 i32.eq
                if (result f64) f64.const 1 else f64.const 0 end end
            global.get $flow f64.ne if unreachable end
            global.get $flow))"#,
    )
    .unwrap();
    for target in [
        NativeTickTargetV1::FixedTradeCohorts { horizon_cohorts: 1 },
        NativeTickTargetV1::FixedEventTime {
            horizon_ns: counter(10).unwrap(),
        },
    ] {
        request.target = target;
        request.input.maximum_source_rows = Some(counter(1).unwrap());
        let limited_legacy = evaluate(&root, &request, &wasm, true).unwrap();
        request.input.maximum_source_rows = None;
        let result = evaluate(&root, &request, &wasm, true).unwrap();
        assert_eq!(
            limited_legacy.coverage.trade_count,
            result.coverage.trade_count
        );
        assert_eq!(result.methods["features"], FLOW_FEATURE_SCHEMA);
        assert_eq!(
            result.coverage.source_evidence["representation"],
            "EVENT_TIME_COHORT_FLOW_CACHE_V1"
        );
        assert!(result.methods["input"].contains("EVENT_TIME_COHORT_FLOW_CACHE_V1"));
        assert_eq!(result.coverage.trade_count.get(), 9);
        assert_eq!(result.coverage.cohort_count.get(), 7);
        assert!(result.points.is_none());
    }
    let mut wrong = request.clone();
    wrong.input.representation = Some(NativeTickRepresentationV1::EventTimeCohortCacheV1);
    assert!(evaluate(&root, &wrong, &model(0), true).is_err());
}

#[test]
fn exact_cohort_reduction_is_tie_order_invariant_and_has_no_last_trade_mark() {
    let rows = vec![trade(10, "a", "10", "1"), trade(10, "b", "20", "3")];
    let mut reversed = rows.clone();
    reversed.reverse();
    let a = cohorts(&rows, 100).unwrap();
    let b = cohorts(&reversed, 100).unwrap();
    assert_eq!(a[0].vwap, 17.5);
    assert_eq!(a[0].quantity, 4.0);
    assert_eq!(a[0].count, 2);
    assert_eq!(a[0].vwap, b[0].vwap);
    assert_eq!(rows[0].price, Price::from("10"));
}

#[test]
fn future_cohort_changes_labels_without_changing_prior_features_or_forecasts() {
    let req = request();
    let wasm = model(0);
    let original = evaluate_series(&req, series(), &wasm, false).unwrap();
    let mut changed = series();
    changed.get_mut("AUD/USD.SIM").unwrap()[3].price = Price::from("10000");
    let changed = evaluate_series(&req, changed, &wasm, false).unwrap();
    let original = original.points.unwrap();
    let changed = changed.points.unwrap();
    for i in 0..3 {
        assert_eq!(
            original[i].observation.forecast,
            changed[i].observation.forecast
        );
    }
    assert_ne!(
        original[2].observation.label_return,
        changed[2].observation.label_return
    );
    assert_eq!(original[2].observation.forecast, Some(13.0));
}

#[test]
fn same_instrument_horizon_is_censored_at_each_protected_partition() {
    let result = evaluate_series(&request(), series(), &model(0), false).unwrap();
    let p = &result.partitions;
    assert_eq!(p[0].metric.counts.indicator_warmup.get(), 1);
    assert_eq!(p[0].metric.counts.cross_boundary_labels.get(), 1);
    assert_eq!(p[1].metric.counts.included.get(), 1);
    assert_eq!(p[1].metric.counts.cross_boundary_labels.get(), 1);
    assert_eq!(p[2].metric.counts.included.get(), 2);
    assert_eq!(p[2].metric.counts.incomplete_labels.get(), 1);
    assert_eq!(result.coverage.trade_count.get(), 7);
    assert!(result
        .coverage
        .limitations
        .iter()
        .any(|x| x == "NOT_PIT_QUALIFIED"));
    let summary = evaluate_series(&request(), series(), &model(0), true).unwrap();
    assert!(!serde_json::to_value(summary)
        .unwrap()
        .as_object()
        .unwrap()
        .contains_key("points"));
}

#[test]
fn a_cohort_is_one_prediction_and_oversized_cohorts_fail_without_truncation() {
    let mut data = series();
    data.get_mut("AUD/USD.SIM")
        .unwrap()
        .insert(0, trade(10, "other", "19", "1"));
    let result = evaluate_series(&request(), data.clone(), &model(5), false).unwrap();
    assert_eq!(result.coverage.trade_count.get(), 8);
    assert_eq!(result.coverage.cohort_count.get(), 7);
    let mut req = request();
    req.sampling.maximum_cohort_rows = 1;
    assert_eq!(
        evaluate_series(&req, data, &model(0), false)
            .unwrap_err()
            .to_string(),
        "TICK_COHORT_ROW_LIMIT"
    );
}

#[test]
fn tick_model_requires_explicit_feature_abi_and_preserves_legacy_row_fields() {
    let bar=wat::parse_str("(module (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))").unwrap();
    assert_eq!(
        evaluate_series(&request(), series(), &bar, false)
            .unwrap_err()
            .to_string(),
        "FEATURE_MODEL_ABI_MISMATCH"
    );
    let mut req = request();
    req.input.maximum_rows = Some(1_000_001);
    assert!(evaluation_request(&req).is_ok());
    req.input.maximum_rows = None;
    assert!(evaluation_request(&req).is_ok());
    req = request();
    req.validation_end_ns = req.train_end_ns;
    assert!(evaluation_request(&req).is_err());
}

#[test]
fn coverage_distinguishes_empty_requested_instrument_from_observed_trade_ranges() {
    let mut request = request();
    request.input.instrument_ids.push("EUR/USD.SIM".into());
    let mut data = series();
    data.insert("EUR/USD.SIM".into(), Vec::new());
    let result = evaluate_series(&request, data, &model(0), false).unwrap();
    let observed = &result.coverage.instrument_coverage[0];
    assert_eq!(observed.trade_count.get(), 7);
    assert_eq!(observed.first_event_ns.unwrap().get(), 10);
    assert_eq!(observed.last_event_ns.unwrap().get(), 70);
    let empty = &result.coverage.instrument_coverage[1];
    assert_eq!(empty.trade_count.get(), 0);
    assert!(empty.first_event_ns.is_none());
    assert!(empty.last_event_ns.is_none());
}

#[test]
fn explicit_tick_universe_does_not_inherit_the_bar_256_instrument_limit() {
    let mut request = request();
    request.input.instrument_ids = (0..257).map(|i| format!("ASSET{i}.SIM")).collect();
    assert!(evaluation_request(&request).is_ok());
    request
        .input
        .instrument_ids
        .push(request.input.instrument_ids[0].clone());
    assert!(evaluation_request(&request).is_err());
}

#[test]
fn derived_cache_prepares_bounded_views_and_preserves_native_factor_results() {
    let temp = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
    let output = temp.path().join("source");
    let mut data = series();
    data.get_mut("AUD/USD.SIM")
        .unwrap()
        .insert(2, trade(20, "extra", "19", "3"));
    let mut builder = cohort_cache::Builder::new(&output, 100, 100, 1_000_000).unwrap();
    // One complete event deliberately spans producer batches.
    for batch in data["AUD/USD.SIM"].chunks(2) {
        for trade in batch {
            builder.push(*trade).unwrap();
        }
    }
    let manifest = builder.seal("fixed-source", 0, 80).unwrap();
    assert_eq!(manifest.trade_count.get(), 8);
    assert_eq!(manifest.cohort_count.get(), 7);
    assert_eq!(manifest.files[0].bytes.get(), 7 * cohort_cache::FRAME_BYTES);
    builder
        .publish(
            manifest,
            json!({"status":"IMPORTED","availability":"EVENT_TIME_PROXY_UNVERIFIED",
                "source_revision":"fixed-source","event_start_ns":"0","event_end_ns":"80",
                "missing_symbols":{},"rejected_rows":{},"missing_partitions":[],"duplicate_trades":0}),
        )
        .unwrap();
    let mut native = request();
    native.parameters.total_fuel = Some(counter(1_000_000).unwrap());
    let mut cached = native.clone();
    cached.input.representation = Some(NativeTickRepresentationV1::EventTimeCohortCacheV1);
    cached.input.maximum_rows = None;
    cached.input.maximum_source_rows = Some(counter(8).unwrap());
    let prepared = temp.path().join("prepared");
    cohort_cache::prepare_views(&output.join("catalog"), &cached, &prepared, 1_000_000).unwrap();
    let relative = tempfile::Builder::new()
        .prefix(".qz-relative-prepared-")
        .tempdir_in(std::env::current_dir().unwrap())
        .unwrap();
    std::fs::remove_dir(relative.path()).unwrap();
    cohort_cache::prepare_views(
        &output.join("catalog"),
        &cached,
        Path::new(relative.path().file_name().unwrap()),
        1_000_000,
    )
    .unwrap();
    assert!(relative.path().join("catalog-metadata.json").is_file());
    let independent = prepared.join("independent/catalog");
    let primary = prepared.join("research/catalog");
    let primary_manifest = cohort_cache::manifest(&primary).unwrap();
    assert_eq!(primary_manifest.cohort_count.get(), 4);
    assert_eq!(primary_manifest.trade_count.get(), 5);
    assert!(primary_manifest.files[0].byte_end < primary_manifest.files[0].bytes);
    assert_eq!(
        evaluate(&primary, &cached, &model(0), true)
            .unwrap_err()
            .to_string(),
        "TICK_CACHE_VIEW_MISMATCH"
    );
    assert_eq!(
        evaluate(&independent, &cached, &model(0), false)
            .unwrap_err()
            .to_string(),
        "TICK_CACHE_SUMMARY_ONLY_REQUIRED"
    );
    let mut incomplete = primary_manifest.clone();
    incomplete.files[0].byte_end = counter(3 * cohort_cache::FRAME_BYTES).unwrap();
    incomplete.files[0].cohort_count = counter(3).unwrap();
    incomplete.files[0].trade_count = counter(4).unwrap();
    incomplete.files[0].last_event_ns = Some(counter(30).unwrap());
    incomplete.cohort_count = counter(3).unwrap();
    incomplete.trade_count = counter(4).unwrap();
    let primary_path = primary.join(cohort_cache::MANIFEST_FILE);
    std::fs::write(&primary_path, serde_json::to_vec(&incomplete).unwrap()).unwrap();
    let mut prefix_request = cached.clone();
    prefix_request.input.event_end_ns = prefix_request.validation_end_ns;
    assert_eq!(
        cohort_cache::load(&primary, &prefix_request)
            .err()
            .unwrap()
            .to_string(),
        "TICK_CACHE_VIEW_OMITS_IN_RANGE_COHORTS"
    );
    std::fs::write(primary_path, serde_json::to_vec(&primary_manifest).unwrap()).unwrap();
    for prefix in [false, true] {
        let mut native = native.clone();
        let mut cached = cached.clone();
        let mut selected = data.clone();
        let root = if prefix {
            native.input.event_end_ns = native.validation_end_ns;
            cached.input.event_end_ns = cached.validation_end_ns;
            for rows in selected.values_mut() {
                rows.retain(|trade| trade.ts_event.as_u64() < native.validation_end_ns.get());
            }
            &primary
        } else {
            &independent
        };
        for target in [
            NativeTickTargetV1::FixedTradeCohorts { horizon_cohorts: 1 },
            NativeTickTargetV1::FixedEventTime {
                horizon_ns: counter(10).unwrap(),
            },
            NativeTickTargetV1::FixedEventTime {
                horizon_ns: counter(25).unwrap(),
            },
        ] {
            native.target = target;
            cached.target = target;
            for slot in 0..7 {
                let wasm = model(slot);
                let expected = evaluate_series(&native, selected.clone(), &wasm, true).unwrap();
                let actual = evaluate(root, &cached, &wasm, true).unwrap();
                assert!(actual.points.is_none());
                assert_eq!(actual.consumed_fuel, expected.consumed_fuel);
                assert_eq!(actual.coverage.trade_count, expected.coverage.trade_count);
                assert_eq!(actual.coverage.cohort_count, expected.coverage.cohort_count);
                for (actual, expected) in actual.partitions.iter().zip(&expected.partitions) {
                    assert_eq!(
                        serde_json::to_value(&actual.metric.counts).unwrap(),
                        serde_json::to_value(&expected.metric.counts).unwrap()
                    );
                    assert_eq!(
                        actual.metric.pearson_ic.status,
                        expected.metric.pearson_ic.status
                    );
                    if let (Some(actual), Some(expected)) = (
                        actual.metric.pearson_ic.value,
                        expected.metric.pearson_ic.value,
                    ) {
                        assert!((actual - expected).abs() <= 1e-12);
                    }
                }
            }
        }
    }
}

#[test]
fn duration_labels_use_first_complete_event_and_never_change_prior_predictions() {
    let mut req = request();
    req.target = NativeTickTargetV1::FixedEventTime {
        horizon_ns: counter(10).unwrap(),
    };
    let mut data = series();
    for (trade, time) in data
        .get_mut("AUD/USD.SIM")
        .unwrap()
        .iter_mut()
        .zip([10_u64, 20, 21, 29, 40, 50, 70])
    {
        trade.ts_event = time.into();
        trade.ts_init = time.into();
    }
    let points = evaluate_series(&req, data.clone(), &model(0), false)
        .unwrap()
        .points
        .unwrap();
    let actual: Vec<_> = points
        .iter()
        .map(|p| p.label_event_ns.map(|v| v.get()))
        .collect();
    assert_eq!(
        actual,
        vec![None, Some(40), Some(40), Some(40), Some(50), Some(70), None]
    );
    assert_eq!(
        points[1].exclusion_reason,
        Some(NativeFactorExclusionReasonV1::LabelCrossesPartition)
    );
    assert_eq!(
        points[4].exclusion_reason,
        Some(NativeFactorExclusionReasonV1::LabelCrossesPartition)
    );
    assert!(points[5].exclusion_reason.is_none());
    assert_eq!(
        points[6].exclusion_reason,
        Some(NativeFactorExclusionReasonV1::LabelNotComplete)
    );
    data.get_mut("AUD/USD.SIM").unwrap()[4].price = Price::from("10000");
    let changed = evaluate_series(&req, data, &model(0), false)
        .unwrap()
        .points
        .unwrap();
    for i in 0..4 {
        assert_eq!(
            points[i].observation.forecast,
            changed[i].observation.forecast
        );
    }
    assert_ne!(
        points[1].observation.label_return,
        changed[1].observation.label_return
    );
}
