//! Replay original v2 fee amounts against actual native fill-clock BARs.
//! This is a disclosed quantity allocation, NOT a fitted fee rate or a claim to
//! reconstruct historical maker/taker eligibility. Provider direction columns
//! remain in the audit only. Nautilus books each returned commission once.
use crate::catalog::NativeMarketData;
use anyhow::{ensure, Context, Result};
use bigdecimal::BigDecimal;
use contracts::{recorded_fees::*, DbCounter, DecimalValue};
use nautilus_common::clock::Clock;
use nautilus_execution::models::fee::{FeeModel, FeeModelHandle};
use nautilus_model::{
    instruments::{Instrument, InstrumentAny},
    orders::{Order, OrderAny},
    types::{Currency, Money, Price, Quantity},
};
use num_bigint::BigInt;
use num_rational::BigRational;
use rust_decimal::Decimal;
use std::{
    cell::RefCell,
    collections::{BTreeMap, BTreeSet},
    path::Path,
    rc::Rc,
    str::FromStr,
};

mod liquidity;
pub(crate) use liquidity::RecordedLiquidityModel;

const MINUTE_NS: u64 = 60_000_000_000;
const ALLOCATION: &str = "Bind actual native fill clock to its latest completed one-minute boundary; source records from that BAR sorted by (timestamp,id), consumed once FIFO, each allocated_fee = original fee_usdc * allocated_quantity / same-ID original native quantity. Original token_amount also retained. Round the sum once per native fill, half-even at pUSD precision. Conditional cost replay, not inferred historical fee eligibility. No fitted rate, direction/liquidity-role inference, missing-to-zero, or extrapolation beyond recorded quantity.";
type Key = (String, u64);

#[derive(Debug)]
struct Bucket {
    records: Vec<RecordedTradeFeeV1>,
    used: Vec<BigDecimal>,
}
#[derive(Debug)]
struct State {
    buckets: BTreeMap<Key, Bucket>,
    report: RecordedFeeReplayV1,
}
#[derive(Clone, Debug)]
pub(crate) struct RecordedFeeModel {
    clock: Rc<RefCell<dyn Clock>>,
    state: Rc<RefCell<State>>,
    currency: Currency,
}

fn wire(value: &BigDecimal) -> Result<DecimalValue> {
    value.to_plain_string().parse().map_err(anyhow::Error::msg)
}

fn ratio(value: &BigDecimal) -> BigRational {
    let (coefficient, scale) = value.as_bigint_and_exponent();
    let power =
        BigInt::from(10).pow(u32::try_from(scale.unsigned_abs()).expect("bounded decimal scale"));
    if scale >= 0 {
        BigRational::new(coefficient, power)
    } else {
        BigRational::from_integer(coefficient * power)
    }
}

fn round_money(amount: BigRational, currency: Currency) -> BigDecimal {
    let scaled =
        amount * BigRational::from_integer(BigInt::from(10).pow(u32::from(currency.precision)));
    let mut units = scaled.numer() / scaled.denom();
    let twice_remainder = (scaled.numer() % scaled.denom()) * BigInt::from(2);
    if twice_remainder > *scaled.denom()
        || (twice_remainder == *scaled.denom() && &units % BigInt::from(2) != BigInt::from(0))
    {
        units += 1;
    }
    BigDecimal::new(units, i64::from(currency.precision))
}

impl RecordedFeeModel {
    pub(crate) fn load(
        root: &Path,
        market: &NativeMarketData,
        clock: Rc<RefCell<dyn Clock>>,
    ) -> Result<Self> {
        let source: RecordedTradeFeesV1 = serde_json::from_reader(
            std::fs::File::open(root.join(RECORDED_FEES_FILE))
                .context("RECORDED_FEE_FILE_REQUIRED")?,
        )
        .context("RECORDED_FEE_INVALID_SOURCE")?;
        Self::new(source, market, clock)
    }

