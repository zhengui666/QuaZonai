//! Offline projection of retained native events. No network, model fitting,
//! qualification, backdating, imputation or registration occurs here.
use crate::polymarket_source_record::{ForwardSourcePlan, SourceRecord};
use anyhow::{Context, Result, bail, ensure};
use contracts::{
    DbCounter, Id, SchemaV1,
    catalogs::{RecordedFeatureFragmentV1, RecordedFeatureInputsV1, RuntimeCatalogMetadataV1},
    execution::NativeDatasetSelectionV1,
    research::{DataPartition, PitStatus},
    science::{
        FeatureAvailabilityV1, FeatureDefinitionV1, FeatureObservationV1, FeatureObservationsV1,
    },
};
use nautilus_common::{clock::TestClock, timer::TimeEvent};
use nautilus_core::UUID4;
use nautilus_data::aggregation::{BarAggregator, TimeBarAggregator};
use nautilus_model::{
    data::{Bar, QuoteTick, TradeTick},
    instruments::{Instrument, InstrumentAny},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{BufRead, BufReader, Write},
    path::{Path, PathBuf},
    rc::Rc,
};

#[derive(clap::Parser)]
pub struct Arguments {
    #[arg(long)]
    recording: PathBuf,
    #[arg(long)]
    plan: PathBuf,
    #[arg(long)]
    warmup_catalog: PathBuf,
    #[arg(long)]
    warmup_metadata: PathBuf,
    #[arg(long)]
    warmup_selection: PathBuf,
    #[arg(long, required = true)]
    original_features: Vec<PathBuf>,
    #[arg(long)]
    output: PathBuf,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PreparationPlan {
    schema_version: SchemaV1,
    source: ForwardSourcePlan,
    /// Local native quality identity; this is not a registered Dataset ID.
    dataset_revision_id: Id,
    feature_schema: Vec<FeatureDefinitionV1>,
    /// New source identity and unchanged Universe/terms; derived clocks/counts
    /// are produced from records, never accepted as client assertions.
    declaration: Value,
}

fn count(n: u64) -> Result<DbCounter> {
    DbCounter::new(n).map_err(anyhow::Error::msg)
}
fn microsecond_ceiling(n: u64) -> Result<u64> {
    Ok(n.checked_add(999)
        .context("FORWARD_REGISTRATION_TIME_RANGE")?
        / 1000
        * 1000)
}
#[cfg(test)]
fn read<T: serde::de::DeserializeOwned>(p: &Path) -> Result<T> {
    Ok(read_original(p)?.0)
}
fn read_original<T: serde::de::DeserializeOwned>(p: &Path) -> Result<(T, Vec<u8>)> {
    ensure!(
        fs::symlink_metadata(p)?.is_file(),
        "FORWARD_REGULAR_INPUT_REQUIRED"
    );
    let bytes = fs::read(p)?;
    Ok((serde_json::from_slice(&bytes)?, bytes))
}
fn write_new(p: &Path, bytes: &[u8]) -> Result<()> {
    let mut f = OpenOptions::new().write(true).create_new(true).open(p)?;
    f.write_all(bytes)?;
    f.sync_all()?;
    Ok(())
}
fn same_terms(a: &InstrumentAny, b: &InstrumentAny) -> Result<bool> {
    fn terms(v: &InstrumentAny) -> Result<Value> {
        let mut v = serde_json::to_value(v)?;
        let p = v
            .as_object_mut()
            .context("FORWARD_INSTRUMENT_SHAPE")?
            .values_mut()
            .next()
            .context("FORWARD_INSTRUMENT_SHAPE")?
            .as_object_mut()
            .context("FORWARD_INSTRUMENT_SHAPE")?;
        p.remove("ts_event");
        p.remove("ts_init");
        Ok(v)
    }
    Ok(terms(a)? == terms(b)?)
}

fn original_price(instrument: &InstrumentAny, price: nautilus_model::types::Price) -> Result<()> {
    ensure!(
        price.precision == instrument.price_precision()
            && instrument.try_normalize_price(price)? == price
            && price.as_f64() >= 0.0
            && price.as_f64() <= 1.0
            && instrument.min_price().is_none_or(|p| price >= p)
            && instrument.max_price().is_none_or(|p| price <= p),
        "FORWARD_ORIGINAL_PRICE_GRID_OR_BOUNDS"
    );
    Ok(())
}

/// The dictionary is versioned and explicit. Arbitrary existing feature names
/// cannot be assigned these meanings merely because their values look similar.
fn quote_fields<'a>(schema: &'a [FeatureDefinitionV1], instrument: &str) -> Result<Vec<&'a str>> {
    domain::execution::features::schema(schema)?;
    let expected = format!("nautilus-polymarket/0.63.0/quote/{instrument}");
    schema
        .iter()
        .map(|f| {
            ensure!(
                f.source_ref == expected && f.availability == FeatureAvailabilityV1::Observed,
                "FORWARD_FEATURE_SOURCE_UNSUPPORTED"
            );
            ensure!(
                matches!(
                    f.source_key.as_str(),
                    "bid_price" | "ask_price" | "bid_size" | "ask_size"
                ),
                "FORWARD_FEATURE_FIELD_UNSUPPORTED"
            );
            Ok(f.source_key.as_str())
        })
        .collect()
}

