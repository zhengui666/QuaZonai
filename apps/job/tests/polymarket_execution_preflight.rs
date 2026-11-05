//! Pure format-parser/native-validator tests, not target, account or trade acceptance.
use contracts::execution_preflight::*;
use job::polymarket_execution_preflight::{parse_and_preflight, preflight, PolymarketPreflight};
use nautilus_model::{
    enums::{OrderSide, OrderStatus, TimeInForce},
    events::OrderDeniedReason,
    orders::{Order, OrderAny},
    types::{Price, Quantity},
};
use serde_json::{json, Value};

fn market_request() -> Value {
    json!({
        "schema_version": 1,
        "scope": "ORDER_FORMAT_ONLY",
        "trader_id": "PREFLIGHT-001",
        "strategy_id": "PREFLIGHT-001",
        "client_order_id": "PREFLIGHT-001",
        "instrument_id": "FORMAT-ONLY.POLYMARKET",
        "initialization_id": "ad2f699b-6342-496e-bc28-9032c3e4ef94",
        "initialized_at_ns": "1791331200123456789",
        "side": "BUY",
        "quantity": {"unit": "PUSD", "amount": "10.25"},
        "order": {"kind": "MARKET", "time_in_force": {"kind": "IOC"}, "reduce_only": false}
    })
}

fn limit_request() -> Value {
    let mut request = market_request();
    request["quantity"] = json!({"unit": "OUTCOME_SHARES", "amount": "10.25"});
    request["order"] = json!({
        "kind": "LIMIT", "time_in_force": {"kind": "GTC"},
        "price": "0.50", "post_only": false, "reduce_only": false
    });
    request
}

fn check(value: &Value) -> PolymarketPreflight {
    parse_and_preflight(&serde_json::to_vec(value).unwrap())
}

fn validated<'a>(result: &'a PolymarketPreflight, value: &Value) -> &'a OrderAny {
    let expected: PolymarketExecutionPreflightV1 = serde_json::from_value(value.clone()).unwrap();
    match &result.report.outcome {
        PolymarketExecutionPreflightOutcomeV1::FormatValidated { request } => {
            assert_eq!(**request, expected);
        }
        outcome => panic!("expected format validation, received {outcome:?}"),
    }
    result.native_order().unwrap()
}

fn rejected(result: &PolymarketPreflight, stage: ExecutionPreflightFailureStageV1) -> &str {
    assert!(result.native_order().is_none());
    match &result.report.outcome {
        PolymarketExecutionPreflightOutcomeV1::Rejected {
            stage: actual,
            reason,
        } => {
            assert_eq!(*actual, stage, "{reason}");
            reason
        }
        outcome => panic!("expected rejection, received {outcome:?}"),
    }
}

#[test]
fn accepted_format_preserves_explicit_native_identity_quantity_and_time() {
    let value = market_request();
    let request: PolymarketExecutionPreflightV1 = serde_json::from_value(value).unwrap();
    let original = serde_json::to_value(&request).unwrap();
    let result = preflight(&request);
    assert_eq!(serde_json::to_value(&request).unwrap(), original);
    assert_eq!(
        result.report.scope,
        ExecutionPreflightScopeV1::OrderFormatOnly
    );
    assert_eq!(result.report.native_version, "0.67.0");
    match &result.report.outcome {
        PolymarketExecutionPreflightOutcomeV1::FormatValidated { request: echoed } => {
            assert_eq!(**echoed, request);
        }
        other => panic!("expected format validation: {other:?}"),
    }
    let native = result.native_order().unwrap();
    assert_eq!(native.trader_id().to_string(), request.trader_id);
    assert_eq!(native.strategy_id().to_string(), request.strategy_id);
    assert_eq!(
        native.client_order_id().to_string(),
        request.client_order_id
    );
    assert_eq!(native.instrument_id().to_string(), request.instrument_id);
    assert_eq!(native.init_id().to_string(), request.initialization_id);
    assert_eq!(native.ts_init().as_u64(), request.initialized_at_ns.get());
    assert_eq!(native.quantity().as_decimal().to_string(), "10.25");
    assert_eq!(native.order_side(), OrderSide::Buy);
    assert_eq!(native.time_in_force(), TimeInForce::Ioc);
    assert_eq!(native.status(), OrderStatus::Initialized);
    assert!(native.is_quote_quantity());
    assert!(!native.is_reduce_only());
    assert_eq!(
        serde_json::to_value(native).unwrap(),
        serde_json::to_value(result.into_native_order().unwrap()).unwrap()
    );
}

