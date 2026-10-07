//! Exact cash-inventory valuation only. No ledger, FX graph, native PnL or fees.
//! Native adapters own truth and provenance; these rules check their projections.
use crate::DomainError;
use bigdecimal::{BigDecimal, ToPrimitive};
use contracts::{
    DbCounter, DecimalValue, SchemaV1,
    science::{NativeAccountKind, NativeSimulationSettingsV1},
    spot_cash::*,
};
use std::collections::{BTreeMap, BTreeSet};

const DAY_NS: u64 = 86_400_000_000_000;
const MAX_FRAMES: usize = 1_000_000;

fn invalid(field: &'static str) -> DomainError {
    DomainError::Invalid(field)
}
fn text_ok(text: &str) -> bool {
    !text.is_empty() && text.len() <= 200 && text.trim() == text
}
fn decimal(value: BigDecimal) -> Result<DecimalValue, ReportValuationFailureV1> {
    value
        .to_plain_string()
        .parse()
        .map_err(|_| ReportValuationFailureV1::ArithmeticOutOfRange)
}

// The same evidence checks protect both individual valuations and daily series.
fn validate_replay<'a>(
    frames: impl IntoIterator<Item = &'a NativeSpotValuationFrameV1>,
) -> Result<(), ReportValuationFailureV1> {
    use ReportValuationFailureV1 as F;
    let mut replay: BTreeMap<u64, (u64, Option<&NativeSpotPriceV1>)> = BTreeMap::new();
    let mut source_rows = BTreeMap::new();
    for frame in frames {
        let binding = &frame.snapshot.binding;
        if replay
            .insert(
                binding.snapshot_sequence.get(),
                (binding.asof_ns.get(), None),
            )
            .is_some()
        {
            return Err(F::ReplaySequenceConflict);
        }
        for price in &frame.prices {
            let sequence = price.observed_sequence.get();
            if let Some((_, seen)) = replay.get(&sequence) {
                if *seen != Some(price) {
                    return Err(F::ReplaySequenceConflict);
                }
            } else {
                replay.insert(sequence, (price.available_ns.get(), Some(price)));
            }
            let source_key = (price.dataset_revision_id, price.source_row_key.as_str());
            if source_rows
                .insert(source_key, price)
                .is_some_and(|seen| seen != price)
            {
                return Err(F::PriceSourceConflict);
            }
        }
    }
    let mut previous_clock = None;
    for (clock, _) in replay.values() {
        if previous_clock.is_some_and(|previous| previous > *clock) {
            return Err(F::ReplayClockConflict);
        }
        previous_clock = Some(*clock);
    }
    Ok(())
}

/// Produces only the safe official-engine configuration values. It does not
/// instantiate an account and does not claim fee or execution readiness.
pub fn account_plan(
    settings: &NativeSimulationSettingsV1,
) -> Result<NativeSpotCashAccountPlanV1, DomainError> {
    let policy = settings
        .multi_currency_spot_cash
        .as_ref()
        .ok_or_else(|| invalid("spot_cash_mode_missing"))?;
    if settings.account_kind != NativeAccountKind::Cash
        || settings.leverage.as_decimal() != &BigDecimal::from(1)
        || !settings.starting_capital.is_positive()
        || policy.report_currency != settings.base_currency
        || !contracts::research_currency::supported(&policy.report_currency)
        || policy.maximum_price_age_ns.get() == 0
        || !(1..=256).contains(&policy.allowed_instrument_ids.len())
        || !(1..=86_400_000).contains(&settings.snapshot_interval_ms)
    {
        return Err(invalid("spot_cash_settings"));
    }
    let mut ids = BTreeSet::new();
    if policy
        .allowed_instrument_ids
        .iter()
        .any(|id| !text_ok(id) || !ids.insert(id))
    {
        return Err(invalid("spot_cash_allowed_instruments"));
    }
    Ok(NativeSpotCashAccountPlanV1 {
        account_kind: NativeAccountKind::Cash,
        native_base_currency: None,
        allow_cash_borrowing: false,
        leverage: "1".parse().expect("constant decimal"),
        starting_balances: vec![NativeSpotCashMoneyV1 {
            currency: policy.report_currency.clone(),
            amount: settings.starting_capital.clone(),
        }],
    })
}

