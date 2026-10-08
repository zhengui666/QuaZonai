//! Controlled report projections only; no native replay or market-performance claim.
#[path = "../../../tests/support/external_evaluation_report.rs"]
mod controlled;
#[path = "../../../tests/support/execution_models.rs"]
mod execution_models;
use contracts::{
    evidence::MetricStatus,
    execution::NativeTaskParametersV1,
    research::{SplitKind, SplitPolicyV1},
    science::*,
    DbCounter, Id, SchemaV1,
};
use domain::execution::experiment_summary;
use execution_models as models;
use serde_json::json;

fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}
fn report(large: bool) -> NativeExperimentEvaluationResultV1 {
    let request = NativeExperimentEvaluationRequestV1 {
        binary_option: None,
        schema_version: SchemaV1,
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: vec!["A.SIM-1-MINUTE-LAST-EXTERNAL".into()],
            event_start_ns: count(1_577_836_800_000_000_000),
            event_end_ns: count(1_577_840_400_000_000_000),
            decision_cutoff_ns: count(1_577_840_400_000_000_000),
            maximum_rows: 32,
        },
        instrument_id: "A.SIM".into(),
        feature_schema: vec![FeatureDefinitionV1 {
            feature_key: "signal".into(),
            source_ref: "fixture:original".into(),
            source_key: "market".into(),
            availability: FeatureAvailabilityV1::Observed,
            max_age_ns: None,
        }],
        split_policy: SplitPolicyV1 {
            schema_version: SchemaV1,
            kind: SplitKind::WalkForward,
            train_size: count(4),
            test_size: count(2),
            step_size: Some(count(2)),
            group_count: None,
            test_group_count: None,
            purge_observations: count(1),
            embargo_observations: count(0),
            label_horizon_observations: Some(count(1)),
            interval_validation_required: true,
            sealed_revision_id: Id::new(),
        },
        label_horizon_observations: 1,
        total_fuel: Some(count(1_000_000)),
        target_ttl_ns: count(60_000_000_000),
        decision_output: ExperimentDecisionOutputV1::TargetWeight,
        settings: NativeSimulationSettingsV1 {
            multi_currency_spot_cash: None,
            schema_version: SchemaV1,
            base_currency: "USDT".into(),
            starting_capital: "1000".parse().unwrap(),
            account_kind: NativeAccountKind::Cash,
            leverage: "1".parse().unwrap(),
            fee_model: models::fee(),
            fill_model: models::fill(),
            latency_model: models::latency(1),
            snapshot_interval_ms: 1000,
            exposure_tolerance: "0.000001".parse().unwrap(),
            fee_rates: vec![NativeFeeRateV1 {
                instrument_id: "A.SIM".into(),
                maker: "0".parse().unwrap(),
                taker: "0".parse().unwrap(),
            }],
        },
    };
    controlled::report(
        &NativeTaskParametersV1::EvaluateExperiment {
            schema_version: SchemaV1,
            dataset_revision_id: Id::new(),
            model_artifact_id: Id::new(),
            feature_artifact_ids: vec![Id::new()],
            request: Box::new(request),
        },
        large.then_some(100_000),
    )
}

