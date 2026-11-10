//! The native domain and generated browser schema consume the same registry-key corpus.
#[test]
fn registry_key_grammar_matches_the_shared_unicode_and_path_corpus() {
    let cases: Vec<serde_json::Value> = serde_json::from_str(r###"[{"value":"catalog","valid":true},{"value":"registered/catalog-v1","valid":true},{"value":"目录/美元","valid":true},{"value":"archive/🧪","valid":true},{"value":"a b/c d","valid":true},{"value":"namespace:catalog","valid":true},{"value":".../a..b","valid":true},{"value":"a/%2F/%2E%2E","valid":true},{"value":"a b","valid":true},{"value":"﻿catalog﻿","valid":true},{"value":"","valid":false},{"value":" ","valid":false},{"value":" catalog","valid":false},{"value":"catalog ","valid":false},{"value":" catalog","valid":false},{"value":"catalog　","valid":false},{"value":" catalog","valid":false},{"value":"catalog ","valid":false},{"value":"catalog\n","valid":false},{"value":"a\tb","valid":false},{"value":"a\u0000b","valid":false},{"value":"ab","valid":false},{"value":"ab","valid":false},{"value":"ab","valid":false},{"value":".","valid":false},{"value":"..","valid":false},{"value":"./catalog","valid":false},{"value":"../catalog","valid":false},{"value":"a/./b","valid":false},{"value":"a/../b","valid":false},{"value":"a/.","valid":false},{"value":"a/..","valid":false},{"value":"/catalog","valid":false},{"value":"catalog/","valid":false},{"value":"a//b","valid":false},{"value":"https://host/catalog","valid":false},{"value":"a\\b","valid":false},{"value":"a?version=1","valid":false},{"value":"a#fragment","valid":false}]"###)
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
