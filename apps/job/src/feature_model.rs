//! Explicit v2 ABI. The original eight-scalar `predict` ABI remains unchanged.
use crate::signals::SignalModule;
use anyhow::{ensure, Result};
use contracts::science::{FeatureMissingReasonV1, FeatureValueV1};
use wasmi::{Store, TypedFunc};

pub use contracts::science::FEATURE_MODEL_ABI_V2 as ABI;

/// Experiment dictionaries are request-owned, with target index zero for their
/// sole traded instrument. Catalog policies assign causal first-arrival indices.
/// No labels, host clock, files, imports or training calls.
pub struct FeatureModel {
    store: Store<()>,
    set_feature: TypedFunc<(i32, f64, i32, i64, i64), ()>,
    predict: TypedFunc<(i64, i64, i32, i32, i32), f64>,
    remaining_predictions: Option<u64>,
    remaining_fuel: Option<u64>,
    failed: bool,
}

impl FeatureModel {
    pub fn new(
        module: &SignalModule,
        predictions: impl Into<Option<u64>>,
        fuel: impl Into<Option<u64>>,
    ) -> Result<Self> {
        let predictions = predictions.into();
        let fuel = fuel.into();
        let (store, instance) = module.instantiate_store(predictions, fuel)?;
        let set_feature = instance
            .get_typed_func(&store, "qz_set_feature_v2")
            .map_err(|_| anyhow::anyhow!("FEATURE_MODEL_ABI_MISMATCH"))?;
        let predict = instance
            .get_typed_func(&store, "qz_predict_v2")
            .map_err(|_| anyhow::anyhow!("FEATURE_MODEL_ABI_MISMATCH"))?;
        let remaining_fuel = fuel.map(|_| store.get_fuel()).transpose()?;
        Ok(Self {
            store,
            set_feature,
            predict,
            remaining_predictions: predictions,
            remaining_fuel,
            failed: false,
        })
    }

    /// An explicit budget covers all setters and predictions. Absent values have
    /// a mandatory nonzero mask; their unused numeric slot is canonical zero.
    pub fn predict(
        &mut self,
        decision_ns: u64,
        event_ns: u64,
        ordinal: u32,
        features: &[FeatureValueV1],
    ) -> Result<f64> {
        self.predict_for_target(decision_ns, event_ns, 0, ordinal, features)
    }

    /// A catalog policy assigns a stable index when an instrument first arrives.
    /// Existing one-instrument experiments retain target index zero.
    pub fn predict_for_target(
        &mut self,
        decision_ns: u64,
        event_ns: u64,
        target_index: u32,
        ordinal: u32,
        features: &[FeatureValueV1],
    ) -> Result<f64> {
        ensure!(!self.failed, "FEATURE_MODEL_INSTANCE_FAILED");
        self.failed = true;
        ensure!(
            self.remaining_predictions != Some(0) && self.remaining_fuel != Some(0),
            "FEATURE_MODEL_BUDGET_EXHAUSTED"
        );
        ensure!(!features.is_empty(), "FEATURE_MODEL_INPUT_LIMIT");
        if let Some(remaining) = &mut self.remaining_predictions {
            *remaining -= 1;
        }
        let result = (|| -> Result<f64> {
            for (index, feature) in features.iter().enumerate() {
                let (value, mask) = match (feature.value, feature.missing_reason) {
                    (Some(value), None) if value.is_finite() => (value, 0),
                    (None, Some(FeatureMissingReasonV1::NotYetAvailable)) => (0.0, 1),
                    (None, Some(FeatureMissingReasonV1::Expired)) => (0.0, 2),
                    (None, Some(FeatureMissingReasonV1::SourceMissing)) => (0.0, 3),
                    _ => anyhow::bail!("FEATURE_MODEL_INPUT_INVALID"),
                };
                self.set_feature.call(
                    &mut self.store,
                    (
                        i32::try_from(index)?,
                        value,
                        mask,
                        i64::try_from(feature.event_ns.map_or(0, |clock| clock.get()))?,
                        i64::try_from(
                            feature
                                .effective_available_ns
                                .map_or(0, |clock| clock.get()),
                        )?,
                    ),
                )?;
            }
            let output = self.predict.call(
                &mut self.store,
                (
                    i64::try_from(decision_ns)?,
                    i64::try_from(event_ns)?,
                    i32::try_from(target_index)?,
                    i32::try_from(ordinal)?,
                    i32::try_from(features.len())?,
                ),
            )?;
            ensure!(output.is_finite(), "FEATURE_MODEL_OUTPUT_NONFINITE");
            Ok(output)
        })();
        if let Some(remaining) = &mut self.remaining_fuel {
            let unused = self.store.get_fuel()?;
            ensure!(
                unused <= *remaining,
                "FEATURE_MODEL_FUEL_ACCOUNTING_INVALID"
            );
            *remaining = unused;
        }
        let value = result.map_err(|_| anyhow::anyhow!("FEATURE_MODEL_EXECUTION_FAILED"))?;
        self.failed = false;
        Ok(value)
    }

    /// Tighten a shared caller budget without replenishing this instance.
    pub fn cap_remaining_fuel(&mut self, maximum: u64) -> Result<()> {
        if let Some(remaining) = &mut self.remaining_fuel {
            let capped = (*remaining).min(maximum);
            self.store.set_fuel(capped)?;
            *remaining = capped;
        }
        Ok(())
    }

    pub fn remaining_fuel(&self) -> Option<u64> {
        self.remaining_fuel
    }
}

#[cfg(test)]
mod shared_fuel_tests {
    use super::*;

    #[test]
    fn per_asset_budget_can_only_shrink_to_global_remainder() {
        let bytes = wat::parse_str(
            r#"(module
          (func (export "qz_set_feature_v2") (param i32 f64 i32 i64 i64))
          (func (export "qz_predict_v2") (param i64 i64 i32 i32 i32) (result f64)
            f64.const 1))"#,
        )
        .unwrap();
        let module = SignalModule::new(&bytes, true).unwrap();
        let mut first = FeatureModel::new(&module, Some(10), Some(100)).unwrap();
        let mut second = FeatureModel::new(&module, Some(10), Some(100)).unwrap();
        first.cap_remaining_fuel(10).unwrap();
        first.cap_remaining_fuel(100).unwrap();
        assert_eq!(first.remaining_fuel(), Some(10));
        second.cap_remaining_fuel(0).unwrap();
        assert_eq!(second.remaining_fuel(), Some(0));
        assert!(second
            .predict(1, 1, 0, &[])
            .unwrap_err()
            .to_string()
            .contains("FEATURE_MODEL_BUDGET_EXHAUSTED"));
    }
}
