//! Native FeatureReplay/WASMI and Nautilus account acceptance over controlled
//! synthetic Parquet; no market feed, external model service or real orders.
#[path = "../../../tests/support/execution_models.rs"]
mod execution_models;
#[path = "support/experiment.rs"]
mod experiment;
#[path = "../../../tests/support/strategy_composition.rs"]
mod fixture;
use contracts::{
    research::{DataOrigin, PitStatus},
    science::*,
    strategy_portfolio::*,
    Id, SchemaV1,
};
use std::collections::BTreeMap;

fn member(report: &NativeExperimentEvaluationResultV1, weight: &str) -> NativeStrategyMemberV1 {
    NativeStrategyMemberV1 {
        alpha_id: Id::new(),
        alpha_version_id: Id::new(),
        ensemble_weight: weight.parse().unwrap(),
        policy: fixture::policy(report),
        feature_artifact_ids: report.feature_artifact_ids.clone(),
    }
}
fn run(
    root: &std::path::Path,
    request: &NativeStrategyCompositionRequestV1,
    objects: &BTreeMap<Id, Vec<u8>>,
) -> anyhow::Result<NativeStrategyCompositionResultV1> {
    job::strategy::compose(root, request, |id| {
        objects
            .get(&id)
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("missing fixture object"))
    })
}

