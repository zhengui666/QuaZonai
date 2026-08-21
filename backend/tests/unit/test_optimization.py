from __future__ import annotations

from optimization import TrialPoint, pareto_front, select_compromise


def test_pareto_front_respects_mixed_directions() -> None:
    points = [
        TrialPoint(0, (10.0, 5.0)),
        TrialPoint(1, (12.0, 7.0)),
        TrialPoint(2, (9.0, 4.0)),
        TrialPoint(3, (11.0, 4.5)),
    ]
    front = pareto_front(points, ("maximize", "minimize"))
    assert [point.trial_no for point in front] == [1, 2, 3]


def test_ideal_point_selection_is_deterministic_and_ties_by_trial_number() -> None:
    points = [
        TrialPoint(8, (1.0, 1.0)),
        TrialPoint(3, (1.0, 1.0)),
    ]
    selected = select_compromise(points, ("maximize", "maximize"))
    assert selected.trial_no == 3