/// Narrow validation for the owned spot replay branch. The legacy simulation
/// entrypoint remains closed until its producer and consumers are integrated.
pub fn execution_settings(settings: &NativeSimulationSettingsV1) -> Result<(), DomainError> {
    account_plan(settings)?;
    crate::portfolio::simulation_models(settings)?;
    let contracts::portfolio::NativeModelRefV1::FrozenSpotFeeScenario { .. } = &settings.fee_model
    else {
        return Err(invalid("spot_cash_explicit_public_fee_scenario_required"));
    };
    let policy = settings
        .multi_currency_spot_cash
        .as_ref()
        .expect("validated policy");
    if !settings.exposure_tolerance.is_positive()
        || settings.exposure_tolerance.as_decimal() > &BigDecimal::new(1.into(), 3)
        || settings.fee_rates.len() != policy.allowed_instrument_ids.len()
    {
        return Err(invalid("spot_cash_execution_settings"));
    }
    let mut ids = BTreeSet::new();
    for fee in &settings.fee_rates {
        if !policy.allowed_instrument_ids.contains(&fee.instrument_id)
            || !ids.insert(&fee.instrument_id)
            || fee.maker.as_decimal() != &BigDecimal::new(4.into(), 4)
            || fee.taker.as_decimal() != &BigDecimal::new(7.into(), 4)
        {
            return Err(invalid("spot_cash_fee_planning_scenario_mismatch"));
        }
    }
    Ok(())
}

/// Validated, session-bound inputs. Private fields prevent unvalidated callers
/// from constructing a context or silently substituting a policy mid-session.
#[derive(Clone, Debug)]
pub struct SpotCashContext {
    settings: NativeSimulationSettingsV1,
    session: NativeSpotCashSessionV1,
    by_id: BTreeMap<String, NativeSpotInstrumentV1>,
    by_currency: BTreeMap<String, String>,
}

impl SpotCashContext {
    fn policy(&self) -> &NativeSpotCashPolicyV1 {
        self.settings
            .multi_currency_spot_cash
            .as_ref()
            .expect("validated immutable settings")
    }

    pub fn new(
        settings: &NativeSimulationSettingsV1,
        session: NativeSpotCashSessionV1,
        instruments: &[NativeSpotInstrumentV1],
    ) -> Result<Self, DomainError> {
        account_plan(settings)?;
        let policy = settings
            .multi_currency_spot_cash
            .as_ref()
            .expect("validated option")
            .clone();
        if !text_ok(&session.account_id)
            || !text_ok(&session.venue)
            || session.period_end_ns <= session.period_start_ns
            || instruments.len() != policy.allowed_instrument_ids.len()
        {
            return Err(invalid("spot_cash_session"));
        }
        let allowed: BTreeSet<_> = policy
            .allowed_instrument_ids
            .iter()
            .map(String::as_str)
            .collect();
        let mut by_id = BTreeMap::new();
        let mut by_currency = BTreeMap::new();
        for instrument in instruments {
            if !allowed.contains(instrument.instrument_id.as_str())
                || instrument.native_kind != "CurrencyPair"
                || instrument.venue != session.venue
                || !instrument
                    .instrument_id
                    .ends_with(&format!(".{}", session.venue))
                || !text_ok(&instrument.base_currency)
                || instrument.base_currency == policy.report_currency
                || instrument.quote_currency != policy.report_currency
                || instrument.is_inverse
                || instrument.has_expiration
                || instrument.multiplier.as_decimal() != &BigDecimal::from(1)
                || by_currency
                    .insert(
                        instrument.base_currency.clone(),
                        instrument.instrument_id.clone(),
                    )
                    .is_some()
                || by_id
                    .insert(instrument.instrument_id.clone(), instrument.clone())
                    .is_some()
            {
                return Err(invalid("spot_cash_instrument"));
            }
        }
        Ok(Self {
            settings: settings.clone(),
            session,
            by_id,
            by_currency,
        })
    }

