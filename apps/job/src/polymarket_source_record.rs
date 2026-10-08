//! Original native public observations. A native timestamp and the recorder's
//! actual observation timestamp are separate facts, never interchangeable.
use contracts::{DbCounter, SchemaV1};
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub(crate) struct ForwardSourcePlan {
    pub(crate) schema_version: SchemaV1,
    pub(crate) instrument_id: String,
    pub(crate) bar_type: String,
    /// The first complete business BAR close, not a process execution deadline.
    pub(crate) first_close_ns: DbCounter,
    pub(crate) required_bars: u32,
}

impl ForwardSourcePlan {
    pub(crate) fn native(&self) -> anyhow::Result<(nautilus_model::data::BarType, u64)> {
        use anyhow::{Context, ensure};
        use nautilus_model::{
            data::BarType,
            enums::{BarAggregation, PriceType},
        };
        let bar: BarType = self.bar_type.parse()?;
        let spec = bar.spec();
        let unit = match spec.aggregation {
            BarAggregation::Millisecond => 1_000_000_u64,
            BarAggregation::Second => 1_000_000_000,
            BarAggregation::Minute => 60_000_000_000,
            BarAggregation::Hour => 3_600_000_000_000,
            _ => anyhow::bail!("FORWARD_BAR_INTERVAL_UNSUPPORTED"),
        };
        let interval = u64::try_from(spec.step.get())?
            .checked_mul(unit)
            .context("FORWARD_BAR_INTERVAL_RANGE")?;
        ensure!(
            bar.is_externally_aggregated()
                && spec.price_type == PriceType::Last
                && bar.to_string() == self.bar_type
                && bar.instrument_id().to_string() == self.instrument_id
                && bar.instrument_id().venue.as_str() == "POLYMARKET",
            "FORWARD_LAST_BAR_REQUIRED"
        );
        ensure!(
            self.required_bars > 0
                && self.first_close_ns.get() >= interval
                && self.first_close_ns.get().is_multiple_of(interval),
            "FORWARD_BUSINESS_WINDOW_INVALID"
        );
        self.final_close(interval)?;
        Ok((bar, interval))
    }

    pub(crate) fn final_close(&self, interval: u64) -> anyhow::Result<u64> {
        use anyhow::Context;
        self.first_close_ns
            .get()
            .checked_add(
                u64::from(self.required_bars - 1)
                    .checked_mul(interval)
                    .context("FORWARD_WINDOW_RANGE")?,
            )
            .context("FORWARD_WINDOW_RANGE")
    }
}

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceRecord {
    pub(crate) schema_version: SchemaV1,
    pub(crate) sequence: DbCounter,
    pub(crate) observed_at_ns: DbCounter,
    pub(crate) kind: String,
    pub(crate) payload: Value,
}
