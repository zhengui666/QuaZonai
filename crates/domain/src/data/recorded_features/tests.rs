use super::*;
use chrono::{DateTime, Utc};
use contracts::{
    catalogs::{RecordedFeatureFragmentV1, RecordedFeatureInputsV1},
    control::{OperatorCommand, OperatorOperation},
    research::{DataOrigin, DataPartition},
    Id, SchemaV1,
};

fn n(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn instant(ns: i64) -> DateTime<Utc> {
    DateTime::from_timestamp_nanos(ns)
}
fn request() -> RecordedFeatureRegisterV1 {
    RecordedFeatureRegisterV1 {
        schema_version: SchemaV1,
        project_id: Id::new(),
        dataset_revision_id: Id::new(),
        feature_part_key: "features-0001".into(),
        content: concat!(" \n{\"schema_version\":1,\"partition\":\"DISCOVERY\",",
            "\"feature_schema\":[{\"feature_key\":\"price\",\"source_ref\":\"paired:report\",\"source_key\":\"pm\",",
            "\"availability\":{\"basis\":\"OBSERVED\"},\"max_age_ns\":null}],",
            "\"observations\":[{\"feature_index\":0,\"event_ns\":\"50000000000\",\"observed_available_ns\":\"55000000000\",",
            "\"sequence\":\"0\",\"value\":0.10000000000000002,\"missing_reason\":null},",
            "{\"feature_index\":0,\"event_ns\":\"100000000000\",\"observed_available_ns\":\"110000000000\",",
            "\"sequence\":\"1\",\"value\":null,\"missing_reason\":\"recorded_missing\"}]}\n").into(),
    }
}
fn attachments(bytes: usize) -> RecordedFeatureInputsV1 {
    RecordedFeatureInputsV1 {
        schema_version: SchemaV1,

        source_selection_start_ns: n(40_000_000_000),
        source_selection_end_ns: n(101_000_000_000),
        partition: DataPartition::Discovery,
        fragments: vec![RecordedFeatureFragmentV1 {
            part_key: "features-0001".into(),
            byte_count: n(bytes as u64),
            observations: n(2),
            min_event_ns: n(50_000_000_000),
            max_event_ns: n(100_000_000_000),
            min_observed_available_ns: n(55_000_000_000),
            max_observed_available_ns: n(110_000_000_000),
        }],
    }
}
fn metadata() -> RuntimeCatalogMetadataV1 {
    serde_json::from_value(serde_json::json!({
        "schema_version":1,"registered_ref":"paired","native_snapshot_ref":"original", "storage_version":"v1",
        "provider_kind":"NAUTILUS_CATALOG","data_kind":"BAR","partition":"DISCOVERY",
        "event_start":instant(60_000_000_000),"event_end":instant(240_000_000_000),"available_through":instant(300_000_000_000),
        "row_count":"3","origin":"REAL","pit_status":"UNVERIFIED","revision_policy":"UNKNOWN",
        "provenance_reference":"controlled test reference","availability_provenance":"recorded test clocks",
        "universe":{"name":"test","calendar_ref":"test","calendar_version":"1","calendar_sessions":null,
            "selection_asof":instant(0),"has_historical_membership":false,"coverage_start":instant(0),"coverage_end":instant(600_000_000_000),
            "membership":[{"instrument_id":"EUR/USD.SIM","valid_from":instant(0),"valid_until":null,"available_at":instant(0),"groups":null}],
            "instrument_definitions":[{"CurrencyPair":{"id":"EUR/USD.SIM","ts_event":0,"ts_init":0,"price_increment":"0.00001"}}]},
        "quality":{"schema_version":1,"native_version":"nautilus-persistence/0.63.0","checked_at":instant(301_000_000_000),
            "datasets":[{"dataset_revision_id":Id::new(),"selection":{"schema_version":1,
                "bar_types":["EUR/USD.SIM-1-MINUTE-LAST-EXTERNAL"],"event_start_ns":"60000000000","event_end_ns":"240000000000",
                "decision_cutoff_ns":"300000000000","maximum_rows":3},"row_count":"3","instrument_ids":["EUR/USD.SIM"],
                "first_event_ns":"60000000000","last_event_ns":"180000000000","available_through_ns":"220000000000",
                "settlements":[],"last_bar_notionals":null}]}
    })).unwrap()
}

#[test]
fn legacy_metadata_and_provenance_remain_readable_without_invented_bindings() {
    let value = metadata();
    assert!(value.recorded_feature_inputs.is_none());
    assert!(serde_json::to_value(value)
        .unwrap()
        .get("recorded_feature_inputs")
        .is_none());
    let old = serde_json::json!({"dataset_revision_id":Id::new(),"market_data_origin":"SYNTHETIC",
        "pit_status":"UNVERIFIED","revision_policy":"UNKNOWN","feature_artifact_origins":{}});
    let parsed: StrategyInputProvenanceV1 = serde_json::from_value(old.clone()).unwrap();
    assert!(parsed.feature_source_bindings.is_none());
    recorded_feature_provenance(&parsed).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), old);
}

