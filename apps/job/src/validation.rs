//! Thin adapters over native splitters and estimators; no qualification authority.
use anyhow::{ensure, Result};
use contracts::research::{SplitKind, SplitPolicyV1};
use linregress::{FormulaRegressionBuilder, RegressionDataBuilder, RegressionModel};
use ndarray::Array2;
use ndarray_stats::CorrelationExt;
use solow_cv::{CombinatorialPurgedKFold, Split, Splitter, TimeSeriesSplit};

pub const MAX_VALIDATION_ROWS: usize = 1_000_000;
pub const MAX_VALIDATION_FOLDS: usize = 256;
pub const MAX_VALIDATION_INDICES: usize = 8_000_000;

fn bounded_count(value: contracts::DbCounter) -> Result<usize> {
    let count = usize::try_from(value.get())?;
    ensure!(count <= MAX_VALIDATION_ROWS, "VALIDATION_COUNT_LIMIT");
    Ok(count)
}

/// `rows` counts observations whose labels have completed, not every market row.
/// Caller owns PIT and isolated feature/model state for each native fold.
pub fn validation_folds(policy: &SplitPolicyV1, rows: usize) -> Result<Vec<Split>> {
    domain::research::split(policy)?;
    ensure!(
        (1..=MAX_VALIDATION_ROWS).contains(&rows),
        "VALIDATION_ROW_LIMIT"
    );
    let train_size = bounded_count(policy.train_size)?;
    let test_size = bounded_count(policy.test_size)?;
    let purge = bounded_count(policy.purge_observations)?;
    let embargo = bounded_count(policy.embargo_observations)?;
    let horizon = bounded_count(
        policy
            .label_horizon_observations
            .ok_or_else(|| anyhow::anyhow!("UNSUPPORTED_LABEL_INTERVALS"))?,
    )?;
    ensure!(horizon > 0 && purge >= horizon, "LABEL_PURGE_TOO_SHORT");
    ensure!(
        train_size >= 3 && test_size > 0,
        "INSUFFICIENT_FOLD_SAMPLES"
    );
    let folds = match policy.kind {
        SplitKind::WalkForward => {
            let gap = purge
                .checked_add(embargo)
                .ok_or_else(|| anyhow::anyhow!("VALIDATION_COUNT_LIMIT"))?;
            ensure!(
                train_size > test_size + gap,
                "NATIVE_ROLLING_WINDOW_UNSUPPORTED"
            );
            let first_end = train_size
                .checked_add(gap)
                .and_then(|n| n.checked_add(test_size))
                .ok_or_else(|| anyhow::anyhow!("VALIDATION_COUNT_LIMIT"))?;
            ensure!(first_end <= rows, "INSUFFICIENT_FOLD_SAMPLES");
            let step = bounded_count(
                policy
                    .step_size
                    .ok_or_else(|| anyhow::anyhow!("MISSING_STEP"))?,
            )?;
            ensure!(step > 0, "MISSING_STEP");
            let count = (rows - first_end) / step + 1;
            ensure!(count <= MAX_VALIDATION_FOLDS, "VALIDATION_FOLD_LIMIT");
            ensure!(
                count
                    .checked_mul(2 * (train_size + test_size))
                    .is_some_and(|n| n <= MAX_VALIDATION_INDICES),
                "VALIDATION_INDEX_LIMIT"
            );
            let splitter = TimeSeriesSplit::new(2)?
                .test_size(test_size)
                .gap(gap)
                .max_train_size(train_size);
            let mut result = Vec::with_capacity(count);
            for index in 0..count {
                let end = first_end + index * step;
                let mut native = splitter.split(end)?;
                let fold = native
                    .pop()
                    .ok_or_else(|| anyhow::anyhow!("NATIVE_SPLIT_MISSING"))?;
                ensure!(
                    fold.train.len() == train_size && fold.test.len() == test_size,
                    "NATIVE_ROLLING_WINDOW_MISMATCH"
                );
                ensure!(
                    fold.train
                        .last()
                        .is_some_and(|&last| last + gap < fold.test[0]),
                    "NATIVE_ROLLING_WINDOW_LEAK"
                );
                result.push(fold);
            }
            result
        }
        SplitKind::CpcvFixedHorizon => {
            let groups = usize::from(
                policy
                    .group_count
                    .ok_or_else(|| anyhow::anyhow!("MISSING_GROUPS"))?,
            );
            let test_groups = usize::from(
                policy
                    .test_group_count
                    .ok_or_else(|| anyhow::anyhow!("MISSING_GROUPS"))?,
            );
            // Bounds also protect the upstream binomial/allocation arithmetic.
            ensure!(
                (2..=16).contains(&groups) && test_groups > 0 && test_groups < groups,
                "VALIDATION_GROUP_LIMIT"
            );
            let splitter = CombinatorialPurgedKFold::new(groups, test_groups, purge, embargo)?;
            let count = splitter.n_folds();
            ensure!(count <= MAX_VALIDATION_FOLDS, "VALIDATION_FOLD_LIMIT");
            ensure!(
                count
                    .checked_mul(rows)
                    .is_some_and(|n| n <= MAX_VALIDATION_INDICES),
                "VALIDATION_INDEX_LIMIT"
            );
            let native = splitter.split(rows)?;
            ensure!(native.len() == count, "NATIVE_SPLIT_COUNT_MISMATCH");
            native
        }
    };
    for fold in &folds {
        ensure!(
            fold.train.len() >= train_size && fold.test.len() >= test_size,
            "INSUFFICIENT_FOLD_SAMPLES"
        );
        for indices in [&fold.train, &fold.test] {
            ensure!(
                !indices.is_empty()
                    && indices.iter().all(|&i| i < rows)
                    && indices.windows(2).all(|pair| pair[0] < pair[1]),
                "NATIVE_SPLIT_INVALID"
            );
        }
        ensure!(
            fold.train
                .iter()
                .all(|i| fold.test.binary_search(i).is_err()),
            "NATIVE_SPLIT_LEAK"
        );
    }
    Ok(folds)
}

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
