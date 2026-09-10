//! Wire/domain boundary tests, not a substitute for native OCI acceptance.
use chrono::{DateTime, Duration, Utc};
use contracts::{
    research::{ArtifactInputRole, DataPartition},
    runs::RunKind,
    runtime::{
        IsolationProfile, LabelIntervalSupportV1, RuntimeArtifactSchemaV1, RuntimeCapabilitiesV1,
        RuntimeDataKind, RuntimeImageV1,
    },
    runtime_jobs::*,
    DbCounter, Id, Revision, SchemaV1,
};
use domain::runtime_jobs::*;
use serde_json::json;
use std::collections::BTreeMap;

fn now() -> DateTime<Utc> {
    DateTime::from_timestamp(1_783_000_000, 123_000).unwrap()
}
fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn image() -> String {
    format!("localhost/science@sha256:{}", "a".repeat(64))
}
fn schema() -> RuntimeArtifactSchemaV1 {
    RuntimeArtifactSchemaV1 {
        name: "qz.data_quality".into(),
        version: "1".into(),
    }
}
fn spec() -> JobSpecV1 {
    let run_id = Id::new();
    JobSpecV1 {
        schema_version: SchemaV1,
        run_id,
        attempt_no: 1,
        owner_epoch: Revision::INITIAL,
        external_job_id: external_id(run_id, 1).unwrap(),
        job_kind: RunKind::DataValidate,
        image_ref: image(),
        input_set_id: Id::new(),
        inputs: vec![RuntimeInputV1::Dataset {
            revision_id: Id::new(),
            registered_ref: "registered-fixture-catalog".into(),
            storage_version: "1".into(),
            role: DataPartition::Discovery,
        }],
        parameters_artifact_id: Id::new(),
        limits: RuntimeJobLimitsV1 {
            cpu: 2,
            cpu_seconds: count(30),
            memory_mib: 128,
            wall_seconds: 60,
            output_bytes: count(4096),
        },
        deadline_at: now() + Duration::seconds(60),
        requested_output_schemas: vec![schema()],
    }
}
fn capabilities() -> RuntimeCapabilitiesV1 {
    RuntimeCapabilitiesV1 {
        schema_version: SchemaV1,
        protocol_versions: vec![SchemaV1],
        runtime_version: "fixture".into(),
        engine_versions: BTreeMap::from([("native-fixture".into(), "1".into())]),
        image_refs: vec![RuntimeImageV1 {
            job_kind: RunKind::DataValidate,
            image_ref: image(),
        }],
        job_kinds: vec![RunKind::DataValidate],
        artifact_schemas: vec![schema()],
        data_kinds: vec![RuntimeDataKind::Bar],
        venues: vec![],
        label_interval_support: LabelIntervalSupportV1 {
            fixed_bars: true,
            fixed_duration: false,
            variable_interval: false,
        },
        solver_capabilities: vec![],
        max_cpu: 2,
        max_memory_mib: 128,
        max_output_bytes: count(4096),
        max_wall_seconds: 60,
        isolation_profile: IsolationProfile::OciResearchV1,
        checked_at: now(),
    }
}
fn result(spec: &JobSpecV1) -> ResultManifestV1 {
    ResultManifestV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: spec.attempt_no,
        external_job_id: spec.external_job_id.clone(),
        input_set_id: spec.input_set_id,
        state: RuntimeResultState::Succeeded,
        engine_versions: BTreeMap::from([("native-fixture".into(), "1".into())]),
        started_at: Some(now()),
        finished_at: now() + Duration::seconds(1),
        resource_usage: RuntimeResourceUsageV1 {
            wall_milliseconds: count(1000),
            cpu_nanoseconds: Some(count(500_000_000)),
            peak_memory_bytes: Some(count(1024)),
            output_bytes: count(128),
        },
        artifacts: vec![RuntimeOutputV1 {
            kind: RuntimeOutputKind::DataQuality,
            schema: schema(),
            storage_ref: Id::new(),
            storage_version: Revision::INITIAL,
            byte_count: count(128),
            media_type: "application/json".into(),
        }],
        error: None,
    }
}

