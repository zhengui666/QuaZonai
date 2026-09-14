//! Native PG concurrency/time/CAS. Capability bodies are explicit protocol fixtures.
#[path = "../../../tests/support/research.rs"]
mod research;
use contracts::{
    delivery::*, forward::ForwardEnvironmentV1, runtime::RuntimeProbeFailure, settings::*, Id,
    SchemaV1,
};
use sqlx::PgPool;
use store::{
    authority::Actor,
    downstream::{ProbePreparation, ProbeTicket},
    Store, StoreError,
};

async fn fixture(pool: &PgPool) -> (Store, Actor, DownstreamView) {
    let (store, actor) = research::operator(pool).await;
    let value = store
        .create_downstream(
            &actor,
            "config",
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: "fixture".into(),
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
        .resource;
    (store, actor, value)
}
async fn prepare(store: &Store, actor: &Actor, value: &DownstreamView, key: &str) -> ProbeTicket {
    match store
        .prepare_downstream_probe(
            actor,
            key,
            value.id,
            &DownstreamProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: value.revision,
            },
        )
        .await
        .unwrap()
    {
        ProbePreparation::Pending(ticket) => *ticket,
        _ => panic!("fresh ticket expected"),
    }
}
fn available(accepting: bool) -> DownstreamProbeOutcomeV1 {
    DownstreamProbeOutcomeV1::Available {
        capabilities: DownstreamCapabilitiesV1 {
            schema_version: SchemaV1,
            delivery_mode: DownstreamDeliveryModeV1::TargetOnly,
            accepted_package_versions: vec![PackageSchemaVersion::V1],
            environments: vec![ForwardEnvironmentV1::Paper, ForwardEnvironmentV1::Live],
            market_capability_versions: vec!["fixture/1".into()],
            accepting_targets: accepting,
            checked_at: chrono::Utc::now(),
        },
    }
}
async fn publish(_: Id, bytes: Vec<u8>) -> Result<(), StoreError> {
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&bytes).unwrap()["schema_version"],
        1
    );
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn observation_intersection_replay_and_newer_failures_survive_out_of_order_completion(
    pool: PgPool,
) {
    let (store, actor, value) = fixture(&pool).await;
    assert_eq!(
        store
            .downstream_readiness(&actor, value.id)
            .await
            .unwrap()
            .state,
        DownstreamReadinessState::NotChecked
    );
    let first = store
        .complete_downstream_probe(
            prepare(&store, &actor, &value, "first").await,
            available(true),
            publish,
        )
        .await
        .unwrap();
    let status = store.downstream_readiness(&actor, value.id).await.unwrap();
    assert_eq!(status.state, DownstreamReadinessState::Available);
    assert_eq!(
        status.available_environments,
        vec![ForwardEnvironmentV1::Paper]
    );
    assert_eq!(
        store.downstream(&actor, value.id).await.unwrap().revision,
        value.revision
    );
    let older = prepare(&store, &actor, &value, "older").await;
    let latest = prepare(&store, &actor, &value, "latest").await;
    let failure = store
        .complete_downstream_probe(
            latest,
            DownstreamProbeOutcomeV1::Unavailable {
                reason: RuntimeProbeFailure::Unavailable,
            },
            publish,
        )
        .await
        .unwrap();
    store
        .complete_downstream_probe(older, available(true), publish)
        .await
        .unwrap();
    let status = store.downstream_readiness(&actor, value.id).await.unwrap();
    assert_eq!(status.state, DownstreamReadinessState::Unavailable);
    assert_eq!(status.latest_observation.unwrap().id, failure.resource.id);
    match store
        .prepare_downstream_probe(
            &actor,
            "first",
            value.id,
            &DownstreamProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: value.revision,
            },
        )
        .await
        .unwrap()
    {
        ProbePreparation::Replay(replay) => {
            assert_eq!(replay.resource.id, first.resource.id);
            assert_eq!(replay.resource.valid_until, first.resource.valid_until);
        }
        _ => panic!("original receipt expected"),
    }
    assert!(sqlx::query("UPDATE app.downstream_probe_observations SET valid_until=valid_until+interval '1 second' WHERE id=$1").bind(first.resource.id.as_uuid()).execute(&pool).await.is_err());
    let maintenance = store
        .complete_downstream_probe(
            prepare(&store, &actor, &value, "maintenance").await,
            available(false),
            publish,
        )
        .await
        .unwrap();
    assert!(matches!(
        maintenance.resource.outcome,
        DownstreamProbeOutcomeV1::Available { .. }
    ));
    assert_eq!(
        store
            .downstream_readiness(&actor, value.id)
            .await
            .unwrap()
            .state,
        DownstreamReadinessState::Unavailable
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn same_key_publishes_once_and_configuration_changes_invalidate_inflight_tickets(
    pool: PgPool,
) {
    let (store, actor, value) = fixture(&pool).await;
    let a = prepare(&store, &actor, &value, "race").await;
    let b = prepare(&store, &actor, &value, "race").await;
    let (a, b) = tokio::join!(
        store.complete_downstream_probe(a, available(true), publish),
        store.complete_downstream_probe(b, available(true), publish)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    let ticket = prepare(&store, &actor, &value, "inflight").await;
    let mut config = value.configuration.clone();
    config.environments = DownstreamEnvironments::Live;
    let updated = store
        .update_downstream(
            &actor,
            "update",
            value.id,
            &DownstreamUpdate {
                schema_version: SchemaV1,
                expected_revision: value.revision,
                configuration: config,
                credential_ref: None,
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(
        store
            .downstream_readiness(&actor, value.id)
            .await
            .unwrap()
            .state,
        DownstreamReadinessState::Stale
    );
    assert!(matches!(
        store
            .complete_downstream_probe(ticket, available(true), |_, _| async {
                panic!("stale config cannot publish")
            })
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    let mut only_paper = available(true);
    if let DownstreamProbeOutcomeV1::Available { capabilities } = &mut only_paper {
        capabilities.environments = vec![ForwardEnvironmentV1::Paper];
    }
    store
        .complete_downstream_probe(
            prepare(&store, &actor, &updated, "mismatch").await,
            only_paper,
            publish,
        )
        .await
        .unwrap();
    let status = store.downstream_readiness(&actor, value.id).await.unwrap();
    assert_eq!(status.state, DownstreamReadinessState::Unavailable);
    assert!(status.available_environments.is_empty());
    let mut disabled = updated.configuration.clone();
    disabled.enabled = false;
    store
        .update_downstream(
            &actor,
            "disable",
            value.id,
            &DownstreamUpdate {
                schema_version: SchemaV1,
                expected_revision: updated.revision,
                configuration: disabled,
                credential_ref: None,
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
    assert_eq!(
        store
            .downstream_readiness(&actor, value.id)
            .await
            .unwrap()
            .state,
        DownstreamReadinessState::Disabled
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn expiry_is_real_time_and_original_replay_cannot_refresh_it(pool: PgPool) {
    let (store, actor, value) = fixture(&pool).await;
    let first = store
        .complete_downstream_probe(
            prepare(&store, &actor, &value, "expires").await,
            available(true),
            publish,
        )
        .await
        .unwrap();
    let remaining = (first.resource.valid_until - chrono::Utc::now())
        .to_std()
        .unwrap_or_default();
    tokio::time::sleep(remaining + std::time::Duration::from_millis(100)).await;
    assert_eq!(
        store
            .downstream_readiness(&actor, value.id)
            .await
            .unwrap()
            .state,
        DownstreamReadinessState::Stale
    );
    assert!(matches!(
        store
            .prepare_downstream_probe(
                &actor,
                "expires",
                value.id,
                &DownstreamProbeRequestV1 {
                    schema_version: SchemaV1,
                    expected_revision: value.revision
                }
            )
            .await
            .unwrap(),
        ProbePreparation::Replay(_)
    ));
    assert_eq!(
        store
            .downstream_readiness(&actor, value.id)
            .await
            .unwrap()
            .state,
        DownstreamReadinessState::Stale
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn publication_failure_and_elapsed_callback_leave_no_observation_or_receipt(pool: PgPool) {
    let (store, actor, value) = fixture(&pool).await;
    assert!(store
        .complete_downstream_probe(
            prepare(&store, &actor, &value, "fail").await,
            available(true),
            |_, _| async { Err(StoreError::Integrity) }
        )
        .await
        .is_err());
    assert!(store
        .complete_downstream_probe(
            prepare(&store, &actor, &value, "slow").await,
            available(true),
            |_, _| async {
                tokio::time::sleep(std::time::Duration::from_secs(21)).await;
                Ok(())
            }
        )
        .await
        .is_err());
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.downstream_probe_observations),(SELECT count(*) FROM app.command_receipts WHERE operation='DOWNSTREAM_PROBE'),(SELECT count(*) FROM app.artifacts WHERE schema_name='qz.downstream_probe')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn uncertain_cleanup_waits_for_original_publication_and_preserves_committed_reference(
    pool: PgPool,
) {
    let (store, actor, value) = fixture(&pool).await;
    let ticket = prepare(&store, &actor, &value, "cleanup-race").await;
    let cleaning_store = store.clone();
    let mut cleanup = None;
    let receipt = store
        .complete_downstream_probe(ticket, available(true), |artifact, bytes| {
            assert!(!bytes.is_empty());
            let (started, waiting) = tokio::sync::oneshot::channel();
            cleanup = Some(tokio::spawn(async move {
                started.send(()).unwrap();
                cleaning_store
                    .discard_unpublished_downstream_artifact(value.id, artifact, |_| async {
                        panic!("cleanup must not discard a committing original observation")
                    })
                    .await
            }));
            async move {
                waiting.await.unwrap();
                // The native publication holds the same downstream row until commit.
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                Ok(())
            }
        })
        .await
        .unwrap();
    assert!(!cleanup.unwrap().await.unwrap().unwrap());
    let observation = store
        .downstream_readiness(&actor, value.id)
        .await
        .unwrap()
        .latest_observation
        .unwrap();
    assert_eq!(
        observation.snapshot_artifact_id,
        receipt.resource.snapshot_artifact_id
    );
    let removed = store
        .discard_unpublished_downstream_artifact(value.id, Id::new(), |_| async { Ok(()) })
        .await
        .unwrap();
    assert!(removed);
}

#[sqlx::test(migrations = "../../migrations")]
async fn delayed_observation_insert_cannot_commit_past_the_probe_deadline(pool: PgPool) {
    let (store, actor, value) = fixture(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION app.delay_probe_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(21); RETURN NEW; END $$; CREATE TRIGGER delay_probe_fixture BEFORE INSERT ON app.downstream_probe_observations FOR EACH ROW EXECUTE FUNCTION app.delay_probe_fixture();").execute(&pool).await.unwrap();
    let result = store
        .complete_downstream_probe(
            prepare(&store, &actor, &value, "delayed-insert").await,
            available(true),
            publish,
        )
        .await;
    assert!(matches!(
        result,
        Err(StoreError::Domain(
            domain::DomainError::CapabilityUnavailable("downstream_probe_expired")
        ))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.downstream_probe_observations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.artifacts WHERE schema_name='qz.downstream_probe'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.command_receipts WHERE operation='DOWNSTREAM_PROBE'"
        )
        .fetch_one(&pool)
        .await
        .unwrap(),
        0
    );
}
