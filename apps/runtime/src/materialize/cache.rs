//! Reclaim only the Runtime's derived copies; SQLite objects and native identities remain.
use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub(super) fn staging(root: &RuntimeRoot, spec: &JobSpecV1) -> PathBuf {
    root.path
        .join("staging")
        .join(format!("{}-{}", spec.run_id, spec.attempt_no))
}

fn remove_tree(path: &Path, remaining: &mut usize, depth: u8) -> Result<()> {
    if *remaining == 0 || depth > 32 {
        return Err(Failure::Invalid("materialization_cleanup_limit"));
    }
    *remaining -= 1;
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.is_dir() {
        // Only derived, stopped-job directories reach this routine. The retained
        // descriptor owns permission changes; links are unlinked, never followed.
        let directory = files::directory_handle(path)?;
        directory.set_permissions(fs::Permissions::from_mode(0o700))?;
        for child in fs::read_dir(path)? {
            remove_tree(&child?.path(), remaining, depth + 1)?;
        }
        directory.sync_all()?;
        fs::remove_dir(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

pub(super) fn discard_staging(root: &RuntimeRoot, spec: &JobSpecV1) -> Result<()> {
    let path = staging(root, spec);
    // The caller owns this canonical slot's durable reservation and the exclusive
    // Runtime file lock. It is never an OCI mount or a caller-provided filename.
    let mut remaining = 4096;
    remove_tree(&path, &mut remaining, 0)?;
    File::open(root.path.join("staging"))?.sync_all()?;
    Ok(())
}

fn exact_spec(directory: &Path) -> Result<JobSpecV1> {
    files::canonical_directory(directory)?;
    let input = files::directory_handle(&directory.join("input"))?;
    let bytes = files::read_child(
        &input,
        "spec.json",
        domain::runtime_jobs::MAX_JOB_REQUEST_BYTES,
    )?;
    serde_json::from_slice(&bytes).map_err(|_| Failure::Invalid("materialization_spec"))
}

fn verify_copy(root: &RuntimeRoot, spec: &JobSpecV1) -> Result<()> {
    let directory = root.job(spec.run_id, spec.attempt_no);
    match fs::symlink_metadata(&directory) {
        Ok(_) => {
            let actual = exact_spec(&directory)?;
            if serde_json::to_vec(&actual)? != serde_json::to_vec(spec)? {
                return Err(Failure::Invalid("materialization_identity"));
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    Ok(())
}

async fn cleanup_one(root: Arc<RuntimeRoot>, journal: &Journal, id: &str) -> Result<()> {
    let row = journal.get(id).await?;
    if row.phase != "TERMINAL" {
        return Err(Failure::NotReady);
    }
    let spec = row.spec()?;
    tokio::task::spawn_blocking(move || {
        verify_copy(&root, &spec)?;
        discard_staging(&root, &spec)?;
        let completed = root.job(spec.run_id, spec.attempt_no);
        if completed.try_exists()? {
            // Move the stopped job's derived copy into its reserved, never-mounted
            // slot before deletion. A crash after removing spec.json is then
            // recoverable by the slot's durable identity and reservation.
            files::publish_directory(&completed, &staging(&root, &spec))?;
            File::open(root.path.join("jobs"))?.sync_all()?;
            discard_staging(&root, &spec)?;
        }
        File::open(root.path.join("jobs"))?.sync_all()?;
        Ok::<(), Failure>(())
    })
    .await
    .map_err(|_| Failure::Integrity)??;
    journal.release_materialization(id).await
}

pub async fn cleanup_terminal(root: Arc<RuntimeRoot>, journal: &Journal) -> Result<usize> {
    let mut cleaned = 0;
    for id in journal.terminal_materializations().await? {
        match cleanup_one(root.clone(), journal, &id).await {
            Ok(()) => cleaned += 1,
            Err(error) => {
                // Do not release quota, hide a failed deletion or undo the native
                // terminal fact. Remaining copies are retried on the next poll.
                tracing::warn!(external_id=%id, code=%error, "native cache cleanup deferred");
            }
        }
    }
    Ok(cleaned)
}

/// Before serving requests after an upgrade/restart, account for existing known
/// copies and discard old staging copies that were never native mount targets.
/// Unknown paths are preserved and prevent silently advertising spare disk.
pub async fn recover(root: Arc<RuntimeRoot>, journal: &Journal) -> Result<()> {
    let directory = root.clone();
    let entries = tokio::task::spawn_blocking(move || {
        let mut entries = Vec::new();
        for category in ["staging", "jobs"] {
            for entry in fs::read_dir(directory.path.join(category))? {
                if entries.len() >= 16384 {
                    return Err(Failure::Invalid("materialization_inventory_limit"));
                }
                entries.push((category == "staging", entry?.path()));
            }
        }
        Ok::<_, Failure>(entries)
    })
    .await
    .map_err(|_| Failure::Integrity)??;
    for (is_staging, path) in entries {
        let selected = path.clone();
        let observed = tokio::task::spawn_blocking(move || exact_spec(&selected))
            .await
            .map_err(|_| Failure::Integrity)?;
        let spec = match observed {
            Ok(spec) => spec,
            Err(Failure::Io(error))
                if is_staging && error.kind() == std::io::ErrorKind::NotFound =>
            {
                // A new deterministic staging slot can be interrupted before its
                // spec file is written. Only its exact pre-existing reservation
                // makes that partial directory an owned copy rather than user data.
                let name = path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .ok_or(Failure::Invalid("materialization_identity"))?;
                let (run, attempt) = name
                    .rsplit_once('-')
                    .ok_or(Failure::Invalid("materialization_identity"))?;
                let run: Id = run
                    .to_owned()
                    .try_into()
                    .map_err(|_| Failure::Invalid("materialization_identity"))?;
                let attempt: u32 = attempt
                    .parse()
                    .map_err(|_| Failure::Invalid("materialization_identity"))?;
                let id = domain::runtime_jobs::external_id(run, attempt)
                    .map_err(|_| Failure::Invalid("materialization_identity"))?;
                if journal.materialization_bytes(&id).await?.is_none() {
                    return Err(Failure::Invalid("unowned_materialization"));
                }
                let spec = journal.get(&id).await?.spec()?;
                if staging(&root, &spec) != path {
                    return Err(Failure::Invalid("materialization_identity"));
                }
                spec
            }
            Err(error) => return Err(error),
        };
        let row = journal.get(&spec.external_job_id).await?;
        if row.spec_json.as_deref() != Some(serde_json::to_string(&spec)?.as_str()) {
            return Err(Failure::Invalid("materialization_identity"));
        }
        if is_staging {
            let canonical = staging(&root, &spec);
            let name = path
                .file_name()
                .and_then(|name| name.to_str())
                .ok_or(Failure::Invalid("materialization_identity"))?;
            if path != canonical && Id::try_from(name.to_owned()).is_err() {
                return Err(Failure::Invalid("materialization_identity"));
            }
            let parent = root.path.join("staging");
            tokio::task::spawn_blocking(move || {
                let mut remaining = 4096;
                remove_tree(&path, &mut remaining, 0)?;
                File::open(parent)?.sync_all()?;
                Ok::<(), Failure>(())
            })
            .await
            .map_err(|_| Failure::Integrity)??;
        } else {
            if root.job(spec.run_id, spec.attempt_no) != path {
                return Err(Failure::Invalid("materialization_identity"));
            }
            journal.recover_materialization(&spec).await?;
        }
    }
    cleanup_terminal(root, journal).await?;
    Ok(())
}
