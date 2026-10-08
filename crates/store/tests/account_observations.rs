//! Disposable PostgreSQL fixtures only; no exchange or order connection.
#[path = "../../../tests/support/research.rs"]
mod research;
use contracts::{
    account_observation::*, control::*, forward::ForwardEnvironmentV1, settings::*, DbCounter, Id,
    SchemaV1,
};
use sqlx::PgPool;
use store::{authority::Actor, Store, StoreError};

async fn setup(pool: &PgPool) -> (Store, Actor, Actor, AccountObservationSubmitV1) {
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
                    accepted_package_versions: vec![PackageSchemaVersion::V2],
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
    let mut request: AccountObservationSubmitV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/native-account-paper-snapshot.json"
    ))
    .unwrap();
    request.binding.project_id = project;
    request.observed_at_ns = DbCounter::new(nanos - 10).unwrap();
    let snapshot = request.snapshot.as_mut().unwrap();
    snapshot.ts_event = DbCounter::new(nanos - 30).unwrap();
    snapshot.ts_init = DbCounter::new(nanos - 20).unwrap();
    (store, operator, actor, request)
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_account_cursor_append_replay_and_heartbeat_are_atomic(pool: PgPool) {
    let (store, owner, actor, request) = setup(&pool).await;
    assert!(matches!(
        store.submit_account_observation(&owner, &request).await,
        Err(StoreError::Forbidden)
    ));
    let (a, b) = tokio::join!(
        store.submit_account_observation(&actor, &request),
        store.submit_account_observation(&actor, &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    let source = a.resource.source_id;
    let current = store
        .account_current(&owner, request.binding.project_id, source)
        .await
        .unwrap();
    assert_eq!(
        current.source.connection,
        AccountConnectionFreshnessV1::Connected
    );
    assert_eq!(current.valuation, AccountValuationV1::Priced);
    assert!(!current.source.has_gap);
    assert_eq!(
        current.latest_snapshot.as_ref().unwrap().observation,
        request
    );
    let mut heartbeat = request.clone();
    heartbeat.sequence = DbCounter::new(3).unwrap();
    heartbeat.dropped_events = DbCounter::new(1).unwrap();
    heartbeat.snapshot = None;
    heartbeat.connection = AccountConnectionV1::Disconnected;
    let h = store
        .submit_account_observation(&actor, &heartbeat)
        .await
        .unwrap();
    assert!(h.resource.gap_before);
    let current = store
        .account_current(&owner, request.binding.project_id, source)
        .await
        .unwrap();
    assert_eq!(current.source.last_sequence, heartbeat.sequence);
    assert_eq!(
        current.source.connection,
        AccountConnectionFreshnessV1::Disconnected
    );
    assert_eq!(current.latest_snapshot.as_ref().unwrap().id, a.resource.id);
    assert!(current.source.has_gap);
    let replay = store
        .submit_account_observation(&actor, &request)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.received_at, a.resource.received_at);
    assert_eq!(
        store
            .account_current(&owner, request.binding.project_id, source)
            .await
            .unwrap()
            .source
            .last_observation_id,
        h.resource.id
    );
    let page = store
        .account_observations(
            &owner,
            request.binding.project_id,
            source,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].id, h.resource.id);
    assert_eq!(page.next_cursor, Some(h.resource.id));
    let tail = store
        .account_observations(
            &owner,
            request.binding.project_id,
            source,
            &ListQuery {
                cursor: page.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(tail.items[0].id, a.resource.id);
    assert_eq!(tail.next_cursor, None);
    let mut conflict = request.clone();
    conflict.snapshot.as_mut().unwrap().total_equity[0].amount = "999999".parse().unwrap();
    assert!(matches!(
        store.submit_account_observation(&actor, &conflict).await,
        Err(StoreError::NativeIdentityConflict)
    ));
    let mut relabelled = request.clone();
    relabelled.binding.environment = ForwardEnvironmentV1::Live;
    assert!(matches!(
        store.submit_account_observation(&actor, &relabelled).await,
        Err(StoreError::Forbidden)
    ));
    assert!(matches!(
        store
            .account_current(&actor, request.binding.project_id, source)
            .await,
        Err(StoreError::Forbidden)
    ));
    assert!(matches!(
        store.account_current(&owner, Id::new(), source).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store
            .account_observations(
                &owner,
                request.binding.project_id,
                source,
                &ListQuery {
                    cursor: Some(Id::new()),
                    limit: 10
                }
            )
            .await,
        Err(StoreError::EventCursorExpired)
    ));
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.native_account_sources),(SELECT count(*) FROM app.native_account_observations),(SELECT count(*) FROM app.native_account_cursors)").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 2, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn heartbeat_only_and_session_restart_never_invent_valuation_history(pool: PgPool) {
    let (store, owner, actor, mut request) = setup(&pool).await;
    let original = request.snapshot.take().unwrap();
    let receipt = store
        .submit_account_observation(&actor, &request)
        .await
        .unwrap();
    let source = receipt.resource.source_id;
    let current = store
        .account_current(&owner, request.binding.project_id, source)
        .await
        .unwrap();
    assert_eq!(current.valuation, AccountValuationV1::Unavailable);
    assert!(current.latest_snapshot.is_none());
    request.sequence = DbCounter::new(2).unwrap();
    request.snapshot = Some(original.clone());
    let snapshot = store
        .submit_account_observation(&actor, &request)
        .await
        .unwrap();
    let mut older = request.clone();
    older.sequence = DbCounter::new(3).unwrap();
    let older_snapshot = older.snapshot.as_mut().unwrap();
    older_snapshot.event_id = "60a29b32-9e54-4f36-9304-0f0a8b16ce92".into();
    older_snapshot.ts_init = DbCounter::new(older_snapshot.ts_init.get() - 1).unwrap();
    store
        .submit_account_observation(&actor, &older)
        .await
        .unwrap();
    assert_eq!(
        store
            .account_current(&owner, request.binding.project_id, source)
            .await
            .unwrap()
            .source
            .latest_snapshot_id,
        Some(snapshot.resource.id)
    );
    request.binding.native_session_id = "new-native-session".into();
    request.sequence = DbCounter::new(1).unwrap();
    let restarted = store
        .submit_account_observation(&actor, &request)
        .await
        .unwrap();
    assert_ne!(restarted.resource.source_id, source);
    assert!(!restarted.resource.gap_before);
    let mut late = older;
    late.sequence = DbCounter::new(2).unwrap();
    late.snapshot = None;
    assert!(store
        .submit_account_observation(&actor, &late)
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.native_account_sources")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "rejected old sequence must not create or replace a source"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn source_wall_clock_skew_and_rollback_preserve_observations_and_replay(pool: PgPool) {
    let (store, owner, actor, mut request) = setup(&pool).await;
    let original_clock = request.observed_at_ns.get();
    request.observed_at_ns = DbCounter::new(original_clock + 60_000_000_000).unwrap();
    let first = store
        .submit_account_observation(&actor, &request)
        .await
        .unwrap();
    assert_eq!(first.resource.observation, request);
    let source = first.resource.source_id;
    let current = store
        .account_current(&owner, request.binding.project_id, source)
        .await
        .unwrap();
    assert_eq!(
        current.source.connection,
        AccountConnectionFreshnessV1::Stale
    );
    assert_eq!(current.source.last_observed_at_ns, request.observed_at_ns);
    assert_eq!(
        current.latest_snapshot.as_ref().unwrap().id,
        first.resource.id
    );

    let mut heartbeat = request.clone();
    heartbeat.sequence = DbCounter::new(2).unwrap();
    heartbeat.snapshot = None;
    heartbeat.observed_at_ns = DbCounter::new(original_clock - 1_000_000_000).unwrap();
    let second = store
        .submit_account_observation(&actor, &heartbeat)
        .await
        .unwrap();
    assert!(!second.resource.gap_before);
    assert_eq!(second.resource.observation, heartbeat);
    let current = store
        .account_current(&owner, request.binding.project_id, source)
        .await
        .unwrap();
    assert_eq!(current.source.last_sequence, heartbeat.sequence);
    assert_eq!(current.source.last_observed_at_ns, heartbeat.observed_at_ns);
    assert_eq!(current.source.last_observation_id, second.resource.id);
    assert_eq!(
        current.latest_snapshot.as_ref().unwrap().id,
        first.resource.id
    );
    assert_eq!(
        current.latest_snapshot.as_ref().unwrap().received_at,
        first.resource.received_at
    );

    let replay = store
        .submit_account_observation(&actor, &heartbeat)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, second.resource.id);
    assert_eq!(replay.resource.received_at, second.resource.received_at);
    assert_eq!(replay.resource.observation, heartbeat);
    let after = store
        .account_current(&owner, request.binding.project_id, source)
        .await
        .unwrap();
    assert_eq!(
        after.source.last_received_at,
        current.source.last_received_at
    );
    assert_eq!(after.source.last_observed_at_ns, heartbeat.observed_at_ns);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.native_account_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
}

fn client_bound(observation: AccountObservationSubmitV1) -> AccountObservationSubmitV2 {
    AccountObservationSubmitV2 {
        schema_version: NativeClientObservationSchemaV2,
        native_client_id: "QZ-NATIVE-CLIENT".into(),
        observation,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_binding_is_immutable_before_replay_and_preserves_v1_read_values(pool: PgPool) {
    let (store, owner, actor, original) = setup(&pool).await;
    let request = client_bound(original.clone());
    let (left, right) = tokio::join!(
        store.submit_client_account_observation(&actor, &request),
        store.submit_client_account_observation(&actor, &request),
    );
    let (left, right) = (left.unwrap(), right.unwrap());
    assert_eq!(left.resource.id, right.resource.id);
    assert_ne!(left.replayed, right.replayed);
    assert_eq!(left.resource.observation, original);
    assert_eq!(left.native_client_id, request.native_client_id);
    let source = left.resource.source_id;
    let binding = store
        .account_client_binding(&owner, original.binding.project_id, source)
        .await
        .unwrap();
    assert_eq!(binding.source_id, source);
    assert_eq!(binding.native_client_id, request.native_client_id);
    let current = store
        .account_current(&owner, original.binding.project_id, source)
        .await
        .unwrap();
    assert_eq!(
        current.latest_snapshot.as_ref().unwrap().observation,
        original
    );

    let mut changed = request.clone();
    changed.native_client_id = "DIFFERENT-CLIENT".into();
    assert!(matches!(
        store
            .submit_client_account_observation(&actor, &changed)
            .await,
        Err(StoreError::NativeIdentityConflict)
    ));
    assert!(matches!(
        store.submit_account_observation(&actor, &original).await,
        Err(StoreError::NativeIdentityConflict)
    ));
    assert!(matches!(
        store
            .account_client_binding(&actor, original.binding.project_id, source)
            .await,
        Err(StoreError::Forbidden)
    ));
    assert!(matches!(
        store
            .submit_client_account_observation(&owner, &request)
            .await,
        Err(StoreError::Forbidden)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.native_account_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let protocol: i16 = sqlx::query_scalar(
        "SELECT source_schema_version FROM app.native_account_observations WHERE id=$1",
    )
    .bind(left.resource.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(protocol, 2);

    // An old server omits the new protocol column. The DB must not let it append
    // an unbound V1 frame to the existing V2 source even after application rollback.
    let mut heartbeat = original.clone();
    heartbeat.sequence = DbCounter::new(2).unwrap();
    heartbeat.snapshot = None;
    let result = sqlx::query("INSERT INTO app.native_account_observations(id,source_id,sequence,native_event_id,content,gap_before) VALUES($1,$2,2,NULL,$3,false)")
        .bind(Id::new().as_uuid()).bind(source.as_uuid()).bind(serde_json::to_value(&heartbeat).unwrap())
        .execute(&pool).await;
    assert!(result.is_err());
    let result = store
        .submit_client_account_observation(&actor, &client_bound(heartbeat.clone()))
        .await
        .unwrap();
    assert_eq!(result.resource.observation, heartbeat);
    let replay = store
        .submit_client_account_observation(&actor, &request)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, left.resource.id);
    let current = store
        .account_current(&owner, original.binding.project_id, source)
        .await
        .unwrap();
    assert_eq!(current.source.last_sequence.get(), 2);
    assert_eq!(current.latest_snapshot.unwrap().id, left.resource.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn legacy_source_cannot_be_backfilled_with_client_evidence(pool: PgPool) {
    let (store, owner, actor, original) = setup(&pool).await;
    let legacy = store
        .submit_account_observation(&actor, &original)
        .await
        .unwrap();
    let source = legacy.resource.source_id;
    assert!(matches!(
        store
            .submit_client_account_observation(&actor, &client_bound(original.clone()))
            .await,
        Err(StoreError::NativeIdentityConflict)
    ));
    assert!(matches!(
        store
            .account_client_binding(&owner, original.binding.project_id, source)
            .await,
        Err(StoreError::Invalid(
            "native_account_client_binding_unavailable"
        ))
    ));
    let client: Option<String> =
        sqlx::query_scalar("SELECT native_client_id FROM app.native_account_sources WHERE id=$1")
            .bind(source.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(client.is_none());
    let protocol: i16 = sqlx::query_scalar(
        "SELECT source_schema_version FROM app.native_account_observations WHERE id=$1",
    )
    .bind(legacy.resource.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(protocol, 1);
    assert!(sqlx::query(
        "UPDATE app.native_account_sources SET native_client_id='FORGED-CLIENT' WHERE id=$1"
    )
    .bind(source.as_uuid())
    .execute(&pool)
    .await
    .is_err());
    let replay = store
        .submit_account_observation(&actor, &original)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, legacy.resource.id);

    // A genuinely new producer session is a separate source, not an upgrade of
    // the retained old record. This is a controlled persistence fixture only.
    let mut next = original;
    next.binding.native_session_id = "new-controlled-node-session".into();
    let accepted = store
        .submit_client_account_observation(&actor, &client_bound(next))
        .await
        .unwrap();
    assert_ne!(accepted.resource.source_id, source);
    assert_eq!(accepted.native_client_id, "QZ-NATIVE-CLIENT");
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_protocols_cannot_mix_the_same_original_source(pool: PgPool) {
    let (store, owner, actor, original) = setup(&pool).await;
    let request = client_bound(original.clone());
    let (legacy, bound) = tokio::join!(
        store.submit_account_observation(&actor, &original),
        store.submit_client_account_observation(&actor, &request),
    );
    assert_ne!(legacy.is_ok(), bound.is_ok());
    let sources = store
        .account_sources(
            &owner,
            original.binding.project_id,
            &ListQuery {
                cursor: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(sources.items.len(), 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.native_account_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    match (legacy, bound) {
        (Ok(_), Err(StoreError::NativeIdentityConflict)) => {}
        (Err(StoreError::NativeIdentityConflict), Ok(_)) => {}
        pair => panic!("unexpected protocol arbitration: {pair:?}"),
    }
}
