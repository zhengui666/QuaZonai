//! Bounded source observations for native ContractExpired records, not a payout engine.
//! These are frozen dataset evidence, never model features or trading instructions.
use crate::{DbCounter, DecimalValue};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSettlementOutcomeV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    pub close_price: DecimalValue,
    pub ts_event: DbCounter,
    pub ts_init: DbCounter,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSettlementGroupV1 {
    #[schema(min_length = 1, max_length = 120)]
    pub condition_id: String,
    #[schema(min_length = 1, max_length = 2000)]
    pub source_reference: String,
    /// The complete two-outcome condition, even when only one token is traded.
    #[schema(min_items = 2, max_items = 2)]
    pub outcomes: Vec<NativeSettlementOutcomeV1>,
}
