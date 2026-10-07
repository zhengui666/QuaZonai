//! Controlled native records only. These are not public-data or PIT acceptance.
use super::*;
use nautilus_model::{
    enums::{AggressorSide, AssetClass},
    identifiers::{InstrumentId, Symbol, TradeId},
    instruments::BinaryOption,
    types::{Currency, Price, Quantity},
};

const ID: &str = "fixture-event-101.POLYMARKET";
const STEP: u64 = 1_000_000_000;
fn instrument() -> InstrumentAny {
    let original = InstrumentAny::BinaryOption(
        BinaryOption::builder()
            .instrument_id(InstrumentId::from(ID))
            .raw_symbol(Symbol::new("101"))
            .asset_class(AssetClass::Alternative)
            .currency(Currency::from("pUSD"))
            .activation_ns(0u64.into())
            .expiration_ns((20 * STEP).into())
            .price_precision(2)
            .size_precision(2)
            .price_increment(Price::from("0.01"))
            .size_increment(Quantity::from("0.01"))
            .ts_event(0u64.into())
            .ts_init(0u64.into())
            .build()
            .unwrap(),
    );
    let mut value = serde_json::to_value(original).unwrap();
    value["BinaryOption"]["info"] = json!({"condition_id":"fixture-event","token_id":"101","fee_schedule":{"rate":0,"exponent":1,"rebateRate":0.2,"takerOnly":true},"source_reference":"SYNTHETIC_NATIVE_REGRESSION"});
    serde_json::from_value(value).unwrap()
}
fn plan() -> ForwardSourcePlan {
    ForwardSourcePlan {
        schema_version: SchemaV1,
        instrument_id: ID.into(),
        bar_type: format!("{ID}-1-SECOND-LAST-EXTERNAL"),
        first_close_ns: count(3 * STEP).unwrap(),
        required_bars: 2,
    }
}
fn schema() -> Vec<FeatureDefinitionV1> {
    vec![FeatureDefinitionV1 {
        feature_key: "original_bid".into(),
        source_ref: format!("nautilus-polymarket/0.63.0/quote/{ID}"),
        source_key: "bid_price".into(),
        availability: FeatureAvailabilityV1::Observed,
        max_age_ns: Some(count(STEP).unwrap()),
    }]
}
fn records() -> Vec<SourceRecord> {
    let p = plan();
    let mut out = Vec::new();
    let mut push = |kind: &str, at: u64, payload: Value| {
        out.push(SourceRecord {
            schema_version: SchemaV1,
            sequence: count(out.len() as u64 + 1).unwrap(),
            observed_at_ns: count(at).unwrap(),
            kind: kind.into(),
            payload,
        })
    };
    push(
        "start",
        STEP / 2,
        json!({"purpose":"CURRENT_POLYMARKET_FORWARD_SOURCE","execution_clients_registered":0,"forward_plan":p,"native_version":"0.63.0","selected_instruments":[ID],"native_session_id":UUID4::new().to_string()}),
    );
    push(
        "instrument",
        STEP / 2 + 1,
        serde_json::to_value(instrument()).unwrap(),
    );
    push(
        "ready",
        STEP / 2 + 2,
        json!({"data_clients_connected":true,"definitions_complete":true,"lifecycle_ready":true}),
    );
    // Actual native initial partial close, retained before the planned window.
    // Its receive observation proves the BAR subscription was already active.
    let startup_trade = TradeTick::new(
        InstrumentId::from(ID),
        Price::from("0.55"),
        Quantity::from("2.00"),
        AggressorSide::Buy,
        TradeId::new("startup-partial"),
        (STEP / 2 + 30).into(),
        (STEP / 2 + 40).into(),
    );
    push(
        "trade",
        STEP / 2 + 50,
        serde_json::to_value(startup_trade).unwrap(),
    );
    let startup_bar = Bar::new(
        format!("{ID}-1-SECOND-LAST-INTERNAL").parse().unwrap(),
        Price::from("0.55"),
        Price::from("0.55"),
        Price::from("0.55"),
        Price::from("0.55"),
        Quantity::from("2.00"),
        STEP.into(),
        STEP.into(),
    );
    push(
        "bar_close",
        STEP + 100,
        serde_json::to_value(startup_bar).unwrap(),
    );
    for index in 0..2 {
        let event = (2 + index) * STEP + 100;
        let q = QuoteTick::new(
            InstrumentId::from(ID),
            Price::from("0.40"),
            Price::from("0.60"),
            Quantity::from("3.00"),
            Quantity::from("4.00"),
            event.into(),
            (event + 10).into(),
        );
        push("quote", event + 20, serde_json::to_value(q).unwrap());
        let t = TradeTick::new(
            InstrumentId::from(ID),
            Price::from("0.55"),
            Quantity::from("2.00"),
            AggressorSide::Buy,
            TradeId::new(format!("fixture-{index}")),
            (event + 30).into(),
            (event + 40).into(),
        );
        push("trade", event + 50, serde_json::to_value(t).unwrap());
        let close = (3 + index) * STEP;
        let bar = Bar::new(
            format!("{ID}-1-SECOND-LAST-INTERNAL").parse().unwrap(),
            Price::from("0.55"),
            Price::from("0.55"),
            Price::from("0.55"),
            Price::from("0.55"),
            Quantity::from("2.00"),
            close.into(),
            close.into(),
        );
        push("bar_close", close + 100, serde_json::to_value(bar).unwrap());
    }
    push(
        "end",
        4 * STEP + 200,
        json!({"complete":true,"gap":false,"dropped_events":0,"encoding_failed":false,"native_shutdown_confirmed":true,"lifecycle_complete":true,"forward_observations_complete":true,"native_returned_success":true,"native_failure":null,"all_selected_ticks_observed":true,"ready":true,"forward_bars_observed":2}),
    );
    for phase in [
        "native_retention_attached",
        "lifecycle_start",
        "lifecycle_control_ready",
        "lifecycle_connect_begin",
        "lifecycle_connected",
    ] {
        push("source_phase", STEP / 2, json!({"phase":phase}));
    }
    push(
        "lifecycle_socket",
        STEP / 2 + 1,
        json!({"state":"Connected","observed_at_ns":count(STEP / 2).unwrap()}),
    );
    push(
        "lifecycle_initial_book",
        STEP / 2 + 1,
        json!({"market":"fixture-event","asset_id":"101","timestamp":"500","hash":"SYNTHETIC_FIXTURE","bids":[{"price":"0.40","size":"3"}],"asks":[{"price":"0.60","size":"4"}]}),
    );
    push(
        "lifecycle_ready",
        STEP / 2 + 1,
        json!({"tokens":["101"],"initial_books_complete":true}),
    );
    push(
        "forward_window_closed",
        4 * STEP + 150,
        json!({"bar_window_end_ns":count(4*STEP).unwrap()}),
    );
    push(
        "lifecycle_coverage",
        4 * STEP + 170,
        json!({"primary_native_drain_finished":true,"parsed_message_channel_drained":true,"scope":"OBSERVED_NATIVE_AND_PARSED_PUBLIC_EVENTS_NOT_ALL_EXCHANGE_PACKETS"}),
    );
    out.sort_by_key(|r| r.observed_at_ns);
    resequence(&mut out);
    out
}
fn resequence(records: &mut [SourceRecord]) {
    for (i, r) in records.iter_mut().enumerate() {
        r.sequence = count(i as u64 + 1).unwrap();
    }
}
fn record_mut<'a>(records: &'a mut [SourceRecord], kind: &str, nth: usize) -> &'a mut SourceRecord {
    records
        .iter_mut()
        .filter(|r| r.kind == kind)
        .nth(nth)
        .unwrap()
}
fn bytes(records: &[SourceRecord]) -> Vec<u8> {
    let mut all = Vec::new();
    for r in records {
        serde_json::to_writer(&mut all, r).unwrap();
        all.push(b'\n');
    }
    all
}
fn run(records: &[SourceRecord]) -> Result<Projection> {
    project(
        &mut bytes(records).as_slice(),
        &plan(),
        &schema(),
        &instrument(),
        &BTreeMap::from([(0, 100)]),
        2 * STEP + 1,
    )
}
#[test]
fn original_trade_last_and_actual_close_observation_are_distinct() {
    let source = records();
    let p = run(&source).unwrap();
    assert_eq!(p.bars.len(), 2);
    assert_eq!(p.bars[0].close, Price::from("0.55"));
    assert_ne!(p.bars[0].close, Price::from("0.50"));
    assert_eq!(p.bars[0].ts_event.as_u64(), 3 * STEP);
    assert_eq!(p.bars[0].ts_init.as_u64(), 3 * STEP + 100);
    assert_eq!(p.features.feature_schema, schema());
    assert_eq!(
        p.features.observations[0]
            .observed_available_ns
            .unwrap()
            .get(),
        2 * STEP + 120
    );
    assert_eq!(
        p.features.observations[0].sequence.get(),
        100 + source
            .iter()
            .find(|r| r.kind == "quote")
            .unwrap()
            .sequence
            .get()
    );
    assert_eq!(p.features.observations[0].value, Some(0.4));
    assert_eq!(
        source
            .iter()
            .filter(|r| r.kind == "bar_close")
            .nth(1)
            .unwrap()
            .payload["ts_init"],
        json!(3 * STEP),
        "original native scheduled boundary retained"
    );
}
#[test]
fn missing_end_or_close_never_publishes_complete_source() {
    let mut r = records();
    r.pop();
    assert!(run(&r).is_err());
    let mut r = records();
    record_mut(&mut r, "bar_close", 1).kind = "source_phase".into();
    assert!(run(&r).is_err());
}
#[test]
fn sequence_gap_or_regressing_observation_stops() {
    let mut r = records();
    record_mut(&mut r, "trade", 1).sequence = count(99).unwrap();
    assert!(run(&r).is_err());
    let mut r = records();
    record_mut(&mut r, "trade", 1).observed_at_ns = count(1).unwrap();
    assert!(run(&r).is_err());
}
#[test]
fn source_plan_and_market_terms_cannot_change() {
    let mut r = records();
    record_mut(&mut r, "start", 0).payload["forward_plan"]["required_bars"] = json!(1);
    assert!(run(&r).is_err());
    let mut r = records();
    record_mut(&mut r, "instrument", 0).payload["BinaryOption"]["price_increment"] = json!("0.02");
    assert!(run(&r).is_err());
}
#[test]
fn gap_disconnect_overflow_and_cancelled_end_fail() {
    for (original_kind, kind, payload) in [
        ("trade", "gap", json!({})),
        ("trade", "socket", json!({"state":"Disconnected"})),
        ("trade", "queue", json!({"state":"Overflow"})),
        ("end", "end", json!({"complete":false})),
    ] {
        let mut r = records();
        let record = record_mut(&mut r, original_kind, 0);
        record.kind = kind.into();
        record.payload = payload;
        assert!(run(&r).is_err());
    }
}
#[test]
fn mid_cannot_be_relabelled_last() {
    let mut p = plan();
    p.bar_type = p.bar_type.replace("LAST", "MID");
    assert!(p.native().is_err());
    let mut r = records();
    record_mut(&mut r, "bar_close", 1).payload["bar_type"] =
        json!(format!("{ID}-1-SECOND-MID-INTERNAL"));
    assert!(run(&r).is_err());
}
#[test]
fn no_trade_means_no_bar_not_carry_forward() {
    let mut r = records();
    record_mut(&mut r, "trade", 1).kind = "source_phase".into();
    assert!(run(&r).unwrap_err().to_string().contains("EMPTY_BAR"));
}
#[test]
fn late_venue_events_follow_native_receive_order_and_keep_original_clocks() {
    let mut r = records();
    record_mut(&mut r, "trade", 2).payload["ts_event"] = json!(STEP + 150);
    record_mut(&mut r, "quote", 1).payload["ts_event"] = json!(STEP + 100);
    let p = run(&r).unwrap();
    assert_eq!(p.bars.len(), 2);
    assert_eq!(p.features.observations[1].event_ns.get(), STEP + 100);
    assert_eq!(
        p.features.observations[1]
            .observed_available_ns
            .unwrap()
            .get(),
        3 * STEP + 120
    );
    assert_eq!(p.source_start, STEP + 100);
    assert_eq!(p.bars[1].volume, Quantity::from("2.00"));
}
#[test]
fn ambiguous_trade_id_cannot_revise_previous_bar() {
    let mut r = records();
    let original_id = record_mut(&mut r, "trade", 1).payload["trade_id"].clone();
    record_mut(&mut r, "trade", 2).payload["trade_id"] = original_id;
    assert!(run(&r).unwrap_err().to_string().contains("AMBIGUOUS"));
}
#[test]
fn missing_real_close_observation_cannot_use_native_timer_time() {
    let mut r = records();
    record_mut(&mut r, "bar_close", 1).observed_at_ns = count(3 * STEP - 1).unwrap();
    assert!(run(&r).is_err());
}
#[test]
fn unknown_feature_meaning_is_unsupported() {
    let mut s = schema();
    s[0].source_ref = "unrelated-macro-source".into();
    assert!(
        quote_fields(&s, ID)
            .unwrap_err()
            .to_string()
            .contains("UNSUPPORTED")
    );
    s = schema();
    s[0].source_key = "future_label".into();
    assert!(quote_fields(&s, ID).is_err());
}
#[test]
fn partial_start_and_market_close_are_unavailable() {
    let mut r = records();
    record_mut(&mut r, "ready", 0).observed_at_ns = count(2 * STEP + 1).unwrap();
    assert!(run(&r).is_err());
    let mut r = records();
    record_mut(&mut r, "trade", 1).kind = "close".into();
    assert!(run(&r).is_err());
}
#[test]
fn partial_line_and_trailing_records_are_rejected() {
    let mut b = bytes(&records());
    b.pop();
    assert!(
        project(
            &mut b.as_slice(),
            &plan(),
            &schema(),
            &instrument(),
            &BTreeMap::new(),
            0
        )
        .is_err()
    );
    let mut r = records();
    r.push(SourceRecord {
        schema_version: SchemaV1,
        sequence: count(11).unwrap(),
        observed_at_ns: count(5 * STEP).unwrap(),
        kind: "source_phase".into(),
        payload: json!({}),
    });
    assert!(run(&r).is_err());
}