#[test]
fn canonical_native_identity_is_not_an_arbitrary_path() {
    let run = Id::new();
    for attempt in [1, 42, u32::MAX] {
        let text = external_id(run, attempt).unwrap();
        assert_eq!(parse_external_id(&text).unwrap(), (run, attempt));
    }
    for text in [
        format!("{run}/0"),
        format!("{run}/01"),
        format!("{run}/+1"),
        format!("{run}/4294967296"),
        format!("{run}/1/../../secret"),
        format!("{run}/1?access=secret"),
        format!("{run}%2F1"),
        format!("{run}/1\n"),
        "../secret/1".into(),
    ] {
        assert!(parse_external_id(&text).is_err());
    }
    assert!(external_id(run, 0).is_err());
}

#[test]
fn strict_wire_types_reject_extension_commands_and_numeric_revisions() {
    let original = serde_json::to_value(spec()).unwrap();
    for (field, value) in [
        ("owner_epoch", json!(1)),
        ("schema_version", json!(2)),
        ("attempt_no", json!(0)),
        ("command", json!(["sh", "-c", "anything"])),
        ("environment", json!({"DATABASE_URL":"unexpected"})),
    ] {
        let mut changed = original.clone();
        changed[field] = value;
        let decoded = serde_json::from_value::<JobSpecV1>(changed);
        assert!(decoded.is_err() || decoded.is_ok_and(|value| spec_shape(&value).is_err()));
    }
    let mut changed = original.clone();
    changed["inputs"][0]["path"] = json!("/etc/passwd");
    assert!(serde_json::from_value::<JobSpecV1>(changed).is_err());
    let mut changed = original;
    changed["limits"]["output_bytes"] = json!(4096);
    assert!(serde_json::from_value::<JobSpecV1>(changed).is_err());
}

#[test]
fn native_admission_requires_exact_observed_image_schema_and_limits() {
    let original = spec();
    let caps = capabilities();
    admit_spec(&original, &caps, now()).unwrap();
    for index in 0..9 {
        let mut changed = original.clone();
        match index {
            0 => changed.image_ref = "localhost/science:latest".into(),
            1 => changed.image_ref = image().replace('a', "b"),
            2 => changed.job_kind = RunKind::AgentResearch,
            3 => changed.limits.cpu = 3,
            4 => changed.limits.memory_mib = 129,
            5 => changed.limits.output_bytes = count(4097),
            6 => changed.limits.wall_seconds = 61,
            7 => changed.requested_output_schemas[0].version = "2".into(),
            _ => changed.deadline_at = now(),
        }
        assert!(admit_spec(&changed, &caps, now()).is_err());
    }
    let mut old = caps;
    old.checked_at = now() - Duration::minutes(6);
    assert!(admit_spec(&original, &old, now()).is_err());
    spec_shape(&original).unwrap();
    assert!(admit_spec(&original, &capabilities(), original.deadline_at).is_err());
}

#[test]
fn duplicate_inputs_schemas_and_excessive_resources_are_rejected() {
    let original = spec();
    for index in 0..9 {
        let mut changed = original.clone();
        match index {
            0 => changed.inputs.push(changed.inputs[0].clone()),
            1 => changed.requested_output_schemas.push(schema()),
            2 => changed.limits.cpu_seconds = count(121),
            3 => changed.limits.output_bytes = count(MAX_INPUT_OBJECT_BYTES + 1),
            4 => changed.deadline_at += Duration::nanoseconds(1),
            5 => changed.inputs.clear(),
            6 => changed.inputs.push(RuntimeInputV1::Artifact {
                artifact_id: changed.parameters_artifact_id,
                storage_version: "1".into(),
                byte_count: count(10),
                role: ArtifactInputRole::Code,
            }),
            7 => changed.inputs.push(RuntimeInputV1::Dataset {
                revision_id: changed.parameters_artifact_id,
                registered_ref: "other".into(),
                storage_version: "1".into(),
                role: DataPartition::Discovery,
            }),
            _ => changed.limits.cpu_seconds = DbCounter::ZERO,
        }
        assert!(spec_shape(&changed).is_err());
    }
    let mut changed = original;
    for _ in 0..5 {
        changed.inputs.push(RuntimeInputV1::Artifact {
            artifact_id: Id::new(),
            storage_version: "1".into(),
            byte_count: count(MAX_INPUT_OBJECT_BYTES),
            role: ArtifactInputRole::Report,
        });
    }
    assert!(spec_shape(&changed).is_err());
}

