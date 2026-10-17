use contracts::portfolio::*;
use utoipa::PartialSchema;

#[test]
fn model_schema_preserves_exact_native_identity_and_closed_parameters() {
    let schema = serde_json::to_value(NativeModelRefV1::schema()).unwrap();
    let variants = schema["oneOf"].as_array().unwrap();
    let expected = [
        (
            "NAUTILUS_DEFAULT_FILL",
            NAUTILUS_FILL_CLASS,
            NAUTILUS_EXECUTION_VERSION,
        ),
        (
            "NAUTILUS_MAKER_TAKER",
            NAUTILUS_FEE_CLASS,
            NAUTILUS_EXECUTION_VERSION,
        ),
        (
            "NAUTILUS_POLYMARKET",
            NAUTILUS_POLYMARKET_FEE_CLASS,
            NAUTILUS_EXECUTION_VERSION,
        ),
        (
            "NAUTILUS_STATIC_LATENCY",
            NAUTILUS_LATENCY_CLASS,
            NAUTILUS_EXECUTION_VERSION,
        ),
        ("CLARABEL_QP", CLARABEL_CLASS, CLARABEL_VERSION),
        (
            "FIXED_WEIGHTED_FORECAST",
            FIXED_ENSEMBLE_CLASS,
            FIXED_ENSEMBLE_VERSION,
        ),
        (
            "SAMPLE_COVARIANCE",
            SAMPLE_COVARIANCE_CLASS,
            SAMPLE_COVARIANCE_VERSION,
        ),
    ];
    // QZ-owned adapters do not impersonate upstream classes or weaken the
    // seven original native adapters.
    assert_eq!(variants.len(), expected.len() + 3);
    for (kind, class, version) in expected {
        let matches = variants
            .iter()
            .filter(|variant| {
                variant["properties"]["adapter_kind"]["enum"] == serde_json::json!([kind])
            })
            .collect::<Vec<_>>();
        assert_eq!(matches.len(), 1, "{kind}");
        let variant = matches[0];
        assert_eq!(variant["additionalProperties"], false);
        assert_eq!(
            variant["properties"]["upstream_class"]["enum"],
            serde_json::json!([class])
        );
        assert_eq!(
            variant["properties"]["upstream_version"]["enum"],
            serde_json::json!([version])
        );
        assert_eq!(
            variant["properties"]["parameters"]["additionalProperties"],
            false
        );
        assert_eq!(variant["required"].as_array().unwrap().len(), 5);
    }

    let matches = variants
        .iter()
        .filter(|variant| {
            variant["properties"]["adapter_kind"]["enum"]
                == serde_json::json!(["FROZEN_SPOT_FEE_SCENARIO"])
        })
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1, "FROZEN_SPOT_FEE_SCENARIO");
    let variant = matches[0];
    assert_eq!(variant["additionalProperties"], false);
    let fields = [
        "adapter_kind",
        "native_version",
        "parameters",
        "schema_version",
    ];
    assert_eq!(
        variant["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        fields.into_iter().collect()
    );
    assert_eq!(
        variant["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        fields.into_iter().collect()
    );
    assert_eq!(variant["required"].as_array().unwrap().len(), 4);
    assert_eq!(
        variant["properties"]["schema_version"],
        serde_json::to_value(contracts::SchemaV1::schema()).unwrap()
    );
    assert_eq!(
        variant["properties"]["native_version"]["enum"],
        serde_json::json!([NAUTILUS_EXECUTION_VERSION])
    );
    let parameters = &variant["properties"]["parameters"];
    assert_eq!(parameters["additionalProperties"], false);
    assert_eq!(
        parameters,
        &serde_json::to_value(contracts::spot_fees::FrozenSpotFeeScenarioParametersV1::schema())
            .unwrap()
    );
    assert_eq!(
        parameters["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        ["acceptance", "schedule"].into_iter().collect()
    );
    assert_eq!(
        parameters["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        ["acceptance", "schedule"].into_iter().collect()
    );
    assert_eq!(parameters["required"].as_array().unwrap().len(), 2);
}

#[test]
fn polymarket_fee_wire_does_not_accept_extra_parameters_or_unknown_fields() {
    let original = serde_json::json!({
        "schema_version": 1,
        "adapter_kind": "NAUTILUS_POLYMARKET",
        "upstream_class": NAUTILUS_POLYMARKET_FEE_CLASS,
        "upstream_version": NAUTILUS_EXECUTION_VERSION,
        "parameters": {},
    });
    assert!(matches!(
        serde_json::from_value::<NativeModelRefV1>(original.clone()).unwrap(),
        NativeModelRefV1::NautilusPolymarket { .. }
    ));
    let mut changed = original.clone();
    changed["parameters"]["fee_rate"] = serde_json::json!(0);
    assert!(serde_json::from_value::<NativeModelRefV1>(changed).is_err());
    let mut changed = original;
    changed["wallet"] = serde_json::json!("not-a-supported-field");
    assert!(serde_json::from_value::<NativeModelRefV1>(changed).is_err());
}

#[test]
fn frozen_spot_fee_scenario_wire_is_closed_and_does_not_claim_an_upstream_class() {
    // Controlled wire fixture only; no execution or real fee applicability claim.
    let rules =
        ["BUY", "SELL"]
            .into_iter()
            .flat_map(|side| {
                ["MAKER", "TAKER"].into_iter().map(move |liquidity| serde_json::json!({
            "instrument_id": "BTC-USDC.TEST", "order_side": side, "liquidity": liquidity,
            "rate": "0.001", "basis": "QUOTE_NOTIONAL", "fee_currency": "QUOTE",
            "currency_code": "USDC", "currency_precision": 6,
            "rounding": "HALF_EVEN_CURRENCY_PRECISION"
        }))
            })
            .collect::<Vec<_>>();
    let original = serde_json::json!({
        "schema_version": 1,
        "adapter_kind": "FROZEN_SPOT_FEE_SCENARIO",
        "native_version": NAUTILUS_EXECUTION_VERSION,
        "parameters": {
            "acceptance": "PUBLIC_RATE_SCENARIO_UNVERIFIED_APPLICABILITY",
            "schedule": {
                "schema_version": 1,
                "source": {
                    "status": "PUBLIC_RATE_SCENARIO_UNVERIFIED_APPLICABILITY",
                    "source_ref": "controlled:wire-test", "source_uri": "https://example.org/controlled",
                    "observed_at_ns": "1", "assumptions": "Controlled wire fixture only"
                },
                "valid_from_ns": "1", "valid_until_ns": "2", "rules": rules
            }
        }
    });
    let parsed = serde_json::from_value::<NativeModelRefV1>(original.clone()).unwrap();
    assert!(
        matches!(&parsed, NativeModelRefV1::FrozenSpotFeeScenario { native_version, .. }
        if native_version == NAUTILUS_EXECUTION_VERSION)
    );
    assert_eq!(serde_json::to_value(parsed).unwrap(), original);
    for field in ["upstream_class", "upstream_version", "wallet"] {
        let mut changed = original.clone();
        changed[field] = serde_json::json!("not-a-supported-field");
        assert!(
            serde_json::from_value::<NativeModelRefV1>(changed).is_err(),
            "{field}"
        );
    }
    for path in [
        "/parameters",
        "/parameters/schedule",
        "/parameters/schedule/source",
        "/parameters/schedule/rules/0",
    ] {
        let mut changed = original.clone();
        changed.pointer_mut(path).unwrap()["unknown"] = serde_json::json!(true);
        assert!(
            serde_json::from_value::<NativeModelRefV1>(changed).is_err(),
            "{path}"
        );
    }
    for field in [
        "schema_version",
        "adapter_kind",
        "native_version",
        "parameters",
    ] {
        let mut changed = original.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<NativeModelRefV1>(changed).is_err(),
            "{field}"
        );
    }
    for field in ["acceptance", "schedule"] {
        let mut changed = original.clone();
        changed["parameters"].as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<NativeModelRefV1>(changed).is_err(),
            "{field}"
        );
    }
}

#[test]
fn recorded_v2_fee_wire_has_no_fitted_rate_or_upstream_identity() {
    let original = serde_json::json!({
        "schema_version":1,"adapter_kind":"POLYMARKET_V2_RECORDED_FEES",
        "native_version":NAUTILUS_EXECUTION_VERSION,"parameters":{}
    });
    let parsed = serde_json::from_value::<NativeModelRefV1>(original.clone()).unwrap();
    assert!(matches!(
        &parsed,
        NativeModelRefV1::PolymarketV2RecordedFees { .. }
    ));
    assert_eq!(serde_json::to_value(parsed).unwrap(), original);
    let schema = serde_json::to_value(NativeModelRefV1::schema()).unwrap();
    let variant = schema["oneOf"]
        .as_array()
        .unwrap()
        .iter()
        .find(|variant| {
            variant["properties"]["adapter_kind"]["enum"]
                == serde_json::json!(["POLYMARKET_V2_RECORDED_FEES"])
        })
        .unwrap();
    assert_eq!(variant["additionalProperties"], false);
    assert_eq!(variant["required"].as_array().unwrap().len(), 4);
    assert_eq!(
        variant["properties"]["native_version"]["enum"],
        serde_json::json!([NAUTILUS_EXECUTION_VERSION])
    );
    assert_eq!(
        variant["properties"]["parameters"]["additionalProperties"],
        false
    );
    assert!(variant["properties"].get("upstream_class").is_none());
    for field in ["upstream_class", "upstream_version", "wallet"] {
        let mut changed = original.clone();
        changed[field] = serde_json::json!("unsupported");
        assert!(serde_json::from_value::<NativeModelRefV1>(changed).is_err());
    }
    let mut changed = original;
    changed["parameters"]["fee_rate"] = serde_json::json!("0.001");
    assert!(serde_json::from_value::<NativeModelRefV1>(changed).is_err());
}

#[test]
fn recorded_v2_liquidity_schema_has_native_identity_and_existing_closed_fill_parameters() {
    let schema = serde_json::to_value(NativeModelRefV1::schema()).unwrap();
    let variants = schema["oneOf"].as_array().unwrap();
    assert_eq!(variants.len(), 10);
    let matches = variants
        .iter()
        .filter(|variant| {
            variant["properties"]["adapter_kind"]["enum"]
                == serde_json::json!(["POLYMARKET_V2_RECORDED_LIQUIDITY"])
        })
        .collect::<Vec<_>>();
    assert_eq!(matches.len(), 1);
    let variant = matches[0];
    assert_eq!(variant["additionalProperties"], false);
    let fields = [
        "adapter_kind",
        "native_version",
        "parameters",
        "schema_version",
    ];
    assert_eq!(
        variant["properties"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<std::collections::BTreeSet<_>>(),
        fields.into_iter().collect()
    );
    assert_eq!(
        variant["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect::<std::collections::BTreeSet<_>>(),
        fields.into_iter().collect()
    );
    assert_eq!(
        variant["properties"]["schema_version"],
        serde_json::to_value(contracts::SchemaV1::schema()).unwrap()
    );
    assert_eq!(
        variant["properties"]["native_version"]["enum"],
        serde_json::json!(["0.63.0"])
    );
    assert_eq!(
        variant["properties"]["parameters"],
        serde_json::to_value(NautilusFillParametersV1::schema()).unwrap()
    );
    assert_eq!(
        variant["properties"]["parameters"]["additionalProperties"],
        false
    );
}

#[test]
fn recorded_v2_liquidity_wire_is_closed_and_preserves_default_fill_seed_contract() {
    let original = serde_json::json!({
        "schema_version": 1,
        "adapter_kind": "POLYMARKET_V2_RECORDED_LIQUIDITY",
        "native_version": NAUTILUS_EXECUTION_VERSION,
        "parameters": {"prob_fill_on_limit":"0.75", "prob_slippage":"1", "random_seed":"7"}
    });
    let parsed = serde_json::from_value::<NativeModelRefV1>(original.clone()).unwrap();
    assert!(matches!(
        &parsed,
        NativeModelRefV1::PolymarketV2RecordedLiquidity { native_version, .. }
            if native_version == "0.63.0"
    ));
    assert_eq!(serde_json::to_value(parsed).unwrap(), original);
    for field in ["upstream_class", "upstream_version", "wallet"] {
        let mut changed = original.clone();
        changed[field] = serde_json::json!("unsupported");
        assert!(
            serde_json::from_value::<NativeModelRefV1>(changed).is_err(),
            "{field}"
        );
    }
    for field in ["fee_rate", "quantity", "unknown"] {
        let mut changed = original.clone();
        changed["parameters"][field] = serde_json::json!("1");
        assert!(
            serde_json::from_value::<NativeModelRefV1>(changed).is_err(),
            "{field}"
        );
    }
    for (path, fields) in [
        (
            "",
            [
                "schema_version",
                "adapter_kind",
                "native_version",
                "parameters",
            ]
            .as_slice(),
        ),
        (
            "/parameters",
            ["prob_fill_on_limit", "prob_slippage", "random_seed"].as_slice(),
        ),
    ] {
        for field in fields {
            let mut changed = original.clone();
            changed
                .pointer_mut(path)
                .unwrap()
                .as_object_mut()
                .unwrap()
                .remove(*field);
            assert!(
                serde_json::from_value::<NativeModelRefV1>(changed).is_err(),
                "{path}/{field}"
            );
        }
    }
    let mut legacy = original.clone();
    legacy.as_object_mut().unwrap().remove("native_version");
    legacy["adapter_kind"] = serde_json::json!("NAUTILUS_DEFAULT_FILL");
    legacy["upstream_class"] = serde_json::json!(NAUTILUS_FILL_CLASS);
    legacy["upstream_version"] = serde_json::json!(NAUTILUS_EXECUTION_VERSION);
    for mut model in [original, legacy] {
        for seed in ["0", "7", "9223372036854775807"] {
            model["parameters"]["random_seed"] = serde_json::json!(seed);
            let parsed = serde_json::from_value::<NativeModelRefV1>(model.clone()).unwrap();
            assert_eq!(serde_json::to_value(parsed).unwrap(), model);
        }
        for seed in [
            serde_json::json!(7),
            serde_json::json!("-1"),
            serde_json::json!("1.5"),
            serde_json::json!("9223372036854775808"),
            serde_json::json!(null),
        ] {
            model["parameters"]["random_seed"] = seed;
            assert!(serde_json::from_value::<NativeModelRefV1>(model.clone()).is_err());
        }
    }
}
