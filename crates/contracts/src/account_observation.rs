//! Read-only projections of official Nautilus account snapshots. No strategy attribution.
use crate::{forward::ForwardEnvironmentV1, DbCounter, DecimalValue, Id, SchemaV1};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

pub const NATIVE_ACCOUNT_VERSION: &str = "0.63.0";
pub const CONNECTION_STALE_SECONDS: i64 = 120;

/// Exact wire version for observations retained by the opt-in native-client producer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeClientObservationSchemaV2;

impl Serialize for NativeClientObservationSchemaV2 {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(2)
    }
}

impl<'de> Deserialize<'de> for NativeClientObservationSchemaV2 {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            2 => Ok(Self),
            _ => Err(serde::de::Error::custom(
                "native client observation requires version 2",
            )),
        }
    }
}

impl utoipa::PartialSchema for NativeClientObservationSchemaV2 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::Integer)
            .enum_values(Some([2]))
            .minimum(Some(2))
            .maximum(Some(2))
            .into()
    }
}
impl ToSchema for NativeClientObservationSchemaV2 {}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountMoneyV1 {
    pub amount: DecimalValue,
    pub currency: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeAccountBalanceV1 {
    pub total: AccountMoneyV1,
    pub locked: AccountMoneyV1,
    pub free: AccountMoneyV1,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeAccountMarginV1 {
    pub initial: AccountMoneyV1,
    pub maintenance: AccountMoneyV1,
    pub instrument_id: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum NativeAccountTypeV1 {
    Cash,
    Margin,
    Betting,
    Wallet,
}

/// Lossless public field projection; no equity or PnL arithmetic occurs in Q.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativePortfolioSnapshotV1 {
    pub account_id: String,
    pub account_type: NativeAccountTypeV1,
    pub base_currency: Option<String>,
    pub balances: Vec<NativeAccountBalanceV1>,
    pub margins: Vec<NativeAccountMarginV1>,
    pub unrealized_pnls: Vec<AccountMoneyV1>,
    /// Native session/cache history only, never broker lifetime profit.
    pub realized_pnls: Vec<AccountMoneyV1>,
    pub total_equity: Vec<AccountMoneyV1>,
    pub base_currency_equity: Option<AccountMoneyV1>,
    pub is_stale: bool,
    pub stale_instruments: Vec<String>,
    pub stale_currencies: Vec<String>,
    pub unpriced_instruments: Vec<String>,
    pub event_id: String,
    pub ts_event: DbCounter,
    pub ts_init: DbCounter,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeAccountBindingV1 {
    pub schema_version: SchemaV1,
    pub project_id: Id,
    pub environment: ForwardEnvironmentV1,
    pub native_trader_id: String,
    /// Stable for this observer/node session. Restart with a new identity when the cursor is lost.
    pub native_session_id: String,
    pub native_account_id: String,
    pub native_version: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountConnectionV1 {
    Unknown,
    Connected,
    Disconnected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountObservationSubmitV1 {
    pub schema_version: SchemaV1,
    pub binding: NativeAccountBindingV1,
    /// Starts at one. Every attempted emission, including a dropped frame, consumes a sequence.
    pub sequence: DbCounter,
    pub dropped_events: DbCounter,
    /// Source heartbeat time, separate from the native valuation and server receipt clocks.
    pub observed_at_ns: DbCounter,
    pub connection: AccountConnectionV1,
    /// None is a heartbeat. It does not overwrite or refresh the last valuation.
    pub snapshot: Option<NativePortfolioSnapshotV1>,
}

/// Original V1 native values plus the actual client checked by the node producer.
/// The receiving service trusts its authorized producer; a caller-supplied string
/// is not cryptographic proof of a native client or of a venue account connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountObservationSubmitV2 {
    pub schema_version: NativeClientObservationSchemaV2,
    #[schema(min_length = 1, max_length = 200)]
    pub native_client_id: String,
    pub observation: AccountObservationSubmitV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountObservationV1 {
    pub id: Id,
    pub source_id: Id,
    pub downstream_id: Id,
    pub observation: AccountObservationSubmitV1,
    pub gap_before: bool,
    pub received_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountObservationReceiptV1 {
    pub replayed: bool,
    pub resource: AccountObservationV1,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountObservationReceiptV2 {
    pub schema_version: NativeClientObservationSchemaV2,
    pub replayed: bool,
    pub native_client_id: String,
    pub resource: AccountObservationV1,
}

/// Additive source provenance. Account values and identity remain in the original
/// source/observation records; a V1 source has no such binding and is not upgraded.
#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountClientBindingV2 {
    pub schema_version: NativeClientObservationSchemaV2,
    pub source_id: Id,
    pub native_client_id: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountConnectionFreshnessV1 {
    Unknown,
    Connected,
    Disconnected,
    Stale,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountSourceV1 {
    pub id: Id,
    pub downstream_id: Id,
    pub binding: NativeAccountBindingV1,
    pub last_sequence: DbCounter,
    pub dropped_events: DbCounter,
    pub has_gap: bool,
    pub last_observation_id: Id,
    pub latest_snapshot_id: Option<Id>,
    pub connection: AccountConnectionFreshnessV1,
    pub last_observed_at_ns: DbCounter,
    pub last_received_at: chrono::DateTime<chrono::Utc>,
    pub checked_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AccountValuationV1 {
    Unavailable,
    Priced,
    Stale,
    Unpriced,
}

#[derive(Clone, Debug, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct AccountCurrentV1 {
    pub source: AccountSourceV1,
    /// Native flags describe valuation inputs, not current transport connectivity.
    pub valuation: AccountValuationV1,
    pub latest_snapshot: Option<AccountObservationV1>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::{json, Value};
    use utoipa::OpenApi;

    #[derive(OpenApi)]
    #[openapi(components(schemas(
        AccountObservationSubmitV2,
        AccountObservationReceiptV2,
        AccountClientBindingV2
    )))]
    struct ClientObservationApi;

    #[test]
    fn client_bound_schemas_are_closed_versioned_and_reference_complete() {
        let document = serde_json::to_value(ClientObservationApi::openapi()).unwrap();
        let schemas = &document["components"]["schemas"];
        let version = &schemas["NativeClientObservationSchemaV2"];
        assert_eq!(version["type"], "integer");
        assert_eq!(version["enum"], json!([2]));
        for name in [
            "AccountObservationSubmitV2",
            "AccountObservationReceiptV2",
            "AccountClientBindingV2",
        ] {
            assert_eq!(schemas[name]["additionalProperties"], false);
            assert_eq!(
                schemas[name]["properties"]["schema_version"]["$ref"],
                "#/components/schemas/NativeClientObservationSchemaV2"
            );
        }
        let submit = &schemas["AccountObservationSubmitV2"];
        let mut required: Vec<_> = submit["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        required.sort_unstable();
        assert_eq!(
            required,
            ["native_client_id", "observation", "schema_version"]
        );
        assert_eq!(submit["properties"]["native_client_id"]["minLength"], 1);
        assert_eq!(submit["properties"]["native_client_id"]["maxLength"], 200);
        assert!(schemas["NativeAccountBindingV1"]["properties"]
            .get("native_client_id")
            .is_none());

        fn check_refs(value: &Value, document: &Value) {
            match value {
                Value::Object(object) => {
                    if let Some(reference) = object.get("$ref").and_then(Value::as_str) {
                        assert!(reference.starts_with("#/"));
                        assert!(document.pointer(&reference[1..]).is_some(), "{reference}");
                    }
                    for child in object.values() {
                        check_refs(child, document);
                    }
                }
                Value::Array(array) => {
                    for child in array {
                        check_refs(child, document);
                    }
                }
                _ => {}
            }
        }
        check_refs(&document, &document);
    }
}
