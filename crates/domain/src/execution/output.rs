//! Fixed native result structure and exact task associations. These checks do not
//! rerun a model, fit statistics, confer PIT, or replace independent qualification.
use super::{selection, NativeTaskParametersV1};
use crate::{control::text, research::invalid, DomainError};
use chrono::{DateTime, Utc};
use contracts::{
    execution::{NativeDataQualityReportV1, NativeModelCompilationV1},
    portfolio::{AllocationResultV1, SolverStatus},
    runtime_jobs::{native_output_contract, RuntimeOutputV1, MAX_JOB_OUTPUT_BYTES},
    science::{NativeBarSelectionV1, NativeForecastResultV1, NativeSimulationResultV1},
};
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet};

mod forecast;
mod sealed;
pub use sealed::{
    binding as check_alpha_sealed, metrics as alpha_sealed_metrics, policy as alpha_sealed_policy,
    request as alpha_sealed_request,
};
mod simulation;
mod validation;
pub use validation::metrics as alpha_validation_metrics;
pub use validation::policy as alpha_validation_policy;
pub use validation::{
    freeze_calibration as freeze_alpha_calibration, frozen_calibration as check_alpha_calibration,
};

fn bad(field: &str) -> DomainError {
    invalid(field, "NATIVE_OUTPUT_CONTRACT_INVALID")
}
fn decode<T: DeserializeOwned>(bytes: &[u8]) -> Result<T, DomainError> {
    serde_json::from_slice(bytes).map_err(|_| bad("native_output.body"))
}

/// Associate an already native-parsed canonical BarType with its instrument.
/// Native `BarType::from_str` and catalog semantics remain in the job. Only the
/// four right-hand native serialization components are skipped, preserving any
/// hyphens in the instrument itself; this is not a replacement BarType parser.
fn instruments(value: &NativeBarSelectionV1) -> Result<Vec<&str>, DomainError> {
    selection(value)?;
    let mut unique = BTreeSet::new();
    value
        .bar_types
        .iter()
        .map(|kind| {
            let mut parts = kind.rsplitn(5, '-');
            for _ in 0..4 {
                if parts.next().is_none_or(str::is_empty) {
                    return Err(bad("native_output.selection"));
                }
            }
            let id = parts.next().ok_or_else(|| bad("native_output.selection"))?;
            text(id, 1, 200, false)?;
            if !unique.insert(id) {
                return Err(bad("native_output.instruments"));
            }
            Ok(id)
        })
        .collect()
}
fn same_selection(a: &NativeBarSelectionV1, b: &NativeBarSelectionV1) -> bool {
    a.bar_types == b.bar_types
        && a.event_start_ns == b.event_start_ns
        && a.event_end_ns == b.event_end_ns
        && a.decision_cutoff_ns == b.decision_cutoff_ns
        && a.maximum_rows == b.maximum_rows
}
fn quality(value: &NativeDataQualityReportV1) -> Result<(), DomainError> {
    let checked = value
        .checked_at
        .timestamp_nanos_opt()
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| bad("native_output.checked_at"))?;
    if value.native_version != "nautilus-persistence/0.63.0"
        || !(1..=256).contains(&value.datasets.len())
        || !value
            .checked_at
            .timestamp_subsec_nanos()
            .is_multiple_of(1000)
    {
        return Err(bad("native_output.quality"));
    }
    let mut datasets = BTreeSet::new();
    for item in &value.datasets {
        let ids = instruments(&item.selection)?;
        if !datasets.insert(item.dataset_revision_id)
            || item
                .instrument_ids
                .iter()
                .map(String::as_str)
                .collect::<Vec<_>>()
                != ids
            || item.row_count.get() < ids.len() as u64
            || item.row_count.get() > u64::from(item.selection.maximum_rows)
            || item.first_event_ns < item.selection.event_start_ns
            || item.first_event_ns > item.last_event_ns
            || item.last_event_ns >= item.selection.event_end_ns
            || item.last_event_ns > item.available_through_ns
            || item.available_through_ns > item.selection.decision_cutoff_ns
            || item.available_through_ns.get() > checked
        {
            return Err(bad("native_output.dataset_quality"));
        }
    }
    Ok(())
}
fn compilation(value: &NativeModelCompilationV1) -> Result<(), DomainError> {
    text(&value.rustc_version, 1, 512, false)?;
    if !value.rustc_version.starts_with("rustc 1.98.1 ")
        || value.target != "wasm32-unknown-unknown"
        || value.abi != "predict(f64,f64,f64,f64,f64,f64,f64,f64)->f64"
        || !(8..=2 * 1024 * 1024).contains(&value.module_bytes.get())
    {
        return Err(bad("native_output.compilation"));
    }
    Ok(())
}
fn allocation(value: &AllocationResultV1) -> Result<(), DomainError> {
    let success = matches!(
        value.solver_status,
        SolverStatus::Optimal | SolverStatus::AcceptableInaccurate
    );
    if value.objective_value.is_some_and(|n| !n.is_finite())
        || value
            .primal_residual
            .is_some_and(|n| !n.is_finite() || n < 0.0)
        || value
            .dual_residual
            .is_some_and(|n| !n.is_finite() || n < 0.0)
        || success != value.targets.is_some()
        || success != value.cash_weight.is_some()
        || success == value.reason_code.is_some()
    {
        return Err(bad("native_output.allocation"));
    }
    if let Some(reason) = &value.reason_code {
        text(reason, 1, 120, false)?;
    }
    if let Some(targets) = &value.targets {
        if !(1..=256).contains(&targets.len())
            || value.objective_value.is_none()
            || value.primal_residual.is_none()
            || value.dual_residual.is_none()
        {
            return Err(bad("native_output.allocation"));
        }
        let mut identities = BTreeSet::new();
        for target in targets {
            text(&target.instrument_id, 1, 200, false)?;
            if !identities.insert(&target.instrument_id)
                || iso_currency::Currency::from_code(&target.currency).is_none()
            {
                return Err(bad("native_output.allocation_target"));
            }
        }
    }
    Ok(())
}

