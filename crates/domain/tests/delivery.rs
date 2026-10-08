//! Structural package checks, not production provenance or Release qualification.
#[path = "../../../tests/support/catalog_metadata.rs"]
mod metadata_fixture;
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio_config;
use contracts::{delivery::*, portfolio::*, science::PortfolioTargetsV1, Id, SchemaV1};
use serde_json::json;

#[test]
fn release_creation_is_an_exact_candidate_command_not_a_package_upload() {
    let intent = ReleaseCreateV1 {
        schema_version: SchemaV1,
        candidate_id: Id::new(),
        evaluation_id: Id::new(),
    };
    let command = contracts::control::OperatorCommand::ReleaseCreate(
        contracts::strategy_portfolio::ReleaseCreateEnvelopeV2::Forecast(intent.clone()),
    );
    assert_eq!(command.operation().code(), "RELEASE_CREATE");
    assert!(!command.operation().creates()); // Grant targets the original Candidate.
    domain::control::command(&command).unwrap();
    for field in [
        "targets",
        "valid_until",
        "package_artifact_id",
        "environment",
        "force_pass",
    ] {
        let mut value = serde_json::to_value(&intent).unwrap();
        value[field] = json!(null);
        assert!(serde_json::from_value::<ReleaseCreateV1>(value).is_err());
    }
}

