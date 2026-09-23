//! Profile rules are configuration validation, not proof of native inference.
use chrono::{Duration, Utc};
use contracts::{codex::*, Id, Revision, SchemaV1};
use domain::codex::settings::*;

fn defaults() -> SavedModelSettingsV1 {
    SavedModelSettingsV1 {
        schema_version: SchemaV1,
        use_default_model_settings: true,
        saved_model: Some("saved-but-not-advertised".into()),
        saved_reasoning_effort: Some("retained-not-active".into()),
        saved_fast_mode: true,
    }
}
fn profile() -> CodexProfileCreateV1 {
    CodexProfileCreateV1 {
        schema_version: SchemaV1,
        name: "Native system".into(),
        home_binding: "operator.native-1".into(),
        profile_origin: ProfileOrigin::OperatorMount,
        connection: CodexConnectionCreateV1::System {},
        model_settings: defaults(),
    }
}
fn model(at: chrono::DateTime<Utc>) -> CodexAdvertisedModelV1 {
    CodexAdvertisedModelV1 {
        capability: ModelCapabilityV1 {
            schema_version: SchemaV1,
            id: "native-catalog-id".into(),
            model: "observed-model".into(),
            display_name: "Actual catalog model".into(),
            hidden: false,
            default_reasoning_effort: "native-default".into(),
            supported_reasoning_efforts: vec![ReasoningEffortCapability {
                reasoning_effort: "native-default".into(),
                description: "From controlled native-shaped fixture".into(),
            }],
            is_default: false,
            fetched_at: at,
            profile_revision: Revision::INITIAL,
        },
        service_tiers: vec![CodexServiceTierV1 {
            id: "priority".into(),
            name: "Native tier".into(),
            description: String::new(),
        }],
        default_service_tier: None,
    }
}
fn available(at: chrono::DateTime<Utc>) -> CodexProbeOutcomeV1 {
    CodexProbeOutcomeV1::Available {
        native_version: "0.156.1".into(),
        account: CodexAccountV1 {
            requires_openai_auth: false,
            authentication_kind: None,
            plan_type: None,
        },
        effective: CodexEffectiveSettingsV1 {
            model: "observed-model".into(),
            provider: "native_provider".into(),
            reasoning_effort: Some("native-default".into()),
            service_tier: None,
        },
        native_default_model: Some("observed-model".into()),
        models: vec![model(at)],
    }
}

#[test]
fn profile_references_are_labels_and_system_cannot_carry_a_custom_credential() {
    assert!(profile_create(&profile()).is_ok());
    for bad in [
        "",
        ".",
        "..",
        "../native",
        "/operator/home",
        "native/home",
        "native home",
        "native\n",
        "账户",
        "bad:key",
    ] {
        assert!(home_binding(bad).is_err(), "bad reference was admitted");
    }
    assert!(home_binding(&"x".repeat(64)).is_ok());
    assert!(home_binding(&"x".repeat(65)).is_err());
    let mut request = serde_json::to_value(profile()).unwrap();
    request["connection"]["credential_ref"] = serde_json::json!(Id::new());
    assert!(serde_json::from_value::<CodexProfileCreateV1>(request).is_err());
}

#[test]
fn custom_provider_and_connection_updates_are_rejected_at_the_wire_boundary() {
    let mut legacy = serde_json::to_value(profile()).unwrap();
    legacy["connection"] = serde_json::json!({
        "mode": "CUSTOM_PROVIDER", "base_url": "https://provider.invalid/v1", "credential_ref": Id::new()
    });
    assert!(serde_json::from_value::<CodexProfileCreateV1>(legacy).is_err());
    let update = CodexProfileUpdateV1 {
        schema_version: SchemaV1,
        expected_revision: Revision::INITIAL,
        model_settings: defaults(),
    };
    assert!(profile_update(&update).is_ok());
    for (key, value) in [
        ("connection", serde_json::json!({"mode":"SYSTEM"})),
        ("home_binding", serde_json::json!("other-home")),
        ("name", serde_json::json!("other-name")),
    ] {
        let mut wire = serde_json::to_value(&update).unwrap();
        wire[key] = value;
        assert!(serde_json::from_value::<CodexProfileUpdateV1>(wire).is_err());
    }
    for invalid in ["", " ", "small\n", "a\0b"] {
        let mut invalid_request = profile();
        invalid_request.model_settings.saved_reasoning_effort = Some(invalid.into());
        assert!(profile_create(&invalid_request).is_err());
    }
}

