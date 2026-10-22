//! Causal signal calibration and simplex stacking. No asset IDs, targets or qualification.
use anyhow::{ensure, Result};
use bigdecimal::{BigDecimal, ToPrimitive};
use clarabel::{algebra::CscMatrix, solver::*};
use contracts::{
    ensemble_fit::*, factors::NativeFactorPartitionV1, science::*, DbCounter, SchemaV1,
};
use ndarray::{Array1, Array2};
use std::collections::BTreeSet;

const METHOD: &str = "TEMPORAL_OOF_OLS_SIMPLEX_V1";
const DECIMAL_PLACES: u32 = 12;
fn count(n: usize) -> Result<DbCounter> {
    Ok(DbCounter::new(n.try_into()?).map_err(anyhow::Error::msg)?)
}
fn nonempty(s: &str) -> bool {
    !s.trim().is_empty() && s.len() <= 512
}

#[derive(Clone)]
struct Row {
    instrument: String,
    event: u64,
    available: u64,
    label_event: u64,
    label_available: u64,
    y: f64,
    x: Vec<f64>,
}

fn complete_before(row: &Row, end: u64) -> bool {
    row.event < end && row.available < end && row.label_event < end && row.label_available < end
}

fn calibrations(rows: &[&Row], members: usize) -> Result<Vec<Vec<NativeEnsembleCalibrationV1>>> {
    let instruments: BTreeSet<_> = rows.iter().map(|r| r.instrument.as_str()).collect();
    ensure!(!instruments.is_empty(), "ENSEMBLE_NO_CALIBRATION_ROWS");
    (0..members)
        .map(|m| {
            instruments
                .iter()
                .map(|instrument| {
                    let selected: Vec<_> = rows
                        .iter()
                        .filter(|r| &r.instrument.as_str() == instrument)
                        .collect();
                    let model = crate::validation::ScoreCalibration::fit(
                        &selected.iter().map(|r| r.x[m]).collect::<Vec<_>>(),
                        &selected.iter().map(|r| r.y).collect::<Vec<_>>(),
                    )?;
                    let [intercept, slope] = model.coefficients();
                    Ok(NativeEnsembleCalibrationV1 {
                        instrument_id: (*instrument).into(),
                        intercept,
                        slope,
                        training_observations: count(selected.len())?,
                        last_label_available_ns: DbCounter::new(
                            selected.iter().map(|r| r.label_available).max().unwrap(),
                        )
                        .map_err(anyhow::Error::msg)?,
                    })
                })
                .collect()
        })
        .collect()
}

fn calibrated(models: &[Vec<NativeEnsembleCalibrationV1>], row: &Row) -> Result<Vec<f64>> {
    models
        .iter()
        .enumerate()
        .map(|(m, fits)| {
            let fit = fits
                .iter()
                .find(|f| f.instrument_id == row.instrument)
                .ok_or_else(|| anyhow::anyhow!("ENSEMBLE_INSTRUMENT_WITHOUT_TRAINING"))?;
            let value = fit.intercept + fit.slope * row.x[m];
            ensure!(value.is_finite(), "ENSEMBLE_CALIBRATION_NONFINITE");
            Ok(value)
        })
        .collect()
}

