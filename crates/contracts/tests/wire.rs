use contracts::{evidence::MetricValueV1, DbCounter, DecimalValue, Id, Revision, SchemaV1};
use serde_json::{from_value, json, to_value};

#[test]
fn precise_database_values_never_cross_json_as_numbers() {
    let boundary = "9007199254740993";
    let counter: DbCounter = from_value(json!(boundary)).unwrap();
    assert_eq!(to_value(counter).unwrap(), json!(boundary));
    let revision: Revision = from_value(json!(i64::MAX.to_string())).unwrap();
    assert!(revision.next().is_none());
    assert!(DbCounter::new(i64::MAX as u64)
        .unwrap()
        .checked_add(1)
        .is_none());
    for invalid in [
        json!(1),
        json!(null),
        json!(true),
        json!("-1"),
        json!("01"),
        json!("+1"),
        json!(" 1"),
        json!("9223372036854775808"),
    ] {
        assert!(
            from_value::<DbCounter>(invalid.clone()).is_err(),
            "{invalid}"
        );
        assert!(
            from_value::<Revision>(invalid.clone()).is_err(),
            "{invalid}"
        );
    }
    assert!(from_value::<Revision>(json!("0")).is_err());
    assert_eq!(
        from_value::<DbCounter>(json!("0")).unwrap(),
        DbCounter::ZERO
    );
}

#[test]
fn native_arbitrary_precision_preserves_the_complete_numeric_38_18_range() {
    for text in [
        "99999999999999999999.999999999999999999",
        "-99999999999999999999.999999999999999999",
        "0.000000000000000001",
        "0",
    ] {
        let value: DecimalValue = from_value(json!(text)).unwrap();
        assert_eq!(to_value(&value).unwrap(), json!(text));
        assert_eq!(
            from_value::<DecimalValue>(to_value(&value).unwrap()).unwrap(),
            value
        );
    }
    assert_eq!(
        to_value("0.2500".parse::<DecimalValue>().unwrap()).unwrap(),
        json!("0.25")
    );
    for text in [
        "100000000000000000000",
        "0.0000000000000000001",
        "1e3",
        "1_000",
        "NaN",
        "Infinity",
        "",
        " 1",
        "1 ",
    ] {
        assert!(from_value::<DecimalValue>(json!(text)).is_err(), "{text}");
    }
    assert!(from_value::<DecimalValue>(json!(0.25)).is_err());
}

#[test]
fn schema_versions_and_local_identity_are_explicit() {
    assert_eq!(to_value(SchemaV1).unwrap(), json!(1));
    for bad in [json!(0), json!(2), json!("1"), json!(1.0), json!(null)] {
        assert!(from_value::<SchemaV1>(bad).is_err());
    }
    let id = Id::new();
    assert_eq!(id.as_uuid().get_version_num(), 7);
    assert_eq!(from_value::<Id>(to_value(id).unwrap()).unwrap(), id);
    for bad in [
        json!("00000000-0000-4000-8000-000000000000"),
        json!("00000000-0000-7000-0000-000000000000"),
        json!("external-job/1"),
        json!(null),
    ] {
        assert!(from_value::<Id>(bad).is_err());
    }
}

#[test]
fn nonfinite_native_metrics_cannot_be_serialized_into_innocent_nulls() {
    let value = json!({"schema_version":1,"evaluation_id":Id::new(),"metric_code":"risk","scope":"total",
        "value":0.1,"status":"OK","reason_code":null,"unit":"fraction","period_start":"2026-01-01T00:00:00Z",
        "period_end":"2026-02-01T00:00:00Z","observation_count":"30","frequency":"daily",
        "annualization_factor":252.0,"method_id":"native-risk","method_version":"1","source_artifact_id":Id::new(),"higher_is_better":false});
    let original: MetricValueV1 = from_value(value).unwrap();
    for invalid in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        let mut metric = original.clone();
        metric.value = Some(invalid);
        assert!(to_value(&metric).is_err());
        metric.value = None;
        metric.annualization_factor = Some(invalid);
        assert!(to_value(&metric).is_err());
    }
}

