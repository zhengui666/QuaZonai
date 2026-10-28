//! Explicit derived-cohort execution estimate; no source trades/quotes are invented.
use super::{cohort_cache, feature, Cohort};
use crate::{feature_model::FeatureModel, signals::SignalModule};
use anyhow::{ensure, Context, Result};
use contracts::{economic_tick::*, DbCounter, DecimalValue, SchemaV1};
use nautilus_indicators::{
    average::ema::ExponentialMovingAverage,
    indicator::{Indicator, MovingAverage},
};
use nautilus_model::{
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

mod native;
use native::{NativeLedger, NativeMark, NativeRate};
const DAY: u64 = 86_400_000_000_000;
const HOUR: u64 = DAY / 24;

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
    for fee in [&s.perp_taker_fee_bps, &s.spot_taker_fee_bps] {
        ensure!(
            decimal(fee)? >= Decimal::ZERO
                && decimal(&s.extra_cost_bps)? >= Decimal::ZERO
                && decimal(fee)? + decimal(&s.extra_cost_bps)? < Decimal::from(10000),
            "ECONOMIC_COST_INVALID"
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
    let ids = &request.research.input.instrument_ids;
    ensure!(
        !request.fx_instrument_ids.is_empty()
            && request.fx_instrument_ids.iter().all(|id| ids.contains(id))
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
struct Funding {
    reader: BufReader<File>,
    next: Option<Rate>,
    last: Option<(u64, String)>,
    rows: u64,
    instruments: BTreeSet<String>,
}
impl Funding {
    fn open(path: &Path, instruments: BTreeSet<String>) -> Result<Self> {
        let mut s = Self {
            reader: BufReader::new(File::open(path)?),
            next: None,
            last: None,
            rows: 0,
            instruments,
        };
        s.advance()?;
        Ok(s)
    }
    fn advance(&mut self) -> Result<()> {
        let mut line = String::new();
        let n = self.reader.by_ref().take(4097).read_line(&mut line)?;
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
    unknown: &mut Option<String>,
    seen: &mut BTreeSet<(String, u64)>,
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
                        if let Some(price) = mark(asset, at, age) {
                            let amount = -p.quantity.as_decimal()
                                * if p.side == nautilus_model::enums::PositionSide::Short {
                                    -Decimal::ONE
                                } else {
                                    Decimal::ONE
                                }
                                * price
                                * decimal(&r.rate)?;
                            ledger.queue_adjustment(
                                at,
                                Money::from_decimal(amount, Currency::USDC())?,
                            )?;
                        } else {
                            unknown.get_or_insert_with(|| format!(
                                "FUNDING_FIRST_CAUSE:STALE_OR_MISSING_MARK:event_ns={at}:instrument_id={}:last_mark_ns={}",
                                r.instrument_id,
                                asset.mark.map(|(at, _)| at.to_string()).unwrap_or_else(|| "NONE".into())
                            ));
                        }
                    }
                }
                native.push(NativeRate {
                    instrument_id: asset.instrument.id(),
                    rate: decimal(&r.rate)?,
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
    unknown: &mut Option<String>,
    seen: &mut BTreeSet<(String, u64)>,
) -> Result<()> {
    while let Some(at) = funding
        .next
        .as_ref()
        .map(|r| r.event_ns.get())
        .filter(|at| *at < limit)
    {
        let rows = rates_at(funding, ledger, assets, at, start, age, unknown, seen)?;
        if let Some(ledger) = ledger.as_mut() {
            if !rows.is_empty() {
                ledger.funding_batch(at, &rows)?;
            }
        }
    }
    Ok(())
}
fn missing_rates_before(
    end: u64,
    expected: &mut BTreeSet<(String, u64)>,
    seen: &mut BTreeSet<(String, u64)>,
) -> Option<String> {
    let limit = end / HOUR;
    let missing = expected
        .iter()
        .find(|(id, hour)| *hour < limit && !seen.contains(&(id.clone(), *hour)))
        .map(|(id, hour)| format!("FUNDING_FIRST_CAUSE:MISSING_RATE:event_ns={end}:instrument_id={id}:rate_hour={hour}"));
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
    model: Option<FeatureModel>,
    score: Option<f64>,
    mark: Option<(u64, Decimal)>,
    cohorts: u64,
    decisions: u64,
    attempts: u64,
    pending: Option<(u64, u64, Decimal)>,
    target_weight: Option<(u64, Decimal)>,
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
fn stable_fx(
    currency: Currency,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
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
        .and_then(|a| mark(a, now, age))
}
fn fx(
    currency: Currency,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
) -> Option<Decimal> {
    if let Some(rate) = stable_fx(currency, assets, fx_ids, now, age) {
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
                mark(a, now, age)?
                    * stable_fx(a.instrument.quote_currency(), assets, fx_ids, now, age)?,
            )
        })
}
fn value(
    ledger: &NativeLedger,
    assets: &[Asset],
    fx_ids: &BTreeSet<String>,
    now: u64,
    age: u64,
    margin: bool,
) -> Result<Option<Decimal>> {
    let mut total = Decimal::ZERO;
    for balance in ledger.balances() {
        let amount = balance.as_decimal();
        if amount.is_zero() {
            continue;
        }
        let Some(rate) = fx(balance.currency, assets, fx_ids, now, age) else {
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
            let Some(price) = mark(asset, now, age) else {
                return Ok(None);
            };
            let pnl = position.try_calculate_pnl(
                position.avg_px_open,
                price.to_f64().context("ECONOMIC_NONFINITE")?,
                position.quantity,
            )?;
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

/// Preserve the optional original funding sidecars through native preparation.
/// Decoding remains the economic runner's job; preparation does not qualify rates.
pub(crate) fn funding_sources(
    root: &Path,
    request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
) -> Result<Vec<(String, u64)>> {
    let names = ["funding-rates.jsonl", "funding-rates.manifest.json"];
    if names.iter().all(|name| !root.join(name).exists()) {
        return Ok(Vec::new());
    }
    let mut files = Vec::new();
    for name in names {
        let state = std::fs::symlink_metadata(root.join(name))?;
        ensure!(
            state.is_file() && !state.file_type().is_symlink(),
            "ECONOMIC_FUNDING_REGULAR_SOURCE_REQUIRED"
        );
        files.push((name.into(), state.len()));
    }
    ensure!(
        files[1].1 <= 16 * 1024 * 1024,
        "ECONOMIC_FUNDING_MANIFEST_LIMIT"
    );
    let manifest: FundingManifest = serde_json::from_reader(File::open(root.join(names[1]))?)?;
    ensure!(
        manifest.source_revision == request.input.source_revision
            && manifest.start_ns <= request.input.event_start_ns
            && manifest.end_ns >= request.input.event_end_ns,
        "ECONOMIC_FUNDING_COVERAGE_INVALID"
    );
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
    validate(request)?;
    ensure!(
        !funding_sources(funding_root, &request.research)?.is_empty(),
        "ECONOMIC_FUNDING_SOURCE_REQUIRED"
    );
    let cache = cohort_cache::load(root, &request.research)?;
    let mut defs = definitions(root)?
        .into_iter()
        .map(|i| (i.id().to_string(), i))
        .collect::<BTreeMap<_, _>>();
    let module = SignalModule::new(wasm, request.research.parameters.total_fuel.is_some())?;
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
        let model = if a.cohort_count == 0 {
            None
        } else {
            let model = FeatureModel::new(&module, Some(a.cohort_count), fuel)?;
            if let (Some(before), Some(after)) = (fuel, model.remaining_fuel()) {
                consumed += before - after;
            }
            Some(model)
        };
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
    let funding_manifest: FundingManifest = serde_json::from_reader(File::open(
        funding_root.join("funding-rates.manifest.json"),
    )?)?;
    let funding_ids = funding_manifest
        .instruments
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    ensure!(
        funding_manifest.source_revision == request.research.input.source_revision
            && funding_manifest.start_ns <= request.research.input.event_start_ns
            && funding_manifest.end_ns >= request.research.input.event_end_ns
            && funding_ids.len() == funding_manifest.instruments.len()
            && assets
                .iter()
                .filter(|a| matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)))
                .all(|a| funding_ids.contains(&a.instrument.id().to_string())),
        "ECONOMIC_FUNDING_COVERAGE_INVALID"
    );
    let missing_funding = funding_manifest
        .missing_instruments
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut funding = Funding::open(&funding_root.join(&request.funding_source), funding_ids)?;
    let mut seed = s.random_seed.get();
    let mut cash: Option<NativeLedger> = None;
    let mut margin: Option<NativeLedger> = None;
    let mut cash_started = false;
    let mut margin_started = false;
    let mut funding_unknown = None;
    let mut remaining = decimal(&s.initial_usdt)? * decimal(&s.usdt0_per_usdt)?;
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
    while let Some(Reverse((event, index))) = heap.pop() {
        // Complete each global timestamp before scheduling new decisions.
        let mut group = vec![index];
        while heap.peek().is_some_and(|Reverse((at, _))| *at == event) {
            group.push(heap.pop().unwrap().0 .1);
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
            )?;
            if let Some(cause) = missing_rates_before(day_end, &mut expected_rates, &mut seen_rates) {
                funding_unknown.get_or_insert(cause);
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
        )?;
        let mut marks_cash = Vec::new();
        let mut marks_margin = Vec::new();
        for &i in &group {
            let a = &mut assets[i];
            let current = a.head.take().unwrap();
            a.fast.update_raw(current.vwap);
            a.slow.update_raw(current.vwap);
            if let Some(previous) = &a.previous {
                if a.fast.initialized() && a.slow.initialized() {
                    let ordinal = u32::try_from(a.cohorts)?;
                    let inputs = [
                        current.vwap,
                        previous.vwap,
                        a.fast.value(),
                        a.slow.value(),
                        current.quantity,
                        current.count as f64,
                        (current.event - previous.event) as f64,
                        current.signed_quantity.context("ECONOMIC_FLOW_REQUIRED")?,
                    ];
                    let features = inputs
                        .into_iter()
                        .map(|v| feature(v, current.event, ordinal))
                        .collect::<Result<Vec<_>>>()?;
                    let model = a.model.as_mut().context("ECONOMIC_MODEL_MISSING")?;
                    if let Some(total) = request.research.parameters.total_fuel {
                        model.cap_remaining_fuel(total.get() - consumed)?;
                    }
                    let before = model.remaining_fuel();
                    a.score = Some(model.predict(event, event, ordinal, &features)?);
                    if let (Some(before), Some(after)) = (before, model.remaining_fuel()) {
                        consumed = consumed
                            .checked_add(before - after)
                            .context("ECONOMIC_FUEL_RANGE")?;
                        ensure!(
                            consumed <= request.research.parameters.total_fuel.unwrap().get(),
                            "FEATURE_MODEL_BUDGET_EXHAUSTED"
                        );
                    }
                }
            }
            let target = if a
                .pending
                .is_some_and(|(decision, due, _)| decision < event && due <= event)
            {
                a.attempts += 1;
                a.pending.take().map(|(_, _, q)| q)
            } else {
                None
            };
            let native = NativeMark {
                instrument_id: a.instrument.id(),
                mark: current.vwap,
                capacity: finite(current.quantity)? * decimal(&s.participation_fraction)?,
                target_quantity: target,
            };
            if matches!(a.instrument, InstrumentAny::CurrencyPair(_)) {
                if native.target_quantity.is_some() || !cash_started {
                    marks_cash.push(native);
                }
            } else if native.target_quantity.is_some() || !margin_started {
                marks_margin.push(native);
            }

            a.mark = Some((event, finite(current.vwap)?));
            a.cohorts += 1;
            a.previous = Some(current);
            a.head = a.reader.next().transpose()?;
            if let Some(h) = &a.head {
                heap.push(Reverse((h.event, i)));
            }
        }
        if cash.is_none() && group.contains(&bridge) {
            let price = assets[bridge].mark.unwrap().1;
            if conversion_due.is_none() {
                conversion_due = Some(
                    event
                        .checked_add(delay(&mut seed, s)?)
                        .and_then(|t| t.checked_add(1))
                        .context("ECONOMIC_CLOCK_OVERFLOW")?,
                );
            } else if conversion_due.is_some_and(|at| event >= at) {
                let capacity = finite(assets[bridge].previous.as_ref().unwrap().quantity)?
                    * decimal(&s.participation_fraction)?;
                let quantity =
                    conversion_quantity(remaining, capacity, price, &assets[bridge].instrument)?;
                converted += quantity
                    * price
                    * (Decimal::ONE
                        - (decimal(&s.spot_taker_fee_bps)? + decimal(&s.extra_cost_bps)?)
                            / Decimal::from(10000));
                conversion_cost += quantity
                    * price
                    * (decimal(&s.spot_taker_fee_bps)? + decimal(&s.extra_cost_bps)?)
                    / Decimal::from(10000);
                remaining -= quantity;
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
                    margin = Some(build(&assets, true, margin_capital, s)?);
                    conversion_ns = Some(counter(event)?);
                    initial_usdc = Some(exact(converted)?);
                }
            }
        }
        if let Some(ledger) = cash.as_mut() {
            if !marks_cash.is_empty() {
                ledger.step_batch(event, &marks_cash)?;
                cash_started = true;
            }
        }
        if let Some(ledger) = margin.as_mut() {
            if !marks_margin.is_empty() || !rates.is_empty() {
                ledger.step_batch_with_rates(event, &marks_margin, &rates)?;
                margin_started = true;
                for id in ledger.open_instrument_ids() {
                    let id = id.to_string();
                    if missing_funding.contains(&id) {
                        funding_unknown.get_or_insert_with(|| format!(
                            "FUNDING_FIRST_CAUSE:MISSING_INSTRUMENT:event_ns={event}:instrument_id={id}"
                        ));
                    }
                    expected_rates.insert((id, event / HOUR));
                }
            }
        }
        if event >= next_grid && cash.is_some() {
            schedule(&mut assets, &cash, &margin, &fx_ids, event, s, &mut seed)?;
            last_target_asof = Some(event);
            next_grid = (event / s.decision_interval_ns.get() + 1)
                .checked_mul(s.decision_interval_ns.get())
                .context("ECONOMIC_CLOCK_OVERFLOW")?;
        }
    }
    let end = request.research.input.event_end_ns.get();
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
        )?;
        if let Some(cause) = missing_rates_before(day_end, &mut expected_rates, &mut seen_rates) {
            funding_unknown.get_or_insert(cause);
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
    )?;
    while funding.next.is_some() {
        funding.advance()?;
    }
    ensure!(
        funding.rows == funding_manifest.rows,
        "ECONOMIC_FUNDING_ROW_COUNT_MISMATCH"
    );
    if let Some(c) = cash.as_mut() {
        c.finish()?;
    }
    if let Some(m) = margin.as_mut() {
        m.finish()?;
    }
    let (fills, rejections) = cash.iter().chain(margin.iter()).fold((0, 0), |(f, r), l| {
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
    Ok(NativeTickEconomicResultV1 {
        schema_version: SchemaV1, request: request.clone(), model: "SEGREGATED_COHORT_MARK_ESTIMATE_V1".into(),
        methods: BTreeMap::from([
            ("funding_source".into(), format!("Public rate catalog revision {}; source window [{}..{}), {} rows; independent immutable catalog may differ from retained price catalog; source file {}", funding_manifest.source_revision, funding_manifest.start_ns.get(), funding_manifest.end_ns.get(), funding_manifest.rows, request.funding_source)),
            ("native".into(),format!("Official nautilus-backtest/model/portfolio; core {}",nautilus_core::consts::NAUTILUS_VERSION_CORE)),
            ("execution".into(),"Derived cohort simulation bars use an engine-required nominal 1ms label at actual irregular cohort timestamps (no resampling); OHLC=VWAP quantized to native price precision; not source BAR/TradeTick/QuoteTick. Earlier-cohort target is submitted only at a later observed cohort after modeled absolute latency and seeded bounded-uniform integer-nanosecond jitter. Quantity capped by participation; native bar liquidity can fill less, and unchanged quantized marks do not imply a refreshed observed book. Ties use deterministic native ordering, not original queue evidence.".into()),
            ("policy".into(),"Fixed global rebalance grid; one outstanding delayed decision per asset, intermediate grid decisions coalesced until its next eligible observed execution attempt. Absolute score-proportional capped gross targets. Spot negatives to cash, perp signed. No covariance fabrication, optimization or dynamic transfers. Native minimum order and cash affordability apply.".into()),
            ("capital".into(),"One initial USDT wealth explicitly mapped to USDT0. Observed delayed participation-limited bridge conversion, one split into native Cash/Margin, no running transfers or reinjection.".into()),
            ("funding".into(),"Actual rate/time times own signed position times preceding observed cohort mark. Estimated mark funding, not HL oracle settlement; native funding disabled. Only acknowledged adjustments enter reported funding total.".into()),
            ("cost".into(),"Native commission combines declared taker and extra spread/impact bps once. fees_by_currency retains native amounts; fees_usdc_equivalent is explicitly valued at final available FX, not historical per-fill FX.".into()),
            ("statistics".into(),"UTC daily total-wealth net returns; risk-free zero, sample-standard-deviation Sharpe times sqrt(365). Regime conditional annualization exp(365*mean(log(1+r)))-1, not concatenated-calendar CAGR; missing returns excluded and counted.".into()),
            ("wire_precision".into(),"Derived decimal outputs round once to 18 fractional places, midpoint nearest even, absolute per-value error at most 0.0000000000000000005; internal native balances and Decimal arithmetic remain unchanged.".into()),
            ("precision".into(),"Native USD-quoted perp PnL uses USD 2-decimal Money, explicitly swept in nominal quote units to USDC; no instrument rewrite. Full USD minor unit 0.01 times all perp fills conservatively bounds native PnL rounding, not VWAP/price precision or execution-model error.".into()),
        ]),
        coverage: cache.coverage,
        assets: assets.iter().map(|a|Ok(NativeTickEconomicAssetV1 {
            instrument_id:a.instrument.id().to_string(), native_kind:if matches!(a.instrument,InstrumentAny::CurrencyPair(_)){"CurrencyPair"}else{"CryptoPerpetual"}.into(),
            cohorts:counter(a.cohorts)?, decisions:counter(a.decisions)?, execution_attempts:counter(a.attempts)?,
        })).collect::<Result<_>>()?,
        metrics: metrics(request,&days)?, days, conversion_ns, initial_usdc_after_cost:initial_usdc,
        funding_usdc:exact(margin_summary.adjustment_totals.get("USDC").copied().unwrap_or(Decimal::ZERO))?,
        fees_usdc_equivalent:fee_equivalent.map(exact).transpose()?,
        fees_by_currency:fees.into_iter().map(|(currency,amount)|Ok((currency,exact(amount)?))).collect::<Result<_>>()?,
        initial_conversion_cost_usdc:exact(conversion_cost)?, nominal_usd_settled_usdc:exact(margin_summary.nominal_settlement_usdc)?,
        native_fills:counter(fills)?, native_rejections:counter(rejections)?,
        native_perp_pnl_rounding_events:counter(margin_summary.fills)?, perp_pnl_rounding_bound_usdc:exact(Decimal::new(1,2)*Decimal::from(margin_summary.fills))?,
        funding_missing_instruments:funding_manifest.missing_instruments,
        targets, target_asof_ns:target_asof.map(counter).transpose()?,target_cash_weight:target_asof.map(|_|exact(Decimal::ONE-target_net)).transpose()?,
        skipped_cash:counter(cash_summary.skipped_cash+margin_summary.skipped_cash)?,skipped_minimum:counter(cash_summary.skipped_minimum+margin_summary.skipped_minimum)?,
        consumed_fuel:request.research.parameters.total_fuel.map(|_|counter(consumed)).transpose()?,
        limitations:vec![if target_asof.is_none(){"TARGET_SNAPSHOT_UNAVAILABLE_NO_COMPLETE_COMMON_DECISION_CLOCK"}else{"TARGET_SNAPSHOT_IS_DECISION_INTENT_NOT_FILLED_INVENTORY"}.into(),"EXPLORATORY_COST_CAPITAL_SCENARIO_NOT_GOAL_QUALIFICATION".into(),"CURRENT_DEFINITIONS_NOT_HISTORICAL_CONTRACT_RULES".into(),"COHORT_VWAP_NOT_EXECUTABLE_QUOTE_OR_CONTINUOUS_BOOK".into(),"OPEN_END_POSITIONS_VALUED_NOT_RETROSPECTIVELY_LIQUIDATED".into(),"HISTORICAL_ADL_DELISTING_BORROWING_NOT_REPLICATED".into(),"STABLE_PAIR_ALIGNED_STAKING_REFERRAL_AND_HISTORICAL_FEE_DISCOUNTS_NOT_RECONSTRUCTED".into(),"SOURCE_STREAM_BOUNDED_NATIVE_OPEN_POSITION_AUDIT_HISTORY_CAN_GROW".into(),"MISSING_RATE_CHECK_CONSERVATIVELY_INCLUDES_POST_BOUNDARY_ENTRIES".into(),if funding_unknown.is_some(){"FUNDING_UNAVAILABLE_NET_EQUITY_UNKNOWN"}else{"FUNDING_USES_PAST_COHORT_MARK_NOT_ORACLE"}.into()].into_iter().chain(funding_unknown).collect(),
    })
}

fn build(
    assets: &[Asset],
    margin: bool,
    capital: Decimal,
    s: &NativeTickEconomicScenarioV1,
) -> Result<NativeLedger> {
    NativeLedger::new(
        assets
            .iter()
            .filter(|a| matches!(a.instrument, InstrumentAny::CryptoPerpetual(_)) == margin)
            .map(|a| a.instrument.clone())
            .collect(),
        capital,
        margin,
        decimal(if margin {
            &s.perp_taker_fee_bps
        } else {
            &s.spot_taker_fee_bps
        })?,
        decimal(&s.extra_cost_bps)?,
        decimal(&s.initial_margin_fraction)?,
        decimal(&s.maintenance_margin_fraction)?,
    )
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
) -> Result<()> {
    let age = s.maximum_mark_age_ns.get();
    let usdt0 = Currency::from_str("USDT0")?;
    let equity = if funding_unknown {
        None
    } else if let (Some(c), Some(m)) = (cash, margin) {
        match (
            value(c, assets, fx_ids, at, age, false)?,
            value(m, assets, fx_ids, at, age, true)?,
            fx(usdt0, assets, fx_ids, at, age),
        ) {
            (Some(c), Some(m), Some(fx)) => {
                Some(((c + m + reserve_usdc) / fx + remaining) / decimal(&s.usdt0_per_usdt)?)
            }
            _ => None,
        }
    } else if converted.is_zero() {
        Some(decimal(&s.initial_usdt)?)
    } else {
        fx(usdt0, assets, fx_ids, at, age)
            .map(|price| (remaining + converted / price) / decimal(&s.usdt0_per_usdt).unwrap())
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
        unknown_reason: equity
            .is_none()
            .then(|| "MISSING_OR_STALE_VALUATION_OR_FUNDING".into()),
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
) -> Result<()> {
    let age = s.maximum_mark_age_ns.get();
    let c = value(cash.as_ref().unwrap(), assets, fx_ids, event, age, false)?;
    let m = value(margin.as_ref().unwrap(), assets, fx_ids, event, age, true)?;
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
                * (Decimal::ONE
                    + (decimal(&s.spot_taker_fee_bps)? + decimal(&s.extra_cost_bps)?)
                        / Decimal::from(10000));
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
    for p in margin.as_ref().unwrap().positions() {
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
        for p in margin.as_ref().unwrap().positions() {
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
            let returns = selected
                .iter()
                .filter_map(|d| d.net_return)
                .collect::<Vec<_>>();
            let n = returns.len();
            let mean = if n > 0 {
                returns.iter().sum::<f64>() / n as f64
            } else {
                0.0
            };
            let variance = if n > 1 {
                returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1) as f64
            } else {
                0.0
            };
            let sharpe = (n > 1 && variance > 0.0)
                .then(|| mean / variance.sqrt() * 365.0_f64.sqrt())
                .filter(|v| v.is_finite());
            let annualized = (n > 0 && returns.iter().all(|r| *r > -1.0))
                .then(|| {
                    (365.0 * returns.iter().map(|r| r.ln_1p()).sum::<f64>() / n as f64).exp_m1()
                })
                .filter(|v| v.is_finite());
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