#[test]
fn default_mode_retains_dormant_values_and_never_confuses_catalog_default_with_effective_model() {
    let at = Utc::now();
    assert!(probe_outcome(
        &available(at),
        &defaults(),
        ConnectionMode::System,
        Revision::INITIAL,
        at,
        at
    )
    .is_ok());
    let mut future = available(at);
    let CodexProbeOutcomeV1::Available { native_version, .. } = &mut future else {
        unreachable!()
    };
    *native_version = "0.999.0-alpha.9+local".into();
    assert!(probe_outcome(
        &future,
        &defaults(),
        ConnectionMode::System,
        Revision::INITIAL,
        at,
        at
    )
    .is_ok());
    let mut explicit = defaults();
    explicit.use_default_model_settings = false;
    assert!(probe_outcome(
        &available(at),
        &explicit,
        ConnectionMode::System,
        Revision::INITIAL,
        at,
        at
    )
    .is_err());
    explicit.saved_model = None;
    explicit.saved_reasoning_effort = Some("native-default".into());
    explicit.saved_fast_mode = false;
    assert!(probe_outcome(
        &available(at),
        &explicit,
        ConnectionMode::System,
        Revision::INITIAL,
        at,
        at
    )
    .is_ok());
}

#[test]
fn incomplete_wrong_revision_stale_duplicate_or_unadvertised_catalogs_cannot_report_available() {
    let at = Utc::now();
    for dimension in 0..12 {
        let mut invalid = available(at);
        let CodexProbeOutcomeV1::Available {
            native_version,
            account,
            models,
            effective,
            ..
        } = &mut invalid
        else {
            unreachable!()
        };
        match dimension {
            0 => models.clear(),
            1 => models.push(models[0].clone()),
            2 => models[0].capability.profile_revision = Revision::INITIAL.next().unwrap(),
            3 => models[0].capability.fetched_at = at - Duration::seconds(6),
            4 => models[0].capability.fetched_at = at + Duration::seconds(6),
            5 => models[0].capability.supported_reasoning_efforts.clear(),
            6 => effective.model = "not-advertised".into(),
            7 => effective.reasoning_effort = Some("not-advertised".into()),
            8 => effective.service_tier = Some("not-advertised".into()),
            9 => account.requires_openai_auth = true,
            10 => *native_version = "0.156.1/other".into(),
            _ => account.plan_type = Some("account-does-not-have-chatgpt-auth".into()),
        }
        assert!(
            probe_outcome(
                &invalid,
                &defaults(),
                ConnectionMode::System,
                Revision::INITIAL,
                at,
                at
            )
            .is_err(),
            "catalog dimension {dimension}"
        );
    }
}

#[test]
fn fast_tier_requires_exact_native_advertisement_and_custom_cannot_fall_back_to_system() {
    let at = Utc::now();
    let mut advertised = model(at);
    assert_eq!(fast_tier(&advertised).unwrap(), "priority");
    advertised.service_tiers[0].id = "priority-like".into();
    assert!(fast_tier(&advertised).is_err());
    assert!(probe_outcome(
        &available(at),
        &defaults(),
        ConnectionMode::CustomProvider,
        Revision::INITIAL,
        at,
        at
    )
    .is_err());
    let mut saved = defaults();
    saved.use_default_model_settings = false;
    saved.saved_model = None;
    saved.saved_reasoning_effort = None;
    let mut observation = available(at);
    assert!(probe_outcome(
        &observation,
        &saved,
        ConnectionMode::System,
        Revision::INITIAL,
        at,
        at
    )
    .is_err());
    if let CodexProbeOutcomeV1::Available { effective, .. } = &mut observation {
        effective.service_tier = Some("priority".into());
    }
    assert!(probe_outcome(
        &observation,
        &saved,
        ConnectionMode::System,
        Revision::INITIAL,
        at,
        at
    )
    .is_ok());
}

#[test]
fn native_default_provenance_is_required_for_new_probes_but_old_records_remain_readable() {
    let at = Utc::now();
    let mut wire = serde_json::to_value(available(at)).unwrap();
    wire.as_object_mut().unwrap().remove("native_default_model");
    let historical: CodexProbeOutcomeV1 = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(&historical).unwrap(), wire);
    assert!(probe_outcome(
        &historical,
        &defaults(),
        ConnectionMode::System,
        Revision::INITIAL,
        at,
        at
    )
    .is_err());
    for name in ["", " ", "not-the-inherited-model"] {
        wire["native_default_model"] = serde_json::json!(name);
        let invalid: CodexProbeOutcomeV1 = serde_json::from_value(wire.clone()).unwrap();
        assert!(probe_outcome(
            &invalid,
            &defaults(),
            ConnectionMode::System,
            Revision::INITIAL,
            at,
            at
        )
        .is_err());
    }
    // A genuinely active model can differ from the native default. This does not
    // grant the inherited model any capabilities or change the catalog marker.
    let mut explicit = defaults();
    explicit.use_default_model_settings = false;
    explicit.saved_model = Some("observed-model".into());
    explicit.saved_reasoning_effort = None;
    explicit.saved_fast_mode = false;
    let active: CodexProbeOutcomeV1 = serde_json::from_value(wire).unwrap();
    assert!(probe_outcome(
        &active,
        &explicit,
        ConnectionMode::System,
        Revision::INITIAL,
        at,
        at
    )
    .is_ok());
}
