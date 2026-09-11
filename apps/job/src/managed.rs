//! One native job per isolated process, using fixed mounts and typed operations.
//! Neither compilation nor successful execution confers REAL data or qualification.
use anyhow::{ensure, Result};
use contracts::{
    execution::*,
    runtime_jobs::{JobSpecV1, RuntimeOutputKind, RuntimeOutputV1},
    DbCounter, Id, Revision, SchemaV1,
};
use nautilus_model::instruments::Instrument;
use serde::{de::DeserializeOwned, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    time::{Duration, Instant},
};

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
fn create(path: &Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options
            .mode(0o600)
            .custom_flags(rustix::fs::OFlags::NOFOLLOW.bits() as i32);
    }
    Ok(options.open(path)?)
}

struct LimitedFile {
    file: File,
    remaining: u64,
    written: u64,
}
impl Write for LimitedFile {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if bytes.len() as u64 > self.remaining {
            return Err(std::io::Error::other("NATIVE_OUTPUT_LIMIT"));
        }
        let written = self.file.write(bytes)?;
        self.remaining -= written as u64;
        self.written += written as u64;
        Ok(written)
    }
    fn flush(&mut self) -> std::io::Result<()> {
        self.file.flush()
    }
}

struct Outputs {
    root: PathBuf,
    remaining: u64,
    items: Vec<RuntimeOutputV1>,
}
impl Outputs {
    fn json(&mut self, name: &str, kind: RuntimeOutputKind, value: &impl Serialize) -> Result<Id> {
        let id = Id::new();
        let mut stream = LimitedFile {
            file: create(&self.root.join(id.to_string()))?,
            remaining: self.remaining,
            written: 0,
        };
        serde_json::to_writer(&mut stream, value)?;
        stream.flush()?;
        ensure!(stream.written > 0, "NATIVE_EMPTY_OUTPUT");
        frozen(&stream.file)?;
        self.remaining -= stream.written;
        self.items.push(RuntimeOutputV1 {
            kind,
            schema: contracts::runtime::RuntimeArtifactSchemaV1 {
                name: name.into(),
                version: "1".into(),
            },
            storage_ref: id,
            storage_version: Revision::INITIAL,
            byte_count: counter(stream.written)?,
            media_type: "application/json".into(),
        });
        Ok(id)
    }
    fn compiled_model(&mut self, id: Id, bytes: &[u8]) -> Result<()> {
        ensure!(bytes.len() as u64 <= self.remaining, "NATIVE_OUTPUT_LIMIT");
        let file = File::open(self.root.join(id.to_string()))?;
        frozen(&file)?;
        self.remaining -= bytes.len() as u64;
        self.items.push(RuntimeOutputV1 {
            kind: RuntimeOutputKind::Model,
            schema: contracts::runtime::RuntimeArtifactSchemaV1 {
                name: "qz.wasm_model".into(),
                version: "1".into(),
            },
            storage_ref: id,
            storage_version: Revision::INITIAL,
            byte_count: counter(bytes.len() as u64)?,
            media_type: "application/wasm".into(),
        });
        Ok(())
    }
    fn seal(self) -> Result<()> {
        let index = NativeJobOutputIndexV1 {
            schema_version: SchemaV1,
            artifacts: self.items,
        };
        let mut stream = LimitedFile {
            file: create(&self.root.join("index.json"))?,
            remaining: SPEC_LIMIT as u64,
            written: 0,
        };
        serde_json::to_writer(&mut stream, &index)?;
        frozen(&stream.file)?;
        File::open(&self.root)?.sync_all()?;
        Ok(())
    }
}

