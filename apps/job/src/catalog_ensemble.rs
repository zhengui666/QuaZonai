//! Bounded time partitions, causal all-market policy, one official native account.
use crate::{
    catalog::{NativeBarSeries, NativeMarketData},
    catalog_ensemble_policy::CatalogEnsemblePolicy,
};
use anyhow::{ensure, Context, Result};
use bigdecimal::ToPrimitive;
use contracts::{
    catalog_ensemble::*,
    portfolio::{AllocationTargetV1, NativeModelRefV1},
    recorded_fees::*,
    science::*,
    DbCounter, SchemaV1,
};
use nautilus_backtest::{
    config::{BacktestEngineConfig, SimulatedVenueConfig},
    engine::BacktestEngine,
};
use nautilus_common::{cache::CacheConfig, logging::logger::LoggerConfig};
use nautilus_core::UnixNanos;
use nautilus_execution::models::{
    fill::{DefaultFillModel, FillModelHandle},
    latency::{LatencyModelHandle, StaticLatencyModel},
};
use nautilus_model::{
    data::{Bar, BarType, CustomData, CustomDataTrait, Data, DataType, HasTsInit},
    enums::{AccountType, BookType, CurrencyType, OmsType},
    identifiers::{InstrumentId, Venue},
    instruments::{CurrencyPair, Instrument, InstrumentAny},
    types::{Currency, Money, Price, Quantity},
};
use nautilus_portfolio::config::PortfolioConfig;
use nautilus_serialization::arrow::DecodeFromRecordBatch;
use parquet::{arrow::arrow_reader::ParquetRecordBatchReaderBuilder, file::statistics::Statistics};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::{
    any::Any,
    collections::{BTreeMap, BTreeSet, HashMap, HashSet},
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    str::FromStr,
    sync::Arc,
};

pub(crate) const EVENT_TYPE: &str = "QzCatalogEnsembleWakeV1";
const MINUTE: u64 = 60_000_000_000;

#[cfg(test)]
mod tests;

