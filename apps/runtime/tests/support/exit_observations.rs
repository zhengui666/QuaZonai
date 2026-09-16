//! SQLite durability tests for observed native exits; container IDs here are explicit fixtures.
use super::*;
use runtime::engine::ContainerObservation;

fn exited(started: chrono::DateTime<chrono::Utc>, code: i64, oom: bool) -> ContainerObservation {
    ContainerObservation {
        id: "a".repeat(64),
        role: "JOB".into(),
        running: false,
        created_only: false,
        started_at: Some(started),
        finished_at: Some(now()),
        exit_code: Some(code),
        oom_killed: oom,
    }
}

#[tokio::test]
async fn failure_before_cancel_survives_gateway_reopen_and_native_container_removal() {
    for (exit_code, oom, expected) in [
        (1, false, RuntimeFailureCode::NativeJobFailed),
        (137, true, RuntimeFailureCode::MemoryLimit),
        (124, false, RuntimeFailureCode::DeadlineExceeded),
    ] {
        let (directory, journal, spec, capability) = fixture().await;
        journal.submit(&spec, &capability).await.unwrap();
        let started = start(&journal, &spec).await;
        let observation = exited(started, exit_code, oom);
        let finished = observation.finished_at.unwrap();
        assert_eq!(
            journal
                .record_native_exit(&spec.external_job_id, &observation)
                .await
                .unwrap(),
            Some(expected)
        );
        assert_eq!(
            journal
                .record_native_exit(&spec.external_job_id, &observation)
                .await
                .unwrap(),
            Some(expected)
        );
        // Ensure the actual cancellation receipt has a later timestamp, without
        // editing any immutable record or faking an in-flight process's exit.
        while now() <= finished {
            tokio::task::yield_now().await;
        }
        journal
            .cancel(&spec.external_job_id, &cancellation(&spec))
            .await
            .unwrap();
        journal.close().await;
        let reopened = Journal::open(
            &directory.path().join("journal.sqlite"),
            64 * 1024 * 1024,
            4,
        )
        .await
        .unwrap();
        let (manifest, _) = completed(&spec, started);
        assert!(matches!(
            reopened
                .finish(&spec.external_job_id, manifest.clone(), vec![])
                .await,
            Err(Failure::NotReady)
        ));
        // The separate native Docker suite proves this non-running barrier exists.
        reopened
            .bind_barrier(&spec.external_job_id, &"b".repeat(64))
            .await
            .unwrap();
        let status = reopened
            .finish(&spec.external_job_id, manifest, vec![])
            .await
            .unwrap();
        assert_eq!(status.state, RuntimeJobState::Failed);
        assert_eq!(status.finished_at, Some(finished));
        let result: ResultManifestV1 =
            serde_json::from_slice(&reopened.result(&spec.external_job_id).await.unwrap()).unwrap();
        assert_eq!(result.error.unwrap().code, expected);
        assert!(result.artifacts.is_empty());
        assert_eq!(result.resource_usage.output_bytes, DbCounter::ZERO);
        assert_eq!(result.started_at, Some(started));
        let replay = reopened
            .cancel(&spec.external_job_id, &cancellation(&spec))
            .await
            .unwrap();
        assert_eq!(replay, status);
        reopened.close().await;
    }
}

#[tokio::test]
async fn signal_exit_after_cancel_is_not_relabelled_as_an_earlier_native_failure() {
    let (_directory, journal, spec, capability) = fixture().await;
    journal.submit(&spec, &capability).await.unwrap();
    let started = start(&journal, &spec).await;
    let requested = journal
        .cancel(&spec.external_job_id, &cancellation(&spec))
        .await
        .unwrap();
    assert_eq!(requested.state, RuntimeJobState::CancelRequested);
    let observation = exited(started, 137, false);
    journal
        .record_native_exit(&spec.external_job_id, &observation)
        .await
        .unwrap();
    journal
        .bind_barrier(&spec.external_job_id, &"b".repeat(64))
        .await
        .unwrap();
    let (mut manifest, _) = completed(&spec, started);
    manifest.state = RuntimeResultState::Failed;
    manifest.error = Some(domain::runtime_jobs::error(
        RuntimeFailureCode::NativeJobFailed,
    ));
    manifest.artifacts.clear();
    manifest.resource_usage.output_bytes = DbCounter::ZERO;
    assert_eq!(
        journal
            .finish(&spec.external_job_id, manifest, vec![])
            .await
            .unwrap()
            .state,
        RuntimeJobState::Cancelled
    );
    let result: ResultManifestV1 =
        serde_json::from_slice(&journal.result(&spec.external_job_id).await.unwrap()).unwrap();
    assert_eq!(result.state, RuntimeResultState::Cancelled);
    assert!(result.error.is_none());
    journal.close().await;
}

#[tokio::test]
async fn native_exit_observation_rejects_foreign_running_changed_and_impossible_processes() {
    let (_directory, journal, spec, capability) = fixture().await;
    journal.submit(&spec, &capability).await.unwrap();
    let started = start(&journal, &spec).await;
    let valid = exited(started, 1, false);
    let mut foreign = valid.clone();
    foreign.id = "c".repeat(64);
    assert!(matches!(
        journal
            .record_native_exit(&spec.external_job_id, &foreign)
            .await,
        Err(Failure::Integrity)
    ));
    let mut running = valid.clone();
    running.running = true;
    assert!(matches!(
        journal
            .record_native_exit(&spec.external_job_id, &running)
            .await,
        Err(Failure::Invalid(_))
    ));
    let mut impossible = valid.clone();
    impossible.finished_at = Some(started - chrono::Duration::microseconds(1));
    assert!(matches!(
        journal
            .record_native_exit(&spec.external_job_id, &impossible)
            .await,
        Err(Failure::Integrity)
    ));
    journal
        .record_native_exit(&spec.external_job_id, &valid)
        .await
        .unwrap();
    let mut changed = valid;
    changed.exit_code = Some(124);
    assert!(matches!(
        journal
            .record_native_exit(&spec.external_job_id, &changed)
            .await,
        Err(Failure::Conflict)
    ));
    journal.close().await;
}
