//! Explicit derived-cohort execution estimate; no source trades/quotes are invented.
use super::{clocked_feature, cohort_cache, Cohort};
use crate::signals::SignalModule;
use anyhow::{ensure, Context, Result};
use contracts::{economic_tick::*, DbCounter, DecimalValue, SchemaV1};
use nautilus_common::timer::{create_valid_interval, VirtualTimer};
use nautilus_core::DurationNanos;
use nautilus_indicators::{
    average::ema::ExponentialMovingAverage,
    indicator::{Indicator, MovingAverage},
};
use nautilus_model::{
    fees::{MakerTakerFeeRates, MakerTakerFeeSchedule},
    instruments::{Instrument, InstrumentAny},
    types::{Currency, Money},
};
use rust_decimal::{
    prelude::{FromPrimitive, ToPrimitive},
    Decimal,
};
use serde::Deserialize;
use std::{
    cmp::Reverse,
    collections::{BTreeMap, BTreeSet, BinaryHeap},
    fs::File,
    io::{BufRead, BufReader, Read},
    path::{Component, Path},
    str::FromStr,
};

pub(super) mod books;
mod models;
mod native;
mod source_books;
use models::EconomicModels;
use native::{NativeBook, NativeLedger, NativeMark, NativeRate};
const DAY: u64 = 86_400_000_000_000;
const HOUR: u64 = DAY / 24;

fn observed(scenario: &NativeTickEconomicScenarioV1) -> bool {
    matches!(
        scenario.execution_model,
        Some(NativeTickEconomicExecutionModelV1::ObservedL2IocV1)
            | Some(NativeTickEconomicExecutionModelV1::ObservedL2IocV2)
    )
}

fn normalize_observed_book_sizes(scenario: &NativeTickEconomicScenarioV1) -> bool {
    scenario.execution_model == Some(NativeTickEconomicExecutionModelV1::ObservedL2IocV2)
}

fn counter(v: u64) -> Result<DbCounter> {
    DbCounter::new(v).map_err(anyhow::Error::msg)
}
fn decimal(v: &DecimalValue) -> Result<Decimal> {
    Ok(Decimal::from_str(&v.as_decimal().to_plain_string())?)
}
fn exact(v: Decimal) -> Result<DecimalValue> {
    v.round_dp_with_strategy(18, rust_decimal::RoundingStrategy::MidpointNearestEven)
        .normalize()
        .to_string()
        .parse()
        .map_err(anyhow::Error::msg)
}
fn finite(v: f64) -> Result<Decimal> {
    Decimal::from_f64(v).context("ECONOMIC_NONFINITE")
}