    fn new(
        source: RecordedTradeFeesV1,
        market: &NativeMarketData,
        clock: Rc<RefCell<dyn Clock>>,
    ) -> Result<Self> {
        ensure!(
            source.source == POLYMARKET_V2_SOURCE
                && !source.revision.trim().is_empty()
                && source.currency == "pUSD"
                && !source.records.is_empty(),
            "RECORDED_FEE_SOURCE_REQUIRED"
        );
        let currency = Currency::from_str(&source.currency)?;
        let mut bars = BTreeMap::new();
        for series in &market.series {
            let InstrumentAny::CurrencyPair(pair) = &series.instrument else {
                anyhow::bail!("RECORDED_FEE_CASH_TOKEN_REQUIRED");
            };
            ensure!(
                pair.id.venue.to_string() == "POLYMARKET"
                    && pair.id.symbol.as_str() == pair.raw_symbol.as_str()
                    && pair.quote_currency == currency
                    && pair.quote_currency.precision == currency.precision
                    && pair.multiplier.as_decimal() == Decimal::ONE
                    && series
                        .bar_type
                        .to_string()
                        .ends_with("-1-MINUTE-LAST-EXTERNAL"),
                "RECORDED_FEE_CASH_TOKEN_REQUIRED"
            );
            for bar in &series.bars {
                let end = bar.ts_init.as_u64();
                ensure!(
                    end == bar.ts_event.as_u64() && end.is_multiple_of(MINUTE_NS),
                    "RECORDED_FEE_BAR_CLOCK_MISMATCH"
                );
                ensure!(
                    bars.insert((pair.id.to_string(), end), bar.volume.as_decimal())
                        .is_none(),
                    "RECORDED_FEE_DUPLICATE_BAR"
                );
            }
        }
        let mut ids = BTreeSet::new();
        let mut buckets: BTreeMap<Key, Bucket> = BTreeMap::new();
        for record in source.records {
            ensure!(
                !record.id.trim().is_empty() && ids.insert(record.id.clone()),
                "RECORDED_FEE_DUPLICATE_ID"
            );
            ensure!(
                record.source_quantity.is_positive()
                    && record.quantity.is_positive()
                    && record.usdc_amount.is_positive()
                    && record.price.is_positive()
                    && record.fee_usdc.as_decimal() >= &BigDecimal::from(0)
                    && !record.maker_direction.is_empty()
                    && !record.taker_direction.is_empty(),
                "RECORDED_FEE_RECORD_INVALID"
            );
            let end = (record.ts_event_ns.get() / MINUTE_NS + 1)
                .checked_mul(MINUTE_NS)
                .context("RECORDED_FEE_TIME_RANGE")?;
            let key = (record.instrument_id.clone(), end);
            if bars.contains_key(&key) {
                buckets
                    .entry(key)
                    .or_insert_with(|| Bucket {
                        records: Vec::new(),
                        used: Vec::new(),
                    })
                    .records
                    .push(record);
            }
        }
        for (key, volume) in bars {
            let bucket = buckets
                .get_mut(&key)
                .context("RECORDED_FEE_BAR_RECORDS_MISSING")?;
            bucket
                .records
                .sort_by(|a, b| (a.ts_event_ns, &a.id).cmp(&(b.ts_event_ns, &b.id)));
            let total: BigDecimal = bucket.records.iter().map(|r| r.quantity.as_decimal()).sum();
            ensure!(
                total == BigDecimal::from_str(&volume.to_string())?,
                "RECORDED_FEE_BAR_QUANTITY_MISMATCH"
            );
            bucket.used = vec![BigDecimal::from(0); bucket.records.len()];
        }
        Ok(Self {
            clock,
            currency,
            state: Rc::new(RefCell::new(State {
                buckets,
                report: RecordedFeeReplayV1 {
                    source: source.source,
                    revision: source.revision,
                    currency: source.currency,
                    allocation_rule: ALLOCATION.into(),
                    fills: Vec::new(),
                },
            })),
        })
    }

