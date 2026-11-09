//! Quality-adjusted constrained diversification, using the official Clarabel QP.
//! Input report authority, time alignment and candidate freezing belong to the host.
use anyhow::{ensure, Result};
use bigdecimal::{BigDecimal, ToPrimitive};
use clarabel::{algebra::CscMatrix, solver::*};
use contracts::{review_allocation::*, DecimalValue, SchemaV1};
use ndarray::Array2;
use ndarray_stats::CorrelationExt;
use std::collections::{BTreeMap, BTreeSet};

fn decimal(value: &DecimalValue) -> Result<f64> {
    value
        .as_decimal()
        .to_f64()
        .filter(|v| v.is_finite())
        .ok_or_else(|| anyhow::anyhow!("REVIEW_ALLOCATION_DECIMAL_RANGE"))
}
fn id(value: &str) -> bool {
    !value.trim().is_empty() && value.len() <= 512
}

pub fn allocate(
    request: &NativeReviewAllocationRequestV1,
) -> Result<NativeReviewAllocationResultV1> {
    let n = request.members.len();
    ensure!(n >= 2, "REVIEW_ALLOCATION_MEMBERS_REQUIRED");
    let samples = request.members[0].observations.len();
    ensure!(samples >= 30, "REVIEW_ALLOCATION_SAMPLE_LIMIT");
    let mut ids = BTreeMap::new();
    let mut report_ids = BTreeSet::new();
    for (i, member) in request.members.iter().enumerate() {
        ensure!(
            id(&member.id)
                && id(&member.source_report_id)
                && ids.insert(member.id.clone(), i).is_none()
                && report_ids.insert(&member.source_report_id),
            "REVIEW_ALLOCATION_MEMBER_ID"
        );
        ensure!(
            member.score.is_finite() && (80.0..=100.0).contains(&member.score),
            "REVIEW_ALLOCATION_UNAPPROVED_SCORE"
        );
        ensure!(
            member.observations.len() == samples
                && member.observations.iter().all(|v| v.is_finite()),
            "REVIEW_ALLOCATION_PANEL_INVALID"
        );
        ensure!(
            member
                .observations
                .iter()
                .any(|value| *value != member.observations[0]),
            "REVIEW_ALLOCATION_ZERO_RISK"
        );
    }
    ensure!(
        request.member_caps.len() == n && request.current_weights.len() == n,
        "REVIEW_ALLOCATION_CONSTRAINTS_REQUIRED"
    );
    let mut caps = vec![None; n];
    let mut current = vec![None; n];
    for cap in &request.member_caps {
        let i = *ids
            .get(&cap.id)
            .ok_or_else(|| anyhow::anyhow!("REVIEW_ALLOCATION_CAP_ID"))?;
        ensure!(
            caps[i].is_none() && cap.maximum_weight.is_fraction(),
            "REVIEW_ALLOCATION_CAP_INVALID"
        );
        caps[i] = Some(decimal(&cap.maximum_weight)?);
    }
    for weight in &request.current_weights {
        let i = *ids
            .get(&weight.id)
            .ok_or_else(|| anyhow::anyhow!("REVIEW_ALLOCATION_CURRENT_ID"))?;
        ensure!(
            current[i].is_none() && weight.weight.is_fraction(),
            "REVIEW_ALLOCATION_CURRENT_INVALID"
        );
        current[i] = Some(decimal(&weight.weight)?);
    }
    ensure!(
        caps.iter().all(Option::is_some) && current.iter().all(Option::is_some),
        "REVIEW_ALLOCATION_CONSTRAINTS_INCOMPLETE"
    );
    let caps: Vec<_> = caps.into_iter().map(Option::unwrap).collect();
    let current: Vec<_> = current.into_iter().map(Option::unwrap).collect();
    ensure!(
        current.iter().sum::<f64>() <= 1.0 + 1e-12,
        "REVIEW_ALLOCATION_CURRENT_SUM"
    );
    let turnover = decimal(&request.maximum_turnover)?;
    ensure!(
        (0.0..=2.0).contains(&turnover),
        "REVIEW_ALLOCATION_TURNOVER_INVALID"
    );
    let mut cluster_ids = BTreeSet::new();
    let mut clusters = Vec::new();
    for cluster in &request.cluster_caps {
        ensure!(
            id(&cluster.id)
                && cluster_ids.insert(&cluster.id)
                && !cluster.members.is_empty()
                && cluster.maximum_weight.is_fraction(),
            "REVIEW_ALLOCATION_CLUSTER_INVALID"
        );
        let members: BTreeSet<_> = cluster
            .members
            .iter()
            .map(|member| {
                ids.get(member)
                    .copied()
                    .ok_or_else(|| anyhow::anyhow!("REVIEW_ALLOCATION_CLUSTER_MEMBER"))
            })
            .collect::<Result<_>>()?;
        ensure!(
            members.len() == cluster.members.len(),
            "REVIEW_ALLOCATION_CLUSTER_DUPLICATE"
        );
        clusters.push((members, decimal(&cluster.maximum_weight)?));
    }
    let panel = Array2::from_shape_vec(
        (n, samples),
        request
            .members
            .iter()
            .flat_map(|member| member.observations.iter().copied())
            .collect(),
    )?;
    // Match the frozen sample estimator (ddof=1); no annualization or imputation.
    let native_covariance = panel.cov(1.0)?;
    ensure!(
        native_covariance.iter().all(|value| value.is_finite()),
        "REVIEW_ALLOCATION_COVARIANCE_RANGE"
    );
    let mut covariance: Vec<Vec<f64>> = native_covariance
        .rows()
        .into_iter()
        .map(|row| row.to_vec())
        .collect();
    let deviations: Vec<_> = (0..n).map(|i| covariance[i][i].sqrt()).collect();
    ensure!(
        deviations.iter().all(|v| v.is_finite() && *v > 0.0),
        "REVIEW_ALLOCATION_ZERO_RISK"
    );
    if request.kind == NativeReviewAllocationKindV1::Factor {
        for i in 0..n {
            for j in 0..n {
                covariance[i][j] /= deviations[i] * deviations[j];
            }
        }
    }
    // Registered fixed diagonal shrinkage; never described as an estimated Ledoit-Wolf coefficient.
    for (i, row) in covariance.iter_mut().enumerate() {
        for (j, value) in row.iter_mut().enumerate() {
            if i != j {
                *value *= 0.9;
            }
        }
    }
    let lambda = (0..n).map(|i| covariance[i][i]).sum::<f64>() / n as f64;
    ensure!(
        lambda.is_finite() && lambda > 0.0,
        "REVIEW_ALLOCATION_LAMBDA_RANGE"
    );
    let qualities: Vec<_> = request
        .members
        .iter()
        .map(|m| ((m.score - 60.0) / 40.0).clamp(0.0, 1.0))
        .collect();
    let mut prior: Vec<_> = qualities
        .iter()
        .enumerate()
        .map(|(i, q)| {
            if request.kind == NativeReviewAllocationKindV1::Portfolio {
                q / deviations[i]
            } else {
                *q
            }
        })
        .collect();
    let sum = prior.iter().sum::<f64>();
    ensure!(
        sum.is_finite() && sum > 0.0,
        "REVIEW_ALLOCATION_PRIOR_RANGE"
    );
    for p in &mut prior {
        *p /= sum;
    }
    // Variables are weights and nonnegative absolute-turnover witnesses.
    let vars = n * 2;
    let mut pr = Vec::new();
    let mut pc = Vec::new();
    let mut pv = Vec::new();
    for (i, row) in covariance.iter().enumerate() {
        for (j, value) in row.iter().enumerate().skip(i) {
            pr.push(i);
            pc.push(j);
            pv.push(2.0 * value + if i == j { 2.0 * lambda } else { 0.0 });
        }
    }
    let p = CscMatrix::new_from_triplets(vars, vars, pr, pc, pv);
    let mut q = vec![0.0; vars];
    for i in 0..n {
        q[i] = -2.0 * lambda * prior[i];
    }
    let mut ar = Vec::new();
    let mut ac = Vec::new();
    let mut av = Vec::new();
    let mut b = vec![1.0];
    for i in 0..n {
        ar.push(0);
        ac.push(i);
        av.push(1.0);
    }
    let mut add_row = |coefficients: &[(usize, f64)], bound: f64| {
        let row = b.len();
        for (col, value) in coefficients {
            ar.push(row);
            ac.push(*col);
            av.push(*value);
        }
        b.push(bound);
    };
    for i in 0..n {
        add_row(&[(i, -1.0)], 0.0);
        add_row(&[(i, 1.0)], caps[i]);
        add_row(&[(n + i, -1.0)], 0.0);
        add_row(&[(i, 1.0), (n + i, -1.0)], current[i]);
        add_row(&[(i, -1.0), (n + i, -1.0)], -current[i]);
    }
    for (members, cap) in &clusters {
        add_row(&members.iter().map(|i| (*i, 1.0)).collect::<Vec<_>>(), *cap);
    }
    add_row(&(0..n).map(|i| (n + i, 1.0)).collect::<Vec<_>>(), turnover);
    let rows = b.len();
    let a = CscMatrix::new_from_triplets(rows, vars, ar, ac, av);
    let settings = DefaultSettingsBuilder::default()
        .verbose(false)
        .tol_gap_abs(1e-12)
        .tol_gap_rel(1e-12)
        .tol_feas(1e-12)
        .max_iter(200)
        .build()?;
    let mut solver = DefaultSolver::new(
        &p,
        &q,
        &a,
        &b,
        &[ZeroConeT(1), NonnegativeConeT(rows - 1)],
        settings,
    )?;
    solver.solve();
    let s = &solver.solution;
    ensure!(
        s.status == SolverStatus::Solved && s.x.len() == vars,
        "REVIEW_ALLOCATION_NOT_OPTIMAL"
    );
    ensure!(
        s.x.iter().all(|v| v.is_finite()) && s.r_prim.is_finite() && s.r_dual.is_finite(),
        "REVIEW_ALLOCATION_SOLUTION_RANGE"
    );
    let scale = 1_000_000_000_000_i64;
    let mut units: Vec<i64> = s.x[..n]
        .iter()
        .map(|w| (w * scale as f64).round() as i64)
        .collect();
    let largest = (0..n).max_by(|a, b| s.x[*a].total_cmp(&s.x[*b])).unwrap();
    units[largest] += scale - units.iter().sum::<i64>();
    ensure!(
        units.iter().all(|v| *v >= 0),
        "REVIEW_ALLOCATION_DECIMAL_NEGATIVE"
    );
    ensure!(
        request.kind != NativeReviewAllocationKindV1::Factor
            || units.iter().filter(|w| **w > 0).count() >= 2,
        "REVIEW_ALLOCATION_FACTOR_DIVERSITY"
    );
    let decimal_weights: Vec<_> = units
        .iter()
        .map(|u| BigDecimal::new((*u).into(), 12))
        .collect();
    for cap in &request.member_caps {
        ensure!(
            decimal_weights[ids[&cap.id]] <= *cap.maximum_weight.as_decimal(),
            "REVIEW_ALLOCATION_DECIMAL_CAP"
        );
    }
    for cluster in &request.cluster_caps {
        let used = cluster
            .members
            .iter()
            .map(|id| &decimal_weights[ids[id]])
            .sum::<BigDecimal>();
        ensure!(
            used <= *cluster.maximum_weight.as_decimal(),
            "REVIEW_ALLOCATION_DECIMAL_CLUSTER"
        );
    }
    let exact_turnover: BigDecimal = request
        .current_weights
        .iter()
        .map(|w| (&decimal_weights[ids[&w.id]] - w.weight.as_decimal()).abs())
        .sum();
    ensure!(
        exact_turnover <= *request.maximum_turnover.as_decimal(),
        "REVIEW_ALLOCATION_DECIMAL_TURNOVER"
    );
    let weights: Vec<_> = units.iter().map(|u| *u as f64 / scale as f64).collect();
    ensure!(
        weights
            .iter()
            .zip(&s.x[..n])
            .all(|(w, x)| (w - x).abs() < 1e-9),
        "REVIEW_ALLOCATION_DECIMAL_PRECISION"
    );
    let risk: f64 = (0..n)
        .map(|i| {
            (0..n)
                .map(|j| weights[i] * covariance[i][j] * weights[j])
                .sum::<f64>()
        })
        .sum();
    let penalty: f64 = weights
        .iter()
        .zip(&prior)
        .map(|(w, p)| (w - p).powi(2))
        .sum();
    let objective = risk + lambda * penalty;
    ensure!(objective.is_finite(), "REVIEW_ALLOCATION_OBJECTIVE_RANGE");
    Ok(NativeReviewAllocationResultV1 {
        schema_version: SchemaV1,
        method: "QZ_REVIEW_QUALITY_DIVERSIFICATION_FIXED_DIAGONAL_10_V1".into(),
        kind: request.kind,
        solver_version: "clarabel/0.11.1".into(),
        members: request
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| NativeReviewAllocatedMemberV1 {
                id: m.id.clone(),
                source_report_id: m.source_report_id.clone(),
                quality_coefficient: qualities[i],
                prior: prior[i],
                weight: decimal_weights[i]
                    .to_string()
                    .parse::<DecimalValue>()
                    .map_err(anyhow::Error::msg)
                    .unwrap(),
            })
            .collect(),
        covariance,
        lambda,
        objective,
        primal_residual: s.r_prim,
        dual_residual: s.r_dual,
        requires_new_candidate: true,
        requires_shared_account_study: true,
    })
}