#[test]
fn parser_rejects_missing_explicit_choices() {
    for key in [
        "scope",
        "trader_id",
        "strategy_id",
        "client_order_id",
        "instrument_id",
        "initialization_id",
        "initialized_at_ns",
        "side",
        "quantity",
        "order",
    ] {
        let mut request = market_request();
        request.as_object_mut().unwrap().remove(key);
        rejected(
            &check(&request),
            ExecutionPreflightFailureStageV1::RequestParsing,
        );
    }
    for key in ["time_in_force", "reduce_only"] {
        let mut request = market_request();
        request["order"].as_object_mut().unwrap().remove(key);
        rejected(
            &check(&request),
            ExecutionPreflightFailureStageV1::RequestParsing,
        );
    }
    let mut request = limit_request();
    request["order"]
        .as_object_mut()
        .unwrap()
        .remove("post_only");
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::RequestParsing,
    );
}

#[test]
fn parser_rejects_unknown_fields_and_caller_supplied_execution_authority() {
    for key in [
        "execution_ready",
        "release_id",
        "claim_id",
        "private_key",
        "account_balance",
    ] {
        let mut request = market_request();
        request[key] = json!("not-an-authority-input");
        rejected(
            &check(&request),
            ExecutionPreflightFailureStageV1::RequestParsing,
        );
    }
    let mut request = market_request();
    request["quantity"]["currency"] = json!("USD");
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::RequestParsing,
    );
    request = market_request();
    request["order"]["post_only"] = json!(false);
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::RequestParsing,
    );
}

#[test]
fn parser_rejects_duplicate_fields_instead_of_last_value_winning() {
    let text = serde_json::to_string(&market_request()).unwrap();
    for modified in [
        text.replacen("\"side\":\"BUY\"", "\"side\":\"BUY\",\"side\":\"SELL\"", 1),
        text.replacen(
            "\"amount\":\"10.25\"",
            "\"amount\":\"10.25\",\"amount\":\"20\"",
            1,
        ),
        text.replacen("\"kind\":\"IOC\"", "\"kind\":\"IOC\",\"kind\":\"GTC\"", 1),
    ] {
        assert_ne!(modified, text);
        rejected(
            &parse_and_preflight(modified.as_bytes()),
            ExecutionPreflightFailureStageV1::RequestParsing,
        );
    }
}

#[test]
fn parser_rejects_unknown_scope_unit_and_order_variants() {
    for (key, value) in [
        ("schema_version", json!(2)),
        ("scope", json!("LIVE_EXECUTION")),
        ("side", json!("SHORT")),
    ] {
        let mut request = market_request();
        request[key] = value;
        rejected(
            &check(&request),
            ExecutionPreflightFailureStageV1::RequestParsing,
        );
    }
    for unit in ["USD", "USDC.e", "BASE", "QUOTE", "shares"] {
        let mut request = market_request();
        request["quantity"]["unit"] = json!(unit);
        rejected(
            &check(&request),
            ExecutionPreflightFailureStageV1::RequestParsing,
        );
    }
    let mut request = market_request();
    request["order"]["kind"] = json!("STOP_MARKET");
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::RequestParsing,
    );
}

#[test]
fn parser_requires_exact_decimal_and_nanosecond_strings() {
    for amount in [json!(10.25), json!("1e1"), json!("NaN"), json!("Infinity")] {
        let mut request = market_request();
        request["quantity"]["amount"] = amount;
        rejected(
            &check(&request),
            ExecutionPreflightFailureStageV1::RequestParsing,
        );
    }
    let mut request = market_request();
    request["initialized_at_ns"] = json!(1791331200123456789_u64);
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::RequestParsing,
    );
}

#[test]
fn native_identity_errors_do_not_panic_or_substitute_identifiers() {
    for (key, value) in [
        ("trader_id", ""),
        ("strategy_id", ""),
        ("client_order_id", ""),
        ("instrument_id", "INVALID"),
        ("instrument_id", "ETHUSDT.BINANCE"),
        ("initialization_id", "invalid"),
    ] {
        let mut request = market_request();
        request[key] = json!(value);
        rejected(
            &check(&request),
            ExecutionPreflightFailureStageV1::NativeIdentity,
        );
    }
}