    fn bound(&self, binding: &NativeSpotSnapshotBindingV1) -> bool {
        binding.session_id == self.session.session_id
            && binding.native_instance_id == self.session.native_instance_id
            && binding.dataset_revision_id == self.session.dataset_revision_id
            && binding.account_id == self.session.account_id
            && binding.venue == self.session.venue
            && binding.asof_ns >= self.session.period_start_ns
            && binding.asof_ns <= self.session.period_end_ns
    }

    // Validate identity and causal ordering even for unvalued intermediate
    // frames and prices that a zero inventory does not need for arithmetic.
    fn validate_price_sources(
        &self,
        frame: &NativeSpotValuationFrameV1,
    ) -> Result<(), ReportValuationFailureV1> {
        use ReportValuationFailureV1 as F;
        if frame.prices.len() > self.by_id.len() {
            return Err(F::DuplicatePrice);
        }
        let mut instruments = BTreeSet::new();
        for price in &frame.prices {
            if !self.by_id.contains_key(&price.instrument_id) {
                return Err(F::UnknownPriceInstrument);
            }
            if price.session_id != self.session.session_id
                || price.native_instance_id != self.session.native_instance_id
                || price.dataset_revision_id != self.session.dataset_revision_id
                || !text_ok(&price.source_row_key)
                || price.bar_open_ns >= price.event_ns
            {
                return Err(F::InvalidPriceSource);
            }
            if !instruments.insert(price.instrument_id.as_str()) {
                return Err(F::DuplicatePrice);
            }
            if price.method != self.policy().price_method {
                return Err(F::PriceMethodMismatch);
            }
            if price.event_ns > price.available_ns
                || price.available_ns > frame.snapshot.binding.asof_ns
            {
                return Err(F::FuturePrice);
            }
            // Equal nanosecond timestamps do not establish which callback ran first.
            if price.observed_sequence >= frame.snapshot.binding.snapshot_sequence {
                return Err(F::AmbiguousPriceOrder);
            }
        }
        Ok(())
    }

    /// Values total once. Locked, positions, PnL and commissions are deliberately
    /// absent from the arithmetic; a single invalid leg erases the entire total.
    pub fn value(&self, frame: &NativeSpotValuationFrameV1) -> ReportCurrencyValuationV1 {
        let outcome = match self.value_complete(frame) {
            Ok((total, legs)) => ReportCurrencyValuationOutcomeV1::Complete { total, legs },
            Err(reason) => ReportCurrencyValuationOutcomeV1::Unavailable { reason },
        };
        ReportCurrencyValuationV1 {
            schema_version: SchemaV1,
            snapshot: frame.snapshot.binding.clone(),
            report_currency: self.policy().report_currency.clone(),
            policy: self.policy().clone(),
            outcome,
        }
    }

