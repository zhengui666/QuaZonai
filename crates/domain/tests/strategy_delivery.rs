//! Package binding tests use a controlled report, not native execution evidence.
#[path = "../../../tests/support/execution_models.rs"]
mod execution_models;
#[path = "../../../tests/support/strategy_composition.rs"]
mod fixture;
use chrono::{DateTime, Utc};
use contracts::{
    delivery::PackageTargetV1, forward::ForwardEnvironmentV1, strategy_portfolio::*, Id, SchemaV1,
};

fn example() -> (
    TargetPackageV2,
    NativeStrategyCompositionResultV1,
    StrategyPortfolioCandidateV1,
) {
    let report = fixture::current_report();
    let StrategyCompositionOutcomeV1::CurrentDecision {
        target,
        account_start,
        ..
    } = &report.outcome
    else {
        unreachable!()
    };
    let candidate = StrategyPortfolioCandidateV1 {
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
            .map(|m| StrategyMemberSelectionV1 {
                alpha_version_id: m.alpha_version_id,
                ensemble_weight: m.ensemble_weight.clone(),
            })
            .collect(),
        targets: target.targets.clone(),
        cash_weight: target.cash_weight.clone(),
        decision_asof: DateTime::<Utc>::from_timestamp_nanos(target.asof_ns.get() as i64),
        created_at: DateTime::<Utc>::from_timestamp_nanos(44_000_000_000),
    };
    let package = TargetPackageV2 {
        release_id: Id::new(),
        package_schema_version: TargetPackageVersionV2::V2,
        project_id: candidate.project_id,
        candidate_id: candidate.id,
        mandate_id: candidate.mandate_id,
        source_kind: StrategyReleaseSourceV1::NativeTargetDecision,
        source: NativeTargetDecisionSourceV1 {
            run_id: candidate.run_id,
            accepted_attempt_id: candidate.accepted_attempt_id,
            report_artifact_id: candidate.report_artifact_id,
            alpha_version_ids: candidate
                .members
                .iter()
                .map(|m| m.alpha_version_id)
                .collect(),
            input_provenance: candidate.input_provenance.clone(),
        },
        execution_environment: ForwardEnvironmentV1::Paper,
        account_start: account_start.as_ref().clone(),
        execution_settings: report.request.settings.clone(),
        input_revision_refs: vec![report.request.input_provenance.dataset_revision_id],
        engine_versions: report.native_versions.clone(),
        asof: candidate.decision_asof,
        valid_from: candidate.created_at,
        valid_until: DateTime::<Utc>::from_timestamp_nanos(target.valid_until_ns.get() as i64),
        base_currency: report.request.mandate.base_currency.clone(),
        capital_assumption: report.request.mandate.capital_assumption.clone(),
        targets: target
            .targets
            .iter()
            .map(|t| PackageTargetV1 {
                instrument_id: t.instrument_id.clone(),
                target_weight: t.weight.clone(),
                currency: t.currency.clone(),
            })
            .collect(),
        cash_weight: target.cash_weight.clone(),
        constraints_summary: report.request.mandate.constraints.clone(),
        exposure_tolerance: report.request.mandate.exposure_tolerance.clone(),
        cost_assumption_ref: report.request.mandate.execution_assumptions_id,
        compatible_market_capabilities: vec!["fixture/1".into()],
        limitations: vec!["Synthetic controlled source".into()],
        provenance_artifact_refs: vec![candidate.report_artifact_id],
    };
    (package, report, candidate)
}

#[test]
fn one_member_preserves_original_source_account_cost_targets_and_clocks() {
    let (package, report, candidate) = example();
    domain::delivery::strategy_target_package(&package, &report, &candidate).unwrap();
    assert_eq!(package.source.alpha_version_ids.len(), 1);
    for mutation in [
        "run",
        "attempt",
        "report",
        "alpha",
        "account",
        "settings",
        "target",
        "cash",
        "decision_time",
        "expiry",
        "origin",
    ] {
        let mut changed = package.clone();
        match mutation {
            "run" => changed.source.run_id = Id::new(),
            "attempt" => changed.source.accepted_attempt_id = Id::new(),
            "report" => changed.source.report_artifact_id = Id::new(),
            "alpha" => changed.source.alpha_version_ids[0] = Id::new(),
            "account" => changed.account_start.account_id = "OTHER-001".into(),
            "settings" => changed.execution_settings.fee_rates[0].taker = "0.01".parse().unwrap(),
            "target" => changed.targets[0].target_weight = "0.5".parse().unwrap(),
            "cash" => changed.cash_weight = "0.5".parse().unwrap(),
            "decision_time" => changed.asof = changed.valid_from,
            "expiry" => changed.valid_until += chrono::Duration::nanoseconds(1),
            "origin" => {
                changed.source.input_provenance.market_data_origin =
                    contracts::research::DataOrigin::Real
            }
            _ => unreachable!(),
        }
        assert!(
            domain::delivery::strategy_target_package(&changed, &report, &candidate).is_err(),
            "{mutation}"
        );
    }
}

#[test]
fn historical_candidate_cannot_be_relabelled_as_a_current_release() {
    let (package, report, mut candidate) = example();
    candidate.purpose = StrategyPortfolioPurposeV1::HistoricalReplay {};
    assert!(domain::delivery::strategy_target_package(&package, &report, &candidate).is_err());
}
