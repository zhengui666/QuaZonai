//! Version-one account-capital control. This is not a target or a trading ledger.
use crate::{
    DbCounter, DecimalValue, Id, Revision, SchemaV1, account_observation::AccountMoneyV1,
    forward::ForwardEnvironmentV1,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum CapitalExitScopeV1 {
    Amount {
        amount: DecimalValue,
        currency: String,
    },
    PortfolioScope {
        stream_ids: Vec<Id>,
        release_ids: Vec<Id>,
        amount: DecimalValue,
        currency: String,
    },
}
impl CapitalExitScopeV1 {
    pub fn money(&self) -> AccountMoneyV1 {
        match self {
            Self::Amount { amount, currency }
            | Self::PortfolioScope {
                amount, currency, ..
            } => AccountMoneyV1 {
                amount: amount.clone(),
                currency: currency.clone(),
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitReductionLegV1 {
    pub instrument_id: String,
    pub maximum_reduction_quantity: DecimalValue,
    pub minimum_sell_price: DecimalValue,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitCostLimitV1 {
    pub amount: DecimalValue,
    pub currency: String,
    pub reference_evidence_id: Id,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum CapitalExitPolicyV1 {
    CashOnly {},
    BoundedLimit {
        deadline: DateTime<Utc>,
        legs: Vec<CapitalExitReductionLegV1>,
        max_execution_cost: CapitalExitCostLimitV1,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitPreviewRequestV1 {
    pub schema_version: SchemaV1,
    pub account_source_id: Id,
    pub expected_source_observation_id: Id,
    pub scope: CapitalExitScopeV1,
    pub policy: CapitalExitPolicyV1,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapitalExitCapabilityV1 {
    Supported,
    Unsupported,
    Blocked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapitalExitStateV1 {
    Requested,
    Fencing,
    CancellingOpeners,
    Reducing,
    WaitingEvidence,
    Paused,
    CancellingExit,
    CancelledReserved,
    ReconcilingWithdrawal,
    Completed,
    Blocked,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapitalExitWithdrawabilityV1 {
    Unverified,
    Verified,
    Stale,
    Simulated,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapitalExitRemainingTradingV1 {
    FencedPendingTarget,
    BudgetAware,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitNativeOrderRefV1 {
    pub native_client_order_id: String,
    pub native_strategy_id: String,
    pub native_instrument_id: String,
    pub original_event_refs: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitPreviewFundsV1 {
    pub requested: AccountMoneyV1,
    pub native_total_cash: Option<AccountMoneyV1>,
    pub native_free_cash: Option<AccountMoneyV1>,
    pub native_locked_cash: Option<AccountMoneyV1>,
    pub verified_idle_cash: Option<AccountMoneyV1>,
    pub estimated_release: Option<AccountMoneyV1>,
    pub native_equity: Option<AccountMoneyV1>,
    pub managed_capital_before: Option<AccountMoneyV1>,
    pub remaining_managed_capital: Option<AccountMoneyV1>,
    pub estimated_execution_cost: Option<AccountMoneyV1>,
    pub existing_unrealized_pnl: Option<AccountMoneyV1>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitPreviewV1 {
    pub schema_version: SchemaV1,
    pub id: Id,
    pub project_id: Id,
    pub account_source_id: Id,
    pub original_observation_id: Id,
    pub environment: ForwardEnvironmentV1,
    /// Resolved from trusted owner configuration, never from the request.
    pub managed_account_key: Option<String>,
    pub owner_binding_ref: Option<String>,
    pub expected_account_control_revision: Option<Revision>,
    pub scope: CapitalExitScopeV1,
    pub policy: CapitalExitPolicyV1,
    pub plan_artifact_id: Id,
    pub evidence_refs: Vec<Id>,
    pub valid_until: DateTime<Utc>,
    pub funds: CapitalExitPreviewFundsV1,
    pub proposed_cancellations: Vec<CapitalExitNativeOrderRefV1>,
    pub retained_protective_orders: Vec<CapitalExitNativeOrderRefV1>,
    pub reduction_legs: Vec<CapitalExitReductionLegV1>,
    pub remaining_risk_evidence_id: Option<Id>,
    pub capability: CapitalExitCapabilityV1,
    pub reason_codes: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitStartV1 {
    pub schema_version: SchemaV1,
    pub preview_id: Id,
    pub expected_account_control_revision: Revision,
    pub acknowledged_plan_artifact_id: Id,
    pub expected_source_observation_id: Id,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "action",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum CapitalExitActionV1 {
    Pause {
        schema_version: SchemaV1,
        expected_revision: Revision,
    },
    Cancel {
        schema_version: SchemaV1,
        expected_revision: Revision,
    },
    Resume {
        schema_version: SchemaV1,
        expected_revision: Revision,
        preview_id: Id,
    },
    ReconcileWithdrawal {
        schema_version: SchemaV1,
        expected_revision: Revision,
        user_reported_amount: DecimalValue,
        currency: String,
        external_transfer_ref: Option<String>,
    },
}
impl CapitalExitActionV1 {
    pub fn expected_revision(&self) -> Revision {
        match self {
            Self::Pause {
                expected_revision, ..
            }
            | Self::Cancel {
                expected_revision, ..
            }
            | Self::Resume {
                expected_revision, ..
            }
            | Self::ReconcileWithdrawal {
                expected_revision, ..
            } => *expected_revision,
        }
    }
    pub fn action(&self) -> &'static str {
        match self {
            Self::Pause { .. } => "PAUSE",
            Self::Cancel { .. } => "CANCEL",
            Self::Resume { .. } => "RESUME",
            Self::ReconcileWithdrawal { .. } => "RECONCILE_WITHDRAWAL",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitFundsV1 {
    pub requested_amount: AccountMoneyV1,
    pub reserved_amount: AccountMoneyV1,
    pub released_cash_amount: Option<AccountMoneyV1>,
    pub verified_withdrawable_amount: Option<AccountMoneyV1>,
    pub reconciled_withdrawal_amount: AccountMoneyV1,
    pub unreleased_amount: Option<AccountMoneyV1>,
    pub evidence_asof: Option<DateTime<Utc>>,
    pub evidence_valid_until: Option<DateTime<Utc>>,
    pub withdrawability: CapitalExitWithdrawabilityV1,
}
/// Original instruction for this exact owner command. A lifecycle phase alone
/// cannot recover the user's withdrawal report or authorize further reductions.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(
    tag = "action",
    rename_all = "SCREAMING_SNAKE_CASE",
    deny_unknown_fields
)]
pub enum CapitalExitOwnerInstructionV1 {
    Start {},
    Pause {},
    Cancel {},
    Resume {},
    ReconcileWithdrawal {
        user_reported_amount: DecimalValue,
        currency: String,
        external_transfer_ref: Option<String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitOwnerCommandV1 {
    pub schema_version: SchemaV1,
    pub command_id: Id,
    pub account_control_epoch: DbCounter,
    pub instruction: CapitalExitOwnerInstructionV1,
}
impl CapitalExitOwnerInstructionV1 {
    pub fn from_action(value: &CapitalExitActionV1) -> Self {
        match value {
            CapitalExitActionV1::Pause { .. } => Self::Pause {},
            CapitalExitActionV1::Cancel { .. } => Self::Cancel {},
            CapitalExitActionV1::Resume { .. } => Self::Resume {},
            CapitalExitActionV1::ReconcileWithdrawal {
                user_reported_amount,
                currency,
                external_transfer_ref,
                ..
            } => Self::ReconcileWithdrawal {
                user_reported_amount: user_reported_amount.clone(),
                currency: currency.clone(),
                external_transfer_ref: external_transfer_ref.clone(),
            },
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitViewV1 {
    pub schema_version: SchemaV1,
    pub id: Id,
    pub project_id: Id,
    pub account_source_id: Id,
    pub environment: ForwardEnvironmentV1,
    pub managed_account_key: String,
    pub owner_binding_ref: String,
    pub account_control_epoch: DbCounter,
    pub account_control_revision: Revision,
    pub revision: Revision,
    pub state: CapitalExitStateV1,
    pub last_phase: CapitalExitStateV1,
    pub reason_codes: Vec<String>,
    pub preview_id: Id,
    pub plan_artifact_id: Id,
    pub scope: CapitalExitScopeV1,
    pub policy: CapitalExitPolicyV1,
    /// Changes on each owner instruction. Claims and evidence bind this exact command.
    pub command_id: Id,
    pub owner_command: CapitalExitOwnerCommandV1,
    pub external_claim_id: Option<String>,
    pub remaining_trading: CapitalExitRemainingTradingV1,
    pub funds: CapitalExitFundsV1,
    pub evidence_refs: Vec<Id>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitClaimV1 {
    pub schema_version: SchemaV1,
    pub expected_revision: Revision,
    pub command_id: Id,
    pub account_control_epoch: DbCounter,
    pub account_source_id: Id,
    pub owner_binding_ref: String,
    pub external_claim_id: String,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CapitalExitAvailabilityBasisV1 {
    NativeVenueAvailable,
    ControlledSandbox,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum CapitalExitEvidenceKindV1 {
    FenceApplied {
        native_gate_report_ref: String,
        controlled_strategy_ids: Vec<String>,
        invalidated_target_claim_refs: Vec<String>,
        invalidated_child_timer_refs: Vec<String>,
        resolved_inflight_report_ref: String,
        remaining_trading: CapitalExitRemainingTradingV1,
        remaining_target_authority_ref: Option<String>,
    },
    NativeProgress {
        phase: CapitalExitStateV1,
        released_cash_amount: Option<AccountMoneyV1>,
        native_order_refs: Vec<CapitalExitNativeOrderRefV1>,
        native_position_refs: Vec<String>,
        native_report_ref: String,
        reason_codes: Vec<String>,
    },
    WithdrawabilityObserved {
        available_cash: AccountMoneyV1,
        basis: CapitalExitAvailabilityBasisV1,
        venue: String,
        native_availability_report_ref: String,
        settlement_report_ref: String,
        margin_report_ref: String,
        open_orders_report_ref: String,
    },
    WithdrawalReconciled {
        amount: AccountMoneyV1,
        native_cash_movement_ref: String,
        external_transfer_ref: String,
        before_observation_id: Id,
        after_observation_id: Id,
        /// Already observed after the transfer; never subtract amount from it again.
        observed_managed_capital_after: AccountMoneyV1,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitOwnerEvidenceV1 {
    pub schema_version: SchemaV1,
    pub intent_id: Id,
    pub command_id: Id,
    pub account_source_id: Id,
    pub owner_binding_ref: String,
    pub native_session_id: String,
    pub native_account_id: String,
    pub account_control_epoch: DbCounter,
    pub external_claim_id: String,
    pub external_message_id: String,
    pub sequence: DbCounter,
    pub source_observation_id: Id,
    pub asof: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub original_event_refs: Vec<String>,
    /// Exact original report, retained immutably on intake. No server-synthesized ledger.
    pub native_evidence: serde_json::Value,
    pub evidence: CapitalExitEvidenceKindV1,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitEvidenceViewV1 {
    pub id: Id,
    pub intent_id: Id,
    pub artifact_id: Id,
    pub content: CapitalExitOwnerEvidenceV1,
    pub received_at: DateTime<Utc>,
}

/// Original owner-computed read-side assessment. Publishing this never starts an
/// exit or grants execution authority. The configured owner supplies native facts.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitOwnerAssessmentV1 {
    pub schema_version: SchemaV1,
    pub account_source_id: Id,
    pub owner_binding_ref: String,
    pub expected_account_control_revision: Revision,
    pub external_message_id: String,
    pub sequence: DbCounter,
    pub request: CapitalExitPreviewRequestV1,
    pub asof: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub funds: CapitalExitPreviewFundsV1,
    pub proposed_cancellations: Vec<CapitalExitNativeOrderRefV1>,
    pub retained_protective_orders: Vec<CapitalExitNativeOrderRefV1>,
    pub reduction_legs: Vec<CapitalExitReductionLegV1>,
    pub evidence_refs: Vec<Id>,
    pub remaining_risk_evidence_id: Option<Id>,
    pub native_report_ref: String,
    pub native_evidence: serde_json::Value,
    pub capability: CapitalExitCapabilityV1,
    pub reason_codes: Vec<String>,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct CapitalExitAssessmentReceiptV1 {
    pub id: Id,
    pub artifact_id: Id,
    pub account_source_id: Id,
    pub external_message_id: String,
    pub received_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn capital_exit_exact_scalars_and_closed_request_reject_authority_injection() {
        let value = json!({"schema_version":1,"account_source_id":Id::new(),"expected_source_observation_id":Id::new(),"scope":{"kind":"AMOUNT","amount":"0.000000000000000001","currency":"USD"},"policy":{"kind":"CASH_ONLY"}});
        assert!(serde_json::from_value::<CapitalExitPreviewRequestV1>(value.clone()).is_ok());
        for (field, replacement) in [
            ("schema_version", json!(2)),
            ("managed_account_key", json!("caller-selected-account")),
            ("ready", json!(true)),
        ] {
            let mut bad = value.clone();
            bad[field] = replacement;
            assert!(serde_json::from_value::<CapitalExitPreviewRequestV1>(bad).is_err());
        }
        let mut bad = value.clone();
        bad["scope"]["amount"] = json!(0.1);
        assert!(serde_json::from_value::<CapitalExitPreviewRequestV1>(bad).is_err());
        let mut bad = value;
        bad["policy"]["deadline"] = json!("2026-10-07T15:00:00Z");
        assert!(serde_json::from_value::<CapitalExitPreviewRequestV1>(bad).is_err());
    }
    #[test]
    fn capital_exit_action_is_closed_and_has_no_transfer_action() {
        let pause = json!({"schema_version":1,"expected_revision":"1","action":"PAUSE"});
        let parsed: CapitalExitActionV1 = serde_json::from_value(pause.clone()).unwrap();
        assert_eq!(parsed.action(), "PAUSE");
        let mut bad = pause.clone();
        bad["preview_id"] = json!(Id::new());
        assert!(serde_json::from_value::<CapitalExitActionV1>(bad).is_err());
        let mut bad = pause.clone();
        bad["expected_revision"] = json!(1);
        assert!(serde_json::from_value::<CapitalExitActionV1>(bad).is_err());
        let mut bad = pause;
        bad["action"] = json!("WITHDRAW");
        assert!(serde_json::from_value::<CapitalExitActionV1>(bad).is_err());
        let resume = json!({"schema_version":1,"expected_revision":"1","action":"RESUME"});
        assert!(serde_json::from_value::<CapitalExitActionV1>(resume).is_err());
    }
    #[test]
    fn capital_exit_control_schema_remains_one_and_target_package_two() {
        use utoipa::OpenApi;
        #[derive(OpenApi)]
        #[openapi(components(schemas(
            CapitalExitPreviewRequestV1,
            CapitalExitActionV1,
            CapitalExitOwnerEvidenceV1,
            CapitalExitViewV1
        )))]
        struct Api;
        let document = serde_json::to_value(Api::openapi()).unwrap();
        for name in [
            "CapitalExitPreviewRequestV1",
            "CapitalExitOwnerEvidenceV1",
            "CapitalExitViewV1",
        ] {
            assert_eq!(
                document["components"]["schemas"][name]["additionalProperties"],
                false
            );
        }
        fn refs(value: &serde_json::Value, root: &serde_json::Value) {
            match value {
                serde_json::Value::Object(v) => {
                    if let Some(r) = v.get("$ref").and_then(|v| v.as_str()) {
                        assert!(root.pointer(&r[1..]).is_some(), "{r}");
                    }
                    for child in v.values() {
                        refs(child, root);
                    }
                }
                serde_json::Value::Array(v) => {
                    for child in v {
                        refs(child, root)
                    }
                }
                _ => {}
            }
        }
        refs(&document, &document);
        assert_eq!(serde_json::to_value(SchemaV1).unwrap(), json!(1));
        assert_eq!(
            serde_json::to_value(crate::strategy_portfolio::TargetPackageVersionV2::V2).unwrap(),
            json!("2")
        );
    }
}

#[cfg(test)]
mod owner_command_tests {
    use super::*;
    #[test]
    fn capital_exit_owner_instruction_exposes_exact_report_and_rejects_unbound_fields() {
        let action = CapitalExitActionV1::ReconcileWithdrawal {
            schema_version: SchemaV1,
            expected_revision: Revision::INITIAL,
            user_reported_amount: "0.000000000000000001".parse().unwrap(),
            currency: "USD".into(),
            external_transfer_ref: Some("native-transfer-reference".into()),
        };
        let command = CapitalExitOwnerCommandV1 {
            schema_version: SchemaV1,
            command_id: Id::new(),
            account_control_epoch: DbCounter::new(9007199254740993).unwrap(),
            instruction: CapitalExitOwnerInstructionV1::from_action(&action),
        };
        let wire = serde_json::to_value(&command).unwrap();
        assert_eq!(wire["schema_version"], 1);
        assert_eq!(wire["account_control_epoch"], "9007199254740993");
        assert_eq!(wire["instruction"]["action"], "RECONCILE_WITHDRAWAL");
        assert_eq!(
            wire["instruction"]["user_reported_amount"],
            "0.000000000000000001"
        );
        assert_eq!(
            serde_json::from_value::<CapitalExitOwnerCommandV1>(wire.clone()).unwrap(),
            command
        );
        let mut bad = wire;
        bad["ready"] = serde_json::json!(true);
        assert!(serde_json::from_value::<CapitalExitOwnerCommandV1>(bad).is_err());
        assert!(
            serde_json::from_value::<CapitalExitOwnerInstructionV1>(
                serde_json::json!({"action":"PAUSE","user_reported_amount":"1"})
            )
            .is_err()
        );
    }
}
