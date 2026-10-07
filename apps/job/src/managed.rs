//! One native job per isolated process, using fixed mounts and typed operations.
//! Neither compilation nor successful execution confers REAL data or qualification.
use anyhow::{ensure, Result};
use contracts::{
    execution::*,
    runtime_jobs::{JobSpecV1, RuntimeOutputKind, RuntimeOutputV1},
    DbCounter, Id, Revision, SchemaV1,
};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

mod compiler_memory;
use compiler_memory::CompilerMemory;
pub use compiler_memory::CompilerMemoryLimit;

const PARAMETERS_LIMIT: usize = 8 * 1024 * 1024;
const SPEC_LIMIT: usize = 1024 * 1024;
const COMPILER: &str = "/opt/rust/bin/rustc";

fn counter(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(|_| anyhow::anyhow!("NATIVE_COUNTER_RANGE"))
}

fn read(path: &Path, maximum: usize) -> Result<Vec<u8>> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = options.open(path)?;
    let metadata = file.metadata()?;
    ensure!(
        metadata.is_file() && metadata.len() > 0 && metadata.len() <= maximum as u64,
        "NATIVE_FILE_LIMIT"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(metadata.nlink() == 1, "NATIVE_FILE_IDENTITY");
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum as u64 + 1).read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() == metadata.len() as usize,
        "NATIVE_FILE_CHANGED"
    );
    Ok(bytes)
}
fn document<T: DeserializeOwned>(path: &Path, maximum: usize) -> Result<T> {
    Ok(serde_json::from_slice(&read(path, maximum)?)?)
}
fn frozen(file: &File) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        file.set_permissions(fs::Permissions::from_mode(0o444))?;
    }
    file.sync_all()?;
    Ok(())
}
struct LimitedFile {
    file: File,
    remaining: Option<u64>,
    written: u64,
}
impl Write for LimitedFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .remaining
            .is_some_and(|remaining| bytes.len() as u64 > remaining)
        {
            return Err(std::io::Error::other("NATIVE_OUTPUT_LIMIT"));
        }
        let written = self.file.write(bytes)?;
        self.remaining = self.remaining.map(|remaining| remaining - written as u64);
        self.written = self
            .written
            .checked_add(written as u64)
            .ok_or_else(|| std::io::Error::other("NATIVE_OUTPUT_SIZE_OVERFLOW"))?;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

struct Outputs {
    root: PathBuf,
    remaining: Option<u64>,
    items: Vec<RuntimeOutputV1>,
}
impl Outputs {
    fn json(&mut self, name: &str, kind: RuntimeOutputKind, value: &impl Serialize) -> Result<Id> {
        self.document(name, kind, "application/json", |stream| {
            Ok(serde_json::to_writer(stream, value)?)
        })
    }
    fn document(
        &mut self,
        name: &str,
        kind: RuntimeOutputKind,
        media: &str,
        write: impl FnOnce(&mut LimitedFile) -> Result<()>,
    ) -> Result<Id> {
        let id = Id::new();
        // Runtime scans this mount while the job is running: all entries must
        // remain ordinary, single-link files, including incomplete writes.
        let staged = tempfile::NamedTempFile::new_in(&self.root)?;
        let mut stream = LimitedFile {
            file: staged.as_file().try_clone()?,
            remaining: self.remaining,
            written: 0,
        };
        write(&mut stream)?;
        stream.flush()?;
        ensure!(stream.written > 0, "NATIVE_EMPTY_OUTPUT");
        frozen(&stream.file)?;
        let item = RuntimeOutputV1 {
            kind,
            schema: contracts::runtime::RuntimeArtifactSchemaV1 {
                name: name.into(),
                version: "1".into(),
            },
            storage_ref: id,
            storage_version: Revision::INITIAL,
            byte_count: counter(stream.written)?,
            media_type: media.into(),
        };
        publish(&staged, &self.root.join(id.to_string()))?;
        self.remaining = self.remaining.map(|remaining| remaining - stream.written);
        self.items.push(item);
        Ok(id)
    }
    fn history(
        &mut self,
        request: &contracts::science::NativePortfolioStudyRequestV1,
        result: &contracts::science::NativePortfolioStudyResultV1,
    ) -> Result<()> {
        use contracts::portfolio_history as history;
        let expected = history::batch(request, result)?;
        let id = self.document(
            history::NAME,
            RuntimeOutputKind::Targets,
            history::MEDIA_TYPE,
            |stream| Ok(history::write(stream, &expected)?),
        )?;
        let bytes = read(
            &self.root.join(id.to_string()),
            contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize,
        )?;
        ensure!(
            history::read(&bytes)? == expected,
            "PORTFOLIO_HISTORY_ROUNDTRIP"
        );
        Ok(())
    }
    fn compiled_model(&mut self, bytes: &[u8]) -> Result<Id> {
        self.document(
            "qz.wasm_model",
            RuntimeOutputKind::Model,
            "application/wasm",
            |stream| Ok(stream.write_all(bytes)?),
        )
    }
    fn seal(self) -> Result<()> {
        let index = NativeJobOutputIndexV1 {
            schema_version: SchemaV1,
            artifacts: self.items,
        };
        let staged = tempfile::NamedTempFile::new_in(&self.root)?;
        let mut stream = LimitedFile {
            file: staged.as_file().try_clone()?,
            remaining: Some(SPEC_LIMIT as u64),
            written: 0,
        };
        serde_json::to_writer(&mut stream, &index)?;
        stream.flush()?;
        frozen(&stream.file)?;
        publish(&staged, &self.root.join("index.json"))?;
        File::open(&self.root)?.sync_all()?;
        Ok(())
    }
}