#[test]
fn native_precision_failure_is_not_rounded_or_clamped() {
    // More precision than either official fixed-point build mode supports.
    let mut request = market_request();
    request["quantity"]["amount"] = json!("0.12345678901234567");
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::NativeRepresentation,
    );
    request = limit_request();
    request["order"]["price"] = json!("0.12345678901234567");
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::NativeRepresentation,
    );
}

#[test]
fn zero_and_negative_amounts_are_not_replaced_by_a_default_size() {
    let mut request = market_request();
    request["quantity"]["amount"] = json!("0");
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::NativeConstruction,
    );
    request["quantity"]["amount"] = json!("-1");
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::NativeRepresentation,
    );
}

#[test]
fn official_market_denomination_failures_are_reported_without_conversion() {
    for (tif, native_tif) in [("IOC", TimeInForce::Ioc), ("FOK", TimeInForce::Fok)] {
        let mut request = market_request();
        request["order"]["time_in_force"]["kind"] = json!(tif);
        request["quantity"]["unit"] = json!("OUTCOME_SHARES");
        let result = check(&request);
        assert!(
            rejected(&result, ExecutionPreflightFailureStageV1::OfficialAdapter)
                .contains("quote_quantity=true")
        );
        request["side"] = json!("SELL");
        let result = check(&request);
        let native = validated(&result, &request);
        assert!(!native.is_quote_quantity());
        assert_eq!(native.quantity(), Quantity::from("10.25"));
        assert_eq!(native.order_side(), OrderSide::Sell);
        assert_eq!(native.time_in_force(), native_tif);
        request["quantity"]["unit"] = json!("PUSD");
        let result = check(&request);
        assert_eq!(
            rejected(&result, ExecutionPreflightFailureStageV1::OfficialAdapter),
            OrderDeniedReason::unsupported_quote_quantity().to_string()
        );
    }
}

#[test]
fn explicit_market_tif_is_preserved_or_rejected_at_its_real_layer() {
    let mut request = market_request();
    request["order"]["time_in_force"]["kind"] = json!("FOK");
    assert_eq!(
        check(&request).native_order().unwrap().time_in_force(),
        TimeInForce::Fok
    );
    request["order"]["time_in_force"]["kind"] = json!("GTC");
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::OfficialAdapter,
    );
    request["order"]["time_in_force"] =
        json!({"kind":"GTD", "expire_time_ns":"2000000000000000000"});
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::NativeConstruction,
    );
}

#[test]
fn reduce_only_is_rejected_instead_of_silently_removed() {
    for mut request in [market_request(), limit_request()] {
        request["order"]["reduce_only"] = json!(true);
        let result = check(&request);
        assert!(
            rejected(&result, ExecutionPreflightFailureStageV1::OfficialAdapter)
                .contains("Reduce-only")
        );
    }
}

#[test]
fn limit_sell_quote_amount_is_rejected_without_conversion() {
    for tif in ["GTC", "GTD", "IOC", "FOK"] {
        let mut request = limit_request();
        request["side"] = json!("SELL");
        request["quantity"]["unit"] = json!("PUSD");
        request["order"]["time_in_force"] = json!({"kind": tif});
        if tif == "GTD" {
            request["order"]["time_in_force"]["expire_time_ns"] = json!("2000000000123456789");
        }
        let result = check(&request);
        assert_eq!(
            rejected(&result, ExecutionPreflightFailureStageV1::OfficialAdapter),
            OrderDeniedReason::unsupported_quote_quantity().to_string()
        );
    }
}