/// Caller binds immutable factor/report identities and separately budgets every fit.
/// No held-out or selection-partition observation enters calibration or the objective.
pub fn fit(request: &NativeEnsembleFitRequestV1) -> Result<NativeEnsembleFitResultV1> {
    let n = request.members.len();
    ensure!(
        (2..=64).contains(&n) && (1..=32).contains(&request.folds.len()),
        "ENSEMBLE_FIT_LIMIT"
    );
    let first = &request.members[0].report;
    domain::execution::forecast_request(&first.request.forecast)?;
    let selected_bars = crate::catalog::bar_types(&first.request.forecast.selection.bar_types)?;
    ensure!(
        selected_bars
            .iter()
            .all(|bar| bar.spec() == selected_bars[0].spec()),
        "ENSEMBLE_COMMON_LABEL_HORIZON"
    );
    let selected_instruments: BTreeSet<_> = selected_bars
        .iter()
        .map(|bar| bar.instrument_id().to_string())
        .collect();
    ensure!(
        request.fit_end_ns <= first.request.train_end_ns
            && request.fit_end_ns > first.request.forecast.selection.event_start_ns,
        "ENSEMBLE_FIT_OUTSIDE_TRAIN"
    );
    ensure!(
        first.points.len() <= 200_000 && !first.points.is_empty(),
        "ENSEMBLE_PANEL_LIMIT"
    );
    let reference = serde_json::to_value(&first.request)?;
    let mut ids = BTreeSet::new();
    for member in &request.members {
        ensure!(
            nonempty(&member.factor_version_id)
                && nonempty(&member.source_evidence_ref)
                && ids.insert(&member.factor_version_id),
            "ENSEMBLE_MEMBER_ID"
        );
        ensure!(
            serde_json::to_value(&member.report.request)? == reference
                && member.report.points.len() == first.points.len(),
            "ENSEMBLE_REPORT_CONTRACT_MISMATCH"
        );
    }
    let mut keys = BTreeSet::new();
    let mut rows = Vec::new();
    for (i, point) in first.points.iter().enumerate() {
        let p = &point.observation;
        // Do not inspect labels or predictions in any later partition.
        if p.event_ns >= request.fit_end_ns {
            continue;
        }
        ensure!(
            selected_instruments.contains(&p.instrument_id)
                && p.event_ns >= first.request.forecast.selection.event_start_ns
                && p.available_ns <= first.request.forecast.selection.decision_cutoff_ns,
            "ENSEMBLE_PANEL_SELECTION"
        );
        ensure!(
            point.partition == NativeFactorPartitionV1::Train,
            "ENSEMBLE_NONTRAIN_POINT"
        );
        ensure!(
            keys.insert((p.instrument_id.clone(), p.event_ns.get())),
            "ENSEMBLE_DUPLICATE_POINT"
        );
        let mut x = Vec::with_capacity(n);
        for member in &request.members {
            let q = &member.report.points[i];
            let o = &q.observation;
            ensure!(
                o.instrument_id == p.instrument_id
                    && o.ordinal == p.ordinal
                    && o.event_ns == p.event_ns
                    && o.available_ns == p.available_ns
                    && o.label_return == p.label_return
                    && o.label_available_ns == p.label_available_ns
                    && q.label_event_ns == point.label_event_ns
                    && q.partition == point.partition,
                "ENSEMBLE_PANEL_ALIGNMENT"
            );
            ensure!(
                o.forecast.is_none_or(|v| v.is_finite())
                    && o.label_return.is_none_or(|v| v.is_finite()),
                "ENSEMBLE_PANEL_NONFINITE"
            );
            if let Some(value) = o.forecast {
                x.push(value);
            }
        }
        if x.len() != n
            || p.label_return.is_none()
            || p.label_available_ns.is_none()
            || point.label_event_ns.is_none()
        {
            continue;
        }
        let row = Row {
            instrument: p.instrument_id.clone(),
            event: p.event_ns.get(),
            available: p.available_ns.get(),
            label_event: point.label_event_ns.unwrap().get(),
            label_available: p.label_available_ns.unwrap().get(),
            y: p.label_return.unwrap(),
            x,
        };
        ensure!(
            row.available >= row.event
                && row.label_event > row.event
                && row.label_available >= row.label_event
                && row.label_available > row.available,
            "ENSEMBLE_PANEL_CLOCK_INVALID"
        );
        if complete_before(&row, request.fit_end_ns.get()) {
            rows.push(row);
        }
    }
    rows.sort_by(|a, b| (a.event, &a.instrument).cmp(&(b.event, &b.instrument)));
    let mut oof_x = Vec::new();
    let mut oof_y = Vec::new();
    let mut folds = Vec::new();
    let mut previous_end = None;
    for fold in &request.folds {
        ensure!(
            fold.train_end_ns > first.request.forecast.selection.event_start_ns
                && fold.train_end_ns <= fold.validation_start_ns
                && fold.validation_start_ns < fold.validation_end_ns
                && fold.validation_end_ns <= request.fit_end_ns
                && previous_end.is_none_or(|end| fold.validation_start_ns >= end),
            "ENSEMBLE_FOLD_TIME_INVALID"
        );
        previous_end = Some(fold.validation_end_ns);
        let training: Vec<_> = rows
            .iter()
            .filter(|r| complete_before(r, fold.train_end_ns.get()))
            .collect();
        let validation: Vec<_> = rows
            .iter()
            .filter(|r| {
                r.event >= fold.validation_start_ns.get()
                    && complete_before(r, fold.validation_end_ns.get())
            })
            .collect();
        ensure!(!validation.is_empty(), "ENSEMBLE_FOLD_NO_VALIDATION");
        let models = calibrations(&training, n)?;
        for row in &validation {
            oof_x.extend(calibrated(&models, row)?);
            oof_y.push(row.y);
        }
        folds.push(NativeEnsembleFoldEvidenceV1 {
            fold: fold.clone(),
            training_observations: count(training.len())?,
            validation_observations: count(validation.len())?,
            training_last_label_available_ns: DbCounter::new(
                training.iter().map(|r| r.label_available).max().unwrap(),
            )
            .map_err(anyhow::Error::msg)?,
            member_calibrations: models,
        });
    }
    ensure!(oof_y.len() >= 3, "ENSEMBLE_OOF_SAMPLE_LIMIT");
    let x = Array2::from_shape_vec((oof_y.len(), n), oof_x)?;
    let y = Array1::from(oof_y);
    let p_dense = x.t().dot(&x) * (2.0 / y.len() as f64);
    let q = x.t().dot(&y) * (-2.0 / y.len() as f64);
    ensure!(
        p_dense.iter().chain(q.iter()).all(|v| v.is_finite()),
        "ENSEMBLE_OBJECTIVE_NONFINITE"
    );
    let mut pr = Vec::new();
    let mut pc = Vec::new();
    let mut pv = Vec::new();
    for i in 0..n {
        for j in i..n {
            pr.push(i);
            pc.push(j);
            pv.push(p_dense[[i, j]]);
        }
    }
    let p = CscMatrix::new_from_triplets(n, n, pr, pc, pv);
    let mut ar = Vec::new();
    let mut ac = Vec::new();
    let mut av = Vec::new();
    for j in 0..n {
        ar.extend([0, j + 1]);
        ac.extend([j, j]);
        av.extend([1.0, -1.0]);
    }
    let a = CscMatrix::new_from_triplets(n + 1, n, ar, ac, av);
    let mut b = vec![0.0; n + 1];
    b[0] = 1.0;
    let settings = DefaultSettingsBuilder::default()
        .verbose(false)
        .tol_gap_abs(1e-12)
        .tol_gap_rel(1e-12)
        .tol_feas(1e-12)
        .build()?;
    let mut solver = DefaultSolver::new(
        &p,
        q.as_slice().unwrap(),
        &a,
        &b,
        &[ZeroConeT(1), NonnegativeConeT(n)],
        settings,
    )?;
    solver.solve();
    ensure!(
        solver.solution.status == SolverStatus::Solved,
        "ENSEMBLE_SOLVER_NOT_OPTIMAL"
    );
    let solution = &solver.solution;
    ensure!(
        solution
            .x
            .iter()
            .all(|w| w.is_finite() && *w >= -1e-10 && *w <= 1.0 + 1e-10)
            && (solution.x.iter().sum::<f64>() - 1.0).abs() <= 1e-10,
        "ENSEMBLE_SOLVER_CONSTRAINTS"
    );
    // Fixed decimal reporting: round to 12 places; allocate its <=n/2 ulp
    // sum residual to the largest coefficient. Never add a positive epsilon.
    let scale = 10_i64.pow(DECIMAL_PLACES);
    let mut units: Vec<i64> = solution
        .x
        .iter()
        .map(|w| (w.max(0.0) * scale as f64).round() as i64)
        .collect();
    let largest = solution
        .x
        .iter()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(b.1))
        .unwrap()
        .0;
    units[largest] += scale - units.iter().sum::<i64>();
    ensure!(
        units.iter().all(|w| *w >= 0),
        "ENSEMBLE_DECIMAL_CONSTRAINTS"
    );
    let weights: Vec<f64> = units.iter().map(|w| *w as f64 / scale as f64).collect();
    ensure!(
        weights
            .iter()
            .zip(&solution.x)
            .all(|(a, b)| (a - b).abs() <= 1e-9),
        "ENSEMBLE_DECIMAL_PRECISION"
    );
    let errors = x.dot(&Array1::from(weights)) - y;
    let mse = errors.dot(&errors) / errors.len() as f64;
    ensure!(
        [mse, solution.obj_val, solution.r_prim, solution.r_dual]
            .iter()
            .all(|v| v.is_finite()),
        "ENSEMBLE_DIAGNOSTIC_NONFINITE"
    );
    let final_models = calibrations(&rows.iter().collect::<Vec<_>>(), n)?;
    let members = request
        .members
        .iter()
        .enumerate()
        .map(|(m, member)| {
            Ok(NativeFittedFactorV1 {
                factor_version_id: member.factor_version_id.clone(),
                source_evidence_ref: member.source_evidence_ref.clone(),
                weight: BigDecimal::new(units[m].into(), i64::from(DECIMAL_PLACES))
                    .to_plain_string()
                    .parse()
                    .map_err(|_| anyhow::anyhow!("ENSEMBLE_DECIMAL_RANGE"))?,
                calibrations: final_models[m].clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NativeEnsembleFitResultV1 {
        schema_version: SchemaV1,
        method: METHOD.into(),
        solver_version: "clarabel-0.11.1".into(),
        calibration_version: "linregress-0.5.4".into(),
        weight_decimal_places: DECIMAL_PLACES,
        fit_end_ns: request.fit_end_ns,
        forecast_contract: first.request.forecast.clone(),
        members,
        folds,
        oof_observations: count(errors.len())?,
        oof_mean_squared_error: mse,
        solver_objective: solution.obj_val,
        primal_residual: solution.r_prim,
        dual_residual: solution.r_dual,
    })
}

/// Apply only saved coefficients. Labels remain evidence and never enter predictions.
pub fn apply(request: &NativeEnsembleApplyRequestV1) -> Result<NativeEnsembleApplyResultV1> {
    let fit = &request.fit;
    ensure!(
        fit.method == METHOD
            && fit.solver_version == "clarabel-0.11.1"
            && fit.calibration_version == "linregress-0.5.4"
            && fit.weight_decimal_places == DECIMAL_PLACES,
        "ENSEMBLE_FROZEN_METHOD"
    );
    ensure!(
        (2..=64).contains(&fit.members.len()) && fit.members.len() == request.members.len(),
        "ENSEMBLE_APPLY_MEMBER_LIMIT"
    );
    let mut sum = BigDecimal::from(0);
    let mut ids = BTreeSet::new();
    let mut weights = Vec::new();
    for (model, input) in fit.members.iter().zip(&request.members) {
        ensure!(
            model.factor_version_id == input.factor_version_id
                && ids.insert(&model.factor_version_id)
                && model.weight.is_nonnegative(),
            "ENSEMBLE_FROZEN_MEMBER_MISMATCH"
        );
        sum += model.weight.as_decimal();
        weights.push(
            model
                .weight
                .as_decimal()
                .to_f64()
                .ok_or_else(|| anyhow::anyhow!("ENSEMBLE_WEIGHT_RANGE"))?,
        );
        domain::execution::forecast_request(&input.forecast_request)?;
        let original = &fit.forecast_contract;
        ensure!(
            input.forecast_request.selection.bar_types == original.selection.bar_types
                && input.forecast_request.parameters.fast_period == original.parameters.fast_period
                && input.forecast_request.parameters.slow_period == original.parameters.slow_period
                && input.forecast_request.parameters.label_horizon_observations
                    == original.parameters.label_horizon_observations,
            "ENSEMBLE_APPLY_FORECAST_CONTRACT"
        );
        ensure!(
            model.calibrations.iter().all(|c| c.intercept.is_finite()
                && c.slope.is_finite()
                && c.last_label_available_ns < fit.fit_end_ns
                && c.training_observations.get() >= 3)
                && model
                    .calibrations
                    .iter()
                    .map(|c| &c.instrument_id)
                    .collect::<BTreeSet<_>>()
                    .len()
                    == model.calibrations.len(),
            "ENSEMBLE_FROZEN_CALIBRATION"
        );
    }
    ensure!(sum == BigDecimal::from(1), "ENSEMBLE_FROZEN_SIMPLEX");
    let first = &request.members[0];
    for member in &request.members {
        ensure!(
            serde_json::to_value(&member.forecast_request)?
                == serde_json::to_value(&first.forecast_request)?
                && member.forecast.points.len() == first.forecast.points.len(),
            "ENSEMBLE_APPLY_ALIGNMENT"
        );
    }
    let mut points = Vec::new();
    let mut keys = BTreeSet::new();
    for (i, point) in first.forecast.points.iter().enumerate() {
        if point.event_ns < fit.fit_end_ns {
            continue;
        } // Causal feature warmup prefix is not emitted.
        ensure!(
            point.available_ns >= fit.fit_end_ns
                && point.available_ns >= point.event_ns
                && point.event_ns >= first.forecast_request.selection.event_start_ns
                && point.event_ns < first.forecast_request.selection.event_end_ns
                && point.available_ns <= first.forecast_request.selection.decision_cutoff_ns
                && keys.insert((point.instrument_id.clone(), point.event_ns)),
            "ENSEMBLE_APPLY_CLOCK"
        );
        let mut result = point.clone();
        let mut value = 0.0;
        let mut missing = None;
        for ((member, model), weight) in request.members.iter().zip(&fit.members).zip(&weights) {
            let p = &member.forecast.points[i];
            ensure!(
                p.instrument_id == point.instrument_id
                    && p.event_ns == point.event_ns
                    && p.available_ns == point.available_ns
                    && p.ordinal == point.ordinal
                    && p.label_return == point.label_return
                    && p.label_available_ns == point.label_available_ns
                    && p.label_reason == point.label_reason,
                "ENSEMBLE_APPLY_ALIGNMENT"
            );
            if *weight == 0.0 {
                continue;
            }
            if let Some(raw) = p.forecast {
                ensure!(
                    raw.is_finite() && p.forecast_reason.is_none(),
                    "ENSEMBLE_APPLY_SCORE_INVALID"
                );
                let c = model
                    .calibrations
                    .iter()
                    .find(|c| c.instrument_id == p.instrument_id)
                    .ok_or_else(|| anyhow::anyhow!("ENSEMBLE_APPLY_INSTRUMENT"))?;
                value += weight * (c.intercept + c.slope * raw);
            } else {
                missing = Some(
                    p.forecast_reason
                        .ok_or_else(|| anyhow::anyhow!("ENSEMBLE_APPLY_MISSING_REASON"))?,
                );
            }
        }
        ensure!(value.is_finite(), "ENSEMBLE_APPLY_NONFINITE");
        result.forecast = missing.is_none().then_some(value);
        result.forecast_reason = missing;
        points.push(result);
    }
    ensure!(!points.is_empty(), "ENSEMBLE_APPLY_NO_FUTURE_POINTS");
    let fuel = request
        .members
        .iter()
        .try_fold(Some(0_u64), |sum, m| -> Result<Option<u64>> {
            Ok(match (sum, m.forecast.consumed_fuel) {
                (Some(s), Some(f)) => Some(
                    s.checked_add(f.get())
                        .ok_or_else(|| anyhow::anyhow!("ENSEMBLE_FUEL_OVERFLOW"))?,
                ),
                _ => None,
            })
        })?;
    let mut versions = first.forecast.native_versions.clone();
    versions.insert("qz-ensemble".into(), METHOD.into());
    Ok(NativeEnsembleApplyResultV1 {
        schema_version: SchemaV1,
        forecast: NativeForecastResultV1 {
            schema_version: SchemaV1,
            native_versions: versions,
            consumed_fuel: fuel
                .map(DbCounter::new)
                .transpose()
                .map_err(anyhow::Error::msg)?,
            points,
        },
    })
}

/// Verify the immutable numerical fit against its original report payloads before
/// adapting it to the established rolling-study calibration protocol. The host
/// still verifies that every report_artifact_id names those actual stored bytes.
pub fn freeze(request: &NativeEnsembleFreezeRequestV1) -> Result<NativeEnsembleFreezeResultV1> {
    ensure!(
        request.members.len() == request.fit.members.len(),
        "ENSEMBLE_FREEZE_MEMBER_LIMIT"
    );
    let mut objects = BTreeSet::new();
    let members = request
        .members
        .iter()
        .zip(&request.fit.members)
        .map(|(source, fitted)| {
            ensure!(
                source.factor_version_id == fitted.factor_version_id
                    && objects.insert(source.report_artifact_id),
                "ENSEMBLE_FREEZE_MEMBER_BINDING"
            );
            Ok(NativeEnsembleFitMemberV1 {
                factor_version_id: source.factor_version_id.clone(),
                source_evidence_ref: fitted.source_evidence_ref.clone(),
                report: source.report.clone(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let verified = fit(&NativeEnsembleFitRequestV1 {
        schema_version: SchemaV1,
        fit_end_ns: request.fit.fit_end_ns,
        folds: request
            .fit
            .folds
            .iter()
            .map(|fold| fold.fold.clone())
            .collect(),
        members,
    })?;
    ensure!(
        serde_json::to_value(&verified)? == serde_json::to_value(&request.fit)?,
        "ENSEMBLE_FREEZE_ORIGINAL_FIT_MISMATCH"
    );
    let bar_types = crate::catalog::bar_types(&verified.forecast_contract.selection.bar_types)?;
    let mut members = Vec::new();
    for (source, fitted) in request.members.iter().zip(&verified.members) {
        let mut assets = Vec::new();
        for calibration in &fitted.calibrations {
            let bar_type = bar_types
                .iter()
                .find(|bar| bar.instrument_id().to_string() == calibration.instrument_id)
                .ok_or_else(|| anyhow::anyhow!("ENSEMBLE_FREEZE_ASSET_BINDING"))?
                .to_string();
            // Match the complete common panel used for final refitting, including
            // rows excluded by another member's original warmup/missing mask.
            let mut training_ordinals = Vec::new();
            for (index, point) in source.report.points.iter().enumerate() {
                let p = &point.observation;
                if p.instrument_id == calibration.instrument_id
                    && p.event_ns < verified.fit_end_ns
                    && p.available_ns < verified.fit_end_ns
                    && p.label_available_ns
                        .is_some_and(|time| time < verified.fit_end_ns)
                    && point
                        .label_event_ns
                        .is_some_and(|time| time < verified.fit_end_ns)
                    && p.label_return.is_some()
                    && request
                        .members
                        .iter()
                        .all(|member| member.report.points[index].observation.forecast.is_some())
                {
                    training_ordinals.push(p.ordinal);
                }
            }
            training_ordinals.sort_unstable();
            ensure!(
                training_ordinals.len() as u64 == calibration.training_observations.get(),
                "ENSEMBLE_FREEZE_TRAINING_BINDING"
            );
            assets.push(NativeAssetCalibrationV1 {
                instrument_id: calibration.instrument_id.clone(),
                bar_type,
                // Full-training refit follows all OOF folds; this is not LAST_NATIVE_FOLD.
                fold_index: verified.folds.len().try_into()?,
                training_ordinals,
                training_end_available_ns: calibration.last_label_available_ns,
                calibration: NativeCalibrationV1 {
                    status: contracts::evidence::MetricStatus::Ok,
                    reason_code: None,
                    intercept: Some(calibration.intercept),
                    slope: Some(calibration.slope),
                    training_observations: calibration.training_observations,
                },
            });
        }
        let calibration = NativeFrozenCalibrationV1 {
            schema_version: SchemaV1,
            source_report_artifact_id: source.report_artifact_id,
            estimator_kind: "linregress.affine_ols".into(),
            estimator_version: "0.5.4".into(),
            selection_rule: "TEMPORAL_OOF_FULL_TRAIN_REFIT_V1".into(),
            horizon_observations: DbCounter::new(u64::from(
                verified
                    .forecast_contract
                    .parameters
                    .label_horizon_observations,
            ))
            .map_err(anyhow::Error::msg)?,
            fit_end_available_ns: verified.fit_end_ns,
            assets,
        };
        domain::execution::check_alpha_calibration(&calibration)?;
        members.push(NativeEnsembleFrozenMemberV1 {
            factor_version_id: source.factor_version_id.clone(),
            calibration,
        });
    }
    Ok(NativeEnsembleFreezeResultV1 {
        schema_version: SchemaV1,
        members,
    })
}
