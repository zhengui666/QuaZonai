//! Causal full-universe scores from original fills and completed native BARs.
//! One v2 model instance per member scans the canonical (availability,id) stream.
use crate::{feature_model::FeatureModel, signals::SignalModule};
use anyhow::{ensure, Context, Result};
use bigdecimal::{BigDecimal, ToPrimitive};
use contracts::{recorded_fees::RecordedTradeFeeV1, science::*, DbCounter};
use domain::execution::features::FeatureState;
use nautilus_model::{data::Bar, identifiers::InstrumentId};
use std::{
    collections::{BTreeMap, BTreeSet},
    str::FromStr,
};

const MINUTE: u64 = 60_000_000_000;
pub const FEATURE_KEYS: [&str; 7] = [
    "completed_bar_close",
    "previous_observed_bar_return",
    "source_taker_quantity_imbalance",
    "source_taker_cash_imbalance",
    "source_trade_count",
    "source_fee_per_cash",
    "source_within_minute_price_response",
];

fn counter(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::science::FeatureMissingReasonV1;
    use nautilus_model::{
        data::BarType,
        types::{Price, Quantity},
    };

    fn model(feature: i32, constant: Option<f64>) -> Vec<u8> {
        let output = constant.map_or_else(
            || "global.get $value".into(),
            |value| format!("f64.const {value}"),
        );
        wat::parse_str(format!(r#"(module
            (global $value (mut f64) (f64.const 0))
            (func (export "qz_set_feature_v2") (param $index i32) (param $value f64) (param i32 i64 i64)
                local.get $index i32.const {feature} i32.eq
                if local.get $value global.set $value end)
            (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64) {output}))"#)).unwrap()
    }

    fn observation(token: &str, minute: u64, buy: bool) -> (Bar, Vec<RecordedTradeFeeV1>) {
        let id = format!("{token}.POLYMARKET");
        let at = minute * MINUTE;
        let price = Price::from("0.500000");
        let bar = Bar::new_checked(
            BarType::from_str(&format!("{id}-1-MINUTE-LAST-EXTERNAL")).unwrap(),
            price,
            price,
            price,
            price,
            Quantity::from("1.000000"),
            at.into(),
            at.into(),
        )
        .unwrap();
        let record = RecordedTradeFeeV1 {
            id: format!("fill-{minute}-{token}"),
            instrument_id: id,
            ts_event_ns: counter(at - 1).unwrap(),
            source_quantity: "1".parse().unwrap(),
            quantity: "1".parse().unwrap(),
            fee_usdc: "0.001".parse().unwrap(),
            usdc_amount: "0.5".parse().unwrap(),
            price: "0.5".parse().unwrap(),
            maker_direction: if buy { "SELL" } else { "BUY" }.into(),
            taker_direction: if buy { "BUY" } else { "SELL" }.into(),
        };
        (bar, vec![record])
    }

    #[test]
    fn all_arriving_markets_and_ensemble_members_change_dynamic_shares() {
        let flow = model(2, None);
        let constant = model(0, Some(1.0));
        let mut policy = CatalogEnsemblePolicy::new(
            "original@revision",
            DbCounter::ZERO,
            Some(MINUTE),
            0.0,
            &[(&flow, 1.0, None), (&constant, 1.0, None)],
        )
        .unwrap();
        for (token, buy) in [("100", true), ("200", true), ("300", false)] {
            let (bar, records) = observation(token, 1, buy);
            policy.observe(bar, &records).unwrap();
        }
        assert_eq!(policy.observed_markets(), 3);
        assert_eq!(policy.active_opportunities(), 2);
        assert_eq!(policy.share(InstrumentId::from("100.POLYMARKET")), 0.5);
        assert_eq!(policy.share(InstrumentId::from("300.POLYMARKET")), 0.0);
        let (bar, records) = observation("100", 2, false);
        policy.observe(bar, &records).unwrap();
        assert_eq!(policy.share(InstrumentId::from("200.POLYMARKET")), 1.0);
        policy.expire(3 * MINUTE + 1).unwrap();
        assert_eq!(policy.active_opportunities(), 0);
        assert_eq!(policy.consumed_fuel(), vec![None, None]);
    }

    #[test]
    fn original_quantity_and_source_order_are_required_without_zero_repair() {
        let binary = model(2, None);
        let models = &[(&binary[..], 1.0, None), (&binary[..], 1.0, None)];
        let mut policy =
            CatalogEnsemblePolicy::new("original@revision", DbCounter::ZERO, None, 0.0, models)
                .unwrap();
        let (bar, mut records) = observation("100", 1, true);
        records[0].quantity = "0.999999".parse().unwrap();
        assert!(policy
            .observe(bar, &records)
            .unwrap_err()
            .to_string()
            .contains("BAR_QUANTITY"));
        let (bar, records) = observation("100", 1, true);
        policy.observe(bar, &records).unwrap();
        assert!(policy
            .observe(bar, &records)
            .unwrap_err()
            .to_string()
            .contains("CANONICAL_ORDER"));
        let definitions = feature_schema("original@revision", counter(20).unwrap());
        let mut state = FeatureState::new(&definitions).unwrap();
        let values = state.values(counter(1).unwrap()).unwrap();
        assert!(values.iter().all(|value| value.value.is_none()
            && value.missing_reason == Some(FeatureMissingReasonV1::NotYetAvailable)));
    }

    #[test]
    fn removing_large_score_keeps_existing_small_opportunity() {
        let binary = wat::parse_str(r#"(module
            (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64))
            (func (export "qz_predict_v2") (param i64 i64) (param $target i32) (param $ordinal i32) (param i32) (result f64)
                local.get $target i32.eqz if (result f64)
                    local.get $ordinal i32.eqz if (result f64) f64.const 1e20 else f64.const 0 end
                else f64.const 1 end))"#).unwrap();
        let mut policy = CatalogEnsemblePolicy::new(
            "original@revision",
            DbCounter::ZERO,
            None,
            0.0,
            &[(&binary, 1.0, None), (&binary, 1.0, None)],
        )
        .unwrap();
        for token in ["100", "200"] {
            let (bar, records) = observation(token, 1, true);
            policy.observe(bar, &records).unwrap();
        }
        let (bar, records) = observation("100", 2, true);
        policy.observe(bar, &records).unwrap();
        assert_eq!(policy.share(InstrumentId::from("200.POLYMARKET")), 1.0);
    }
}