#[test]
fn supported_limit_denominations_preserve_quantity_price_expiry_tif_and_flags() {
    // In 0.67 BUY accepts either shares or pUSD; SELL still requires shares.
    for (side, native_side, unit, quote_quantity) in [
        ("BUY", OrderSide::Buy, "PUSD", true),
        ("BUY", OrderSide::Buy, "OUTCOME_SHARES", false),
        ("SELL", OrderSide::Sell, "OUTCOME_SHARES", false),
    ] {
        for (tif, native_tif) in [
            ("GTC", TimeInForce::Gtc),
            ("GTD", TimeInForce::Gtd),
            ("IOC", TimeInForce::Ioc),
            ("FOK", TimeInForce::Fok),
        ] {
            for post_only in [false, true] {
                if post_only && matches!(native_tif, TimeInForce::Ioc | TimeInForce::Fok) {
                    continue;
                }
                let mut request = limit_request();
                request["side"] = json!(side);
                request["quantity"]["unit"] = json!(unit);
                request["order"]["price"] = json!("0.53");
                request["order"]["post_only"] = json!(post_only);
                request["order"]["time_in_force"] = json!({"kind": tif});
                let expiry =
                    (native_tif == TimeInForce::Gtd).then_some(2_000_000_000_123_456_789_u64);
                if let Some(expiry) = expiry {
                    request["order"]["time_in_force"]["expire_time_ns"] = json!(expiry.to_string());
                }
                let result = check(&request);
                let native = validated(&result, &request);
                assert_eq!(native.order_side(), native_side);
                assert_eq!(native.quantity(), Quantity::from("10.25"));
                assert_eq!(native.is_quote_quantity(), quote_quantity);
                assert_eq!(native.price(), Some(Price::from("0.53")));
                assert_eq!(native.expire_time().map(|time| time.as_u64()), expiry);
                assert_eq!(native.time_in_force(), native_tif);
                assert_eq!(native.is_post_only(), post_only);
                assert!(!native.is_reduce_only());
                assert_eq!(native.status(), OrderStatus::Initialized);
            }
        }
    }
    // No current clock, market tick or GTD safety-window acceptance is claimed.
}

#[test]
fn post_only_immediate_limit_reports_official_failure() {
    for (side, unit) in [
        ("BUY", "PUSD"),
        ("BUY", "OUTCOME_SHARES"),
        ("SELL", "OUTCOME_SHARES"),
    ] {
        for tif in ["IOC", "FOK"] {
            let mut request = limit_request();
            request["side"] = json!(side);
            request["quantity"]["unit"] = json!(unit);
            request["order"]["post_only"] = json!(true);
            request["order"]["time_in_force"]["kind"] = json!(tif);
            let result = check(&request);
            assert_eq!(
                rejected(&result, ExecutionPreflightFailureStageV1::OfficialAdapter),
                OrderDeniedReason::unsupported_post_only().to_string()
            );
        }
    }
}

#[test]
fn official_limit_signing_minimum_failures_are_reported_without_rounding() {
    for (side, unit) in [
        ("BUY", "PUSD"),
        ("BUY", "OUTCOME_SHARES"),
        ("SELL", "OUTCOME_SHARES"),
    ] {
        let mut request = limit_request();
        request["side"] = json!(side);
        request["quantity"] = json!({"unit": unit, "amount": "0.009"});
        let result = check(&request);
        assert!(
            rejected(&result, ExecutionPreflightFailureStageV1::OfficialAdapter)
                .contains("truncates to zero at 2 decimal places")
        );
    }
    for tif in ["IOC", "FOK"] {
        let mut request = limit_request();
        request["quantity"]["amount"] = json!("0.01");
        request["order"]["price"] = json!("0.01");
        request["order"]["time_in_force"]["kind"] = json!(tif);
        let result = check(&request);
        let reason = rejected(&result, ExecutionPreflightFailureStageV1::OfficialAdapter);
        assert!(reason.contains("BUY maker amount 0.0001 pUSD"), "{reason}");
        assert!(reason.contains("truncates to zero at 2 decimal places"));
    }
}

#[test]
fn gtd_missing_or_extra_expiry_is_a_parsing_error() {
    let mut request = limit_request();
    request["order"]["time_in_force"] = json!({"kind":"GTD"});
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::RequestParsing,
    );
    request["order"]["time_in_force"] =
        json!({"kind":"GTC", "expire_time_ns":"2000000000000000000"});
    rejected(
        &check(&request),
        ExecutionPreflightFailureStageV1::RequestParsing,
    );
}

#[test]
fn repeated_preflight_is_pure_and_does_not_create_a_claim_or_fresh_identity() {
    let request = market_request();
    let first = check(&request);
    let second = check(&request);
    assert_eq!(first.report, second.report);
    assert_eq!(
        serde_json::to_value(first.native_order().unwrap()).unwrap(),
        serde_json::to_value(second.native_order().unwrap()).unwrap()
    );
    let report = serde_json::to_value(first.report).unwrap();
    assert!(report.get("execution_ready").is_none());
    assert!(report.get("handoff_id").is_none());
    assert!(report.get("acknowledged_at").is_none());
}