pub fn validate(request: &NativeTickEconomicRequestV1) -> Result<()> {
    domain::ticks::evaluation_request(&request.research)?;
    ensure!(
        [
            request.research.input.event_start_ns,
            request.research.input.event_end_ns,
            request.research.train_end_ns,
            request.research.validation_end_ns,
        ]
        .into_iter()
        .all(|v| v.get().is_multiple_of(DAY)),
        "ECONOMIC_UTC_DAILY_BOUNDARIES_REQUIRED"
    );
    ensure!(
        request.research.input.is_flow_cache(),
        "ECONOMIC_FLOW_CACHE_REQUIRED"
    );
    let s = &request.scenario;
    if let Some(weights) = &request.signal_weights {
        ensure!(!weights.is_empty(), "ECONOMIC_SIGNAL_WEIGHTS_INVALID");
        let sum = weights.iter().try_fold(Decimal::ZERO, |sum, weight| {
            let weight = decimal(weight)?;
            ensure!(
                weight > Decimal::ZERO && weight <= Decimal::ONE,
                "ECONOMIC_SIGNAL_WEIGHTS_INVALID"
            );
            sum.checked_add(weight)
                .context("ECONOMIC_SIGNAL_WEIGHTS_INVALID")
        })?;
        ensure!(sum == Decimal::ONE, "ECONOMIC_SIGNAL_WEIGHTS_INVALID");
    }
    for v in [&s.initial_usdt, &s.usdt0_per_usdt] {
        ensure!(
            decimal(v)? > Decimal::ZERO,
            "ECONOMIC_POSITIVE_CAPITAL_REQUIRED"
        );
    }
    ensure!(
        decimal(&s.spot_fraction)? > Decimal::ZERO && decimal(&s.spot_fraction)? < Decimal::ONE,
        "ECONOMIC_SPLIT_INVALID"
    );
    for v in [
        &s.gross_fraction,
        &s.maximum_asset_weight,
        &s.participation_fraction,
    ] {
        ensure!(
            decimal(v)? > Decimal::ZERO && decimal(v)? <= Decimal::ONE,
            "ECONOMIC_FRACTION_INVALID"
        );
    }
    for fee in [&s.perp_taker_fee_bps, &s.spot_taker_fee_bps]
        .into_iter()
        .chain(
            s.taker_fee_bps_by_instrument
                .iter()
                .flat_map(|fees| fees.values()),
        )
    {
        ensure!(
            decimal(fee)? >= Decimal::ZERO
                && decimal(&s.extra_cost_bps)? >= Decimal::ZERO
                && decimal(fee)? + decimal(&s.extra_cost_bps)? < Decimal::from(10000),
            "ECONOMIC_COST_INVALID"
        );
    }
    if let Some(fees) = &s.taker_fee_bps_by_instrument {
        ensure!(
            fees.keys().all(|id| !id.trim().is_empty()),
            "ECONOMIC_FEE_INSTRUMENT_INVALID"
        );
    }
    ensure!(
        decimal(&s.maintenance_margin_fraction)? > Decimal::ZERO
            && decimal(&s.maintenance_margin_fraction)? < decimal(&s.initial_margin_fraction)?
            && decimal(&s.initial_margin_fraction)? <= Decimal::ONE,
        "ECONOMIC_MARGIN_INVALID"
    );
    ensure!(
        s.decision_interval_ns.get() > 0
            && s.maximum_mark_age_ns.get() > 0
            && s.jitter_min_ns <= s.jitter_max_ns,
        "ECONOMIC_CLOCK_INVALID"
    );
    s.base_latency_ns
        .get()
        .checked_add(s.jitter_max_ns.get())
        .context("ECONOMIC_CLOCK_OVERFLOW")?;
    ensure!(
        Path::new(&request.funding_source).components().count() == 1
            && Path::new(&request.funding_source)
                .components()
                .all(|p| matches!(p, Component::Normal(_))),
        "ECONOMIC_FUNDING_PATH_INVALID"
    );
    ensure!(
        request
            .context_source
            .as_deref()
            .is_none_or(|name| name == "context-prices.manifest.json"),
        "ECONOMIC_CONTEXT_PATH_INVALID"
    );
    ensure!(
        request
            .event_time_context_source
            .as_deref()
            .is_none_or(|name| name == EVENT_TIME_CONTEXT_MANIFEST),
        "ECONOMIC_CONTEXT_PATH_INVALID"
    );
    ensure!(
        request.event_time_context_source.is_none() || !request.research.input.uses_received_time(),
        "ECONOMIC_EVENT_TIME_CONTEXT_CLOCK_INVALID"
    );
    ensure!(
        request
            .spot_valuation_source
            .as_deref()
            .is_none_or(|name| name == "spot-valuation.jsonl"),
        "ECONOMIC_SPOT_VALUATION_PATH_INVALID"
    );
    if let Some(policy) = &request.small_capital_policy {
        ensure!(
            s.decision_clock.is_none()
                || policy.decision_clock.is_none()
                || s.decision_clock == policy.decision_clock,
            "ECONOMIC_DECISION_CLOCK_CONFLICT"
        );
        ensure!(
            decimal(&policy.maximum_entry_notional_usdc)? > Decimal::ZERO
                && policy.maximum_holding_ns.get() > 0,
            "ECONOMIC_SMALL_POLICY_INVALID"
        );
        request
            .research
            .input
            .event_end_ns
            .get()
            .checked_add(policy.maximum_holding_ns.get())
            .context("ECONOMIC_CLOCK_OVERFLOW")?;
    }
    ensure!(
        !request.fx_instrument_ids.is_empty()
            && request
                .fx_instrument_ids
                .iter()
                .collect::<BTreeSet<_>>()
                .len()
                == request.fx_instrument_ids.len(),
        "ECONOMIC_FX_SCOPE_INVALID"
    );
    Ok(())
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rate {
    instrument_id: String,
    event_ns: DbCounter,
    rate: DecimalValue,
}
#[derive(Deserialize)]
struct FundingManifest {
    source_revision: String,
    start_ns: DbCounter,
    end_ns: DbCounter,
    instruments: Vec<String>,
    rows: u64,
    missing_instruments: Vec<String>,
}
#[derive(Default)]
struct Funding {
    reader: Option<BufReader<File>>,
    next: Option<Rate>,
    last: Option<(u64, String)>,
    rows: u64,
    instruments: BTreeSet<String>,
}
impl Funding {
    fn open(path: &Path, instruments: BTreeSet<String>) -> Result<Self> {
        let mut s = Self {
            reader: Some(BufReader::new(File::open(path)?)),
            next: None,
            last: None,
            rows: 0,
            instruments,
        };
        s.advance()?;
        Ok(s)
    }
    fn advance(&mut self) -> Result<()> {
        let Some(reader) = self.reader.as_mut() else {
            return Ok(());
        };
        let mut line = String::new();
        let n = reader.by_ref().take(4097).read_line(&mut line)?;
        if n == 0 {
            self.next = None;
            return Ok(());
        }
        ensure!(
            n <= 4096 && line.ends_with('\n'),
            "ECONOMIC_FUNDING_ROW_INVALID"
        );
        let row: Rate = serde_json::from_str(&line)?;
        decimal(&row.rate)?;
        ensure!(
            self.instruments.contains(&row.instrument_id),
            "ECONOMIC_FUNDING_IDENTITY_INVALID"
        );
        self.rows += 1;
        let key = (row.event_ns.get(), row.instrument_id.clone());
        ensure!(
            self.last.as_ref().is_none_or(|last| last < &key),
            "ECONOMIC_FUNDING_ORDER_INVALID"
        );
        self.last = Some(key);
        self.next = Some(row);
        Ok(())
    }
}

const CONTEXT_MANIFEST: &str = "context-prices.manifest.json";
const EVENT_TIME_CONTEXT_MANIFEST: &str = "event-time-context-prices.manifest.json";
#[derive(Deserialize)]
struct ContextFile {
    instrument_id: String,
    path: String,
    bytes: DbCounter,
    rows: DbCounter,
    first_event_ns: Option<DbCounter>,
    last_event_ns: Option<DbCounter>,
}
#[derive(Deserialize)]
struct ContextManifest {
    schema_version: SchemaV1,
    format: String,
    clock: String,
    source_revision: String,
    research_source_revision: String,
    start_ns: DbCounter,
    end_ns: DbCounter,
    rows: DbCounter,
    instrument_ids: Vec<String>,
    files: Vec<ContextFile>,
}
fn context_manifest(
    root: &Path,
    request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
) -> Result<ContextManifest> {
    context_manifest_at(root, request, CONTEXT_MANIFEST, "context-prices-", false)
}
fn event_time_context_manifest(
    root: &Path,
    request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
) -> Result<ContextManifest> {
    context_manifest_at(
        root,
        request,
        EVENT_TIME_CONTEXT_MANIFEST,
        "event-time-context-prices-",
        true,
    )
}
fn context_manifest_at(
    root: &Path,
    request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
    manifest_name: &str,
    file_prefix: &str,
    event_time_only: bool,
) -> Result<ContextManifest> {
    let state = std::fs::symlink_metadata(root.join(manifest_name))?;
    ensure!(
        state.is_file() && !state.file_type().is_symlink(),
        "ECONOMIC_CONTEXT_MANIFEST_INVALID"
    );
    let m: ContextManifest =
        serde_json::from_reader(BufReader::new(File::open(root.join(manifest_name))?))?;
    let _ = m.schema_version;
    let clock_matches = if event_time_only {
        m.format == "EVENT_TIME_CONTEXT_PRICE_CSV_V1" && m.clock == "EVENT_TIME_PROXY_UNVERIFIED"
    } else {
        ((m.format == "EVENT_TIME_CONTEXT_PRICE_CSV_V1"
            && m.clock == "EVENT_TIME_PROXY_UNVERIFIED")
            || (m.format == "RECEIVED_TIME_CONTEXT_PRICE_CSV_V1"
                && m.clock == "RECEIVED_TIME_OBSERVED"))
            && (!request.input.uses_received_time() || m.clock == "RECEIVED_TIME_OBSERVED")
    };
    ensure!(
        clock_matches
            && !m.source_revision.trim().is_empty()
            && m.research_source_revision == request.input.source_revision
            && m.start_ns <= request.input.event_start_ns
            && m.end_ns >= request.input.event_end_ns
            && m.start_ns < m.end_ns
            && !m.instrument_ids.is_empty()
            && m.instrument_ids.windows(2).all(|p| p[0] < p[1])
            && m.files.len() == m.instrument_ids.len(),
        "ECONOMIC_CONTEXT_MANIFEST_INVALID"
    );
    let mut rows = 0u64;
    for (index, f) in m.files.iter().enumerate() {
        ensure!(
            f.instrument_id == m.instrument_ids[index]
                && !f.instrument_id.is_empty()
                && f.path == format!("{file_prefix}{index:03}.csv"),
            "ECONOMIC_CONTEXT_FILE_INVALID"
        );
        let state = std::fs::symlink_metadata(root.join(&f.path))?;
        ensure!(
            state.is_file()
                && !state.file_type().is_symlink()
                && state.len() == f.bytes.get()
                && state.len() >= "event_ns,oracle_px,mark_px\n".len() as u64,
            "ECONOMIC_CONTEXT_FILE_INVALID"
        );
        ensure!(
            if f.rows.get() == 0 {
                f.first_event_ns.is_none() && f.last_event_ns.is_none()
            } else {
                matches!((f.first_event_ns, f.last_event_ns), (Some(first), Some(last))
                if first <= last && first >= m.start_ns && last < m.end_ns)
            },
            "ECONOMIC_CONTEXT_RANGE_INVALID"
        );
        rows = rows
            .checked_add(f.rows.get())
            .context("ECONOMIC_CONTEXT_ROW_OVERFLOW")?;
    }
    ensure!(rows == m.rows.get(), "ECONOMIC_CONTEXT_ROW_COUNT_MISMATCH");
    Ok(m)
}
struct ContextRow {
    event: u64,
    oracle: Option<Decimal>,
    mark: Option<Decimal>,
}
struct ContextReader {
    reader: BufReader<File>,
    line: String,
    file: ContextFile,
    next: Option<ContextRow>,
    current: Option<ContextRow>,
    first: Option<u64>,
    last: Option<u64>,
    rows: u64,
}
pub fn context_decimal(text: &str) -> Result<Option<Decimal>> {
    if text.is_empty() {
        return Ok(None);
    }
    let value = if text.contains(['e', 'E']) {
        Decimal::from_scientific(text)?
    } else {
        Decimal::from_str_exact(text)?
    };
    ensure!(value > Decimal::ZERO, "ECONOMIC_CONTEXT_PRICE_INVALID");
    Ok(Some(value))
}
impl ContextReader {
    fn open(root: &Path, file: ContextFile) -> Result<Self> {
        let mut reader = BufReader::new(File::open(root.join(&file.path))?);
        let mut header = String::new();
        reader.by_ref().take(4097).read_line(&mut header)?;
        ensure!(header.ends_with('\n'), "ECONOMIC_CONTEXT_HEADER_INVALID");
        let header = header.strip_suffix('\n').unwrap();
        let header = header.strip_suffix('\r').unwrap_or(header);
        ensure!(
            matches!(
                header,
                "event_ns,oracle_px,mark_px" | "\"event_ns\",\"oracle_px\",\"mark_px\""
            ),
            "ECONOMIC_CONTEXT_HEADER_INVALID"
        );
        let mut result = Self {
            reader,
            line: String::new(),
            file,
            next: None,
            current: None,
            first: None,
            last: None,
            rows: 0,
        };
        result.advance()?;
        Ok(result)
    }
    fn advance(&mut self) -> Result<()> {
        self.line.clear();
        let n = self.reader.by_ref().take(4097).read_line(&mut self.line)?;
        if n == 0 {
            ensure!(
                self.rows == self.file.rows.get()
                    && self.first == self.file.first_event_ns.map(|v| v.get())
                    && self.last == self.file.last_event_ns.map(|v| v.get()),
                "ECONOMIC_CONTEXT_ROW_COUNT_MISMATCH"
            );
            self.next = None;
            return Ok(());
        }
        ensure!(
            n <= 4096 && self.line.ends_with('\n'),
            "ECONOMIC_CONTEXT_ROW_INVALID"
        );
        let line = self.line.strip_suffix('\n').unwrap_or(&self.line);
        let line = line.strip_suffix('\r').unwrap_or(line);
        let mut fields = line.split(',');
        let event = counter(
            fields
                .next()
                .context("ECONOMIC_CONTEXT_ROW_INVALID")?
                .parse()?,
        )?
        .get();
        let oracle = context_decimal(fields.next().context("ECONOMIC_CONTEXT_ROW_INVALID")?)?;
        let mark = context_decimal(fields.next().context("ECONOMIC_CONTEXT_ROW_INVALID")?)?;
        ensure!(fields.next().is_none(), "ECONOMIC_CONTEXT_ROW_INVALID");
        ensure!(
            self.last.is_none_or(|previous| previous < event),
            "ECONOMIC_CONTEXT_ORDER_INVALID"
        );
        let row = ContextRow {
            event,
            oracle,
            mark,
        };
        self.first.get_or_insert(event);
        self.last = Some(event);
        self.rows += 1;
        ensure!(
            self.rows <= self.file.rows.get(),
            "ECONOMIC_CONTEXT_ROW_COUNT_MISMATCH"
        );
        self.next = Some(row);
        Ok(())
    }
    fn price(&mut self, at: u64, age: u64, oracle: bool) -> Result<Option<Decimal>> {
        while self.next.as_ref().is_some_and(|row| row.event <= at) {
            self.current = self.next.take();
            self.advance()?;
        }
        Ok(self
            .current
            .as_ref()
            .filter(|row| {
                at.checked_sub(row.event)
                    .is_some_and(|elapsed| elapsed <= age)
            })
            .and_then(|row| if oracle { row.oracle } else { row.mark }))
    }
}
struct ContextPriceSource {
    source_revision: String,
    clock: String,
    readers: BTreeMap<String, ContextReader>,
}
impl ContextPriceSource {
    fn open(
        root: &Path,
        request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
        manifest_name: &str,
        file_prefix: &str,
        event_time_only: bool,
    ) -> Result<Self> {
        let manifest =
            context_manifest_at(root, request, manifest_name, file_prefix, event_time_only)?;
        let readers = manifest
            .files
            .into_iter()
            .filter(|file| request.input.instrument_ids.contains(&file.instrument_id))
            .map(|file| Ok((file.instrument_id.clone(), ContextReader::open(root, file)?)))
            .collect::<Result<_>>()?;
        Ok(Self {
            source_revision: manifest.source_revision,
            clock: manifest.clock,
            readers,
        })
    }
    fn price(&mut self, id: &str, at: u64, age: u64, oracle: bool) -> Result<Option<Decimal>> {
        self.readers
            .get_mut(id)
            .map_or(Ok(None), |reader| reader.price(at, age, oracle))
    }
    fn finish(&mut self) -> Result<()> {
        for reader in self.readers.values_mut() {
            while reader.next.is_some() {
                reader.advance()?;
            }
        }
        Ok(())
    }
}
struct ContextPrices {
    primary: Option<ContextPriceSource>,
    event_time_fallback: Option<ContextPriceSource>,
    fallback_lookups: u64,
}
impl ContextPrices {
    fn latest(&self, id: &str) -> Option<&ContextRow> {
        [self.primary.as_ref(), self.event_time_fallback.as_ref()]
            .into_iter()
            .flatten()
            .filter_map(|source| {
                source
                    .readers
                    .get(id)
                    .and_then(|reader| reader.current.as_ref())
            })
            .max_by_key(|row| row.event)
    }
    fn open(
        root: &Path,
        request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
        primary_enabled: bool,
        event_time_fallback_enabled: bool,
    ) -> Result<Self> {
        Ok(Self {
            primary: primary_enabled
                .then(|| {
                    ContextPriceSource::open(
                        root,
                        request,
                        CONTEXT_MANIFEST,
                        "context-prices-",
                        false,
                    )
                })
                .transpose()?,
            event_time_fallback: event_time_fallback_enabled
                .then(|| {
                    ContextPriceSource::open(
                        root,
                        request,
                        EVENT_TIME_CONTEXT_MANIFEST,
                        "event-time-context-prices-",
                        true,
                    )
                })
                .transpose()?,
            fallback_lookups: 0,
        })
    }
    fn price(&mut self, id: &str, at: u64, age: u64, oracle: bool) -> Result<Option<Decimal>> {
        if let Some(value) = self
            .primary
            .as_mut()
            .map(|source| source.price(id, at, age, oracle))
            .transpose()?
            .flatten()
        {
            return Ok(Some(value));
        }
        let value = self
            .event_time_fallback
            .as_mut()
            .map(|source| source.price(id, at, age, oracle))
            .transpose()?
            .flatten();
        if value.is_some() {
            self.fallback_lookups += 1;
        }
        Ok(value)
    }
    fn finish(&mut self) -> Result<()> {
        if let Some(source) = &mut self.primary {
            source.finish()?;
        }
        if let Some(source) = &mut self.event_time_fallback {
            source.finish()?;
        }
        Ok(())
    }
    fn description(&self) -> String {
        match (&self.primary, &self.event_time_fallback) {
            (Some(primary), Some(fallback)) => format!(
                "primary source {} ({}) with event-time proxy fallback {} ({} successful lookups)",
                primary.source_revision,
                primary.clock,
                fallback.source_revision,
                self.fallback_lookups
            ),
            (Some(primary), None) => {
                format!("source {} ({})", primary.source_revision, primary.clock)
            }
            (None, Some(fallback)) => format!(
                "event-time proxy source {} ({}, {} successful lookups)",
                fallback.source_revision, fallback.clock, self.fallback_lookups
            ),
            (None, None) => "no context source".into(),
        }
    }
}

fn expected_until(
    limit: u64,
    next_hour: &mut u64,
    ledger: &Option<NativeLedger>,
    expected: &mut BTreeSet<(String, u64)>,
) -> Result<()> {
    while *next_hour < limit {
        if let Some(ledger) = ledger {
            for position in ledger.positions() {
                expected.insert((position.instrument_id.to_string(), *next_hour / HOUR));
            }
        }
        *next_hour = next_hour
            .checked_add(HOUR)
            .context("ECONOMIC_CLOCK_OVERFLOW")?;
    }
    Ok(())
}
#[allow(clippy::too_many_arguments)]
fn rates_at(
    funding: &mut Funding,
    ledger: &mut Option<NativeLedger>,
    assets: &[Asset],
    at: u64,
    start: u64,
    age: u64,
    unknown: &mut Option<(u64, String)>,
    seen: &mut BTreeSet<(String, u64)>,
    contexts: &mut Option<ContextPrices>,
) -> Result<Vec<NativeRate>> {
    let mut native = Vec::new();
    while funding
        .next
        .as_ref()
        .is_some_and(|r| r.event_ns.get() == at)
    {
        let r = funding.next.take().unwrap();
        if at >= start {
            if let Some(asset) = assets
                .iter()
                .find(|a| a.instrument.id().to_string() == r.instrument_id)
            {
                ensure!(
                    matches!(asset.instrument, InstrumentAny::CryptoPerpetual(_)),
                    "ECONOMIC_FUNDING_NONPERP"
                );
                seen.insert((r.instrument_id.clone(), at / HOUR));
                if let Some(ledger) = ledger.as_mut() {
                    for p in ledger
                        .positions()
                        .into_iter()
                        .filter(|p| p.instrument_id == asset.instrument.id())
                    {
                        let price = match contexts.as_mut() {
                            Some(context) => context.price(&r.instrument_id, at, age, true)?,
                            None => mark(asset, at, age),
                        };
                        if let Some(price) = price {
                            // Native funding covers standard mark/settlement
                            // terms. Preserve the venue's distinct oracle and
                            // nominal quote-unit semantics in one thin extension.
                            if contexts.is_some() {
                                let side = if p.side == nautilus_model::enums::PositionSide::Short {
                                    Decimal::ONE
                                } else {
                                    -Decimal::ONE
                                };
                                // Keep oracle precision through settlement in
                                // the venue currency; native mark funding has
                                // no oracle-price selector.
                                let rate = decimal(&r.rate)?;
                                let amount = p
                                    .quantity
                                    .as_decimal()
                                    .checked_mul(price)
                                    .and_then(|value| value.checked_mul(p.multiplier.as_decimal()))
                                    .and_then(|value| value.checked_mul(side))
                                    .and_then(|value| value.checked_mul(rate))
                                    .context("TICK_ECONOMIC_ORACLE_FUNDING_RANGE")?;
                                ledger.queue_adjustment(
                                    at,
                                    Money::from_decimal(
                                        amount,
                                        asset.instrument.settlement_currency(),
                                    )?,
                                )?;
                            }
                        } else {
                            let cause = if let Some(context) = contexts.as_ref() {
                                let last = context
                                    .latest(&r.instrument_id)
                                    .map(|row| row.event.to_string())
                                    .unwrap_or_else(|| "NONE".into());
                                format!("FUNDING_FIRST_CAUSE:STALE_OR_MISSING_ORACLE:event_ns={at}:instrument_id={}:last_context_ns={last}", r.instrument_id)
                            } else {
                                format!("FUNDING_FIRST_CAUSE:STALE_OR_MISSING_MARK:event_ns={at}:instrument_id={}:last_mark_ns={}", r.instrument_id, asset.mark.map(|(at, _)| at.to_string()).unwrap_or_else(|| "NONE".into()))
                            };
                            retain_first_funding_cause(unknown, (at, cause));
                        }
                    }
                }
                native.push(NativeRate {
                    instrument_id: asset.instrument.id(),
                    rate: decimal(&r.rate)?,
                    settlement_mark: if contexts.is_none() {
                        mark(asset, at, age).and_then(|value| {
                            asset.mark.map(|(source_event, _)| (source_event, value))
                        })
                    } else {
                        None
                    },
                });
            }
        }
        funding.advance()?;
    }
    Ok(native)
}
#[allow(clippy::too_many_arguments)]
fn funding_before(
    limit: u64,
    funding: &mut Funding,
    ledger: &mut Option<NativeLedger>,
    assets: &[Asset],
    start: u64,
    age: u64,
    unknown: &mut Option<(u64, String)>,
    seen: &mut BTreeSet<(String, u64)>,
    contexts: &mut Option<ContextPrices>,
) -> Result<()> {
    while let Some(at) = funding
        .next
        .as_ref()
        .map(|r| r.event_ns.get())
        .filter(|at| *at < limit)
    {
        let rows = rates_at(
            funding, ledger, assets, at, start, age, unknown, seen, contexts,
        )?;
        if let Some(ledger) = ledger.as_mut() {
            if !rows.is_empty() {
                ledger.funding_batch(at, &rows)?;
            }
        }
    }
    Ok(())
}
fn retain_first_funding_cause(first: &mut Option<(u64, String)>, cause: (u64, String)) {
    if first.as_ref().is_none_or(|(at, _)| cause.0 < *at) {
        *first = Some(cause);
    }
}

fn missing_rates_before(
    end: u64,
    expected: &mut BTreeSet<(String, u64)>,
    seen: &mut BTreeSet<(String, u64)>,
) -> Option<(u64, String)> {
    let limit = end / HOUR;
    let missing = expected
        .iter()
        .filter(|(id, hour)| *hour < limit && !seen.contains(&(id.clone(), *hour)))
        .min_by_key(|(id, hour)| (*hour, id))
        .map(|(id, hour)| (*hour * HOUR, format!("FUNDING_FIRST_CAUSE:MISSING_RATE:event_ns={}:instrument_id={id}:rate_hour={hour}", *hour * HOUR)));
    expected.retain(|(_, hour)| *hour >= limit);
    seen.retain(|(_, hour)| *hour >= limit);
    missing
}

struct Asset {
    instrument: InstrumentAny,
    reader: cohort_cache::CacheReader,
    head: Option<Cohort>,
    previous: Option<Cohort>,
    fast: ExponentialMovingAverage,
    slow: ExponentialMovingAverage,
    model: Option<EconomicModels>,
    feature_books: Option<books::FeatureBooks>,
    score: Option<f64>,
    mark: Option<(u64, Decimal)>,
    cohorts: u64,
    decisions: u64,
    attempts: u64,
    pending: Option<(u64, u64, Decimal)>,
    target_weight: Option<(u64, Decimal)>,
}
#[derive(Clone, Debug)]
struct SmallCycle {
    decision: u64,
    deadline: u64,
    reference: Decimal,
    direction: Decimal,
    reservation_usdc: Decimal,
    reservation_quote: Decimal,
    first_fill: Option<u64>,
    exit_decision: Option<u64>,
    quantity: Decimal,
}
struct SmallCapital {
    cycles: Vec<Option<SmallCycle>>,
    buckets: Vec<Option<u64>>,
    deadlines: BinaryHeap<Reverse<(u64, usize)>>,
    entry_attempts: u64,
    unfilled_entries: u64,
    positions_opened: u64,
    positions_closed: u64,
    exit_attempts: u64,
    maximum_exit_delay: Option<u64>,
    decision_clock_ticks: Option<u64>,
}
impl SmallCapital {
    fn new(count: usize) -> Self {
        Self {
            cycles: vec![None; count],
            buckets: vec![None; count],
            deadlines: BinaryHeap::new(),
            entry_attempts: 0,
            unfilled_entries: 0,
            positions_opened: 0,
            positions_closed: 0,
            exit_attempts: 0,
            maximum_exit_delay: None,
            decision_clock_ticks: None,
        }
    }
    fn exit(
        &mut self,
        i: usize,
        at: u64,
        assets: &mut [Asset],
        s: &NativeTickEconomicScenarioV1,
        seed: &mut u64,
    ) -> Result<()> {
        let Some(cycle) = self.cycles[i].as_mut() else {
            return Ok(());
        };
        if cycle.exit_decision.is_some() {
            return Ok(());
        }
        if cycle.first_fill.is_none() {
            assets[i].pending = None;
            self.cycles[i] = None;
            self.unfilled_entries += 1;
        } else {
            cycle.exit_decision = Some(at);
            small_queue(&mut assets[i], at, at, Decimal::ZERO, s, seed)?;
        }
        Ok(())
    }
    fn expire(
        &mut self,
        through: u64,
        assets: &mut [Asset],
        s: &NativeTickEconomicScenarioV1,
        seed: &mut u64,
    ) -> Result<()> {
        while self
            .deadlines
            .peek()
            .is_some_and(|Reverse((at, _))| *at <= through)
        {
            let Reverse((at, i)) = self.deadlines.pop().unwrap();
            if self.cycles[i].as_ref().is_some_and(|c| c.deadline == at) {
                self.exit(i, at, assets, s, seed)?;
            }
        }
        Ok(())
    }
    fn observe_reference(
        &mut self,
        i: usize,
        at: u64,
        price: Decimal,
        assets: &mut [Asset],
        s: &NativeTickEconomicScenarioV1,
        seed: &mut u64,
    ) -> Result<()> {
        if self.cycles[i]
            .as_ref()
            .is_some_and(|c| (price - c.reference) * c.direction >= Decimal::ZERO)
        {
            self.exit(i, at, assets, s, seed)?;
        }
        Ok(())
    }
    fn reconcile(
        &mut self,
        i: usize,
        at: u64,
        quantity: Decimal,
        attempted: bool,
        assets: &mut [Asset],
        s: &NativeTickEconomicScenarioV1,
        seed: &mut u64,
    ) -> Result<()> {
        let Some(cycle) = self.cycles[i].as_mut() else {
            return Ok(());
        };
        if attempted {
            if cycle.exit_decision.is_some() {
                self.exit_attempts += 1
            } else {
                self.entry_attempts += 1
            }
        }
        cycle.quantity = quantity;
        if cycle.first_fill.is_none() {
            if !quantity.is_zero() {
                cycle.first_fill = Some(at);
                self.positions_opened += 1;
            } else if attempted {
                self.unfilled_entries += 1;
                self.cycles[i] = None;
            }
        } else if quantity.is_zero() {
            if let Some(decision) = cycle.exit_decision {
                let elapsed = at
                    .checked_sub(decision)
                    .context("ECONOMIC_CLOCK_OVERFLOW")?;
                self.maximum_exit_delay = Some(
                    self.maximum_exit_delay
                        .map_or(elapsed, |old| old.max(elapsed)),
                );
            }
            self.positions_closed += 1;
            self.cycles[i] = None;
        } else if let Some(decision) = cycle.exit_decision {
            if assets[i].pending.is_none() {
                small_queue(&mut assets[i], decision, at, Decimal::ZERO, s, seed)?;
            }
        }
        Ok(())
    }
    fn report(&self, assets: &[Asset]) -> Result<NativeTickSmallCapitalDiagnosticsV1> {
        Ok(NativeTickSmallCapitalDiagnosticsV1 {
            decision_clock_ticks: self.decision_clock_ticks.map(counter).transpose()?,
            entry_attempts: counter(self.entry_attempts)?,
            unfilled_entries: counter(self.unfilled_entries)?,
            positions_opened: counter(self.positions_opened)?,
            positions_closed: counter(self.positions_closed)?,
            exit_attempts: counter(self.exit_attempts)?,
            maximum_exit_delay_ns: self.maximum_exit_delay.map(counter).transpose()?,
            remaining_positions: self
                .cycles
                .iter()
                .enumerate()
                .filter_map(|(i, cycle)| {
                    cycle
                        .as_ref()
                        .filter(|c| !c.quantity.is_zero())
                        .map(|c| (i, c))
                })
                .map(|(i, c)| {
                    Ok(NativeTickSmallCapitalPositionV1 {
                        instrument_id: assets[i].instrument.id().to_string(),
                        quantity: exact(c.quantity)?,
                        entry_decision_ns: counter(c.decision)?,
                        first_fill_ns: counter(
                            c.first_fill.context("ECONOMIC_SMALL_FILL_MISSING")?,
                        )?,
                        exit_decision_ns: c.exit_decision.map(counter).transpose()?,
                    })
                })
                .collect::<Result<_>>()?,
        })
    }
}
fn small_queue(
    a: &mut Asset,
    decision: u64,
    now: u64,
    quantity: Decimal,
    s: &NativeTickEconomicScenarioV1,
    seed: &mut u64,
) -> Result<()> {
    let due = now
        .checked_add(delay(seed, s)?)
        .and_then(|v| v.checked_add(1))
        .context("ECONOMIC_CLOCK_OVERFLOW")?;
    a.pending = Some((decision, due, quantity));
    a.target_weight = None;
    a.decisions += 1;
    Ok(())
}
fn small_quantity(a: &Asset, cash: &NativeLedger, margin: Option<&NativeLedger>) -> Decimal {
    if matches!(a.instrument, InstrumentAny::CurrencyPair(_)) {
        a.instrument
            .base_currency()
            .and_then(|currency| cash.balance(currency))
            .map_or(Decimal::ZERO, |b| b.as_decimal())
    } else {
        margin.map_or(Decimal::ZERO, |ledger| {
            ledger.current_quantity(a.instrument.id())
        })
    }
}
fn small_legal_quantity(
    instrument: &InstrumentAny,
    quantity: Decimal,
    price: Decimal,
    reduce_only: bool,
) -> Result<Decimal> {
    let direction = if quantity < Decimal::ZERO {
        -Decimal::ONE
    } else {
        Decimal::ONE
    };
    let step = instrument.size_increment().as_decimal();
    let amount = (quantity.abs() / step).trunc() * step;
    if amount.is_zero()
        || instrument
            .min_quantity()
            .is_some_and(|m| amount < m.as_decimal())
        || !(reduce_only && matches!(instrument, InstrumentAny::CryptoPerpetual(_)))
            && instrument.min_notional().is_some_and(|m| {
                amount * price * instrument.multiplier().as_decimal() < m.as_decimal()
            })
    {
        return Ok(Decimal::ZERO);
    }
    // Let the official native precision boundary reject unsupported quantities.
    Ok(instrument
        .try_make_qty_from_decimal(amount, Some(true))?
        .as_decimal()
        * direction)
}
fn small_unit(
    a: &Asset,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
) -> Option<Decimal> {
    let quote = if matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)) {
        Decimal::ONE
    } else {
        fx(a.instrument.quote_currency(), assets, fx_ids, now, age)?
    };
    Some(
        a.instrument
            .try_make_price(mark(a, now, age)?.to_f64()?)
            .ok()?
            .as_decimal()
            * quote
            * a.instrument.multiplier().as_decimal(),
    )
}
fn small_position_price(
    a: &Asset,
    now: u64,
    age: u64,
    contexts: &mut Option<ContextPrices>,
) -> Result<Option<Decimal>> {
    match contexts.as_mut() {
        Some(context) => context.price(&a.instrument.id().to_string(), now, age, false),
        None => Ok(mark(a, now, age)),
    }
}
// A missing mark in the other segregated sleeve cannot consume this sleeve's budget.
fn small_exposure(
    small: &SmallCapital,
    assets: &[Asset],
    cash: &NativeLedger,
    margin: Option<&NativeLedger>,
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
    is_margin: bool,
    contexts: &mut Option<ContextPrices>,
) -> Result<Option<Vec<Decimal>>> {
    let mut exposure = vec![Decimal::ZERO; assets.len()];
    if !is_margin {
        for balance in cash
            .balances()
            .into_iter()
            .filter(|b| b.currency != Currency::USDC() && !b.as_decimal().is_zero())
        {
            let Some(i) = assets
                .iter()
                .enumerate()
                .find(|(i, a)| {
                    matches!(a.instrument, InstrumentAny::CurrencyPair(_))
                        && a.instrument.base_currency() == Some(balance.currency)
                        && (small.cycles[*i].is_some()
                            || fx_ids.contains(&a.instrument.id().to_string()))
                })
                .map(|(i, _)| i)
            else {
                return Ok(None);
            };
            let Some(rate) = fx(balance.currency, assets, fx_ids, now, age) else {
                return Ok(None);
            };
            exposure[i] += balance.as_decimal().abs() * rate;
        }
    } else if let Some(margin) = margin {
        for position in margin.positions() {
            let Some(i) = assets
                .iter()
                .position(|a| a.instrument.id() == position.instrument_id)
            else {
                return Ok(None);
            };
            let price = small_position_price(&assets[i], now, age, contexts)?;
            let Some(price) = price else { return Ok(None) };
            exposure[i] += position.quantity.as_decimal()
                * price
                * assets[i].instrument.multiplier().as_decimal();
        }
    }
    for (i, a) in assets
        .iter()
        .enumerate()
        .filter(|(_, a)| matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)) == is_margin)
    {
        if let Some(cycle) = small.cycles[i].as_ref().filter(|c| c.first_fill.is_none()) {
            exposure[i] += cycle.reservation_usdc;
        } else if fx_ids.contains(&a.instrument.id().to_string()) {
            if let Some((_, _, q)) = a.pending {
                let Some(unit) = small_unit(a, assets, fx_ids, now, age) else {
                    return Ok(None);
                };
                exposure[i] = exposure[i].max(q.abs() * unit);
            }
        }
    }
    Ok(Some(exposure))
}
fn small_quote_available(
    small: &SmallCapital,
    assets: &[Asset],
    cash: &NativeLedger,
    currency: Currency,
    now: u64,
    age: u64,
) -> Option<Decimal> {
    let mut available = cash
        .balance(currency)
        .map_or(Decimal::ZERO, |b| b.as_decimal());
    for (i, a) in assets.iter().enumerate() {
        if matches!(a.instrument, InstrumentAny::CurrencyPair(_))
            && a.instrument.quote_currency() == currency
        {
            if let Some(cycle) = small.cycles[i].as_ref().filter(|c| c.first_fill.is_none()) {
                available -= cycle.reservation_quote;
            } else if let Some((_, _, q)) = a.pending {
                let held = a
                    .instrument
                    .base_currency()
                    .and_then(|c| cash.balance(c))
                    .map_or(Decimal::ZERO, |b| b.as_decimal());
                available -= (q - held).max(Decimal::ZERO)
                    * mark(a, now, age)?
                    * (Decimal::ONE + cash.cost_fraction(a.instrument.id()));
            }
        }
    }
    // Preserve deficits: execution adds back only its own reservation, not everybody else's.
    Some(available)
}

