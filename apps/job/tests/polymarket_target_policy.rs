//! Synthetic native adapter acceptance only, never production PIT/performance evidence.
#[path = "../../../tests/support/execution_models.rs"]
mod execution_models;
#[path = "support/experiment.rs"]
mod experiment;
#[path = "support/polymarket.rs"]
mod prediction;
#[path = "../../../tests/support/strategy_composition.rs"]
mod strategy_fixture;
use contracts::{science::*, strategy_portfolio::*, Id, SchemaV1};
use std::collections::BTreeMap;

fn fixture(
    rows: u32,
    expiration: u64,
) -> (
    tempfile::TempDir,
    NativeExperimentEvaluationRequestV1,
    Vec<FeatureObservationsV1>,
) {
    let (_, mut request, parts) = experiment::fixture(false);
    let root = tempfile::tempdir().unwrap();
    prediction::write_catalog(root.path(), rows, "0.02", expiration, false);
    prediction::settings(&mut request.settings, "0.02", expiration);
    request.settings.fee_rates.truncate(1);
    // Native BAR execution arrives on a later minute, so this synthetic policy
    // explicitly grants two minutes rather than reusing the seconds fixture TTL.
    request.target_ttl_ns = experiment::count(2 * prediction::STEP);
    request.instrument_id = prediction::IDS[0].into();
    request.selection.bar_types = vec![format!("{}-1-MINUTE-LAST-EXTERNAL", request.instrument_id)];
    request.selection.event_end_ns = experiment::count((u64::from(rows) + 1) * prediction::STEP);
    request.selection.decision_cutoff_ns = request.selection.event_end_ns;
    let definitions = prediction::instruments("0.02", expiration)
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    request.binary_option = domain::prediction::binary_option_context(
        &definitions,
        &[],
        &request.selection,
        &request.instrument_id,
        &request.settings,
    )
    .unwrap();
    (root, request, parts)
}

fn evaluate(
    root: &std::path::Path,
    request: &NativeExperimentEvaluationRequestV1,
    parts: &[FeatureObservationsV1],
) -> anyhow::Result<NativeExperimentEvaluationResultV1> {
    let data = Id::new();
    let model = Id::new();
    let ids = vec![Id::new()];
    let wasm = experiment::module("", "", "f64.const 0.2");
    let result = job::experiment::evaluate(root, request, data, model, &ids, parts, &wasm)?;
    domain::execution::check_experiment_evaluation(request, data, model, &ids, &result)?;
    Ok(result)
}

