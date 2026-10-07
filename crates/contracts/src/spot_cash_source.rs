//! Frozen public candle source facts. Capture and complete valuation do not
//! establish historical market-data availability or research qualification.
use crate::{DbCounter, Id, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpotCashSourceMethodV1 {
    HyperliquidPublicRestCandleSnapshot,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpotCashHistoricalAvailabilityV1 {
    Unverified,
}
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SpotCashSourceNetworkV1 {
    Mainnet,
}

/// One actual parsed JSON response from the official public Info request.
/// This preserves the returned JSON value, not the HTTP wire bytes or headers.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenSpotCandleResponseV1 {
    pub sequence: DbCounter,
    pub coin: String,
    pub interval: String,
    pub requested_start_ms: DbCounter,
    /// Exact original API endTime, inclusive final millisecond of one closed minute.
    pub requested_end_ms: DbCounter,
    pub request_started_ns: DbCounter,
    pub received_ns: DbCounter,
    pub response: serde_json::Value,
}
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenSpotCandleRowV1 {
    pub response_sequence: DbCounter,
    /// Zero-based position in that unchanged original response array.
    pub source_row_index: DbCounter,
}

/// Stored beside the native Parquet in one existing registered catalog snapshot.
/// Selection may retain fewer original response objects and row locators;
/// each retained response JSON, original sequence and receipt stay unchanged.
/// This first producer admits one ordinary one-minute Hyper USDC spot pair.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct FrozenSpotCandleSourceV1 {
    pub schema_version: SchemaV1,
    /// A capture session actually created by the QZ acquisition operation, not
    /// an Artifact or Dataset identity reserved before registration.
    pub capture_id: Id,
    pub method: SpotCashSourceMethodV1,
    pub network: SpotCashSourceNetworkV1,
    pub native_version: String,
    pub historical_availability: SpotCashHistoricalAvailabilityV1,
    /// Complete original official InstrumentAny serialization from the capture.
    pub instrument_definition: serde_json::Value,
    pub instrument_request_started_ns: DbCounter,
    pub instrument_received_ns: DbCounter,
    pub bar_type: String,
    pub responses: Vec<FrozenSpotCandleResponseV1>,
    pub selected_rows: Vec<FrozenSpotCandleRowV1>,
}

/// Kept separately from valuation completeness, native receipts and fee policy.
/// Actual retrieval clocks cannot certify when old candles were first public.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSpotCashSourceEvidenceV1 {
    pub schema_version: SchemaV1,
    pub capture_id: Id,
    pub method: SpotCashSourceMethodV1,
    pub network: SpotCashSourceNetworkV1,
    pub native_version: String,
    pub historical_availability: SpotCashHistoricalAvailabilityV1,
    pub first_received_ns: DbCounter,
    pub last_received_ns: DbCounter,
    pub response_count: DbCounter,
    pub selected_row_count: DbCounter,
}

pub const SPOT_CANDLE_SOURCE_FILE: &str = "qz-spot-cash-source.json";
