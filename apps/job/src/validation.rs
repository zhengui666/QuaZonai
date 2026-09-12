//! Thin adapters over native splitters and estimators; no qualification authority.
use anyhow::{ensure, Result};
use linregress::{FormulaRegressionBuilder, RegressionDataBuilder, RegressionModel};
use ndarray::Array2;
use ndarray_stats::CorrelationExt;

mod alpha;
pub use alpha::validate_alpha;

pub use domain::execution::validation::{
    validation_folds, MAX_VALIDATION_FOLDS, MAX_VALIDATION_INDICES, MAX_VALIDATION_ROWS,
};

/// Columns are synchronized observation times; rows are a frozen asset ordering.
/// No annualization, missing-value imputation or unregistered shrinkage is performed.
pub fn sample_covariance(asset_returns: &[Vec<f64>]) -> Result<Vec<Vec<f64>>> {
    ensure!(
        (1..=contracts::portfolio::MAX_ALLOCATION_ASSETS).contains(&asset_returns.len()),
        "COVARIANCE_ASSET_LIMIT"
    );
    let observations = asset_returns[0].len();
    ensure!(
        (2..=MAX_VALIDATION_ROWS).contains(&observations),
        "COVARIANCE_SAMPLE_LIMIT"
    );
    ensure!(
        asset_returns
            .len()
            .checked_mul(observations)
            .is_some_and(|n| n <= MAX_VALIDATION_INDICES),
        "COVARIANCE_SIZE_LIMIT"
    );
    ensure!(
        asset_returns
            .iter()
            .all(|row| row.len() == observations && row.iter().all(|value| value.is_finite())),
        "COVARIANCE_INPUT_INVALID"
    );
    let values = asset_returns.iter().flatten().copied().collect();
    let matrix = Array2::from_shape_vec((asset_returns.len(), observations), values)?;
    let covariance = matrix.cov(1.0)?;
    ensure!(
        covariance.iter().all(|value| value.is_finite()),
        "COVARIANCE_RESULT_NONFINITE"
    );
    Ok(covariance
        .rows()
        .into_iter()
        .map(|row| row.to_vec())
        .collect())
}

/// A real fitted estimator, not a caller-provided multiplier masquerading as calibration.
pub struct ScoreCalibration {
    model: RegressionModel,
    observations: usize,
}
impl ScoreCalibration {
    /// Inputs must be selected from the same authorized training fold, with complete labels.
    pub fn fit(scores: &[f64], returns: &[f64]) -> Result<Self> {
        ensure!(
            (3..=MAX_VALIDATION_ROWS).contains(&scores.len()) && scores.len() == returns.len(),
            "CALIBRATION_SAMPLE_LIMIT"
        );
        ensure!(
            scores.iter().chain(returns).all(|value| value.is_finite()),
            "CALIBRATION_NONFINITE"
        );
        ensure!(
            scores.iter().any(|value| *value != scores[0]),
            "CALIBRATION_CONSTANT_SCORE"
        );
        let data = RegressionDataBuilder::new().build_from(vec![
            ("return", returns.to_vec()),
            ("score", scores.to_vec()),
        ])?;
        let model = FormulaRegressionBuilder::new()
            .data(&data)
            .formula("return ~ score")
            .fit()?;
        ensure!(
            model.parameters().len() == 2 && model.parameters().iter().all(|v| v.is_finite()),
            "CALIBRATION_FIT_INVALID"
        );
        Ok(Self {
            model,
            observations: scores.len(),
        })
    }

    pub fn predict(&self, scores: &[f64]) -> Result<Vec<f64>> {
        ensure!(
            !scores.is_empty()
                && scores.len() <= MAX_VALIDATION_ROWS
                && scores.iter().all(|value| value.is_finite()),
            "CALIBRATION_INPUT_INVALID"
        );
        let values = self.model.predict(vec![("score", scores.to_vec())])?;
        ensure!(
            values.len() == scores.len() && values.iter().all(|value| value.is_finite()),
            "CALIBRATION_PREDICTION_INVALID"
        );
        Ok(values)
    }

    pub fn coefficients(&self) -> [f64; 2] {
        [self.model.parameters()[0], self.model.parameters()[1]]
    }

    pub fn observations(&self) -> usize {
        self.observations
    }
}