/// The original source has no receive clock. Lag is an explicit research model.
pub fn feature_schema(source: &str, lag_ns: DbCounter) -> Vec<FeatureDefinitionV1> {
    FEATURE_KEYS
        .iter()
        .map(|key| FeatureDefinitionV1 {
            feature_key: (*key).into(),
            source_ref: source.into(),
            source_key: (*key).into(),
            availability: FeatureAvailabilityV1::ModeledLag { lag_ns },
            max_age_ns: None,
        })
        .collect()
}

struct Market {
    last_bar: Bar,
    ordinal: u32,
    index: u32,
}

struct Member {
    model: FeatureModel,
    weight: f64,
    initial_fuel: Option<u64>,
}

/// Scores retain only positive opportunities; holdings belong to the one native
/// account and are never deleted when a score expires or leaves this index.
pub struct CatalogEnsemblePolicy {
    definitions: Vec<FeatureDefinitionV1>,
    members: Vec<Member>,
    markets: BTreeMap<InstrumentId, Market>,
    scores: BTreeMap<InstrumentId, (f64, Option<u64>)>,
    expiries: BTreeSet<(u64, InstrumentId)>,
    score_total: BigDecimal,
    score_threshold: f64,
    maximum_age_ns: Option<u64>,
    last: Option<(u64, InstrumentId)>,
    observations: u64,
}

impl CatalogEnsemblePolicy {
    pub fn new(
        source: &str,
        lag_ns: DbCounter,
        maximum_age_ns: Option<u64>,
        score_threshold: f64,
        models: &[(&[u8], f64, Option<u64>)],
    ) -> Result<Self> {
        ensure!(models.len() >= 2, "CATALOG_ENSEMBLE_MEMBERS_REQUIRED");
        ensure!(score_threshold.is_finite(), "CATALOG_ENSEMBLE_THRESHOLD");
        ensure!(maximum_age_ns != Some(0), "CATALOG_ENSEMBLE_SCORE_AGE");
        let weight: f64 = models.iter().map(|(_, weight, _)| weight).sum();
        ensure!(
            weight.is_finite()
                && weight > 0.0
                && models.iter().all(|(_, w, _)| w.is_finite() && *w > 0.0),
            "CATALOG_ENSEMBLE_MEMBER_WEIGHTS"
        );
        let members = models
            .iter()
            .map(|(bytes, member_weight, fuel)| {
                let module = SignalModule::new(bytes, fuel.is_some())?;
                Ok(Member {
                    model: FeatureModel::new(&module, None, *fuel)?,
                    weight: member_weight / weight,
                    initial_fuel: *fuel,
                })
            })
            .collect::<Result<_>>()?;
        let definitions = feature_schema(source, lag_ns);
        domain::execution::features::schema(&definitions)?;
        Ok(Self {
            definitions,
            members,
            markets: BTreeMap::new(),
            scores: BTreeMap::new(),
            expiries: BTreeSet::new(),
            score_total: BigDecimal::from(0),
            score_threshold,
            maximum_age_ns,
            last: None,
            observations: 0,
        })
    }

