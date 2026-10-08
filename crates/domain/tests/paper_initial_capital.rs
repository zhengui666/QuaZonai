//! Controlled model-capital conditions only; no initialization receipt or account execution.
#[path = "../../../tests/support/catalog_metadata.rs"]
mod metadata_fixture;
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio_fixture;
use contracts::{
    delivery::*,
    forward::{DownstreamWeightsSubmitV1, ForwardEnvironmentV1},
    portfolio::*,
    science::*,
    strategy_portfolio::{FreshPaperCashV1, TargetPackageVersionV2},
    DbCounter, Id, Revision, SchemaV1,
};
use serde_json::json;

fn example() -> NativePortfolioBuildRequestV1 {
    let input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let mut request = portfolio_fixture::request(&input);
    let root = PaperInitializationRefV1 {
        artifact_id: request.current_weights_artifact_id,
        downstream_id: Id::new(),
        trader_id: "TRADER-001".into(),
        account_id: "SIM-001".into(),
    };
    request.current_weights.paper_initialization = Some(root.clone());
    request.current_weights.source = PortfolioWeightsSourceV1::PaperInitialCapital {
        account_start: FreshPaperCashV1 {
            downstream_id: root.downstream_id,
            trader_id: root.trader_id,
            account_id: root.account_id,
            base_currency: request.execution_settings.base_currency.clone(),
            starting_capital: request.execution_settings.starting_capital.clone(),
            execution_assumptions_id: request.mandate.execution_assumptions_id,
        },
    };
    request.current_weights.asof_ns = request.selection.decision_cutoff_ns;
    request.current_weights.available_ns = request.selection.decision_cutoff_ns;
    request.current_weights.valid_until_ns = DbCounter::new(
        request.selection.decision_cutoff_ns.get()
            + u64::from(request.mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000,
    )
    .unwrap();
    request.current_weights.cash_weight = "1".parse().unwrap();
    for weight in &mut request.current_weights.weights {
        weight.weight = "0".parse().unwrap();
    }
    for asset in &mut request.assets {
        asset.current_weight = "0".parse().unwrap();
    }
    request
}

fn build(request: &NativePortfolioBuildRequestV1) -> PortfolioBuildRequestV1 {
    let root = request
        .current_weights
        .paper_initialization
        .as_ref()
        .unwrap();
    PortfolioBuildRequestV1 {
        schema_version: SchemaV1,
        cycle_id: Id::new(),
        mandate_id: Id::new(),
        input_set_id: Id::new(),
        runtime_id: Id::new(),
        expected_runtime_revision: Revision::INITIAL,
        current_weights_source: PortfolioBuildWeightsV1::PaperInitialCapital {
            downstream_id: root.downstream_id,
            trader_id: root.trader_id.clone(),
            account_id: root.account_id.clone(),
        },
        environment: ForwardEnvironmentV1::Paper,
        members: vec![
            PortfolioMemberSelectionV1 {
                qualification_id: Id::new(),
                ensemble_weight: "0.5".parse().unwrap(),
            },
            PortfolioMemberSelectionV1 {
                qualification_id: Id::new(),
                ensemble_weight: "0.5".parse().unwrap(),
            },
        ],
        limits: contracts::lifecycle::JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: None,
            wall_seconds: None,
            memory_mib: Some(128),
            output_bytes: None,
        },
    }
}

#[test]
fn initial_model_condition_is_self_identified_and_preserves_real_source_rules() {
    let request = example();
    domain::portfolio::paper_weights_source(&request).unwrap();
    domain::execution::portfolio_build_request(&request).unwrap();
    domain::portfolio::build_selection(&build(&request)).unwrap();
    assert_eq!(
        request.current_weights.asof_ns,
        request.selection.decision_cutoff_ns
    );
    assert_eq!(
        request.current_weights.available_ns,
        request.selection.decision_cutoff_ns
    );
    // There is no backdated receipt field: actual creation time stays in the immutable Store record.
    assert!(serde_json::to_value(&request.current_weights)
        .unwrap()
        .get("created_at")
        .is_none());
}