#[test]
fn generated_schema_describes_initial_slice_not_nonexistent_http_routes() {
    let first = contracts::openapi_json().unwrap();
    assert_eq!(first, contracts::openapi_json().unwrap());
    let schema: serde_json::Value = serde_json::from_str(&first).unwrap();
    assert!(schema["paths"].as_object().unwrap().is_empty());
    for name in [
        "NativePortfolioStudyRequestV1",
        "NativePortfolioStudyFrameV1",
        "NativePortfolioStudyResultV1",
    ] {
        assert!(schema["components"]["schemas"][name].is_object(), "{name}");
    }
    assert_eq!(
        schema["components"]["schemas"]["Revision"]["type"],
        "string"
    );
    assert_eq!(
        schema["components"]["schemas"]["DecimalValue"]["type"],
        "string"
    );
    assert_eq!(schema["components"]["schemas"]["SchemaV1"]["minimum"], 1);
}

#[test]
fn native_scientific_export_is_compact_and_deterministic() {
    let generated = contracts::openapi_json().unwrap();
    let document: serde_json::Value = serde_json::from_str(&generated).unwrap();
    assert_eq!(generated, serde_json::to_string(&document).unwrap() + "\n");
    assert_eq!(generated, contracts::openapi_json().unwrap());
}

#[test]
fn metric_nullable_fields_are_present_not_silently_defaulted() {
    let value = json!({"schema_version":1,"evaluation_id":Id::new(),"metric_code":"risk","scope":"total",
        "value":null,"status":"INSUFFICIENT_DATA","reason_code":"NO_SAMPLES","unit":"fraction","period_start":"2026-01-01T00:00:00Z",
        "period_end":"2026-02-01T00:00:00Z","observation_count":"0","frequency":"daily",
        "annualization_factor":null,"method_id":"native-risk","method_version":"1","source_artifact_id":Id::new(),"higher_is_better":false});
    assert!(from_value::<MetricValueV1>(value.clone()).is_ok());
    for key in ["value", "annualization_factor"] {
        let mut omitted = value.clone();
        omitted.as_object_mut().unwrap().remove(key);
        assert!(from_value::<MetricValueV1>(omitted).is_err(), "{key}");
    }
    let schema: serde_json::Value =
        serde_json::from_str(&contracts::openapi_json().unwrap()).unwrap();
    let required = schema["components"]["schemas"]["MetricValueV1"]["required"]
        .as_array()
        .unwrap();
    assert!(
        required.contains(&json!("value")) && required.contains(&json!("annualization_factor"))
    );
    assert!(schema["components"]["schemas"]["Id"]["pattern"]
        .as_str()
        .unwrap()
        .contains("-7"));
    assert!(from_value::<Id>(json!(Id::new().as_uuid().simple().to_string())).is_err());
}

