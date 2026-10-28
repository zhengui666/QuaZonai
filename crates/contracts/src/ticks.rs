//! Explicit exploratory native TradeTick factor inputs. BAR contracts are unchanged.
use crate::{
    factors::{NativeFactorPartitionReportV1, NativeFactorPointV1},
    DbCounter, SchemaV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeTickModalityV1 {
    TradeTick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeTickAvailabilityV1 {
    EventTimeProxyUnverified,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeTickRepresentationV1 {
    EventTimeCohortCacheV1,
    EventTimeCohortFlowCacheV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTradeTickInputV1 {
    pub modality: NativeTickModalityV1,
    pub instrument_ids: Vec<String>,
    pub event_start_ns: DbCounter,
    /// Exclusive event boundary, independently rechecked after native queries.
    pub event_end_ns: DbCounter,
    pub decision_cutoff_ns: DbCounter,
    /// Absence retains the original native TradeTick representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub representation: Option<NativeTickRepresentationV1>,
    /// Original total selected-row cap; not a streaming memory limit.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_rows: Option<u32>,
    /// Exact full-source budget for the explicitly derived cache representation.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_source_rows: Option<DbCounter>,
    pub source_revision: String,
    pub availability: NativeTickAvailabilityV1,
}

impl NativeTradeTickInputV1 {
    pub fn is_cohort_cache(&self) -> bool {
        self.representation.is_some()
    }

    pub fn is_flow_cache(&self) -> bool {
        self.representation == Some(NativeTickRepresentationV1::EventTimeCohortFlowCacheV1)
    }

    pub fn cache_representation(&self) -> Option<&'static str> {
        self.representation
            .map(|representation| match representation {
                NativeTickRepresentationV1::EventTimeCohortCacheV1 => "EVENT_TIME_COHORT_CACHE_V1",
                NativeTickRepresentationV1::EventTimeCohortFlowCacheV1 => {
                    "EVENT_TIME_COHORT_FLOW_CACHE_V1"
                }
            })
    }

    pub fn source_row_limit(&self) -> Option<u64> {
        if self.is_cohort_cache() {
            self.maximum_source_rows.map(|value| value.get())
        } else {
            self.maximum_rows.map(u64::from)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeTickSamplingKindV1 {
    EventTimeCohortV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickSamplingV1 {
    pub kind: NativeTickSamplingKindV1,
    pub maximum_cohort_rows: u32,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "SCREAMING_SNAKE_CASE", deny_unknown_fields)]
pub enum NativeTickTargetV1 {
    FixedTradeCohorts {
        horizon_cohorts: u32,
    },
    /// First complete same-instrument cohort at or after t + horizon_ns.
    FixedEventTime {
        horizon_ns: DbCounter,
    },
}

impl NativeTickTargetV1 {
    pub fn label_method(&self) -> &'static str {
        match self {
            Self::FixedTradeCohorts { .. } => "FIXED_TRADE_COHORTS: VWAP(j+H)/VWAP(j)-1; same instrument; no labels supplied to model",
            Self::FixedEventTime { .. } => "FIXED_EVENT_TIME: VWAP(first complete same-instrument cohort with event >= t+horizon_ns)/VWAP(t)-1; actual span may exceed horizon; no labels supplied to model; not executable price",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickParametersV1 {
    pub fast_period_cohorts: u32,
    pub slow_period_cohorts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_fuel: Option<DbCounter>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTradeTickFactorEvaluationRequestV1 {
    pub schema_version: SchemaV1,
    pub input: NativeTradeTickInputV1,
    pub sampling: NativeTickSamplingV1,
    pub target: NativeTickTargetV1,
    pub parameters: NativeTickParametersV1,
    pub train_end_ns: DbCounter,
    pub validation_end_ns: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTradeTickInstrumentCoverageV1 {
    pub instrument_id: String,
    pub trade_count: DbCounter,
    pub cohort_count: DbCounter,
    /// None explicitly represents no observed trades in the requested interval.
    pub first_event_ns: Option<DbCounter>,
    pub last_event_ns: Option<DbCounter>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTradeTickCoverageV1 {
    pub modality: NativeTickModalityV1,
    pub source_revision: String,
    pub instrument_ids: Vec<String>,
    pub instrument_coverage: Vec<NativeTradeTickInstrumentCoverageV1>,
    pub trade_count: DbCounter,
    pub cohort_count: DbCounter,
    pub first_event_ns: DbCounter,
    pub last_event_ns: DbCounter,
    pub availability: NativeTickAvailabilityV1,
    pub limitations: Vec<String>,
    /// Selected streams, source paths and definition/availability limitations.
    /// Contains no original trade observations or independent labels.
    pub source_evidence: BTreeMap<String, serde_json::Value>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTradeTickFactorEvaluationResultV1 {
    pub schema_version: SchemaV1,
    pub request: NativeTradeTickFactorEvaluationRequestV1,
    pub native_versions: BTreeMap<String, String>,
    pub methods: BTreeMap<String, String>,
    pub consumed_fuel: Option<DbCounter>,
    pub coverage: NativeTradeTickCoverageV1,
    pub partitions: Vec<NativeFactorPartitionReportV1>,
    /// Omitted on the summary-only path; labels remain evaluation evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<NativeFactorPointV1>>,
}