#[derive(Clone, Debug, PartialEq, Serialize)]
struct Wake {
    at: u64,
}
impl HasTsInit for Wake {
    fn ts_init(&self) -> UnixNanos {
        self.at.into()
    }
}
impl CustomDataTrait for Wake {
    fn type_name(&self) -> &'static str {
        EVENT_TYPE
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
    fn ts_event(&self) -> UnixNanos {
        self.at.into()
    }
    fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }
    fn clone_arc(&self) -> Arc<dyn CustomDataTrait> {
        Arc::new(self.clone())
    }
    fn eq_arc(&self, other: &dyn CustomDataTrait) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }
}
fn wake(at: u64) -> Data {
    Data::Custom(CustomData::new(
        Arc::new(Wake { at }),
        DataType::new(EVENT_TYPE, None, None),
    ))
}
fn counter(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

/// Read the existing operator summary directly; paths remain private CLI inputs.
#[derive(Deserialize)]
struct Manifest {
    parts: Vec<Partition>,
}
#[derive(Deserialize)]
struct Partition {
    source_start: String,
    source_end_exclusive: String,
    offline_lossless_archive: PathBuf,
    records: u64,
    bars: u64,
}
fn timestamp(text: &str) -> Result<u64> {
    Ok(u64::try_from(
        chrono::DateTime::parse_from_rfc3339(text)?
            .timestamp_nanos_opt()
            .context("CATALOG_ENSEMBLE_SOURCE_TIME")?,
    )?)
}

fn source(
    archive: &Path,
    request: &NativeCatalogEnsembleRequestV1,
    expected_records: u64,
) -> Result<RecordedTradeFeesV1> {
    let mut child = Command::new("tar")
        .arg("--zstd")
        .arg("-xOf")
        .arg(archive)
        .arg("--wildcards")
        .arg("./*.json")
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let parsed = (|| -> Result<RecordedTradeFeesV1> {
        let reader = BufReader::new(
            child
                .stdout
                .take()
                .context("CATALOG_ENSEMBLE_ARCHIVE_PIPE")?,
        );
        let mut combined = RecordedTradeFeesV1 {
            schema_version: SchemaV1,
            source: request.source.clone(),
            revision: request.revision.clone(),
            currency: request.settings.base_currency.clone(),
            records: Vec::new(),
        };
        for part in serde_json::Deserializer::from_reader(reader).into_iter::<RecordedTradeFeesV1>()
        {
            let part = part?;
            ensure!(
                part.source == combined.source
                    && part.revision == combined.revision
                    && part.currency == combined.currency,
                "CATALOG_ENSEMBLE_ORIGINAL_SOURCE_BINDING"
            );
            combined.records.extend(part.records);
        }
        ensure!(
            u64::try_from(combined.records.len())? == expected_records,
            "CATALOG_ENSEMBLE_PARTITION_RECORD_COUNT"
        );
        Ok(combined)
    })();
    if parsed.is_err() {
        let _ = child.kill();
    }
    let status = child.wait()?;
    let source = parsed?;
    ensure!(status.success(), "CATALOG_ENSEMBLE_ARCHIVE_READ_FAILED");
    Ok(source)
}

fn model_instrument(id: InstrumentId, bar: &Bar, at: u64) -> Result<InstrumentAny> {
    let base = if let Some(currency) = Currency::try_from_str("QZSHARE") {
        currency
    } else {
        let currency = Currency::new_checked(
            "QZSHARE",
            6,
            0,
            "Conditional token-share proxy",
            CurrencyType::Crypto,
        )?;
        Currency::register(currency, false)?;
        currency
    };
    ensure!(
        id.venue == Venue::from("POLYMARKET"),
        "CATALOG_ENSEMBLE_VENUE"
    );
    Ok(InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(id)
            .raw_symbol(id.symbol)
            .base_currency(base)
            .quote_currency(Currency::from_str("pUSD")?)
            .price_precision(bar.close.precision)
            .size_precision(bar.volume.precision)
            .price_increment(Price::new(
                10_f64.powi(-i32::from(bar.close.precision)),
                bar.close.precision,
            ))
            .size_increment(Quantity::new(
                10_f64.powi(-i32::from(bar.volume.precision)),
                bar.volume.precision,
            ))
            .margin_init(Decimal::ONE)
            .margin_maint(Decimal::ONE)
            .ts_event(at.into())
            .ts_init(at.into())
            .build()?,
    ))
}

fn bound_scope(root: &Path, scope: &[String]) -> Result<Vec<BarType>> {
    let kinds = scope
        .iter()
        .map(|name| Ok(crate::catalog::bar_types(std::slice::from_ref(name))?[0]))
        .collect::<Result<Vec<_>>>()?;
    let mut directories = kinds
        .iter()
        .map(|kind| {
            nautilus_persistence::backend::catalog::urisafe_instrument_id(&kind.to_string())
        })
        .collect::<HashSet<_>>();
    ensure!(
        !kinds.is_empty() && directories.len() == kinds.len(),
        "CATALOG_ENSEMBLE_SCOPE_CHANGED"
    );
    for entry in fs::read_dir(root.join("data/bars"))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            ensure!(
                entry
                    .file_name()
                    .to_str()
                    .is_some_and(|name| directories.remove(name)),
                "CATALOG_ENSEMBLE_SCOPE_CHANGED"
            );
        }
    }
    ensure!(directories.is_empty(), "CATALOG_ENSEMBLE_SCOPE_CHANGED");
    Ok(kinds)
}