fn small_new_entries(
    small: &mut SmallCapital,
    fresh: &[usize],
    assets: &mut [Asset],
    cash: &NativeLedger,
    margin: Option<&NativeLedger>,
    fx_ids: &BTreeSet<String>,
    event: u64,
    policy: &NativeTickSmallCapitalPolicyV1,
    s: &NativeTickEconomicScenarioV1,
    seed: &mut u64,
    contexts: &mut Option<ContextPrices>,
) -> Result<()> {
    let age = s.maximum_mark_age_ns.get();
    let bucket = event / s.decision_interval_ns.get();
    for &i in fresh {
        if small.buckets[i] == Some(bucket) {
            continue;
        }
        small.buckets[i] = Some(bucket);
        let a = &assets[i];
        let is_margin = matches!(a.instrument, InstrumentAny::CryptoPerpetual(_));
        // Wallet quote reserves are a distinct role, never an alpha position.
        let reserve_base = a.instrument.base_currency().is_some_and(|currency| {
            assets.iter().any(|b| {
                fx_ids.contains(&b.instrument.id().to_string())
                    && b.instrument.base_currency() == Some(currency)
            })
        });
        if small.cycles[i].is_some()
            || a.pending.is_some()
            || fx_ids.contains(&a.instrument.id().to_string())
            || (!is_margin && reserve_base)
        {
            continue;
        }
        let Some(score) = a.score.filter(|v| *v != 0.0 && (is_margin || *v > 0.0)) else {
            continue;
        };
        if !is_margin
            && small.cycles.iter().enumerate().any(|(j, c)| {
                j != i
                    && c.is_some()
                    && matches!(assets[j].instrument, InstrumentAny::CurrencyPair(_))
                    && assets[j].instrument.base_currency() == a.instrument.base_currency()
            })
        {
            continue;
        }
        if !small_quantity(a, cash, margin).is_zero() {
            continue;
        }
        let Some(price) = mark(a, event, age) else {
            continue;
        };
        let reference = finite(a.fast.value())?;
        let fee = if is_margin {
            margin.context("TICK_ECONOMIC_LEDGER_INPUT")?
        } else {
            cash
        }
        .cost_fraction(a.instrument.id());
        if reference <= Decimal::ZERO
            || (price - reference).abs() / reference <= Decimal::from(2) * fee
        {
            continue;
        }
        let direction = if score > 0.0 {
            Decimal::ONE
        } else {
            -Decimal::ONE
        };
        if (price - reference) * direction >= Decimal::ZERO {
            continue;
        }
        // An unknown sleeve prevents adding risk, but exits never enter this branch.
        let Some(capital) = value(
            if is_margin {
                margin.context("TICK_ECONOMIC_LEDGER_INPUT")?
            } else {
                cash
            },
            assets,
            fx_ids,
            event,
            age,
            is_margin,
            contexts,
            None,
        )?
        else {
            continue;
        };
        if capital <= Decimal::ZERO {
            continue;
        }
        let Some(exposure) = small_exposure(
            small, assets, cash, margin, fx_ids, event, age, is_margin, contexts,
        )?
        else {
            continue;
        };
        let gross: Decimal = exposure
            .iter()
            .enumerate()
            .filter(|(j, _)| {
                matches!(assets[*j].instrument, InstrumentAny::CryptoPerpetual(_)) == is_margin
            })
            .map(|(_, v)| *v)
            .sum();
        let notional = decimal(&policy.maximum_entry_notional_usdc)?
            .min((capital * decimal(&s.maximum_asset_weight)? - exposure[i]).max(Decimal::ZERO))
            .min((capital * decimal(&s.gross_fraction)? - gross).max(Decimal::ZERO));
        if notional <= Decimal::ZERO {
            continue;
        }
        let Some(unit) = small_unit(a, assets, fx_ids, event, age).filter(|u| *u > Decimal::ZERO)
        else {
            continue;
        };
        let native_price = a
            .instrument
            .try_make_price(price.to_f64().context("ECONOMIC_NONFINITE")?)?
            .as_decimal();
        let mut quantity = small_legal_quantity(
            &a.instrument,
            notional / unit * direction,
            native_price,
            false,
        )?;
        if quantity.is_zero() {
            continue;
        }
        let quote_rate = if is_margin {
            Some(Decimal::ONE)
        } else {
            fx(a.instrument.quote_currency(), assets, fx_ids, event, age)
        };
        let Some(quote_rate) = quote_rate else {
            continue;
        };
        let mut needed = quantity.abs()
            * native_price
            * a.instrument.multiplier().as_decimal()
            * (Decimal::ONE + fee);
        if !is_margin {
            let currency = a.instrument.quote_currency();
            let Some(available) = small_quote_available(small, assets, cash, currency, event, age)
            else {
                continue;
            };
            let available = available.max(Decimal::ZERO);
            if available < needed {
                let affordable = small_legal_quantity(
                    &a.instrument,
                    available
                        / (native_price
                            * a.instrument.multiplier().as_decimal()
                            * (Decimal::ONE + fee)),
                    native_price,
                    false,
                )?;
                if affordable > Decimal::ZERO {
                    quantity = quantity.min(affordable);
                    needed = quantity
                        * native_price
                        * a.instrument.multiplier().as_decimal()
                        * (Decimal::ONE + fee);
                } else {
                    // Buy only actual missing quote inventory; the alpha must wait for a later fresh signal.
                    if currency != Currency::USDC() {
                        let bridge = assets.iter().position(|b| {
                            fx_ids.contains(&b.instrument.id().to_string())
                                && b.instrument.base_currency() == Some(currency)
                                && b.instrument.quote_currency() == Currency::USDC()
                        });
                        if let Some(j) = bridge.filter(|j| assets[*j].pending.is_none()) {
                            let Some(bridge_price) = mark(&assets[j], event, age) else {
                                continue;
                            };
                            let Some(bridge_unit) =
                                small_unit(&assets[j], assets, fx_ids, event, age)
                            else {
                                continue;
                            };
                            let Some(usdc) = small_quote_available(
                                small,
                                assets,
                                cash,
                                Currency::USDC(),
                                event,
                                age,
                            ) else {
                                continue;
                            };
                            let allowed = (capital * decimal(&s.maximum_asset_weight)?
                                - exposure[j])
                                .max(Decimal::ZERO)
                                .min(
                                    (capital * decimal(&s.gross_fraction)? - gross)
                                        .max(Decimal::ZERO),
                                )
                                .min(
                                    usdc.max(Decimal::ZERO)
                                        / (Decimal::ONE
                                            + cash.cost_fraction(assets[j].instrument.id())),
                                );
                            let additional = ((needed - available) * quote_rate).min(allowed);
                            let buy = small_legal_quantity(
                                &assets[j].instrument,
                                additional / bridge_unit,
                                bridge_price,
                                false,
                            )?;
                            if buy > Decimal::ZERO {
                                let held = cash
                                    .balance(currency)
                                    .map_or(Decimal::ZERO, |b| b.as_decimal());
                                small_queue(&mut assets[j], event, event, held + buy, s, seed)?;
                            }
                        }
                    }
                    continue;
                }
            }
        }
        let deadline = event
            .checked_add(policy.maximum_holding_ns.get())
            .context("ECONOMIC_CLOCK_OVERFLOW")?;
        small.cycles[i] = Some(SmallCycle {
            decision: event,
            deadline,
            reference,
            direction,
            reservation_usdc: quantity.abs() * unit,
            reservation_quote: needed,
            first_fill: None,
            exit_decision: None,
            quantity: Decimal::ZERO,
        });
        small.deadlines.push(Reverse((deadline, i)));
        small_queue(&mut assets[i], event, event, quantity, s, seed)?;
    }
    Ok(())
}
// Price moves may shrink an entry; they never increase its reserved USDC notional.
fn small_cap_entries(
    small: &SmallCapital,
    rows: &mut [NativeMark],
    assets: &[Asset],
    cash: &NativeLedger,
    margin: Option<&NativeLedger>,
    fx_ids: &BTreeSet<String>,
    event: u64,
    s: &NativeTickEconomicScenarioV1,
    contexts: &mut Option<ContextPrices>,
    observed_books: Option<&BTreeMap<usize, books::Snapshot>>,
) -> Result<()> {
    let age = s.maximum_mark_age_ns.get();
    for row in rows {
        let Some(target) = row.target_quantity.filter(|q| !q.is_zero()) else {
            continue;
        };
        let i = assets
            .iter()
            .position(|a| a.instrument.id() == row.instrument_id)
            .context("ECONOMIC_NATIVE_DEFINITION_REQUIRED")?;
        let a = &assets[i];
        let is_margin = matches!(a.instrument, InstrumentAny::CryptoPerpetual(_));
        let ledger = if is_margin {
            margin.context("TICK_ECONOMIC_LEDGER_INPUT")?
        } else {
            cash
        };
        let current = ledger.current_quantity(a.instrument.id());
        if !current.is_zero()
            && target.is_sign_negative() == current.is_sign_negative()
            && target.abs() <= current.abs()
        {
            continue;
        }
        let capital = value(
            ledger, assets, fx_ids, event, age, is_margin, contexts, None,
        )?;
        let exposure = small_exposure(
            small, assets, cash, margin, fx_ids, event, age, is_margin, contexts,
        )?;
        let native_price = a.instrument.try_make_price(row.mark)?.as_decimal();
        let unit = small_unit(a, assets, fx_ids, event, age);
        let Some((capital, exposure, unit)) = capital
            .zip(exposure)
            .zip(unit)
            .map(|((c, e), u)| (c, e, u))
            .filter(|(c, _, u)| *c > Decimal::ZERO && *u > Decimal::ZERO)
        else {
            row.target_quantity = None;
            continue;
        };
        let other_gross: Decimal = exposure
            .iter()
            .enumerate()
            .filter(|(j, _)| {
                *j != i
                    && matches!(assets[*j].instrument, InstrumentAny::CryptoPerpetual(_))
                        == is_margin
            })
            .map(|(_, v)| *v)
            .sum();
        let own_limit = small.cycles[i]
            .as_ref()
            .map_or(exposure[i], |c| c.reservation_usdc);
        let allowed = own_limit
            .min(capital * decimal(&s.maximum_asset_weight)?)
            .min((capital * decimal(&s.gross_fraction)? - other_gross).max(Decimal::ZERO));
        let held = if fx_ids.contains(&a.instrument.id().to_string()) {
            a.instrument
                .base_currency()
                .and_then(|currency| cash.balance(currency))
                .map_or(Decimal::ZERO, |b| b.as_decimal())
        } else {
            Decimal::ZERO
        };
        let mut capped = target.abs().min(allowed / unit);
        let mut quote_budget = None;
        if !is_margin {
            let fee = cash.cost_fraction(a.instrument.id());
            let quote_unit =
                native_price * a.instrument.multiplier().as_decimal() * (Decimal::ONE + fee);
            let owned = small.cycles[i]
                .as_ref()
                .map_or((target - held).max(Decimal::ZERO) * quote_unit, |c| {
                    c.reservation_quote
                });
            let available = small_quote_available(
                small,
                assets,
                cash,
                a.instrument.quote_currency(),
                event,
                age,
            );
            let Some(available) = available else {
                row.target_quantity = None;
                continue;
            };
            let budget = (available + owned).min(owned).max(Decimal::ZERO);
            capped = capped.min(held + budget / quote_unit);
            quote_budget = Some((budget, fee));
        }
        let mut quantity = small_legal_quantity(
            &a.instrument,
            capped
                * if target < Decimal::ZERO {
                    -Decimal::ONE
                } else {
                    Decimal::ONE
                },
            native_price,
            false,
        )?;
        if let Some(snapshots) = observed_books {
            let difference = quantity - current;
            let closing = if !current.is_zero()
                && !difference.is_zero()
                && current.is_sign_negative() != difference.is_sign_negative()
            {
                current.abs().min(difference.abs())
            } else {
                Decimal::ZERO
            };
            if difference.abs() > closing {
                let snapshot = snapshots
                    .get(&i)
                    .context("ECONOMIC_BOOK_SNAPSHOT_REQUIRED")?;
                let budget = if closing > Decimal::ZERO {
                    allowed
                } else {
                    (allowed - current.abs() * unit).max(Decimal::ZERO)
                };
                let quote_fx = unit / (native_price * a.instrument.multiplier().as_decimal());
                let buy = difference > Decimal::ZERO;
                let mut bounded = native::affordable_depth_quantity(
                    &a.instrument,
                    snapshot.levels(buy),
                    difference.abs(),
                    closing,
                    budget / quote_fx,
                    Decimal::ZERO,
                )?;
                if let Some((budget, fee)) = quote_budget.filter(|_| buy) {
                    bounded = native::affordable_depth_quantity(
                        &a.instrument,
                        snapshot.levels(true),
                        bounded,
                        closing,
                        budget,
                        fee,
                    )?;
                }
                let change = small_legal_quantity(
                    &a.instrument,
                    bounded * if buy { Decimal::ONE } else { -Decimal::ONE },
                    snapshot
                        .price(buy)
                        .context("ECONOMIC_BOOK_EXECUTION_PRICE_REQUIRED")?,
                    false,
                )?;
                if change.is_zero() {
                    row.target_quantity = None;
                    continue;
                }
                quantity = current + change;
            }
        }
        // Funding inventory may only grow here; a tighter cap cannot sell an existing reserve.
        row.target_quantity = if fx_ids.contains(&a.instrument.id().to_string()) && quantity < held
        {
            None
        } else {
            Some(quantity)
        };
    }
    Ok(())
}

