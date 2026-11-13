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
    io::{BufWriter, Read, Write},
    path::{Path, PathBuf},
};

fn counter(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(|_| anyhow::anyhow!("NATIVE_COUNTER_RANGE"))
}

fn read(path: &Path) -> Result<Vec<u8>> {
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
        metadata.is_file() && metadata.len() > 0,
        "NATIVE_FILE_LIMIT"
    );
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        ensure!(metadata.nlink() == 1, "NATIVE_FILE_IDENTITY");
    }
    let capacity = usize::try_from(metadata.len())?;
    let mut bytes = Vec::new();
    bytes.try_reserve_exact(capacity)?;
    file.take(metadata.len().saturating_add(1))
        .read_to_end(&mut bytes)?;
    ensure!(
        bytes.len() == metadata.len() as usize,
        "NATIVE_FILE_CHANGED"
    );
    Ok(bytes)
}
fn document<T: DeserializeOwned>(path: &Path) -> Result<T> {
    Ok(serde_json::from_slice(&read(path)?)?)
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

fn write_json(writer: impl Write, value: &impl Serialize) -> Result<()> {
    let mut buffered = BufWriter::with_capacity(64 * 1024, writer);
    serde_json::to_writer(&mut buffered, value)?;
    // A successful serializer can leave all bytes in memory. Surface quota and
    // disk errors before freezing the file or publishing its descriptor.
    buffered.flush()?;
    Ok(())
}

struct Outputs {
    root: PathBuf,
    remaining: Option<u64>,
    items: Vec<RuntimeOutputV1>,
}
impl Outputs {
    fn json(&mut self, name: &str, kind: RuntimeOutputKind, value: &impl Serialize) -> Result<Id> {
        self.document(name, kind, "application/json", |stream| {
            write_json(stream, value)
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
    #[cfg(test)]
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
            remaining: None,
            written: 0,
        };
        write_json(&mut stream, &index)?;
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
    let spec: JobSpecV1 = document(&input.join("spec.json"))?;
    let parameters: NativeTaskParametersV1 = document(
        &input
            .join("objects")
            .join(spec.parameters_artifact_id.to_string()),
    )?;
    ensure!(
        !matches!(
            parameters,
            NativeTaskParametersV1::CompileModel { .. }
                | NativeTaskParametersV1::CompileFeatureModel { .. }
                | NativeTaskParametersV1::EvaluateExperiment { .. }
                | NativeTaskParametersV1::ComposeStrategyTargets { .. }
                | NativeTaskParametersV1::SimulatePortfolio { .. }
                | NativeTaskParametersV1::SimulateCandidate { .. }
                | NativeTaskParametersV1::SimulatePortfolioSequence { .. }
        ),
        "NATIVE_RESEARCH_OPERATION_RETIRED"
    );
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
        NativeTaskParametersV1::ValidateData { .. } => {}
        NativeTaskParametersV1::EvaluateForward { request, .. } => {
            let result = crate::forward::evaluate(&request, |id| {
                let bytes = read(&input.join("objects").join(id.to_string()))?;
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
        // Original task documents remain readable, but Execute cannot revive retired research.
        NativeTaskParametersV1::CompileModel { .. }
        | NativeTaskParametersV1::CompileFeatureModel { .. }
        | NativeTaskParametersV1::EvaluateExperiment { .. }
        | NativeTaskParametersV1::ComposeStrategyTargets { .. }
        | NativeTaskParametersV1::EvaluateAlpha { .. }
        | NativeTaskParametersV1::StudyPortfolio { .. }
        | NativeTaskParametersV1::BuildPortfolio { .. }
        | NativeTaskParametersV1::SimulatePortfolio { .. }
        | NativeTaskParametersV1::SimulateCandidate { .. }
        | NativeTaskParametersV1::SimulatePortfolioSequence { .. } => {
            anyhow::bail!("NATIVE_RESEARCH_OPERATION_RETIRED")
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
    use std::os::unix::fs::MetadataExt;

    fn test_outputs(root: &Path, remaining: u64) -> Outputs {
        Outputs {
            root: root.to_owned(),
            remaining: Some(remaining),
            items: Vec::new(),
        }
    }

    // Exercises output mount and byte-accounting rules; no compiler is run.
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
    fn buffered_json_keeps_exact_bytes_with_batched_writes() {
        #[derive(Default)]
        struct Writes {
            bytes: Vec<u8>,
            calls: usize,
        }
        impl Write for Writes {
            fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
                self.calls += 1;
                self.bytes.extend_from_slice(bytes);
                Ok(bytes.len())
            }
            fn flush(&mut self) -> std::io::Result<()> {
                Ok(())
            }
        }
        let rows = vec![
            serde_json::json!({
                "timestamp": "1790000000000000001", "decimal": "0.000000000000000001"
            });
            10_000
        ];
        let mut writer = Writes::default();
        write_json(&mut writer, &rows).unwrap();
        assert_eq!(writer.bytes, serde_json::to_vec(&rows).unwrap());
        assert!(
            writer.calls < 100,
            "JSON tokens must not each reach the file"
        );
    }

    #[test]
    fn buffered_json_flush_enforces_exact_budget_before_publication() {
        for count in [1, 2048] {
            let value = vec![serde_json::json!({"clock": "1790000000000000001"}); count];
            let expected = serde_json::to_vec(&value).unwrap();
            let size = expected.len() as u64;
            let root = tempfile::tempdir().unwrap();
            let mut outputs = test_outputs(root.path(), size - 1);
            let error = outputs
                .json("test", RuntimeOutputKind::Report, &value)
                .unwrap_err();
            assert!(error.to_string().contains("NATIVE_OUTPUT_LIMIT"));
            assert_eq!(outputs.remaining, Some(size - 1));
            assert!(outputs.items.is_empty());
            assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);

            let mut outputs = test_outputs(root.path(), size);
            let id = outputs
                .json("test", RuntimeOutputKind::Report, &value)
                .unwrap();
            assert_eq!(
                fs::read(root.path().join(id.to_string())).unwrap(),
                expected
            );
            assert_eq!(outputs.items[0].byte_count.get(), size);
            assert_eq!(outputs.remaining, Some(0));
        }
    }

    #[test]
    fn buffered_json_reports_a_write_error_deferred_until_flush() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("read-only-handle");
        File::create(&path).unwrap();
        let mut writer = LimitedFile {
            file: File::open(path).unwrap(),
            remaining: None,
            written: 0,
        };
        assert!(write_json(&mut writer, &true).is_err());
        assert_eq!(writer.written, 0);
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
        let bytes = read(&root.path().join(id.to_string())).unwrap();
        assert_eq!(history::read(&bytes).unwrap(), batch);
        let artifacts = outputs.items.clone();
        assert_eq!(
            scan_flat_output(root.path(), 16 * 1024),
            16 * 1024 - outputs.remaining.unwrap()
        );
        outputs.seal().unwrap();
        let index: NativeJobOutputIndexV1 = document(&root.path().join("index.json")).unwrap();
        assert_eq!(
            serde_json::to_value(index.artifacts).unwrap(),
            serde_json::to_value(artifacts).unwrap()
        );
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 3);
    }

    #[test]
    fn complete_large_index_and_failed_publish_preserve_original_files() {
        let root = tempfile::tempdir().unwrap();
        let mut outputs = test_outputs(root.path(), 1024);
        outputs
            .json("test", RuntimeOutputKind::Report, &true)
            .unwrap();
        assert_eq!(outputs.remaining, Some(1020));
        outputs.items[0].schema.name = "x".repeat(1024 * 1024 + 1);
        let expected = serde_json::to_vec(&NativeJobOutputIndexV1 {
            schema_version: SchemaV1,
            artifacts: outputs.items.clone(),
        })
        .unwrap();
        outputs.seal().unwrap();
        assert_eq!(fs::read(root.path().join("index.json")).unwrap(), expected);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);

        // Retry publication against the actual immutable index. Rewriting this
        // 0444 file would fail for non-root users before exercising NOREPLACE.
        let original = fs::symlink_metadata(root.path().join("index.json")).unwrap();
        assert!(original.is_file());
        assert_eq!(original.nlink(), 1);
        assert_eq!(original.mode() & 0o777, 0o444);
        let error = test_outputs(root.path(), 1024).seal().unwrap_err();
        assert_eq!(
            error.downcast_ref::<std::io::Error>().unwrap().kind(),
            std::io::ErrorKind::AlreadyExists
        );
        assert_eq!(fs::read(root.path().join("index.json")).unwrap(), expected);
        let preserved = fs::symlink_metadata(root.path().join("index.json")).unwrap();
        assert!(preserved.is_file());
        assert_eq!(
            (preserved.dev(), preserved.ino()),
            (original.dev(), original.ino())
        );
        assert_eq!(preserved.nlink(), 1);
        assert_eq!(preserved.mode() & 0o777, 0o444);
        assert_eq!(fs::read_dir(root.path()).unwrap().count(), 2);
    }
}