#[test]
fn package_preserves_original_targets_and_mandate_without_order_fields() {
    let input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let now = chrono::DateTime::from_timestamp(20, 0).unwrap();
    let until = now + chrono::Duration::seconds(300);
    let mandate = MandateViewV1 {
        id: Id::new(),
        project_id: Id::new(),
        version: 1,
        content: portfolio_config::request(&input).mandate,
        created_at: now,
    };
    let targets = PortfolioTargetsV1 {
        schema_version: SchemaV1,
        candidate_id: Id::new(),
        base_currency: "USD".into(),
        asof: now,
        valid_until: until,
        cash_weight: "0".parse().unwrap(),
        targets: input
            .assets
            .iter()
            .map(|a| AllocationTargetV1 {
                instrument_id: a.instrument_id.clone(),
                currency: a.currency.clone(),
                weight: a.current_weight.clone(),
            })
            .collect(),
    };
    let qualifications = vec![Id::new(), Id::new()];
    let candidate: CandidateDetailV1 = serde_json::from_value(json!({
        "header": {"id":targets.candidate_id,"project_id":mandate.project_id,"mandate_id":mandate.id,
            "input_set_id":Id::new(),"run_id":Id::new(),"decision_asof":now,"created_at":now,
            "execution_status":"SUCCEEDED","solver_status":"OPTIMAL","evidence_status":"VALID",
            "origin":"SYNTHETIC","reason_code":null,"forecast_artifact_id":null,"covariance_artifact_id":null,
            "diagnostics_artifact_id":Id::new(),"target_artifact_id":Id::new(),"allocation_evaluation_id":Id::new(),
            "cash_weight":"0","current_weights_source":"LAST_TARGET","current_weights_artifact_id":Id::new()},
        "members":qualifications.iter().map(|id| json!({"alpha_version_id":Id::new(),"qualification_id":id,"ensemble_weight":"0.5","calibration_id":null,"forecast_unit":"RETURN_PER_HORIZON","coverage_fraction":"1"})).collect::<Vec<_>>(),
        "targets":targets.targets.iter().map(|t| json!({"instrument_id":t.instrument_id,"target_weight":t.weight,"currency":t.currency,"asof":now,"valid_until":until})).collect::<Vec<_>>()
    })).unwrap();
    let mut frozen = portfolio_config::request(&input);
    frozen.current_weights_artifact_id = candidate.header.current_weights_artifact_id.unwrap();
    frozen.current_weights.valid_until_ns = contracts::DbCounter::new(320_000_000_000).unwrap();
    let dataset = Id::new();
    let metadata_id = Id::new();
    let mut metadata = metadata_fixture::metadata();
    metadata.partition = contracts::research::DataPartition::Forward;
    metadata.universe.instrument_definitions = input
        .assets
        .iter()
        .map(|a| {
            json!({"CurrencyPair":{
        "id":a.instrument_id,"ts_event":0,"ts_init":0,"price_increment":"0.01"}})
        })
        .collect();
    let ids = input
        .assets
        .iter()
        .map(|a| a.instrument_id.clone())
        .collect::<Vec<_>>();
    let forward_dataset = domain::delivery::freeze_forward_dataset(
        dataset,
        metadata_id,
        &frozen.selection,
        &metadata,
        &ids,
    )
    .unwrap();
    let source = ForecastEvaluationSourceV2 {
        build_run_id: candidate.header.run_id,
        build_accepted_attempt_id: Id::new(),
        build_parameters_artifact_id: Id::new(),
        build_report_artifact_id: Id::new(),
        build_input_set_id: candidate.header.input_set_id,
        build_environment: contracts::forward::ForwardEnvironmentV1::Live,
        forward_dataset_revision_id: dataset,
        forward_metadata_artifact_id: metadata_id,
        current_weights_artifact_id: frozen.current_weights_artifact_id,
    };
    let source_refs = vec![
        source.build_parameters_artifact_id,
        source.build_report_artifact_id,
        source.current_weights_artifact_id,
        metadata_id,
    ];
    let package = ForecastTargetPackageV2 {
        release_id: Id::new(),
        package_schema_version: contracts::strategy_portfolio::TargetPackageVersionV2::V2,
        source_kind: ForecastReleaseSourceV2::ForecastEvaluation,
        source,
        forward_dataset: forward_dataset.clone(),
        current_weights: frozen.current_weights.clone(),
        execution_settings: frozen.execution_settings.clone(),
        environment_origin: PackageOriginV1::Demo,
        project_id: mandate.project_id,
        candidate_id: targets.candidate_id,
        mandate_id: mandate.id,
        qualification_refs: qualifications,
        evaluation_refs: vec![Id::new()],
        input_revision_refs: vec![dataset],
        engine_versions: [("optimizer".into(), CLARABEL_VERSION.into())].into(),
        asof: now,
        valid_from: now,
        valid_until: until,
        base_currency: "USD".into(),
        capital_assumption: mandate.content.capital_assumption.clone(),
        current_weights_source: CandidateWeightsSourceV1::LastTarget,
        targets: targets
            .targets
            .iter()
            .map(|t| PackageTargetV1 {
                instrument_id: t.instrument_id.clone(),
                target_weight: t.weight.clone(),
                currency: t.currency.clone(),
            })
            .collect(),
        cash_weight: targets.cash_weight.clone(),
        constraints_summary: mandate.content.constraints.clone(),
        exposure_tolerance: mandate.content.exposure_tolerance.clone(),
        cost_assumption_ref: mandate.content.execution_assumptions_id,
        compatible_market_capabilities: vec!["synthetic-test/1".into()],
        limitations: vec!["Synthetic structural test only".into()],
        provenance_artifact_refs: source_refs,
    };
    domain::delivery::target_package(&package, &targets, &mandate, &candidate).unwrap();
    let contracts::science::PortfolioWeightsSourceV1::LastTarget {
        candidate_id: previous,
    } = frozen.current_weights.source
    else {
        unreachable!()
    };
    let build = PortfolioBuildRequestV1 {
        schema_version: SchemaV1,
        cycle_id: Id::new(),
        mandate_id: mandate.id,
        input_set_id: candidate.header.input_set_id,
        runtime_id: Id::new(),
        expected_runtime_revision: contracts::Revision::INITIAL,
        current_weights_source: PortfolioBuildWeightsV1::LastTarget {
            candidate_id: previous,
        },
        environment: contracts::forward::ForwardEnvironmentV1::Live,
        members: Vec::new(),
        limits: contracts::lifecycle::JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: None,
            wall_seconds: None,
            memory_mib: Some(128),
            output_bytes: None,
        },
    };
    domain::delivery::forecast_source_binding(&package, &build, &frozen, &forward_dataset).unwrap();
    for case in 0..10 {
        let mut changed = package.clone();
        match case {
            0 => changed.source.build_input_set_id = Id::new(),
            1 => changed.source.build_environment = contracts::forward::ForwardEnvironmentV1::Paper,
            2 => changed.source.forward_dataset_revision_id = Id::new(),
            3 => changed.source.current_weights_artifact_id = Id::new(),
            4 => changed.current_weights.cash_weight = "1".parse().unwrap(),
            5 => changed.execution_settings.fee_rates[0].taker = "0.2".parse().unwrap(),
            6 => changed.forward_dataset.instrument_definitions.clear(),
            7 => changed.source.forward_metadata_artifact_id = Id::new(),
            8 => changed.provenance_artifact_refs.pop().map(|_| ()).unwrap(),
            _ => changed.valid_until += chrono::Duration::nanoseconds(1),
        }
        assert!(
            domain::delivery::forecast_source_binding(&changed, &build, &frozen, &forward_dataset)
                .is_err(),
            "source case {case}"
        );
    }
    // The source retains LIVE as a research fact; no Handoff environment enters this validator.
    assert_eq!(
        package.source.build_environment,
        contracts::forward::ForwardEnvironmentV1::Live
    );
    let mut reordered = candidate.clone();
    reordered.targets.reverse();
    domain::delivery::target_package(&package, &targets, &mandate, &reordered).unwrap();
    reordered.targets[0].target_weight = "0.4".parse().unwrap();
    assert!(domain::delivery::target_package(&package, &targets, &mandate, &reordered).is_err());
    for case in 0..14 {
        let mut wrong = package.clone();
        match case {
            0 => wrong.project_id = Id::new(),
            1 => wrong.mandate_id = Id::new(),
            2 => wrong.capital_assumption = "1".parse().unwrap(),
            3 => wrong.exposure_tolerance = "0.001".parse().unwrap(),
            4 => wrong.constraints_summary.max_asset_weight = "0.75".parse().unwrap(),
            5 => wrong.cost_assumption_ref = Id::new(),
            6 => wrong.current_weights_source = CandidateWeightsSourceV1::None,
            7 => wrong.qualification_refs.reverse(),
            8 => wrong.targets.reverse(),
            9 => wrong.targets[0].target_weight = "0.500000000000000001".parse().unwrap(),
            10 => wrong.valid_until += chrono::Duration::seconds(1),
            11 => wrong.evaluation_refs.push(wrong.evaluation_refs[0]),
            12 => wrong.engine_versions.clear(),
            _ => wrong.cash_weight = "0.000000000000000001".parse().unwrap(),
        }
        assert!(
            domain::delivery::target_package(&wrong, &targets, &mandate, &candidate).is_err(),
            "case {case}"
        );
    }
    for field in ["quantity", "order_type", "broker_credentials"] {
        let mut value = serde_json::to_value(&package).unwrap();
        value["targets"][0][field] = json!("forbidden-placeholder");
        assert!(serde_json::from_value::<ForecastTargetPackageV2>(value).is_err());
    }
}