#[test]
fn output_schema_kind_and_media_are_one_registered_contract() {
    let mut spec = spec();
    let observed = now() + Duration::seconds(2);
    for contract in NATIVE_OUTPUT_CONTRACTS {
        spec.requested_output_schemas[0].name = contract.name.into();
        let mut value = result(&spec);
        value.artifacts[0].schema = spec.requested_output_schemas[0].clone();
        value.artifacts[0].kind = contract.kind;
        value.artifacts[0].media_type = contract.media_type.into();
        manifest(&value, &spec, now(), observed).unwrap();
        for wrong in [
            RuntimeOutputKind::Model,
            RuntimeOutputKind::Signals,
            RuntimeOutputKind::Targets,
            RuntimeOutputKind::Report,
            RuntimeOutputKind::Metrics,
            RuntimeOutputKind::DataQuality,
        ] {
            if wrong == contract.kind {
                continue;
            }
            value.artifacts[0].kind = wrong;
            assert!(manifest(&value, &spec, now(), observed).is_err());
        }
    }
    let mut value = result(&spec);
    spec.requested_output_schemas[0].name = "qz.unregistered_output".into();
    value.artifacts[0].schema = spec.requested_output_schemas[0].clone();
    assert!(manifest(&value, &spec, now(), observed).is_err());
}

#[test]
fn invalid_oci_names_never_become_probe_or_job_admission_authority() {
    let digest = format!("sha256:{}", "a".repeat(64));
    for name in [
        "https://registry.example/image",
        "../image",
        "./image",
        "/image",
        "registry.example//image",
        "registry.example/UPPER",
        "registry.example/image/",
    ] {
        let reference = format!("{name}@{digest}");
        let mut caps = capabilities();
        caps.image_refs[0].image_ref = reference.clone();
        assert!(domain::runtime::capabilities(&caps, now()).is_err());
        let mut request = spec();
        request.image_ref = reference;
        assert!(spec_shape(&request).is_err());
        assert!(admit_spec(&request, &caps, now()).is_err());
    }
    for reference in [
        digest.clone(),
        format!("registry.example:5000/team/native@{digest}"),
        format!("library/native:v1@{digest}"),
    ] {
        assert!(domain::runtime::pinned_image(&reference));
    }
}

#[test]
fn result_identity_resources_and_immutable_output_contract_are_all_required() {
    let spec = spec();
    let original = result(&spec);
    let observed = now() + Duration::seconds(2);
    manifest(&original, &spec, now(), observed).unwrap();
    for index in 0..20 {
        let mut changed = original.clone();
        match index {
            0 => changed.run_id = Id::new(),
            1 => changed.attempt_no = 2,
            2 => changed.external_job_id = external_id(Id::new(), 1).unwrap(),
            3 => changed.input_set_id = Id::new(),
            4 => changed.finished_at = spec.deadline_at + Duration::seconds(1),
            5 => changed.started_at = None,
            6 => changed.artifacts.clear(),
            7 => changed.artifacts[0].storage_version = Revision::INITIAL.next().unwrap(),
            8 => changed.artifacts[0].schema.version = "2".into(),
            9 => changed.artifacts[0].media_type = "text/html".into(),
            10 => changed.artifacts[0].byte_count = DbCounter::ZERO,
            11 => changed.artifacts.push(changed.artifacts[0].clone()),
            12 => changed.resource_usage.output_bytes = count(129),
            13 => changed.resource_usage.cpu_nanoseconds = Some(count(30_000_000_001)),
            14 => changed.resource_usage.peak_memory_bytes = Some(count(128 * 1024 * 1024 + 1)),
            15 => changed.resource_usage.wall_milliseconds = count(60_001),
            16 => changed.error = Some(error(RuntimeFailureCode::InvalidOutput)),
            17 => changed.started_at = Some(changed.finished_at + Duration::seconds(1)),
            18 => changed.finished_at += Duration::nanoseconds(1),
            _ => changed.engine_versions.clear(),
        }
        assert!(manifest(&changed, &spec, now(), observed).is_err());
    }
}