#[test]
fn native_cash_binary_policy_and_historical_strategy_reuse_original_replay() {
    let (root, request, parts) = fixture(40, 100 * prediction::STEP);
    let report = evaluate(root.path(), &request, &parts).unwrap();
    assert!(!report.folds.is_empty());
    assert!(report
        .folds
        .iter()
        .all(|fold| fold.simulation_request.settlements.is_empty()));
    let policy = strategy_fixture::policy(&report);
    let report_id = policy.source.report_artifact_id;
    let mut composition = NativeStrategyCompositionRequestV1 {
        binary_option: request.binary_option.clone(),
        schema_version: SchemaV1,
        selection: report.folds[0].simulation_request.selection.clone(),
        mandate: strategy_fixture::mandate(&request.settings),
        settings: request.settings.clone(),
        purpose: StrategyPortfolioPurposeV1::HistoricalReplay {},
        input_provenance: StrategyInputProvenanceV1 {
            dataset_revision_id: report.dataset_revision_id,
            market_data_origin: contracts::research::DataOrigin::Synthetic,
            pit_status: contracts::research::PitStatus::Unverified,
            revision_policy: contracts::catalogs::DataRevisionPolicy::Unknown,
            feature_artifact_origins: BTreeMap::from([(
                report.feature_artifact_ids[0],
                contracts::research::DataOrigin::Synthetic,
            )]),
            feature_source_bindings: None,
        },
        members: vec![NativeStrategyMemberV1 {
            alpha_id: Id::new(),
            alpha_version_id: Id::new(),
            ensemble_weight: "1".parse().unwrap(),
            feature_artifact_ids: report.feature_artifact_ids.clone(),
            policy,
        }],
        total_fuel: Some(experiment::count(1_000_000)),
    };
    composition.mandate.target_ttl_seconds = 120;
    let result = job::strategy::compose(root.path(), &composition, |id| {
        assert_eq!(id, report_id);
        Ok(serde_json::to_vec(&report).unwrap())
    })
    .unwrap();
    let StrategyCompositionOutcomeV1::HistoricalReplay {
        simulation_request,
        simulation,
    } = &result.outcome
    else {
        panic!()
    };
    let direct = job::simulation::simulate(root.path(), simulation_request).unwrap();
    for field in ["accounts", "fills", "positions", "portfolio_snapshots"] {
        assert_eq!(
            simulation.canonical_result[field],
            direct.canonical_result[field]
        );
    }
    let next = Id::new();
    let mut extension = strategy_fixture::new_observations(&request.feature_schema);
    extension.observations[0].event_ns = experiment::count(20 * prediction::STEP);
    extension.observations[0].observed_available_ns =
        Some(experiment::count(20 * prediction::STEP + 1));
    composition.members[0].feature_artifact_ids.push(next);
    composition
        .input_provenance
        .feature_artifact_origins
        .insert(next, contracts::research::DataOrigin::Synthetic);
    composition.selection = request.selection.clone();
    composition.purpose = StrategyPortfolioPurposeV1::CurrentDecision {
        account_start: FreshPaperCashV1 {
            downstream_id: Id::new(),
            trader_id: "TRADER-001".into(),
            account_id: "SIM-001".into(),
            base_currency: "pUSD".into(),
            starting_capital: request.settings.starting_capital.clone(),
            execution_assumptions_id: composition.mandate.execution_assumptions_id,
        },
        member_inputs: vec![StrategyCurrentInputsV1 {
            alpha_version_id: composition.members[0].alpha_version_id,
            feature_artifact_ids: vec![next],
        }],
    };
    let objects = BTreeMap::from([
        (report_id, serde_json::to_vec(&report).unwrap()),
        (
            report.model_artifact_id,
            experiment::module("", "", "f64.const 0.2"),
        ),
        (
            report.feature_artifact_ids[0],
            serde_json::to_vec(&parts[0]).unwrap(),
        ),
        (next, serde_json::to_vec(&extension).unwrap()),
    ]);
    let current = job::strategy::compose(root.path(), &composition, |id| {
        Ok(objects.get(&id).unwrap().clone())
    })
    .unwrap();
    let StrategyCompositionOutcomeV1::CurrentDecision { target, .. } = &current.outcome else {
        panic!()
    };
    assert_eq!(target.asof_ns, experiment::count(40 * prediction::STEP + 1));
    assert_eq!(target.targets[0].weight, "0.2".parse().unwrap());
    domain::execution::strategy::result(&composition, &current).unwrap();
    composition.binary_option = None;
    assert!(
        job::strategy::compose(root.path(), &composition, |_| Ok(serde_json::to_vec(
            &report
        )
        .unwrap()))
        .is_err()
    );
}

#[test]
fn missing_wrong_or_altered_native_context_cannot_downgrade() {
    let (root, request, parts) = fixture(40, 100 * prediction::STEP);
    let mut invalid = request.clone();
    invalid.binary_option = None;
    assert!(evaluate(root.path(), &invalid, &parts)
        .unwrap_err()
        .to_string()
        .contains("polymarket_target_context_missing"));
    let mut invalid = request.clone();
    invalid.settings.fee_model = execution_models::fee();
    assert!(evaluate(root.path(), &invalid, &parts).is_err());
    let mut invalid = request.clone();
    invalid.settings.account_kind = NativeAccountKind::Margin;
    assert!(evaluate(root.path(), &invalid, &parts).is_err());
    let mut invalid = request.clone();
    invalid
        .binary_option
        .as_mut()
        .unwrap()
        .instrument_definitions[0]["BinaryOption"]["expiration_ns"] =
        serde_json::json!(101 * prediction::STEP);
    assert!(evaluate(root.path(), &invalid, &parts)
        .unwrap_err()
        .to_string()
        .contains("NATIVE_SOURCE_MISMATCH"));
    let mut invalid = request.clone();
    invalid.target_ttl_ns = experiment::count(1000 * prediction::STEP);
    assert!(evaluate(root.path(), &invalid, &parts)
        .unwrap_err()
        .to_string()
        .contains("polymarket_target_lifetime"));
}