    /// Caller supplies every source record in this original completed BAR. The
    /// directions remain provider BUY/SELL columns, never inferred aggressors.
    pub fn observe(&mut self, bar: Bar, records: &[RecordedTradeFeeV1]) -> Result<u64> {
        crate::catalog::validate_price_bar(&bar, None)?;
        let id = bar.bar_type.instrument_id();
        let end = bar.ts_event.as_u64();
        ensure!(
            end >= MINUTE && end.is_multiple_of(MINUTE),
            "CATALOG_ENSEMBLE_MINUTE"
        );
        let FeatureAvailabilityV1::ModeledLag { lag_ns } = &self.definitions[0].availability else {
            unreachable!("owned schema always declares modeled lag")
        };
        let available = end
            .checked_add(lag_ns.get())
            .context("CATALOG_ENSEMBLE_CLOCK")?;
        ensure!(
            bar.ts_init.as_u64() <= available && self.last.is_none_or(|old| old < (available, id)),
            "CATALOG_ENSEMBLE_CANONICAL_ORDER"
        );
        let previous = self.markets.get(&id);
        ensure!(
            previous.is_none_or(|m| m.last_bar.ts_event < bar.ts_event),
            "CATALOG_ENSEMBLE_DUPLICATE_BAR"
        );
        let ordinal = previous.map_or(Ok(0), |m| {
            m.ordinal.checked_add(1).context("CATALOG_ENSEMBLE_ORDINAL")
        })?;
        let target_index =
            previous.map_or_else(|| u32::try_from(self.markets.len()), |m| Ok(m.index))?;
        let mut quantities = BigDecimal::from(0);
        let mut signed_quantities = BigDecimal::from(0);
        let mut cash = BigDecimal::from(0);
        let mut signed_cash = BigDecimal::from(0);
        let mut fees = BigDecimal::from(0);
        let mut direction_available = true;
        let mut last_key = None;
        for record in records {
            ensure!(
                record.instrument_id == id.to_string()
                    && record.ts_event_ns.get() >= end - MINUTE
                    && record.ts_event_ns.get() < end
                    && record.quantity.is_positive()
                    && record.usdc_amount.is_positive()
                    && record.price.is_positive()
                    && record.fee_usdc.to_decimal() >= BigDecimal::from(0),
                "CATALOG_ENSEMBLE_ORIGINAL_RECORD"
            );
            let key = (record.ts_event_ns, record.id.as_str());
            ensure!(
                last_key.is_none_or(|old| old < key),
                "CATALOG_ENSEMBLE_RECORD_ORDER"
            );
            last_key = Some(key);
            let quantity = record.quantity.as_decimal();
            let notional = record.usdc_amount.to_decimal();
            quantities += quantity;
            cash += &notional;
            fees += record.fee_usdc.to_decimal();
            match (
                record.maker_direction.as_str(),
                record.taker_direction.as_str(),
            ) {
                ("SELL", "BUY") => {
                    signed_quantities += quantity;
                    signed_cash += notional;
                }
                ("BUY", "SELL") => {
                    signed_quantities -= quantity;
                    signed_cash -= notional;
                }
                _ => direction_available = false,
            }
        }
        ensure!(
            !records.is_empty() && quantities > BigDecimal::from(0),
            "CATALOG_ENSEMBLE_RECORDS_MISSING"
        );
        ensure!(
            quantities == BigDecimal::from_str(&bar.volume.as_decimal().to_string())?,
            "CATALOG_ENSEMBLE_BAR_QUANTITY"
        );
        let numeric = |value: BigDecimal| {
            value
                .to_f64()
                .filter(|value| value.is_finite())
                .context("CATALOG_ENSEMBLE_FEATURE_RANGE")
        };
        let close = bar.close.as_f64();
        let values = [
            Some(close),
            previous.map(|market| close / market.last_bar.close.as_f64() - 1.0),
            direction_available
                .then(|| numeric(signed_quantities / &quantities))
                .transpose()?,
            direction_available
                .then(|| numeric(signed_cash / &cash))
                .transpose()?,
            Some(records.len() as f64),
            Some(numeric(fees / cash)?),
            Some(
                numeric(
                    records.last().unwrap().price.to_decimal() / records[0].price.to_decimal(),
                )? - 1.0,
            ),
        ];
        let mut state = FeatureState::new(&self.definitions)?;
        for (index, value) in values.into_iter().enumerate() {
            if index == 1 && value.is_none() {
                continue;
            }
            state.observe(&FeatureObservationV1 {
                feature_index: u16::try_from(index)?,
                event_ns: counter(end)?,
                observed_available_ns: None,
                sequence: counter(self.observations)?,
                value,
                missing_reason: value.is_none().then(|| "SOURCE_NOT_AVAILABLE".into()),
            })?;
        }
        let features = state.values(counter(available)?)?;
        let mut score = 0.0;
        for member in &mut self.members {
            score += member.weight
                * member.model.predict_for_target(
                    available,
                    end,
                    target_index,
                    ordinal,
                    &features,
                )?;
        }
        ensure!(score.is_finite(), "CATALOG_ENSEMBLE_SCORE_RANGE");
        self.expire(available)?;
        self.remove_score(id);
        let excess = score - self.score_threshold;
        ensure!(excess.is_finite(), "CATALOG_ENSEMBLE_SCORE_RANGE");
        if excess > 0.0 {
            let expiry = self
                .maximum_age_ns
                .map(|age| {
                    available
                        .checked_add(age)
                        .context("CATALOG_ENSEMBLE_SCORE_CLOCK")
                })
                .transpose()?;
            self.scores.insert(id, (excess, expiry));
            self.score_total += BigDecimal::from_str(&excess.to_string())?;
            if let Some(expiry) = expiry {
                self.expiries.insert((expiry, id));
            }
        }
        self.markets.insert(
            id,
            Market {
                last_bar: bar,
                ordinal,
                index: target_index,
            },
        );
        self.last = Some((available, id));
        self.observations += 1;
        Ok(available)
    }