#[test]
fn native_decimal_parser_matches_the_shared_wire_boundary_corpus() {
    let cases: Vec<serde_json::Value> =
        serde_json::from_str(r###"[{"input":"0","valid":true},{"input":"-0","valid":true},{"input":"+0","valid":true},{"input":"000","valid":true},{"input":"+000.1","valid":true},{"input":"-0.25","valid":true},{"input":"1.","valid":true},{"input":".1","valid":true},{"input":"+.1","valid":true},{"input":"-.1","valid":true},{"input":"0.000000000000000001","valid":true},{"input":"99999999999999999999.999999999999999999","valid":true},{"input":"-99999999999999999999.999999999999999999","valid":true},{"input":"0000000000000000000000000000000000000000000000000000000000000000","valid":true},{"input":"1.00000000000000000000000000000000000000000000000000000000000000","valid":true},{"input":".000000000000000000000000000000000000000000000000000000000000000","valid":true},{"input":"","valid":false},{"input":"+","valid":false},{"input":"-","valid":false},{"input":".","valid":false},{"input":"+.","valid":false},{"input":"1e3","valid":false},{"input":"1E-3","valid":false},{"input":"1_000","valid":false},{"input":"NaN","valid":false},{"input":"Infinity","valid":false},{"input":" 1","valid":false},{"input":"1 ","valid":false},{"input":"1\n","valid":false},{"input":"1\r\n","valid":false},{"input":"1 ","valid":false},{"input":"1 ","valid":false},{"input":"\n1","valid":false},{"input":"1\u0000","valid":false},{"input":"١","valid":false},{"input":"１","valid":false},{"input":"1..0","valid":false},{"input":"++1","valid":false},{"input":"+-1","valid":false},{"input":"00000000000000000000000000000000000000000000000000000000000000000","valid":false},{"input":"1.000000000000000000000000000000000000000000000000000000000000000","valid":false},{"input":1,"valid":false},{"input":0.25,"valid":false},{"input":null,"valid":false},{"input":true,"valid":false},{"input":"9","valid":true},{"input":"99","valid":true},{"input":"999","valid":true},{"input":"9999","valid":true},{"input":"99999","valid":true},{"input":"999999","valid":true},{"input":"9999999","valid":true},{"input":"99999999","valid":true},{"input":"999999999","valid":true},{"input":"9999999999","valid":true},{"input":"99999999999","valid":true},{"input":"999999999999","valid":true},{"input":"9999999999999","valid":true},{"input":"99999999999999","valid":true},{"input":"999999999999999","valid":true},{"input":"9999999999999999","valid":true},{"input":"99999999999999999","valid":true},{"input":"999999999999999999","valid":true},{"input":"9999999999999999999","valid":true},{"input":"99999999999999999999","valid":true},{"input":"999999999999999999999","valid":false},{"input":"9999999999999999999999","valid":false},{"input":"99999999999999999999999","valid":false},{"input":"0.1","valid":true},{"input":"0.01","valid":true},{"input":"0.001","valid":true},{"input":"0.0001","valid":true},{"input":"0.00001","valid":true},{"input":"0.000001","valid":true},{"input":"0.0000001","valid":true},{"input":"0.00000001","valid":true},{"input":"0.000000001","valid":true},{"input":"0.0000000001","valid":true},{"input":"0.00000000001","valid":true},{"input":"0.000000000001","valid":true},{"input":"0.0000000000001","valid":true},{"input":"0.00000000000001","valid":true},{"input":"0.000000000000001","valid":true},{"input":"0.0000000000000001","valid":true},{"input":"0.00000000000000001","valid":true},{"input":"0.000000000000000001","valid":true},{"input":"0.0000000000000000001","valid":false},{"input":"0.00000000000000000001","valid":false},{"input":"0.000000000000000000001","valid":false},{"input":"0.0000000000000000000001","valid":false},{"input":"0.00000000000000000000001","valid":false},{"input":"0.25","valid":true},{"input":"0.250","valid":true},{"input":"0.2500000000000000000","valid":true},{"input":"0.25000000000000000000","valid":true},{"input":"0.250000000000000000000","valid":true},{"input":"0.2500000000000000000000000000000000","valid":true},{"input":"0.250000000000000000000000000000000000000000000000000000000000000","valid":false},{"input":"-9","valid":true},{"input":"-99","valid":true},{"input":"-999","valid":true},{"input":"-9999","valid":true},{"input":"-99999","valid":true},{"input":"-999999","valid":true},{"input":"-9999999","valid":true},{"input":"-99999999","valid":true},{"input":"-999999999","valid":true},{"input":"-9999999999","valid":true},{"input":"-99999999999","valid":true},{"input":"-999999999999","valid":true},{"input":"-9999999999999","valid":true},{"input":"-99999999999999","valid":true},{"input":"-999999999999999","valid":true},{"input":"-9999999999999999","valid":true},{"input":"-99999999999999999","valid":true},{"input":"-999999999999999999","valid":true},{"input":"-9999999999999999999","valid":true},{"input":"-99999999999999999999","valid":true},{"input":"-999999999999999999999","valid":false},{"input":"-9999999999999999999999","valid":false},{"input":"-99999999999999999999999","valid":false},{"input":"-0.1","valid":true},{"input":"-0.01","valid":true},{"input":"-0.001","valid":true},{"input":"-0.0001","valid":true},{"input":"-0.00001","valid":true},{"input":"-0.000001","valid":true},{"input":"-0.0000001","valid":true},{"input":"-0.00000001","valid":true},{"input":"-0.000000001","valid":true},{"input":"-0.0000000001","valid":true},{"input":"-0.00000000001","valid":true},{"input":"-0.000000000001","valid":true},{"input":"-0.0000000000001","valid":true},{"input":"-0.00000000000001","valid":true},{"input":"-0.000000000000001","valid":true},{"input":"-0.0000000000000001","valid":true},{"input":"-0.00000000000000001","valid":true},{"input":"-0.000000000000000001","valid":true},{"input":"-0.0000000000000000001","valid":false},{"input":"-0.00000000000000000001","valid":false},{"input":"-0.000000000000000000001","valid":false},{"input":"-0.0000000000000000000001","valid":false},{"input":"-0.00000000000000000000001","valid":false},{"input":"-0.25","valid":true},{"input":"-0.250","valid":true},{"input":"-0.2500000000000000000","valid":true},{"input":"-0.25000000000000000000","valid":true},{"input":"-0.250000000000000000000","valid":true},{"input":"-0.2500000000000000000000000000000000","valid":true},{"input":"-0.250000000000000000000000000000000000000000000000000000000000000","valid":false},{"input":"+9","valid":true},{"input":"+99","valid":true},{"input":"+999","valid":true},{"input":"+9999","valid":true},{"input":"+99999","valid":true},{"input":"+999999","valid":true},{"input":"+9999999","valid":true},{"input":"+99999999","valid":true},{"input":"+999999999","valid":true},{"input":"+9999999999","valid":true},{"input":"+99999999999","valid":true},{"input":"+999999999999","valid":true},{"input":"+9999999999999","valid":true},{"input":"+99999999999999","valid":true},{"input":"+999999999999999","valid":true},{"input":"+9999999999999999","valid":true},{"input":"+99999999999999999","valid":true},{"input":"+999999999999999999","valid":true},{"input":"+9999999999999999999","valid":true},{"input":"+99999999999999999999","valid":true},{"input":"+999999999999999999999","valid":false},{"input":"+9999999999999999999999","valid":false},{"input":"+99999999999999999999999","valid":false},{"input":"+0.1","valid":true},{"input":"+0.01","valid":true},{"input":"+0.001","valid":true},{"input":"+0.0001","valid":true},{"input":"+0.00001","valid":true},{"input":"+0.000001","valid":true},{"input":"+0.0000001","valid":true},{"input":"+0.00000001","valid":true},{"input":"+0.000000001","valid":true},{"input":"+0.0000000001","valid":true},{"input":"+0.00000000001","valid":true},{"input":"+0.000000000001","valid":true},{"input":"+0.0000000000001","valid":true},{"input":"+0.00000000000001","valid":true},{"input":"+0.000000000000001","valid":true},{"input":"+0.0000000000000001","valid":true},{"input":"+0.00000000000000001","valid":true},{"input":"+0.000000000000000001","valid":true},{"input":"+0.0000000000000000001","valid":false},{"input":"+0.00000000000000000001","valid":false},{"input":"+0.000000000000000000001","valid":false},{"input":"+0.0000000000000000000001","valid":false},{"input":"+0.00000000000000000000001","valid":false},{"input":"+0.25","valid":true},{"input":"+0.250","valid":true},{"input":"+0.2500000000000000000","valid":true},{"input":"+0.25000000000000000000","valid":true},{"input":"+0.250000000000000000000","valid":true},{"input":"+0.2500000000000000000000000000000000","valid":true},{"input":"+0.250000000000000000000000000000000000000000000000000000000000000","valid":false}]"###).unwrap();
    for case in cases {
        assert_eq!(
            from_value::<DecimalValue>(case["input"].clone()).is_ok(),
            case["valid"].as_bool().unwrap(),
            "{}",
            case["input"]
        );
    }
    let smallest: DecimalValue = "0.000000000000000001".parse().unwrap();
    let largest: DecimalValue = "99999999999999999999.999999999999999999".parse().unwrap();
    assert!(largest.checked_add(&smallest).is_none());
    assert_eq!(
        smallest.checked_add(&smallest).unwrap(),
        "0.000000000000000002".parse().unwrap()
    );
    assert_eq!(
        smallest.checked_add(&DecimalValue::zero()).unwrap(),
        smallest
    );
}

#[test]
fn generated_bigint_schemas_share_the_native_postgres_boundary() {
    let cases: serde_json::Value =
        serde_json::from_str(r###"[{"schema":"DbCounter","input":"0","valid":true},{"schema":"DbCounter","input":"1","valid":true},{"schema":"DbCounter","input":"01","valid":false},{"schema":"DbCounter","input":"+1","valid":false},{"schema":"DbCounter","input":"-1","valid":false},{"schema":"DbCounter","input":" 1","valid":false},{"schema":"DbCounter","input":"1 ","valid":false},{"schema":"DbCounter","input":"1\\n","valid":false},{"schema":"DbCounter","input":"1\n","valid":false},{"schema":"DbCounter","input":"1\r","valid":false},{"schema":"DbCounter","input":"1 ","valid":false},{"schema":"DbCounter","input":"１２","valid":false},{"schema":"DbCounter","input":"٢","valid":false},{"schema":"DbCounter","input":"","valid":false},{"schema":"DbCounter","input":null,"valid":false},{"schema":"DbCounter","input":1,"valid":false},{"schema":"DbCounter","input":true,"valid":false},{"schema":"DbCounter","input":"10","valid":true},{"schema":"DbCounter","input":"100","valid":true},{"schema":"DbCounter","input":"1000","valid":true},{"schema":"DbCounter","input":"10000","valid":true},{"schema":"DbCounter","input":"100000","valid":true},{"schema":"DbCounter","input":"1000000","valid":true},{"schema":"DbCounter","input":"10000000","valid":true},{"schema":"DbCounter","input":"100000000","valid":true},{"schema":"DbCounter","input":"1000000000","valid":true},{"schema":"DbCounter","input":"10000000000","valid":true},{"schema":"DbCounter","input":"100000000000","valid":true},{"schema":"DbCounter","input":"1000000000000","valid":true},{"schema":"DbCounter","input":"10000000000000","valid":true},{"schema":"DbCounter","input":"100000000000000","valid":true},{"schema":"DbCounter","input":"1000000000000000","valid":true},{"schema":"DbCounter","input":"10000000000000000","valid":true},{"schema":"DbCounter","input":"100000000000000000","valid":true},{"schema":"DbCounter","input":"1000000000000000000","valid":true},{"schema":"DbCounter","input":"10000000000000000000","valid":false},{"schema":"DbCounter","input":"100000000000000000000","valid":false},{"schema":"DbCounter","input":"9223372036854775768","valid":true},{"schema":"DbCounter","input":"9223372036854775769","valid":true},{"schema":"DbCounter","input":"9223372036854775770","valid":true},{"schema":"DbCounter","input":"9223372036854775771","valid":true},{"schema":"DbCounter","input":"9223372036854775772","valid":true},{"schema":"DbCounter","input":"9223372036854775773","valid":true},{"schema":"DbCounter","input":"9223372036854775774","valid":true},{"schema":"DbCounter","input":"9223372036854775775","valid":true},{"schema":"DbCounter","input":"9223372036854775776","valid":true},{"schema":"DbCounter","input":"9223372036854775777","valid":true},{"schema":"DbCounter","input":"9223372036854775778","valid":true},{"schema":"DbCounter","input":"9223372036854775779","valid":true},{"schema":"DbCounter","input":"9223372036854775780","valid":true},{"schema":"DbCounter","input":"9223372036854775781","valid":true},{"schema":"DbCounter","input":"9223372036854775782","valid":true},{"schema":"DbCounter","input":"9223372036854775783","valid":true},{"schema":"DbCounter","input":"9223372036854775784","valid":true},{"schema":"DbCounter","input":"9223372036854775785","valid":true},{"schema":"DbCounter","input":"9223372036854775786","valid":true},{"schema":"DbCounter","input":"9223372036854775787","valid":true},{"schema":"DbCounter","input":"9223372036854775788","valid":true},{"schema":"DbCounter","input":"9223372036854775789","valid":true},{"schema":"DbCounter","input":"9223372036854775790","valid":true},{"schema":"DbCounter","input":"9223372036854775791","valid":true},{"schema":"DbCounter","input":"9223372036854775792","valid":true},{"schema":"DbCounter","input":"9223372036854775793","valid":true},{"schema":"DbCounter","input":"9223372036854775794","valid":true},{"schema":"DbCounter","input":"9223372036854775795","valid":true},{"schema":"DbCounter","input":"9223372036854775796","valid":true},{"schema":"DbCounter","input":"9223372036854775797","valid":true},{"schema":"DbCounter","input":"9223372036854775798","valid":true},{"schema":"DbCounter","input":"9223372036854775799","valid":true},{"schema":"DbCounter","input":"9223372036854775800","valid":true},{"schema":"DbCounter","input":"9223372036854775801","valid":true},{"schema":"DbCounter","input":"9223372036854775802","valid":true},{"schema":"DbCounter","input":"9223372036854775803","valid":true},{"schema":"DbCounter","input":"9223372036854775804","valid":true},{"schema":"DbCounter","input":"9223372036854775805","valid":true},{"schema":"DbCounter","input":"9223372036854775806","valid":true},{"schema":"DbCounter","input":"9223372036854775807","valid":true},{"schema":"DbCounter","input":"9223372036854775808","valid":false},{"schema":"DbCounter","input":"9223372036854775809","valid":false},{"schema":"DbCounter","input":"9223372036854775810","valid":false},{"schema":"DbCounter","input":"9223372036854775811","valid":false},{"schema":"DbCounter","input":"9223372036854775812","valid":false},{"schema":"DbCounter","input":"9223372036854775813","valid":false},{"schema":"DbCounter","input":"9223372036854775814","valid":false},{"schema":"DbCounter","input":"9223372036854775815","valid":false},{"schema":"DbCounter","input":"9223372036854775816","valid":false},{"schema":"DbCounter","input":"9223372036854775817","valid":false},{"schema":"DbCounter","input":"9223372036854775818","valid":false},{"schema":"DbCounter","input":"9223372036854775819","valid":false},{"schema":"DbCounter","input":"9223372036854775820","valid":false},{"schema":"DbCounter","input":"9223372036854775821","valid":false},{"schema":"DbCounter","input":"9223372036854775822","valid":false},{"schema":"DbCounter","input":"9223372036854775823","valid":false},{"schema":"DbCounter","input":"9223372036854775824","valid":false},{"schema":"DbCounter","input":"9223372036854775825","valid":false},{"schema":"DbCounter","input":"9223372036854775826","valid":false},{"schema":"DbCounter","input":"9223372036854775827","valid":false},{"schema":"DbCounter","input":"9223372036854775828","valid":false},{"schema":"DbCounter","input":"9223372036854775829","valid":false},{"schema":"DbCounter","input":"9223372036854775830","valid":false},{"schema":"DbCounter","input":"9223372036854775831","valid":false},{"schema":"DbCounter","input":"9223372036854775832","valid":false},{"schema":"DbCounter","input":"9223372036854775833","valid":false},{"schema":"DbCounter","input":"9223372036854775834","valid":false},{"schema":"DbCounter","input":"9223372036854775835","valid":false},{"schema":"DbCounter","input":"9223372036854775836","valid":false},{"schema":"DbCounter","input":"9223372036854775837","valid":false},{"schema":"DbCounter","input":"9223372036854775838","valid":false},{"schema":"DbCounter","input":"9223372036854775839","valid":false},{"schema":"DbCounter","input":"9223372036854775840","valid":false},{"schema":"DbCounter","input":"9223372036854775841","valid":false},{"schema":"DbCounter","input":"9223372036854775842","valid":false},{"schema":"DbCounter","input":"9223372036854775843","valid":false},{"schema":"DbCounter","input":"9223372036854775844","valid":false},{"schema":"DbCounter","input":"9223372036854775845","valid":false},{"schema":"DbCounter","input":"9223372036854775846","valid":false},{"schema":"DbCounter","input":"9223372036854775847","valid":false},{"schema":"DbCounter","input":"9223372036854775848","valid":false},{"schema":"DbCounter","input":"00000000000000000000","valid":false},{"schema":"DbCounter","input":"99999999999999999999","valid":false},{"schema":"DbCounter","input":"1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","valid":false},{"schema":"Revision","input":"0","valid":false},{"schema":"Revision","input":"1","valid":true},{"schema":"Revision","input":"01","valid":false},{"schema":"Revision","input":"+1","valid":false},{"schema":"Revision","input":"-1","valid":false},{"schema":"Revision","input":" 1","valid":false},{"schema":"Revision","input":"1 ","valid":false},{"schema":"Revision","input":"1\\n","valid":false},{"schema":"Revision","input":"1\n","valid":false},{"schema":"Revision","input":"1\r","valid":false},{"schema":"Revision","input":"1 ","valid":false},{"schema":"Revision","input":"１２","valid":false},{"schema":"Revision","input":"٢","valid":false},{"schema":"Revision","input":"","valid":false},{"schema":"Revision","input":null,"valid":false},{"schema":"Revision","input":1,"valid":false},{"schema":"Revision","input":true,"valid":false},{"schema":"Revision","input":"10","valid":true},{"schema":"Revision","input":"100","valid":true},{"schema":"Revision","input":"1000","valid":true},{"schema":"Revision","input":"10000","valid":true},{"schema":"Revision","input":"100000","valid":true},{"schema":"Revision","input":"1000000","valid":true},{"schema":"Revision","input":"10000000","valid":true},{"schema":"Revision","input":"100000000","valid":true},{"schema":"Revision","input":"1000000000","valid":true},{"schema":"Revision","input":"10000000000","valid":true},{"schema":"Revision","input":"100000000000","valid":true},{"schema":"Revision","input":"1000000000000","valid":true},{"schema":"Revision","input":"10000000000000","valid":true},{"schema":"Revision","input":"100000000000000","valid":true},{"schema":"Revision","input":"1000000000000000","valid":true},{"schema":"Revision","input":"10000000000000000","valid":true},{"schema":"Revision","input":"100000000000000000","valid":true},{"schema":"Revision","input":"1000000000000000000","valid":true},{"schema":"Revision","input":"10000000000000000000","valid":false},{"schema":"Revision","input":"100000000000000000000","valid":false},{"schema":"Revision","input":"9223372036854775768","valid":true},{"schema":"Revision","input":"9223372036854775769","valid":true},{"schema":"Revision","input":"9223372036854775770","valid":true},{"schema":"Revision","input":"9223372036854775771","valid":true},{"schema":"Revision","input":"9223372036854775772","valid":true},{"schema":"Revision","input":"9223372036854775773","valid":true},{"schema":"Revision","input":"9223372036854775774","valid":true},{"schema":"Revision","input":"9223372036854775775","valid":true},{"schema":"Revision","input":"9223372036854775776","valid":true},{"schema":"Revision","input":"9223372036854775777","valid":true},{"schema":"Revision","input":"9223372036854775778","valid":true},{"schema":"Revision","input":"9223372036854775779","valid":true},{"schema":"Revision","input":"9223372036854775780","valid":true},{"schema":"Revision","input":"9223372036854775781","valid":true},{"schema":"Revision","input":"9223372036854775782","valid":true},{"schema":"Revision","input":"9223372036854775783","valid":true},{"schema":"Revision","input":"9223372036854775784","valid":true},{"schema":"Revision","input":"9223372036854775785","valid":true},{"schema":"Revision","input":"9223372036854775786","valid":true},{"schema":"Revision","input":"9223372036854775787","valid":true},{"schema":"Revision","input":"9223372036854775788","valid":true},{"schema":"Revision","input":"9223372036854775789","valid":true},{"schema":"Revision","input":"9223372036854775790","valid":true},{"schema":"Revision","input":"9223372036854775791","valid":true},{"schema":"Revision","input":"9223372036854775792","valid":true},{"schema":"Revision","input":"9223372036854775793","valid":true},{"schema":"Revision","input":"9223372036854775794","valid":true},{"schema":"Revision","input":"9223372036854775795","valid":true},{"schema":"Revision","input":"9223372036854775796","valid":true},{"schema":"Revision","input":"9223372036854775797","valid":true},{"schema":"Revision","input":"9223372036854775798","valid":true},{"schema":"Revision","input":"9223372036854775799","valid":true},{"schema":"Revision","input":"9223372036854775800","valid":true},{"schema":"Revision","input":"9223372036854775801","valid":true},{"schema":"Revision","input":"9223372036854775802","valid":true},{"schema":"Revision","input":"9223372036854775803","valid":true},{"schema":"Revision","input":"9223372036854775804","valid":true},{"schema":"Revision","input":"9223372036854775805","valid":true},{"schema":"Revision","input":"9223372036854775806","valid":true},{"schema":"Revision","input":"9223372036854775807","valid":true},{"schema":"Revision","input":"9223372036854775808","valid":false},{"schema":"Revision","input":"9223372036854775809","valid":false},{"schema":"Revision","input":"9223372036854775810","valid":false},{"schema":"Revision","input":"9223372036854775811","valid":false},{"schema":"Revision","input":"9223372036854775812","valid":false},{"schema":"Revision","input":"9223372036854775813","valid":false},{"schema":"Revision","input":"9223372036854775814","valid":false},{"schema":"Revision","input":"9223372036854775815","valid":false},{"schema":"Revision","input":"9223372036854775816","valid":false},{"schema":"Revision","input":"9223372036854775817","valid":false},{"schema":"Revision","input":"9223372036854775818","valid":false},{"schema":"Revision","input":"9223372036854775819","valid":false},{"schema":"Revision","input":"9223372036854775820","valid":false},{"schema":"Revision","input":"9223372036854775821","valid":false},{"schema":"Revision","input":"9223372036854775822","valid":false},{"schema":"Revision","input":"9223372036854775823","valid":false},{"schema":"Revision","input":"9223372036854775824","valid":false},{"schema":"Revision","input":"9223372036854775825","valid":false},{"schema":"Revision","input":"9223372036854775826","valid":false},{"schema":"Revision","input":"9223372036854775827","valid":false},{"schema":"Revision","input":"9223372036854775828","valid":false},{"schema":"Revision","input":"9223372036854775829","valid":false},{"schema":"Revision","input":"9223372036854775830","valid":false},{"schema":"Revision","input":"9223372036854775831","valid":false},{"schema":"Revision","input":"9223372036854775832","valid":false},{"schema":"Revision","input":"9223372036854775833","valid":false},{"schema":"Revision","input":"9223372036854775834","valid":false},{"schema":"Revision","input":"9223372036854775835","valid":false},{"schema":"Revision","input":"9223372036854775836","valid":false},{"schema":"Revision","input":"9223372036854775837","valid":false},{"schema":"Revision","input":"9223372036854775838","valid":false},{"schema":"Revision","input":"9223372036854775839","valid":false},{"schema":"Revision","input":"9223372036854775840","valid":false},{"schema":"Revision","input":"9223372036854775841","valid":false},{"schema":"Revision","input":"9223372036854775842","valid":false},{"schema":"Revision","input":"9223372036854775843","valid":false},{"schema":"Revision","input":"9223372036854775844","valid":false},{"schema":"Revision","input":"9223372036854775845","valid":false},{"schema":"Revision","input":"9223372036854775846","valid":false},{"schema":"Revision","input":"9223372036854775847","valid":false},{"schema":"Revision","input":"9223372036854775848","valid":false},{"schema":"Revision","input":"00000000000000000000","valid":false},{"schema":"Revision","input":"99999999999999999999","valid":false},{"schema":"Revision","input":"1111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111111","valid":false}]"###).unwrap();
    for case in cases.as_array().unwrap() {
        let input = case["input"].clone();
        let accepted = match case["schema"].as_str().unwrap() {
            "DbCounter" => serde_json::from_value::<DbCounter>(input).is_ok(),
            "Revision" => serde_json::from_value::<Revision>(input).is_ok(),
            other => panic!("unknown scalar in shared corpus: {other}"),
        };
        assert_eq!(accepted, case["valid"].as_bool().unwrap(), "{case}");
    }
}