#[test]
fn future_native_resolution_is_not_leaked_into_earlier_folds() {
    let expiration = 45 * prediction::STEP;
    let (root, mut request, parts) = fixture(40, expiration);
    let available = expiration + 1;
    prediction::settle(root.path(), expiration, available, ["0.5000", "0.5000"]);
    request.selection.decision_cutoff_ns = experiment::count(available + 1);
    request.binary_option.as_mut().unwrap().settlements =
        prediction::settlement_groups(expiration, available, ["0.5", "0.5"]);
    let report = evaluate(root.path(), &request, &parts).unwrap();
    assert_eq!(
        report.request.binary_option.as_ref().unwrap().settlements[0]
            .outcomes
            .len(),
        2
    );
    assert!(report
        .folds
        .iter()
        .all(|f| f.simulation_request.settlements.is_empty()));
    let mut forged = report.clone();
    forged.folds[0].simulation_request.settlements =
        request.binary_option.as_ref().unwrap().settlements.clone();
    assert!(domain::execution::check_experiment_evaluation(
        &request,
        report.dataset_revision_id,
        report.model_artifact_id,
        &report.feature_artifact_ids,
        &forged
    )
    .is_err());
}

#[test]
fn crossing_expiry_without_a_close_in_the_original_fold_remains_unsupported() {
    let expiration = 8 * prediction::STEP;
    let (root, mut request, parts) = fixture(8, expiration);
    request.target_ttl_ns = experiment::count(10 * experiment::SECOND);
    request.label_horizon_observations = 1;
    request.split_policy.label_horizon_observations = Some(experiment::count(1));
    request.split_policy.train_size = experiment::count(4);
    request.split_policy.test_size = experiment::count(2);
    request.split_policy.step_size = Some(experiment::count(2));
    request.split_policy.purge_observations = experiment::count(1);
    request.split_policy.embargo_observations = experiment::count(0);
    let error = evaluate(root.path(), &request, &parts)
        .unwrap_err()
        .to_string();
    assert!(error.contains("POLYMARKET_PENDING_RESOLUTION"), "{error}");
}

#[test]
fn old_optional_context_wire_bytes_and_exact_native_lifetime_boundaries() {
    let (_, old, _) = experiment::fixture(false);
    let bytes = serde_json::to_vec(&old).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("binary_option"));
    let decoded: NativeExperimentEvaluationRequestV1 = serde_json::from_slice(&bytes).unwrap();
    assert!(decoded.binary_option.is_none());
    assert_eq!(bytes, serde_json::to_vec(&decoded).unwrap());
    let (_, request, _) = fixture(40, 100 * prediction::STEP);
    let context = request.binary_option.as_ref().unwrap();
    let expiry = 100 * prediction::STEP;
    let mut point = NativeTargetPointV1 {
        schema_version: SchemaV1,
        asof_ns: experiment::count(expiry - 1),
        valid_until_ns: experiment::count(expiry),
        targets: vec![contracts::portfolio::AllocationTargetV1 {
            instrument_id: request.instrument_id,
            currency: "pUSD".into(),
            weight: "0.2".parse().unwrap(),
        }],
        cash_weight: "0.8".parse().unwrap(),
    };
    domain::prediction::binary_option_target(Some(context), &point).unwrap();
    point.valid_until_ns = experiment::count(expiry + 1);
    assert!(domain::prediction::binary_option_target(Some(context), &point).is_err());
    point.asof_ns = experiment::count(expiry);
    assert!(domain::prediction::binary_option_target(Some(context), &point).is_err());
    assert!(
        domain::prediction::binary_option_source(None, Some(context), &request.selection).is_err()
    );
}