#[test]
fn bounded_preview_preserves_original_fold_identity_endpoints_and_absence() {
    let report = report(true);
    let ids = [Id::new(), Id::new(), Id::new()];
    let summary = experiment_summary(ids[0], ids[1], ids[2], &report).unwrap();
    assert_eq!(summary.experiment_id, ids[0]);
    assert_eq!(summary.run_id, ids[1]);
    assert_eq!(summary.report_artifact_id, ids[2]);
    assert_eq!(summary.dataset_revision_id, report.dataset_revision_id);
    assert_eq!(summary.model_artifact_id, report.model_artifact_id);
    assert_eq!(summary.feature_artifact_ids, report.feature_artifact_ids);
    assert!(summary.folds.len() > 1);
    assert_eq!(summary.folds.len(), report.folds.len());
    assert!(summary.folds.iter().any(|f| f.equity_preview.sampled));
    for (fold, original) in summary.folds.iter().zip(&report.folds) {
        assert_eq!(fold.fold_index, original.fold_index);
        assert_eq!(fold.base_currency, "USDT");
        assert_eq!(fold.starting_capital, "1000".parse().unwrap());
        assert_eq!(fold.account_id, "SIM-001");
        assert!(fold.equity_preview.points.len() <= 64);
        let snapshots = original.simulation.canonical_result["portfolio_snapshots"]
            .as_array()
            .unwrap();
        assert_eq!(
            fold.equity_preview.source_point_count.get(),
            snapshots.len() as u64
        );
        for point in &fold.equity_preview.points {
            let timestamp = point.timestamp_ns.get().to_string();
            assert!(snapshots
                .iter()
                .any(|s| s["ts_event"].as_str() == Some(timestamp.as_str())));
            assert_eq!(point.value, Some("1000".parse().unwrap()));
        }
        assert_eq!(
            fold.equity_preview
                .points
                .first()
                .unwrap()
                .timestamp_ns
                .get()
                .to_string(),
            snapshots.first().unwrap()["ts_event"].as_str().unwrap()
        );
        assert_eq!(
            fold.equity_preview
                .points
                .last()
                .unwrap()
                .timestamp_ns
                .get()
                .to_string(),
            snapshots.last().unwrap()["ts_event"].as_str().unwrap()
        );
        assert!(fold.native_account_pnl.value.is_none());
        assert_eq!(
            fold.native_account_pnl.status,
            MetricStatus::InsufficientData
        );
        assert!(fold.commissions.is_none());
        assert!(fold.commissions_reason.is_some());
        assert_eq!(fold.sharpe_ratio.status, MetricStatus::InsufficientData);
        assert_eq!(fold.max_drawdown.status, MetricStatus::InsufficientData);
    }
    let encoded = serde_json::to_vec(&summary).unwrap();
    assert!(encoded.len() < 256_000);
    let value = serde_json::to_value(summary).unwrap();
    assert!(value.get("sharpe_ratio").is_none());
    assert!(value.get("equity").is_none());
}

#[test]
fn native_values_preserve_currency_daily_basis_units_and_missing_return_precedence() {
    let mut report = report(false);
    let native = &mut report.folds[0].simulation;
    native.statistics = vec![
        NativeStatisticV1 {
            group: NativeStatisticGroup::Pnl,
            native_key: "PnL (total)".into(),
            currency: Some("USDT".into()),
            value: Some(12.5),
            reason_code: None,
        },
        NativeStatisticV1 {
            group: NativeStatisticGroup::Pnl,
            native_key: "PnL (total)".into(),
            currency: Some("USD".into()),
            value: Some(99.0),
            reason_code: None,
        },
        NativeStatisticV1 {
            group: NativeStatisticGroup::Returns,
            native_key: "Sharpe Ratio (252 days)".into(),
            currency: None,
            value: Some(1.25),
            reason_code: None,
        },
        NativeStatisticV1 {
            group: NativeStatisticGroup::Returns,
            native_key: "Max Drawdown".into(),
            currency: None,
            value: Some(0.0),
            reason_code: None,
        },
    ];
    native.canonical_result["accounts"][0]["Cash"]["base"]["commissions"] =
        json!({"USDT":"1.23 USDT", "USD":"99.00 USD"});
    let project = || {
        experiment_summary(Id::new(), Id::new(), Id::new(), &report)
            .unwrap()
            .folds
            .remove(0)
    };
    let empty = project();
    assert_eq!(empty.native_account_pnl.value, Some(12.5));
    assert_eq!(empty.native_account_pnl.unit, "USDT");
    assert_eq!(empty.commissions, Some("1.23".parse().unwrap()));
    assert!(empty.commissions_reason.is_none());
    assert_eq!(
        empty.max_drawdown.value, None,
        "native empty-return zero is unavailable"
    );
    assert_eq!(
        empty.sharpe_ratio.reason_code.as_deref(),
        Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE")
    );
    let native = &mut report.folds[0].simulation;
    native.returns_status = MetricStatus::Ok;
    native.returns_reason = None;
    native.returns = vec![NativeReturnV1 {
        timestamp_ns: count(1_577_836_800_000_000_000),
        value: Some(0.01),
        reason_code: None,
    }];
    native.statistics[3].value = Some(-0.2);
    let fold = experiment_summary(Id::new(), Id::new(), Id::new(), &report)
        .unwrap()
        .folds
        .remove(0);
    assert_eq!(fold.sharpe_ratio.value, Some(1.25));
    assert_eq!(fold.sharpe_ratio.annualization_days, Some(252));
    assert_eq!(fold.max_drawdown.value, Some(-0.2));
    assert_eq!(fold.max_drawdown.unit, "NEGATIVE_RETURN_FRACTION");
    assert_eq!(fold.max_drawdown.annualization_days, None);
    report.folds[0].simulation.statistics[2].value = None;
    report.folds[0].simulation.statistics[2].reason_code =
        Some("NATIVE_STATISTIC_UNAVAILABLE".into());
    let fold = experiment_summary(Id::new(), Id::new(), Id::new(), &report)
        .unwrap()
        .folds
        .remove(0);
    assert_eq!(fold.sharpe_ratio.value, None);
    assert_eq!(fold.sharpe_ratio.status, MetricStatus::Failed);
}

