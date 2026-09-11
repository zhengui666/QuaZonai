//! Device activity is security metadata, not session renewal.
use chrono::{DateTime, Utc};
use contracts::Id;
use sqlx::PgPool;
use store::{auth::LoginAuthority, Store, StoreError};

const BINDING: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ";
async fn enroll(store: &Store, trust: bool) -> LoginAuthority {
    let capability = store
        .issue_bootstrap_capability("$argon2id$test-only-native-verification-is-outside-this-layer")
        .await
        .unwrap();
    let enrollment = store
        .start_enrollment(capability.id, &capability.verifier, Id::new(), BINDING)
        .await
        .unwrap();
    store
        .confirm_enrollment(
            enrollment.id,
            BINDING,
            enrollment.secret_ref,
            enrollment.database_now.timestamp() / 30,
            trust,
            trust.then_some("test browser"),
        )
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn trusted_device_use_refreshes_activity_without_extending_authority(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let login = enroll(&store, true).await;
    let device = login.device_id.unwrap();
    sqlx::query("UPDATE app.trusted_devices SET last_used_at=clock_timestamp()-interval '1 hour' WHERE id=$1")
        .bind(device.as_uuid()).execute(&pool).await.unwrap();
    let before: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let used = store.browser_authority(login.id).await.unwrap();
    let (last_used, expires): (DateTime<Utc>, DateTime<Utc>) =
        sqlx::query_as("SELECT last_used_at,expires_at FROM app.trusted_devices WHERE id=$1")
            .bind(device.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(last_used >= before && last_used <= used.database_now);
    assert_eq!(expires, login.expires_at);
    assert_eq!(used.expires_at, login.expires_at);
    assert_eq!(used.authenticated_at, login.authenticated_at);
    store.logout_browser(login.id).await.unwrap();
    assert!(matches!(
        store.browser_authority(login.id).await,
        Err(StoreError::AuthenticationRequired)
    ));
    let unchanged: DateTime<Utc> =
        sqlx::query_scalar("SELECT last_used_at FROM app.trusted_devices WHERE id=$1")
            .bind(device.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unchanged, last_used);
}

#[sqlx::test(migrations = "../../migrations")]
async fn device_activity_is_monotonic_and_epoch_revocation_does_not_refresh_it(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let login = enroll(&store, true).await;
    let device = login.device_id.unwrap();
    let future: DateTime<Utc> = sqlx::query_scalar(
        "UPDATE app.trusted_devices SET last_used_at=clock_timestamp()+interval '1 hour' WHERE id=$1 RETURNING last_used_at",
    )
    .bind(device.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    store.browser_authority(login.id).await.unwrap();
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=session_epoch+1 WHERE singleton")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.browser_authority(login.id).await,
        Err(StoreError::AuthenticationRequired)
    ));
    let unchanged: DateTime<Utc> =
        sqlx::query_scalar("SELECT last_used_at FROM app.trusted_devices WHERE id=$1")
            .bind(device.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(unchanged, future);
}

#[sqlx::test(migrations = "../../migrations")]
async fn revoked_device_does_not_gain_new_activity(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let login = enroll(&store, true).await;
    let device = login.device_id.unwrap();
    store.revoke_trusted_device(login.id, device).await.unwrap();
    let previous: DateTime<Utc> =
        sqlx::query_scalar("SELECT last_used_at FROM app.trusted_devices WHERE id=$1")
            .bind(device.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(matches!(
        store.browser_authority(login.id).await,
        Err(StoreError::AuthenticationRequired)
    ));
    let after: DateTime<Utc> =
        sqlx::query_scalar("SELECT last_used_at FROM app.trusted_devices WHERE id=$1")
            .bind(device.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(after, previous);
}
