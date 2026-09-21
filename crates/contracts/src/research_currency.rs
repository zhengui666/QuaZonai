//! Research units are not model-billing currencies and are never aliases for USD.
//! These spellings are the pinned Nautilus native currencies. The original instrument
//! and source evidence still determine the collateral contract and historical regime.
use utoipa::openapi::{schema::{ObjectBuilder, OneOfBuilder, Type}, RefOr, Schema};

pub const NATIVE_COLLATERAL: [&str; 3] = ["USDC", "USDC.e", "pUSD"];

pub fn supported(value: &str) -> bool {
    iso_currency::Currency::from_code(value).is_some() || NATIVE_COLLATERAL.contains(&value)
}

pub(crate) fn schema() -> RefOr<Schema> {
    OneOfBuilder::new()
        .item(crate::budget::currency_schema())
        .item(ObjectBuilder::new().schema_type(Type::String)
            .enum_values(Some(NATIVE_COLLATERAL)))
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collateral_is_explicit_case_sensitive_and_not_a_fiat_alias() {
        for code in ["USD", "EUR", "CNY", "USDC", "USDC.e", "pUSD"] {
            assert!(supported(code), "{code}");
        }
        for code in ["", "usd", "PUSD", "usdc", "USDT", "BTC", "USD ", " USD"] {
            assert!(!supported(code), "{code}");
        }
        for code in NATIVE_COLLATERAL {
            assert!(iso_currency::Currency::from_code(code).is_none());
        }
    }
}