#[test]
fn accepted_current_result_cannot_hide_an_expired_member_behind_shorter_mandate_ttl() {
    let mut report = strategy_fixture::current_report();
    let expiry = 50 * experiment::SECOND;
    prediction::settings(&mut report.request.settings, "0.02", expiry);
    report.request.settings.fee_rates.truncate(1);
    report.request.selection.bar_types =
        vec![format!("{}-1-SECOND-LAST-EXTERNAL", prediction::IDS[0])];
    report.request.mandate.base_currency = "pUSD".into();
    report.request.members[0].policy.base_currency = "pUSD".into();
    report.request.members[0].policy.instrument_id = prediction::IDS[0].into();
    let StrategyPortfolioPurposeV1::CurrentDecision { account_start, .. } =
        &mut report.request.purpose
    else {
        panic!()
    };
    account_start.base_currency = "pUSD".into();
    let StrategyCompositionOutcomeV1::CurrentDecision {
        account_start,
        target,
        ..
    } = &mut report.outcome
    else {
        panic!()
    };
    account_start.base_currency = "pUSD".into();
    target.targets[0].instrument_id = prediction::IDS[0].into();
    target.targets[0].currency = "pUSD".into();
    target.valid_until_ns = experiment::count(target.asof_ns.get() + experiment::SECOND);
    let definitions = prediction::instruments("0.02", expiry)
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .unwrap();
    report.request.binary_option = domain::prediction::binary_option_context(
        &definitions,
        &[],
        &report.request.selection,
        prediction::IDS[0],
        &report.request.settings,
    )
    .unwrap();
    domain::execution::strategy::request(&report.request).unwrap();
    assert!(
        domain::execution::strategy::result(&report.request, &report)
            .unwrap_err()
            .to_string()
            .contains("polymarket_target_lifetime")
    );
    report.request.members[0].policy.target_ttl_ns = experiment::count(experiment::SECOND);
    domain::execution::strategy::result(&report.request, &report).unwrap();
    // The native close can precede the definition's expiration. The last BAR
    // is earlier than this close, but a current publisher already knows it.
    report.request.binary_option.as_mut().unwrap().settlements = prediction::settlement_groups(
        42 * experiment::SECOND,
        43 * experiment::SECOND + experiment::SECOND / 2,
        ["1", "0"],
    );
    domain::execution::strategy::request(&report.request).unwrap();
    let StrategyCompositionOutcomeV1::CurrentDecision { target, .. } = &report.outcome else {
        panic!()
    };
    domain::prediction::binary_option_target(report.request.binary_option.as_ref(), target)
        .unwrap();
    assert!(
        domain::execution::strategy::result(&report.request, &report)
            .unwrap_err()
            .to_string()
            .contains("polymarket_target_already_closed")
    );
    report
        .request
        .binary_option
        .as_mut()
        .unwrap()
        .settlements
        .clear();
    report.request.members[0].policy.target_ttl_ns = experiment::count(i64::MAX as u64);
    assert!(domain::execution::strategy::result(&report.request, &report).is_err());
}

#[test]
fn legacy_wire_and_specific_runtime_capability_cannot_be_substituted_by_venue() {
    let old = strategy_fixture::current_report();
    let bytes = serde_json::to_vec(&old).unwrap();
    assert!(!String::from_utf8_lossy(&bytes).contains("binary_option"));
    let decoded: NativeStrategyCompositionResultV1 = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(bytes, serde_json::to_vec(&decoded).unwrap());
    let (_, request, _) = fixture(40, 100 * prediction::STEP);
    let mut capabilities: contracts::runtime::RuntimeCapabilitiesV1 = serde_json::from_str(
        include_str!("../../../tests/contracts/runtime-capabilities.fixture.json"),
    )
    .unwrap();
    capabilities
        .engine_versions
        .insert("polymarket-research".into(), "1".into());
    domain::prediction::binary_option_capability(None, &capabilities).unwrap();
    assert!(domain::prediction::binary_option_capability(
        request.binary_option.as_ref(),
        &capabilities
    )
    .is_err());
    capabilities.engine_versions.insert(
        contracts::settlement::BINARY_OPTION_V2_CAPABILITY.into(),
        "0".into(),
    );
    assert!(domain::prediction::binary_option_capability(
        request.binary_option.as_ref(),
        &capabilities
    )
    .is_err());
    capabilities.engine_versions.insert(
        contracts::settlement::BINARY_OPTION_V2_CAPABILITY.into(),
        "1".into(),
    );
    domain::prediction::binary_option_capability(request.binary_option.as_ref(), &capabilities)
        .unwrap();
}
