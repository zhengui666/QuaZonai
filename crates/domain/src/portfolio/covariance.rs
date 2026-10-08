//! Shared native estimator used by solving and publication.
use anyhow::{Result, ensure};
use ndarray::Array2;
use ndarray_stats::CorrelationExt;

/// Columns are synchronized observation times; rows are a frozen asset ordering.
/// No annualization, missing-value imputation or unregistered shrinkage is performed.
pub fn sample_covariance(
    model: &contracts::portfolio::NativeModelRefV1,
    asset_returns: &[Vec<f64>],
) -> Result<Vec<Vec<f64>>> {
    let parameters = super::sample_covariance_parameters(model)?;
    ensure!(!asset_returns.is_empty(), "COVARIANCE_ASSET_LIMIT");
    let observations = asset_returns[0].len();
    ensure!(observations >= 2, "COVARIANCE_SAMPLE_LIMIT");
    ensure!(
        asset_returns.len().checked_mul(observations).is_some(),
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
