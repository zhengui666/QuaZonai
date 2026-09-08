use contracts::budget::BudgetV1;
use serde_json::Value;
use std::collections::BTreeSet;
use utoipa::OpenApi;

#[derive(OpenApi)]
#[openapi(components(schemas(BudgetV1)))]
struct BudgetDocument;

#[test]
fn cost_currency_schema_matches_the_pinned_native_lookup_exhaustively() {
    let document: Value = serde_json::to_value(BudgetDocument::openapi()).unwrap();
    let schema = &document["components"]["schemas"]["BudgetV1"]["properties"]["cost_currency"];
    let alternatives = schema["oneOf"].as_array().unwrap();
    assert_eq!(alternatives.len(), 2);
    assert!(alternatives.iter().any(|value| value["type"] == "null"));
    let strings = alternatives
        .iter()
        .find(|value| value["type"] == "string")
        .unwrap();
    assert_eq!(strings["minLength"], 3);
    assert_eq!(strings["maxLength"], 3);
    let entries = strings["enum"].as_array().unwrap();
    let codes: BTreeSet<&str> = entries
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect();
    assert_eq!(entries.len(), codes.len());
    assert!(codes.contains("USD"));
    assert!(!codes.contains("ZZZ"));
    for a in b'A'..=b'Z' {
        for b in b'A'..=b'Z' {
            for c in b'A'..=b'Z' {
                let bytes = [a, b, c];
                let code = std::str::from_utf8(&bytes).unwrap();
                assert_eq!(
                    codes.contains(code),
                    iso_currency::Currency::from_code(code).is_some(),
                    "{code}"
                );
            }
        }
    }
}