#[test]
fn original_upload_survives_wire_and_command_retains_source_identity() {
    let request = request();
    let encoded = serde_json::to_vec(&request).unwrap();
    let decoded: RecordedFeatureRegisterV1 = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(decoded.content.as_bytes(), request.content.as_bytes());
    let parsed = recorded_feature_register(&decoded).unwrap();
    assert_eq!(parsed.observations[1].value, None);
    assert_eq!(parsed.observations[0].sequence, DbCounter::ZERO);
    let intent = request.intent().unwrap();
    let command = OperatorCommand::RecordedFeatureRegister(intent.clone());
    crate::control::command(&command).unwrap();
    assert_eq!(
        command.operation(),
        OperatorOperation::RecordedFeatureRegister
    );
    assert_eq!(command.operation().code(), "RECORDED_FEATURE_REGISTER");
    assert!(!command.operation().creates());
    assert_eq!(
        command.normalized_request().unwrap(),
        serde_json::to_value(&intent).unwrap()
    );
    let normalized = serde_json::to_string(&command).unwrap();
    assert!(normalized.len() < 1024);
    assert!(!normalized.contains("observations"));
    assert!(!format!("{command:?}").contains("recorded_missing"));
    assert_eq!(intent.byte_count.get(), request.content.len() as u64);
    assert_eq!(intent.project_id, request.project_id);
    assert_eq!(intent.dataset_revision_id, request.dataset_revision_id);
    assert_eq!(intent.feature_part_key, request.feature_part_key);
    let mut illegal = serde_json::to_value(request).unwrap();
    for field in ["origin", "pit_status", "source_id", "grant_id", "file"] {
        illegal[field] = "client-assertion".into();
        assert!(serde_json::from_value::<RecordedFeatureRegisterV1>(illegal.clone()).is_err());
        illegal.as_object_mut().unwrap().remove(field);
    }
}

#[test]
fn descriptor_preserves_feature_prefix_and_late_receipt_instead_of_using_bar_window() {
    let request = request();
    let content = recorded_feature_register(&request).unwrap();
    let mut metadata = metadata();
    metadata.recorded_feature_inputs = Some(attachments(request.content.len()));
    crate::catalogs::metadata(&metadata, instant(302_000_000_000)).unwrap();
    let check = |metadata: &RuntimeCatalogMetadataV1, bytes: usize| {
        recorded_feature_matches(metadata, "features-0001", n(bytes as u64), &content)
    };
    check(&metadata, request.content.len()).unwrap();
    assert!(check(&metadata, request.content.len() + 1).is_err());
    metadata.recorded_feature_inputs.as_mut().unwrap().fragments[0].max_observed_available_ns =
        n(109_000_000_000);
    assert!(check(&metadata, request.content.len()).is_err());
    metadata.recorded_feature_inputs = Some(attachments(request.content.len()));
    metadata
        .recorded_feature_inputs
        .as_mut()
        .unwrap()
        .source_selection_end_ns = n(100_000_000_000);
    assert!(check(&metadata, request.content.len()).is_err());
    metadata.recorded_feature_inputs = Some(attachments(request.content.len()));
    metadata.available_through = instant(109_000_000_000);
    assert!(check(&metadata, request.content.len()).is_err());
    metadata.recorded_feature_inputs = None;
    assert!(check(&metadata, request.content.len()).is_err());
}

