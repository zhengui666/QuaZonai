use super::*;
use contracts::portfolio::*;

#[test]
fn native_conic_solution_meets_correlated_contribution_tolerance() {
    let input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let budget = RiskBudgetSettingsV1 {
        schema_version: contracts::SchemaV1,
        risky_gross_exposure: "1".parse().unwrap(),
        assets: input
            .assets
            .iter()
            .zip(["0.2", "0.8"])
            .map(|(a, share)| RiskBudgetAssetV1 {
                instrument_id: a.instrument_id.clone(),
                share: share.parse().unwrap(),
                sign: RiskBudgetSign::Long,
            })
            .collect(),
    };
    let native = risk_budgeting::solve(
        &input,
        &[vec![1.0, 1.0], vec![1.0, 5.0]],
        &Cholesky::new(DMatrix::from_row_slice(2, 2, &[1.0, 1.0, 1.0, 5.0]))
            .unwrap()
            .l(),
        &budget,
        domain::portfolio::optimizer_settings(&input.optimizer).unwrap(),
    )
    .unwrap();
    assert_eq!(native.status, clarabel::solver::SolverStatus::Solved);
    let w = &native.x;
    let variance = w[0] * w[0] + 2.0 * w[0] * w[1] + 5.0 * w[1] * w[1];
    let share = w[0] * (w[0] + w[1]) / variance;
    assert!(
        (share - 0.2).abs() <= 1e-6,
        "share={share:.16}, weights={:?}, status={:?}",
        &w[..2],
        native.status
    );
}

#[test]
fn native_conic_catalog_scale() {
    let input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let returns: Vec<Vec<f64>> = [1.0, 2.0]
        .into_iter()
        .map(|base| {
            (1..=18)
                .map(|i| (base + (i + 2) as f64 * 0.001) / (base + i as f64 * 0.001) - 1.0)
                .collect()
        })
        .collect();
    let covariance =
        domain::portfolio::sample_covariance(&input.covariance_estimator, &returns).unwrap();
    let budget = RiskBudgetSettingsV1 {
        schema_version: SchemaV1,
        risky_gross_exposure: "1".parse().unwrap(),
        assets: input
            .assets
            .iter()
            .map(|a| RiskBudgetAssetV1 {
                instrument_id: a.instrument_id.clone(),
                share: "0.5".parse().unwrap(),
                sign: RiskBudgetSign::Long,
            })
            .collect(),
    };
    let lower = Cholesky::new(DMatrix::from_fn(2, 2, |i, j| covariance[i][j]))
        .unwrap()
        .l();
    let native = risk_budgeting::solve(
        &input,
        &covariance,
        &lower,
        &budget,
        domain::portfolio::optimizer_settings(&input.optimizer).unwrap(),
    )
    .unwrap();
    let w = &native.x;
    let contribution = w[0] * (covariance[0][0] * w[0] + covariance[0][1] * w[1]);
    let variance = contribution + w[1] * (covariance[1][0] * w[0] + covariance[1][1] * w[1]);
    assert_eq!(native.status, clarabel::solver::SolverStatus::Solved);
    assert!(
        (contribution / variance - 0.5).abs() <= 1e-6,
        "share={}, covariance={covariance:?}, weights={:?}",
        contribution / variance,
        &w[..2]
    );
}