#[derive(Debug)]
struct Projection {
    bars: Vec<Bar>,
    features: FeatureObservationsV1,
    source_start: u64,
    source_end: u64,
    native_session_id: String,
}

fn project(
    input: &mut impl BufRead,
    plan: &ForwardSourcePlan,
    schema: &[FeatureDefinitionV1],
    original: &InstrumentAny,
    cursors: &BTreeMap<u16, u64>,
    last_warmup_available: u64,
) -> Result<Projection> {
    let (external, interval) = plan.native()?;
    let fields = quote_fields(schema, &plan.instrument_id)?;
    let first_start = plan.first_close_ns.get() - interval;
    let final_close = plan.final_close(interval)?;
    ensure!(
        original.id().to_string() == plan.instrument_id
            && matches!(original, InstrumentAny::BinaryOption(_)),
        "FORWARD_ORIGINAL_INSTRUMENT_REQUIRED"
    );
    let mut sequence = 0_u64;
    let mut observed = 0_u64;
    let mut started = false;
    let mut ready = false;
    let mut ended = false;
    let mut definition = false;
    let mut native_session_id = String::new();
    let mut lifecycle_ready = false;
    let mut lifecycle_coverage = false;
    let mut window_closed = false;
    let mut initial_book = false;
    let token = original.raw_symbol().to_string();
    let mut bars = Vec::new();
    let mut observations = Vec::new();
    let mut trades = Vec::<TradeTick>::new();
    let mut ids = BTreeSet::new();
    let native_type = plan
        .bar_type
        .strip_suffix("-EXTERNAL")
        .context("FORWARD_EXTERNAL_BAR_REQUIRED")?
        .to_owned()
        + "-INTERNAL";
    let emitted = Rc::new(RefCell::new(Vec::<Bar>::new()));
    let sink = emitted.clone();
    let clock = Rc::new(RefCell::new(TestClock::new()));
    // Replay only actual retained close events through the official aggregator.
    // TestClock is a deterministic replay clock, never a source availability clock.
    let mut aggregator = TimeBarAggregator::new(
        native_type.parse()?,
        original.price_precision(),
        original.size_precision(),
        clock.clone(),
        move |bar| sink.borrow_mut().push(bar),
        false,
        true,
        nautilus_model::enums::BarIntervalType::LeftOpen,
        None,
        0,
        false,
    );
    let mut next_close = plan.first_close_ns.get();
    let mut last_replayed_close = None;
    let mut aggregation_observed_before_window = false;
    loop {
        let mut raw = Vec::new();
        let n = input.read_until(b'\n', &mut raw)?;
        if n == 0 {
            break;
        }
        ensure!(
            raw.last() == Some(&b'\n'),
            "FORWARD_SOURCE_RECORD_INCOMPLETE"
        );
        ensure!(!ended, "FORWARD_RECORD_AFTER_END");
        let r: SourceRecord = serde_json::from_slice(&raw)?;
        ensure!(
            r.sequence.get() == sequence.checked_add(1).context("FORWARD_SEQUENCE_RANGE")?
                && r.observed_at_ns.get() >= observed,
            "FORWARD_SOURCE_SEQUENCE_OR_CLOCK_GAP"
        );
        sequence = r.sequence.get();
        observed = r.observed_at_ns.get();
        match r.kind.as_str() {
            "start" => {
                ensure!(!started && sequence == 1, "FORWARD_SOURCE_START");
                ensure!(
                    r.payload["purpose"] == "CURRENT_POLYMARKET_FORWARD_SOURCE"
                        && r.payload["execution_clients_registered"] == 0
                        && r.payload["forward_plan"] == serde_json::to_value(plan)?
                        && r.payload["native_version"]
                            == contracts::account_observation::NATIVE_ACCOUNT_VERSION
                        && r.payload["selected_instruments"] == json!([plan.instrument_id]),
                    "FORWARD_SOURCE_PLAN_BINDING"
                );
                native_session_id = r.payload["native_session_id"]
                    .as_str()
                    .context("FORWARD_NATIVE_SESSION_REQUIRED")?
                    .to_owned();
                native_session_id
                    .parse::<UUID4>()
                    .map_err(|_| anyhow::anyhow!("FORWARD_NATIVE_SESSION_REQUIRED"))?;
                started = true;
            }
            "instrument" => {
                ensure!(started, "FORWARD_SOURCE_START");
                let v: InstrumentAny = serde_json::from_value(r.payload)?;
                ensure!(
                    v.ts_event() <= v.ts_init()
                        && v.ts_init().as_u64() <= observed
                        && same_terms(original, &v)?,
                    "FORWARD_INSTRUMENT_CHANGED_OR_CLOCK"
                );
                definition = true;
            }
            "ready" => {
                ensure!(
                    started
                        && definition
                        && lifecycle_ready
                        && !ready
                        && observed <= first_start
                        && r.payload["data_clients_connected"] == true
                        && r.payload["definitions_complete"] == true
                        && r.payload["lifecycle_ready"] == true,
                    "FORWARD_COMPLETE_WINDOW_START_REQUIRED"
                );
                ready = true;
            }
            "quote" => {
                ensure!(ready, "FORWARD_SOURCE_NOT_READY");
                let q: QuoteTick = serde_json::from_value(r.payload)?;
                original_price(original, q.bid_price)?;
                original_price(original, q.ask_price)?;
                ensure!(
                    q.instrument_id == original.id()
                        && q.ts_event <= q.ts_init
                        && q.ts_init.as_u64() <= observed,
                    "FORWARD_QUOTE_IDENTITY_OR_CLOCK"
                );
                // Select newly observed inputs, not venue event-time buckets. An
                // old venue event may arrive now; the original max-age policy
                // decides whether it can inform a decision. Drain-only inputs
                // after the final BAR stay in the raw source, outside this fold.
                if observed <= last_warmup_available || bars.len() == plan.required_bars as usize {
                    continue;
                }
                for (index, field) in fields.iter().enumerate() {
                    let index = u16::try_from(index)?;
                    let value = match *field {
                        "bid_price" => q.bid_price.as_f64(),
                        "ask_price" => q.ask_price.as_f64(),
                        "bid_size" => q.bid_size.as_f64(),
                        "ask_size" => q.ask_size.as_f64(),
                        _ => unreachable!(),
                    };
                    ensure!(value.is_finite(), "FORWARD_FEATURE_NONFINITE");
                    observations.push(FeatureObservationV1 {
                        feature_index: index,
                        event_ns: count(q.ts_event.as_u64())?,
                        observed_available_ns: Some(r.observed_at_ns),
                        sequence: count(
                            cursors
                                .get(&index)
                                .copied()
                                .unwrap_or(0)
                                .checked_add(sequence)
                                .context("FORWARD_FEATURE_SEQUENCE_RANGE")?,
                        )?,
                        value: Some(value),
                        missing_reason: None,
                    });
                }
            }
            "trade" => {
                ensure!(ready, "FORWARD_SOURCE_NOT_READY");
                let t: TradeTick = serde_json::from_value(r.payload)?;
                original_price(original, t.price)?;
                ensure!(
                    t.instrument_id == original.id()
                        && t.ts_event <= t.ts_init
                        && t.ts_init.as_u64() <= observed
                        && t.size.as_f64() > 0.0,
                    "FORWARD_TRADE_IDENTITY_OR_CLOCK"
                );
                ensure!(ids.insert(t.trade_id), "FORWARD_TRADE_ID_AMBIGUOUS");
                // Official aggregation uses native receive ordering (ts_init).
                // Retain and replay every trade, including prewindow and late
                // venue events; never create a second event-time OHLC algorithm.
                aggregator.handle_trade(t);
                trades.push(t);
            }
            "bar_close" => {
                ensure!(ready, "FORWARD_BAR_CLOSE_COUNT");
                let native: Bar = serde_json::from_value(r.payload)?;
                ensure!(
                    native.bar_type.instrument_id() == original.id()
                        && native.bar_type.spec() == external.spec()
                        && native.ts_event <= native.ts_init
                        && native.ts_init.as_u64() <= observed,
                    "FORWARD_BAR_CLOSE_CLOCK_OR_TYPE"
                );
                let close = native.ts_event.as_u64();
                ensure!(
                    close % interval == 0
                        && last_replayed_close.is_none_or(|last| close > last)
                        && native.bar_type.to_string() == native_type,
                    "FORWARD_NATIVE_CLOSE_ORDER"
                );
                ensure!(!trades.is_empty(), "FORWARD_EMPTY_BAR_UNSUPPORTED");
                clock.borrow_mut().set_time(r.observed_at_ns.get().into());
                let event = TimeEvent::new(
                    native_type.as_str().into(),
                    UUID4::new(),
                    native.ts_event,
                    native.ts_init,
                );
                BarAggregator::build_bar(&mut aggregator, &event);
                let mut native_outputs = emitted.borrow_mut();
                ensure!(
                    native_outputs.len() == 1 && native_outputs[0] == native,
                    // A suppressed initial partial bar has no retained close
                    // callback. Without enough original evidence to replay that
                    // initialization we refuse it, rather than guess a reset.
                    "FORWARD_NATIVE_BAR_REPLAY_OR_INITIAL_PARTIAL_UNSUPPORTED"
                );
                native_outputs.clear();
                trades.clear();
                last_replayed_close = Some(close);
                // SourceActor readiness precedes the BAR subscription command.
                // Only an actual, successfully replayed native close observed by
                // first_start proves aggregation was already active for the
                // entire first selected interval. A startup partial cannot pass.
                if observed <= first_start {
                    aggregation_observed_before_window = true;
                }
                if close < plan.first_close_ns.get() || close > final_close {
                    continue;
                }
                ensure!(
                    aggregation_observed_before_window,
                    "FORWARD_AGGREGATION_START_EVIDENCE_REQUIRED"
                );
                ensure!(
                    bars.len() < plan.required_bars as usize
                        && close == next_close
                        && observed > last_warmup_available
                        && bars
                            .last()
                            .is_none_or(|b: &Bar| b.ts_init.as_u64() < observed),
                    "FORWARD_BAR_CLOSE_CLOCK_OR_TYPE"
                );
                // Only catalog representation and proven availability are projected.
                // OHLCV remains the exact official result, retained in source.ndjson.
                bars.push(Bar {
                    bar_type: external,
                    ts_init: observed.into(),
                    ..native
                });
                next_close = next_close
                    .checked_add(interval)
                    .context("FORWARD_CLOSE_RANGE")?;
            }
            "end" => {
                ensure!(
                    ready
                        && lifecycle_coverage
                        && window_closed
                        && r.payload["complete"] == true
                        && r.payload["gap"] == false
                        && r.payload["dropped_events"] == 0
                        && r.payload["encoding_failed"] == false
                        && r.payload["native_shutdown_confirmed"] == true
                        && r.payload["lifecycle_complete"] == true
                        && r.payload["native_returned_success"] == true
                        && r.payload.get("native_failure") == Some(&Value::Null)
                        && r.payload["all_selected_ticks_observed"] == true
                        && r.payload["ready"] == true
                        && r.payload["forward_bars_observed"] == plan.required_bars
                        && r.payload["forward_observations_complete"] == true,
                    "FORWARD_SOURCE_INCOMPLETE"
                );
                ended = true;
            }
            "gap" | "lifecycle_reconnected" | "lifecycle_event" | "status" | "close" => {
                bail!("FORWARD_GAP_OR_CHANGED_MARKET")
            }
            "socket" | "lifecycle_socket" => {
                ensure!(r.payload["state"] == "Connected", "FORWARD_TRANSPORT_GAP")
            }
            "queue" => ensure!(
                matches!(r.payload["condition"].as_str(), Some("Slow" | "Backlogged"))
                    && matches!(r.payload["state"].as_str(), Some("Triggered" | "Cleared")),
                "FORWARD_QUEUE_GAP"
            ),
            "lifecycle_initial_book" => {
                ensure!(
                    started && !lifecycle_ready && r.payload["asset_id"] == token,
                    "FORWARD_LIFECYCLE_BOOK_SCOPE"
                );
                initial_book = true;
            }
            "lifecycle_ready" => {
                ensure!(
                    started
                        && initial_book
                        && !lifecycle_ready
                        && !ready
                        && r.payload["initial_books_complete"] == true
                        && r.payload["tokens"] == json!([token]),
                    "FORWARD_LIFECYCLE_READY_FACTS_REQUIRED"
                );
                lifecycle_ready = true;
            }
            "lifecycle_coverage" => {
                ensure!(
                    ready
                        && !lifecycle_coverage
                        && r.payload["primary_native_drain_finished"] == true
                        && r.payload["parsed_message_channel_drained"] == true
                        && r.payload["scope"]
                            == "OBSERVED_NATIVE_AND_PARSED_PUBLIC_EVENTS_NOT_ALL_EXCHANGE_PACKETS",
                    "FORWARD_LIFECYCLE_DRAIN_FACTS_REQUIRED"
                );
                lifecycle_coverage = true;
            }
            "forward_window_closed" => {
                ensure!(
                    started
                        && !window_closed
                        && observed >= final_close
                        && r.payload["bar_window_end_ns"] == json!(count(final_close)?),
                    "FORWARD_BUSINESS_CLOSE_OBSERVATION_REQUIRED"
                );
                window_closed = true;
            }
            "source_phase" => ensure!(started, "FORWARD_SOURCE_START"),
            _ => bail!("FORWARD_RECORD_KIND_UNSUPPORTED"),
        }
    }
    ensure!(
        ended && bars.len() == plan.required_bars as usize,
        "FORWARD_BUSINESS_WINDOW_INCOMPLETE"
    );
    let available = bars.last().context("FORWARD_NO_BARS")?.ts_init.as_u64();
    ensure!(
        observations.iter().all(|o| o
            .observed_available_ns
            .is_some_and(|t| t.get() <= available)),
        "FORWARD_FEATURE_AFTER_FINAL_DECISION"
    );
    let features = FeatureObservationsV1 {
        schema_version: SchemaV1,
        partition: DataPartition::Forward,
        feature_schema: schema.to_vec(),
        observations,
    };
    domain::execution::features::observations(&features)?;
    let source_start = features
        .observations
        .iter()
        .map(|o| o.event_ns.get())
        .min()
        .context("FORWARD_NO_FEATURES")?;
    let source_end = features
        .observations
        .iter()
        .map(|o| o.event_ns.get())
        .max()
        .unwrap()
        .checked_add(1)
        .context("FORWARD_FEATURE_EVENT_RANGE")?;
    Ok(Projection {
        bars,
        features,
        source_start,
        source_end,
        native_session_id,
    })
}