#[test]
fn missing_root_wrong_scope_capital_nonzero_holdings_and_extended_clocks_fail() {
    let original = example();
    for case in 0..18 {
        let mut changed = original.clone();
        match case {
            0 => changed.current_weights.paper_initialization = None,
            1 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .artifact_id = Id::new()
            }
            2 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .downstream_id = Id::new()
            }
            3 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .trader_id = "OTHER-001".into()
            }
            4 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .account_id = "OTHER-001".into()
            }
            5 => changed.execution_settings.starting_capital = "1".parse().unwrap(),
            6 => changed.mandate.capital_assumption = "1".parse().unwrap(),
            7 => changed.mandate.execution_assumptions_id = Id::new(),
            8 => changed.current_weights.base_currency = "USDT".into(),
            9 => changed.current_weights.cash_weight = "0.9".parse().unwrap(),
            10 => changed.current_weights.weights[0].weight = "0.1".parse().unwrap(),
            11 => changed.current_weights.asof_ns = DbCounter::ZERO,
            12 => changed.current_weights.available_ns = DbCounter::ZERO,
            13 => {
                changed.current_weights.valid_until_ns =
                    DbCounter::new(changed.current_weights.valid_until_ns.get() + 1).unwrap()
            }
            14 => {
                changed.current_weights.valid_until_ns =
                    DbCounter::new(changed.current_weights.valid_until_ns.get() - 1).unwrap()
            }
            15 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .trader_id = " TRADER-001".into()
            }
            16 => {
                changed
                    .current_weights
                    .paper_initialization
                    .as_mut()
                    .unwrap()
                    .account_id = "SIM-001 ".into()
            }
            _ => {
                let PortfolioWeightsSourceV1::PaperInitialCapital { account_start } =
                    &mut changed.current_weights.source
                else {
                    unreachable!()
                };
                account_start.starting_capital = "0".parse().unwrap();
            }
        }
        assert!(
            domain::portfolio::paper_weights_source(&changed).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn lineage_compares_original_root_and_downstream_without_mandate_or_session_reset_keys() {
    let request = example();
    let original = request
        .current_weights
        .paper_initialization
        .as_ref()
        .unwrap();
    domain::portfolio::paper_initialization_binding(original, original, original.downstream_id)
        .unwrap();
    for case in 0..4 {
        let mut changed = original.clone();
        match case {
            0 => changed.artifact_id = Id::new(),
            1 => changed.downstream_id = Id::new(),
            2 => changed.trader_id = "OTHER-001".into(),
            _ => changed.account_id = "OTHER-001".into(),
        }
        assert!(domain::portfolio::paper_initialization_binding(
            &changed,
            original,
            original.downstream_id
        )
        .is_err());
    }
    assert!(
        domain::portfolio::paper_initialization_binding(original, original, Id::new()).is_err()
    );
    let mut later = request.clone();
    later.current_weights.source = PortfolioWeightsSourceV1::LastTarget {
        candidate_id: Id::new(),
    };
    later.current_weights_artifact_id = Id::new();
    domain::portfolio::paper_weights_source(&later).unwrap();
    assert_eq!(
        later.current_weights.paper_initialization.as_ref(),
        Some(original)
    );
    later.current_weights.source = PortfolioWeightsSourceV1::ForwardSnapshot {
        downstream_id: Id::new(),
        external_message_id: "original".into(),
    };
    assert!(domain::portfolio::paper_weights_source(&later).is_err());
}

#[test]
fn live_initialization_and_live_rooted_snapshots_are_rejected_without_changing_snapshot_clocks() {
    let request = example();
    let mut intent = build(&request);
    intent.environment = ForwardEnvironmentV1::Live;
    assert!(domain::portfolio::build_selection(&intent).is_err());
    let mut snapshot:DownstreamWeightsSubmitV1=serde_json::from_value(json!({"schema_version":1,"project_id":Id::new(),"environment":"PAPER","external_message_id":"original",
        "paper_initialization":request.current_weights.paper_initialization,"asof_ns":"10","available_ns":"11","valid_until_ns":"12","base_currency":request.current_weights.base_currency,
        "cash_weight":"1","weights":request.current_weights.weights})).unwrap();
    domain::forward::weights(&snapshot).unwrap();
    assert_eq!(snapshot.asof_ns.get(), 10);
    assert_eq!(snapshot.available_ns.get(), 11);
    snapshot.environment = ForwardEnvironmentV1::Live;
    assert!(domain::forward::weights(&snapshot).is_err());
    snapshot.paper_initialization = None;
    domain::forward::weights(&snapshot).unwrap(); // Legacy real snapshot shape still follows its original rules.
}

#[test]
fn synthetic_forecast_funding_never_upgrades_market_provenance_or_becomes_live() {
    let frozen = example();
    let intent = build(&frozen);
    let mut metadata = metadata_fixture::metadata();
    metadata.partition = contracts::research::DataPartition::Forward;
    metadata.origin = contracts::research::DataOrigin::Real;
    metadata.pit_status = contracts::research::PitStatus::Verified;
    let ids = frozen
        .assets
        .iter()
        .map(|a| a.instrument_id.clone())
        .collect::<Vec<_>>();
    metadata.universe.instrument_definitions=ids.iter().map(|id|json!({"CurrencyPair":{"id":id,"ts_event":0,"ts_init":0,"price_increment":"0.01"}})).collect();
    let forward = domain::delivery::freeze_forward_dataset(
        Id::new(),
        Id::new(),
        &frozen.selection,
        &metadata,
        &ids,
    )
    .unwrap();
    let source = ForecastEvaluationSourceV2 {
        build_run_id: Id::new(),
        build_accepted_attempt_id: Id::new(),
        build_parameters_artifact_id: Id::new(),
        build_report_artifact_id: Id::new(),
        build_input_set_id: intent.input_set_id,
        build_environment: ForwardEnvironmentV1::Paper,
        forward_dataset_revision_id: forward.dataset_revision_id,
        forward_metadata_artifact_id: forward.native_metadata_artifact_id,
        current_weights_artifact_id: frozen.current_weights_artifact_id,
    };
    let refs = vec![
        source.build_parameters_artifact_id,
        source.build_report_artifact_id,
        source.forward_metadata_artifact_id,
        source.current_weights_artifact_id,
    ];
    let package = ForecastTargetPackageV2 {
        release_id: Id::new(),
        package_schema_version: TargetPackageVersionV2::V2,
        source_kind: ForecastReleaseSourceV2::ForecastEvaluation,
        source,
        forward_dataset: forward.clone(),
        environment_origin: PackageOriginV1::Synthetic,
        project_id: Id::new(),
        candidate_id: Id::new(),
        mandate_id: intent.mandate_id,
        qualification_refs: vec![Id::new(), Id::new()],
        evaluation_refs: vec![Id::new()],
        input_revision_refs: vec![forward.dataset_revision_id],
        engine_versions: Default::default(),
        asof: chrono::DateTime::from_timestamp(20, 0).unwrap(),
        valid_from: chrono::DateTime::from_timestamp(21, 0).unwrap(),
        valid_until: chrono::DateTime::from_timestamp(320, 0).unwrap(),
        base_currency: frozen.mandate.base_currency.clone(),
        capital_assumption: frozen.mandate.capital_assumption.clone(),
        current_weights_source: CandidateWeightsSourceV1::PaperInitialCapital,
        current_weights: frozen.current_weights.clone(),
        execution_settings: frozen.execution_settings.clone(),
        targets: frozen
            .current_weights
            .weights
            .iter()
            .map(|w| PackageTargetV1 {
                instrument_id: w.instrument_id.clone(),
                target_weight: w.weight.clone(),
                currency: w.currency.clone(),
            })
            .collect(),
        cash_weight: "1".parse().unwrap(),
        constraints_summary: frozen.mandate.constraints.clone(),
        exposure_tolerance: frozen.mandate.exposure_tolerance.clone(),
        cost_assumption_ref: frozen.mandate.execution_assumptions_id,
        compatible_market_capabilities: vec!["controlled".into()],
        limitations: vec![],
        provenance_artifact_refs: refs,
    };
    domain::delivery::forecast_source_binding(&package, &intent, &frozen, &forward).unwrap();
    for origin in [PackageOriginV1::Real, PackageOriginV1::Demo] {
        let mut changed = package.clone();
        changed.environment_origin = origin;
        assert!(
            domain::delivery::forecast_source_binding(&changed, &intent, &frozen, &forward)
                .is_err()
        );
    }
    let mut live = intent.clone();
    live.environment = ForwardEnvironmentV1::Live;
    assert!(domain::delivery::forecast_source_binding(&package, &live, &frozen, &forward).is_err());
    for case in 0..3 {
        let mut changed = forward.clone();
        match case {
            0 => changed.origin = contracts::research::DataOrigin::Synthetic,
            1 => changed.pit_status = contracts::research::PitStatus::Unverified,
            _ => changed.revision_policy = contracts::catalogs::DataRevisionPolicy::Restated,
        };
        let mut p = package.clone();
        p.forward_dataset = changed.clone();
        assert!(domain::delivery::forecast_source_binding(&p, &intent, &frozen, &changed).is_err());
    }
}