    fn key(&self, instrument: &InstrumentAny) -> Key {
        let now = self.clock.borrow().timestamp_ns().as_u64();
        (instrument.id().to_string(), now / MINUTE_NS * MINUTE_NS)
    }

    pub(crate) fn handle(&self) -> FeeModelHandle {
        FeeModelHandle::new(self.clone())
    }
    pub(crate) fn report(&self) -> RecordedFeeReplayV1 {
        self.state.borrow().report.clone()
    }
}

impl FeeModel for RecordedFeeModel {
    fn get_commission(
        &self,
        order: &OrderAny,
        quantity: Quantity,
        price: Price,
        instrument: &InstrumentAny,
    ) -> Result<Money> {
        ensure!(
            order.instrument_id() == instrument.id()
                && !order.is_quote_quantity()
                && !quantity.is_undefined()
                && !price.is_undefined()
                && quantity.as_decimal() > Decimal::ZERO
                && price.as_decimal() > Decimal::ZERO
                && instrument.quote_currency() == self.currency,
            "RECORDED_FEE_FILL_INVALID"
        );
        let now = self.clock.borrow().timestamp_ns().as_u64();
        let key = self.key(instrument);
        let mut state = self.state.borrow_mut();
        let bucket = state
            .buckets
            .get(&key)
            .context("RECORDED_FEE_FILL_CLOCK_UNMATCHED")?;
        let fill_quantity = BigDecimal::from_str(&quantity.as_decimal().to_string())?;
        let mut remaining = fill_quantity.clone();
        let mut amount = BigRational::from_integer(BigInt::from(0));
        let mut allocations = Vec::new();
        let mut updates = Vec::new();
        for (index, record) in bucket.records.iter().enumerate() {
            if remaining == BigDecimal::from(0) {
                break;
            }
            let available = record.quantity.as_decimal() - &bucket.used[index];
            let allocated = remaining.clone().min(available);
            if allocated == BigDecimal::from(0) {
                continue;
            }
            let fee = ratio(record.fee_usdc.as_decimal()) * ratio(&allocated)
                / ratio(record.quantity.as_decimal());
            amount += &fee;
            remaining -= &allocated;
            allocations.push(RecordedFeeAllocationV1 {
                original: record.clone(),
                allocated_quantity: wire(&allocated)?,
                allocated_fee_numerator: fee.numer().to_string(),
                allocated_fee_denominator: fee.denom().to_string(),
            });
            updates.push((index, allocated));
        }
        ensure!(remaining == BigDecimal::from(0), "RECORDED_FEE_QUANTITY_EXHAUSTED: instrument={} fill_ns={} bar_end_ns={} quantity={} unmatched={}", key.0, now, key.1, fill_quantity, remaining);
        let rounded = round_money(amount, self.currency);
        let money = Money::from_decimal(
            Decimal::from_str_exact(&rounded.to_plain_string())?,
            self.currency,
        )?;
        let fill = RecordedFeeFillV1 {
            bar_end_ns: DbCounter::new(key.1).map_err(anyhow::Error::msg)?,
            client_order_id: order.client_order_id().to_string(),
            instrument_id: key.0.clone(),
            fill_ns: DbCounter::new(now).map_err(anyhow::Error::msg)?,
            order_side: order.order_side().to_string(),
            fill_quantity: wire(&fill_quantity)?,
            fill_price: price
                .as_decimal()
                .to_string()
                .parse()
                .map_err(anyhow::Error::msg)?,
            commission: wire(&rounded)?,
            allocations,
        };
        let bucket = state.buckets.get_mut(&key).expect("validated bucket");
        for (index, allocated) in updates {
            bucket.used[index] += allocated;
        }
        state.report.fills.push(fill);
        Ok(money)
    }
}

#[cfg(test)]
mod tests;