pub(super) fn run(args: &Arguments) -> Result<Value> {
    let (plan, plan_bytes): (PreparationPlan, Vec<u8>) = read_original(&args.plan)?;
    let _ = plan.schema_version;
    let (bar_type, interval) = plan.source.native()?;
    quote_fields(&plan.feature_schema, &plan.source.instrument_id)?;
    let (metadata, metadata_bytes): (RuntimeCatalogMetadataV1, Vec<u8>) =
        read_original(&args.warmup_metadata)?;
    domain::catalogs::metadata(&metadata, metadata.quality.checked_at)?;
    let (selection, selection_bytes): (NativeDatasetSelectionV1, Vec<u8>) =
        read_original(&args.warmup_selection)?;
    ensure!(
        selection.settlements.is_empty(),
        "FORWARD_SETTLEMENT_NOT_SUPPORTED"
    );
    let (market, measured) =
        crate::catalog::measure_catalog(&args.warmup_catalog, &selection, true)?;
    ensure!(
        market.series.len() == 1
            && metadata.quality.datasets.len() == 1
            && serde_json::to_value(&measured)?
                == serde_json::to_value(&metadata.quality.datasets[0])?,
        "FORWARD_WARMUP_METADATA_BINDING"
    );
    let series = &market.series[0];
    ensure!(
        series.instrument.id().to_string() == plan.source.instrument_id
            && series.bar_type == bar_type
            && series.instrument_updates.is_empty(),
        "FORWARD_WARMUP_INSTRUMENT_OR_TERMS_UNSUPPORTED"
    );
    let last = series.bars.last().context("FORWARD_WARMUP_EMPTY")?;
    ensure!(
        last.ts_event.as_u64().checked_add(interval) == Some(plan.source.first_close_ns.get()),
        "FORWARD_WARMUP_CONTINUATION_GAP"
    );
    let original_parts = args
        .original_features
        .iter()
        .map(|p| read_original::<FeatureObservationsV1>(p))
        .collect::<Result<Vec<_>>>()?;
    let parts = original_parts
        .iter()
        .map(|(part, _)| part.clone())
        .collect::<Vec<_>>();
    domain::execution::features::bind_observations(
        &parts,
        &plan.feature_schema,
        DataPartition::Validation,
    )?;
    let mut cursors = BTreeMap::<u16, u64>::new();
    for row in parts.iter().flat_map(|p| &p.observations) {
        cursors
            .entry(row.feature_index)
            .and_modify(|n| *n = (*n).max(row.sequence.get()))
            .or_insert(row.sequence.get());
    }
    fs::create_dir(&args.output)?;
    let retained = args.output.join("source.ndjson");
    fs::copy(&args.recording, &retained)?;
    let observed = project(
        &mut BufReader::new(File::open(&retained)?),
        &plan.source,
        &plan.feature_schema,
        &series.instrument,
        &cursors,
        last.ts_init.as_u64(),
    )?;
    let mut bars = series.bars.clone();
    bars.extend_from_slice(&observed.bars);
    let feature_bytes = serde_json::to_vec(&observed.features)?;
    let feature_path = args.output.join("forward-features.json");
    write_new(&feature_path, &feature_bytes)?;
    for (i, (_, bytes)) in original_parts.iter().enumerate() {
        write_new(
            &args.output.join(format!("warmup-features-{i}.json")),
            bytes,
        )?;
    }
    write_new(&args.output.join("warmup-metadata.json"), &metadata_bytes)?;
    write_new(&args.output.join("warmup-selection.json"), &selection_bytes)?;
    write_new(&args.output.join("preparation-plan.json"), &plan_bytes)?;
    let native_root = args.output.join("native-input");
    fs::create_dir(&native_root)?;
    let catalog = ParquetDataCatalog::from_uri(
        native_root.to_str().context("FORWARD_PATH_ENCODING")?,
        None,
        None,
        None,
        None,
    )?;
    catalog.write_instruments(vec![series.instrument.clone()])?;
    catalog.write_to_parquet(&bars, None, None, None)?;
    let final_bar = bars.last().unwrap();
    let mut selected = selection.clone();
    selected.dataset_revision_id = plan.dataset_revision_id;
    // PostgreSQL registration uses microseconds. Enclose the original nanosecond
    // records conservatively; never round a native receipt backwards or rewrite it.
    selected.selection.event_end_ns = count(microsecond_ceiling(
        final_bar
            .ts_event
            .as_u64()
            .checked_add(1)
            .context("FORWARD_RANGE")?,
    )?)?;
    selected.selection.decision_cutoff_ns =
        count(microsecond_ceiling(final_bar.ts_init.as_u64())?)?;
    selected.selection.maximum_rows = u32::try_from(bars.len())?;
    let to_time = |n: DbCounter| -> Result<chrono::DateTime<chrono::Utc>> {
        Ok(chrono::DateTime::from_timestamp_nanos(i64::try_from(
            n.get(),
        )?))
    };
    let mut declaration = plan.declaration;
    let object = declaration
        .as_object_mut()
        .context("FORWARD_DECLARATION_OBJECT")?;
    ensure!(
        !object.contains_key("row_count")
            && !object.contains_key("quality")
            && !object.contains_key("recorded_feature_inputs"),
        "FORWARD_MEASURED_FIELDS_NOT_CALLER_ASSERTIONS"
    );
    ensure!(
        object.get("universe") == Some(&serde_json::to_value(&metadata.universe)?)
            && object.get("origin") == Some(&serde_json::to_value(metadata.origin)?),
        "FORWARD_ORIGINAL_UNIVERSE_ORIGIN_REQUIRED"
    );
    ensure!(
        object.get("pit_status") == Some(&serde_json::to_value(PitStatus::Unverified)?),
        "FORWARD_PIT_NOT_GRANTED"
    );
    object.insert("partition".into(), json!("FORWARD"));
    object.insert(
        "event_start".into(),
        serde_json::to_value(to_time(selected.selection.event_start_ns)?)?,
    );
    object.insert(
        "event_end".into(),
        serde_json::to_value(to_time(selected.selection.event_end_ns)?)?,
    );
    object.insert(
        "available_through".into(),
        serde_json::to_value(to_time(selected.selection.decision_cutoff_ns)?)?,
    );
    let rows = &observed.features.observations;
    let fragment = RecordedFeatureFragmentV1 {
        part_key: "native-quote-forward-000".into(),
        byte_count: count(feature_bytes.len() as u64)?,
        observations: count(rows.len() as u64)?,
        min_event_ns: rows
            .iter()
            .map(|r| r.event_ns)
            .min()
            .context("FORWARD_NO_FEATURES")?,
        max_event_ns: rows.iter().map(|r| r.event_ns).max().unwrap(),
        min_observed_available_ns: rows
            .iter()
            .filter_map(|r| r.observed_available_ns)
            .min()
            .unwrap(),
        max_observed_available_ns: rows
            .iter()
            .filter_map(|r| r.observed_available_ns)
            .max()
            .unwrap(),
    };
    object.insert(
        "recorded_feature_inputs".into(),
        serde_json::to_value(RecordedFeatureInputsV1 {
            schema_version: SchemaV1,
            source_selection_start_ns: count(observed.source_start)?,
            source_selection_end_ns: count(observed.source_end)?,
            partition: DataPartition::Forward,
            fragments: vec![fragment],
        })?,
    );
    let declaration_path = args.output.join("declaration.json");
    write_new(&declaration_path, &serde_json::to_vec(&declaration)?)?;
    let selection_path = args.output.join("selection.json");
    write_new(&selection_path, &serde_json::to_vec(&selected)?)?;
    let prepared = super::prepare(&super::Arguments {
        catalog: native_root,
        declaration: declaration_path,
        selection: selection_path,
        output: args.output.join("prepared"),
    })?;
    domain::data::recorded_feature_matches(
        &prepared,
        "native-quote-forward-000",
        count(feature_bytes.len() as u64)?,
        &observed.features,
    )?;
    let (_, roundtrip) =
        crate::catalog::measure_catalog(&args.output.join("prepared/catalog"), &selected, true)?;
    ensure!(
        roundtrip.row_count.get() == bars.len() as u64,
        "FORWARD_NATIVE_READBACK"
    );
    let receipt = json!({"schema_version":1,"status":"PREPARED_UNREGISTERED","native_version":"0.63.0","native_source_session_id":observed.native_session_id,"source_plan":plan.source,"warmup_rows":series.bars.len(),"new_bars":observed.bars.len(),"feature_observations":rows.len(),"catalog":"prepared/catalog","metadata":"prepared/catalog-metadata.json","features":"forward-features.json","pit":"UNVERIFIED","warmup_model_replay":"NOT_PERFORMED_CURRENT_DECISION_RECHECKS_ORIGINAL_POLICY","orders":false});
    write_new(
        &args.output.join("forward-preparation.json"),
        &serde_json::to_vec_pretty(&receipt)?,
    )?;
    Ok(receipt)
}

#[cfg(test)]
#[path = "polymarket_forward_tests.rs"]
mod tests;
