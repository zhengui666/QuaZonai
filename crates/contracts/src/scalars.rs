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
#[schema(value_type = String, format = Uuid, pattern = "^[0-9a-fA-F]{8}-[0-9a-fA-F]{4}-7[0-9a-fA-F]{3}-[89aAbB][0-9a-fA-F]{3}-[0-9a-fA-F]{12}$")]
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
        if !value.hyphenated().to_string().eq_ignore_ascii_case(&text)
            || value.get_version() != Some(Version::SortRand)
            || value.get_variant() != Variant::RFC4122
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

/// Describe the native bigint boundary without a second number parser. Each
/// alternative shares a prefix with i64::MAX and has a strictly smaller next
/// digit, or is the exact maximum. The end assertion rejects a final newline.
fn bigint_schema(nonzero: bool) -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    let maximum = i64::MAX.to_string();
    let mut alternatives = vec![format!("[1-9][0-9]{{0,{}}}", maximum.len() - 2)];
    if !nonzero {
        alternatives.push("0".into());
    }
    for (index, byte) in maximum.bytes().enumerate() {
        let digit = byte - b'0';
        let first_allowed = u8::from(index == 0);
        if digit > first_allowed {
            let prefix = &maximum[..index];
            let last_allowed = digit - 1;
            let remaining = maximum.len() - index - 1;
            alternatives.push(format!(
                "{prefix}[{first_allowed}-{last_allowed}][0-9]{{{remaining}}}"
            ));
        }
    }
    alternatives.push(maximum);
    utoipa::openapi::schema::ObjectBuilder::new()
        .schema_type(utoipa::openapi::schema::Type::String)
        .description(Some("Canonical decimal string in the PostgreSQL signed bigint range; nonnegative counters or positive revisions."))
        .min_length(Some(1))
        .max_length(Some(19))
        .pattern(Some(format!(r"^(?:{})(?![\s\S])", alternatives.join("|"))))
        .into()
}

impl utoipa::PartialSchema for DbCounter {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        bigint_schema(false)
    }
}
impl ToSchema for DbCounter {}
impl utoipa::PartialSchema for Revision {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        bigint_schema(true)
    }
}
impl ToSchema for Revision {}

/// Non-negative PostgreSQL bigint encoded as a JSON string, including zero.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
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
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
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
#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub struct DecimalValue(BigDecimal);

impl utoipa::PartialSchema for DecimalValue {
    fn schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
        // The final lookahead anchors the true end, including a trailing newline.
        // Leading zeros and trailing fractional zeros match BigDecimal's accepted
        // exact inputs; significant precision is capped at 20 + 18 digits.
        utoipa::openapi::schema::ObjectBuilder::new()
            .schema_type(utoipa::openapi::schema::Type::String)
            .description(Some(
                "Plain decimal exactly representable by NUMERIC(38,18).",
            ))
            .min_length(Some(1))
            .max_length(Some(64))
            .pattern(Some(
                r"^[+-]?(?:0*[0-9]{1,20}(?:\.[0-9]{0,18}0*)?|\.[0-9]{1,18}0*)(?![\s\S])",
            ))
            .into()
    }
}
impl ToSchema for DecimalValue {}

impl DecimalValue {
    pub fn as_decimal(&self) -> &BigDecimal {
        &self.0
    }

    pub fn zero() -> Self {
        Self(BigDecimal::from(0))
    }

    pub fn is_nonnegative(&self) -> bool {
        self.0 >= BigDecimal::from(0)
    }

    /// Native exact addition, revalidated against the same NUMERIC(38,18) range.
    /// No floating-point budget accounting or silent rounding.
    pub fn checked_add(&self, other: &Self) -> Option<Self> {
        (&self.0 + &other.0).to_plain_string().parse().ok()
    }

    pub fn is_positive(&self) -> bool {
        self.0 > BigDecimal::from(0)
    }

    pub fn is_fraction(&self) -> bool {
        self.0 >= BigDecimal::from(0) && self.0 <= BigDecimal::from(1)
    }

    /// Compare a finite observable metric with the exact frozen threshold.
    pub fn compare_metric(&self, value: f64) -> Result<std::cmp::Ordering, String> {
        if !value.is_finite() {
            return Err("non-finite metric".into());
        }
        // Preserve the observable JSON number and the complete frozen decimal.
        // Do not round the threshold or invent a binary floating-point tail.
        let wire = serde_json::to_string(&value).map_err(|_| "invalid metric")?;
        let metric = BigDecimal::from_str(&wire).map_err(|_| "invalid metric decimal")?;
        Ok(metric.cmp(&self.0))
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

/// Native DecimalValue precision plus the exact [0, 1] wire subset. The existing
/// BigDecimal implementation remains the sole numeric validation authority.
pub fn fraction_schema() -> utoipa::openapi::RefOr<utoipa::openapi::schema::Schema> {
    use utoipa::openapi::schema::{AllOfBuilder, ObjectBuilder, Type};
    AllOfBuilder::new()
        .item(<DecimalValue as utoipa::PartialSchema>::schema())
        .item(ObjectBuilder::new().schema_type(Type::String).pattern(Some(
            r"^(?:\+?(?:0*1(?:\.0*)?|0+(?:\.[0-9]*)?|\.[0-9]+)|-(?:0+(?:\.0*)?|\.0+))(?![\s\S])",
        )))
        .into()
}
