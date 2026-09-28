//! Real PostgreSQL local session creation, fixed expiry and one-way revocation.
use chrono::{Duration, Utc};
use contracts::{control::ProjectCreate, Id, SchemaV1};
use sqlx::PgPool;
use store::{authority::Actor, Store, StoreError};

#[sqlx::test(migrations = "../../migrations")]
async fn setup_has_one_winner_and_password_change_fences_inflight_admission(pool: PgPool) {
    let store = Store::from_pool(pool);
    let (a, b) = tokio::join!(
        store.setup_password("first-verifier", false),
        store.setup_password("second-verifier", false)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let login = a.or(b).unwrap();
    let snapshot = store.authentication_snapshot().await.unwrap();
    store
        .change_password(login.id, &snapshot, "changed-verifier")
        .await
        .unwrap();
    assert!(matches!(
        store.password_login(&snapshot, true).await,
        Err(StoreError::InvalidCredentials)
    ));
    assert!(matches!(
        store
            .register_cli_device(&snapshot, "stale login", "device-verifier")
            .await,
        Err(StoreError::InvalidCredentials)
    ));
    assert!(matches!(
        store.browser_authority(login.id).await,
        Err(StoreError::AuthenticationRequired)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoked_device_actor_is_rechecked_inside_business_transactions_and_restore(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let login = store
        .setup_password("password-verifier", false)
        .await
        .unwrap();
    let snapshot = store.authentication_snapshot().await.unwrap();
    let device = store
        .register_cli_device(&snapshot, "test device", "device-verifier")
        .await
        .unwrap();
    let actor = Actor::OwnerDevice {
        device_id: device.id,
        verifier: "device-verifier".into(),
    };
    store.cli_device_session(&actor).await.unwrap();
    store.revoke_cli_device(login.id, device.id).await.unwrap();
    let request = ProjectCreate {
        schema_version: SchemaV1,
        name: "denied".into(),
        description: String::new(),
        fork_from_project_id: None,
    };
    assert!(matches!(
        store
            .create_project(&actor, "revoked-device", &request)
            .await,
        Err(StoreError::InvalidCredentials)
    ));
    let second = store
        .register_cli_device(&snapshot, "second device", "second-verifier")
        .await
        .unwrap();
    store.invalidate_restored_access(Id::new()).await.unwrap();
    assert!(matches!(
        store.cli_device_verifier(second.id).await,
        Err(StoreError::InvalidCredentials)
    ));
    assert!(
        sqlx::query("UPDATE app.cli_devices SET revoked_at=NULL WHERE id=$1")
            .bind(device.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_local_sessions_need_no_setup_and_have_fixed_twelve_hour_lifetimes(
    pool: PgPool,
) {
    let store = Store::from_pool(pool.clone());
    let (a, b) = tokio::join!(store.local_browser(), store.local_browser());
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.id, b.id);
    assert_eq!(a.epoch, b.epoch);
    assert_eq!(a.expires_at - a.authenticated_at, Duration::hours(12));
    let observed = store.browser_authority(a.id).await.unwrap();
    assert_eq!(observed.expires_at, a.expires_at);
    assert_eq!(observed.authenticated_at, a.authenticated_at);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.trusted_devices")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoked_session_is_not_resurrected_and_new_session_can_work(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let first = store.local_browser().await.unwrap();
    let actor = Actor::Browser { login_id: first.id };
    store.logout_browser(first.id).await.unwrap();
    let request = ProjectCreate {
        schema_version: SchemaV1,
        name: "local".into(),
        description: String::new(),
        fork_from_project_id: None,
    };
    assert!(matches!(
        store.create_project(&actor, "revoked", &request).await,
        Err(StoreError::AuthenticationRequired)
    ));
    let next = store.local_browser().await.unwrap();
    assert_ne!(next.id, first.id);
    let created = store
        .create_project(&Actor::Browser { login_id: next.id }, "new", &request)
        .await
        .unwrap();
    assert_eq!(created.resource.name, "local");
    assert!(matches!(
        store.browser_authority(first.id).await,
        Err(StoreError::AuthenticationRequired)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_session_and_unknown_identifier_cannot_authorize_business_commands(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let id = Id::new();
    sqlx::query("INSERT INTO app.browser_logins(id,created_at,auth_epoch,authenticated_at,expires_at) SELECT $1,clock_timestamp()-interval '2 hours',session_epoch,clock_timestamp()-interval '2 hours',clock_timestamp()-interval '1 hour' FROM app.operator_auth_state WHERE singleton")
        .bind(id.as_uuid()).execute(&pool).await.unwrap();
    for login in [id, Id::new()] {
        assert!(matches!(
            store.browser_authority(login).await,
            Err(StoreError::AuthenticationRequired)
        ));
    }
    let fresh = store.local_browser().await.unwrap();
    assert!(fresh.expires_at > Utc::now());
}

#[sqlx::test(migrations = "../../migrations")]
async fn epoch_change_denies_old_session_without_deleting_its_history(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let first = store.local_browser().await.unwrap();
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=session_epoch+1 WHERE singleton")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.browser_authority(first.id).await,
        Err(StoreError::AuthenticationRequired)
    ));
    let second = store.local_browser().await.unwrap();
    assert!(second.epoch > first.epoch);
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT revoked_at IS NULL FROM app.browser_logins WHERE id=$1"
    )
    .bind(first.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn legacy_trusted_device_cannot_supply_current_local_authority(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let device = Id::new();
    let login = Id::new();
    sqlx::query("INSERT INTO app.trusted_devices(id,token_verifier_ref,label,expires_at,auth_epoch) SELECT $1,$2,'historical device',clock_timestamp()+interval '1 day',session_epoch FROM app.operator_auth_state")
        .bind(device.as_uuid()).bind(device.to_string()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO app.browser_logins(id,auth_epoch,authenticated_at,expires_at,device_id) SELECT $1,session_epoch,clock_timestamp(),clock_timestamp()+interval '1 hour',$2 FROM app.operator_auth_state")
        .bind(login.as_uuid()).bind(device.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store.browser_authority(login).await,
        Err(StoreError::AuthenticationRequired)
    ));
    assert!(store.local_browser().await.is_ok());
}
