//! Pure admission and evidence projection. Nautilus owns quantities, fills and valuation.
use crate::{DomainError, control::text};
use chrono::{DateTime, Utc};
use contracts::{
    DecimalValue, account_observation::AccountMoneyV1, capital_exit::*,
    forward::ForwardEnvironmentV1,
};
use std::collections::BTreeSet;

fn invalid(code: &'static str) -> DomainError {
    DomainError::Invalid(code)
}
fn label(value: &str) -> Result<(), DomainError> {
    text(value, 1, 200, false)
}
fn labels(values: &[String]) -> Result<(), DomainError> {
    if values.len() > 1024 {
        return Err(invalid("capital_exit_reference_limit"));
    }
    let mut unique = BTreeSet::new();
    for value in values {
        label(value)?;
        if !unique.insert(value) {
            return Err(invalid("capital_exit_duplicate_reference"));
        }
    }
    Ok(())
}
fn money(value: &AccountMoneyV1, currency: &str) -> Result<(), DomainError> {
    if value.currency != currency || !value.amount.is_nonnegative() {
        return Err(invalid("capital_exit_money"));
    }
    Ok(())
}
pub fn subtract(left: &DecimalValue, right: &DecimalValue) -> Result<DecimalValue, DomainError> {
    if right > left {
        return Err(invalid("capital_exit_amount_exceeds_reservation"));
    }
    (left.as_decimal() - right.as_decimal())
        .to_plain_string()
        .parse()
        .map_err(|_| invalid("capital_exit_money_overflow"))
}
pub fn preview_request(
    value: &CapitalExitPreviewRequestV1,
    now: DateTime<Utc>,
) -> Result<(), DomainError> {
    let requested = value.scope.money();
    if !requested.amount.is_positive()
        || !contracts::research_currency::supported(&requested.currency)
    {
        return Err(invalid("capital_exit_requested_amount"));
    }
    if let CapitalExitScopeV1::PortfolioScope {
        stream_ids,
        release_ids,
        ..
    } = &value.scope
    {
        if stream_ids.is_empty()
            || release_ids.is_empty()
            || stream_ids.len() > 64
            || release_ids.len() > 64
            || stream_ids.iter().collect::<BTreeSet<_>>().len() != stream_ids.len()
            || release_ids.iter().collect::<BTreeSet<_>>().len() != release_ids.len()
        {
            return Err(invalid("capital_exit_portfolio_scope"));
        }
    }
    if let CapitalExitPolicyV1::BoundedLimit {
        deadline,
        legs,
        max_execution_cost,
    } = &value.policy
    {
        if *deadline <= now
            || legs.len() != 1
            || !max_execution_cost.amount.is_nonnegative()
            || max_execution_cost.currency != requested.currency
        {
            return Err(invalid("capital_exit_execution_bounds"));
        }
        for leg in legs {
            label(&leg.instrument_id)?;
            if !leg.maximum_reduction_quantity.is_positive()
                || !leg.minimum_sell_price.is_positive()
            {
                return Err(invalid("capital_exit_execution_bounds"));
            }
        }
    }
    Ok(())
}
pub fn admit_preview(value: &CapitalExitPreviewV1, now: DateTime<Utc>) -> Result<(), DomainError> {
    if value.capability != CapitalExitCapabilityV1::Supported || !value.reason_codes.is_empty() {
        return Err(DomainError::CapabilityUnavailable(
            "capital_exit_owner_blocked",
        ));
    }
    if value.valid_until <= now {
        return Err(invalid("capital_exit_preview_expired"));
    }
    if value.managed_account_key.is_none()
        || value.owner_binding_ref.is_none()
        || value.expected_account_control_revision.is_none()
        || value.evidence_refs.is_empty()
    {
        return Err(invalid("account_owner_binding_unavailable"));
    }
    preview_request(
        &CapitalExitPreviewRequestV1 {
            schema_version: contracts::SchemaV1,
            account_source_id: value.account_source_id,
            expected_source_observation_id: value.original_observation_id,
            scope: value.scope.clone(),
            policy: value.policy.clone(),
        },
        now,
    )?;
    if matches!(value.scope, CapitalExitScopeV1::PortfolioScope { .. }) {
        return Err(DomainError::CapabilityUnavailable(
            "capital_exit_portfolio_scope_unavailable",
        ));
    }
    let requested = value.scope.money();
    if value.funds.requested != requested {
        return Err(invalid("capital_exit_plan_amount"));
    }
    for amount in [
        &value.funds.native_total_cash,
        &value.funds.native_free_cash,
        &value.funds.native_locked_cash,
        &value.funds.verified_idle_cash,
        &value.funds.estimated_release,
        &value.funds.native_equity,
        &value.funds.managed_capital_before,
        &value.funds.remaining_managed_capital,
        &value.funds.estimated_execution_cost,
    ]
    .into_iter()
    .flatten()
    {
        money(amount, &requested.currency)?;
    }
    let before = value
        .funds
        .managed_capital_before
        .as_ref()
        .ok_or(invalid("capital_exit_budget_evidence_unavailable"))?;
    let after = value
        .funds
        .remaining_managed_capital
        .as_ref()
        .ok_or(invalid("capital_exit_budget_evidence_unavailable"))?;
    if subtract(&before.amount, &requested.amount)? != after.amount {
        return Err(invalid("capital_exit_budget_evidence_mismatch"));
    }
    let proposed: BTreeSet<_> = value
        .proposed_cancellations
        .iter()
        .map(|v| &v.native_client_order_id)
        .collect();
    if value
        .retained_protective_orders
        .iter()
        .any(|v| proposed.contains(&v.native_client_order_id))
    {
        return Err(invalid("protective_order_quantity_conflict"));
    }
    match &value.policy {
        CapitalExitPolicyV1::CashOnly {} => {
            let idle = value
                .funds
                .verified_idle_cash
                .as_ref()
                .ok_or(invalid("capital_exit_idle_cash_unavailable"))?;
            if idle.amount < requested.amount || !value.reduction_legs.is_empty() {
                return Err(invalid("capital_exit_cash_only_insufficient"));
            }
        }
        CapitalExitPolicyV1::BoundedLimit {
            legs,
            max_execution_cost,
            ..
        } => {
            if legs != &value.reduction_legs
                || value
                    .funds
                    .estimated_execution_cost
                    .as_ref()
                    .is_none_or(|cost| cost.amount > max_execution_cost.amount)
                || !value
                    .evidence_refs
                    .contains(&max_execution_cost.reference_evidence_id)
            {
                return Err(invalid("capital_exit_cost_or_quantity_bound_unavailable"));
            }
        }
    }
    Ok(())
}
pub fn action_state(
    current: CapitalExitStateV1,
    action: &CapitalExitActionV1,
) -> Result<CapitalExitStateV1, DomainError> {
    use CapitalExitStateV1::*;
    if current == Completed {
        return Err(DomainError::InvalidTransition);
    }
    match action {
        CapitalExitActionV1::Pause { .. }
            if current != CancelledReserved && current != CancellingExit =>
        {
            Ok(Paused)
        }
        CapitalExitActionV1::Cancel { .. } => Ok(CancellingExit),
        CapitalExitActionV1::Resume { .. }
            if matches!(
                current,
                Paused | Blocked | WaitingEvidence | CancelledReserved
            ) =>
        {
            Ok(Requested)
        }
        CapitalExitActionV1::ReconcileWithdrawal {
            user_reported_amount,
            currency,
            external_transfer_ref,
            ..
        } => {
            if !user_reported_amount.is_positive()
                || !contracts::research_currency::supported(currency)
            {
                return Err(invalid("capital_exit_reported_withdrawal"));
            }
            if let Some(reference) = external_transfer_ref {
                label(reference)?;
            }
            Ok(ReconcilingWithdrawal)
        }
        _ => Err(DomainError::InvalidTransition),
    }
}
pub fn evidence(value: &CapitalExitOwnerEvidenceV1, now: DateTime<Utc>) -> Result<(), DomainError> {
    for text in [
        &value.owner_binding_ref,
        &value.native_session_id,
        &value.native_account_id,
        &value.external_claim_id,
        &value.external_message_id,
    ] {
        label(text)?;
    }
    if value.sequence.get() == 0
        || value.account_control_epoch.get() == 0
        || value.asof > now
        || value.valid_until <= value.asof
        || value.valid_until <= now
        || value.original_event_refs.is_empty()
        || !value.native_evidence.is_object()
        || value
            .native_evidence
            .as_object()
            .is_none_or(|o| o.is_empty())
    {
        return Err(invalid("capital_exit_native_evidence"));
    }
    labels(&value.original_event_refs)?;
    match &value.evidence {
        CapitalExitEvidenceKindV1::FenceApplied {
            native_gate_report_ref,
            controlled_strategy_ids,
            invalidated_target_claim_refs,
            invalidated_child_timer_refs,
            resolved_inflight_report_ref,
            remaining_trading,
            remaining_target_authority_ref,
        } => {
            label(native_gate_report_ref)?;
            label(resolved_inflight_report_ref)?;
            labels(controlled_strategy_ids)?;
            labels(invalidated_target_claim_refs)?;
            labels(invalidated_child_timer_refs)?;
            if controlled_strategy_ids.is_empty() {
                return Err(invalid("capital_exit_gate_coverage_unavailable"));
            }
            if *remaining_trading == CapitalExitRemainingTradingV1::BudgetAware {
                label(
                    remaining_target_authority_ref
                        .as_deref()
                        .ok_or(invalid("capital_exit_remaining_target_unavailable"))?,
                )?;
            }
        }
        CapitalExitEvidenceKindV1::NativeProgress {
            phase,
            native_order_refs,
            native_position_refs,
            native_report_ref,
            reason_codes,
            ..
        } => {
            if !matches!(
                phase,
                CapitalExitStateV1::CancellingOpeners
                    | CapitalExitStateV1::Reducing
                    | CapitalExitStateV1::WaitingEvidence
                    | CapitalExitStateV1::Paused
                    | CapitalExitStateV1::CancellingExit
                    | CapitalExitStateV1::CancelledReserved
                    | CapitalExitStateV1::Blocked
            ) {
                return Err(invalid("capital_exit_native_phase"));
            }
            label(native_report_ref)?;
            labels(native_position_refs)?;
            labels(reason_codes)?;
            for order in native_order_refs {
                label(&order.native_client_order_id)?;
                label(&order.native_strategy_id)?;
                label(&order.native_instrument_id)?;
                labels(&order.original_event_refs)?;
                if order.original_event_refs.is_empty() {
                    return Err(invalid("capital_exit_order_evidence_unavailable"));
                }
            }
        }
        CapitalExitEvidenceKindV1::WithdrawabilityObserved {
            venue,
            native_availability_report_ref,
            settlement_report_ref,
            margin_report_ref,
            open_orders_report_ref,
            ..
        } => {
            for reference in [
                venue,
                native_availability_report_ref,
                settlement_report_ref,
                margin_report_ref,
                open_orders_report_ref,
            ] {
                label(reference)?;
            }
        }
        CapitalExitEvidenceKindV1::WithdrawalReconciled {
            native_cash_movement_ref,
            external_transfer_ref,
            before_observation_id,
            after_observation_id,
            ..
        } => {
            label(native_cash_movement_ref)?;
            label(external_transfer_ref)?;
            if before_observation_id == after_observation_id {
                return Err(invalid("capital_exit_cash_movement_observations"));
            }
        }
    }
    Ok(())
}
/// Expiry changes only the live projection, never the retained original evidence.
pub fn freshness(
    value: &mut CapitalExitViewV1,
    now: DateTime<Utc>,
    source_observation_is_current: bool,
) {
    if value
        .funds
        .evidence_valid_until
        .is_some_and(|expiry| expiry <= now)
        || !source_observation_is_current
    {
        if value.funds.verified_withdrawable_amount.is_some() {
            value.funds.withdrawability = CapitalExitWithdrawabilityV1::Stale;
        }
        value.funds.verified_withdrawable_amount = None;
    }
}
/// This clamps a venue-provided amount to this intent's reservation. It does not
/// infer availability from balance/equity or value positions in the service.
pub fn apply_availability(
    view: &mut CapitalExitViewV1,
    available: &AccountMoneyV1,
    basis: CapitalExitAvailabilityBasisV1,
    asof: DateTime<Utc>,
    valid_until: DateTime<Utc>,
) -> Result<(), DomainError> {
    money(available, &view.funds.reserved_amount.currency)?;
    if (view.environment == ForwardEnvironmentV1::Paper)
        != (basis == CapitalExitAvailabilityBasisV1::ControlledSandbox)
    {
        return Err(invalid("capital_exit_availability_environment"));
    }
    view.funds.verified_withdrawable_amount = Some(AccountMoneyV1 {
        amount: std::cmp::min(
            available.amount.clone(),
            view.funds.reserved_amount.amount.clone(),
        ),
        currency: available.currency.clone(),
    });
    view.funds.evidence_asof = Some(asof);
    view.funds.evidence_valid_until = Some(valid_until);
    view.funds.withdrawability = if view.environment == ForwardEnvironmentV1::Paper {
        CapitalExitWithdrawabilityV1::Simulated
    } else {
        CapitalExitWithdrawabilityV1::Verified
    };
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::{DbCounter, Id, Revision, SchemaV1};
    fn money(amount: &str) -> AccountMoneyV1 {
        AccountMoneyV1 {
            amount: amount.parse().unwrap(),
            currency: "USD".into(),
        }
    }
    fn time() -> DateTime<Utc> {
        "2026-10-07T14:00:00Z".parse().unwrap()
    }
    fn preview() -> CapitalExitPreviewV1 {
        CapitalExitPreviewV1 {
            schema_version: SchemaV1,
            id: Id::new(),
            project_id: Id::new(),
            account_source_id: Id::new(),
            original_observation_id: Id::new(),
            environment: ForwardEnvironmentV1::Paper,
            managed_account_key: Some("sandbox:SIM-001".into()),
            owner_binding_ref: Some("registered-owner".into()),
            expected_account_control_revision: Some(Revision::INITIAL),
            scope: CapitalExitScopeV1::Amount {
                amount: "100".parse().unwrap(),
                currency: "USD".into(),
            },
            policy: CapitalExitPolicyV1::CashOnly {},
            plan_artifact_id: Id::new(),
            evidence_refs: vec![Id::new()],
            valid_until: time() + chrono::Duration::seconds(30),
            funds: CapitalExitPreviewFundsV1 {
                requested: money("100"),
                native_total_cash: Some(money("1000")),
                native_free_cash: Some(money("1000")),
                native_locked_cash: Some(money("0")),
                verified_idle_cash: Some(money("1000")),
                estimated_release: Some(money("100")),
                native_equity: Some(money("1000")),
                managed_capital_before: Some(money("1000")),
                remaining_managed_capital: Some(money("900")),
                estimated_execution_cost: None,
                existing_unrealized_pnl: None,
            },
            proposed_cancellations: vec![],
            retained_protective_orders: vec![],
            reduction_legs: vec![],
            remaining_risk_evidence_id: None,
            capability: CapitalExitCapabilityV1::Supported,
            reason_codes: vec![],
        }
    }
    fn view() -> CapitalExitViewV1 {
        let p = preview();
        let command_id = Id::new();
        CapitalExitViewV1 {
            schema_version: SchemaV1,
            id: Id::new(),
            project_id: p.project_id,
            account_source_id: p.account_source_id,
            environment: p.environment,
            managed_account_key: p.managed_account_key.unwrap(),
            owner_binding_ref: p.owner_binding_ref.unwrap(),
            account_control_epoch: DbCounter::new(1).unwrap(),
            account_control_revision: Revision::INITIAL,
            revision: Revision::INITIAL,
            state: CapitalExitStateV1::WaitingEvidence,
            last_phase: CapitalExitStateV1::Fencing,
            reason_codes: vec![],
            preview_id: p.id,
            plan_artifact_id: p.plan_artifact_id,
            scope: p.scope,
            policy: p.policy,
            command_id,
            owner_command: CapitalExitOwnerCommandV1 {
                schema_version: SchemaV1,
                command_id,
                account_control_epoch: DbCounter::new(1).unwrap(),
                instruction: CapitalExitOwnerInstructionV1::Start {},
            },
            external_claim_id: Some("claim".into()),
            remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
            funds: CapitalExitFundsV1 {
                requested_amount: money("100"),
                reserved_amount: money("100"),
                released_cash_amount: None,
                verified_withdrawable_amount: None,
                reconciled_withdrawal_amount: money("0"),
                unreleased_amount: None,
                evidence_asof: None,
                evidence_valid_until: None,
                withdrawability: CapitalExitWithdrawabilityV1::Unverified,
            },
            evidence_refs: vec![],
            created_at: time(),
            updated_at: time(),
        }
    }
    #[test]
    fn capital_exit_lone_cash_and_locked_zero_are_not_idle_evidence() {
        let mut p = preview();
        admit_preview(&p, time()).unwrap();
        p.funds.verified_idle_cash = None;
        assert_eq!(
            admit_preview(&p, time()),
            Err(DomainError::Invalid("capital_exit_idle_cash_unavailable"))
        );
    }
    #[test]
    fn capital_exit_expired_plan_and_missing_binding_fail_closed() {
        let mut p = preview();
        p.valid_until = time();
        assert!(admit_preview(&p, time()).is_err());
        p = preview();
        p.managed_account_key = None;
        assert!(admit_preview(&p, time()).is_err());
        p = preview();
        p.reason_codes.push("uncoordinated_owner".into());
        assert!(admit_preview(&p, time()).is_err());
    }
    #[test]
    fn capital_exit_price_cost_and_instrument_bounds_are_not_defaulted() {
        let mut p = preview();
        let leg = CapitalExitReductionLegV1 {
            instrument_id: "TEST.SIM".into(),
            maximum_reduction_quantity: "1".parse().unwrap(),
            minimum_sell_price: "100".parse().unwrap(),
        };
        p.policy = CapitalExitPolicyV1::BoundedLimit {
            deadline: p.valid_until,
            legs: vec![leg.clone()],
            max_execution_cost: CapitalExitCostLimitV1 {
                amount: "1".parse().unwrap(),
                currency: "USD".into(),
                reference_evidence_id: p.evidence_refs[0],
            },
        };
        p.reduction_legs = vec![leg];
        assert!(admit_preview(&p, time()).is_err());
        p.funds.estimated_execution_cost = Some(money("1"));
        admit_preview(&p, time()).unwrap();
        p.reduction_legs[0].minimum_sell_price = "99".parse().unwrap();
        assert!(admit_preview(&p, time()).is_err());
    }
    #[test]
    fn capital_exit_cancel_never_changes_the_reservation_or_creates_reinvestment() {
        let v = view();
        let pause = CapitalExitActionV1::Pause {
            schema_version: SchemaV1,
            expected_revision: Revision::INITIAL,
        };
        let cancel = CapitalExitActionV1::Cancel {
            schema_version: SchemaV1,
            expected_revision: Revision::INITIAL,
        };
        assert_eq!(
            action_state(v.state, &pause).unwrap(),
            CapitalExitStateV1::Paused
        );
        assert_eq!(
            action_state(v.state, &cancel).unwrap(),
            CapitalExitStateV1::CancellingExit
        );
        assert_eq!(v.funds.reserved_amount, money("100"));
        assert!(action_state(CapitalExitStateV1::Completed, &pause).is_err());
    }
    #[test]
    fn capital_exit_simulation_cannot_be_verified_and_expiry_clears_ready_amount() {
        let mut v = view();
        assert!(
            apply_availability(
                &mut v,
                &money("1000"),
                CapitalExitAvailabilityBasisV1::NativeVenueAvailable,
                time(),
                time() + chrono::Duration::seconds(30)
            )
            .is_err()
        );
        apply_availability(
            &mut v,
            &money("1000"),
            CapitalExitAvailabilityBasisV1::ControlledSandbox,
            time(),
            time() + chrono::Duration::seconds(30),
        )
        .unwrap();
        assert_eq!(v.funds.verified_withdrawable_amount, Some(money("100")));
        assert_eq!(
            v.funds.withdrawability,
            CapitalExitWithdrawabilityV1::Simulated
        );
        freshness(&mut v, time() + chrono::Duration::seconds(30), true);
        assert_eq!(v.funds.withdrawability, CapitalExitWithdrawabilityV1::Stale);
        assert_eq!(v.funds.verified_withdrawable_amount, None);
        assert_eq!(v.funds.evidence_asof, Some(time()));
    }
    #[test]
    fn capital_exit_new_account_observation_invalidates_old_availability() {
        let mut v = view();
        apply_availability(
            &mut v,
            &money("50"),
            CapitalExitAvailabilityBasisV1::ControlledSandbox,
            time(),
            time() + chrono::Duration::seconds(30),
        )
        .unwrap();
        freshness(&mut v, time(), false);
        assert_eq!(v.funds.verified_withdrawable_amount, None);
        assert_eq!(v.funds.withdrawability, CapitalExitWithdrawabilityV1::Stale);
    }
    #[test]
    fn capital_exit_budget_uses_exact_remaining_reserve_not_another_equity_subtraction() {
        let outstanding = subtract(
            &"100".parse().unwrap(),
            &"30.000000000000000001".parse().unwrap(),
        )
        .unwrap();
        assert_eq!(
            outstanding.as_decimal().to_plain_string(),
            "69.999999999999999999"
        );
        assert!(subtract(&DecimalValue::zero(), &"1".parse().unwrap()).is_err());
        let mut p = preview();
        p.funds.remaining_managed_capital = Some(money("800"));
        assert!(admit_preview(&p, time()).is_err());
    }
    #[test]
    fn capital_exit_portfolio_scope_never_manufactures_independent_cash_lots() {
        let mut p = preview();
        p.scope = CapitalExitScopeV1::PortfolioScope {
            stream_ids: vec![Id::new()],
            release_ids: vec![Id::new()],
            amount: "100".parse().unwrap(),
            currency: "USD".into(),
        };
        assert_eq!(
            admit_preview(&p, time()),
            Err(DomainError::CapabilityUnavailable(
                "capital_exit_portfolio_scope_unavailable"
            ))
        );
    }
}
