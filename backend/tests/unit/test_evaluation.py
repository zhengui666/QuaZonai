import pytest

from research_engine.evaluation import benjamini_hochberg, evaluate_alpha, purged_walk_forward


def test_missing_evidence_stays_not_available() -> None:
    result = evaluate_alpha([0.1, 0.2], [None, None], annualization_factor=252, trial_count=2)

    assert result.observation_count == 0
    assert result.sharpe_ratio.value is None
    assert result.sharpe_ratio.status == "NOT_AVAILABLE"
    assert result.net_return.value is None


def test_evaluation_and_trial_adjustment_are_computed_from_real_returns() -> None:
    result = evaluate_alpha(
        [1.0, -1.0, 2.0, -2.0],
        [0.01, -0.02, 0.03, -0.01],
        annualization_factor=1,
        trial_count=4,
    )

    assert result.coverage.value == 1.0
    assert result.ic_mean.value is not None
    assert result.sharpe_ratio.value is not None
    assert result.trial_adjusted_sharpe.value == result.sharpe_ratio.value / 2


def test_purged_walk_forward_never_leaks_train_rows_into_test_rows() -> None:
    folds = purged_walk_forward(
        observation_count=12,
        train_size=4,
        test_size=2,
        purge_size=1,
        embargo_size=1,
    )

    assert folds
    assert all(set(fold.train_indices).isdisjoint(fold.test_indices) for fold in folds)
    assert folds[0].train_indices == (0, 1, 2, 3)
    assert folds[0].test_indices == (5, 6)


def test_benjamini_hochberg_rejects_and_accepts_as_a_family() -> None:
    assert benjamini_hochberg([0.01, 0.04, 0.2], false_discovery_rate=0.05) == (True, False, False)
    with pytest.raises(ValueError, match="p_values"):
        benjamini_hochberg([1.1], false_discovery_rate=0.05)
