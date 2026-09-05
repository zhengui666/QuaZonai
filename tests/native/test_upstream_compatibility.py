"""Independent upstream compatibility tests, not a Python product runtime.

These tests use fixed synthetic inputs and no application/production credentials.
The Rust probe must independently pass the same numerical reference; passing
these tests cannot substitute for the Rust -> PyO3 execution path.
"""

import unittest

import cvxpy as cp
import numpy as np
from skfolio import RiskMeasure
from skfolio.optimization import MeanRisk, ObjectiveFunction


class NativeCompatibilityTest(unittest.TestCase):
    def test_direct_cvxpy_clarabel_reference(self):
        weights = cp.Variable(2)
        problem = cp.Problem(
            cp.Minimize(cp.quad_form(weights, np.diag([1.0, 4.0]))),
            [cp.sum(weights) == 1.0, weights >= 0.0, weights <= 1.0],
        )
        problem.solve(solver="CLARABEL")
        self.assertEqual(problem.status, cp.OPTIMAL)
        np.testing.assert_allclose(weights.value, [0.8, 0.2], atol=1e-5, rtol=0)

    def test_skfolio_minimum_variance_reference(self):
        returns = np.tile(
            np.array([[-0.01, -0.02], [-0.01, 0.02], [0.01, -0.02], [0.01, 0.02]]),
            (20, 1),
        )
        estimator = MeanRisk(
            objective_function=ObjectiveFunction.MINIMIZE_RISK,
            risk_measure=RiskMeasure.VARIANCE,
            min_weights=0.0,
            max_weights=1.0,
            budget=1.0,
            solver="CLARABEL",
        )
        estimator.fit(returns)
        np.testing.assert_allclose(estimator.weights_, [0.8, 0.2], atol=1e-5, rtol=0)


if __name__ == "__main__":
    unittest.main(verbosity=2)