#[test]
fn release_decisions_require_bounded_reasons_and_exact_wire_fields() {
    use contracts::{
        control::OperatorCommand,
        delivery::{ReleaseRejectV1, ReleaseReopenV1},
        forward::ForwardEnvironmentV1,
    };
    let r = ReleaseRejectV1 {
        schema_version: SchemaV1,
        downstream_id: Id::new(),
        environment: ForwardEnvironmentV1::Paper,
        expected_latest_decision_id: None,
        reason_code: "REJECTED".into(),
        reason: "User decision".into(),
    };
    domain::control::command(&OperatorCommand::ReleaseReject(r.clone())).unwrap();
    let reopen = ReleaseReopenV1 {
        schema_version: SchemaV1,
        expected_latest_decision_id: Id::new(),
        reason_code: "RECONSIDERED".into(),
        reason: "User reconsideration".into(),
    };
    domain::control::command(&OperatorCommand::ReleaseReopen(reopen)).unwrap();
    for (code, reason) in [
        ("".to_owned(), "reason".to_owned()),
        ("a".repeat(121), "reason".into()),
        ("CODE".into(), " ".into()),
        ("CODE".into(), "a".repeat(2001)),
    ] {
        assert!(domain::delivery::decision_reason(&code, &reason).is_err());
    }
    let mut raw = serde_json::to_value(r).unwrap();
    raw["approval_id"] = json!(Id::new());
    assert!(serde_json::from_value::<ReleaseRejectV1>(raw).is_err());
}