fn publish(staged: &tempfile::NamedTempFile, target: &Path) -> Result<()> {
    // Unlike persist_noclobber's portable fallback, this cannot momentarily
    // hard-link the file and trip Runtime's live single-link invariant.
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        staged.path(),
        rustix::fs::CWD,
        target,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(std::io::Error::from)?;
    Ok(())
}

struct CompilerChild(Child);
impl Drop for CompilerChild {
    fn drop(&mut self) {
        // Reap only this owned compiler. The outer native cgroup owns all descendants.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn compile(
    spec: &JobSpecV1,
    input: &Path,
    code: Id,
    outputs: &mut Outputs,
    features: bool,
) -> Result<()> {
    compile_with(spec, input, code, outputs, features, Path::new(COMPILER))
}

fn compile_with(
    spec: &JobSpecV1,
    input: &Path,
    code: Id,
    outputs: &mut Outputs,
    features: bool,
    compiler: &Path,
) -> Result<()> {
    let source = input.join("objects").join(code.to_string());
    let bytes = read(&source, crate::signals::MAX_SIGNAL_MODULE_BYTES)?;
    std::str::from_utf8(&bytes)?;
    let memory = CompilerMemory::capture();
    let version = Command::new(compiler)
        .arg("--version")
        .env_clear()
        .env("PATH", "/opt/rust/bin:/usr/bin:/bin")
        .env("HOME", "/tmp")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    CompilerMemory::check(version.status, memory, "NATIVE_COMPILER_VERSION")?;
    ensure!(version.stdout.len() <= 512, "NATIVE_COMPILER_VERSION");
    let version = std::str::from_utf8(&version.stdout)?.trim();
    ensure!(
        version.starts_with("rustc 1.98.1 "),
        "NATIVE_COMPILER_VERSION"
    );
    // rustc creates linker scratch directories next to -o, regardless of
    // TMPDIR. Keep those directories off the flat, live-scanned output mount.
    let mut staging_builder = tempfile::Builder::new();
    staging_builder.prefix("qz-compile-");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        staging_builder.permissions(fs::Permissions::from_mode(0o700));
    }
    let staging = staging_builder.tempdir_in("/tmp")?;
    let target = staging.path().join("model.wasm");
    let memory = CompilerMemory::capture();
    let child = Command::new(compiler)
        .args([
            "--edition=2021",
            "--crate-type=cdylib",
            "--target=wasm32-unknown-unknown",
            "--crate-name=research_signal",
            "-C",
            "opt-level=1",
            "-C",
            "panic=abort",
            "-C",
            "codegen-units=1",
        ])
        .arg(&source)
        .arg("-o")
        .arg(&target)
        .env_clear()
        .env("PATH", "/opt/rust/bin:/usr/bin:/bin")
        .env("HOME", "/tmp")
        .env("TMPDIR", "/tmp")
        .current_dir("/tmp")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()?;
    let mut child = CompilerChild(child);
    let began = Instant::now();
    loop {
        if let Some(status) = child.0.try_wait()? {
            CompilerMemory::check(status, memory, "NATIVE_COMPILATION_FAILED")?;
            break;
        }
        ensure!(
            !domain::execution_limits::expired(spec.deadline_at, chrono::Utc::now())
                && spec
                    .limits
                    .wall_seconds
                    .is_none_or(|wall| began.elapsed() < Duration::from_secs(u64::from(wall))),
            "NATIVE_COMPILATION_DEADLINE"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let wasm = read(&target, crate::signals::MAX_SIGNAL_MODULE_BYTES)?;
    // Native Wasmi verifies imports, start functions, limits and exact predict ABI.
    let abi = if features {
        let module = crate::signals::SignalModule::new(&wasm)?;
        crate::feature_model::FeatureModel::new(&module, 1, 100_000)?;
        crate::feature_model::ABI
    } else {
        crate::signals::WasmSignal::new(&wasm, 1, 100_000)?;
        "predict(f64,f64,f64,f64,f64,f64,f64,f64)->f64"
    };
    let model = outputs.compiled_model(&wasm)?;
    outputs.json(
        "qz.model_compilation",
        RuntimeOutputKind::Report,
        &NativeModelCompilationV1 {
            schema_version: SchemaV1,
            code_artifact_id: code,
            model_storage_ref: model,
            rustc_version: version.to_owned(),
            target: "wasm32-unknown-unknown".into(),
            abi: abi.into(),
            module_bytes: counter(wasm.len() as u64)?,
        },
    )?;
    Ok(())
}

/// `input` and `output` are trusted local mounts. The HTTP body cannot select them.
/// Each process calls this exactly once; native numerical engines are not embedded in API/Worker.
pub fn execute(input: &Path, output: &Path) -> Result<()> {
    for root in [input, output] {
        let metadata = fs::symlink_metadata(root)?;
        ensure!(
            metadata.is_dir() && !metadata.file_type().is_symlink(),
            "NATIVE_JOB_ROOT"
        );
    }
    let spec: JobSpecV1 = document(&input.join("spec.json"), SPEC_LIMIT)?;
    let parameters: NativeTaskParametersV1 = document(
        &input
            .join("objects")
            .join(spec.parameters_artifact_id.to_string()),
        PARAMETERS_LIMIT,
    )?;
    domain::execution::task(&spec, &parameters)?;
    ensure!(
        !domain::execution_limits::expired(spec.deadline_at, chrono::Utc::now()),
        "NATIVE_JOB_DEADLINE"
    );
    let mut outputs = Outputs {
        root: output.to_owned(),
        remaining: spec.limits.output_bytes.map(|maximum| maximum.get()),
        items: Vec::new(),
    };
    let selections = match &parameters {
        NativeTaskParametersV1::ValidateData { selections, .. } => selections.clone(),
        NativeTaskParametersV1::StudyPortfolio {
            dataset_revision_id,
            request,
            ..
        } => vec![contracts::execution::NativeDatasetSelectionV1 {
            settlements: request.settlements.clone(),
            dataset_revision_id: *dataset_revision_id,
            selection: request.source_selection.clone(),
        }],
        NativeTaskParametersV1::SimulateCandidate {
            dataset_revision_id,
            source_selection,
            request,
            ..
        }
        | NativeTaskParametersV1::SimulatePortfolioSequence {
            dataset_revision_id,
            source_selection,
            request,
            ..
        } => vec![contracts::execution::NativeDatasetSelectionV1 {
            settlements: request.settlements.clone(),
            dataset_revision_id: *dataset_revision_id,
            selection: source_selection.clone(),
        }],
        _ => Vec::new(),
    };
    if !selections.is_empty() {
        let mut datasets = Vec::with_capacity(selections.len());
        for selected in selections {
            let measure_notionals = spec.inputs.iter().any(|input| {
                matches!(input,
                    contracts::runtime_jobs::RuntimeInputV1::Dataset { revision_id, role, .. }
                    if *revision_id == selected.dataset_revision_id
                        && *role != contracts::research::DataPartition::Sealed
                )
            });
            let (_, quality) = crate::catalog::measure_catalog(
                &input
                    .join("catalogs")
                    .join(selected.dataset_revision_id.to_string()),
                &selected,
                measure_notionals,
            )?;
            datasets.push(quality);
        }
        outputs.json(
            "qz.data_quality",
            RuntimeOutputKind::DataQuality,
            &crate::catalog::quality_report(datasets)?,
        )?;
    }
    match parameters {
        NativeTaskParametersV1::CompileModel {
            code_artifact_id, ..
        } => compile(&spec, input, code_artifact_id, &mut outputs, false)?,
        NativeTaskParametersV1::CompileFeatureModel {
            code_artifact_id, ..
        } => compile(&spec, input, code_artifact_id, &mut outputs, true)?,
        NativeTaskParametersV1::EvaluateExperiment {
            dataset_revision_id,
            model_artifact_id,
            feature_artifact_ids,
            request,
            ..
        } => {
            let bytes = read(
                &input.join("objects").join(model_artifact_id.to_string()),
                crate::signals::MAX_SIGNAL_MODULE_BYTES,
            )?;
            let parts = feature_artifact_ids
                .iter()
                .map(|id| document(&input.join("objects").join(id.to_string()), 2 * 1024 * 1024))
                .collect::<Result<Vec<contracts::science::FeatureObservationsV1>>>()?;
            let result = crate::experiment::evaluate(
                &input.join("catalogs").join(dataset_revision_id.to_string()),
                &request,
                dataset_revision_id,
                model_artifact_id,
                &feature_artifact_ids,
                &parts,
                &bytes,
            )?;
            outputs.json(
                "qz.experiment_evaluation",
                RuntimeOutputKind::Report,
                &result,
            )?;
        }
        NativeTaskParametersV1::ValidateData { .. } => {}
        NativeTaskParametersV1::EvaluateForward { request, .. } => {
            let result = crate::forward::evaluate(&request, |id| {
                let bytes = read(&input.join("objects").join(id.to_string()), 2 * 1024 * 1024)?;
                let expected = spec.inputs.iter().find_map(|i| match i {
                    contracts::runtime_jobs::RuntimeInputV1::Artifact {
                        artifact_id,
                        byte_count,
                        ..
                    } if *artifact_id == id => Some(byte_count.get()),
                    _ => None,
                });
                ensure!(expected == Some(bytes.len() as u64), "FORWARD_SOURCE_SIZE");
                Ok(bytes)
            })?;
            outputs.json("qz.forward_evaluation", RuntimeOutputKind::Report, &result)?;
        }
        NativeTaskParametersV1::EvaluateAlpha {
            dataset_revision_id,
            model_artifact_id,
            request,
            ..
        } => {
            let bytes = read(
                &input.join("objects").join(model_artifact_id.to_string()),
                crate::signals::MAX_SIGNAL_MODULE_BYTES,
            )?;
            let result = crate::forecast::forecast(
                &input.join("catalogs").join(dataset_revision_id.to_string()),
                &request,
                &bytes,
            )?;
            outputs.json("qz.native_forecast", RuntimeOutputKind::Report, &result)?;
        }
        NativeTaskParametersV1::ValidateAlpha {
            dataset_revision_id,
            model_artifact_id,
            request,
            ..
        } => {
            let bytes = read(
                &input.join("objects").join(model_artifact_id.to_string()),
                crate::signals::MAX_SIGNAL_MODULE_BYTES,
            )?;
            let result = crate::validation::validate_alpha(
                &input.join("catalogs").join(dataset_revision_id.to_string()),
                &request,
                &bytes,
            )?;
            outputs.json("qz.alpha_validation", RuntimeOutputKind::Report, &result)?;
        }
        NativeTaskParametersV1::EvaluateSealedAlpha {
            dataset_revision_id,
            model_artifact_id,
            calibration_artifact_id,
            request,
            ..
        } => {
            let bytes = read(
                &input.join("objects").join(model_artifact_id.to_string()),
                crate::signals::MAX_SIGNAL_MODULE_BYTES,
            )?;
            let calibration: Option<contracts::science::NativeFrozenCalibrationV1> =
                calibration_artifact_id
                    .map(|id| {
                        document(
                            &input.join("objects").join(id.to_string()),
                            PARAMETERS_LIMIT,
                        )
                    })
                    .transpose()?;
            let result = crate::validation::evaluate_sealed_alpha(
                &input.join("catalogs").join(dataset_revision_id.to_string()),
                &request,
                &bytes,
                calibration.as_ref(),
            )?;
            outputs.json("qz.alpha_sealed", RuntimeOutputKind::Report, &result)?;
        }
        NativeTaskParametersV1::StudyPortfolio {
            dataset_revision_id,
            request,
            ..
        } => {
            let result = crate::study::evaluate(
                &input.join("catalogs").join(dataset_revision_id.to_string()),
                &request,
                |id| {
                    read(
                        &input.join("objects").join(id.to_string()),
                        PARAMETERS_LIMIT,
                    )
                },
            )?;
            outputs.json("qz.portfolio_study", RuntimeOutputKind::Report, &result)?;
            outputs.history(&request, &result)?;
        }
        NativeTaskParametersV1::ComposeStrategyTargets {
            dataset_revision_id,
            request,
            ..
        } => {
            let settings: contracts::science::NativeSimulationSettingsV1 = document(
                &input.join("objects").join(
                    request
                        .mandate
                        .constraints
                        .transaction_costs_ref
                        .to_string(),
                ),
                PARAMETERS_LIMIT,
            )?;
            ensure!(
                serde_json::to_value(&settings)? == serde_json::to_value(&request.settings)?,
                "STRATEGY_SETTINGS_SOURCE_MISMATCH"
            );
            let result = crate::strategy::compose(
                &input.join("catalogs").join(dataset_revision_id.to_string()),
                &request,
                |id| {
                    let maximum = if request
                        .members
                        .iter()
                        .any(|member| member.policy.source.report_artifact_id == id)
                    {
                        contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize
                    } else {
                        2 * 1024 * 1024
                    };
                    read(&input.join("objects").join(id.to_string()), maximum)
                },
            )?;
            outputs.json("qz.strategy_portfolio", RuntimeOutputKind::Report, &result)?;
        }
        NativeTaskParametersV1::BuildPortfolio {
            dataset_revision_id,
            request,
            ..
        } => {
            // An infeasible solve is a real diagnostic report, not fabricated fallback targets.
            outputs.json(
                "qz.native_portfolio",
                RuntimeOutputKind::Report,
                &crate::portfolio::build(
                    &input.join("catalogs").join(dataset_revision_id.to_string()),
                    &request,
                    |id| {
                        read(
                            &input.join("objects").join(id.to_string()),
                            if request
                                .bar_liquidity
                                .as_ref()
                                .is_some_and(|b| b.assumption.report_artifact_id == id)
                            {
                                contracts::runtime_jobs::MAX_JOB_OUTPUT_BYTES as usize
                            } else {
                                PARAMETERS_LIMIT
                            },
                        )
                    },
                )?,
            )?;
        }
        NativeTaskParametersV1::SimulatePortfolio {
            dataset_revision_id,
            request,
            ..
        } => {
            let result = crate::simulation::simulate_explicit(
                &input.join("catalogs").join(dataset_revision_id.to_string()),
                &request,
                Some(dataset_revision_id),
            )?;
            outputs.json("qz.native_simulation", RuntimeOutputKind::Report, &result)?;
        }
        NativeTaskParametersV1::SimulateCandidate {
            candidate_id,
            candidate_available_ns,
            dataset_revision_id,
            target_artifact_id,
            settings_artifact_id,
            request,
            ..
        } => {
            let target: contracts::science::PortfolioTargetsV1 = serde_json::from_slice(&read(
                &input.join("objects").join(target_artifact_id.to_string()),
                PARAMETERS_LIMIT,
            )?)?;
            let settings: contracts::science::NativeSimulationSettingsV1 =
                serde_json::from_slice(&read(
                    &input.join("objects").join(settings_artifact_id.to_string()),
                    PARAMETERS_LIMIT,
                )?)?;
            ensure!(
                serde_json::to_value(&settings)? == serde_json::to_value(&request.settings)?,
                "CANDIDATE_SIMULATION_SETTINGS_SOURCE_MISMATCH"
            );
            domain::execution::candidate_simulation(
                candidate_id,
                candidate_available_ns,
                &target,
                &request,
            )?;
            let result = crate::simulation::simulate(
                &input.join("catalogs").join(dataset_revision_id.to_string()),
                &request,
            )?;
            outputs.json("qz.native_simulation", RuntimeOutputKind::Report, &result)?;
        }
        NativeTaskParametersV1::SimulatePortfolioSequence {
            dataset_revision_id,
            sources,
            settings_artifact_id,
            request,
            ..
        } => {
            let settings: contracts::science::NativeSimulationSettingsV1 = document(
                &input.join("objects").join(settings_artifact_id.to_string()),
                PARAMETERS_LIMIT,
            )?;
            ensure!(
                serde_json::to_value(&settings)? == serde_json::to_value(&request.settings)?,
                "PORTFOLIO_SEQUENCE_SETTINGS_SOURCE_MISMATCH"
            );
            let targets = sources
                .iter()
                .map(|source| {
                    document(
                        &input
                            .join("objects")
                            .join(source.target_artifact_id.to_string()),
                        PARAMETERS_LIMIT,
                    )
                })
                .collect::<Result<Vec<contracts::science::PortfolioTargetsV1>>>()?;
            domain::execution::portfolio_sequence(&sources, &targets, &request)?;
            let result = crate::simulation::simulate(
                &input.join("catalogs").join(dataset_revision_id.to_string()),
                &request,
            )?;
            outputs.json("qz.native_simulation", RuntimeOutputKind::Report, &result)?;
        }
    }
    ensure!(
        !domain::execution_limits::expired(spec.deadline_at, chrono::Utc::now()),
        "NATIVE_JOB_DEADLINE"
    );
    outputs.seal()
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fn test_outputs(root: &Path, remaining: u64) -> Outputs {
        Outputs {
            root: root.to_owned(),
            remaining: Some(remaining),
            items: Vec::new(),
        }
    }

    fn signal() -> Vec<u8> {
        wat::parse_str(
            r#"(module (func (export "predict")
                (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64)
                f64.const 0))"#,
        )
        .unwrap()
    }