#[test]
fn source_bar_prices_must_match_official_replay() {
    let mut r = records();
    for field in ["open", "high", "low", "close"] {
        record_mut(&mut r, "bar_close", 1).payload[field] = json!("0.56");
    }
    assert!(
        run(&r)
            .unwrap_err()
            .to_string()
            .contains("NATIVE_BAR_REPLAY_OR_INITIAL_PARTIAL_UNSUPPORTED")
    );
}

#[test]
fn actual_lifecycle_ready_and_drain_records_are_required() {
    for kind in [
        "lifecycle_ready",
        "lifecycle_coverage",
        "forward_window_closed",
    ] {
        let mut records = records();
        record_mut(&mut records, kind, 0).kind = "source_phase".into();
        assert!(
            run(&records).is_err(),
            "missing {kind} cannot be inferred from end flags"
        );
    }
    let mut records = records();
    record_mut(&mut records, "lifecycle_coverage", 0).payload["parsed_message_channel_drained"] =
        json!(false);
    assert!(
        run(&records)
            .unwrap_err()
            .to_string()
            .contains("DRAIN_FACTS")
    );
}

#[test]
fn unselected_original_bars_remain_in_source_without_becoming_forward_rows() {
    let mut records = records();
    let mut prior = record_mut(&mut records, "bar_close", 1).payload.clone();
    prior["ts_event"] = json!(2 * STEP);
    prior["ts_init"] = json!(2 * STEP);
    let mut prior_trade = record_mut(&mut records, "trade", 1).payload.clone();
    prior_trade["ts_event"] = json!(STEP + 200);
    prior_trade["ts_init"] = json!(STEP + 210);
    prior_trade["trade_id"] = json!("prewindow");
    records.push(SourceRecord {
        schema_version: SchemaV1,
        sequence: count(1).unwrap(),
        observed_at_ns: count(STEP + 220).unwrap(),
        kind: "trade".into(),
        payload: prior_trade,
    });
    records.push(SourceRecord {
        schema_version: SchemaV1,
        sequence: count(1).unwrap(),
        observed_at_ns: count(2 * STEP + 10).unwrap(),
        kind: "bar_close".into(),
        payload: prior,
    });
    records.sort_by_key(|r| r.observed_at_ns);
    resequence(&mut records);
    let projected = run(&records).unwrap();
    assert_eq!(projected.bars.len(), 2);
    assert_eq!(projected.bars[0].ts_event.as_u64(), 3 * STEP);
    // An absent early close or an unrecorded native skip/reset cannot be guessed.
    record_mut(&mut records, "bar_close", 1).kind = "source_phase".into();
    assert!(
        run(&records)
            .unwrap_err()
            .to_string()
            .contains("INITIAL_PARTIAL_UNSUPPORTED")
    );
}

