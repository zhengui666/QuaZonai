//! Existing account forecast arithmetic and split adapters; no model fitting.
use anyhow::{ensure, Result};
use ndarray::Array2;

/// Apply saved native OLS coefficients; linregress 0.5.4 has no model decoder.
/// No labels or fitting inputs are accepted by this fixed ndarray operation.
pub fn predict_frozen_calibration(
    model: &contracts::science::NativeFrozenCalibrationV1,
    bar_type: &str,
    horizon: contracts::DbCounter,
    first_prediction_available_ns: contracts::DbCounter,
    scores: &[f64],
) -> Result<Vec<f64>> {
    domain::execution::check_alpha_calibration(model)?;
    ensure!(
        horizon == model.horizon_observations
            && first_prediction_available_ns > model.fit_end_available_ns,
        "CALIBRATION_INPUT_TIME_OR_HORIZON"
    );
    ensure!(
        !scores.is_empty() && scores.iter().all(|v| v.is_finite()),
        "CALIBRATION_INPUT_INVALID"
    );
    let fit = &model
        .assets
        .iter()
        .find(|a| a.bar_type == bar_type)
        .ok_or_else(|| anyhow::anyhow!("CALIBRATION_ASSET_MISSING"))?
        .calibration;
    let values = &ndarray::ArrayView1::from(scores) * fit.slope.unwrap() + fit.intercept.unwrap();
    ensure!(
        values.iter().all(|v| v.is_finite()),
        "CALIBRATION_PREDICTION_INVALID"
    );
    Ok(values.into_raw_vec_and_offset().0)
}

pub use domain::execution::validation::{validation_folds, MAX_VALIDATION_FOLDS};

/// Pure numerical adapter: callers must bind distinct qualified Alpha versions,
/// common units/horizon/cutoff and this exact asset order before invoking it.
/// Fixed mixture weights are configuration, not fitted calibration or targets.
pub fn fixed_weighted_forecast(
    forecasts: &[Vec<f64>],
    weights: &[contracts::DecimalValue],
) -> Result<Vec<f64>> {
    weighted_forecast(
        &forecasts.iter().map(Vec::as_slice).collect::<Vec<_>>(),
        &weights.iter().collect::<Vec<_>>(),
    )
}

fn weighted_forecast(
    forecasts: &[&[f64]],
    weights: &[&contracts::DecimalValue],
) -> Result<Vec<f64>> {
    use bigdecimal::ToPrimitive;
    ensure!(
        forecasts.len() >= 2 && forecasts.len() == weights.len(),
        "ENSEMBLE_MEMBER_LIMIT"
    );
    let assets = forecasts[0].len();
    ensure!(
        assets > 0
            && forecasts
                .iter()
                .all(|row| row.len() == assets && row.iter().all(|v| v.is_finite())),
        "ENSEMBLE_FORECAST_INVALID"
    );
    domain::portfolio::ensemble_weights(weights.iter().copied())?;
    let weights = weights
        .iter()
        .map(|w| {
            let native = w
                .as_decimal()
                .to_f64()
                .ok_or_else(|| anyhow::anyhow!("ENSEMBLE_WEIGHT_RANGE"))?;
            ensure!(
                native.is_finite() && (!w.is_positive() || native > 0.0),
                "ENSEMBLE_WEIGHT_RANGE"
            );
            Ok(native)
        })
        .collect::<Result<Vec<_>>>()?;
    let matrix = Array2::from_shape_vec(
        (forecasts.len(), assets),
        forecasts
            .iter()
            .flat_map(|row| row.iter().copied())
            .collect(),
    )?;
    let forecast = ndarray::ArrayView1::from(&weights).dot(&matrix);
    ensure!(
        forecast.iter().all(|v| v.is_finite()),
        "ENSEMBLE_RESULT_NONFINITE"
    );
    Ok(forecast.into_raw_vec_and_offset().0)
}

/// Common ordering and metadata are checked before any aggregation. This does not
/// turn a caller-supplied UUID or unit label into authoritative evidence.
pub fn aligned_portfolio_forecast(
    input: &contracts::portfolio::PortfolioForecastInputV1,
) -> Result<Vec<f64>> {
    domain::portfolio::portfolio_forecast_alignment(input)?;
    let bars = crate::catalog::bar_types(&input.bar_types)?;
    ensure!(
        bars.iter()
            .zip(&input.instrument_ids)
            .all(
                |(bar, instrument)| bar.instrument_id().to_string() == *instrument
                    && bar.spec() == bars[0].spec()
            ),
        "ENSEMBLE_BAR_CONTRACT_MISMATCH"
    );
    weighted_forecast(
        &input
            .members
            .iter()
            .map(|member| member.forecasts.as_slice())
            .collect::<Vec<_>>(),
        &input
            .members
            .iter()
            .map(|member| &member.ensemble_weight)
            .collect::<Vec<_>>(),
    )
}
