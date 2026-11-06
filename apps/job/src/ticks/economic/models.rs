use crate::{feature_model::FeatureModel, signals::SignalModule};
use anyhow::{ensure, Result};
use contracts::science::FeatureValueV1;

pub struct EconomicModels(Vec<(f64, FeatureModel)>);

impl EconomicModels {
    pub fn new(
        modules: &[(f64, SignalModule)],
        predictions: u64,
        fuel: Option<u64>,
    ) -> Result<(Self, u64)> {
        ensure!(
            !modules.is_empty()
                && modules
                    .iter()
                    .all(|(weight, _)| weight.is_finite() && *weight > 0.0),
            "ECONOMIC_SIGNAL_WEIGHTS_INVALID"
        );
        let mut consumed = 0;
        let mut models = Vec::with_capacity(modules.len());
        for (weight, module) in modules {
            let remaining = fuel.map(|total| total - consumed);
            let model = FeatureModel::new(module, Some(predictions), remaining)?;
            if let (Some(before), Some(after)) = (remaining, model.remaining_fuel()) {
                consumed += before - after;
            }
            models.push((*weight, model));
        }
        Ok((Self(models), consumed))
    }

    pub fn predict(
        &mut self,
        event_ns: u64,
        source_event_ns: u64,
        ordinal: u32,
        features: &[FeatureValueV1],
        total_remaining: Option<u64>,
    ) -> Result<(f64, u64)> {
        let mut consumed = 0;
        let mut score = 0.0;
        for (weight, model) in &mut self.0 {
            if let Some(total) = total_remaining {
                model.cap_remaining_fuel(total - consumed)?;
            }
            let before = model.remaining_fuel();
            score += *weight * model.predict(event_ns, source_event_ns, ordinal, features)?;
            if let (Some(before), Some(after)) = (before, model.remaining_fuel()) {
                consumed += before - after;
            }
        }
        ensure!(score.is_finite(), "ECONOMIC_SIGNAL_NONFINITE");
        Ok((score, consumed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use contracts::science::FeatureMissingReasonV1;

    fn module(value: f64, metered: bool) -> SignalModule {
        let wasm = wat::parse_str(format!(
            r#"(module
              (global $state (mut f64) (f64.const 0))
              (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64))
              (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
                global.get $state f64.const {value} f64.add global.set $state
                global.get $state))"#
        ))
        .unwrap();
        SignalModule::new(&wasm, metered).unwrap()
    }

    fn features() -> Vec<FeatureValueV1> {
        vec![FeatureValueV1 {
            value: None,
            missing_reason: Some(FeatureMissingReasonV1::SourceMissing),
            event_ns: None,
            observed_available_ns: None,
            effective_available_ns: None,
            sequence: None,
        }]
    }

    #[test]
    fn weighted_signals_keep_independent_state_and_share_one_fuel_budget() {
        let modules = vec![(0.25, module(2.0, true)), (0.75, module(-1.0, true))];
        let (mut models, initial) = EconomicModels::new(&modules, 3, Some(1000)).unwrap();
        let (first, consumed) = models
            .predict(2, 1, 0, &features(), Some(1000 - initial))
            .unwrap();
        assert_eq!(first, -0.25);
        assert!(consumed > 0);
        let (second, _) = models
            .predict(3, 2, 1, &features(), Some(1000 - initial - consumed))
            .unwrap();
        assert_eq!(second, -0.5);
        assert!(models
            .predict(4, 3, 2, &features(), Some(consumed - 1))
            .is_err());
    }

    #[test]
    fn unmetered_signals_stay_unmetered_and_invalid_weights_fail() {
        let modules = vec![(1.0, module(2.0, false))];
        let (mut models, initial) = EconomicModels::new(&modules, 1, None).unwrap();
        assert_eq!(initial, 0);
        assert_eq!(
            models.predict(2, 1, 0, &features(), None).unwrap(),
            (2.0, 0)
        );
        assert!(EconomicModels::new(&[], 1, None).is_err());
        assert!(EconomicModels::new(&[(0.0, module(2.0, false))], 1, None).is_err());
    }
}
