//! Shared native estimator used by solving and publication.
use crate::execution::validation::{MAX_VALIDATION_INDICES, MAX_VALIDATION_ROWS};
use anyhow::{ensure, Result};
use ndarray::Array2;
use ndarray_stats::CorrelationExt;

/// Columns are synchronized observation times; rows are a frozen asset ordering.
/// No annualization, missing-value imputation or unregistered shrinkage is performed.
pub fn sample_covariance(
    model: &contracts::portfolio::NativeModelRefV1,
    asset_returns: &[Vec<f64>],
) -> Result<Vec<Vec<f64>>> {
    let parameters = super::sample_covariance_parameters(model)?;
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
    let covariance = matrix.cov(f64::from(parameters.ddof))?;
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