/// Public transport validation, without borrowing Operator or evaluator authority.
/// Exact immutable task associations are additionally enforced by output_bindings.
pub fn output_shape(output: &RuntimeOutputV1, bytes: &[u8]) -> Result<(), DomainError> {
    let contract = native_output_contract(&output.schema.name, &output.schema.version)
        .ok_or_else(|| bad("native_output.schema"))?;
    if output.kind != contract.kind
        || output.media_type != contract.media_type
        || output.storage_version.get() != 1
        || bytes.is_empty()
        || bytes.len() as u64 != output.byte_count.get()
        || bytes.len() as u64 > MAX_JOB_OUTPUT_BYTES
    {
        return Err(bad("native_output"));
    }
    match contract.name {
        "qz.wasm_model"
            if bytes.starts_with(b"\0asm\x01\0\0\0") && bytes.len() <= 2 * 1024 * 1024 =>
        {
            Ok(())
        }
        "qz.model_compilation" => compilation(&decode(bytes)?),
        "qz.data_quality" => quality(&decode(bytes)?),
        "qz.native_forecast" => forecast::shape(&decode::<NativeForecastResultV1>(bytes)?),
        "qz.alpha_validation" => validation::shape(&decode(bytes)?),
        "qz.alpha_sealed" => sealed::shape(&decode(bytes)?),
        "qz.native_allocation" => allocation(&decode(bytes)?),
        "qz.native_simulation" => simulation::shape(&decode::<NativeSimulationResultV1>(bytes)?),
        _ => Err(bad("native_output.schema")),
    }
}

/// Called inside the final publication transaction using the original immutable
/// PARAMETERS bytes, not a remote report's description of what it claims to have run.
pub fn output_bindings(
    parameters: &NativeTaskParametersV1,
    calibration: Option<&contracts::science::NativeFrozenCalibrationV1>,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    outputs: &[(RuntimeOutputV1, Vec<u8>)],
) -> Result<(), DomainError> {
    if started_at > finished_at {
        return Err(bad("native_output.times"));
    }
    let expected = parameters.output_schemas();
    if outputs.len() != expected.len() {
        return Err(bad("native_output.membership"));
    }
    let mut by_schema = BTreeMap::new();
    for (output, bytes) in outputs {
        output_shape(output, bytes)?;
        if !expected
            .iter()
            .any(|s| s.name == output.schema.name && s.version == output.schema.version)
            || by_schema
                .insert(output.schema.name.as_str(), (output, bytes.as_slice()))
                .is_some()
        {
            return Err(bad("native_output.membership"));
        }
    }
    let body = |name| {
        by_schema
            .get(name)
            .copied()
            .ok_or_else(|| bad("native_output.membership"))
    };
    match parameters {
        NativeTaskParametersV1::ValidateData { selections, .. } => {
            let value: NativeDataQualityReportV1 = decode(body("qz.data_quality")?.1)?;
            if value.checked_at < started_at
                || value.checked_at > finished_at
                || value.datasets.len() != selections.len()
                || value
                    .datasets
                    .iter()
                    .zip(selections)
                    .any(|(actual, expected)| {
                        actual.dataset_revision_id != expected.dataset_revision_id
                            || !same_selection(&actual.selection, &expected.selection)
                    })
            {
                return Err(bad("native_output.quality_input"));
            }
        }
        NativeTaskParametersV1::CompileModel {
            code_artifact_id, ..
        } => {
            let value: NativeModelCompilationV1 = decode(body("qz.model_compilation")?.1)?;
            let model = body("qz.wasm_model")?.0;
            if value.code_artifact_id != *code_artifact_id
                || value.model_storage_ref != model.storage_ref
                || value.module_bytes != model.byte_count
            {
                return Err(bad("native_output.model_binding"));
            }
        }
        NativeTaskParametersV1::EvaluateAlpha { request, .. } => {
            forecast::binding(request, &decode(body("qz.native_forecast")?.1)?)?;
        }
        NativeTaskParametersV1::ValidateAlpha { request, .. } => {
            validation::binding(request, &decode(body("qz.alpha_validation")?.1)?)?;
        }
        NativeTaskParametersV1::EvaluateSealedAlpha {
            request,
            calibration_artifact_id,
            ..
        } => {
            if calibration_artifact_id.is_some() != calibration.is_some() {
                return Err(bad("sealed.calibration_input"));
            }
            sealed::binding(request, calibration, &decode(body("qz.alpha_sealed")?.1)?)?;
        }
        NativeTaskParametersV1::BuildPortfolio { request, .. } => {
            let value: AllocationResultV1 = decode(body("qz.native_allocation")?.1)?;
            if value.iterations > request.settings.max_iterations {
                return Err(bad("native_output.solver_iterations"));
            }
            crate::portfolio::allocation_result(request, &value)?;
        }
        NativeTaskParametersV1::SimulatePortfolio { request, .. } => {
            simulation::binding(request, &decode(body("qz.native_simulation")?.1)?)?;
        }
    }
    Ok(())
}