fn delay(seed: &mut u64, s: &NativeTickEconomicScenarioV1) -> Result<u64> {
    *seed = seed
        .wrapping_mul(6364136223846793005)
        .wrapping_add(1442695040888963407);
    let width = s.jitter_max_ns.get() - s.jitter_min_ns.get();
    let jitter = if width == u64::MAX {
        *seed
    } else {
        *seed % (width + 1)
    };
    s.base_latency_ns
        .get()
        .checked_add(s.jitter_min_ns.get())
        .and_then(|v| v.checked_add(jitter))
        .context("ECONOMIC_CLOCK_OVERFLOW")
}

fn mark(asset: &Asset, now: u64, age: u64) -> Option<Decimal> {
    asset
        .mark
        .filter(|(at, _)| now.checked_sub(*at).is_some_and(|v| v <= age))
        .map(|(_, p)| p)
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SpotQuoteRow {
    instrument_id: String,
    received_ns: DbCounter,
    bid: Option<DecimalValue>,
    ask: Option<DecimalValue>,
}

struct SpotQuotes(BTreeMap<String, Vec<(u64, Option<Decimal>)>>);
impl SpotQuotes {
    fn open(path: &Path, assets: &[Asset], start: u64, end: u64) -> Result<Self> {
        let ids: BTreeSet<String> = assets
            .iter()
            .filter(|a| matches!(a.instrument, InstrumentAny::CurrencyPair(_)))
            .map(|a| a.instrument.id().to_string())
            .collect();
        let mut rows: BTreeMap<String, Vec<(u64, Option<Decimal>)>> = BTreeMap::new();
        let mut last_seen = BTreeMap::<String, u64>::new();
        for line in BufReader::new(File::open(path)?).lines() {
            let row: SpotQuoteRow = serde_json::from_str(&line?)?;
            ensure!(
                ids.contains(&row.instrument_id),
                "ECONOMIC_SPOT_QUOTE_INSTRUMENT_INVALID"
            );
            let at = row.received_ns.get();
            ensure!(
                last_seen
                    .get(&row.instrument_id)
                    .is_none_or(|prev| *prev <= at),
                "ECONOMIC_SPOT_QUOTE_ORDER_INVALID"
            );
            last_seen.insert(row.instrument_id.clone(), at);
            if at >= end {
                continue;
            }
            let series = rows.entry(row.instrument_id).or_default();
            let price = match (row.bid.as_ref(), row.ask.as_ref()) {
                (Some(bid), Some(ask)) => {
                    let bid = decimal(bid)?;
                    let ask = decimal(ask)?;
                    (bid > Decimal::ZERO && ask >= bid).then_some((bid + ask) / Decimal::TWO)
                }
                _ => None,
            };
            if let Some((prev, value)) = series.last_mut() {
                if *prev == at {
                    *value = None;
                    continue;
                }
                if (*prev < start && at < start) || (*prev >= start && *prev / DAY == at / DAY) {
                    *prev = at;
                    *value = price;
                    continue;
                }
            }
            series.push((at, price));
        }
        Ok(Self(rows))
    }
    // Day-end lookups need only each day's last quote and the last pre-start quote.
    // None means no prior quote; Some(None) means the newest quote is stale or invalid.
    fn price(&self, asset: &Asset, now: u64, age: u64) -> Option<Option<Decimal>> {
        debug_assert_eq!(now % DAY, DAY - 1);
        let rows = self.0.get(&asset.instrument.id().to_string())?;
        let i = rows.partition_point(|(at, _)| *at <= now).checked_sub(1)?;
        let (at, price) = rows[i];
        Some(if now - at <= age { price } else { None })
    }
}
fn valuation_mark(
    asset: &Asset,
    now: u64,
    age: u64,
    quotes: Option<&SpotQuotes>,
) -> Option<Decimal> {
    quotes
        .and_then(|q| q.price(asset, now, age))
        .unwrap_or_else(|| mark(asset, now, age))
}

fn fx(
    currency: Currency,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
) -> Option<Decimal> {
    valuation_fx(currency, assets, fx_ids, now, age, None)
}
fn stable_valuation_fx(
    currency: Currency,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
    quotes: Option<&SpotQuotes>,
) -> Option<Decimal> {
    if currency == Currency::USDC() {
        return Some(Decimal::ONE);
    }
    assets
        .iter()
        .find(|a| {
            fx_ids.contains(&a.instrument.id().to_string())
                && a.instrument.base_currency() == Some(currency)
                && a.instrument.quote_currency() == Currency::USDC()
        })
        .and_then(|a| valuation_mark(a, now, age, quotes))
}
fn valuation_fx(
    currency: Currency,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
    quotes: Option<&SpotQuotes>,
) -> Option<Decimal> {
    if let Some(rate) = stable_valuation_fx(currency, assets, fx_ids, now, age, quotes) {
        return Some(rate);
    }
    assets
        .iter()
        .filter(|a| {
            matches!(a.instrument, InstrumentAny::CurrencyPair(_))
                && a.instrument.base_currency() == Some(currency)
        })
        .find_map(|a| {
            Some(
                valuation_mark(a, now, age, quotes)?
                    * stable_valuation_fx(
                        a.instrument.quote_currency(),
                        assets,
                        fx_ids,
                        now,
                        age,
                        quotes,
                    )?,
            )
        })
}
fn missing_mark_reason(
    source: &str,
    id: &str,
    observation: Option<(u64, bool)>,
    now: u64,
    age: u64,
) -> String {
    let state = match observation {
        None => "MISSING",
        Some((at, _)) if at > now => "FUTURE",
        Some((at, _)) if now - at > age => "STALE",
        Some((_, false)) => "NULL_PRICE",
        Some(_) => "UNAVAILABLE",
    };
    let last = observation.map_or_else(|| "NONE".into(), |(at, _)| at.to_string());
    format!("source={source}:instrument_id={id}:last_ns={last}:at_ns={now}:maximum_age_ns={age}:state={state}")
}

fn missing_fx_reason(
    currency: Currency,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
) -> String {
    let candidate = assets
        .iter()
        .find(|asset| {
            fx_ids.contains(&asset.instrument.id().to_string())
                && asset.instrument.base_currency() == Some(currency)
                && asset.instrument.quote_currency() == Currency::USDC()
        })
        .or_else(|| {
            assets.iter().find(|asset| {
                matches!(asset.instrument, InstrumentAny::CurrencyPair(_))
                    && asset.instrument.base_currency() == Some(currency)
            })
        });
    let detail = match candidate {
        Some(asset) if mark(asset, now, age).is_none() => missing_mark_reason(
            "COHORT_MARK",
            &asset.instrument.id().to_string(),
            asset.mark.map(|(at, _)| (at, true)),
            now,
            age,
        ),
        Some(asset) => {
            let quote = asset.instrument.quote_currency();
            let bridge = assets.iter().find(|asset| {
                fx_ids.contains(&asset.instrument.id().to_string())
                    && asset.instrument.base_currency() == Some(quote)
                    && asset.instrument.quote_currency() == Currency::USDC()
            });
            match bridge {
                Some(bridge) => missing_mark_reason("QUOTE_FX_COHORT_MARK", &bridge.instrument.id().to_string(), bridge.mark.map(|(at, _)| (at, true)), now, age),
                None => format!("source=QUOTE_FX_ROUTE:instrument_id={}:quote={quote}:at_ns={now}:maximum_age_ns={age}:state=NO_ROUTE", asset.instrument.id()),
            }
        }
        None => format!(
            "source=FX_ROUTE:instrument_id=NONE:at_ns={now}:maximum_age_ns={age}:state=NO_ROUTE"
        ),
    };
    format!("currency={currency}:{detail}")
}

fn value(
    ledger: &NativeLedger,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
    margin: bool,
    contexts: &mut Option<ContextPrices>,
    reason: Option<&mut String>,
) -> Result<Option<Decimal>> {
    valuation_value(
        ledger, assets, fx_ids, now, age, margin, contexts, reason, None,
    )
}

fn valuation_value(
    ledger: &NativeLedger,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
    margin: bool,
    contexts: &mut Option<ContextPrices>,
    reason: Option<&mut String>,
    quotes: Option<&SpotQuotes>,
) -> Result<Option<Decimal>> {
    let mut total = Decimal::ZERO;
    for balance in ledger.balances() {
        let amount = balance.as_decimal();
        if amount.is_zero() {
            continue;
        }
        let Some(rate) = valuation_fx(balance.currency, assets, fx_ids, now, age, quotes) else {
            if let Some(reason) = reason.filter(|reason| reason.is_empty()) {
                *reason = format!(
                    "BALANCE_FX_UNAVAILABLE:ledger={}:amount={amount}:{}",
                    if margin { "MARGIN" } else { "CASH" },
                    if quotes.is_some() {
                        format!("currency={}:source=SPOT_QUOTE_OR_COHORT_FX:at_ns={now}:maximum_age_ns={age}:state=UNAVAILABLE", balance.currency)
                    } else {
                        missing_fx_reason(balance.currency, assets, fx_ids, now, age)
                    }
                );
            }
            return Ok(None);
        };
        total += amount * rate;
    }
    if margin {
        for position in ledger.positions() {
            let Some(asset) = assets
                .iter()
                .find(|a| a.instrument.id() == position.instrument_id)
            else {
                anyhow::bail!("ECONOMIC_POSITION_OUTSIDE_SCOPE")
            };
            let price = match contexts.as_mut() {
                Some(context) => {
                    context.price(&position.instrument_id.to_string(), now, age, false)?
                }
                None => mark(asset, now, age),
            };
            let Some(price) = price else {
                if let Some(reason) = reason.filter(|reason| reason.is_empty()) {
                    let id = position.instrument_id.to_string();
                    let detail = if let Some(context) = contexts.as_ref() {
                        missing_mark_reason(
                            "CONTEXT_MARK",
                            &id,
                            context
                                .latest(&id)
                                .map(|row| (row.event, row.mark.is_some())),
                            now,
                            age,
                        )
                    } else {
                        missing_mark_reason(
                            "COHORT_MARK",
                            &id,
                            asset.mark.map(|(at, _)| (at, true)),
                            now,
                            age,
                        )
                    };
                    *reason = format!(
                        "PERP_MARK_UNAVAILABLE:quantity={}:{}",
                        position.quantity, detail
                    );
                }
                return Ok(None);
            };
            let pnl = ledger.unrealized_pnl_at(position.instrument_id, price)?;
            ensure!(
                pnl.currency == Currency::USDC() || pnl.currency == Currency::USD(),
                "ECONOMIC_PERP_SETTLEMENT_UNSUPPORTED"
            );
            total += pnl.as_decimal();
        }
    }
    Ok(Some(total))
}

fn definitions(root: &Path) -> Result<Vec<InstrumentAny>> {
    let manifest = cohort_cache::manifest(root)?;
    crate::catalog::currencies::instruments_from_value(
        manifest.source_report["instrument_source"].clone(),
    )
}

/// Preserve optional economic sidecars through native preparation.
/// Decoding remains the economic runner's job; preparation does not qualify observations.
pub(crate) fn funding_sources(
    root: &Path,
    request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
) -> Result<Vec<(String, u64)>> {
    let names = ["funding-rates.jsonl", "funding-rates.manifest.json"];
    let mut files = Vec::new();
    if names.iter().any(|name| root.join(name).exists()) {
        for name in names {
            let state = std::fs::symlink_metadata(root.join(name))?;
            ensure!(
                state.is_file() && !state.file_type().is_symlink(),
                "ECONOMIC_FUNDING_REGULAR_SOURCE_REQUIRED"
            );
            files.push((name.into(), state.len()));
        }
        let manifest: FundingManifest =
            serde_json::from_reader(BufReader::new(File::open(root.join(names[1]))?))?;
        ensure!(
            manifest.source_revision == request.input.source_revision
                && manifest.start_ns <= request.input.event_start_ns
                && manifest.end_ns >= request.input.event_end_ns,
            "ECONOMIC_FUNDING_COVERAGE_INVALID"
        );
    }
    if root.join(CONTEXT_MANIFEST).exists() {
        let context = context_manifest(root, request)?;
        files.push((
            CONTEXT_MANIFEST.into(),
            std::fs::metadata(root.join(CONTEXT_MANIFEST))?.len(),
        ));
        files.extend(
            context
                .files
                .into_iter()
                .map(|file| (file.path, file.bytes.get())),
        );
    }
    if root.join(EVENT_TIME_CONTEXT_MANIFEST).exists() {
        let context = event_time_context_manifest(root, request)?;
        files.push((
            EVENT_TIME_CONTEXT_MANIFEST.into(),
            std::fs::metadata(root.join(EVENT_TIME_CONTEXT_MANIFEST))?.len(),
        ));
        files.extend(
            context
                .files
                .into_iter()
                .map(|file| (file.path, file.bytes.get())),
        );
    }
    for (name, issue) in [
        (
            "spot-valuation.jsonl",
            "ECONOMIC_SPOT_QUOTE_REGULAR_SOURCE_REQUIRED",
        ),
        (
            "book-snapshots.source.json",
            "ECONOMIC_BOOK_SOURCE_REGULAR_FILE_REQUIRED",
        ),
    ] {
        match std::fs::symlink_metadata(root.join(name)) {
            Ok(state) => {
                ensure!(
                    state.is_file() && !state.file_type().is_symlink(),
                    "{issue}"
                );
                files.push((name.into(), state.len()));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(files)
}

/// Model and research definition are the retained candidate's, never an unlogged formula.
pub fn replay(
    root: &Path,
    request: &NativeTickEconomicRequestV1,
    wasm: &[u8],
) -> Result<NativeTickEconomicResultV1> {
    replay_with_funding(root, root, request, wasm)
}

pub fn replay_with_funding(
    root: &Path,
    funding_root: &Path,
    request: &NativeTickEconomicRequestV1,
    wasm: &[u8],
) -> Result<NativeTickEconomicResultV1> {
    replay_models_with_funding(root, funding_root, request, &[wasm.to_vec()])
}

pub fn replay_models_with_funding(
    root: &Path,
    funding_root: &Path,
    request: &NativeTickEconomicRequestV1,
    wasms: &[Vec<u8>],
) -> Result<NativeTickEconomicResultV1> {
    replay_models_with_funding_and_cores(root, funding_root, request, wasms, 1)
}

pub fn replay_models_with_funding_and_cores(
    root: &Path,
    funding_root: &Path,
    request: &NativeTickEconomicRequestV1,
    wasms: &[Vec<u8>],
    cores: usize,
) -> Result<NativeTickEconomicResultV1> {
    ensure!(cores > 0, "ECONOMIC_BOOK_CORES_INVALID");
    validate(request)?;
    let mut source_request = request.research.clone();
    for id in &request.fx_instrument_ids {
        if !source_request.input.instrument_ids.contains(id) {
            source_request.input.instrument_ids.push(id.clone());
        }
    }
    let cache = cohort_cache::load(root, &source_request)?;
    let mut defs = definitions(root)?
        .into_iter()
        .map(|i| (i.id().to_string(), i))
        .collect::<BTreeMap<_, _>>();
    let weights = request
        .signal_weights
        .clone()
        .unwrap_or_else(|| vec!["1".parse().unwrap()]);
    ensure!(
        weights.len() == wasms.len(),
        "ECONOMIC_SIGNAL_MODEL_COUNT_MISMATCH"
    );
    let modules = weights
        .iter()
        .zip(wasms)
        .map(|(weight, wasm)| {
            Ok((
                decimal(weight)?
                    .to_f64()
                    .context("ECONOMIC_SIGNAL_WEIGHT_RANGE")?,
                SignalModule::new(wasm, request.research.parameters.total_fuel.is_some())?,
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let mut consumed = 0u64;
    let mut assets = Vec::new();
    let mut heap = BinaryHeap::<Reverse<(u64, usize)>>::new();
    for a in &cache.assets {
        let instrument = defs
            .remove(&a.instrument_id)
            .context("ECONOMIC_NATIVE_DEFINITION_REQUIRED")?;
        ensure!(
            matches!(
                instrument,
                InstrumentAny::CurrencyPair(_) | InstrumentAny::CryptoPerpetual(_)
            ) && !instrument.is_inverse(),
            "ECONOMIC_INSTRUMENT_UNSUPPORTED"
        );
        if matches!(instrument, InstrumentAny::CryptoPerpetual(_)) {
            ensure!(
                instrument.settlement_currency() == Currency::USDC(),
                "ECONOMIC_PERP_SETTLEMENT_UNSUPPORTED"
            );
            ensure!(
                !observed(&request.scenario) || request.context_source.is_some(),
                "ECONOMIC_OBSERVED_ORACLE_CONTEXT_REQUIRED"
            );
        }
        let mut reader = cache.cohorts(a)?;
        let head = reader.next().transpose()?;
        let index = assets.len();
        if let Some(h) = &head {
            heap.push(Reverse((h.event, index)));
        }
        let fuel = request
            .research
            .parameters
            .total_fuel
            .map(|v| v.get() - consumed);
        let model = if a.cohort_count == 0
            || !request
                .research
                .input
                .instrument_ids
                .contains(&a.instrument_id)
        {
            None
        } else {
            let (model, used) = EconomicModels::new(&modules, a.cohort_count, fuel)?;
            consumed += used;
            Some(model)
        };
        let feature_books = (request.research.parameters.uses_book_features() && model.is_some())
            .then(|| {
                books::FeatureBooks::open(
                    root,
                    instrument.id(),
                    request.research.input.event_start_ns.get(),
                    request.research.input.event_end_ns.get(),
                )
            })
            .transpose()?;
        assets.push(Asset {
            instrument,
            reader,
            head,
            previous: None,
            fast: ExponentialMovingAverage::new(
                request.research.parameters.fast_period_cohorts as usize,
                None,
            ),
            slow: ExponentialMovingAverage::new(
                request.research.parameters.slow_period_cohorts as usize,
                None,
            ),
            model,
            feature_books,
            score: None,
            mark: None,
            cohorts: 0,
            decisions: 0,
            attempts: 0,
            pending: None,
            target_weight: None,
        });
    }
    let s = &request.scenario;
    let age = s.maximum_mark_age_ns.get();
    let fx_ids = request
        .fx_instrument_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let usdt0 = Currency::from_str("USDT0")?;
    let bridge = assets
        .iter()
        .position(|a| {
            fx_ids.contains(&a.instrument.id().to_string())
                && a.instrument.base_currency() == Some(usdt0)
                && a.instrument.quote_currency() == Currency::USDC()
        })
        .context("ECONOMIC_USDT0_USDC_BRIDGE_REQUIRED")?;
    let observed = observed(s);
    let normalize_sizes = normalize_observed_book_sizes(s);
    let book_profile = request.research.parameters.uses_book_features();
    let mut book_heap = BinaryHeap::<Reverse<(u64, usize)>>::new();
    let mut book_readers = if observed {
        Some(books::BookReaders::open(
            funding_root,
            &assets
                .iter()
                .map(|asset| asset.instrument.id())
                .collect::<Vec<_>>(),
            request.research.input.event_start_ns.get(),
            request.research.input.event_end_ns.get(),
            cores,
        )?)
    } else {
        None
    };
    let mut book_heads = Vec::new();
    if observed {
        for (i, asset) in assets.iter().enumerate() {
            let head = book_readers.as_mut().unwrap().next(i)?;
            let head = if normalize_sizes {
                head.map(|frame| frame.floor_sizes(asset.instrument.size_increment()))
                    .transpose()?
            } else {
                head
            };
            if let Some(head) = &head {
                book_heap.push(Reverse((head.received(), i)));
            }
            book_heads.push(head);
        }
    }
    let (funding_manifest, mut funding) = if assets
        .iter()
        .any(|a| matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)))
    {
        ensure!(
            !funding_sources(funding_root, &source_request)?.is_empty(),
            "ECONOMIC_FUNDING_SOURCE_REQUIRED"
        );
        let manifest: FundingManifest = serde_json::from_reader(File::open(
            funding_root.join("funding-rates.manifest.json"),
        )?)?;
        let funding_ids = manifest
            .instruments
            .iter()
            .cloned()
            .collect::<BTreeSet<_>>();
        ensure!(
            manifest.source_revision == request.research.input.source_revision
                && manifest.start_ns <= request.research.input.event_start_ns
                && manifest.end_ns >= request.research.input.event_end_ns
                && funding_ids.len() == manifest.instruments.len()
                && assets
                    .iter()
                    .filter(|a| matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)))
                    .all(|a| funding_ids.contains(&a.instrument.id().to_string())),
            "ECONOMIC_FUNDING_COVERAGE_INVALID"
        );
        let funding = Funding::open(&funding_root.join(&request.funding_source), funding_ids)?;
        (Some(manifest), funding)
    } else {
        (None, Funding::default())
    };
    let missing_funding = funding_manifest
        .as_ref()
        .map(|manifest| {
            manifest
                .missing_instruments
                .iter()
                .cloned()
                .collect::<BTreeSet<_>>()
        })
        .unwrap_or_default();
    let mut contexts = (request.context_source.is_some()
        || request.event_time_context_source.is_some())
    .then(|| {
        ContextPrices::open(
            funding_root,
            &source_request,
            request.context_source.is_some(),
            request.event_time_context_source.is_some(),
        )
    })
    .transpose()?;
    let spot_quotes = request
        .spot_valuation_source
        .as_ref()
        .map(|name| {
            SpotQuotes::open(
                &funding_root.join(name),
                &assets,
                request.research.input.event_start_ns.get(),
                request.research.input.event_end_ns.get(),
            )
        })
        .transpose()?;
    let mut seed = s.random_seed.get();
    let mut cash: Option<NativeLedger> = None;
    let mut margin: Option<NativeLedger> = None;
    let mut cash_started = false;
    let mut margin_started = false;
    let mut funding_unknown = None;
    let mut remaining = decimal(&s.initial_usdt)? * decimal(&s.usdt0_per_usdt)?;
    let mut conversion_ledger = if observed {
        Some(NativeLedger::new_conversion(
            assets[bridge].instrument.clone(),
            Money::from_decimal(remaining, usdt0)?,
            fee_schedule(s, false)?,
        )?)
    } else {
        Some(NativeLedger::new_conversion_scenario(
            assets[bridge].instrument.clone(),
            Money::from_decimal(remaining, usdt0)?,
            fee_schedule(s, false)?,
            s.execution_model.is_some(),
        )?)
    };
    let mut converted = Decimal::ZERO;
    let mut reserve_usdc = Decimal::ZERO;
    let mut conversion_due = None;
    let mut conversion_ns = None;
    let mut initial_usdc = None;
    let mut conversion_cost = Decimal::ZERO;
    let mut last_target_asof = None;
    let mut next_grid = request.research.input.event_start_ns.get();
    let mut day_end = (next_grid / DAY + 1)
        .checked_mul(DAY)
        .context("ECONOMIC_CLOCK_OVERFLOW")?;
    let mut days = Vec::new();
    let mut previous_equity = Some(decimal(&s.initial_usdt)?);
    let start = request.research.input.event_start_ns.get();
    let mut next_hour = (start / HOUR + u64::from(!start.is_multiple_of(HOUR))) * HOUR;
    let mut expected_rates = BTreeSet::new();
    let mut seen_rates = BTreeSet::new();
    let mut small = request
        .small_capital_policy
        .as_ref()
        .map(|_| SmallCapital::new(assets.len()));
    let fixed_clock = s.decision_clock.or_else(|| {
        request
            .small_capital_policy
            .as_ref()
            .and_then(|policy| policy.decision_clock)
    }) == Some(NativeTickDecisionClockV1::FixedIntervalV1);
    let mut decision_timer = fixed_clock.then(|| {
        VirtualTimer::new(
            "economic-decisions".into(),
            create_valid_interval(DurationNanos::new(s.decision_interval_ns.get())),
            start.into(),
            Some((request.research.input.event_end_ns.get() - 1).into()),
            true,
        )
    });
    let mut decision_fresh = BTreeSet::new();
    let mut latest_source_events = vec![0; assets.len()];
    while let Some(event) = heap
        .peek()
        .map(|Reverse((at, _))| *at)
        .into_iter()
        .chain(book_heap.peek().map(|Reverse((at, _))| *at))
        .chain(
            decision_timer
                .as_ref()
                .filter(|timer| !timer.is_expired())
                .map(|timer| timer.next_time_ns().as_u64())
                .filter(|at| *at < request.research.input.event_end_ns.get()),
        )
        .min()
    {
        let decision_tick = decision_timer
            .as_mut()
            .is_some_and(|timer| timer.next_time_ns().as_u64() == event && timer.next().is_some());
        // Complete each global timestamp before scheduling new decisions.
        let mut group = Vec::new();
        while heap.peek().is_some_and(|Reverse((at, _))| *at == event) {
            group.push(heap.pop().unwrap().0 .1);
        }
        let mut snapshots = BTreeMap::new();
        while book_heap
            .peek()
            .is_some_and(|Reverse((at, _))| *at == event)
        {
            let i = book_heap.pop().unwrap().0 .1;
            let frame = book_heads[i].take().unwrap();
            snapshots.insert(i, frame);
            let next = book_readers.as_mut().unwrap().next(i)?;
            book_heads[i] = if normalize_sizes {
                next.map(|frame| frame.floor_sizes(assets[i].instrument.size_increment()))
                    .transpose()?
            } else {
                next
            };
            if let Some(next) = &book_heads[i] {
                book_heap.push(Reverse((next.received(), i)));
            }
        }
        if let Some(small) = &mut small {
            if decision_tick {
                *small.decision_clock_ticks.get_or_insert(0) += 1;
            }
            small.expire(event, &mut assets, s, &mut seed)?;
        }
        while day_end <= event {
            expected_until(day_end, &mut next_hour, &margin, &mut expected_rates)?;
            funding_before(
                day_end,
                &mut funding,
                &mut margin,
                &assets,
                start,
                age,
                &mut funding_unknown,
                &mut seen_rates,
                &mut contexts,
            )?;
            if let Some(cause) = missing_rates_before(day_end, &mut expected_rates, &mut seen_rates)
            {
                retain_first_funding_cause(&mut funding_unknown, cause);
            }
            append_day(
                &mut days,
                day_end - DAY,
                day_end - 1,
                &mut previous_equity,
                &cash,
                &margin,
                &assets,
                &fx_ids,
                s,
                remaining,
                converted,
                reserve_usdc,
                funding_unknown.is_some(),
                &mut contexts,
                spot_quotes.as_ref(),
            )?;
            day_end = day_end
                .checked_add(DAY)
                .context("ECONOMIC_CLOCK_OVERFLOW")?;
        }
        expected_until(
            event.checked_add(1).context("ECONOMIC_CLOCK_OVERFLOW")?,
            &mut next_hour,
            &margin,
            &mut expected_rates,
        )?;
        funding_before(
            event,
            &mut funding,
            &mut margin,
            &assets,
            start,
            age,
            &mut funding_unknown,
            &mut seen_rates,
            &mut contexts,
        )?;
        // Same-timestamp rates and marks share one complete native streaming cohort.
        let rates = rates_at(
            &mut funding,
            &mut margin,
            &assets,
            event,
            start,
            age,
            &mut funding_unknown,
            &mut seen_rates,
            &mut contexts,
        )?;
        for (&i, frame) in &snapshots {
            assets[i].mark = frame.midpoint.map(|price| (event, price));
        }
        let mut marks_cash = Vec::new();
        let mut marks_margin = Vec::new();
        let mut fresh = Vec::new();
        let mut attempted = Vec::new();
        for &i in &group {
            if !fixed_clock {
                if let Some(small) = &mut small {
                    let price = finite(assets[i].head.as_ref().unwrap().vwap)?;
                    small.observe_reference(i, event, price, &mut assets, s, &mut seed)?;
                }
            }
            let a = &mut assets[i];
            let current = a.head.take().unwrap();
            latest_source_events[i] = latest_source_events[i].max(current.source_event_end - 1);
            a.fast.update_raw(current.vwap);
            a.slow.update_raw(current.vwap);
            if let Some(previous) = &a.previous {
                if a.fast.initialized() && a.slow.initialized() && a.model.is_some() {
                    let ordinal = u32::try_from(a.cohorts)?;
                    let received = request.research.input.uses_received_time();
                    let feature_at = |value, cohort: &Cohort| {
                        clocked_feature(
                            value,
                            cohort.source_event_end - 1,
                            cohort.event,
                            ordinal,
                            received,
                        )
                    };
                    let features = [
                        feature_at(current.vwap, &current)?,
                        feature_at(previous.vwap, previous)?,
                        clocked_feature(
                            a.fast.value(),
                            latest_source_events[i],
                            current.event,
                            ordinal,
                            received,
                        )?,
                        clocked_feature(
                            a.slow.value(),
                            latest_source_events[i],
                            current.event,
                            ordinal,
                            received,
                        )?,
                        feature_at(current.quantity, &current)?,
                        feature_at(current.count as f64, &current)?,
                        feature_at((current.event - previous.event) as f64, &current)?,
                        feature_at(
                            current.signed_quantity.context("ECONOMIC_FLOW_REQUIRED")?,
                            &current,
                        )?,
                    ];
                    let mut extended_features;
                    let features = if book_profile {
                        extended_features = features.to_vec();
                        books::BookFeatures::append(
                            &mut extended_features,
                            a.feature_books
                                .as_mut()
                                .context("TICK_BOOK_FEATURE_SOURCE_REQUIRED")?
                                .advance(current.event)?,
                            current.event,
                        );
                        extended_features.as_slice()
                    } else {
                        &features
                    };
                    let model = a.model.as_mut().context("ECONOMIC_MODEL_MISSING")?;
                    let remaining_fuel = request
                        .research
                        .parameters
                        .total_fuel
                        .map(|total| total.get() - consumed);
                    let (score, used) = model.predict(
                        event,
                        current.feature_event(received),
                        ordinal,
                        features,
                        remaining_fuel,
                    )?;
                    a.score = Some(score);
                    if small.is_some() {
                        fresh.push(i);
                        if fixed_clock {
                            decision_fresh.insert(i);
                        }
                    }
                    if request.research.parameters.total_fuel.is_some() {
                        consumed = consumed.checked_add(used).context("ECONOMIC_FUEL_RANGE")?;
                        ensure!(
                            consumed <= request.research.parameters.total_fuel.unwrap().get(),
                            "FEATURE_MODEL_BUDGET_EXHAUSTED"
                        );
                    }
                }
            }
            let target = if !observed
                && a.pending
                    .is_some_and(|(decision, due, _)| decision < event && due <= event)
            {
                a.attempts += 1;
                if small.is_some() {
                    attempted.push(i);
                }
                if small.is_some() && fx_ids.contains(&a.instrument.id().to_string()) {
                    a.pending.map(|(_, _, q)| q)
                } else {
                    a.pending.take().map(|(_, _, q)| q)
                }
            } else {
                None
            };
            let native = NativeMark {
                instrument_id: a.instrument.id(),
                mark: current.vwap,
                capacity: finite(current.quantity)? * decimal(&s.participation_fraction)?,
                target_quantity: target,
            };
            if !observed && matches!(a.instrument, InstrumentAny::CurrencyPair(_)) {
                if native.target_quantity.is_some() || !cash_started {
                    marks_cash.push(native);
                }
            } else if !observed && (native.target_quantity.is_some() || !margin_started) {
                marks_margin.push(native);
            }

            if !observed {
                a.mark = Some((event, finite(current.vwap)?));
            }
            a.cohorts += 1;
            a.previous = Some(current);
            a.head = a.reader.next().transpose()?;
            if let Some(h) = &a.head {
                heap.push(Reverse((h.event, i)));
            }
        }
        if observed {
            for (&i, frame) in &snapshots {
                let a = &mut assets[i];
                let ledger = if matches!(a.instrument, InstrumentAny::CurrencyPair(_)) {
                    &cash
                } else {
                    &margin
                };
                let due = a
                    .pending
                    .filter(|(decision, due, _)| *decision < event && *due <= event);
                let held = if due.is_some() {
                    ledger.as_ref().map_or(Decimal::ZERO, |ledger| {
                        ledger.current_quantity(a.instrument.id())
                    })
                } else {
                    Decimal::ZERO
                };
                let buy = due.is_some_and(|(_, _, target)| target > held);
                let reducing = due.is_some_and(|(_, _, target)| {
                    !held.is_zero()
                        && (target.is_zero()
                            || target.is_sign_negative() == held.is_sign_negative()
                                && target.abs() < held.abs())
                });
                let Some(price) =
                    frame
                        .midpoint
                        .or_else(|| if reducing { frame.price(buy) } else { None })
                else {
                    continue;
                };
                let capacity = frame.capacity(buy) * decimal(&s.participation_fraction)?;
                let target = if let Some((_, _, quantity)) = due {
                    let executable = frame
                        .price(buy)
                        .map(|price| {
                            small_legal_quantity(
                                &a.instrument,
                                (quantity - held).abs().min(capacity),
                                price,
                                reducing,
                            )
                        })
                        .transpose()?
                        .unwrap_or(Decimal::ZERO);
                    if quantity == held {
                        a.pending = None;
                        None
                    } else if executable.is_zero() {
                        None
                    } else {
                        a.attempts += 1;
                        if small.is_some() {
                            attempted.push(i);
                        }
                        if small.is_none() || !fx_ids.contains(&a.instrument.id().to_string()) {
                            a.pending = None;
                        }
                        Some(quantity)
                    }
                } else {
                    None
                };
                let row = NativeMark {
                    instrument_id: a.instrument.id(),
                    mark: price.to_f64().context("ECONOMIC_BOOK_PRICE_RANGE")?,
                    capacity,
                    target_quantity: target,
                };
                if matches!(a.instrument, InstrumentAny::CurrencyPair(_)) {
                    if row.target_quantity.is_some() || !cash_started {
                        marks_cash.push(row);
                    }
                } else if row.target_quantity.is_some() || !margin_started {
                    marks_margin.push(row);
                }
            }
        }
        let converting = cash.is_none();
        let mut conversion_advanced = false;
        let bridge_price = if observed {
            snapshots.get(&bridge).and_then(|frame| frame.price(false))
        } else {
            assets[bridge].mark.map(|(_, price)| price)
        };
        if converting
            && ((!observed && group.contains(&bridge)) || snapshots.contains_key(&bridge))
            && bridge_price.is_some()
        {
            let price = bridge_price.unwrap();
            if conversion_due.is_none() {
                conversion_due = Some(
                    event
                        .checked_add(delay(&mut seed, s)?)
                        .and_then(|t| t.checked_add(1))
                        .context("ECONOMIC_CLOCK_OVERFLOW")?,
                );
            } else if conversion_due.is_some_and(|at| event >= at) {
                if let Some(ledger) = &mut conversion_ledger {
                    let capacity = if observed {
                        snapshots[&bridge].bid_depth
                    } else {
                        finite(assets[bridge].previous.as_ref().unwrap().quantity)?
                    } * decimal(&s.participation_fraction)?;
                    let quantity = conversion_quantity(
                        remaining,
                        capacity,
                        price,
                        &assets[bridge].instrument,
                    )?;
                    if quantity > Decimal::ZERO {
                        if observed {
                            ledger.step_books(
                                event,
                                vec![NativeBook {
                                    snapshot: snapshots[&bridge].deltas.clone(),
                                    capacity,
                                    target_quantity: Some(Decimal::ZERO),
                                }],
                                &[],
                            )?;
                        } else {
                            ledger.step_batch(
                                event,
                                &[NativeMark {
                                    instrument_id: assets[bridge].instrument.id(),
                                    mark: price
                                        .to_f64()
                                        .context("ECONOMIC_CONVERSION_PRICE_RANGE")?,
                                    capacity,
                                    target_quantity: Some(Decimal::ZERO),
                                }],
                            )?;
                        }
                        conversion_advanced = true;
                    }
                    remaining = ledger
                        .balance(usdt0)
                        .context("ECONOMIC_CONVERSION_BALANCE_MISSING")?
                        .as_decimal();
                    converted = ledger
                        .balance(Currency::USDC())
                        .context("ECONOMIC_CONVERSION_BALANCE_MISSING")?
                        .as_decimal();
                    conversion_cost = ledger
                        .summary()
                        .commissions
                        .get("USDC")
                        .copied()
                        .unwrap_or(Decimal::ZERO);
                }
                if converted > Decimal::ZERO
                    && conversion_quantity(remaining, remaining, price, &assets[bridge].instrument)?
                        .is_zero()
                {
                    let split = decimal(&s.spot_fraction)?;
                    let precision = Currency::USDC().precision.into();
                    let cash_capital = (converted * split)
                        .round_dp_with_strategy(precision, rust_decimal::RoundingStrategy::ToZero);
                    let margin_capital = (converted - cash_capital)
                        .round_dp_with_strategy(precision, rust_decimal::RoundingStrategy::ToZero);
                    reserve_usdc = converted - cash_capital - margin_capital;
                    let ledger = build(&assets, false, cash_capital, s)?;
                    let bridges = assets
                        .iter()
                        .filter(|a| fx_ids.contains(&a.instrument.id().to_string()))
                        .map(|a| a.instrument.id())
                        .collect::<Vec<_>>();
                    ledger.inventory_target_instruments(&bridges)?;
                    cash = Some(ledger);
                    if assets
                        .iter()
                        .any(|a| matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)))
                    {
                        margin = Some(build(&assets, true, margin_capital, s)?);
                    } else {
                        reserve_usdc += margin_capital;
                    }
                    conversion_ns = Some(counter(event)?);
                    initial_usdc = Some(exact(converted)?);
                }
            }
        }
        if converting && !conversion_advanced {
            if let (Some(ledger), Some(frame)) = (&mut conversion_ledger, snapshots.get(&bridge)) {
                ledger.step_books(
                    event,
                    vec![NativeBook {
                        snapshot: frame.deltas.clone(),
                        capacity: Decimal::ZERO,
                        target_quantity: None,
                    }],
                    &[],
                )?;
            }
        }
        if let (Some(small), Some(c)) = (&small, &cash) {
            let m = margin.as_ref();
            small_cap_entries(
                small,
                &mut marks_cash,
                &assets,
                c,
                m,
                &fx_ids,
                event,
                s,
                &mut contexts,
                observed.then_some(&snapshots),
            )?;
            small_cap_entries(
                small,
                &mut marks_margin,
                &assets,
                c,
                m,
                &fx_ids,
                event,
                s,
                &mut contexts,
                observed.then_some(&snapshots),
            )?;
            if funding_unknown.is_some() {
                for row in &mut marks_margin {
                    if row.target_quantity.is_some_and(|q| !q.is_zero()) {
                        row.target_quantity = None;
                    }
                }
            }
        }
        if let Some(ledger) = cash.as_mut() {
            if !marks_cash.is_empty()
                || observed
                    && snapshots
                        .keys()
                        .any(|i| matches!(assets[*i].instrument, InstrumentAny::CurrencyPair(_)))
            {
                if observed {
                    ledger.step_books(
                        event,
                        native_books(marks_cash, &snapshots, &assets, false),
                        &[],
                    )?;
                } else {
                    ledger.step_batch(event, &marks_cash)?;
                }
                cash_started = true;
            }
        }
        if let Some(ledger) = margin.as_mut() {
            if !marks_margin.is_empty()
                || !rates.is_empty()
                || observed
                    && snapshots
                        .keys()
                        .any(|i| matches!(assets[*i].instrument, InstrumentAny::CryptoPerpetual(_)))
            {
                if observed {
                    ledger.step_books(
                        event,
                        native_books(marks_margin, &snapshots, &assets, true),
                        &rates,
                    )?;
                } else {
                    ledger.step_batch_with_rates(event, &marks_margin, &rates)?;
                }
                margin_started = true;
                for id in ledger.open_instrument_ids() {
                    let id = id.to_string();
                    if missing_funding.contains(&id) {
                        retain_first_funding_cause(&mut funding_unknown, (event, format!(
                            "FUNDING_FIRST_CAUSE:MISSING_INSTRUMENT:event_ns={event}:instrument_id={id}"
                        )));
                    }
                }
            }
        }
        if let (Some(small), Some(c)) = (&mut small, &cash) {
            let m = margin.as_ref();
            let reconciled = if observed {
                snapshots.keys().copied().collect::<Vec<_>>()
            } else {
                group.clone()
            };
            for i in reconciled {
                if small.cycles[i].is_some() {
                    let quantity = small_quantity(&assets[i], c, m);
                    small.reconcile(
                        i,
                        event,
                        quantity,
                        attempted.contains(&i),
                        &mut assets,
                        s,
                        &mut seed,
                    )?;
                }
                if attempted.contains(&i) && fx_ids.contains(&assets[i].instrument.id().to_string())
                {
                    assets[i].pending = None;
                }
            }
            if decision_tick {
                for &i in &decision_fresh {
                    let price = finite(assets[i].previous.as_ref().unwrap().vwap)?;
                    small.observe_reference(i, event, price, &mut assets, s, &mut seed)?;
                }
            }
            if decision_tick || (!fixed_clock && event >= next_grid) {
                if let Some(m) = m {
                    let net = value(m, &assets, &fx_ids, event, age, true, &mut contexts, None)?;
                    let mut maintenance = Some(Decimal::ZERO);
                    for p in m.positions() {
                        let Some(a) = assets.iter().find(|a| a.instrument.id() == p.instrument_id)
                        else {
                            maintenance = None;
                            break;
                        };
                        let Some(price) = small_position_price(a, event, age, &mut contexts)?
                        else {
                            maintenance = None;
                            break;
                        };
                        let native_margin = m.maintenance_at(p.instrument_id, price)?;
                        maintenance = maintenance.map(|sum| sum + native_margin.as_decimal());
                    }
                    if net
                        .zip(maintenance)
                        .is_some_and(|(net, needed)| needed > Decimal::ZERO && net <= needed)
                    {
                        for i in 0..assets.len() {
                            if matches!(assets[i].instrument, InstrumentAny::CryptoPerpetual(_)) {
                                small.exit(i, event, &mut assets, s, &mut seed)?;
                            }
                        }
                    }
                }
                if !fixed_clock {
                    next_grid = (event / s.decision_interval_ns.get() + 1)
                        .checked_mul(s.decision_interval_ns.get())
                        .context("ECONOMIC_CLOCK_OVERFLOW")?;
                }
            }
            if event < request.research.input.decision_cutoff_ns.get()
                && (!fixed_clock || decision_tick)
            {
                let candidates = if fixed_clock {
                    decision_fresh.iter().copied().collect()
                } else {
                    fresh
                };
                let eligible: Vec<_> = candidates
                    .into_iter()
                    .filter(|i| {
                        funding_unknown.is_none()
                            || !matches!(assets[*i].instrument, InstrumentAny::CryptoPerpetual(_))
                    })
                    .collect();
                small_new_entries(
                    small,
                    &eligible,
                    &mut assets,
                    c,
                    m,
                    &fx_ids,
                    event,
                    request.small_capital_policy.as_ref().unwrap(),
                    s,
                    &mut seed,
                    &mut contexts,
                )?;
            }
        }
        if (decision_tick || !fixed_clock && event >= next_grid)
            && cash.is_some()
            && small.is_none()
        {
            schedule(
                &mut assets,
                &cash,
                &margin,
                &fx_ids,
                event,
                s,
                &mut seed,
                &mut contexts,
            )?;
            last_target_asof = Some(event);
            if !fixed_clock {
                next_grid = (event / s.decision_interval_ns.get() + 1)
                    .checked_mul(s.decision_interval_ns.get())
                    .context("ECONOMIC_CLOCK_OVERFLOW")?;
            }
        }
        if decision_tick {
            decision_fresh.clear();
        }
    }
    let end = request.research.input.event_end_ns.get();
    if let Some(small) = &mut small {
        small.expire(end - 1, &mut assets, s, &mut seed)?;
    }
    while day_end <= end {
        expected_until(day_end, &mut next_hour, &margin, &mut expected_rates)?;
        funding_before(
            day_end,
            &mut funding,
            &mut margin,
            &assets,
            start,
            age,
            &mut funding_unknown,
            &mut seen_rates,
            &mut contexts,
        )?;
        if let Some(cause) = missing_rates_before(day_end, &mut expected_rates, &mut seen_rates) {
            retain_first_funding_cause(&mut funding_unknown, cause);
        }
        append_day(
            &mut days,
            day_end - DAY,
            day_end - 1,
            &mut previous_equity,
            &cash,
            &margin,
            &assets,
            &fx_ids,
            s,
            remaining,
            converted,
            reserve_usdc,
            funding_unknown.is_some(),
            &mut contexts,
            spot_quotes.as_ref(),
        )?;
        day_end = day_end
            .checked_add(DAY)
            .context("ECONOMIC_CLOCK_OVERFLOW")?;
    }
    funding_before(
        end,
        &mut funding,
        &mut margin,
        &assets,
        start,
        age,
        &mut funding_unknown,
        &mut seen_rates,
        &mut contexts,
    )?;
    while funding.next.is_some() {
        funding.advance()?;
    }
    if let Some(manifest) = &funding_manifest {
        ensure!(
            funding.rows == manifest.rows,
            "ECONOMIC_FUNDING_ROW_COUNT_MISMATCH"
        );
    }
    if let Some(context) = contexts.as_mut() {
        context.finish()?;
    }
    if let Some(c) = cash.as_mut() {
        c.finish()?;
    }
    if let Some(conversion) = conversion_ledger.as_mut() {
        conversion.finish()?;
    }
    if let Some(m) = margin.as_mut() {
        m.finish()?;
    }
    let initial_conversion_evidence = conversion_ledger
        .as_ref()
        .map(NativeLedger::conversion_evidence)
        .transpose()?;
    let (fills, rejections) = cash
        .iter()
        .chain(margin.iter())
        .chain(conversion_ledger.iter())
        .fold((0, 0), |(f, r), l| {
            let s = l.summary();
            (f + s.fills, r + s.rejected)
        });
    let mut fees = BTreeMap::<String, Decimal>::from([("USDC".into(), conversion_cost)]);
    for ledger in cash.iter().chain(margin.iter()) {
        for (currency, amount) in ledger.summary().commissions {
            *fees.entry(currency).or_default() += amount;
        }
    }
    let fee_equivalent = fees
        .iter()
        .try_fold(Decimal::ZERO, |total, (currency, amount)| {
            Some(
                total
                    + *amount
                        * fx(
                            Currency::from_str(currency).ok()?,
                            &assets,
                            &fx_ids,
                            end - 1,
                            age,
                        )?,
            )
        });
    let target_asof = last_target_asof.filter(|at| {
        assets
            .iter()
            .filter(|a| a.score.is_some() || a.target_weight.is_some())
            .all(|a| a.target_weight.is_some_and(|(clock, _)| clock == *at))
    });
    let targets = assets
        .iter()
        .filter_map(|a| {
            a.target_weight
                .filter(|(at, _)| Some(*at) == target_asof)
                .map(|(_, weight)| (a.instrument.id().to_string(), weight))
        })
        .map(|(instrument_id, weight)| {
            Ok(contracts::portfolio::AllocationTargetV1 {
                instrument_id,
                currency: "USDT".into(),
                weight: exact(weight)?,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let target_net = targets.iter().try_fold(Decimal::ZERO, |sum, t| {
        Ok::<_, anyhow::Error>(sum + decimal(&t.weight)?)
    })?;
    let margin_summary = margin.as_ref().map(|l| l.summary()).unwrap_or_default();
    let cash_summary = cash.as_ref().map(|l| l.summary()).unwrap_or_default();
    let mut result = NativeTickEconomicResultV1 {
        schema_version: SchemaV1, request: request.clone(),
        small_capital_diagnostics: small.as_ref().map(|p| p.report(&assets)).transpose()?, model: if request.scenario.execution_model.is_some() { "SEGREGATED_SYNTHETIC_QUOTE_IOC_V1" } else { "SEGREGATED_COHORT_MARK_ESTIMATE_V1" }.into(),
        methods: BTreeMap::from([
            ("funding_source".into(), funding_manifest.as_ref().map_or_else(|| "Periodic funding is inapplicable to this CurrencyPair-only replay.".into(), |manifest| format!("Public rate catalog revision {}; source window [{}..{}), {} rows; independent immutable catalog may differ from retained price catalog; source file {}", manifest.source_revision, manifest.start_ns.get(), manifest.end_ns.get(), manifest.rows, request.funding_source))),
            ("native".into(),format!("Official nautilus-backtest/model/portfolio; core {}",nautilus_core::consts::NAUTILUS_VERSION_CORE)),
            ("execution".into(),if request.scenario.execution_model.is_some() { "Synthetic equal bid/ask quotes at derived cohort VWAP, with each side sized to participation times cohort quantity, are execution proxies and not observed quotes or book depth. One target IOC per instrument and batch; native liquidity consumption is disabled, so no shared liquidity is modeled. Legal order size is checked before native IOC partial fills, with residual positions retained. Earlier decisions execute only at a later cohort after modeled latency and jitter; ties do not establish real queue order." } else { "Derived cohort simulation bars use an engine-required nominal 1ms label at actual irregular cohort timestamps (no resampling); OHLC=VWAP quantized to native price precision; not source BAR/TradeTick/QuoteTick. Earlier-cohort target is submitted only at a later observed cohort after modeled absolute latency and seeded bounded-uniform integer-nanosecond jitter. Quantity capped by participation. Nautilus 0.67 divides BAR volume across OHLC legs in native size increments; on_bar IOC orders match liquidity remaining after that sequence and can fill less than that cap. Nonzero legs refresh the synthetic book even at unchanged prices; this is not observed market depth. Ties use deterministic native ordering, not original queue evidence." }.into()),
            ("decision_clock".into(), if fixed_clock { format!("FIXED_INTERVAL_V1. Official Nautilus VirtualTimer at start + k * {}ns through the half-open input window. Same-time source data and scores precede alpha decisions. Generic rebalances use the latest scores. Small-capital entries consume each new cohort score once, with independent holding-risk deadlines. Queued execution requires actual later source observations.", s.decision_interval_ns.get()) } else { "Legacy cohort-event decisions, capped by global interval buckets rather than a fixed timer.".into() }),
            ("policy".into(),if small.is_some() && fixed_clock {"Fixed capped USDC alpha entries and reference-price exits at native timer deadlines, using each asset latest new cohort once. Empty clock events hold without resubmitting old signals. Declared roundtrip-cost hurdle and frozen entry fastEMA remain. Timeout is measured from entry decision with the original precise risk deadline. Orders execute only at real later observations after latency, partial fills and residual inventory remain, and quote inventory is not alpha. No dynamic sleeve transfers.".into()} else if small.is_some() {"Fixed capped USDC alpha entries only on each asset first fresh signal in each global decision bucket; declared roundtrip-cost hurdle; frozen entry fastEMA; timeout measured from decision, precise deadline exit intent, real later-cohort execution and retries. One entry attempt; partial fill is held without top-up. Exits require no NAV and retain nonzero dust. FX bridge instruments and their base currencies serve wallet quote inventory only, never alpha. Real holdings, quote inventory and unfilled reservations consume gross/per-asset budgets; missing NAV blocks new risk. No dynamic sleeve transfers.".into()} else {"Fixed global rebalance grid; one outstanding delayed decision per asset, intermediate grid decisions coalesced until its next eligible observed execution attempt. Absolute score-proportional capped gross targets. Spot negatives to cash, perp signed. No covariance fabrication, optimization or dynamic transfers. Native minimum order and cash affordability apply.".into()}),
            ("capital".into(),"One initial USDT wealth explicitly mapped to USDT0. Observed delayed participation-limited bridge conversion, one split into native Cash/Margin, no running transfers or reinjection.".into()),
            ("funding".into(), if funding_manifest.is_none() { "Periodic funding is inapplicable to this CurrencyPair-only replay.".into() } else if let Some(context) = &contexts { format!("Actual rate/time times own signed position times latest nonfuture oracle context; maximum age {}ns; perp NAV/capital PnL uses the same mark context. Context: {}; event-time context is an explicitly unverified fallback only when primary data is missing or stale. Trade execution and feature prices are unchanged; the venue oracle extension books only acknowledged account-cash adjustments. Native per-position realized PnL does not include this oracle extension; scientific performance uses account equity, never position-return fallback.", age, context.description()) } else { "Original rate boundaries and source-qualified cohort marks feed official FundingRateUpdate/FundingSettlement. PositionAdjusted and native account balances own the payment; the explicit nominal USD-to-USDC module preserves the declared venue unit. This is mark funding, not HL oracle settlement.".into() }),
            ("cost".into(),"Native commission combines declared taker and extra spread/impact bps once. fees_by_currency retains native amounts; fees_usdc_equivalent is explicitly valued at final available FX, not historical per-fill FX.".into()),
            ("statistics".into(),"UTC daily total-wealth net returns analyzed by official nautilus-analysis SharpeRatio/CAGR(365), retaining real UTC day keys. Partition metrics use all observed daily returns in each partition. Selected regime bins are conditional annualization, not a continuous-calendar portfolio CAGR; missing observations are counted and never zero-filled.".into()),
            ("wire_precision".into(),"Derived decimal outputs round once to 18 fractional places, midpoint nearest even, absolute per-value error at most 0.0000000000000000005; internal native balances and Decimal arithmetic remain unchanged.".into()),
            ("precision".into(),"Native USD-quoted perp PnL uses USD 2-decimal Money, explicitly swept in nominal quote units to USDC; no instrument rewrite. Full USD minor unit 0.01 times all perp fills conservatively bounds native PnL rounding, not VWAP/price precision or execution-model error.".into()),
        ]),
        coverage: cache.coverage,
        assets: assets.iter().map(|a|Ok(NativeTickEconomicAssetV1 {
            instrument_id:a.instrument.id().to_string(), native_kind:if matches!(a.instrument,InstrumentAny::CurrencyPair(_)){"CurrencyPair"}else{"CryptoPerpetual"}.into(),
            cohorts:counter(a.cohorts)?, decisions:counter(a.decisions)?, execution_attempts:counter(a.attempts)?,
        })).collect::<Result<_>>()?,
        metrics: metrics(request,&days)?, partition_metrics: Some(partition_metrics(request, &days)?), days, conversion_ns, initial_usdc_after_cost:initial_usdc,
        funding_usdc:exact(margin_summary.adjustment_totals.get("USDC").copied().unwrap_or(Decimal::ZERO))?,
        fees_usdc_equivalent:fee_equivalent.map(exact).transpose()?,
        fees_by_currency:fees.into_iter().map(|(currency,amount)|Ok((currency,exact(amount)?))).collect::<Result<_>>()?,
        initial_conversion_cost_usdc:exact(conversion_cost)?, nominal_usd_settled_usdc:exact(margin_summary.nominal_settlement_usdc)?,
        native_fills:counter(fills)?, native_rejections:counter(rejections)?,
        native_perp_pnl_rounding_events:counter(margin_summary.fills)?, perp_pnl_rounding_bound_usdc:exact(Decimal::new(1,2)*Decimal::from(margin_summary.fills))?,
        funding_missing_instruments:funding_manifest.as_ref().map(|manifest| manifest.missing_instruments.clone()).unwrap_or_default(),
        targets, target_asof_ns:target_asof.map(counter).transpose()?,target_cash_weight:target_asof.map(|_|exact(Decimal::ONE-target_net)).transpose()?,
        skipped_cash:counter(cash_summary.skipped_cash+margin_summary.skipped_cash)?,skipped_minimum:counter(cash_summary.skipped_minimum+margin_summary.skipped_minimum)?,
        consumed_fuel:request.research.parameters.total_fuel.map(|_|counter(consumed)).transpose()?,
        limitations:vec![if target_asof.is_none(){"TARGET_SNAPSHOT_UNAVAILABLE_NO_COMPLETE_COMMON_DECISION_CLOCK"}else{"TARGET_SNAPSHOT_IS_DECISION_INTENT_NOT_FILLED_INVENTORY"}.into(),"EXPLORATORY_COST_CAPITAL_SCENARIO_NOT_GOAL_QUALIFICATION".into(),"CURRENT_DEFINITIONS_NOT_HISTORICAL_CONTRACT_RULES".into(),"COHORT_VWAP_NOT_EXECUTABLE_QUOTE_OR_CONTINUOUS_BOOK".into(),"OPEN_END_POSITIONS_VALUED_NOT_RETROSPECTIVELY_LIQUIDATED".into(),"HISTORICAL_ADL_DELISTING_BORROWING_NOT_REPLICATED".into(),"STABLE_PAIR_ALIGNED_STAKING_REFERRAL_AND_HISTORICAL_FEE_DISCOUNTS_NOT_RECONSTRUCTED".into(),"SOURCE_STREAM_BOUNDED_NATIVE_OPEN_POSITION_AUDIT_HISTORY_CAN_GROW".into(),if funding_unknown.is_some(){"FUNDING_UNAVAILABLE_NET_EQUITY_UNKNOWN"}else if funding_manifest.is_none(){"PERIODIC_FUNDING_INAPPLICABLE_TO_CURRENCY_PAIR_ASSETS"}else if contexts.is_some(){"CONTEXT_EVENT_TIME_PROXY_NOT_VALIDATED_HISTORICAL_AVAILABILITY"}else{"FUNDING_USES_PAST_COHORT_MARK_NOT_ORACLE"}.into()].into_iter().chain(funding_unknown.map(|(_, cause)| cause)).collect(),
    };
    if book_profile {
        result.methods.insert(
            "features".into(),
            super::feature_schema(&request.research).into(),
        );
    }
    if let Some(evidence) = initial_conversion_evidence {
        result
            .methods
            .insert("initial_conversion_native_evidence".into(), evidence);
    }
    if request.event_time_context_source.is_some()
        && !result
            .limitations
            .iter()
            .any(|limit| limit == "CONTEXT_EVENT_TIME_PROXY_NOT_VALIDATED_HISTORICAL_AVAILABILITY")
    {
        result
            .limitations
            .push("CONTEXT_EVENT_TIME_PROXY_NOT_VALIDATED_HISTORICAL_AVAILABILITY".into());
    }
    if observed {
        if funding_root.join("book-snapshots.source.json").is_file() {
            let (carry_snapshots, carry_rows) = book_readers.as_ref().unwrap().checkpoint_counts();
            result.coverage.source_evidence.insert("published_books".into(), serde_json::json!({
                "carry_checkpoint_snapshots": carry_snapshots.to_string(),
                "carry_checkpoint_rows": carry_rows.to_string(),
                "policy": "Only the first complete frame of an observed CryptoHFTData file, received at its file hour with transaction_time explicitly null on every row, is a carried checkpoint. It is structurally validated but does not refresh liquidity, marks or receipt age; original files and clocks are retained."
            }));
        }
        let v2 = normalize_observed_book_sizes(&request.scenario);
        result.model = if v2 {
            "SEGREGATED_OBSERVED_L2_IOC_V2"
        } else {
            "SEGREGATED_OBSERVED_L2_IOC_V1"
        }
        .into();
        result.methods.insert("execution".into(), if v2 {
            "Published complete L2 snapshots retain exchange ts_event and provider ts_init. Their first-level bid/ask price and lot-normalized size also feed the official market-order risk cache as a same-clock QuoteTick; quotes do not update the L2 matching book. Level sizes are rounded down to the currently declared lot increment before matching; source rows remain unchanged. The next complete snapshot after a signal's absolute latency plus seeded jitter supplies lot-normalized bid/ask levels to official Nautilus L2_MBP matching, with BAR and trade execution disabled and native liquidity consumption enabled. Participation is a cap on the visible side's total depth, not a measured fraction of traded volume. Each asset submits at most one delayed aggregate IOC at a snapshot; native partial fills, commissions and residual positions remain in its shared sleeve account. Cash buys and capped new exposure use the lot-normalized execution-side depth cost, with declared fees and native Money rounding accounted for. A one-sided snapshot may only reduce existing inventory through an IOC limit at the depth actually touched; it does not create a BBO or a NAV mark. Two-sided snapshots retain market IOC orders."
        } else {
            "Published complete L2 snapshots retain exchange ts_event and provider ts_init. Their actual first-level bid/ask price and size also feed the official market-order risk cache as a same-clock QuoteTick; quotes do not update the L2 matching book. The next complete snapshot after a signal's absolute latency plus seeded jitter supplies actual bid/ask levels to official Nautilus L2_MBP matching, with BAR and trade execution disabled and native liquidity consumption enabled. Participation is a cap on the visible side's total depth, not a measured fraction of traded volume. Each asset submits at most one delayed aggregate IOC at a snapshot; actual native partial fills, commissions and residual positions remain in its shared sleeve account. Cash buys and capped new exposure use the actual execution-side depth cost, with declared fees and native Money rounding accounted for. A one-sided snapshot may only reduce existing inventory through an IOC limit at the depth actually touched; it does not create a BBO or a NAV mark. Two-sided snapshots retain market IOC orders."
        }.into());
        result.methods.insert("capital".into(), "Initial USDT wealth is explicitly mapped to USDT0 in a native bootstrap cash account. Delayed real-book IOC sales convert that inventory to USDC, including native depth, size limits and fees. Realized net proceeds become one initial cash/margin split; unconverted dust remains in total wealth. No running transfers or reinjection.".into());
        result.methods.insert("policy".into(), if small.is_some() { "Capped small-capital entry intent and timeout retain the registered policy. Features advance on observed trade receipt cohorts; execution advances only on complete published book snapshots. Unexecutable size/minimum-notional exits retain their inventory and pending intent without generating native orders. A later legal snapshot may retry; dust is never called closed." } else { "Registered global decision grid and absolute score-proportional capped gross targets. Coalesced delayed targets execute against a later complete published book; spot negatives remain cash, perpetual targets are signed. Multiple model scores share one target calculation, cash balance, margin balance and matching liquidity." }.into());
        result
            .limitations
            .retain(|limit| limit != "COHORT_VWAP_NOT_EXECUTABLE_QUOTE_OR_CONTINUOUS_BOOK");
        result.limitations.extend(
            [
                "PUBLISHED_L2_SNAPSHOTS_NOT_CONTINUOUS_EXCHANGE_BOOK",
                "MILLISECOND_JITTER_IS_A_SCENARIO_NOT_MEASURED_NETWORK_OR_QUEUE_LATENCY",
                "PUBLISHED_HISTORY_CAN_OMIT_EMPTY_BOOK_RESETS_OR_MARKET_INTERVALS",
            ]
            .map(str::to_owned),
        );
        if request.research.input.uses_received_time() {
            result.limitations.retain(|limit| {
                limit != "CONTEXT_EVENT_TIME_PROXY_NOT_VALIDATED_HISTORICAL_AVAILABILITY"
            });
            result
                .limitations
                .push("PROVIDER_RECEIPT_CLOCK_NOT_LOCAL_NETWORK_OR_EXCHANGE_COMPLETENESS".into());
            result.methods.insert("funding".into(), "Published settlement rates apply to own signed quantity times the latest nonfuture observed oracle, selected by provider receipt time and the registered age limit. Context observations value perpetual NAV at observed marks; missing settlement rates or oracle coverage make net equity unknown. Context rate samples are never substituted for settlement-rate history. The oracle extension books acknowledged account cash; native position realized PnL excludes this extension, and scientific performance uses account equity.".into());
        }
    }
    if wasms.len() > 1 {
        result.methods.insert("signals".into(), "Declared positive weights sum to one and combine original model scores at each shared cohort before any target, order or account calculation. Models share the request's measured fuel allowance. This is a fixed signal combination, not a sum of separately backtested NAV curves or a covariance optimizer.".into());
    }
    if request.spot_valuation_source.is_some() {
        result.methods.insert("spot_valuation".into(), "Daily valuation uses the midpoint of the latest nonfuture provider-received spot quote when present; missing prior quotes use cohort marks, and stale, invalid or ambiguous quotes remain unavailable. Quotes do not change trading, margin or fees, and do not prove executable liquidity. Provider receive time is not local receipt time.".into());
        result
            .limitations
            .push("SPOT_QUOTE_SNAPSHOT_STALENESS_AND_PROVIDER_RECEIPT_TIME_UNVERIFIED".into());
    }
    Ok(result)
}

fn fee_schedule(s: &NativeTickEconomicScenarioV1, margin: bool) -> Result<MakerTakerFeeSchedule> {
    let extra = decimal(&s.extra_cost_bps)?;
    let fraction = |fee: &DecimalValue| -> Result<Decimal> {
        Ok((decimal(fee)? + extra) / Decimal::from(10000))
    };
    let default = fraction(if margin {
        &s.perp_taker_fee_bps
    } else {
        &s.spot_taker_fee_bps
    })?;
    let mut schedule = MakerTakerFeeSchedule::new(Decimal::ZERO, default);
    if let Some(fees) = &s.taker_fee_bps_by_instrument {
        for (id, fee) in fees {
            schedule.set_override(
                id.parse()?,
                MakerTakerFeeRates::new(Decimal::ZERO, fraction(fee)?),
            );
        }
    }
    Ok(schedule)
}

fn build(
    assets: &[Asset],
    margin: bool,
    capital: Decimal,
    s: &NativeTickEconomicScenarioV1,
) -> Result<NativeLedger> {
    let instruments = assets
        .iter()
        .filter(|a| matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)) == margin)
        .map(|a| a.instrument.clone())
        .collect();
    let fees = fee_schedule(s, margin)?;
    if observed(s) {
        let (initial_margin, maintenance_margin) = (
            decimal(&s.initial_margin_fraction)?,
            decimal(&s.maintenance_margin_fraction)?,
        );
        return if normalize_observed_book_sizes(s) {
            NativeLedger::new_observed_v2(
                instruments,
                capital,
                margin,
                fees,
                initial_margin,
                maintenance_margin,
            )
        } else {
            NativeLedger::new_observed(
                instruments,
                capital,
                margin,
                fees,
                initial_margin,
                maintenance_margin,
            )
        };
    }
    NativeLedger::new(
        instruments,
        capital,
        margin,
        fees,
        decimal(&s.initial_margin_fraction)?,
        decimal(&s.maintenance_margin_fraction)?,
        s.execution_model == Some(NativeTickEconomicExecutionModelV1::SyntheticQuoteIocV1),
    )
}

fn native_books(
    rows: Vec<NativeMark>,
    snapshots: &BTreeMap<usize, books::Snapshot>,
    assets: &[Asset],
    margin: bool,
) -> Vec<NativeBook> {
    let mut targets = rows
        .into_iter()
        .map(|row| (row.instrument_id, row))
        .collect::<BTreeMap<_, _>>();
    snapshots
        .iter()
        .filter(|(index, _)| {
            matches!(
                assets[**index].instrument,
                InstrumentAny::CryptoPerpetual(_)
            ) == margin
        })
        .map(|(_, snapshot)| {
            let target = targets.remove(&snapshot.deltas.instrument_id);
            NativeBook {
                snapshot: snapshot.deltas.clone(),
                capacity: target.as_ref().map_or(Decimal::ZERO, |row| row.capacity),
                target_quantity: target.and_then(|row| row.target_quantity),
            }
        })
        .collect()
}

fn append_day(
    days: &mut Vec<NativeTickEconomicDayV1>,
    day: u64,
    at: u64,
    previous: &mut Option<Decimal>,
    cash: &Option<NativeLedger>,
    margin: &Option<NativeLedger>,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    s: &NativeTickEconomicScenarioV1,
    remaining: Decimal,
    converted: Decimal,
    reserve_usdc: Decimal,
    funding_unknown: bool,
    contexts: &mut Option<ContextPrices>,
    quotes: Option<&SpotQuotes>,
) -> Result<()> {
    let age = s.maximum_mark_age_ns.get();
    let usdt0 = Currency::from_str("USDT0")?;
    let mut missing_balances = Vec::new();
    for (name, ledger) in [
        (NativeTickBalanceLedgerV1::Cash, cash),
        (NativeTickBalanceLedgerV1::Margin, margin),
    ] {
        if let Some(ledger) = ledger {
            for balance in ledger.balances() {
                if !balance.as_decimal().is_zero()
                    && valuation_fx(balance.currency, assets, fx_ids, at, age, quotes).is_none()
                {
                    missing_balances.push(NativeTickMissingBalanceV1 {
                        ledger: name,
                        currency: balance.currency.to_string(),
                        amount: balance.as_decimal().to_string(),
                    });
                }
            }
        }
    }
    missing_balances.sort_by(|a, b| (&a.ledger, &a.currency).cmp(&(&b.ledger, &b.currency)));
    let mut reason = String::new();
    let equity = if funding_unknown {
        reason = "FUNDING_UNAVAILABLE".into();
        None
    } else if let Some(c) = cash {
        match (
            valuation_value(
                c,
                assets,
                fx_ids,
                at,
                age,
                false,
                contexts,
                Some(&mut reason),
                quotes,
            )?,
            margin
                .as_ref()
                .map(|m| {
                    valuation_value(
                        m,
                        assets,
                        fx_ids,
                        at,
                        age,
                        true,
                        contexts,
                        Some(&mut reason),
                        quotes,
                    )
                })
                .transpose()?
                .unwrap_or(Some(Decimal::ZERO)),
            valuation_fx(usdt0, assets, fx_ids, at, age, quotes),
        ) {
            (Some(c), Some(m), Some(fx)) => {
                Some(((c + m + reserve_usdc) / fx + remaining) / decimal(&s.usdt0_per_usdt)?)
            }
            _ => {
                if reason.is_empty() {
                    reason = format!(
                        "REPORTING_FX_UNAVAILABLE:{}",
                        if quotes.is_some() {
                            format!("currency={usdt0}:source=SPOT_QUOTE_OR_COHORT_FX:at_ns={at}:maximum_age_ns={age}:state=UNAVAILABLE")
                        } else {
                            missing_fx_reason(usdt0, assets, fx_ids, at, age)
                        }
                    );
                }
                None
            }
        }
    } else if converted.is_zero() {
        Some(decimal(&s.initial_usdt)?)
    } else {
        let rate = valuation_fx(usdt0, assets, fx_ids, at, age, quotes);
        if rate.is_none() {
            reason = format!(
                "REPORTING_FX_UNAVAILABLE:{}",
                if quotes.is_some() {
                    format!("currency={usdt0}:source=SPOT_QUOTE_OR_COHORT_FX:at_ns={at}:maximum_age_ns={age}:state=UNAVAILABLE")
                } else {
                    missing_fx_reason(usdt0, assets, fx_ids, at, age)
                }
            );
        }
        rate.map(|price| (remaining + converted / price) / decimal(&s.usdt0_per_usdt).unwrap())
    };
    let net_return = match (equity, *previous) {
        (Some(now), Some(before)) if before > Decimal::ZERO => {
            ((now / before) - Decimal::ONE).to_f64()
        }
        _ => None,
    };
    *previous = equity;
    days.push(NativeTickEconomicDayV1 {
        day_start_ns: counter(day)?,
        equity_usdt: equity.map(exact).transpose()?,
        net_return,
        unknown_reason: equity.is_none().then_some(reason),
        missing_balances,
    });
    Ok(())
}

fn schedule(
    assets: &mut [Asset],
    cash: &Option<NativeLedger>,
    margin: &Option<NativeLedger>,
    fx_ids: &BTreeSet<String>,
    event: u64,
    s: &NativeTickEconomicScenarioV1,
    seed: &mut u64,
    contexts: &mut Option<ContextPrices>,
) -> Result<()> {
    let age = s.maximum_mark_age_ns.get();
    let c = value(
        cash.as_ref().unwrap(),
        assets,
        fx_ids,
        event,
        age,
        false,
        contexts,
        None,
    )?;
    let m = margin
        .as_ref()
        .map(|m| value(m, assets, fx_ids, event, age, true, contexts, None))
        .transpose()?
        .unwrap_or(Some(Decimal::ZERO));
    let sums = assets
        .iter()
        .filter(|a| mark(a, event, age).is_some())
        .fold([0.0; 2], |mut totals, a| {
            let margin = usize::from(matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)));
            totals[margin] += a
                .score
                .unwrap_or(0.0)
                .max(if margin == 1 { f64::NEG_INFINITY } else { 0.0 })
                .abs();
            totals
        });
    ensure!(
        sums.iter().all(|v| v.is_finite()),
        "ECONOMIC_SCORE_SUM_NONFINITE"
    );
    let mut plans = Vec::new();
    for (i, a) in assets.iter().enumerate() {
        let margin = usize::from(matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)));
        let Some(capital) = (if margin == 1 { m } else { c }) else {
            continue;
        };
        let Some(price) = mark(a, event, age) else {
            continue;
        };
        let Some(score) = a.score else { continue };
        let score = if margin == 0 { score.max(0.0) } else { score };
        let weight = (if sums[margin] == 0.0 {
            Decimal::ZERO
        } else {
            finite(score.abs() / sums[margin])?
        } * decimal(&s.gross_fraction)?)
        .min(decimal(&s.maximum_asset_weight)?)
            * if score < 0.0 {
                -Decimal::ONE
            } else {
                Decimal::ONE
            };
        let quote_fx = if margin == 1 {
            Some(Decimal::ONE)
        } else {
            fx(a.instrument.quote_currency(), assets, fx_ids, event, age)
        };
        let Some(quote_fx) = quote_fx else { continue };
        let quantity = capital.max(Decimal::ZERO) * weight / quote_fx / price;
        plans.push((i, quantity));
    }
    // Quote inventories are funded through their actual bridge, never credited for free.
    let mut quote_needs = BTreeMap::<String, (Currency, Decimal)>::new();
    for (i, q) in &plans {
        let a = &assets[*i];
        if matches!(a.instrument, InstrumentAny::CurrencyPair(_))
            && a.instrument.quote_currency() != Currency::USDC()
        {
            let held = cash
                .as_ref()
                .unwrap()
                .positions()
                .into_iter()
                .filter(|p| p.instrument_id == a.instrument.id())
                .map(|p| p.signed_decimal_qty())
                .sum::<Decimal>();
            let required = (*q - held).max(Decimal::ZERO)
                * mark(a, event, age).unwrap()
                * (Decimal::ONE + cash.as_ref().unwrap().cost_fraction(a.instrument.id()));
            let quote = a.instrument.quote_currency();
            let entry = quote_needs
                .entry(quote.to_string())
                .or_insert((quote, Decimal::ZERO));
            entry.1 += required;
        }
    }
    for (_, (currency, need)) in quote_needs {
        if let Some(i) = assets.iter().position(|a| {
            fx_ids.contains(&a.instrument.id().to_string())
                && a.instrument.base_currency() == Some(currency)
        }) {
            let old = plans.iter().position(|(idx, _)| *idx == i);
            let alpha = old.map(|p| plans.remove(p).1).unwrap_or(Decimal::ZERO);
            // Reserve is a target cash inventory in the quote coin, acquired at a later source event.
            plans.push((i, alpha.max(need)));
        }
    }
    // Reserve FX targets remain inside the same predeclared gross and per-asset caps.
    for sleeve in [false, true] {
        let capital = if sleeve { m } else { c }
            .unwrap_or(Decimal::ZERO)
            .max(Decimal::ZERO);
        let mut gross = Decimal::ZERO;
        for (i, q) in &mut plans {
            let a = &assets[*i];
            if matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)) != sleeve {
                continue;
            }
            let rate = if sleeve {
                Some(Decimal::ONE)
            } else {
                fx(a.instrument.quote_currency(), assets, fx_ids, event, age)
            };
            let Some(unit) = mark(a, event, age).zip(rate).map(|(p, r)| p * r) else {
                continue;
            };
            let maximum = capital * decimal(&s.maximum_asset_weight)? / unit;
            *q = q.abs().min(maximum)
                * if *q < Decimal::ZERO {
                    -Decimal::ONE
                } else {
                    Decimal::ONE
                };
            gross += q.abs() * unit;
        }
        let allowed = capital * decimal(&s.gross_fraction)?;
        if gross > allowed {
            let scale = allowed / gross;
            for (i, q) in &mut plans {
                if matches!(assets[*i].instrument, InstrumentAny::CryptoPerpetual(_)) == sleeve {
                    *q *= scale;
                }
            }
        }
    }
    // This scenario closes under-margined positions at later observed events, not at hindsight prices.
    let mut maintenance = Decimal::ZERO;
    for p in margin.iter().flat_map(|m| m.positions()) {
        let Some(a) = assets.iter().find(|a| a.instrument.id() == p.instrument_id) else {
            continue;
        };
        if let Some(price) = mark(a, event, age) {
            maintenance +=
                p.quantity.as_decimal() * price * decimal(&s.maintenance_margin_fraction)?;
        }
    }
    if m.is_some_and(|net| net <= maintenance) && maintenance > Decimal::ZERO {
        plans.retain(|(i, _)| !matches!(assets[*i].instrument, InstrumentAny::CryptoPerpetual(_)));
        for p in margin.iter().flat_map(|m| m.positions()) {
            if let Some(i) = assets
                .iter()
                .position(|a| a.instrument.id() == p.instrument_id)
            {
                plans.push((i, Decimal::ZERO));
            }
        }
    }
    for (i, q) in plans {
        // One outstanding decision per asset survives sparse market arrivals.
        // Intermediate grid decisions are coalesced until that attempt is consumed.
        if assets[i].pending.is_some() {
            continue;
        }
        let due = event
            .checked_add(delay(seed, s)?)
            .and_then(|v| v.checked_add(1))
            .context("ECONOMIC_CLOCK_OVERFLOW")?;
        let quote_rate = if matches!(assets[i].instrument, InstrumentAny::CryptoPerpetual(_)) {
            Some(Decimal::ONE)
        } else {
            fx(
                assets[i].instrument.quote_currency(),
                assets,
                fx_ids,
                event,
                age,
            )
        };
        let total = c.zip(m).map(|(c, m)| c + m);
        assets[i].target_weight = quote_rate
            .zip(total)
            .filter(|(_, total)| *total > Decimal::ZERO)
            .and_then(|(rate, total)| {
                mark(&assets[i], event, age).map(|p| (event, q * p * rate / total))
            });
        assets[i].pending = Some((event, due, q));
        assets[i].decisions += 1;
    }
    Ok(())
}