/// Whole in-window row groups can be counted from official Parquet metadata.
/// Decode only boundary/unknown groups when checking the independent scope.
fn series_rows(
    root: &Path,
    kind: BarType,
    start: u64,
    end: u64,
    retain_bars: bool,
) -> Result<(u64, Vec<Bar>)> {
    let name = kind.to_string();
    let directory = root
        .join("data/bars")
        .join(nautilus_persistence::backend::catalog::urisafe_instrument_id(&name));
    let mut files = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    files.retain(|path| {
        path.extension()
            .is_some_and(|extension| extension == "parquet")
    });
    files.sort();
    ensure!(!files.is_empty(), "CATALOG_ENSEMBLE_NATIVE_BARS_REQUIRED");
    let mut rows = 0_u64;
    let mut bars = Vec::new();
    for path in files {
        if retain_bars
            && nautilus_persistence::backend::catalog::parse_filename_timestamps(
                &path.to_string_lossy(),
            )
            .is_some_and(|(first, last)| last < start || first >= end)
        {
            continue;
        }
        let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?;
        let metadata = builder.schema().metadata().clone();
        ensure!(
            metadata.get("bar_type") == Some(&name),
            "CATALOG_ENSEMBLE_BAR_IDENTITY"
        );
        let event_index = builder.schema().index_of("ts_event")?;
        let mut groups = Vec::new();
        for (index, group) in builder.metadata().row_groups().iter().enumerate() {
            let stats = group
                .columns()
                .get(event_index)
                .and_then(|column| column.statistics());
            let bounds = match stats {
                Some(Statistics::Int64(stats)) => stats
                    .min_opt()
                    .zip(stats.max_opt())
                    .map(|(first, last)| (*first as u64, *last as u64)),
                _ => None,
            };
            if let Some((first, last)) = bounds.filter(|(first, last)| first <= last) {
                if last < start || first >= end {
                    continue;
                }
                if !retain_bars
                    && first >= start
                    && last < end
                    && stats.and_then(|stats| stats.null_count_opt()) == Some(0)
                {
                    rows += u64::try_from(group.num_rows())?;
                    continue;
                }
            }
            groups.push(index);
        }
        if groups.is_empty() {
            continue;
        }
        for batch in builder
            .with_row_groups(groups)
            .with_batch_size(4096)
            .build()?
        {
            for bar in Bar::decode_batch(&metadata, batch?)? {
                ensure!(bar.bar_type == kind, "CATALOG_ENSEMBLE_BAR_IDENTITY");
                let at = bar.ts_event.as_u64();
                if at >= start && at < end {
                    rows += 1;
                    if retain_bars {
                        bars.push(bar);
                    }
                }
            }
        }
    }
    Ok((rows, bars))
}

fn market(
    root: &Path,
    source: &RecordedTradeFeesV1,
    first: u64,
    last: u64,
    request: &NativeCatalogEnsembleRequestV1,
    covered: &mut HashMap<BarType, u64>,
) -> Result<NativeMarketData> {
    let ids = source
        .records
        .iter()
        .map(|record| record.instrument_id.as_str())
        .collect::<BTreeSet<_>>();
    let mut series = Vec::new();
    let mut rows = 0;
    for id in ids {
        let kind = BarType::from_str(&format!("{id}-1-MINUTE-LAST-EXTERNAL"))?;
        let (_, mut bars) = series_rows(
            root,
            kind,
            first.checked_add(1).context("CATALOG_ENSEMBLE_CLOCK")?,
            last.checked_add(1).context("CATALOG_ENSEMBLE_CLOCK")?,
            true,
        )?;
        bars.sort_by_key(|bar| (bar.ts_init, bar.ts_event));
        for (index, bar) in bars.iter().enumerate() {
            crate::catalog::validate_price_bar(
                bar,
                index.checked_sub(1).map(|previous| &bars[previous]),
            )?;
        }
        *covered.entry(kind).or_default() += u64::try_from(
            bars.iter()
                .filter(|bar| {
                    bar.ts_event.as_u64() >= request.event_start_ns.get()
                        && bar.ts_event.as_u64() < request.event_end_ns.get()
                })
                .count(),
        )?;
        let baseline = bars
            .first()
            .context("CATALOG_ENSEMBLE_NATIVE_BARS_REQUIRED")?;
        let instrument =
            model_instrument(kind.instrument_id(), baseline, request.event_start_ns.get())?;
        rows += bars.len();
        series.push(NativeBarSeries {
            instrument,
            instrument_updates: Vec::new(),
            bar_type: kind,
            bars,
        });
    }
    Ok(NativeMarketData { series, rows })
}