    // This checks the live mount contract only. The native_oci integration test
    // separately executes the fixed official rustc image and Runtime scanner.
    fn scan_flat_output(root: &Path, maximum: u64) -> u64 {
        let mut total = 0;
        for entry in fs::read_dir(root).unwrap() {
            let metadata = fs::symlink_metadata(entry.unwrap().path()).unwrap();
            assert!(metadata.is_file());
            assert_eq!(metadata.nlink(), 1);
            total += metadata.len();
        }
        assert!(total <= maximum);
        total
    }

    #[test]
    fn absent_output_budget_keeps_exact_actual_bytes_and_publication_guards() {
        let root = tempfile::tempdir().unwrap();
        let mut outputs = Outputs {
            root: root.path().to_owned(),
            remaining: None,
            items: Vec::new(),
        };
        let first = outputs.compiled_model(b"first output").unwrap();
        let second = outputs.compiled_model(b"another output").unwrap();
        assert_eq!(outputs.remaining, None);
        assert_eq!(outputs.items[0].byte_count.get(), 12);
        assert_eq!(outputs.items[1].byte_count.get(), 14);
        assert_eq!(
            fs::read(root.path().join(first.to_string())).unwrap(),
            b"first output"
        );
        assert_eq!(
            fs::read(root.path().join(second.to_string())).unwrap(),
            b"another output"
        );
        assert_eq!(
            fs::metadata(root.path().join(first.to_string()))
                .unwrap()
                .nlink(),
            1
        );
    }

