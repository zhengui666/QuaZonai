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
        .item(currency_schema())
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

fn currency_schema() -> RefOr<Schema> {
    static CODES: std::sync::LazyLock<Vec<String>> = std::sync::LazyLock::new(|| {
        let mut codes = Vec::new();
        for a in b'A'..=b'Z' {
            for b in b'A'..=b'Z' {
                for c in b'A'..=b'Z' {
                    let bytes = [a, b, c];
                    let code = std::str::from_utf8(&bytes).expect("ASCII currency code");
                    if iso_currency::Currency::from_code(code).is_some() {
                        codes.push(code.to_owned());
                    }
                }
            }
        }
        codes
    });
    ObjectBuilder::new()
        .schema_type(Type::String)
        .min_length(Some(3))
        .max_length(Some(3))
        .enum_values(Some(CODES.iter().cloned()))
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
