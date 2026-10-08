//! Controlled wire/domain examples; they do not attest native execution.
#![allow(dead_code)]
use super::execution_models as models;
use contracts::{
    portfolio::PortfolioConstraintsV1,
    research::{DataOrigin, DataPartition, PitStatus},
    science::*,
    strategy_portfolio::*,
    DbCounter, Id, SchemaV1,
};
use std::collections::BTreeMap;
pub fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
pub fn settings() -> NativeSimulationSettingsV1 {
    NativeSimulationSettingsV1 {
        multi_currency_spot_cash: None,
        schema_version: SchemaV1,
        base_currency: "USD".into(),
        starting_capital: "1000000".parse().unwrap(),
        account_kind: NativeAccountKind::Cash,
        leverage: "1".parse().unwrap(),
        fee_model: models::fee(),
        fill_model: models::fill(),
        latency_model: models::latency(1_000_000),
        snapshot_interval_ms: 1000,
        exposure_tolerance: "0.0002".parse().unwrap(),
        fee_rates: vec![NativeFeeRateV1 {
            instrument_id: "TARGET.SIM".into(),
            maker: "0".parse().unwrap(),
            taker: "0.001".parse().unwrap(),
        }],
    }
}
pub fn mandate(settings: &NativeSimulationSettingsV1) -> StrategyMandateContentV1 {
    StrategyMandateContentV1 {
        schema_version: SchemaV1,
        allocation_method: StrategyAllocationMethodV1::FixedTargetWeights,
        base_currency: settings.base_currency.clone(),
        capital_assumption: settings.starting_capital.clone(),
        universe_version_id: Id::new(),
        execution_assumptions_id: Id::new(),
        constraints: PortfolioConstraintsV1 {
            schema_version: SchemaV1,
            long_only: true,
            min_cash_weight: "0".parse().unwrap(),
            max_cash_weight: "1".parse().unwrap(),
            min_asset_weight: "0".parse().unwrap(),
            max_asset_weight: "1".parse().unwrap(),
            max_gross_exposure: "1".parse().unwrap(),
            min_net_exposure: "0".parse().unwrap(),
            max_net_exposure: "1".parse().unwrap(),
            max_turnover_per_rebalance: "2".parse().unwrap(),
            max_participation: None,
            max_ex_ante_risk: None,
            group_bounds: vec![],
            asset_overrides: vec![],
            transaction_costs_ref: Id::new(),
            liquidity_ref: None,
        },
        exposure_tolerance: settings.exposure_tolerance.clone(),
        max_input_age_seconds: 60,
        target_ttl_seconds: 10,
    }
}
pub fn policy(report: &NativeExperimentEvaluationResultV1) -> FrozenTargetPolicyV1 {
    let first = &report.folds[0].decisions[0];
    FrozenTargetPolicyV1 {
        schema_version: SchemaV1,
        output_kind: StrategyOutputKindV1::TargetWeight,
        source: AcceptedExperimentSourceV1 {
            experiment_id: Id::new(),
            evaluation_run_id: Id::new(),
            accepted_attempt_id: Id::new(),
            report_artifact_id: Id::new(),
        },
        code_artifact_id: Id::new(),
        model_artifact_id: report.model_artifact_id,
        parameter_artifact_id: Id::new(),
        dataset_revision_id: report.dataset_revision_id,
        feature_artifact_ids: report.feature_artifact_ids.clone(),
        feature_schema: report.request.feature_schema.clone(),
        instrument_id: report.instrument_id.clone(),
        base_currency: report.request.settings.base_currency.clone(),
        model_abi: FEATURE_MODEL_ABI_V2.into(),
        initialization: FeatureReplayInitializationV1 {
            source_fold_index: 0,
            first_ordinal: first.ordinal,
            first_event_ns: first.event_ns,
            first_decision_ns: first.decision_ns,
        },
        target_ttl_ns: report.request.target_ttl_ns,
        runtime_image_ref: "fixture/native@sha256:controlled".into(),
    }
}
pub fn current_report() -> NativeStrategyCompositionResultV1 {
    let settings = settings();
    let mandate = mandate(&settings);
    let alpha = Id::new();
    let old = Id::new();
    let new = Id::new();
    let dataset = Id::new();
    let definitions = vec![FeatureDefinitionV1 {
        feature_key: "measurement".into(),
        source_ref: "frozen:fixture".into(),
        source_key: "fixture".into(),
        availability: FeatureAvailabilityV1::Observed,
        max_age_ns: None,
    }];
    let policy = FrozenTargetPolicyV1 {
        schema_version: SchemaV1,
        output_kind: StrategyOutputKindV1::TargetWeight,
        source: AcceptedExperimentSourceV1 {
            experiment_id: Id::new(),
            evaluation_run_id: Id::new(),
            accepted_attempt_id: Id::new(),
            report_artifact_id: Id::new(),
        },
        code_artifact_id: Id::new(),
        model_artifact_id: Id::new(),
        parameter_artifact_id: Id::new(),
        dataset_revision_id: Id::new(),
        feature_artifact_ids: vec![old],
        feature_schema: definitions,
        instrument_id: "TARGET.SIM".into(),
        base_currency: "USD".into(),
        model_abi: FEATURE_MODEL_ABI_V2.into(),
        initialization: FeatureReplayInitializationV1 {
            source_fold_index: 0,
            first_ordinal: 11,
            first_event_ns: count(12_000_000_000),
            first_decision_ns: count(12_000_000_001),
        },
        target_ttl_ns: count(10_000_000_000),
        runtime_image_ref: "fixture/native@sha256:controlled".into(),
    };
    let account = FreshPaperCashV1 {
        downstream_id: Id::new(),
        trader_id: "TRADER-001".into(),
        account_id: "SIM-001".into(),
        base_currency: settings.base_currency.clone(),
        starting_capital: settings.starting_capital.clone(),
        execution_assumptions_id: mandate.execution_assumptions_id,
    };
    NativeStrategyCompositionResultV1 {
        schema_version: SchemaV1,
        request: NativeStrategyCompositionRequestV1 {
        binary_option: None,
            schema_version: SchemaV1,
            selection: NativeBarSelectionV1 {
                schema_version: SchemaV1,
                bar_types: vec!["TARGET.SIM-1-SECOND-LAST-EXTERNAL".into()],
                event_start_ns: count(0),
                event_end_ns: count(44_000_000_000),
                decision_cutoff_ns: count(44_000_000_000),
                maximum_rows: 100,
            },
            mandate,
            settings,
            purpose: StrategyPortfolioPurposeV1::CurrentDecision {
                account_start: account.clone(),
                member_inputs: vec![StrategyCurrentInputsV1 {
                    alpha_version_id: alpha,
                    feature_artifact_ids: vec![new],
                }],
            },
            input_provenance: StrategyInputProvenanceV1 {
                feature_source_bindings: None,
                dataset_revision_id: dataset,
                market_data_origin: DataOrigin::Synthetic,
                pit_status: PitStatus::Unverified,
                revision_policy: contracts::catalogs::DataRevisionPolicy::Unknown,
                feature_artifact_origins: BTreeMap::from([
                    (old, DataOrigin::Synthetic),
                    (new, DataOrigin::Synthetic),
                ]),
            },
            members: vec![NativeStrategyMemberV1 {
                alpha_id: Id::new(),
                alpha_version_id: alpha,
                ensemble_weight: "1".parse().unwrap(),
                policy,
                feature_artifact_ids: vec![old, new],
            }],
            total_fuel: Some(count(100_000)),
        },
        native_versions: BTreeMap::from([
            ("nautilus-backtest".into(), "0.63.0".into()),
            ("wasmi".into(), "2.0.0".into()),
            ("strategy-composition".into(), "1".into()),
        ]),
        consumed_fuel: Some(count(1000)),
        outcome: StrategyCompositionOutcomeV1::CurrentDecision {
            account_start: Box::new(account),
            target: NativeTargetPointV1 {
                schema_version: SchemaV1,
                asof_ns: count(43_000_000_001),
                valid_until_ns: count(53_000_000_001),
                targets: vec![contracts::portfolio::AllocationTargetV1 {
                    instrument_id: "TARGET.SIM".into(),
                    currency: "USD".into(),
                    weight: "0.25".parse().unwrap(),
                }],
                cash_weight: "0.75".parse().unwrap(),
            },
            predictions_per_member: BTreeMap::from([(alpha, count(29))]),
        },
    }
}
pub fn new_observations(schema: &[FeatureDefinitionV1]) -> FeatureObservationsV1 {
    FeatureObservationsV1 {
        schema_version: SchemaV1,
        partition: DataPartition::Forward,
        feature_schema: schema.to_vec(),
        observations: vec![FeatureObservationV1 {
            feature_index: 0,
            event_ns: count(40_000_000_000),
            observed_available_ns: Some(count(40_000_000_001)),
            sequence: count(100),
            value: Some(0.5),
            missing_reason: None,
        }],
    }
}