    #[test]
    fn failed_bounded_write_removes_partial_output_without_spending_budget() {
        let root = tempfile::tempdir().unwrap();
        let mut outputs = test_outputs(root.path(), 5);
        let error = outputs
            .document(
                "test",
                RuntimeOutputKind::Model,
                "application/wasm",
                |stream| {
                    stream.write_all(b"1234")?;
                    assert_eq!(scan_flat_output(root.path(), 5), 4);
                    // A partial output is still a temporary, never a committed ID.
                    assert!(fs::read_dir(root.path()).unwrap().all(|entry| entry
                        .unwrap()
                        .file_name()
                        .to_str()
                        .unwrap()
                        .starts_with('.')));
                    Ok(stream.write_all(b"56")?)
                },
            )
            .unwrap_err();
        assert!(error.to_string().contains("NATIVE_OUTPUT_LIMIT"));
        assert_eq!(outputs.remaining, Some(5));
        assert!(outputs.items.is_empty());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
    }

    #[test]
    fn output_publication_never_overwrites_or_adds_a_link() {
        let root = tempfile::tempdir().unwrap();
        let target = root.path().join(Id::new().to_string());
        fs::write(&target, b"original").unwrap();
        let mut staged = tempfile::NamedTempFile::new_in(root.path()).unwrap();
        staged.write_all(b"replacement").unwrap();
        frozen(staged.as_file()).unwrap();
        assert!(publish(&staged, &target).is_err());
        assert_eq!(fs::read(&target).unwrap(), b"original");
        assert_eq!(staged.as_file().metadata().unwrap().nlink(), 1);
        drop(staged);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);