#[test]
fn malformed_money_or_conflicting_native_equity_cannot_become_a_summary() {
    let original = report(false);
    for case in 0..3 {
        let mut report = original.clone();
        let canonical = &mut report.folds[0].simulation.canonical_result;
        if case == 0 {
            canonical["accounts"][0]["Cash"]["base"]["commissions"] = json!({"USDT":"1.23 USD"});
        } else if case == 1 {
            canonical["portfolio_snapshots"][0]["total_equity"] = json!(["1000 USD"]);
        } else {
            let mut duplicate = canonical["portfolio_snapshots"][0].clone();
            duplicate["total_equity"] = json!(["900 USDT"]);
            canonical["portfolio_snapshots"]
                .as_array_mut()
                .unwrap()
                .push(duplicate);
        }
        assert!(experiment_summary(Id::new(), Id::new(), Id::new(), &report).is_err());
    }
}

#[path = "../../../tests/support/strategy_composition.rs"]
mod strategy_fixture;
use contracts::strategy_portfolio::*;

fn candidate_for(report: &NativeStrategyCompositionResultV1) -> StrategyPortfolioCandidateV1 {
    let target = match &report.outcome {
        StrategyCompositionOutcomeV1::HistoricalReplay {
            simulation_request, ..
        } => simulation_request.target_points.last().unwrap(),
        StrategyCompositionOutcomeV1::CurrentDecision { target, .. } => target,
    };
    StrategyPortfolioCandidateV1 {
        schema_version: SchemaV1,
        source_kind: StrategyPortfolioSourceV1::StrategyAlpha,
        id: Id::new(),
        project_id: Id::new(),
        mandate_id: Id::new(),
        input_set_id: Id::new(),
        run_id: Id::new(),
        accepted_attempt_id: Id::new(),
        report_artifact_id: Id::new(),
        allocation_method: StrategyAllocationMethodV1::FixedTargetWeights,
        purpose: report.request.purpose.clone(),
        input_provenance: report.request.input_provenance.clone(),
        members: report
            .request
            .members
            .iter()
            .map(|member| StrategyMemberSelectionV1 {
                alpha_version_id: member.alpha_version_id,
                ensemble_weight: member.ensemble_weight.clone(),
            })
            .collect(),
        targets: target.targets.clone(),
        cash_weight: target.cash_weight.clone(),
        decision_asof: chrono::DateTime::from_timestamp_nanos(target.asof_ns.get() as i64),
        created_at: chrono::Utc::now(),
    }
}