struct CompilerChild(Child);
impl Drop for CompilerChild {
    fn drop(&mut self) {
        // Reap only this owned compiler. The outer native cgroup owns all descendants.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn compile(spec: &JobSpecV1, input: &Path, code: Id, outputs: &mut Outputs) -> Result<()> {
    let source = input.join("objects").join(code.to_string());
    let bytes = read(&source, crate::signals::MAX_SIGNAL_MODULE_BYTES)?;
    std::str::from_utf8(&bytes)?;
    let version = Command::new(COMPILER)
        .arg("--version")
        .env_clear()
        .env("PATH", "/opt/rust/bin:/usr/bin:/bin")
        .env("HOME", "/tmp")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()?;
    ensure!(
        version.status.success() && version.stdout.len() <= 512,
        "NATIVE_COMPILER_VERSION"
    );
    let version = std::str::from_utf8(&version.stdout)?.trim();
    ensure!(
        version.starts_with("rustc 1.98.1 "),
        "NATIVE_COMPILER_VERSION"
    );
    let model = Id::new();
    let target = outputs.root.join(model.to_string());
    ensure!(!target.exists(), "NATIVE_OUTPUT_EXISTS");
    let child = Command::new(COMPILER)
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
            ensure!(status.success(), "NATIVE_COMPILATION_FAILED");
            break;
        }
        ensure!(
            chrono::Utc::now() < spec.deadline_at
                && began.elapsed() < Duration::from_secs(u64::from(spec.limits.wall_seconds)),
            "NATIVE_COMPILATION_DEADLINE"
        );
        std::thread::sleep(Duration::from_millis(20));
    }
    let wasm = read(&target, crate::signals::MAX_SIGNAL_MODULE_BYTES)?;
    // Native Wasmi verifies imports, start functions, limits and exact predict ABI.
    crate::signals::WasmSignal::new(&wasm, 1, 100_000)?;
    outputs.compiled_model(model, &wasm)?;
    outputs.json(
        "qz.model_compilation",
        RuntimeOutputKind::Report,
        &NativeModelCompilationV1 {
            schema_version: SchemaV1,
            code_artifact_id: code,
            model_storage_ref: model,
            rustc_version: version.to_owned(),
            target: "wasm32-unknown-unknown".into(),
            abi: "predict(f64,f64,f64,f64,f64,f64,f64,f64)->f64".into(),
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
    ensure!(chrono::Utc::now() < spec.deadline_at, "NATIVE_JOB_DEADLINE");
    let mut outputs = Outputs {
        root: output.to_owned(),
        remaining: spec.limits.output_bytes.get(),
        items: Vec::new(),
    };
    match parameters {
        NativeTaskParametersV1::CompileModel {
            code_artifact_id, ..
        } => compile(&spec, input, code_artifact_id, &mut outputs)?,
        NativeTaskParametersV1::ValidateData { selections, .. } => {
            let mut datasets = Vec::with_capacity(selections.len());
            for selected in selections {
                let data = crate::catalog::load_catalog(
                    &input
                        .join("catalogs")
                        .join(selected.dataset_revision_id.to_string()),
                    &selected.selection,
                )?;
                let mut first = u64::MAX;
                let mut last = 0;
                let mut available = 0;
                let mut instrument_ids = Vec::with_capacity(data.series.len());
                for series in &data.series {
                    instrument_ids.push(series.instrument.id().to_string());
                    for bar in &series.bars {
                        first = first.min(bar.ts_event.as_u64());
                        last = last.max(bar.ts_event.as_u64());
                        available = available.max(bar.ts_init.as_u64());
                    }
                }
                datasets.push(NativeDatasetQualityV1 {
                    dataset_revision_id: selected.dataset_revision_id,
                    selection: selected.selection,
                    row_count: counter(data.rows as u64)?,
                    instrument_ids,
                    first_event_ns: counter(first)?,
                    last_event_ns: counter(last)?,
                    available_through_ns: counter(available)?,
                });
            }
            outputs.json(
                "qz.data_quality",
                RuntimeOutputKind::DataQuality,
                &NativeDataQualityReportV1 {
                    schema_version: SchemaV1,
                    native_version: "nautilus-persistence/0.63.0".into(),
                    checked_at: chrono::DateTime::from_timestamp_micros(
                        chrono::Utc::now().timestamp_micros(),
                    )
                    .ok_or_else(|| anyhow::anyhow!("NATIVE_CLOCK"))?,
                    datasets,
                },
            )?;
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
        NativeTaskParametersV1::BuildPortfolio { request, .. } => {
            // An infeasible solve is a real diagnostic report, not fabricated fallback targets.
            outputs.json(
                "qz.native_allocation",
                RuntimeOutputKind::Report,
                &crate::allocate(&request)?,
            )?;
        }
        NativeTaskParametersV1::SimulatePortfolio {
            dataset_revision_id,
            request,
            ..
        } => {
            let result = crate::simulation::simulate(
                &input.join("catalogs").join(dataset_revision_id.to_string()),
                &request,
            )?;
            outputs.json("qz.native_simulation", RuntimeOutputKind::Report, &result)?;
        }
    }
    ensure!(chrono::Utc::now() < spec.deadline_at, "NATIVE_JOB_DEADLINE");
    outputs.seal()
}
