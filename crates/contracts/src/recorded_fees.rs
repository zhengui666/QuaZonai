//! Exact original source decimals, before native account rounding.
use bigdecimal::BigDecimal;
use serde::{Deserialize, Serialize};
use std::str::FromStr;
use utoipa::ToSchema;

/// Exact original price/notional/fee text, before native account rounding.
/// Source decimals need not fit the accounting NUMERIC(38,18) contract.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ToSchema)]
#[serde(try_from = "String", into = "String")]
pub struct RecordedSourceDecimal(String);

impl FromStr for RecordedSourceDecimal {
    type Err = String;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        if text.is_empty()
            || text
                .bytes()
                .any(|byte| !byte.is_ascii_digit() && !matches!(byte, b'.' | b'-' | b'+'))
        {
            return Err("expected a finite plain source decimal string".into());
        }
        BigDecimal::from_str(text).map_err(|_| "invalid source decimal")?;
        Ok(Self(text.to_owned()))
    }
}

impl TryFrom<String> for RecordedSourceDecimal {
    type Error = String;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<RecordedSourceDecimal> for String {
    fn from(value: RecordedSourceDecimal) -> Self {
        value.0
    }
}

impl RecordedSourceDecimal {
    pub fn is_positive(&self) -> bool {
        BigDecimal::from_str(&self.0).expect("validated source decimal") > BigDecimal::from(0)
    }

    pub fn to_decimal(&self) -> BigDecimal {
        BigDecimal::from_str(&self.0).expect("validated source decimal")
    }
}