#[test]
fn original_fold_single_and_two_member_replay_and_stateful_current_continuation() {
    let (root, evaluation, parts) = experiment::fixture(false);
    let wasm = experiment::module(
        "(global $n (mut i32) (i32.const 0)) (memory 1)",
        "",
        r#"
      global.get $n i32.const 1 i32.add global.set $n
      i32.const 0 i32.const 0 i32.load i32.const 1 i32.add i32.store
      global.get $n i32.const 0 i32.load i32.add local.get 3 i32.add f64.convert_i32_s f64.const 256 f64.div"#,
    );
    let ids = vec![Id::new()];
    let dataset = Id::new();
    let model = Id::new();
    let original = job::experiment::evaluate(
        root.path(),
        &evaluation,
        dataset,
        model,
        &ids,
        &parts,
        &wasm,
    )
    .unwrap();
    let first = member(&original, "1");
    let mut objects = BTreeMap::from([
        (
            first.policy.source.report_artifact_id,
            serde_json::to_vec(&original).unwrap(),
        ),
        (model, wasm.clone()),
        (ids[0], serde_json::to_vec(&parts[0]).unwrap()),
    ]);
    let mut request = NativeStrategyCompositionRequestV1 {
        binary_option: None,
        schema_version: SchemaV1,
        selection: original.folds[0].simulation_request.selection.clone(),
        mandate: fixture::mandate(&evaluation.settings),
        settings: evaluation.settings.clone(),
        purpose: StrategyPortfolioPurposeV1::HistoricalReplay {},
        input_provenance: StrategyInputProvenanceV1 {
            feature_source_bindings: None,
            dataset_revision_id: dataset,
            market_data_origin: DataOrigin::Synthetic,
            pit_status: PitStatus::Unverified,
            revision_policy: contracts::catalogs::DataRevisionPolicy::Unknown,
            feature_artifact_origins: BTreeMap::from([(ids[0], DataOrigin::Synthetic)]),
        },
        members: vec![first],
        total_fuel: fixture::count(10_000_000),
    };
    let single = run(root.path(), &request, &objects).unwrap();
    let StrategyCompositionOutcomeV1::HistoricalReplay {
        simulation_request,
        simulation,
    } = &single.outcome
    else {
        unreachable!()
    };
    assert_eq!(single.consumed_fuel.get(), 0);
    assert_eq!(
        serde_json::to_value(&simulation_request.target_points).unwrap(),
        serde_json::to_value(&original.folds[0].simulation_request.target_points).unwrap()
    );
    let direct = job::simulation::simulate(root.path(), simulation_request).unwrap();
    for field in ["accounts", "fills", "positions", "portfolio_snapshots"] {
        assert_eq!(
            simulation.canonical_result[field],
            direct.canonical_result[field]
        );
    }

    let constant = experiment::module("", "", "f64.const 0.5");
    let second_model = Id::new();
    let second_report = job::experiment::evaluate(
        root.path(),
        &evaluation,
        dataset,
        second_model,
        &ids,
        &parts,
        &constant,
    )
    .unwrap();
    let second = member(&second_report, "0.75");
    objects.insert(
        second.policy.source.report_artifact_id,
        serde_json::to_vec(&second_report).unwrap(),
    );
    request.members[0].ensemble_weight = "0.25".parse().unwrap();
    request.members.push(second);
    let combined = run(root.path(), &request, &objects).unwrap();
    let StrategyCompositionOutcomeV1::HistoricalReplay {
        simulation_request,
        simulation,
    } = &combined.outcome
    else {
        unreachable!()
    };
    let expected = original.folds[0].decisions[0].target_weight.as_decimal()
        * bigdecimal::BigDecimal::from(25)
        / bigdecimal::BigDecimal::from(100)
        + bigdecimal::BigDecimal::from(375) / bigdecimal::BigDecimal::from(1000);
    assert_eq!(
        simulation_request.target_points[0].targets[0]
            .weight
            .as_decimal(),
        &expected
    );
    let direct = job::simulation::simulate(root.path(), simulation_request).unwrap();
    assert_eq!(
        simulation.canonical_result["accounts"],
        direct.canonical_result["accounts"]
    );
    assert_eq!(
        simulation.canonical_result["fills"],
        direct.canonical_result["fills"]
    );
    assert_eq!(
        simulation_request.settings.starting_capital,
        evaluation.settings.starting_capital
    );

    request.members.truncate(1);
    request.members[0].ensemble_weight = "1".parse().unwrap();
    let new = Id::new();
    let extension = fixture::new_observations(&evaluation.feature_schema);
    objects.insert(new, serde_json::to_vec(&extension).unwrap());
    request.members[0].feature_artifact_ids.push(new);
    request
        .input_provenance
        .feature_artifact_origins
        .insert(new, DataOrigin::Synthetic);
    request.selection = evaluation.selection.clone();
    request.purpose = StrategyPortfolioPurposeV1::CurrentDecision {
        account_start: FreshPaperCashV1 {
            downstream_id: Id::new(),
            trader_id: "TRADER-001".into(),
            account_id: "SIM-001".into(),
            base_currency: request.settings.base_currency.clone(),
            starting_capital: request.settings.starting_capital.clone(),
            execution_assumptions_id: request.mandate.execution_assumptions_id,
        },
        member_inputs: vec![StrategyCurrentInputsV1 {
            alpha_version_id: request.members[0].alpha_version_id,
            feature_artifact_ids: vec![new],
        }],
    };
    let current = run(root.path(), &request, &objects).unwrap();
    let StrategyCompositionOutcomeV1::CurrentDecision {
        target,
        predictions_per_member,
        ..
    } = current.outcome
    else {
        unreachable!()
    };
    // 29 calls from original ordinal 11 through 39. Both a persistent global and
    // WASM memory contribute; last-row-only and reset ordinals produce other values.
    assert_eq!(
        predictions_per_member[&request.members[0].alpha_version_id].get(),
        29
    );
    assert_eq!(target.targets[0].weight, "0.37890625".parse().unwrap());
    assert_eq!(target.asof_ns, fixture::count(experiment::event(39) + 1));
    assert_ne!(
        target.targets[0].weight,
        original.folds[0].decisions.last().unwrap().target_weight
    );
    assert!(current.consumed_fuel.get() > 0);
    let fold = &original.folds[0];
    domain::execution::strategy::current_continuation(fold, &target, fixture::count(29)).unwrap();
    assert!(domain::execution::strategy::current_continuation(
        fold,
        &target,
        fixture::count(fold.decisions.len() as u64)
    )
    .is_err());
    let mut prefix_only = target.clone();
    prefix_only.asof_ns = fold.decisions.last().unwrap().decision_ns;
    assert!(domain::execution::strategy::current_continuation(
        fold,
        &prefix_only,
        fixture::count(29)
    )
    .is_err());
    let mut incomplete = request.clone();
    incomplete.selection.event_start_ns = fixture::count(experiment::event(12));
    assert!(run(root.path(), &incomplete, &objects)
        .unwrap_err()
        .to_string()
        .contains("WARMUP_START_MISSING"));
    let mut changed = extension;
    changed.observations[0].sequence = fixture::count(1);
    objects.insert(new, serde_json::to_vec(&changed).unwrap());
    assert!(run(root.path(), &request, &objects)
        .unwrap_err()
        .to_string()
        .contains("SEQUENCE_COLLISION"));
    changed.observations[0].sequence = fixture::count(100);
    changed.observations[0].event_ns = fixture::count(12_000_000_000);
    changed.observations[0].observed_available_ns = Some(fixture::count(12_000_000_001));
    objects.insert(new, serde_json::to_vec(&changed).unwrap());
    assert!(run(root.path(), &request, &objects)
        .unwrap_err()
        .to_string()
        .contains("NEW_OBSERVATION_WINDOW"));
}