fn complete_scope(
    root: &Path,
    scope: &[BarType],
    request: &NativeCatalogEnsembleRequestV1,
    mut covered: HashMap<BarType, u64>,
) -> Result<()> {
    for (index, &kind) in scope.iter().enumerate() {
        if index > 0 && index % 10_000 == 0 {
            eprintln!("catalog_ensemble: scope_series={index}/{}", scope.len());
        }
        let (expected, _) = series_rows(
            root,
            kind,
            request.event_start_ns.get(),
            request.event_end_ns.get(),
            false,
        )?;
        ensure!(
            covered.remove(&kind).unwrap_or(0) == expected,
            "CATALOG_ENSEMBLE_SOURCE_SCOPE_INCOMPLETE"
        );
    }
    ensure!(covered.is_empty(), "CATALOG_ENSEMBLE_SCOPE_CHANGED");
    Ok(())
}

pub fn replay(
    root: &Path,
    manifest: &Path,
    request: &NativeCatalogEnsembleRequestV1,
    models: &[Vec<u8>],
    scope: &[String],
) -> Result<NativeCatalogEnsembleResultV1> {
    ensure!(
        request.source == POLYMARKET_V2_SOURCE
            && !request.revision.trim().is_empty()
            && request.event_start_ns < request.evaluation_start_ns
            && request.evaluation_start_ns < request.event_end_ns
            && request.event_end_ns <= request.decision_cutoff_ns
            && models.len() == request.members.len(),
        "CATALOG_ENSEMBLE_REQUEST"
    );
    let settings = &request.settings;
    ensure!(
        settings.account_kind == NativeAccountKind::Margin
            && settings.base_currency == "pUSD"
            && settings.leverage.as_decimal() == &bigdecimal::BigDecimal::from(1)
            && settings.starting_capital.is_positive()
            && settings.exposure_tolerance.is_positive()
            && settings.exposure_tolerance.as_decimal()
                <= &bigdecimal::BigDecimal::new(1.into(), 3)
            && (1..=86_400_000).contains(&settings.snapshot_interval_ms)
            && settings.multi_currency_spot_cash.is_none()
            && matches!(
                settings.fee_model,
                NativeModelRefV1::PolymarketV2RecordedFees { .. }
            )
            && matches!(
                settings.fill_model,
                NativeModelRefV1::PolymarketV2RecordedLiquidity { .. }
            ),
        "CATALOG_ENSEMBLE_CONDITIONAL_ACCOUNT"
    );
    let gross = Decimal::from_str(&request.gross_exposure.as_decimal().to_plain_string())?;
    ensure!(
        gross > Decimal::ZERO && gross <= Decimal::ONE && request.target_ttl_ns.get() > 0,
        "CATALOG_ENSEMBLE_EXPOSURE"
    );
    let (fill, latency) = domain::portfolio::simulation_models(settings)?;
    let insert_ns = latency
        .base_latency_ns
        .get()
        .checked_add(latency.insert_latency_ns.get())
        .context("CATALOG_ENSEMBLE_LATENCY")?;
    let settlement_ns = insert_ns
        .checked_mul(2)
        .context("CATALOG_ENSEMBLE_LATENCY")?;
    ensure!(
        request.target_ttl_ns.get() > settlement_ns,
        "CATALOG_ENSEMBLE_TARGET_EXPIRES_BEFORE_INSERT"
    );
    let root = crate::catalog::catalog_root(root)?;
    crate::catalog::currencies::load(&root)?;
    let scope = bound_scope(&root, scope)?;
    let bindings = models
        .iter()
        .zip(&request.members)
        .map(|(bytes, member)| {
            Ok((
                &bytes[..],
                member
                    .weight
                    .as_decimal()
                    .to_f64()
                    .context("CATALOG_ENSEMBLE_MEMBER_WEIGHT")?,
                member.total_fuel.map(|fuel| fuel.get()),
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut policy = CatalogEnsemblePolicy::new(
        &format!("{}@{}", request.source, request.revision),
        request.modeled_lag_ns,
        request.score_max_age_ns.map(|age| age.get()),
        request.score_threshold,
        &bindings,
    )?;
    let manifest: Manifest = serde_json::from_reader(File::open(manifest)?)?;
    let first_source = manifest
        .parts
        .first()
        .context("CATALOG_ENSEMBLE_SOURCE_REQUIRED")?;
    let last_source = manifest
        .parts
        .last()
        .context("CATALOG_ENSEMBLE_SOURCE_REQUIRED")?;
    ensure!(
        timestamp(&first_source.source_start)? <= request.event_start_ns.get()
            && timestamp(&last_source.source_end_exclusive)?
                .checked_add(1)
                .is_some_and(|end| end >= request.event_end_ns.get()),
        "CATALOG_ENSEMBLE_SOURCE_WINDOW_REQUIRED"
    );
    let config = BacktestEngineConfig {
        cache: Some(
            CacheConfig::builder()
                .tick_capacity(1)
                .bar_capacity(1)
                .build()?,
        ),
        portfolio: Some(
            PortfolioConfig::builder()
                .snapshot_interval_ms(u64::from(settings.snapshot_interval_ms))
                .build()?,
        ),
        logging: LoggerConfig::builder()
            .bypass_logging(true)
            .is_colored(false)
            .build()?,
        shutdown_on_error: true,
        bypass_logging: true,
        ..Default::default()
    };
    let mut engine = BacktestEngine::new(config)?;
    let result = (|| -> Result<NativeCatalogEnsembleResultV1> {
        let mut fees = None;
        let mut liquidity = None;
        let fee_errors = crate::prediction::FillFeeObserver::default();
        let (strategy, status, inbox) =
            crate::simulation::catalog_target_strategy(settings.clone(), gross)?;
        engine.add_strategy(strategy)?;
        let mut registered = BTreeSet::new();
        let mut covered = HashMap::new();
        let mut previous_end = None;
        let mut records = 0_u64;
        let mut native_started = false;
        let mut account_started = false;
        let mut last_available: Option<u64> = None;
        for (ordinal, part) in manifest.parts.iter().enumerate() {
            let first = timestamp(&part.source_start)?;
            let last = timestamp(&part.source_end_exclusive)?;
            ensure!(
                first < last
                    && first.is_multiple_of(MINUTE)
                    && last.is_multiple_of(MINUTE)
                    && previous_end.is_none_or(|end| end == first),
                "CATALOG_ENSEMBLE_PARTITION_CONTINUITY"
            );
            previous_end = Some(last);
            if last < request.event_start_ns.get() || first >= request.event_end_ns.get() {
                continue;
            }
            eprintln!(
                "catalog_ensemble: partition={}/{} observations={} markets={}",
                ordinal + 1,
                manifest.parts.len(),
                policy.observations(),
                policy.observed_markets()
            );
            let source = source(&part.offline_lossless_archive, request, part.records)?;
            ensure!(
                source
                    .records
                    .iter()
                    .all(|record| record.ts_event_ns.get() >= first
                        && record.ts_event_ns.get() < last),
                "CATALOG_ENSEMBLE_PARTITION_SOURCE_TIME"
            );
            let market = market(&root, &source, first, last, request, &mut covered)?;
            ensure!(
                u64::try_from(market.rows)? == part.bars,
                "CATALOG_ENSEMBLE_PARTITION_BAR_COUNT"
            );
            records += u64::try_from(source.records.len())?;
            if let Some(fees) = &fees {
                crate::recorded_fees::RecordedFeeModel::replace_source(fees, source, &market)?;
            } else {
                let model = crate::recorded_fees::RecordedFeeModel::new(
                    source,
                    &market,
                    engine.kernel().clock.clone(),
                )?;
                let default_fill = DefaultFillModel::new(
                    fill.prob_fill_on_limit
                        .as_decimal()
                        .to_f64()
                        .context("CATALOG_ENSEMBLE_FILL")?,
                    fill.prob_slippage
                        .as_decimal()
                        .to_f64()
                        .context("CATALOG_ENSEMBLE_SLIPPAGE")?,
                    Some(fill.random_seed.get()),
                )?;
                let fill_model =
                    crate::recorded_fees::RecordedLiquidityModel::new(model.clone(), default_fill);
                liquidity = Some(fill_model.observer());
                engine.add_venue(
                    SimulatedVenueConfig::builder()
                        .venue(Venue::from("POLYMARKET"))
                        .oms_type(OmsType::Netting)
                        .account_type(AccountType::Margin)
                        .book_type(BookType::L1_MBP)
                        .base_currency(Currency::from_str("pUSD")?)
                        .starting_balances(vec![Money::from_str(&format!(
                            "{} pUSD",
                            settings.starting_capital.as_decimal().to_plain_string()
                        ))
                        .map_err(anyhow::Error::msg)?])
                        .default_leverage(Decimal::ONE)
                        .allow_cash_borrowing(false)
                        .fee_model(fee_errors.wrap(model.handle()))
                        .fill_model(FillModelHandle::new(fill_model))
                        .latency_model(LatencyModelHandle::new(StaticLatencyModel::new(
                            latency.base_latency_ns.get().into(),
                            latency.insert_latency_ns.get().into(),
                            latency.update_latency_ns.get().into(),
                            latency.cancel_latency_ns.get().into(),
                        )))
                        .bar_execution(true)
                        .trade_execution(false)
                        .liquidity_consumption(true)
                        .build()?,
                )?;
                fees = Some(model);
            }
            let fees = fees.as_ref().expect("loaded source creates fee model");
            let instruments = market
                .series
                .iter()
                .map(|series| (series.instrument.id(), series.instrument.clone()))
                .collect::<BTreeMap<_, _>>();
            let mut bars = market
                .series
                .into_iter()
                .flat_map(|series| series.bars)
                .collect::<Vec<_>>();
            bars.sort_by_key(|bar| (bar.ts_event, bar.bar_type.instrument_id()));
            let mut cursor = 0;
            while cursor < bars.len() {
                let at = bars[cursor].ts_event.as_u64();
                let end =
                    cursor + bars[cursor..].partition_point(|bar| bar.ts_event.as_u64() == at);
                if at >= request.event_start_ns.get() && at < request.event_end_ns.get() {
                    let modeled_available = at
                        .checked_add(request.modeled_lag_ns.get())
                        .context("CATALOG_ENSEMBLE_CLOCK")?;
                    if modeled_available > request.decision_cutoff_ns.get() {
                        cursor = end;
                        continue;
                    }
                    let mut available = 0;
                    for &bar in &bars[cursor..end] {
                        available = fees.with_records(
                            &bar.bar_type.instrument_id().to_string(),
                            at,
                            |records| policy.observe(bar, records),
                        )?;
                    }
                    if at >= request.evaluation_start_ns.get()
                        && available <= request.decision_cutoff_ns.get()
                    {
                        // Native execution sees an explicitly modeled delivery
                        // clock; the retained source BAR is never rewritten.
                        let observed = bars[cursor..end]
                            .iter()
                            .map(|bar| {
                                let mut delivered = *bar;
                                delivered.ts_init = available.into();
                                delivered
                            })
                            .collect::<Vec<_>>();
                        let marks = observed
                            .iter()
                            .filter(|bar| registered.contains(&bar.bar_type.instrument_id()))
                            .copied()
                            .map(Data::Bar)
                            .collect::<Vec<_>>();
                        engine.clear_data();
                        if !account_started && request.evaluation_start_ns.get() < available {
                            engine.add_data(
                                vec![wake(request.evaluation_start_ns.get())],
                                None,
                                true,
                                true,
                            )?;
                        }
                        if !marks.is_empty() {
                            engine.add_data(marks, None, true, true)?;
                        }
                        engine.add_data(vec![wake(available)], None, true, true)?;
                        let native = engine.run(None, Some(available.into()), None, true);
                        status.borrow_mut().check_callback_error()?;
                        fee_errors.check()?;
                        liquidity
                            .as_ref()
                            .expect("native fill model installed")
                            .check()?;
                        native?;
                        account_started = true;
                        ensure!(
                            status.borrow().failure.is_none(),
                            "CATALOG_ENSEMBLE_NATIVE_TARGET_FAILED"
                        );
                        let ready = {
                            let cache = engine.kernel().cache.borrow();
                            (!native_started || status.borrow().consumed == 1)
                                && cache.orders_open_count(None, None, None, None, None) == 0
                                && cache.orders_inflight_count(None, None, None, None, None) == 0
                        };
                        if ready {
                            let held = engine
                                .kernel()
                                .cache
                                .borrow()
                                .positions_open(None, None, None, None, None)
                                .iter()
                                .map(|position| position.instrument_id)
                                .collect::<BTreeSet<_>>();
                            let mut new_data = Vec::new();
                            let mut targets = Vec::new();
                            let mut definitions = Vec::new();
                            let mut allocated = Decimal::ZERO;
                            for &bar in &observed {
                                let id = bar.bar_type.instrument_id();
                                let weight = Decimal::from_f64_retain(policy.share(id))
                                    .context("CATALOG_ENSEMBLE_WEIGHT")?
                                    .checked_mul(gross)
                                    .context("CATALOG_ENSEMBLE_WEIGHT")?
                                    .round_dp_with_strategy(
                                        18,
                                        rust_decimal::RoundingStrategy::ToZero,
                                    )
                                    .min((gross - allocated).max(Decimal::ZERO));
                                if weight.is_zero() && !held.contains(&id) {
                                    continue;
                                }
                                let instrument = instruments
                                    .get(&id)
                                    .context("CATALOG_ENSEMBLE_DEFINITION")?;
                                if registered.insert(id) {
                                    engine.add_instrument(instrument)?;
                                    new_data.push(Data::Bar(bar));
                                }
                                definitions.push(instrument.clone());
                                allocated += weight;
                                targets.push(AllocationTargetV1 {
                                    instrument_id: id.to_string(),
                                    currency: "pUSD".into(),
                                    weight: weight
                                        .to_string()
                                        .parse()
                                        .map_err(anyhow::Error::msg)?,
                                });
                            }
                            fees.bind_source_bars(
                                &targets
                                    .iter()
                                    .map(|target| target.instrument_id.clone())
                                    .collect::<Vec<_>>(),
                                at,
                            )?;
                            let expires = available
                                .checked_add(request.target_ttl_ns.get())
                                .context("CATALOG_ENSEMBLE_TARGET_CLOCK")?;
                            inbox.borrow_mut().pending =
                                Some(crate::simulation::CatalogReplayFrame {
                                    instruments: definitions,
                                    bar_end_ns: at,
                                    point: NativeTargetPointV1 {
                                        schema_version: SchemaV1,
                                        asof_ns: counter(available)?,
                                        valid_until_ns: counter(expires)?,
                                        rebalance_instrument_ids: Some(
                                            targets
                                                .iter()
                                                .map(|target| target.instrument_id.clone())
                                                .collect(),
                                        ),
                                        targets,
                                        cash_weight: (Decimal::ONE - allocated)
                                            .to_string()
                                            .parse()
                                            .map_err(anyhow::Error::msg)?,
                                    },
                                });
                            // shortcut: A/B share a clock only with newly registered unheld BARs, positive latency and no venue modules; merge timestamp batches before expanding those conditions.
                            engine.clear_data();
                            if !new_data.is_empty() {
                                engine.add_data(new_data, None, true, true)?;
                            }
                            engine.add_data(vec![wake(available)], None, true, true)?;
                            let native = engine.run(None, Some(available.into()), None, true);
                            status.borrow_mut().check_callback_error()?;
                            fee_errors.check()?;
                            liquidity
                                .as_ref()
                                .expect("native fill model installed")
                                .check()?;
                            native?;
                            ensure!(
                                status.borrow().failure.is_none(),
                                "CATALOG_ENSEMBLE_NATIVE_TARGET_FAILED"
                            );
                            native_started = true;
                        }
                        last_available = Some(available);
                    }
                }
                cursor = end;
            }
        }
        complete_scope(&root, &scope, request, covered)?;
        ensure!(native_started, "CATALOG_ENSEMBLE_EVALUATION_EMPTY");
        // Only pending native commands advance beyond the final observed frame.
        // Clock events provide no extra prices, quantities or fee observations.
        let through = last_available
            .context("CATALOG_ENSEMBLE_EVALUATION_EMPTY")?
            .checked_add(settlement_ns)
            .and_then(|at| at.checked_add(1))
            .context("CATALOG_ENSEMBLE_TARGET_CLOCK")?;
        engine.clear_data();
        engine.add_data(vec![wake(through)], None, true, true)?;
        let native = engine.run(None, Some(through.into()), None, true);
        status.borrow_mut().check_callback_error()?;
        native?;
        engine.end()?;
        ensure!(
            status.borrow().failure.is_none() && status.borrow().consumed == 1,
            "CATALOG_ENSEMBLE_TARGET_NOT_SETTLED"
        );
        status.borrow_mut().check_callback_error()?;
        fee_errors.check()?;
        liquidity
            .as_ref()
            .context("CATALOG_ENSEMBLE_SOURCE_REQUIRED")?
            .check()?;
        let consumed = inbox.borrow().consumed + status.borrow().consumed;
        let simulation = crate::simulation::catalog_simulation_result(
            &engine,
            settings,
            request.evaluation_start_ns.get(),
            request.event_end_ns.get(),
            consumed,
            fees.as_ref().context("CATALOG_ENSEMBLE_SOURCE_REQUIRED")?,
        )?;
        Ok(NativeCatalogEnsembleResultV1 { schema_version: SchemaV1, source: request.source.clone(), revision: request.revision.clone(),
            catalog_series: counter(u64::try_from(scope.len())?)?,
            observed_markets: counter(u64::try_from(policy.observed_markets())?)?, bars: counter(policy.observations())?, source_records: counter(records)?,
            consumed_fuel: policy.consumed_fuel().into_iter().map(|fuel| fuel.map(counter).transpose()).collect::<Result<_>>()?, simulation,
            assumptions: vec![
                "CONDITIONAL_RESEARCH_NOT_VERIFIED_HISTORICAL_EXECUTION".into(),
                "CurrencyPair share-price proxy; one native MARGIN account, leverage 1, long-only and target gross bounded by original request; no physical cash-token inventory claim".into(),
                "Native BAR precision defines modeled tick/size units; no historical venue execution-definition claim".into(),
                "Original fill directions are provider BUY/SELL columns, not independently established aggressor roles; no L2 orderbook or spread is reconstructed".into(),
                format!("Feature availability is modeled at completed BAR event plus {}ns; no observed receive-clock or PIT qualification", request.modeled_lag_ns.get()),
                "Actual native fills consume the already completed source BAR pool bound to their target, using original same-ID quantities/fees once with the existing FIFO proportional allocator; pending orders retain that old pool across later BARs/source windows, never a future fee pool. This is conditional liquidity/cost reuse, not verified historical execution or fee eligibility".into(),
                "An execution-only BAR copy carries the disclosed modeled delivery clock; original event/OHLCV and stored native ts_init remain unchanged. Only existing native commands settle beyond the final source frame, with no additional prices, volume or fees".into(),
                "A pending native target remains active until its reductions/increases settle; later source frames still update the account and policy, and a subsequent target is issued only against the next currently observed market frame".into(),
                "All positive current scores participate in dynamic normalization; only markets with current real BARs can rebalance, and other native holdings retain their original last real marks".into(),
            ] })
    })();
    engine.dispose();
    result
}
