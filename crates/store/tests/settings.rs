//! Native PostgreSQL configuration/receipt regressions. Secret callbacks are
//! controlled failure boundaries; actual AEAD and HTTP use separate native tests.
#[path = "../../../tests/support/research.rs"]
mod support;
use contracts::{control::ListQuery, runs::RunKind, settings::*, Id, SchemaV1};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use store::{authority::Actor, settings::NativeSecretBinding, StoreError};

fn request() -> RuntimeCreate {
    RuntimeCreate {
        schema_version: SchemaV1,
        configuration: RuntimeConfigurationV1 {
            name: "Runtime fixture".into(),
            endpoint: "https://runtime.example".into(),
            tls_policy: TlsPolicy::SystemCa,
            allowed_capabilities: vec![RunKind::AlphaEvaluate],
            enabled: true,
            development_http: false,
        },
        credential_ref: Id::new(),
        ca_certificate_ref: None,
    }
}
async fn allowed(refs: Vec<NativeSecretBinding>) -> Result<(), StoreError> {
    assert!(!refs.is_empty() && refs.len() <= 2);
    Ok(())
}
async fn cli(pool: &PgPool, scopes: &[&str]) -> Actor {
    let principal = Id::new();
    let credential = Id::new();
    let public = Id::new();
    let verifier_ref = Id::new();
    sqlx::query("INSERT INTO app.machine_principals(id,name,kind,enabled,credential_epoch) VALUES($1,'fixture doctor','CLI',true,1)")
        .bind(principal.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by) VALUES($1,$2,$3,$5,1,$4,clock_timestamp(),clock_timestamp()+interval '1 hour','OPERATOR')")
        .bind(credential.as_uuid()).bind(principal.as_uuid()).bind(public.to_string()).bind(scopes)
        .bind(verifier_ref.to_string()).execute(pool).await.unwrap();
    Actor::Machine {
        credential_id: credential,
        verifier_ref,
        operator_grant: None,
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_commands_publish_exact_original_receipt_and_compare_revision(pool: PgPool) {
    let (store, actor) = support::operator(&pool).await;
    let request = request();
    let (a, b) = tokio::join!(
        store.create_runtime(&actor, "create", &request, allowed),
        store.create_runtime(&actor, "create", &request, allowed)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    let id = a.resource.id;
    let mut update = RuntimeUpdate {
        schema_version: SchemaV1,
        expected_revision: a.resource.revision,
        configuration: request.configuration.clone(),
        credential_ref: None,
        ca_certificate_ref: None,
    };
    update.configuration.name = "Updated runtime".into();
    let changed = store
        .update_runtime(&actor, "update", id, &update, allowed)
        .await
        .unwrap();
    assert_ne!(changed.resource.revision, a.resource.revision);
    let original = store
        .create_runtime(&actor, "create", &request, |_| async {
            panic!("original receipt must not repeat native verification")
        })
        .await
        .unwrap();
    assert!(original.replayed);
    assert_eq!(
        serde_json::to_value(original.resource).unwrap(),
        serde_json::to_value(a.resource).unwrap()
    );
    assert!(matches!(
        store
            .update_runtime(&actor, "stale", id, &update, allowed)
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    let read = store.runtime(&actor, id).await.unwrap();
    assert_eq!(read.configuration.name, "Updated runtime");
    let visible = serde_json::to_value(read).unwrap();
    assert!(visible.get("credential_ref").is_none() && visible.get("ca_certificate_ref").is_none());
    assert!(visible["last_capability_snapshot_artifact_id"].is_null());
    assert_eq!(visible["credential_configured"], true);
    let mut different = request.clone();
    different.configuration.enabled = false;
    assert!(matches!(
        store
            .create_runtime(&actor, "create", &different, allowed)
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_reference_failure_never_publishes_partial_configuration(pool: PgPool) {
    let (store, actor) = support::operator(&pool).await;
    let request = request();
    assert!(matches!(
        store
            .create_runtime(&actor, "bad", &request, |_| async {
                Err(StoreError::SecretCleanup)
            })
            .await,
        Err(StoreError::SecretCleanup)
    ));
    let counts: (i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runtime_integrations),(SELECT count(*) FROM app.command_receipts WHERE operation='RUNTIME_CREATE')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0));
    let created = store
        .create_runtime(&actor, "bad", &request, allowed)
        .await
        .unwrap();
    assert!(!created.replayed);
}

#[sqlx::test(migrations = "../../migrations")]
async fn ca_rotation_is_exact_and_system_ca_clears_only_the_binding(pool: PgPool) {
    let (store, actor) = support::operator(&pool).await;
    let mut request = request();
    request.configuration.tls_policy = TlsPolicy::PinnedCa;
    let ca = Id::new();
    request.ca_certificate_ref = Some(ca);
    let created = store
        .create_runtime(&actor, "create", &request, |refs| async move {
            assert_eq!(refs[1].id, ca);
            assert_eq!(refs[1].purpose, IntegrationSecretPurpose::TlsCa);
            Ok(())
        })
        .await
        .unwrap()
        .resource;
    let mut update = RuntimeUpdate {
        schema_version: SchemaV1,
        expected_revision: created.revision,
        configuration: request.configuration.clone(),
        credential_ref: None,
        ca_certificate_ref: None,
    };
    let kept = store
        .update_runtime(&actor, "retain", created.id, &update, |refs| async move {
            assert_eq!(refs[1].id, ca);
            Ok(())
        })
        .await
        .unwrap()
        .resource;
    assert!(kept.ca_configured);
    update.expected_revision = kept.revision;
    update.configuration.tls_policy = TlsPolicy::SystemCa;
    let cleared = store
        .update_runtime(&actor, "clear", created.id, &update, |refs| async move {
            assert_eq!(refs.len(), 1);
            Ok(())
        })
        .await
        .unwrap()
        .resource;
    assert!(!cleared.ca_configured);
    update.expected_revision = cleared.revision;
    update.configuration.tls_policy = TlsPolicy::PinnedCa;
    assert!(store
        .update_runtime(&actor, "missing", created.id, &update, allowed)
        .await
        .is_err());
    assert!(
        !store
            .runtime(&actor, created.id)
            .await
            .unwrap()
            .ca_configured
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn downstream_pagination_and_rotation_preserve_immutable_receipts(pool: PgPool) {
    let (store, actor) = support::operator(&pool).await;
    let request = DownstreamCreate {
        schema_version: SchemaV1,
        configuration: DownstreamConfigurationV1 {
            name: "Recipient".into(),
            endpoint: "https://recipient.example".into(),
            accepted_package_versions: vec![PackageSchemaVersion::V1],
            environments: DownstreamEnvironments::Paper,
            enabled: true,
            development_http: false,
        },
        credential_ref: Id::new(),
    };
    let a = store
        .create_downstream(&actor, "a", &request, allowed)
        .await
        .unwrap()
        .resource;
    let b = store
        .create_downstream(&actor, "b", &request, allowed)
        .await
        .unwrap()
        .resource;
    let first = store
        .downstreams(
            &actor,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.items[0].id, b.id);
    let second = store
        .downstreams(
            &actor,
            &ListQuery {
                cursor: first.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(second.items[0].id, a.id);
    assert!(second.next_cursor.is_none());
    let new_secret = Id::new();
    let mut update = DownstreamUpdate {
        schema_version: SchemaV1,
        expected_revision: a.revision,
        configuration: request.configuration.clone(),
        credential_ref: Some(new_secret),
    };
    update.configuration.enabled = false;
    let result = store
        .update_downstream(&actor, "rotate", a.id, &update, |refs| async move {
            assert_eq!(refs[0].id, new_secret);
            assert_eq!(refs[0].purpose, IntegrationSecretPurpose::Downstream);
            Ok(())
        })
        .await
        .unwrap();
    assert!(!result.resource.configuration.enabled);
    let stored: String =
        sqlx::query_scalar("SELECT credential_ref FROM app.downstream_integrations WHERE id=$1")
            .bind(a.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, new_secret.to_string());
    let old = store
        .create_downstream(&actor, "a", &request, allowed)
        .await
        .unwrap();
    assert!(old.replayed && old.resource.configuration.enabled);
    assert!(matches!(
        store
            .update_downstream(&actor, "stale", a.id, &update, allowed)
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn doctor_reads_redacted_configuration_without_management_or_native_file_access(
    pool: PgPool,
) {
    let (store, operator) = support::operator(&pool).await;
    let created = store
        .create_runtime(&operator, "create", &request(), allowed)
        .await
        .unwrap()
        .resource;
    let doctor = cli(&pool, &["DOCTOR_READ"]).await;
    assert_eq!(
        store.runtime(&doctor, created.id).await.unwrap().id,
        created.id
    );
    assert!(matches!(
        store
            .create_runtime(&doctor, "denied", &request(), allowed)
            .await,
        Err(StoreError::Forbidden)
    ));
    let Actor::Machine { credential_id, .. } = doctor else {
        unreachable!()
    };
    sqlx::query("INSERT INTO app.machine_credential_revocations(credential_id,effective_at,reason) VALUES($1,clock_timestamp(),'test revoke')")
        .bind(credential_id.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store.runtime(&doctor, created.id).await,
        Err(StoreError::InvalidCredentials)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn secret_registration_receipt_contains_only_intent_and_calls_exact_replay_comparison(
    pool: PgPool,
) {
    let (store, actor) = support::operator(&pool).await;
    let intent = IntegrationSecretIntent {
        schema_version: SchemaV1,
        purpose: IntegrationSecretPurpose::Runtime,
        label: "Runtime credential".into(),
    };
    let published = Arc::new(AtomicUsize::new(0));
    let callback = published.clone();
    let initial = store
        .register_integration_secret(
            &actor,
            "one",
            &intent,
            move |_, purpose, replayed| async move {
                assert_eq!(purpose, IntegrationSecretPurpose::Runtime);
                assert!(!replayed);
                callback.fetch_add(1, Ordering::SeqCst);
                Ok(())
            },
        )
        .await
        .unwrap();
    let id = initial.resource.id;
    let retry = store
        .register_integration_secret(
            &actor,
            "one",
            &intent,
            move |seen, _, replayed| async move {
                assert_eq!(seen, id);
                assert!(replayed);
                Ok(())
            },
        )
        .await
        .unwrap();
    assert!(retry.replayed);
    assert_eq!(published.load(Ordering::SeqCst), 1);
    assert!(matches!(
        store
            .register_integration_secret(&actor, "one", &intent, |_, _, _| async {
                Err(StoreError::IdempotencyConflict)
            })
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    let row=sqlx::query("SELECT normalized_nonsecret_request,response_nonsecret_body FROM app.command_receipts WHERE operation='INTEGRATION_SECRET_REGISTER'").fetch_one(&pool).await.unwrap();
    let normalized: Value = row.get("normalized_nonsecret_request");
    assert_eq!(normalized, serde_json::to_value(intent).unwrap());
    let response: Value = row.get("response_nonsecret_body");
    assert!(response["resource"].get("value").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn grant_expiry_after_native_work_rolls_back_configuration(pool: PgPool) {
    let (store, _) = support::operator(&pool).await;
    let mut actor = cli(&pool, &["DOCTOR_READ"]).await;
    let request = request();
    let grant = Id::new();
    let target = Id::new();
    let Actor::Machine {
        credential_id,
        operator_grant,
        ..
    } = &mut actor
    else {
        unreachable!()
    };
    sqlx::query("INSERT INTO app.operator_command_grants(id,credential_id,operation,target_id,auth_epoch,authenticated_at,expires_at,normalized_nonsecret_request) VALUES($1,$2,'RUNTIME_CREATE',$3,1,clock_timestamp(),clock_timestamp()+interval '1 second',$4)")
        .bind(grant.as_uuid()).bind(credential_id.as_uuid()).bind(target.as_uuid()).bind(serde_json::to_value(&request).unwrap()).execute(&pool).await.unwrap();
    *operator_grant = Some(grant);
    let reached = Arc::new(AtomicUsize::new(0));
    let observed = reached.clone();
    let result = store
        .create_runtime(&actor, "expired-native", &request, move |_| async move {
            observed.fetch_add(1, Ordering::SeqCst);
            tokio::time::sleep(std::time::Duration::from_millis(1100)).await;
            Ok(())
        })
        .await;
    assert_eq!(reached.load(Ordering::SeqCst), 1);
    assert!(matches!(result, Err(StoreError::Forbidden)));
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.runtime_integrations WHERE id=$1)")
            .bind(target.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!exists);
    let consumed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app.operator_command_consumptions WHERE grant_id=$1)",
    )
    .bind(grant.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(!consumed);
}

#[sqlx::test(migrations = "../../migrations")]
async fn database_rejects_secret_value_in_an_original_command_receipt(pool: PgPool) {
    let (_store, _) = support::operator(&pool).await;
    let error=sqlx::query("INSERT INTO app.command_receipts(principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status,response_nonsecret_body) VALUES('OPERATOR','INTEGRATION_SECRET_REGISTER','bad',$1,$2,201,$3)")
        .bind(json!({"schema_version":1,"purpose":"RUNTIME","label":"test","value":"SECRET_SENTINEL"}))
        .bind(Id::new().as_uuid()).bind(json!({"schema_version":1,"replayed":false,"resource":{"id":Id::new(),"value":"SECRET_SENTINEL"}}))
        .execute(&pool).await.unwrap_err();
    assert_eq!(
        error.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
}
