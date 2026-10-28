//! Explicit exploratory native TradeTick factor inputs. BAR contracts are unchanged.
use crate::{
    factors::{
        NativeFactorAssetMetricV1, NativeFactorMetricV1, NativeFactorPartitionReportV1,
        NativeFactorPartitionV1, NativeFactorPointV1,
    },
    DbCounter, SchemaV1,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use utoipa::{PartialSchema, ToSchema};

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

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
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

// Utoipa's internally tagged derive leaves variant objects open despite Serde's strict fields.
impl PartialSchema for NativeTickTargetV1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        use utoipa::openapi::schema::{AdditionalProperties, ObjectBuilder, OneOfBuilder, Type};
        let variant = |kind| {
            ObjectBuilder::new()
                .schema_type(Type::Object)
                .additional_properties(Some(AdditionalProperties::FreeForm(false)))
                .property(
                    "kind",
                    ObjectBuilder::new()
                        .schema_type(Type::String)
                        .enum_values(Some([kind])),
                )
                .required("kind")
        };
        OneOfBuilder::new()
            .item(
                variant("FIXED_TRADE_COHORTS")
                    .property("horizon_cohorts", u32::schema())
                    .required("horizon_cohorts"),
            )
            .item(
                variant("FIXED_EVENT_TIME")
                    .property("horizon_ns", DbCounter::schema())
                    .required("horizon_ns"),
            )
            .into()
    }
}
impl ToSchema for NativeTickTargetV1 {}

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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub evaluation_regimes: Option<NativeTickRegimeEvaluationV1>,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeTickRegimeMethodV1 {
    #[serde(rename = "BTC_COMPLETED_30_DAY_RETURN_V1")]
    BtcCompleted30DayReturnV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickRegimeMarkV1 {
    pub event_ns: DbCounter,
    /// Original positive plain decimal VWAP, without NUMERIC(38,18) rounding.
    #[schema(min_length = 1, max_length = 512)]
    pub price: String,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickRegimeDayV1 {
    pub day_start_ns: DbCounter,
    #[serde(deserialize_with = "Option::deserialize")]
    #[schema(required = true)]
    pub previous: Option<NativeTickRegimeMarkV1>,
    #[serde(deserialize_with = "Option::deserialize")]
    #[schema(required = true)]
    pub lookback: Option<NativeTickRegimeMarkV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeTickRegimeEvaluationV1 {
    pub kind: NativeTickRegimeMethodV1,
    pub days: Vec<NativeTickRegimeDayV1>,
}

impl NativeTradeTickFactorEvaluationRequestV1 {
    pub fn clip_end(&mut self, end: DbCounter) {
        self.input.event_end_ns = end;
        if let Some(regimes) = &mut self.evaluation_regimes {
            regimes.days.retain(|day| day.day_start_ns < end);
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeTickRegimeV1 {
    Bull,
    Bear,
    Range,
    Unknown,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeTickLabelSpanBucketV1 {
    pub lower_ns: DbCounter,
    pub upper_ns: Option<DbCounter>,
    pub count: DbCounter,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeTickLabelSpansV1 {
    pub count: DbCounter,
    pub min_ns: Option<DbCounter>,
    pub max_ns: Option<DbCounter>,
    pub buckets: Vec<NativeTickLabelSpanBucketV1>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeTickRegimeMetricV1 {
    pub regime: NativeTickRegimeV1,
    pub days: DbCounter,
    pub metric: NativeFactorMetricV1,
    pub assets: Vec<NativeFactorAssetMetricV1>,
    pub observed_assets: DbCounter,
    pub included_assets: DbCounter,
    pub label_spans: NativeTickLabelSpansV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
pub struct NativeTickRegimePartitionV1 {
    pub partition: NativeFactorPartitionV1,
    pub start_ns: DbCounter,
    pub end_ns: DbCounter,
    pub regimes: Vec<NativeTickRegimeMetricV1>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub regime_partitions: Option<Vec<NativeTickRegimePartitionV1>>,
    /// Omitted on the summary-only path; labels remain evaluation evidence.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub points: Option<Vec<NativeFactorPointV1>>,
}