fn daily_statistics(days: &[&NativeTickEconomicDayV1]) -> (usize, Option<f64>, Option<f64>) {
    use nautilus_analysis::{
        statistic::PortfolioStatistic,
        statistics::{cagr::CAGR, sharpe_ratio::SharpeRatio},
        Returns,
    };
    use nautilus_core::UnixNanos;
    let returns: Returns = days
        .iter()
        .filter_map(|day| {
            day.net_return
                .map(|value| (UnixNanos::from(day.day_start_ns.get()), value))
        })
        .collect();
    let n = returns.len();
    let sharpe = SharpeRatio::new(Some(365))
        .calculate_from_returns(&returns)
        .filter(|value| value.is_finite());
    let annualized = (n > 0 && returns.values().all(|value| *value > -1.0))
        .then(|| CAGR::new(Some(365)).calculate_from_returns(&returns))
        .flatten()
        .filter(|value| value.is_finite());
    (n, sharpe, annualized)
}

fn partition_metrics(
    request: &NativeTickEconomicRequestV1,
    days: &[NativeTickEconomicDayV1],
) -> Result<Vec<NativeTickEconomicPartitionMetricV1>> {
    use contracts::factors::NativeFactorPartitionV1;
    [
        NativeFactorPartitionV1::Train,
        NativeFactorPartitionV1::Validation,
        NativeFactorPartitionV1::Heldout,
    ]
    .into_iter()
    .map(|partition| {
        let selected = days
            .iter()
            .filter(|day| {
                domain::ticks::partition_for_event(&request.research, day.day_start_ns)
                    == Some(partition)
            })
            .collect::<Vec<_>>();
        let (n, sharpe, annualized) = daily_statistics(&selected);
        Ok(NativeTickEconomicPartitionMetricV1 {
            partition,
            days: counter(selected.len() as u64)?,
            valid_return_days: counter(n as u64)?,
            sharpe_365: sharpe,
            annualized_return_365: annualized,
        })
    })
    .collect()
}

