//! Native PostgreSQL authority/CAS/observation tests. Native outcome and object
//! publication are controlled adapter fixtures; HTTP/TLS tests cover those edges.
#[path = "../../../tests/support/research.rs"]
mod research;
#[path = "../../../tests/support/runtime.rs"]
mod support;
use contracts::{runs::RunKind, runtime::*, settings::*, Id, SchemaV1};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::{
    authority::Actor,
    runtime::{ProbePreparation, ProbeTicket},
    Store, StoreError,
};

async fn fixture(pool: &PgPool) -> (Store, Actor, RuntimeView) {
    let (store, actor) = research::operator(pool).await;
    let request = RuntimeCreate {
        schema_version: SchemaV1,
        configuration: RuntimeConfigurationV1 {
            name: "Probe fixture".into(),
            endpoint: "https://runtime.example".into(),
            tls_policy: TlsPolicy::SystemCa,
            allowed_capabilities: vec![RunKind::DataValidate, RunKind::AlphaEvaluate],
            enabled: true,
            development_http: false,
        },
        credential_ref: Id::new(),
        ca_certificate_ref: None,
    };
    let runtime = store
        .create_runtime(&actor, "configuration", &request, |_| async { Ok(()) })
        .await
        .unwrap()
        .resource;
    (store, actor, runtime)
}
async fn prepare(store: &Store, actor: &Actor, runtime: &RuntimeView, key: &str) -> ProbeTicket {
    match store
        .prepare_runtime_probe(
            actor,
            key,
            runtime.id,
            &RuntimeProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: runtime.revision,
            },
        )
        .await
        .unwrap()
    {
        ProbePreparation::Pending(ticket) => *ticket,
        _ => panic!("fresh native probe ticket expected"),
    }
}
async fn publish_fixture(_: Id, bytes: Vec<u8>) -> Result<(), StoreError> {
    let document: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document["schema_version"], json!(1));
    Ok(())
}
fn available() -> RuntimeProbeOutcomeV1 {
    RuntimeProbeOutcomeV1::Available {
        capabilities: Box::new(support::capabilities(chrono::Utc::now())),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn real_observation_not_saved_configuration_controls_readiness(pool: PgPool) {
    let (store, actor, runtime) = fixture(&pool).await;
    assert_eq!(
        store
            .runtime_readiness(&actor, runtime.id)
            .await
            .unwrap()
            .state,
        RuntimeReadinessState::NotChecked
    );
    let ticket = prepare(&store, &actor, &runtime, "probe").await;
    let outcome = available();
    let result = store
        .complete_runtime_probe(ticket, outcome.clone(), publish_fixture)
        .await
        .unwrap();
    let readiness = store.runtime_readiness(&actor, runtime.id).await.unwrap();
    assert_eq!(readiness.state, RuntimeReadinessState::Available);
    assert_eq!(readiness.integration_revision, runtime.revision);
    assert_eq!(
        readiness.available_job_kinds,
        vec![RunKind::DataValidate, RunKind::AlphaEvaluate]
    );
    let config = store.runtime(&actor, runtime.id).await.unwrap();
    assert_eq!(
        config.revision, runtime.revision,
        "a metadata refresh must not invalidate configured tasks"
    );
    assert_eq!(
        config.last_capability_snapshot_artifact_id,
        Some(result.resource.snapshot_artifact_id)
    );
    let stored: Value =
        sqlx::query_scalar("SELECT outcome FROM app.runtime_probe_observations WHERE id=$1")
            .bind(result.resource.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored["result"], serde_json::to_value(outcome).unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn failures_invalidate_previous_success_and_original_replays_do_not_probe_again(
    pool: PgPool,
) {
    let (store, actor, runtime) = fixture(&pool).await;
    let initial = available();
    let first = store
        .complete_runtime_probe(
            prepare(&store, &actor, &runtime, "first").await,
            initial.clone(),
            publish_fixture,
        )
        .await
        .unwrap();
    let failed = RuntimeProbeOutcomeV1::Unavailable {
        reason: RuntimeProbeFailure::Unavailable,
    };
    store
        .complete_runtime_probe(
            prepare(&store, &actor, &runtime, "second").await,
            failed.clone(),
            publish_fixture,
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .runtime_readiness(&actor, runtime.id)
            .await
            .unwrap()
            .state,
        RuntimeReadinessState::Unavailable
    );
    let replay = store
        .prepare_runtime_probe(
            &actor,
            "first",
            runtime.id,
            &RuntimeProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: runtime.revision,
            },
        )
        .await
        .unwrap();
    let ProbePreparation::Replay(replay) = replay else {
        panic!("original receipt expected")
    };
    assert!(replay.replayed);
    assert_eq!(
        serde_json::to_value(replay.resource).unwrap(),
        serde_json::to_value(first.resource).unwrap()
    );
    assert_eq!(
        store
            .runtime_readiness(&actor, runtime.id)
            .await
            .unwrap()
            .state,
        RuntimeReadinessState::Unavailable
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_io_holds_no_db_lock_and_changed_configuration_cannot_adopt_its_result(
    pool: PgPool,
) {
    let (store, actor, runtime) = fixture(&pool).await;
    let ticket = prepare(&store, &actor, &runtime, "in-flight").await;
    tokio::time::timeout(std::time::Duration::from_secs(2),
        sqlx::query("UPDATE app.runtime_integrations SET endpoint='https://new-runtime.example' WHERE id=$1")
            .bind(runtime.id.as_uuid()).execute(&pool)
    ).await.unwrap().unwrap();
    let outcome = available();
    assert!(matches!(
        store
            .complete_runtime_probe(ticket, outcome.clone(), publish_fixture)
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runtime_probe_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        store
            .runtime_readiness(&actor, runtime.id)
            .await
            .unwrap()
            .state,
        RuntimeReadinessState::NotChecked
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_authority_is_rechecked_after_the_native_roundtrip(pool: PgPool) {
    let (store, actor, runtime) = fixture(&pool).await;
    let ticket = prepare(&store, &actor, &runtime, "revoked").await;
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=session_epoch+1 WHERE singleton")
        .execute(&pool)
        .await
        .unwrap();
    let outcome = available();
    assert!(store
        .complete_runtime_probe(ticket, outcome.clone(), publish_fixture)
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runtime_probe_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_same_command_accepts_one_original_observation(pool: PgPool) {
    let (store, actor, runtime) = fixture(&pool).await;
    let a = prepare(&store, &actor, &runtime, "same").await;
    let b = prepare(&store, &actor, &runtime, "same").await;
    let outcome = available();
    let publications = std::sync::atomic::AtomicUsize::new(0);
    let published = &publications;
    let publish = |id, bytes| async move {
        published.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        publish_fixture(id, bytes).await
    };
    let (a, b) = tokio::join!(
        store.complete_runtime_probe(a, outcome.clone(), publish),
        store.complete_runtime_probe(b, outcome.clone(), publish)
    );
    assert_eq!(publications.load(std::sync::atomic::Ordering::SeqCst), 1);
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.id, b.resource.id);
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runtime_probe_observations),(SELECT count(*) FROM app.command_receipts WHERE operation='RUNTIME_PROBE')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_native_publication_leaves_no_metadata_or_receipt_and_retry_can_succeed(
    pool: PgPool,
) {
    let (store, actor, runtime) = fixture(&pool).await;
    let ticket = prepare(&store, &actor, &runtime, "publication-retry").await;
    let result = store
        .complete_runtime_probe(ticket, available(), |_, _| async {
            Err(StoreError::Invalid("publication_failure_fixture"))
        })
        .await;
    assert!(matches!(
        result,
        Err(StoreError::Invalid("publication_failure_fixture"))
    ));
    let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runtime_probe_observations),(SELECT count(*) FROM app.command_receipts WHERE operation='RUNTIME_PROBE'),(SELECT count(*) FROM app.artifacts WHERE schema_name='qz.runtime_probe')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
    let ticket = prepare(&store, &actor, &runtime, "publication-retry").await;
    let outcome = available();
    let expected = serde_json::to_vec(&json!({"schema_version":1,"result":outcome})).unwrap();
    let size = expected.len() as i64;
    let result = store
        .complete_runtime_probe(ticket, outcome, |_, bytes| async move {
            assert_eq!(bytes, expected);
            Ok(())
        })
        .await
        .unwrap();
    assert!(!result.replayed);
    let stored: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
        .bind(result.resource.snapshot_artifact_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored, size);
}

#[sqlx::test(migrations = "../../migrations")]
async fn old_observation_and_disabled_runtime_never_admit_capabilities(pool: PgPool) {
    let (store, actor, runtime) = fixture(&pool).await;
    let outcome = available();
    let result = store
        .complete_runtime_probe(
            prepare(&store, &actor, &runtime, "initial").await,
            outcome.clone(),
            publish_fixture,
        )
        .await
        .unwrap();
    let mut tx = pool.begin().await.unwrap();
    assert!(store::runtime::require_capabilities(
        &mut tx,
        runtime.id,
        runtime.revision,
        RunKind::DataValidate
    )
    .await
    .is_ok());
    assert!(store::runtime::require_capabilities(
        &mut tx,
        runtime.id,
        runtime.revision,
        RunKind::PortfolioBuild
    )
    .await
    .is_err());
    tx.commit().await.unwrap();
    sqlx::query("UPDATE app.runtime_integrations SET name='A new configuration' WHERE id=$1")
        .bind(runtime.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let config = store.runtime(&actor, runtime.id).await.unwrap();
    assert!(config.last_capability_snapshot_artifact_id.is_none());
    assert_eq!(
        store
            .runtime_readiness(&actor, runtime.id)
            .await
            .unwrap()
            .state,
        RuntimeReadinessState::Stale
    );
    sqlx::query("UPDATE app.runtime_integrations SET enabled=false WHERE id=$1")
        .bind(runtime.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        store
            .runtime_readiness(&actor, runtime.id)
            .await
            .unwrap()
            .state,
        RuntimeReadinessState::Disabled
    );
    let mutation = sqlx::query("UPDATE app.runtime_probe_observations SET valid_until=clock_timestamp()+interval '1 day' WHERE id=$1")
        .bind(result.resource.id.as_uuid()).execute(&pool).await.unwrap_err();
    assert_eq!(
        mutation.as_database_error().unwrap().code().as_deref(),
        Some("23000")
    );
}