#[test]
fn source_ready_alone_cannot_certify_a_late_started_first_partial_bar() {
    let mut r = records();
    // Drop both startup callbacks as if the native bar subscription began only
    // inside the requested first interval. Source ready and end flags still pass.
    r.retain(|r| {
        !((r.kind == "trade" || r.kind == "bar_close") && r.observed_at_ns.get() < 2 * STEP)
    });
    resequence(&mut r);
    assert!(
        run(&r)
            .unwrap_err()
            .to_string()
            .contains("AGGREGATION_START_EVIDENCE_REQUIRED")
    );
}

#[test]
fn actual_native_catalog_preparation_preserves_warmup_and_source_bytes() {
    use contracts::science::NativeBarSelectionV1;
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    let native = root.join("warmup-native");
    fs::create_dir(&native).unwrap();
    let original = instrument();
    let (bar_type, _) = plan().native().unwrap();
    let warmup = (1..=2u64)
        .map(|i| {
            Bar::new(
                bar_type,
                Price::from("0.40"),
                Price::from("0.40"),
                Price::from("0.40"),
                Price::from("0.40"),
                Quantity::from("1.00"),
                (i * STEP).into(),
                (i * STEP + 1).into(),
            )
        })
        .collect::<Vec<_>>();
    let catalog =
        ParquetDataCatalog::from_uri(native.to_str().unwrap(), None, None, None, None).unwrap();
    catalog.write_instruments(vec![original]).unwrap();
    catalog.write_to_parquet(&warmup, None, None, None).unwrap();
    let selection = NativeDatasetSelectionV1 {
        dataset_revision_id: Id::new(),
        settlements: vec![],
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: vec![plan().bar_type],
            event_start_ns: count(STEP).unwrap(),
            event_end_ns: count(2 * STEP + 1000).unwrap(),
            decision_cutoff_ns: count(2 * STEP + 1000).unwrap(),
            maximum_rows: 2,
        },
    };
    let instant = |n: u64| chrono::DateTime::<chrono::Utc>::from_timestamp_nanos(n as i64);
    let mut declaration = json!({"schema_version":1,"registered_ref":"fixture-warmup","native_snapshot_ref":"fixture-warmup-v1","storage_version":"1","provider_kind":"NAUTILUS_CATALOG","data_kind":"BAR","partition":"VALIDATION","event_start":instant(STEP),"event_end":instant(2*STEP+1000),"available_through":instant(2*STEP+1000),"origin":"FIXTURE","pit_status":"UNVERIFIED","revision_policy":"AS_KNOWN_THEN","provenance_reference":"CONTROLLED_NATIVE_FIXTURE_ONLY","availability_provenance":"Synthetic regression clocks; no market or historical PIT claim","universe":{"name":"Controlled native fixture","calendar_ref":"fixture-calendar","calendar_version":"1","selection_asof":instant(0),"has_historical_membership":false,"coverage_start":instant(0),"coverage_end":instant(20*STEP),"membership":[{"instrument_id":ID,"valid_from":instant(0),"valid_until":null,"available_at":instant(0),"groups":null}]}});
    let declared = root.join("warmup-declaration.json");
    let selected = root.join("warmup-selection.json");
    write_new(&declared, &serde_json::to_vec(&declaration).unwrap()).unwrap();
    write_new(&selected, &serde_json::to_vec(&selection).unwrap()).unwrap();
    let prepared = root.join("warmup");
    let metadata = super::super::prepare(&super::super::Arguments {
        catalog: native,
        declaration: declared,
        selection: selected.clone(),
        output: prepared.clone(),
    })
    .unwrap();
    let old_feature = FeatureObservationsV1 {
        schema_version: SchemaV1,
        partition: DataPartition::Validation,
        feature_schema: schema(),
        observations: vec![FeatureObservationV1 {
            feature_index: 0,
            event_ns: count(STEP).unwrap(),
            observed_available_ns: Some(count(STEP + 1).unwrap()),
            sequence: count(100).unwrap(),
            value: Some(0.4),
            missing_reason: None,
        }],
    };
    let feature = root.join("original-features.json");
    let original_feature_bytes = serde_json::to_vec_pretty(&old_feature).unwrap();
    write_new(&feature, &original_feature_bytes).unwrap();
    declaration["universe"] = serde_json::to_value(&metadata.universe).unwrap();
    declaration["registered_ref"] = json!("fixture-forward");
    declaration["native_snapshot_ref"] = json!("fixture-forward-v2");
    declaration["storage_version"] = json!("2");
    let plan_path = root.join("plan.json");
    write_new(&plan_path,&serde_json::to_vec(&json!({"schema_version":1,"source":plan(),"dataset_revision_id":Id::new(),"feature_schema":schema(),"declaration":declaration})).unwrap()).unwrap();
    let recording = root.join("original.ndjson");
    let raw = bytes(&records());
    write_new(&recording, &raw).unwrap();
    let output = root.join("forward");
    let args = Arguments {
        recording: recording.clone(),
        plan: plan_path,
        warmup_catalog: prepared.join("catalog"),
        warmup_metadata: prepared.join("catalog-metadata.json"),
        warmup_selection: selected,
        original_features: vec![feature],
        output: output.clone(),
    };
    let receipt = super::run(&args).unwrap();
    assert_eq!(receipt["status"], "PREPARED_UNREGISTERED");
    assert_eq!(fs::read(output.join("source.ndjson")).unwrap(), raw);
    assert_eq!(
        fs::read(output.join("warmup-features-0.json")).unwrap(),
        original_feature_bytes
    );
    let final_selection: NativeDatasetSelectionV1 = read(&output.join("selection.json")).unwrap();
    let replay =
        crate::catalog::load_catalog(&output.join("prepared/catalog"), &final_selection.selection)
            .unwrap();
    assert_eq!(&replay.series[0].bars[..2], &warmup);
    assert_eq!(replay.series[0].bars.len(), 4);
    let metadata: RuntimeCatalogMetadataV1 =
        read(&output.join("prepared/catalog-metadata.json")).unwrap();
    assert_eq!(metadata.origin, contracts::research::DataOrigin::Fixture);
    assert_eq!(metadata.pit_status, PitStatus::Unverified);
    assert_eq!(metadata.available_through, instant(4 * STEP + 1000));
    assert_eq!(
        metadata.quality.datasets[0].available_through_ns.get(),
        4 * STEP + 100
    );
    assert_eq!(
        replay.series[0].bars.last().unwrap().ts_init.as_u64(),
        4 * STEP + 100
    );
    assert!(
        super::run(&args).is_err(),
        "original publication is never overwritten"
    );
    assert_eq!(fs::read(recording).unwrap(), raw);
}