#[test]
fn failure_keeps_actual_overage_and_does_not_publish_partial_scientific_outputs() {
    let spec = spec();
    let mut value = result(&spec);
    value.state = RuntimeResultState::Failed;
    value.error = Some(error(RuntimeFailureCode::MemoryLimit));
    value.artifacts.clear();
    value.resource_usage.output_bytes = DbCounter::ZERO;
    value.resource_usage.peak_memory_bytes = Some(count(512 * 1024 * 1024));
    manifest(&value, &spec, now(), now() + Duration::seconds(2)).unwrap();
    value.artifacts.push(result(&spec).artifacts.remove(0));
    assert!(manifest(&value, &spec, now(), now() + Duration::seconds(2)).is_err());
    value.artifacts.clear();
    value.error.as_mut().unwrap().safe_message = "native stderr must not escape".into();
    assert!(manifest(&value, &spec, now(), now() + Duration::seconds(2)).is_err());
}

#[test]
fn unknown_resource_measurements_are_not_invented_and_do_not_erase_payload_accounting() {
    let spec = spec();
    let mut value = result(&spec);
    value.resource_usage.cpu_nanoseconds = None;
    value.resource_usage.peak_memory_bytes = None;
    manifest(&value, &spec, now(), now() + Duration::seconds(2)).unwrap();
    let wire = serde_json::to_value(&value).unwrap();
    assert!(wire["resource_usage"]["cpu_nanoseconds"].is_null());
    assert_eq!(wire["resource_usage"]["output_bytes"], "128");
    value.resource_usage.output_bytes = DbCounter::ZERO;
    assert!(manifest(&value, &spec, now(), now() + Duration::seconds(2)).is_err());
}

#[test]
fn cancelled_identity_without_a_result_is_only_valid_when_execution_never_started() {
    let spec = spec();
    let mut value = RuntimeJobStatusV1 {
        schema_version: SchemaV1,
        run_id: spec.run_id,
        attempt_no: 1,
        external_job_id: spec.external_job_id.clone(),
        state: RuntimeJobState::Cancelled,
        has_result: false,
        submitted_at: now(),
        started_at: None,
        finished_at: Some(now()),
    };
    status(&value, spec.run_id, 1, now()).unwrap();
    value.started_at = Some(now());
    assert!(status(&value, spec.run_id, 1, now()).is_err());
    value.has_result = true;
    status(&value, spec.run_id, 1, now()).unwrap();
    value.state = RuntimeJobState::CancelRequested;
    assert!(status(&value, spec.run_id, 1, now()).is_err());
    value.finished_at = None;
    value.has_result = false;
    status(&value, spec.run_id, 1, now()).unwrap();
    assert!(status(&value, Id::new(), 1, now()).is_err());
}

#[test]
fn native_storage_versions_are_exact_single_header_values() {
    for valid in ["1", "native-UUID/version.3", "quoted-version_ABC"] {
        storage_version(valid).unwrap();
    }
    for invalid in ["", " ", "version 1", "a\rb", "a\nb", "\u{7f}", "原生"] {
        assert!(storage_version(invalid).is_err());
    }
    assert!(storage_version(&"v".repeat(121)).is_err());
    storage_version(&"v".repeat(120)).unwrap();
}
