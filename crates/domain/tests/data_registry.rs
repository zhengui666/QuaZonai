//! The native domain and generated browser schema consume the same registry-key corpus.
#[test]
fn registry_key_grammar_matches_the_shared_unicode_and_path_corpus() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(include_str!(
        "../../../tests/contracts/data-registry-keys.json"
    ))
    .unwrap();
    for case in cases {
        let value = case["value"].as_str().unwrap();
        let valid = case["valid"].as_bool().unwrap();
        assert_eq!(
            domain::data::registry_key(value).is_ok(),
            valid,
            "{value:?}"
        );
    }
    for unit in ["a", "数", "🧪"] {
        assert!(domain::data::registry_key(&unit.repeat(512)).is_ok());
        assert!(domain::data::registry_key(&unit.repeat(513)).is_err());
    }
}