#[test]
fn rejects_unbounded_descriptors_unobserved_clocks_and_unsafe_part_keys() {
    let mut descriptor = attachments(123);
    let check = |v: &RecordedFeatureInputsV1| {
        crate::catalogs::recorded_feature_inputs(
            v,
            DataPartition::Discovery,
            instant(300_000_000_000),
        )
    };
    descriptor.fragments.push(descriptor.fragments[0].clone());
    assert!(check(&descriptor).is_err());
    descriptor.fragments[1].part_key = "features-0002".into();
    descriptor.fragments[0].observations = n(100_000);
    assert!(check(&descriptor).is_err());
    descriptor.fragments.pop();
    descriptor.fragments[0].byte_count = n(MAX_UPLOAD_BYTES as u64 + 1);
    assert!(check(&descriptor).is_err());
    for key in [
        "",
        "../x",
        "/tmp/feature",
        "x/y",
        "https://source",
        "part\n",
        "界",
    ] {
        assert!(crate::catalogs::recorded_feature_part_key(key).is_err());
    }
    let mut request = request();
    request.content = request.content.replace(
        "\"basis\":\"OBSERVED\"",
        "\"basis\":\"MODELED_LAG\",\"lag_ns\":\"1\"",
    );
    assert!(recorded_feature_register(&request).is_err());
    let mut intent = request.intent().unwrap();
    intent.byte_count = DbCounter::ZERO;
    assert!(recorded_feature_intent(&intent).is_err());
}

#[test]
fn optional_provenance_matches_actual_origins_but_keeps_historical_dataset_identity() {
    let historical = Id::new();
    let artifact = Id::new();
    let binding = RecordedFeatureSourceBindingV1 {
        dataset_revision_id: historical,
        source_id: Id::new(),
        data_use_grant_id: Id::new(),
        native_metadata_artifact_id: Id::new(),
        feature_part_key: "features-0001".into(),
        origin: DataOrigin::Real,
        pit_status: PitStatus::Unverified,
        revision_policy: DataRevisionPolicy::Unknown,
    };
    let mut value = StrategyInputProvenanceV1 {
        dataset_revision_id: Id::new(),
        market_data_origin: DataOrigin::Real,
        pit_status: PitStatus::Unverified,
        revision_policy: DataRevisionPolicy::Unknown,
        feature_artifact_origins: BTreeMap::from([(artifact, DataOrigin::Real)]),
        feature_source_bindings: Some(BTreeMap::from([(artifact, binding.clone())])),
    };
    recorded_feature_provenance(&value).unwrap();
    value
        .feature_artifact_origins
        .insert(artifact, DataOrigin::Synthetic);
    assert!(recorded_feature_provenance(&value).is_err());
    value
        .feature_artifact_origins
        .insert(artifact, DataOrigin::Real);
    let mut second = binding.clone();
    second.feature_part_key = "features-0002".into();
    second.data_use_grant_id = Id::new();
    let second_artifact = Id::new();
    value
        .feature_artifact_origins
        .insert(second_artifact, DataOrigin::Real);
    value
        .feature_source_bindings
        .as_mut()
        .unwrap()
        .insert(second_artifact, second);
    assert!(recorded_feature_provenance(&value).is_err());
}
