//! Global invalidation is one-way even when old individual credentials still exist.
mod support;
use contracts::Id;
use sqlx::PgPool;
use store::{Store, StoreError};

fn sqlstate(error: sqlx::Error, expected: &str) {
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some(expected),
        "{error:?}"
    );
}
async fn epoch(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT session_epoch::bigint FROM app.operator_auth_state WHERE singleton")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn old_browser_and_device_authority_cannot_return_after_epoch_invalidation(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let capability = store
        .issue_bootstrap_capability("$argon2id$test-only-crypto-verified-at-adapter")
        .await
        .unwrap();
    let binding = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ";
    let enrollment = store
        .start_enrollment(capability.id, &capability.verifier, Id::new(), binding)
        .await
        .unwrap();
    let login = store
        .confirm_enrollment(
            enrollment.id,
            binding,
            enrollment.secret_ref,
            enrollment.database_now.timestamp() / 30,
            true,
            Some("fixture"),
        )
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
    sqlstate(
        sqlx::query("UPDATE app.operator_auth_state SET session_epoch=1 WHERE singleton")
            .execute(&pool)
            .await
            .unwrap_err(),
        "23514",
    );
    assert_eq!(epoch(&pool).await, 2);
    assert!(matches!(
        store.browser_authority(login.id).await,
        Err(StoreError::AuthenticationRequired)
    ));
    let intact: bool=sqlx::query_scalar("SELECT b.revoked_at IS NULL AND b.auth_epoch=1 AND d.revoked_at IS NULL AND d.auth_epoch=1 FROM app.browser_logins b JOIN app.trusted_devices d ON d.id=b.device_id WHERE b.id=$1")
        .bind(login.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert!(
        intact,
        "old individual credentials remain; global invalidation alone must deny them"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn equal_and_increasing_epochs_work_but_native_overflow_never_wraps(pool: PgPool) {
    for value in [1_i64, 2, 2, 3, i64::MAX] {
        sqlx::query("UPDATE app.operator_auth_state SET session_epoch=$1 WHERE singleton")
            .bind(value)
            .execute(&pool)
            .await
            .unwrap();
        assert_eq!(epoch(&pool).await, value);
    }
    sqlstate(
        sqlx::query(
            "UPDATE app.operator_auth_state SET session_epoch=session_epoch+1 WHERE singleton",
        )
        .execute(&pool)
        .await
        .unwrap_err(),
        "22003",
    );
    sqlstate(
        sqlx::query("UPDATE app.operator_auth_state SET session_epoch=1 WHERE singleton")
            .execute(&pool)
            .await
            .unwrap_err(),
        "23514",
    );
    assert_eq!(epoch(&pool).await, i64::MAX);
}

#[sqlx::test(migrations = "../../migrations")]
async fn stale_epoch_update_is_rejected_after_waiting_for_a_newer_commit(pool: PgPool) {
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=3 WHERE singleton")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let mut connection = pool.acquire().await.unwrap();
    let backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let stale = sqlx::query("UPDATE app.operator_auth_state SET session_epoch=2 WHERE singleton")
        .execute(&mut *connection);
    let release = async {
        support::wait_for_database_lock(&pool, backend).await;
        blocker.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(stale, release);
    sqlstate(result.unwrap_err(), "23514");
    assert_eq!(epoch(&pool).await, 3);
}
