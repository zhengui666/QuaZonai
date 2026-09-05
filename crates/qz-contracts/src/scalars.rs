//! Use native UUID/time/decimal implementations; never pass precise IDs or money
//! through a JavaScript number or a floating-point decimal approximation.

use std::{fmt, str::FromStr};

use bigdecimal::BigDecimal;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use utoipa::ToSchema;
use uuid::{Uuid, Variant, Version};

pub type Timestamp = DateTime<Utc>;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct SchemaV1;

impl utoipa::PartialSchema for SchemaV1 {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::Integer)
            .minimum(Some(1))
            .maximum(Some(1))
            .enum_values(Some([1]))
            .into()
    }
}
impl ToSchema for SchemaV1 {}

impl Serialize for SchemaV1 {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(1)
    }
}

impl<'de> Deserialize<'de> for SchemaV1 {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        match u8::deserialize(deserializer)? {
            1 => Ok(Self),
            _ => Err(serde::de::Error::custom(
                "unsupported schema version; expected 1",
            )),
        }
    }
}

#[derive(
    Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema,
)]
#[serde(try_from = "String", into = "String")]
#[schema(value_type = String, format = Uuid)]
pub struct Id(Uuid);

impl Id {
    pub fn new() -> Self {
        Self(Uuid::now_v7())
    }

    pub fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl Default for Id {
    fn default() -> Self {
        Self::new()
    }
}

impl TryFrom<String> for Id {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        let value = Uuid::parse_str(&text).map_err(|_| "invalid UUID".to_owned())?;
        if value.get_version() != Some(Version::SortRand) || value.get_variant() != Variant::RFC4122
        {
            return Err("expected a UUIDv7 local identity".into());
        }
        Ok(Self(value))
    }
}

impl From<Id> for String {
    fn from(id: Id) -> Self {
        id.0.to_string()
    }
}

impl fmt::Display for Id {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

fn parse_db_unsigned(text: &str, nonzero: bool) -> Result<u64, String> {
    if text.is_empty()
        || !text.bytes().all(|byte| byte.is_ascii_digit())
        || (text.len() > 1 && text.starts_with('0'))
    {
        return Err("expected a canonical unsigned decimal string".into());
    }
    let value = text.parse::<u64>().map_err(|_| "integer out of range")?;
    if value > i64::MAX as u64 || (nonzero && value == 0) {
        return Err("integer outside PostgreSQL bigint contract".into());
    }
    Ok(value)
}

/// Non-negative PostgreSQL bigint encoded as a JSON string, including zero.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "String", into = "String")]
#[schema(value_type = String, pattern = "^(0|[1-9][0-9]*)$", example = "42")]
pub struct DbCounter(u64);

impl DbCounter {
    pub const ZERO: Self = Self(0);

    pub fn new(value: u64) -> Result<Self, String> {
        if value > i64::MAX as u64 {
            return Err("integer outside PostgreSQL bigint contract".into());
        }
        Ok(Self(value))
    }

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn checked_add(self, amount: u64) -> Option<Self> {
        self.0
            .checked_add(amount)
            .and_then(|value| Self::new(value).ok())
    }
}

impl TryFrom<String> for DbCounter {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        parse_db_unsigned(&text, false).map(Self)
    }
}

impl From<DbCounter> for String {
    fn from(value: DbCounter) -> Self {
        value.0.to_string()
    }
}

/// Positive PostgreSQL bigint revision. JSON numeric values are never accepted.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "String", into = "String")]
#[schema(value_type = String, pattern = "^[1-9][0-9]*$", example = "1")]
pub struct Revision(u64);

impl Revision {
    pub const INITIAL: Self = Self(1);

    pub fn get(self) -> u64 {
        self.0
    }

    pub fn next(self) -> Option<Self> {
        self.0
            .checked_add(1)
            .filter(|value| *value <= i64::MAX as u64)
            .map(Self)
    }
}

impl TryFrom<String> for Revision {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        parse_db_unsigned(&text, true).map(Self)
    }
}

impl From<Revision> for String {
    fn from(value: Revision) -> Self {
        value.0.to_string()
    }
}

/// NUMERIC(38,18), not rust_decimal's smaller 96-bit coefficient or f64.
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, ToSchema)]
#[schema(value_type = String, example = "0.250000000000000001")]
pub struct DecimalValue(BigDecimal);

impl DecimalValue {
    pub fn as_decimal(&self) -> &BigDecimal {
        &self.0
    }

    pub fn is_positive(&self) -> bool {
        self.0 > BigDecimal::from(0)
    }

    pub fn is_fraction(&self) -> bool {
        self.0 >= BigDecimal::from(0) && self.0 <= BigDecimal::from(1)
    }

    /// Only statistical metric thresholds use floating point, never money or weights.
    pub fn metric_threshold(&self) -> Result<f64, String> {
        let value: f64 = self
            .0
            .to_string()
            .parse()
            .map_err(|_| "invalid metric threshold")?;
        if value.is_finite() {
            Ok(value)
        } else {
            Err("non-finite metric threshold".into())
        }
    }
}

impl FromStr for DecimalValue {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        // Bound parsing work before the arbitrary-precision library allocates.
        if text.is_empty()
            || text.len() > 64
            || text.trim() != text
            || text
                .bytes()
                .any(|byte| !byte.is_ascii_digit() && !matches!(byte, b'.' | b'-' | b'+'))
        {
            return Err("expected a bounded plain decimal string".into());
        }
        let value = BigDecimal::from_str(text)
            .map_err(|_| "invalid decimal")?
            .normalized();
        let (_, scale) = value.as_bigint_and_exponent();
        let limit = BigDecimal::from_str("100000000000000000000").expect("fixed decimal bound");
        if scale > 18 || value.abs() >= limit {
            return Err("decimal outside NUMERIC(38,18); rounding is not allowed".into());
        }
        Ok(Self(value))
    }
}

impl Serialize for DecimalValue {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0.to_plain_string())
    }
}

impl<'de> Deserialize<'de> for DecimalValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        String::deserialize(deserializer)?
            .parse()
            .map_err(serde::de::Error::custom)
    }
}
