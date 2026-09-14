//! Structural package checks, not production provenance or Release qualification.
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
    let command = contracts::control::OperatorCommand::ReleaseCreate(intent.clone());
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
    let package = TargetPackageV1 {
        release_id: Id::new(),
        package_schema_version: contracts::settings::PackageSchemaVersion::V1,
        environment_origin: PackageOriginV1::Demo,
        project_id: mandate.project_id,
        candidate_id: targets.candidate_id,
        mandate_id: mandate.id,
        qualification_refs: qualifications,
        evaluation_refs: vec![Id::new()],
        input_revision_refs: vec![Id::new()],
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
        provenance_artifact_refs: vec![Id::new()],
    };
    domain::delivery::target_package(&package, &targets, &mandate, &candidate).unwrap();
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
        assert!(serde_json::from_value::<TargetPackageV1>(value).is_err());
    }
}
