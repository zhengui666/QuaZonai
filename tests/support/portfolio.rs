//! Synthetic numerical configuration only; no ownership or qualification evidence.
use contracts::{brief::TargetKind, portfolio::*, science::*, Id, SchemaV1};

pub fn request(input: &AllocationInputV1) -> NativePortfolioBuildRequestV1 {
    NativePortfolioBuildRequestV1 {
        schema_version: SchemaV1,
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: input.forecasts.bar_types.clone(),
            event_start_ns: contracts::DbCounter::ZERO,
            event_end_ns: input.forecasts.decision_asof_ns,
            decision_cutoff_ns: input.forecasts.decision_asof_ns,
            maximum_rows: 1000,
        },
        mandate: MandateContentV1 {
            objective: input.objective,
            risk_measure: input.risk,
            base_currency: input.base_currency.clone(),
            capital_assumption: input.capital_assumption.clone(),
            universe_version_id: Id::new(),
            covariance_estimator: input.covariance_estimator.clone(),
            alpha_ensemble: input.alpha_ensemble.clone(),
            optimizer: input.optimizer.clone(),
            constraints: input.constraints.clone(),
            rebalance_schedule: RebalanceScheduleV1 {
                schema_version: SchemaV1,
                kind: RebalanceKind::Manual,
                interval_seconds: None,
                calendar_ref: None,
                timezone: "UTC".into(),
                session_offset_seconds: None,
                max_input_age_seconds: input.forecasts.max_input_age_seconds,
                target_ttl_seconds: 300,
            },
            required_evaluation_policy_id: Id::new(),
            execution_assumptions_id: Id::new(),
            exposure_tolerance: input.exposure_tolerance.clone(),
        },
        current_cash_weight: input.current_cash_weight.clone(),
        assets: input.assets.clone(),
        members: input
            .forecasts
            .members
            .iter()
            .map(|m| NativePortfolioAlphaV1 {
                alpha_id: m.alpha_id,
                alpha_version_id: m.alpha_version_id,
                model_artifact_id: Id::new(),
                calibration_artifact_id: None,
                target_kind: TargetKind::ExpectedReturn,
                ensemble_weight: m.ensemble_weight.clone(),
                parameters: NativeForecastParametersV1 {
                    schema_version: SchemaV1,
                    fast_period: 2,
                    slow_period: 3,
                    label_horizon_observations: m.horizon_value.get() as u32,
                    total_fuel: contracts::DbCounter::new(100_000_000).unwrap(),
                },
            })
            .collect(),
    }
}