#[test]
fn generated_schema_contains_the_exact_native_contract_export() {
    let recorded: Value = serde_json::from_slice(include_bytes!(
        "../../../contracts/generated/domain-v1.openapi.json"
    ))
    .unwrap();
    let actual: Value = serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    assert_eq!(
        recorded, actual,
        "regenerate the domain schema through its native generator"
    );
    let schemas = &actual["components"]["schemas"];
    for name in [
        "NativeCatalogFactorScanRequestV1",
        "NativeCatalogFactorScanSummaryV1",
        "NativeCatalogFactorCoverageV1",
        "ExecutionPreflightScopeV1",
        "PolymarketPreflightSideV1",
        "PolymarketPreflightQuantityV1",
        "PolymarketPreflightTimeInForceV1",
        "PolymarketPreflightOrderV1",
        "PolymarketExecutionPreflightV1",
        "ExecutionPreflightFailureStageV1",
        "PolymarketExecutionPreflightOutcomeV1",
        "PolymarketExecutionPreflightReportV1",
    ] {
        assert!(schemas.get(name).is_some(), "missing schema {name}");
    }
    assert_eq!(
        schemas["PolymarketExecutionPreflightV1"]["additionalProperties"],
        false
    );
}

fn assert_closed_variants<T: serde::de::DeserializeOwned + serde::Serialize>(
    schema: &Value,
    tag: &str,
    samples: &[Value],
) {
    use std::collections::BTreeSet;
    let variants = schema["oneOf"].as_array().unwrap();
    assert_eq!(variants.len(), samples.len());
    for sample in samples {
        let typed: T = serde_json::from_value(sample.clone()).unwrap();
        let encoded = serde_json::to_value(typed).unwrap();
        let variant = variants
            .iter()
            .find(|variant| variant["properties"][tag]["enum"][0] == encoded[tag])
            .expect("serialized discriminator has a schema variant");
        assert_eq!(variant["type"], "object");
        assert_eq!(variant["additionalProperties"], false);
        let declared: BTreeSet<_> = variant["properties"]
            .as_object()
            .unwrap()
            .keys()
            .cloned()
            .collect();
        let present: BTreeSet<_> = encoded.as_object().unwrap().keys().cloned().collect();
        let required: BTreeSet<_> = variant["required"]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(
            declared, present,
            "schema fields must match the serialized variant"
        );
        assert_eq!(required, present, "every declared choice is explicit");
        let mut extra = encoded.clone();
        extra["unexpected_field"] = json!(true);
        assert!(
            serde_json::from_value::<T>(extra).is_err(),
            "closed schema and Serde must both reject extra variant fields"
        );
    }
}

#[test]
fn all_tagged_union_schema_variants_match_strict_serde_fields() {
    let export: Value = serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    let schemas = &export["components"]["schemas"];
    assert_closed_variants::<contracts::ticks::NativeTickTargetV1>(
        &schemas["NativeTickTargetV1"],
        "kind",
        &[
            json!({"kind":"FIXED_TRADE_COHORTS", "horizon_cohorts":1}),
            json!({"kind":"FIXED_EVENT_TIME", "horizon_ns":"10000000000"}),
        ],
    );
    assert_closed_variants::<PolymarketPreflightQuantityV1>(
        &schemas["PolymarketPreflightQuantityV1"],
        "unit",
        &[
            json!({"unit":"PUSD", "amount":"10.25"}),
            json!({"unit":"OUTCOME_SHARES", "amount":"10.25"}),
        ],
    );
    assert_closed_variants::<PolymarketPreflightTimeInForceV1>(
        &schemas["PolymarketPreflightTimeInForceV1"],
        "kind",
        &[
            json!({"kind":"GTC"}),
            json!({"kind":"GTD", "expire_time_ns":"2000000000000000000"}),
            json!({"kind":"IOC"}),
            json!({"kind":"FOK"}),
        ],
    );
    assert_closed_variants::<PolymarketPreflightOrderV1>(
        &schemas["PolymarketPreflightOrderV1"],
        "kind",
        &[
            market_request()["order"].clone(),
            limit_request()["order"].clone(),
        ],
    );
    assert_closed_variants::<PolymarketExecutionPreflightOutcomeV1>(
        &schemas["PolymarketExecutionPreflightOutcomeV1"],
        "status",
        &[
            json!({"status":"FORMAT_VALIDATED", "request": market_request()}),
            json!({"status":"REJECTED", "stage":"OFFICIAL_ADAPTER", "reason":"format rejected"}),
        ],
    );
}
