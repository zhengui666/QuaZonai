//! Shared bounded adapter to the pinned native splitters. No model execution or qualification.
use anyhow::{ensure, Result};
use contracts::research::{SplitKind, SplitPolicyV1};
use solow_cv::{CombinatorialPurgedKFold, Split, Splitter, TimeSeriesSplit};

pub const MAX_VALIDATION_ROWS: usize = 1_000_000;
pub const MAX_VALIDATION_FOLDS: usize = 256;
pub const MAX_VALIDATION_INDICES: usize = 8_000_000;

/// Executable parameter bounds shared by Brief admission and the native task.
/// This does not assert that unseen market rows contain enough eligible samples.
pub fn policy_parameters(policy: &SplitPolicyV1, horizon: u64) -> Result<(), crate::DomainError> {
    crate::research::split(policy)?;
    if !(1..=100_000).contains(&horizon)
        || policy.label_horizon_observations.map(|n| n.get()) != Some(horizon)
        || policy.purge_observations.get() < horizon
        || policy.train_size.get() < 3
        || [
            policy.train_size,
            policy.test_size,
            policy.purge_observations,
            policy.embargo_observations,
        ]
        .into_iter()
        .chain(policy.step_size)
        .any(|n| n.get() > MAX_VALIDATION_ROWS as u64)
        || policy.group_count.is_some_and(|n| n > 16)
        || (policy.kind == SplitKind::WalkForward
            && policy.train_size.get()
                <= policy.test_size.get()
                    + policy.purge_observations.get()
                    + policy.embargo_observations.get())
    {
        return Err(super::bad("validation_parameters"));
    }
    if policy.kind == SplitKind::CpcvFixedHorizon {
        let native = CombinatorialPurgedKFold::new(
            usize::from(
                policy
                    .group_count
                    .ok_or_else(|| super::bad("validation_parameters"))?,
            ),
            usize::from(
                policy
                    .test_group_count
                    .ok_or_else(|| super::bad("validation_parameters"))?,
            ),
            policy.purge_observations.get() as usize,
            policy.embargo_observations.get() as usize,
        )
        .map_err(|_| super::bad("validation_parameters"))?;
        if native.n_folds() > MAX_VALIDATION_FOLDS {
            return Err(super::bad("validation_parameters"));
        }
    }
    Ok(())
}

fn bounded_count(value: contracts::DbCounter) -> Result<usize> {
    let count = usize::try_from(value.get())?;
    ensure!(count <= MAX_VALIDATION_ROWS, "VALIDATION_COUNT_LIMIT");
    Ok(count)
}

/// `rows` counts observations whose labels have completed, not every market row.
/// Caller owns PIT and isolated feature/model state for each native fold.
pub fn validation_folds(policy: &SplitPolicyV1, rows: usize) -> Result<Vec<Split>> {
    crate::research::split(policy)?;
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
