//! Native splitter/estimator regression with independent, explicit small references.
use contracts::{
    research::{SplitKind, SplitPolicyV1},
    DbCounter, Id, SchemaV1,
};
use job::validation::validation_folds;

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn policy() -> SplitPolicyV1 {
    SplitPolicyV1 {
        schema_version: SchemaV1,
        kind: SplitKind::WalkForward,
        train_size: count(8),
        test_size: count(2),
        step_size: Some(count(2)),
        group_count: None,
        test_group_count: None,
        purge_observations: count(1),
        embargo_observations: count(1),
        label_horizon_observations: Some(count(1)),
        interval_validation_required: true,
        sealed_revision_id: Id::new(),
    }
}

#[test]
fn native_walk_forward_has_exact_windows_and_never_uses_future_training() {
    let folds = validation_folds(&policy(), 16).unwrap();
    assert_eq!(folds.len(), 3);
    let references = [(0..8, 10..12), (2..10, 12..14), (4..12, 14..16)];
    for (fold, (train, test)) in folds.iter().zip(references) {
        assert_eq!(fold.train, train.collect::<Vec<_>>());
        assert_eq!(fold.test, test.collect::<Vec<_>>());
        assert!(fold.train.iter().all(|&i| i + 2 < fold.test[0]));
    }
}

#[test]
fn a_frozen_step_is_not_silently_replaced_by_test_size() {
    let mut request = policy();
    request.step_size = Some(count(3));
    let folds = validation_folds(&request, 18).unwrap();
    assert_eq!(folds.len(), 3);
    assert_eq!(folds[1].train, (3..11).collect::<Vec<_>>());
    assert_eq!(folds[1].test, vec![13, 14]);
    assert_eq!(folds[2].test, vec![16, 17]);
}

fn cpcv(test_groups: u16) -> SplitPolicyV1 {
    let mut request = policy();
    request.kind = SplitKind::CpcvFixedHorizon;
    request.train_size = count(3);
    request.step_size = None;
    request.group_count = Some(4);
    request.test_group_count = Some(test_groups);
    request
}

#[test]
fn native_cpcv_purges_both_edges_and_embargoes_each_test_block() {
    let folds = validation_folds(&cpcv(1), 20).unwrap();
    assert_eq!(folds.len(), 4);
    assert_eq!(folds[0].test, (0..5).collect::<Vec<_>>());
    assert_eq!(folds[0].train, (7..20).collect::<Vec<_>>());
    assert_eq!(folds[1].test, (5..10).collect::<Vec<_>>());
    assert_eq!(folds[1].train, (0..4).chain(12..20).collect::<Vec<_>>());
    assert_eq!(folds[2].train, (0..9).chain(17..20).collect::<Vec<_>>());
    assert_eq!(folds[3].train, (0..14).collect::<Vec<_>>());
}

#[test]
fn cpcv_really_enumerates_all_six_two_block_combinations() {
    let folds = validation_folds(&cpcv(2), 40).unwrap();
    assert_eq!(folds.len(), 6);
    let blocks = [[0, 1], [0, 2], [0, 3], [1, 2], [1, 3], [2, 3]];
    for (fold, pair) in folds.iter().zip(blocks) {
        let reference: Vec<_> = pair
            .into_iter()
            .flat_map(|block| block * 10..(block + 1) * 10)
            .collect();
        assert_eq!(fold.test, reference);
        assert!(!fold.train.is_empty());
        for &train in &fold.train {
            assert!(!fold.test.contains(&train));
        }
    }
}

#[test]
fn invalid_horizons_budgets_and_combinatorial_bombs_are_rejected() {
    let mut request = policy();
    request.label_horizon_observations = None;
    assert!(validation_folds(&request, 100).is_err());
    request = policy();
    request.label_horizon_observations = Some(count(2));
    assert!(validation_folds(&request, 100).is_err());
    request = cpcv(17);
    request.group_count = Some(34);
    assert!(validation_folds(&request, 1000).is_err());
    request = cpcv(2);
    request.group_count = Some(u16::MAX);
    assert!(validation_folds(&request, 1000).is_err());
    request = policy();
    request.train_size = count(i64::MAX as u64);
    assert!(validation_folds(&request, 1000).is_err());
    assert!(validation_folds(&policy(), 0).is_err());
    assert!(validation_folds(&policy(), 1_000_001).is_err());
    assert!(validation_folds(&policy(), 11).is_err());
    assert!(validation_folds(&policy(), 1_000_000).is_err());
}

#[test]
fn large_training_sample_keeps_the_frozen_gap_and_complete_test_window() {
    let mut request = policy();
    request.train_size = count(1_000_001);
    let folds = validation_folds(&request, 1_000_005).unwrap();
    assert_eq!(folds.len(), 1);
    assert_eq!(folds[0].train.len(), 1_000_001);
    assert_eq!(folds[0].train.first(), Some(&0));
    assert_eq!(folds[0].train.last(), Some(&1_000_000));
    assert_eq!(folds[0].test, vec![1_000_003, 1_000_004]);
    assert!(folds[0]
        .train
        .iter()
        .all(|&index| index + 2 < folds[0].test[0]));
}

#[test]
fn validation_keeps_every_fold_above_the_old_256_and_16_group_caps() {
    let folds = validation_folds(&policy(), 526).unwrap();
    assert_eq!(folds.len(), 258);
    assert_eq!(folds.last().unwrap().test, vec![524, 525]);
    let mut request = cpcv(1);
    request.group_count = Some(17);
    let folds = validation_folds(&request, 51).unwrap();
    assert_eq!(folds.len(), 17);
    assert_eq!(folds[0].test, vec![0, 1, 2]);
    assert_eq!(folds[16].test, vec![48, 49, 50]);
}