        let mut outputs = test_outputs(root.path(), 100);
        let id = outputs.compiled_model(b"validated fixture").unwrap();
        let metadata = fs::metadata(root.path().join(id.to_string())).unwrap();
        assert_eq!(metadata.nlink(), 1);
        assert_eq!(metadata.mode() & 0o777, 0o444);
        assert_eq!(
            outputs.remaining,
            Some(100 - b"validated fixture".len() as u64)
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    #[test]
    fn json_and_arrow_outputs_keep_exact_content_budget_and_index() {
        use arrow_array::{
            ArrayRef, Decimal128Array, RecordBatch, StringArray, TimestampNanosecondArray,
        };
        use contracts::portfolio_history as history;
        use std::sync::Arc;

        let root = tempfile::tempdir().unwrap();
        let mut outputs = test_outputs(root.path(), 16 * 1024);
        let json = serde_json::json!({"fixture": "unchanged report"});
        let report = outputs
            .json("test", RuntimeOutputKind::Report, &json)
            .unwrap();
        assert_eq!(
            fs::read(root.path().join(report.to_string())).unwrap(),
            serde_json::to_vec(&json).unwrap()
        );
        let time: ArrayRef =
            Arc::new(TimestampNanosecondArray::from(vec![123]).with_timezone("UTC"));
        let weight: ArrayRef = Arc::new(
            Decimal128Array::from(vec![Some(0)])
                .with_precision_and_scale(38, 18)
                .unwrap(),
        );
        let batch = RecordBatch::try_new(
            history::schema(),
            vec![
                time.clone(),
                time.clone(),
                time,
                Arc::new(StringArray::from(vec!["A"])),
                Arc::new(StringArray::from(vec!["USD"])),
                Arc::new(StringArray::from(vec!["OPTIMAL"])),
                weight.clone(),
                weight,
            ],
        )
        .unwrap();
        let id = outputs
            .document(
                history::NAME,
                RuntimeOutputKind::Targets,
                history::MEDIA_TYPE,
                |stream| Ok(history::write(stream, &batch)?),
            )
            .unwrap();
        let bytes = read(&root.path().join(id.to_string()), 16 * 1024).unwrap();
        assert_eq!(history::read(&bytes).unwrap(), batch);
        let artifacts = outputs.items.clone();
        assert_eq!(
            scan_flat_output(root.path(), 16 * 1024),
            16 * 1024 - outputs.remaining.unwrap()
        );
        outputs.seal().unwrap();
        let index: NativeJobOutputIndexV1 =
            document(&root.path().join("index.json"), SPEC_LIMIT).unwrap();
        assert_eq!(
            serde_json::to_value(index.artifacts).unwrap(),
            serde_json::to_value(artifacts).unwrap()
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 3);
    }

    #[test]
    fn failed_index_write_or_publish_leaves_no_partial_index() {
        let root = tempfile::tempdir().unwrap();
        let mut outputs = test_outputs(root.path(), 1024);
        outputs
            .json("test", RuntimeOutputKind::Report, &true)
            .unwrap();
        outputs.items[0].schema.name = "x".repeat(SPEC_LIMIT);
        assert!(outputs
            .seal()
            .unwrap_err()
            .to_string()
            .contains("NATIVE_OUTPUT_LIMIT"));
        assert!(!root.path().join("index.json").exists());
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 1);

        fs::write(root.path().join("index.json"), b"original index").unwrap();
        assert!(test_outputs(root.path(), 1024).seal().is_err());
        assert_eq!(
            fs::read(root.path().join("index.json")).unwrap(),
            b"original index"
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }

    // Deliberately controlled compiler fixture, not evidence of official rustc
    // execution. It holds an adjacent linker scratch directory until the test
    // has scanned output, then emits the supplied WASM or a compilation error.
    fn controlled_compile(
        wasm: &[u8],
        capacity: u64,
        features: bool,
        exit_code: u8,
    ) -> (tempfile::TempDir, Outputs, Result<()>) {
        let fixture = tempfile::tempdir().unwrap();
        let input = fixture.path().join("input");
        let output = fixture.path().join("output");
        fs::create_dir_all(input.join("objects")).unwrap();
        fs::create_dir(&output).unwrap();
        let code = Id::new();
        fs::write(input.join("objects").join(code.to_string()), b"fixture").unwrap();
        fs::write(fixture.path().join("module.wasm"), wasm).unwrap();
        fs::write(fixture.path().join("exit-code"), exit_code.to_string()).unwrap();
        let compiler = fixture.path().join("controlled-compiler");
        fs::write(
            &compiler,
            r#"#!/bin/sh
set -eu
if [ "$1" = '--version' ]; then
    printf '%s\n' 'rustc 1.98.1 (controlled fixture)'
    exit 0
fi
fixture=${0%/*}
while [ "$1" != '-o' ]; do shift; done
target=$2
mkdir "${target%/*}/rustc-controlled-linker"
printf '%s' "$target" > "$fixture/ready.tmp"
mv "$fixture/ready.tmp" "$fixture/ready"
while [ ! -f "$fixture/release" ]; do sleep 0.01; done
cp "$fixture/module.wasm" "$target"
exit "$(cat "$fixture/exit-code")"
"#,
        )
        .unwrap();
        fs::set_permissions(&compiler, fs::Permissions::from_mode(0o700)).unwrap();
        let parameters = NativeTaskParametersV1::CompileModel {
            schema_version: SchemaV1,
            code_artifact_id: code,
        };
        let run_id = Id::new();
        let spec = JobSpecV1 {
            schema_version: SchemaV1,
            run_id,
            attempt_no: 1,
            owner_epoch: Revision::INITIAL,
            external_job_id: domain::runtime_jobs::external_id(run_id, 1).unwrap(),
            job_kind: parameters.job_kind(),
            image_ref: format!(
                "example.invalid/controlled-compiler@sha256:{}",
                "a".repeat(64)
            ),
            input_set_id: Id::new(),
            inputs: Vec::new(),
            parameters_artifact_id: Id::new(),
            limits: contracts::runtime_jobs::RuntimeJobLimitsV1 {
                cpu: 1,
                cpu_seconds: Some(counter(10).unwrap()),
                memory_mib: 512,
                wall_seconds: Some(10),
                output_bytes: Some(counter(capacity).unwrap()),
            },
            deadline_at: Some(chrono::Utc::now() + chrono::Duration::seconds(10)),
            requested_output_schemas: parameters.output_schemas(),
        };
        let mut outputs = test_outputs(&output, capacity);
        let (outputs, result) = std::thread::scope(|scope| {
            let running = scope.spawn(move || {
                let result = compile_with(&spec, &input, code, &mut outputs, features, &compiler);
                (outputs, result)
            });
            let began = Instant::now();
            let ready = fixture.path().join("ready");
            while !ready.is_file() {
                assert!(began.elapsed() < Duration::from_secs(5));
                std::thread::sleep(Duration::from_millis(5));
            }
            let target = PathBuf::from(fs::read_to_string(ready).unwrap());
            let staging = target.parent().unwrap();
            assert!(staging.starts_with("/tmp"));
            assert_ne!(staging, output);
            assert_eq!(fs::metadata(staging).unwrap().mode() & 0o777, 0o700);
            assert!(staging.join("rustc-controlled-linker").is_dir());
            assert!(!target.exists());
            assert_eq!(scan_flat_output(&output, capacity), 0);
            assert_eq!(fs::read_dir(&output).unwrap().count(), 0);
            fs::write(fixture.path().join("release"), b"").unwrap();
            let result = running.join().unwrap();
            assert!(
                !staging.exists(),
                "compiler scratch must be removed on every return"
            );
            result
        });
        (fixture, outputs, result)
    }

    #[test]
    fn compiler_scratch_is_private_until_valid_model_publication() {
        let wasm = signal();
        let (fixture, outputs, result) = controlled_compile(&wasm, 8192, false, 0);
        result.unwrap();
        assert_eq!(outputs.items.len(), 2);
        let model = outputs
            .items
            .iter()
            .find(|item| item.kind == RuntimeOutputKind::Model)
            .unwrap();
        assert_eq!(
            fs::read(outputs.root.join(model.storage_ref.to_string())).unwrap(),
            wasm
        );
        let report = outputs
            .items
            .iter()
            .find(|item| item.kind == RuntimeOutputKind::Report)
            .unwrap();
        let report: NativeModelCompilationV1 =
            document(&outputs.root.join(report.storage_ref.to_string()), 8192).unwrap();
        assert_eq!(report.model_storage_ref, model.storage_ref);
        assert_eq!(report.module_bytes.get(), wasm.len() as u64);
        assert_eq!(
            scan_flat_output(&outputs.root, 8192),
            8192 - outputs.remaining.unwrap()
        );
        assert!(!outputs.root.join("index.json").exists());
        outputs.seal().unwrap();
        let output = fixture.path().join("output");
        assert_eq!(fs::read_dir(&output).unwrap().count(), 3);
        let index: NativeJobOutputIndexV1 =
            document(&output.join("index.json"), SPEC_LIMIT).unwrap();
        assert_eq!(index.artifacts.len(), 2);
    }

    #[test]
    fn compiler_rejections_leave_no_model_report_or_temporary_output() {
        let invalid_abi =
            wat::parse_str(r#"(module (func (export "predict") (result f64) f64.const 0))"#)
                .unwrap();
        let wasm = signal();
        for (bytes, capacity, features, exit_code, expected) in [
            (
                wasm.clone(),
                wasm.len() as u64 - 1,
                false,
                0,
                "NATIVE_OUTPUT_LIMIT",
            ),
            (invalid_abi, 8192, false, 0, "SIGNAL_ABI_MISMATCH"),
            (wasm.clone(), 8192, true, 0, "FEATURE_MODEL_ABI_MISMATCH"),
            (
                vec![0; crate::signals::MAX_SIGNAL_MODULE_BYTES + 1],
                8192,
                false,
                0,
                "NATIVE_FILE_LIMIT",
            ),
            (wasm, 8192, false, 1, "NATIVE_COMPILATION_FAILED"),
        ] {
            let (_fixture, outputs, result) =
                controlled_compile(&bytes, capacity, features, exit_code);
            assert!(
                result.unwrap_err().to_string().contains(expected),
                "{expected}"
            );
            assert_eq!(outputs.remaining, Some(capacity));
            assert!(outputs.items.is_empty());
            assert_eq!(fs::read_dir(&outputs.root).unwrap().count(), 0);
        }
    }
}
