//! Deployment-only historical file copying. No HTTP/MCP path or source credential input.
use contracts::{imports::*, DbCounter, SchemaV1};
use integrations::{
    artifacts::{ArtifactStore, MAX_LOCAL_OBJECT_BYTES},
    mission_files::MissionFiles,
};
use serde::Deserialize;
use std::{collections::BTreeSet, fs, io::Write, path::Path};

// Deliberately local configuration, not a public API contract or generated schema.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    schema_version: SchemaV1,
    source_installation_id: contracts::Id,
    artifacts: Vec<SelectedArtifact>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SelectedArtifact {
    identity: HistoricalIdentityV1,
    relative_path: Option<String>,
    disposition: Disposition,
}
#[derive(Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
enum Disposition {
    CopyPublic,
    SealedRetained,
    ManualReviewRequired,
}

fn invalid() -> std::io::Error {
    std::io::Error::other("historical artifact export failed; no complete report is available")
}

/// Caller must supply an independent, stable source copy and reviewed local selection.
/// Failure leaves only this invocation's new output directory for operator recovery.
pub fn export(source_root: &Path, selection: &Path, output: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if !output.is_absolute() {
        return Err(invalid());
    }
    // Resolve only the trusted configuration directory; the file itself must not be a link.
    let selection = selection
        .parent()
        .ok_or_else(invalid)?
        .canonicalize()
        .map_err(|_| invalid())?
        .join(selection.file_name().ok_or_else(invalid)?);
    let selection_root =
        MissionFiles::open(selection.parent().ok_or_else(invalid)?).map_err(|_| invalid())?;
    let bytes = selection_root
        .read_bytes(
            selection
                .file_name()
                .and_then(|v| v.to_str())
                .ok_or_else(invalid)?,
            4 * 1024 * 1024,
        )
        .map_err(|_| invalid())?;
    let selected: Selection = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
    let _ = selected.schema_version;
    if selected.artifacts.is_empty() || selected.artifacts.len() > 10_000 {
        return Err(invalid());
    }
    let mut identities = BTreeSet::new();
    for item in &selected.artifacts {
        if item.identity.kind != HistoricalKindV1::Artifact
            || !matches!(
                item.identity.source_table.as_str(),
                "mission_artifacts" | "alpha_signal_artifacts"
            )
            || item.identity.source_id.is_nil()
            || item.identity.source_id.is_max()
            || !identities.insert(item.identity.clone())
            || (matches!(item.disposition, Disposition::CopyPublic) && item.relative_path.is_none())
        {
            return Err(invalid());
        }
    }
    let source = MissionFiles::open(source_root).map_err(|_| invalid())?;
    // This command owns only a new directory, never an existing export or user backup.
    fs::DirBuilder::new()
        .mode(0o700)
        .create(output)
        .map_err(|_| invalid())?;
    fs::File::open(output.parent().ok_or_else(invalid)?)?.sync_all()?;
    let objects = ArtifactStore::open(&output.join("objects")).map_err(|_| invalid())?;
    let mut artifacts = Vec::with_capacity(selected.artifacts.len());
    for item in selected.artifacts {
        let mut result = HistoricalArtifactCopyV1 {
            identity: item.identity,
            outcome: HistoricalArtifactOutcomeV1::ManualReviewRequired,
            object_ref: None,
            byte_count: None,
        };
        match item.disposition {
            Disposition::SealedRetained => {
                result.outcome = HistoricalArtifactOutcomeV1::SealedRetained
            }
            Disposition::ManualReviewRequired => {}
            Disposition::CopyPublic => match source.read_bytes(
                item.relative_path.as_deref().ok_or_else(invalid)?,
                MAX_LOCAL_OBJECT_BYTES,
            ) {
                Ok(bytes) => {
                    let id = contracts::Id::new();
                    let count = DbCounter::new(bytes.len() as u64).map_err(|_| invalid())?;
                    objects.put(id, &bytes).map_err(|_| invalid())?;
                    if objects.read(id, count).map_err(|_| invalid())? != bytes {
                        return Err(invalid());
                    }
                    result.outcome = HistoricalArtifactOutcomeV1::Copied;
                    result.object_ref = Some(id);
                    result.byte_count = Some(count);
                }
                Err(error) => {
                    result.outcome = match error.kind() {
                        std::io::ErrorKind::NotFound => HistoricalArtifactOutcomeV1::Missing,
                        std::io::ErrorKind::InvalidInput | std::io::ErrorKind::NotADirectory => {
                            HistoricalArtifactOutcomeV1::Unsupported
                        }
                        _ => HistoricalArtifactOutcomeV1::Unreadable,
                    }
                }
            },
        }
        artifacts.push(result);
    }
    let report = HistoricalArtifactExportV1 {
        schema_version: SchemaV1,
        source_installation_id: selected.source_installation_id,
        exported_at: chrono::Utc::now(),
        artifacts,
    };
    let bytes = serde_json::to_vec_pretty(&report).map_err(|_| invalid())?;
    use std::os::unix::fs::OpenOptionsExt;
    let mut pending = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(output.join("report.pending"))?;
    pending.write_all(&bytes)?;
    pending.sync_all()?;
    fs::rename(output.join("report.pending"), output.join("report.json"))?;
    fs::File::open(output)?.sync_all()?;
    Ok(())
}