#[test]
fn new_downstream_capabilities_reject_historical_versions() {
    let now = chrono::Utc::now();
    let mut capability = DownstreamCapabilitiesV1 {
        schema_version: SchemaV1,
        delivery_mode: DownstreamDeliveryModeV1::TargetOnly,
        accepted_package_versions: vec![
            contracts::settings::PackageSchemaVersion::V1,
            contracts::settings::PackageSchemaVersion::V2,
        ],
        environments: vec![contracts::forward::ForwardEnvironmentV1::Paper],
        market_capability_versions: vec!["native-paper/1".into()],
        accepting_targets: true,
        checked_at: now,
    };
    assert!(domain::delivery::downstream_capabilities(&capability, now).is_err());
    capability.accepted_package_versions = vec![contracts::settings::PackageSchemaVersion::V2];
    assert!(domain::delivery::downstream_capabilities(&capability, now).is_ok());
    capability
        .accepted_package_versions
        .push(contracts::settings::PackageSchemaVersion::V2);
    assert!(domain::delivery::downstream_capabilities(&capability, now).is_err());
    capability.accepted_package_versions.clear();
    assert!(domain::delivery::downstream_capabilities(&capability, now).is_err());
}

#[test]
fn strategy_release_uses_existing_operator_authority_without_fake_evaluation() {
    let candidate = Id::new();
    let request = json!({"operation":"RELEASE_CREATE", "request":{
        "schema_version":1,"source_kind":"NATIVE_TARGET_DECISION","candidate_id":candidate
    }});
    let command: contracts::control::OperatorCommand =
        serde_json::from_value(request.clone()).unwrap();
    assert_eq!(command.operation().code(), "RELEASE_CREATE");
    assert!(!command.operation().creates());
    domain::control::command(&command).unwrap();
    assert_eq!(serde_json::to_value(&command).unwrap(), request);
    assert!(command
        .normalized_request()
        .unwrap()
        .get("evaluation_id")
        .is_none());
}

#[test]
fn forward_projection_keeps_complete_required_definition_histories_and_no_locators() {
    let mut metadata = metadata_fixture::metadata();
    metadata.partition = contracts::research::DataPartition::Forward;
    let first = metadata.universe.instrument_definitions[0].clone();
    let mut second = first.clone();
    second["CurrencyPair"]["ts_event"] = json!(1);
    second["CurrencyPair"]["ts_init"] = json!(1);
    second["CurrencyPair"]["price_increment"] = json!("0.0001");
    let mut closing = first.clone();
    closing["CurrencyPair"]["id"] = json!("CLOSING.SIM");
    let mut unrelated = first.clone();
    unrelated["CurrencyPair"]["id"] = json!("UNRELATED.SIM");
    metadata.universe.instrument_definitions =
        vec![first.clone(), second.clone(), closing.clone(), unrelated];
    let ids = vec!["EUR/USD.SIM".to_owned(), "CLOSING.SIM".to_owned()];
    let selection = &metadata.quality.datasets[0].selection;
    let projection =
        domain::delivery::freeze_forward_dataset(Id::new(), Id::new(), selection, &metadata, &ids)
            .unwrap();
    assert_eq!(
        projection.instrument_definitions,
        vec![first, second, closing]
    );
    assert_eq!(projection.origin, metadata.origin);
    assert_eq!(projection.pit_status, metadata.pit_status);
    let wire = serde_json::to_value(&projection).unwrap();
    for field in [
        "registered_ref",
        "native_snapshot_ref",
        "quality",
        "provenance_reference",
        "recorded_feature_inputs",
    ] {
        assert!(wire.get(field).is_none(), "{field}");
    }
    assert!(domain::delivery::freeze_forward_dataset(
        Id::new(),
        Id::new(),
        selection,
        &metadata,
        &["MISSING.SIM".into()]
    )
    .is_err());
    assert!(domain::delivery::freeze_forward_dataset(
        Id::new(),
        Id::new(),
        selection,
        &metadata,
        &[]
    )
    .is_err());
}