fn historical_strategy(
    source: &NativeExperimentEvaluationResultV1,
) -> NativeStrategyCompositionResultV1 {
    let fold = &source.folds[0];
    let mut report = strategy_fixture::current_report();
    report.request.selection = fold.simulation_request.selection.clone();
    report.request.settings = fold.simulation_request.settings.clone();
    report.request.mandate = strategy_fixture::mandate(&report.request.settings);
    report.request.purpose = StrategyPortfolioPurposeV1::HistoricalReplay {};
    report.request.members = vec![NativeStrategyMemberV1 {
        alpha_id: Id::new(),
        alpha_version_id: Id::new(),
        ensemble_weight: "1".parse().unwrap(),
        policy: strategy_fixture::policy(source),
        feature_artifact_ids: source.feature_artifact_ids.clone(),
    }];
    report.request.input_provenance.dataset_revision_id = source.dataset_revision_id;
    report.request.input_provenance.feature_artifact_origins = source
        .feature_artifact_ids
        .iter()
        .map(|&id| (id, contracts::research::DataOrigin::Synthetic))
        .collect();
    report.consumed_fuel = Some(DbCounter::ZERO);
    report.outcome = StrategyCompositionOutcomeV1::HistoricalReplay {
        simulation_request: Box::new(fold.simulation_request.clone()),
        simulation: Box::new(fold.simulation.clone()),
    };
    report
}

#[test]
fn strategy_replay_reuses_exact_native_statistics_missingness_and_bounded_equity() {
    let source = report(true);
    let report = historical_strategy(&source);
    let candidate = candidate_for(&report);
    let summary = domain::execution::strategy_portfolio_summary(&candidate, &report).unwrap();
    assert_eq!(summary.candidate_id, candidate.id);
    assert_eq!(summary.accepted_attempt_id, candidate.accepted_attempt_id);
    assert_eq!(summary.report_artifact_id, candidate.report_artifact_id);
    assert!(summary
        .interpretation
        .contains("no scientific qualification"));
    let StrategyPortfolioSummaryOutcomeV1::HistoricalReplay { simulation } = summary.outcome else {
        panic!("historical simulation");
    };
    let mut expected = serde_json::to_value(
        experiment_summary(Id::new(), Id::new(), Id::new(), &source)
            .unwrap()
            .folds
            .remove(0),
    )
    .unwrap();
    expected.as_object_mut().unwrap().remove("fold_index");
    assert_eq!(serde_json::to_value(&simulation).unwrap(), expected);
    assert_eq!(simulation.equity_preview.points.len(), 64);
    assert!(simulation.equity_preview.sampled);
    assert_eq!(simulation.sharpe_ratio.value, None);
    assert!(simulation.sharpe_ratio.reason_code.is_some());
    assert!(simulation.commissions.is_none());
    assert!(simulation.commissions_reason.is_some());
    assert!(simulation.period_start_ns < simulation.period_end_ns);
}

#[test]
fn current_strategy_is_explicitly_target_only_and_mismatched_sources_are_rejected() {
    let report = strategy_fixture::current_report();
    let candidate = candidate_for(&report);
    let summary = domain::execution::strategy_portfolio_summary(&candidate, &report).unwrap();
    let wire = serde_json::to_value(summary).unwrap();
    assert_eq!(
        wire["outcome"],
        json!({
            "purpose": "CURRENT_DECISION",
            "reason_code": "CURRENT_DECISION_TARGET_ONLY_NO_HISTORICAL_SIMULATION"
        })
    );
    for case in 0..5 {
        let mut altered = candidate.clone();
        match case {
            0 => altered.input_provenance.dataset_revision_id = Id::new(),
            1 => altered.members[0].alpha_version_id = Id::new(),
            2 => altered.targets[0].weight = "0.5".parse().unwrap(),
            3 => altered.decision_asof += chrono::Duration::nanoseconds(1),
            _ => altered.purpose = StrategyPortfolioPurposeV1::HistoricalReplay {},
        }
        assert!(domain::execution::strategy_portfolio_summary(&altered, &report).is_err());
    }
}