fn metrics(
    request: &NativeTickEconomicRequestV1,
    days: &[NativeTickEconomicDayV1],
) -> Result<Vec<NativeTickEconomicMetricV1>> {
    use contracts::{factors::NativeFactorPartitionV1, ticks::NativeTickRegimeV1};
    let regimes = domain::ticks::evaluation_regimes(&request.research)?;
    let first = request.research.input.event_start_ns.get() / DAY * DAY;
    let mut reports = Vec::new();
    for partition in [
        NativeFactorPartitionV1::Train,
        NativeFactorPartitionV1::Validation,
        NativeFactorPartitionV1::Heldout,
    ] {
        for regime in [
            NativeTickRegimeV1::Bull,
            NativeTickRegimeV1::Bear,
            NativeTickRegimeV1::Range,
            NativeTickRegimeV1::Unknown,
        ] {
            let selected = days
                .iter()
                .filter(|d| {
                    domain::ticks::partition_for_event(&request.research, d.day_start_ns)
                        == Some(partition)
                        && regimes
                            .get(((d.day_start_ns.get() - first) / DAY) as usize)
                            .copied()
                            .unwrap_or(NativeTickRegimeV1::Unknown)
                            == regime
                })
                .collect::<Vec<_>>();
            // Preserve real UTC-day identities, including gaps between regime
            // samples. Native CAGR annualizes the selected daily-bin count; it
            // is a conditional regime statistic, not contiguous-calendar CAGR.
            let (n, sharpe, annualized) = daily_statistics(&selected);
            reports.push(NativeTickEconomicMetricV1 {
                partition,
                regime,
                days: counter(selected.len() as u64)?,
                valid_return_days: counter(n as u64)?,
                sharpe_365: sharpe,
                conditional_annualized_return_365: annualized,
            });
        }
    }
    Ok(reports)
}

#[cfg(test)]
mod replay_tests;
#[cfg(test)]
mod tests;

fn conversion_quantity(
    remaining: Decimal,
    capacity: Decimal,
    price: Decimal,
    instrument: &InstrumentAny,
) -> Result<Decimal> {
    let step = instrument.size_increment().as_decimal();
    let amount = (remaining.min(capacity) / step).trunc() * step;
    if amount <= Decimal::ZERO
        || instrument
            .min_quantity()
            .is_some_and(|min| amount < min.as_decimal())
        || instrument
            .min_notional()
            .is_some_and(|min| amount * price < min.as_decimal())
    {
        return Ok(Decimal::ZERO);
    }
    Ok(amount)
}