    fn remove_score(&mut self, id: InstrumentId) {
        if let Some((old, expiry)) = self.scores.remove(&id) {
            self.score_total -=
                BigDecimal::from_str(&old.to_string()).expect("stored finite score");
            if let Some(expiry) = expiry {
                self.expiries.remove(&(expiry, id));
            }
        }
        if self.scores.is_empty() {
            self.score_total = BigDecimal::from(0);
        }
    }

    pub fn expire(&mut self, asof_ns: u64) -> Result<()> {
        ensure!(
            self.last.is_none_or(|last| asof_ns >= last.0),
            "CATALOG_ENSEMBLE_CLOCK_REVERSED"
        );
        while let Some(&(expiry, id)) = self.expiries.first() {
            if expiry >= asof_ns {
                break;
            }
            self.remove_score(id);
        }
        Ok(())
    }

    /// Call after observing the complete timestamp batch. This returns only one
    /// market's desired share; account sizing must retain other native holdings.
    pub fn share(&self, id: InstrumentId) -> f64 {
        self.scores.get(&id).map_or(0.0, |(score, _)| {
            // Exact existing decimal arithmetic retains a small opportunity
            // when a much larger score leaves, without a universe rescan.
            let value = BigDecimal::from_str(&score.to_string()).expect("stored finite score");
            (value / &self.score_total)
                .to_f64()
                .expect("positive normalized finite score")
        })
    }

    pub fn observations(&self) -> u64 {
        self.observations
    }
    pub fn observed_markets(&self) -> usize {
        self.markets.len()
    }
    pub fn active_opportunities(&self) -> usize {
        self.scores.len()
    }
    pub fn consumed_fuel(&self) -> Vec<Option<u64>> {
        self.members
            .iter()
            .map(|member| {
                member
                    .initial_fuel
                    .zip(member.model.remaining_fuel())
                    .map(|(initial, remaining)| initial - remaining)
            })
            .collect()
    }
}