    fn value_complete(
        &self,
        frame: &NativeSpotValuationFrameV1,
    ) -> Result<(DecimalValue, Vec<ReportCurrencyValuationLegV1>), ReportValuationFailureV1> {
        use ReportValuationFailureV1 as F;
        let snapshot = &frame.snapshot;
        if !self.bound(&snapshot.binding) {
            return Err(F::InvalidSnapshotBinding);
        }
        if snapshot.account_kind != NativeAccountKind::Cash
            || snapshot.native_base_currency.is_some()
        {
            return Err(F::UnsupportedAccount);
        }
        if !snapshot.balances_complete
            || snapshot.balances.is_empty()
            || snapshot.balances.len() > 257
        {
            return Err(F::IncompleteBalances);
        }
        self.validate_price_sources(frame)?;
        let prices: BTreeMap<_, _> = frame
            .prices
            .iter()
            .map(|price| (price.instrument_id.as_str(), price))
            .collect();
        let mut currencies = BTreeSet::new();
        let mut total = DecimalValue::zero();
        let mut legs = Vec::new();
        for balance in &snapshot.balances {
            if !currencies.insert(balance.currency.as_str()) {
                return Err(F::DuplicateCurrency);
            }
            if !balance.total.is_nonnegative()
                || !balance.free.is_nonnegative()
                || !balance.locked.is_nonnegative()
                || balance.free.checked_add(&balance.locked).as_ref() != Some(&balance.total)
            {
                return Err(F::InvalidBalance);
            }
            let direct = balance.currency == self.policy().report_currency;
            let instrument_id = if direct {
                None
            } else {
                Some(
                    self.by_currency
                        .get(&balance.currency)
                        .ok_or(F::UnknownCurrency)?,
                )
            };
            let (report_value, price) = if direct || !balance.total.is_positive() {
                (balance.total.clone(), None)
            } else {
                let price = *prices
                    .get(instrument_id.expect("non-report balance").as_str())
                    .ok_or(F::MissingPrice)?;
                if snapshot.binding.asof_ns.get() - price.event_ns.get()
                    > self.policy().maximum_price_age_ns.get()
                {
                    return Err(F::StalePrice);
                }
                if !price.price.is_positive() {
                    return Err(F::NonPositivePrice);
                }
                (
                    decimal(balance.total.as_decimal() * price.price.as_decimal())?,
                    Some(price.clone()),
                )
            };
            total = total
                .checked_add(&report_value)
                .ok_or(F::ArithmeticOutOfRange)?;
            legs.push(ReportCurrencyValuationLegV1 {
                currency: balance.currency.clone(),
                native_total: balance.total.clone(),
                report_value,
                price,
            });
        }
        if !currencies.contains(self.policy().report_currency.as_str()) {
            return Err(F::IncompleteBalances);
        }
        validate_replay(std::iter::once(frame))?;
        legs.sort_by(|a, b| a.currency.cmp(&b.currency));
        Ok((total, legs))
    }

