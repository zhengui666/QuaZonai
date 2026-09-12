//! Independent inputs use the existing immutable local object implementation;
//! native Codex still owns file tools and its entire tool loop.
use super::*;
use integrations::artifacts::{ArtifactError, ArtifactStore};
use store::turns::ReviewWork;

fn materialize(
    objects: &ArtifactStore,
    workspace: &Path,
    work: &ReviewWork,
    maximum: u64,
) -> Result<(), WorkerFailure> {
    let context = serde_json::to_vec(&work.context).map_err(|_| WorkerFailure::Contract)?;
    if context.len() > 256 * 1024
        || work
            .code_bytes
            .get()
            .checked_add(work.parameter_bytes.get())
            .and_then(|n| n.checked_add(context.len() as u64))
            .is_none_or(|n| n > maximum)
    {
        return Err(WorkerFailure::Contract);
    }
    let code = objects
        .read(work.code_artifact_id, work.code_bytes)
        .map_err(|_| WorkerFailure::Contract)?;
    let parameters = objects
        .read(work.parameter_artifact_id, work.parameter_bytes)
        .map_err(|_| WorkerFailure::Contract)?;
    let copies = ArtifactStore::open(&workspace.join(format!("review-{}", work.experiment_id)))
        .map_err(|_| WorkerFailure::Contract)?;
    for (id, bytes) in [
        (work.code_artifact_id, code),
        (work.parameter_artifact_id, parameters),
        (work.alpha_version_id, context),
    ] {
        let size =
            contracts::DbCounter::new(bytes.len() as u64).map_err(|_| WorkerFailure::Contract)?;
        match copies.read(id, size) {
            Ok(existing) if existing == bytes => {}
            Err(ArtifactError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
                copies
                    .put(id, &bytes)
                    .map_err(|_| WorkerFailure::Contract)?;
            }
            _ => return Err(WorkerFailure::Contract),
        }
    }
    Ok(())
}

impl Worker {
    pub(super) async fn drive_review(
        &self,
        launcher: &MissionLauncher,
        lease: &RunLease,
        shutdown: &watch::Receiver<bool>,
    ) -> Result<bool, WorkerFailure> {
        let run = lease.run.id;
        let fence = &lease.fence;
        let work = self
            .store
            .mission_review_work(run, fence)
            .await?
            .ok_or(WorkerFailure::Contract)?;
        let experiment = work.experiment_id;
        let workspace = launcher.workspace(run).await?;
        let objects = self.objects.clone();
        let maximum = lease.limits.output_bytes.get();
        tokio::task::spawn_blocking(move || materialize(&objects, &workspace, &work, maximum))
            .await
            .map_err(|_| WorkerFailure::Contract)??;
        let mut connection = launcher
            .open(&self.store, self.vault.clone(), run, fence)
            .await?;
        let result: Result<(), WorkerFailure> = async {
            let reading = self.objects.clone();
            let publishing = self.objects.clone();
            self.store
                .prepare_mission_review_turn(
                    run,
                    fence,
                    experiment,
                    move |id, size| async move {
                        tokio::task::spawn_blocking(move || reading.read(id, size))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    },
                    move |object| async move {
                        tokio::task::spawn_blocking(move || {
                            publishing.put(object.id, &object.bytes)
                        })
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                    },
                )
                .await?;
            connection
                .drive_turn(&self.store, self.objects.clone(), run, fence, shutdown)
                .await?;
            self.capture_mission_summary(&mut connection, lease).await?;
            Ok(())
        }
        .await;
        let closed = connection.client.close().await;
        result?;
        closed.map_err(|reason| WorkerFailure::Codex("CLOSE_REVIEW", reason))?;
        Ok(self.store.complete_mission(run, fence).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn original_review_inputs_are_reusable_but_never_overwritten_or_over_budget() {
        let temp = tempfile::tempdir().unwrap();
        let objects = ArtifactStore::open(&temp.path().join("objects")).unwrap();
        let workspace = temp.path().join("reviewer");
        fs::create_dir(&workspace).unwrap();
        let code = Id::new();
        let parameters = Id::new();
        objects.put(code, b"// original Rust").unwrap();
        objects.put(parameters, b"{\"schema_version\":1}").unwrap();
        let mut work = ReviewWork {
            experiment_id: Id::new(),
            alpha_version_id: Id::new(),
            code_artifact_id: code,
            code_bytes: contracts::DbCounter::new(16).unwrap(),
            parameter_artifact_id: parameters,
            parameter_bytes: contracts::DbCounter::new(20).unwrap(),
            context: serde_json::json!({"schema_version":1,"qualification":"NOT_GRANTED"}),
        };
        assert!(materialize(&objects, &workspace, &work, 10).is_err());
        materialize(&objects, &workspace, &work, 4096).unwrap();
        materialize(&objects, &workspace, &work, 4096).unwrap();
        work.context["qualification"] = serde_json::json!("CHANGED");
        assert!(materialize(&objects, &workspace, &work, 4096).is_err());
        let copies =
            ArtifactStore::open(&workspace.join(format!("review-{}", work.experiment_id))).unwrap();
        assert_eq!(
            copies.read(code, work.code_bytes).unwrap(),
            b"// original Rust"
        );
    }
}
