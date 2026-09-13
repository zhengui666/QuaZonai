//! Actual PostgreSQL receipt/identity transactions. All observations are PAPER fixtures.
#[path = "../../../tests/support/research.rs"]
mod research;
use contracts::{
    control::*, forward::*, portfolio::AllocationTargetV1, settings::*, DbCounter, Id, SchemaV1,
};
use sqlx::PgPool;
use store::{authority::Actor, Store, StoreError};

async fn setup(pool: &PgPool) -> (Store, Actor, Actor, DownstreamWeightsSubmitV1) {
    let (store, operator) = research::operator(pool).await;
    let project = store
        .create_project(
            &operator,
            "project",
            &ProjectCreate {
                schema_version: SchemaV1,
                name: "Paper downstream weights".into(),
                description: "Controlled observations, not qualification".into(),
                fork_from_project_id: None,
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let downstream = store
        .create_downstream(
            &operator,
            "downstream",
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: "Paper fixture".into(),
                    endpoint: "https://downstream.example".into(),
                    accepted_package_versions: vec![PackageSchemaVersion::V1],
                    environments: DownstreamEnvironments::Paper,
                    enabled: true,
                    development_http: false,
                },
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource
        .id;
    let principal = store
        .create_principal(
            &operator,
            "principal",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "Bound downstream".into(),
                kind: AssignablePrincipalKind::Downstream,
                project_id: Some(project),
                downstream_id: Some(downstream),
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    let verifier = Id::new();
    let store::control::CredentialPreparation::New(prepared) = store
        .prepare_credential_issuance(
            &operator,
            "credential",
            principal,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![MachineScope::ForwardSubmit],
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new fixture credential");
    };
    let credential = prepared
        .publish(Id::new(), verifier)
        .await
        .unwrap()
        .resource
        .id;
    let actor = Actor::Machine {
        credential_id: credential,
        verifier_ref: verifier,
        operator_grant: None,
    };
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let nanos = now.timestamp_nanos_opt().unwrap() as u64;
    let request = DownstreamWeightsSubmitV1 {
        schema_version: SchemaV1,
        project_id: project,
        environment: ForwardEnvironmentV1::Paper,
        external_message_id: "original-paper-message".into(),
        asof_ns: DbCounter::new(nanos - 1_000_000).unwrap(),
        available_ns: DbCounter::new(nanos).unwrap(),
        valid_until_ns: DbCounter::new(nanos + 60_000_000_000).unwrap(),
        base_currency: "USD".into(),
        cash_weight: "0.25".parse().unwrap(),
        weights: vec![AllocationTargetV1 {
            instrument_id: "A.SIM".into(),
            currency: "USD".into(),
            weight: "0.75".parse().unwrap(),
        }],
    };
    (store, operator, actor, request)
}

#[sqlx::test(migrations = "../../migrations")]
async fn downstream_original_message_is_atomic_immutable_and_replays_across_concurrency(
    pool: PgPool,
) {
    let (store, operator, actor, request) = setup(&pool).await;
    assert!(matches!(
        store
            .submit_downstream_weights(&operator, &request, |_| async {
                panic!("operator cannot assert downstream")
            })
            .await,
        Err(StoreError::Forbidden)
    ));
    let (a, b) = tokio::join!(
        store.submit_downstream_weights(&actor, &request, |_| async { Ok(()) }),
        store.submit_downstream_weights(&actor, &request, |_| async { Ok(()) })
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    let replay = store
        .submit_downstream_weights(&actor, &request, |_| async {
            panic!("replay must not publish")
        })
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        serde_json::to_value(&replay.resource).unwrap(),
        serde_json::to_value(&a.resource).unwrap()
    );
    let mut changed = request.clone();
    changed.cash_weight = "0.5".parse().unwrap();
    changed.weights[0].weight = "0.5".parse().unwrap();
    assert!(matches!(
        store
            .submit_downstream_weights(&actor, &changed, |_| async { panic!() })
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    let origin: String = sqlx::query_scalar("SELECT origin FROM app.artifacts WHERE id=$1")
        .bind(a.resource.report_artifact_id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(origin, "SYNTHETIC");
    assert!(
        sqlx::query("UPDATE app.forward_weight_snapshots SET environment='LIVE' WHERE id=$1")
            .bind(a.resource.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM app.forward_weight_snapshots WHERE id=$1")
            .bind(a.resource.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE operation='FORWARD_WEIGHTS'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_observations_or_failed_publication_never_commit_a_source(pool: PgPool) {
    let (store, _, actor, request) = setup(&pool).await;
    for case in 0..5 {
        let mut bad = request.clone();
        match case {
            0 => bad.environment = ForwardEnvironmentV1::Live,
            1 => bad.project_id = Id::new(),
            2 => bad.available_ns = bad.valid_until_ns,
            3 => bad.weights.push(bad.weights[0].clone()),
            _ => bad.cash_weight = "0.1".parse().unwrap(),
        }
        assert!(store
            .submit_downstream_weights(&actor, &bad, |_| async {
                panic!("invalid source cannot publish")
            })
            .await
            .is_err());
    }
    assert!(store
        .submit_downstream_weights(&actor, &request, |_| async { Err(StoreError::Integrity) })
        .await
        .is_err());
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.forward_weight_snapshots),(SELECT count(*) FROM app.artifacts WHERE schema_name='qz.portfolio_current_weights'),(SELECT count(*) FROM app.command_receipts WHERE operation='FORWARD_WEIGHTS')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn cleanup_waits_for_the_producer_and_preserves_committed_objects(pool: PgPool) {
    let (store, _, actor, request) = setup(&pool).await;
    for commit in [true, false] {
        let mut request = request.clone();
        request.external_message_id = format!("cleanup-{commit}");
        let producer = store.clone();
        let actor = actor.clone();
        let project = request.project_id;
        let (published, allocated) = tokio::sync::oneshot::channel();
        let (release, wait) = tokio::sync::oneshot::channel();
        let task = tokio::spawn(async move {
            producer
                .submit_downstream_weights(&actor, &request, |object| async move {
                    published.send(object.id).unwrap();
                    wait.await.unwrap();
                    if commit {
                        Ok(())
                    } else {
                        Err(StoreError::Integrity)
                    }
                })
                .await
        });
        let artifact = allocated.await.unwrap();
        let discarded = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let observed = discarded.clone();
        let cleanup =
            store.discard_unpublished_forward_weights(project, artifact, move |_| async move {
                observed.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            });
        tokio::pin!(cleanup);
        assert!(
            tokio::time::timeout(std::time::Duration::from_millis(100), &mut cleanup)
                .await
                .is_err()
        );
        assert!(!discarded.load(std::sync::atomic::Ordering::SeqCst));
        release.send(()).unwrap();
        assert_eq!(task.await.unwrap().is_ok(), commit);
        assert_eq!(cleanup.await.unwrap(), !commit);
        assert_eq!(discarded.load(std::sync::atomic::Ordering::SeqCst), !commit);
    }
}