    /// Uses real adjacent UTC-midnight boundaries; missing boundaries stay gaps.
    /// Every supplied frame still undergoes identity/order/source validation.
    /// No inferred intraday grid, native Analyzer.add_return or synthetic equity.
    /// `flows` must come from the trusted completed-run owner described by its
    /// contract. This pure rule checks correspondence, not receipt authenticity.
    pub fn daily_returns(
        &self,
        frames: &[NativeSpotValuationFrameV1],
        flows: &NativeSpotCashFlowEvidenceV1,
    ) -> Result<ReportCurrencyDailyReturnsV1, DomainError> {
        if flows.external_flow_count.map(DbCounter::get) != Some(0)
            || flows.native_instance_id != self.session.native_instance_id
            || flows.session_id != self.session.session_id
            || flows.dataset_revision_id != self.session.dataset_revision_id
            || flows.account_id != self.session.account_id
            || flows.venue != self.session.venue
            || flows.period_start_ns != self.session.period_start_ns
            || flows.period_end_ns != self.session.period_end_ns
            || flows.opening_snapshot.asof_ns != self.session.period_start_ns
            || flows.closing_snapshot.asof_ns != self.session.period_end_ns
            || flows.observed_snapshot_count.get() != frames.len() as u64
            || frames.first().map(|frame| &frame.snapshot.binding) != Some(&flows.opening_snapshot)
            || frames.last().map(|frame| &frame.snapshot.binding) != Some(&flows.closing_snapshot)
        {
            return Err(invalid("spot_cash_external_flows_unavailable"));
        }
        if frames.len() > MAX_FRAMES {
            return Err(invalid("spot_cash_frame_count"));
        }
        let mut observations = BTreeMap::new();
        let mut previous: Option<&NativeSpotSnapshotBindingV1> = None;
        let mut snapshot_ids = BTreeSet::new();
        for frame in frames {
            let b = &frame.snapshot.binding;
            if !self.bound(b)
                || !snapshot_ids.insert(b.event_id)
                || previous.is_some_and(|p| {
                    p.asof_ns > b.asof_ns || p.snapshot_sequence >= b.snapshot_sequence
                })
            {
                return Err(invalid("spot_cash_snapshot_order_or_binding"));
            }
            self.validate_price_sources(frame)
                .map_err(|_| invalid("spot_cash_price_source_or_order"))?;
            if b.origin == NativeSpotSnapshotOriginV1::NativePublication
                && b.asof_ns.get() % DAY_NS == 0
            {
                observations
                    .entry(b.asof_ns.get())
                    .or_insert_with(|| self.value(frame));
            }
            previous = Some(b);
        }
        validate_replay(frames.iter())
            .map_err(|_| invalid("spot_cash_replay_evidence_conflict"))?;
        match self.policy().daily_sampling {
            NativeSpotDailySamplingV1::NativeUtcMidnightBoundaries => {}
        }
        let start = self.session.period_start_ns.get();
        let end = self.session.period_end_ns.get();
        let first_day = start.div_ceil(DAY_NS) * DAY_NS;
        let last_boundary = end / DAY_NS * DAY_NS;
        if last_boundary.saturating_sub(first_day) / DAY_NS > 10_000 {
            return Err(invalid("spot_cash_day_count"));
        }
        let mut days = Vec::new();
        let mut day = first_day;
        while day < last_boundary {
            let next = day + DAY_NS;
            let opening = observations.get(&day);
            let closing = observations.get(&next);
            let mut reason = None;
            if opening.is_none() || closing.is_none() {
                reason = Some(ReportCurrencyReturnFailureV1::SnapshotGap);
            } else if [opening, closing].iter().flatten().any(|point| {
                matches!(
                    point.outcome,
                    ReportCurrencyValuationOutcomeV1::Unavailable { .. }
                )
            }) {
                reason = Some(ReportCurrencyReturnFailureV1::ValuationUnavailable);
            }
            let mut value = None;
            if reason.is_none() {
                if let (
                    Some(ReportCurrencyValuationV1 {
                        outcome: ReportCurrencyValuationOutcomeV1::Complete { total: opening, .. },
                        ..
                    }),
                    Some(ReportCurrencyValuationV1 {
                        outcome: ReportCurrencyValuationOutcomeV1::Complete { total: closing, .. },
                        ..
                    }),
                ) = (opening, closing)
                {
                    if !opening.is_positive() {
                        reason = Some(ReportCurrencyReturnFailureV1::NonPositiveOpeningEquity);
                    } else {
                        // Balance arithmetic remains exact decimal. Only the final
                        // dimensionless statistical observation is converted to f64.
                        let ratio =
                            (closing.as_decimal() - opening.as_decimal()) / opening.as_decimal();
                        value = ratio.to_f64().filter(|v| v.is_finite());
                        if value.is_none() {
                            reason = Some(ReportCurrencyReturnFailureV1::ArithmeticOutOfRange);
                        }
                    }
                } else {
                    reason = Some(ReportCurrencyReturnFailureV1::SnapshotGap);
                }
            }
            days.push(ReportCurrencyDailyReturnV1 {
                day_start_ns: DbCounter::new(day).map_err(|_| invalid("spot_cash_day_clock"))?,
                day_end_ns: DbCounter::new(next).map_err(|_| invalid("spot_cash_day_clock"))?,
                value,
                reason,
                opening_snapshot: opening.map(|point| point.snapshot.clone()),
                closing_snapshot: closing.map(|point| point.snapshot.clone()),
            });
            day = next;
        }
        Ok(ReportCurrencyDailyReturnsV1 {
            schema_version: SchemaV1,
            kind: ReportCurrencyReturnsKindV1::ReportCurrencyDaily,
            report_currency: self.policy().report_currency.clone(),
            policy: self.policy().clone(),
            annualization_days: 365,
            flow_evidence: flows.clone(),
            days,
        })
    }
}

#[cfg(test)]
mod tests;
