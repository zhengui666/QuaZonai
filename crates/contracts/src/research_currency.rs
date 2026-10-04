//! Research units are not model-billing currencies and are never aliases for USD.
//! These spellings are the pinned Nautilus native currencies. The original instrument
//! and source evidence still determine the collateral contract and historical regime.
use utoipa::openapi::{
    schema::{ObjectBuilder, OneOfBuilder, Type},
    RefOr, Schema,
};

pub const NATIVE_COLLATERAL: [&str; 3] = ["USDC", "USDC.e", "pUSD"];
// Native market quote units are separate from supported prediction collateral.
// Matching instrument definitions and Nautilus retain their exact currency; no FX alias.
pub const NATIVE_QUOTE: [&str; 1] = ["USDT"];

pub fn supported(value: &str) -> bool {
    iso_currency::Currency::from_code(value).is_some()
        || NATIVE_COLLATERAL.contains(&value)
        || NATIVE_QUOTE.contains(&value)
}

pub(crate) fn schema() -> RefOr<Schema> {
    OneOfBuilder::new()
        .item(crate::budget::currency_schema())
        .item(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .enum_values(Some(NATIVE_COLLATERAL)),
        )
        .item(
            ObjectBuilder::new()
                .schema_type(Type::String)
                .enum_values(Some(NATIVE_QUOTE)),
        )
        .into()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn collateral_is_explicit_case_sensitive_and_not_a_fiat_alias() {
        for code in ["USD", "EUR", "CNY", "USDC", "USDC.e", "pUSD", "USDT"] {
            assert!(supported(code), "{code}");
        }
        for code in ["", "usd", "PUSD", "usdc", "usdt", "BTC", "USD ", " USD"] {
            assert!(!supported(code), "{code}");
        }
        for code in NATIVE_COLLATERAL {
            assert!(iso_currency::Currency::from_code(code).is_none());
        }
        assert!(!NATIVE_COLLATERAL.contains(&"USDT"));
        assert!(iso_currency::Currency::from_code("USDT").is_none());
    }
}
