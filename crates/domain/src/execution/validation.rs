//! Shared checked adapter to the pinned native splitters. No model execution or qualification.
use anyhow::{Result, ensure};
use contracts::research::{SplitKind, SplitPolicyV1};
use solow_cv::{CombinatorialPurgedKFold, Split, Splitter, TimeSeriesSplit};

// The result wire stores each fold ordinal as u16. This is a representation
// boundary, not a research budget; every admitted fold is emitted.
pub const MAX_VALIDATION_FOLDS: usize = u16::MAX as usize + 1;

fn checked_fold_count(groups: usize, test_groups: usize) -> Result<usize> {
    ensure!(
        groups >= 2 && test_groups > 0 && test_groups < groups,
        "VALIDATION_GROUP_COUNTS"
    );
    // Match the pinned native recurrence, proving every intermediate safe before
    // entering its unchecked binomial calculation or allocating split vectors.
    let mut count = 1_usize;
    for index in 0..test_groups.min(groups - test_groups) {
        count = count
            .checked_mul(groups - index)
            .ok_or_else(|| anyhow::anyhow!("VALIDATION_FOLD_COUNT_OVERFLOW"))?
            / (index + 1);
        ensure!(count <= MAX_VALIDATION_FOLDS, "VALIDATION_FOLD_INDEX_RANGE");
    }
    Ok(count)
}

pub fn capabilities(
    value: &contracts::runtime::RuntimeCapabilitiesV1,
) -> Result<(), crate::DomainError> {
    if !value.label_interval_support.fixed_bars
        || !value
            .artifact_schemas
            .iter()
            .any(|s| s.name == "qz.alpha_validation" && s.version == "1")
        || [
            ("solow-cv", "0.7.3"),
            ("ndarray-stats", "0.7.0"),
            ("linregress", "0.5.4"),
        ]
        .into_iter()
        .any(|(name, version)| value.engine_versions.get(name).map(String::as_str) != Some(version))
    {
        return Err(crate::DomainError::CapabilityUnavailable(
            "native_alpha_validation",
        ));
    }
    Ok(())
}

/// Executable parameter bounds shared by Brief admission and the native task.
/// This does not assert that unseen market rows contain enough eligible samples.
pub fn policy_parameters(policy: &SplitPolicyV1, horizon: u64) -> Result<(), crate::DomainError> {
    crate::research::split(policy)?;
    if horizon == 0
        || policy.label_horizon_observations.map(|n| n.get()) != Some(horizon)
        || policy.purge_observations.get() < horizon
        || policy.train_size.get() < 3
        || (policy.kind == SplitKind::WalkForward
            && policy.train_size.get()
                <= policy.test_size.get()
                    + policy.purge_observations.get()
                    + policy.embargo_observations.get())
    {
        return Err(super::bad("validation_parameters"));
    }
    if policy.kind == SplitKind::CpcvFixedHorizon {
        checked_fold_count(
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
        )
        .map_err(|_| super::bad("validation_fold_index_range"))?;
    }
    Ok(())
}

fn bounded_count(value: contracts::DbCounter) -> Result<usize> {
    let count = usize::try_from(value.get())?;
    Ok(count)
}

/// `rows` counts observations whose labels have completed, not every market row.
/// Caller owns PIT and isolated feature/model state for each native fold.
pub fn validation_folds(policy: &SplitPolicyV1, rows: usize) -> Result<Vec<Split>> {
    crate::research::split(policy)?;
    ensure!(rows > 0, "VALIDATION_ROW_LIMIT");
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
            ensure!(count <= MAX_VALIDATION_FOLDS, "VALIDATION_FOLD_INDEX_RANGE");
            ensure!(
                count
                    .checked_mul(
                        train_size
                            .checked_add(test_size)
                            .and_then(|n| n.checked_mul(2))
                            .ok_or_else(|| anyhow::anyhow!("VALIDATION_COUNT_OVERFLOW"))?
                    )
                    .is_some(),
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
            let count = checked_fold_count(groups, test_groups)?;
            // solow-cv 0.7.3 calculates k*rows and end+purge+embargo using
            // native usize arithmetic. Reject non-representable inputs first.
            ensure!(
                groups.checked_mul(rows).is_some()
                    && rows
                        .checked_add(purge)
                        .and_then(|n| n.checked_add(embargo))
                        .is_some()
                    && count.checked_mul(rows).is_some(),
                "VALIDATION_INDEX_RANGE"
            );
            let splitter = CombinatorialPurgedKFold::new(groups, test_groups, purge, embargo)?;
            ensure!(splitter.n_folds() == count, "NATIVE_SPLIT_COUNT_MISMATCH");
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

#[cfg(test)]
mod representation_tests {
    use super::*;

    #[test]
    fn combinations_are_checked_before_native_allocation_without_magic_group_limits() {
        assert_eq!(checked_fold_count(16, 8).unwrap(), 12_870);
        assert_eq!(checked_fold_count(17, 1).unwrap(), 17);
        assert_eq!(checked_fold_count(65_535, 1).unwrap(), 65_535);
        assert_eq!(checked_fold_count(65_535, 65_534).unwrap(), 65_535);
        assert!(checked_fold_count(34, 17).is_err());
        assert!(checked_fold_count(65_535, 2).is_err());
        assert!(checked_fold_count(1, 1).is_err());
        assert!(checked_fold_count(17, 0).is_err());
        assert_eq!(MAX_VALIDATION_FOLDS, 65_536);
    }
}
