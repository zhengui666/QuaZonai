//! Native-crypto verification is tested in integrations and the HTTP suite.
//! These tests exercise the authoritative PostgreSQL transactions independently.
use contracts::Id;
use sqlx::PgPool;
use store::{
    auth::{AuthOperation, LoginAuthority},
    Store, StoreError,
};

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
async fn setup_consumes_local_capability_and_binds_enrollment_to_browser(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    assert!(!store.authentication_snapshot().await.unwrap().initialized);
    assert!(matches!(
        store.bootstrap_challenge(Id::new()).await,
        Err(StoreError::InvalidCredentials)
    ));
    let capability = store
        .issue_bootstrap_capability("$argon2id$test-native-verification-is-outside-this-layer")
        .await
        .unwrap();
    let secret = Id::new();
    assert!(matches!(
        store
            .start_enrollment(capability.id, "wrong", secret, BINDING)
            .await,
        Err(StoreError::InvalidCredentials)
    ));
    let enrollment = store
        .start_enrollment(capability.id, &capability.verifier, secret, BINDING)
        .await
        .unwrap();
    assert!(matches!(
        store
            .start_enrollment(capability.id, &capability.verifier, secret, BINDING)
            .await,
        Err(StoreError::InvalidCredentials)
    ));
    assert!(store
        .enrollment_challenge(enrollment.id, "wrong")
        .await
        .is_err());
    assert!(store
        .confirm_enrollment(
            enrollment.id,
            BINDING,
            Id::new(),
            enrollment.database_now.timestamp() / 30,
            false,
            None
        )
        .await
        .is_err());
    let login = store
        .confirm_enrollment(
            enrollment.id,
            BINDING,
            secret,
            enrollment.database_now.timestamp() / 30,
            false,
            None,
        )
        .await
        .unwrap();
    assert!(store.browser_authority(login.id).await.unwrap().recent());
    assert!(store.authentication_snapshot().await.unwrap().initialized);
    assert!(matches!(
        store.issue_bootstrap_capability("$argon2id$anything").await,
        Err(StoreError::SetupCompleted)
    ));
    assert!(matches!(
        store
            .confirm_enrollment(
                enrollment.id,
                BINDING,
                secret,
                enrollment.database_now.timestamp() / 30,
                false,
                None
            )
            .await,
        Err(StoreError::SetupCompleted)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn parallel_confirmation_initializes_only_one_secret(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let a = store
        .issue_bootstrap_capability("$argon2id$a")
        .await
        .unwrap();
    let b = store
        .issue_bootstrap_capability("$argon2id$b")
        .await
        .unwrap();
    let a = store
        .start_enrollment(a.id, &a.verifier, Id::new(), BINDING)
        .await
        .unwrap();
    let b = store
        .start_enrollment(b.id, &b.verifier, Id::new(), BINDING)
        .await
        .unwrap();
    let (left, right) = tokio::join!(
        store.confirm_enrollment(
            a.id,
            BINDING,
            a.secret_ref,
            a.database_now.timestamp() / 30,
            false,
            None
        ),
        store.confirm_enrollment(
            b.id,
            BINDING,
            b.secret_ref,
            b.database_now.timestamp() / 30,
            false,
            None
        )
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.browser_logins")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn otp_step_is_consumed_once_across_concurrent_login_requests(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let _ = enroll(&store, false).await;
    let snapshot = store.authentication_snapshot().await.unwrap();
    assert!(matches!(
        store
            .login_with_verified_step(
                &snapshot,
                snapshot.database_now.timestamp() / 30,
                false,
                None
            )
            .await,
        Err(StoreError::TotpReplay)
    ));
    let step = snapshot.database_now.timestamp() / 30 + 1;
    let (left, right) = tokio::join!(
        store.login_with_verified_step(&snapshot, step, false, None),
        store.login_with_verified_step(&snapshot, step, false, None)
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
    assert!(store
        .login_with_verified_step(&snapshot, step + 2, false, None)
        .await
        .is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn database_rate_limit_is_atomic_shared_and_does_not_refund_bad_attempts(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let mut jobs = Vec::new();
    for _ in 0..12 {
        let s = store.clone();
        jobs.push(tokio::spawn(async move {
            s.reserve_auth_attempt(AuthOperation::Login).await
        }));
    }
    let mut success = 0;
    for job in jobs {
        match job.await.unwrap() {
            Ok(()) => success += 1,
            Err(StoreError::AuthRateLimited {
                retry_after_seconds,
            }) => assert!((1..=60).contains(&retry_after_seconds)),
            Err(e) => panic!("{e}"),
        }
    }
    assert_eq!(success, 5);
    assert!(store
        .reserve_auth_attempt(AuthOperation::Bootstrap)
        .await
        .is_ok());
    sqlx::query("UPDATE app.auth_rate_windows SET window_started_at=clock_timestamp()-interval '61 seconds' WHERE operation='LOGIN'").execute(&pool).await.unwrap();
    assert!(store
        .reserve_auth_attempt(AuthOperation::Login)
        .await
        .is_ok());
    let count: i32 =
        sqlx::query_scalar("SELECT attempts FROM app.auth_rate_windows WHERE operation='LOGIN'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn logout_and_device_revocation_cannot_be_undone_by_stale_session_data(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let login = enroll(&store, true).await;
    let device = login.device_id.unwrap();
    assert_eq!(
        store
            .trusted_devices(login.id, None)
            .await
            .unwrap()
            .items
            .len(),
        1
    );
    store.revoke_trusted_device(login.id, device).await.unwrap();
    assert!(matches!(
        store.browser_authority(login.id).await,
        Err(StoreError::AuthenticationRequired)
    ));
    assert!(
        sqlx::query("UPDATE app.browser_logins SET revoked_at=NULL WHERE id=$1")
            .bind(login.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    let snapshot = store.authentication_snapshot().await.unwrap();
    let second = store
        .login_with_verified_step(
            &snapshot,
            snapshot.database_now.timestamp() / 30 + 1,
            false,
            None,
        )
        .await
        .unwrap();
    store.logout_browser(second.id).await.unwrap();
    store.logout_browser(second.id).await.unwrap();
    assert!(store.browser_authority(second.id).await.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn epoch_change_revokes_sessions_and_rejects_previously_verified_snapshot(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let login = enroll(&store, false).await;
    let snapshot = store.authentication_snapshot().await.unwrap();
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=session_epoch+1 WHERE singleton")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.browser_authority(login.id).await.is_err());
    assert!(matches!(
        store
            .login_with_verified_step(
                &snapshot,
                snapshot.database_now.timestamp() / 30 + 1,
                false,
                None
            )
            .await,
        Err(StoreError::InvalidCredentials)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn expired_enrollments_and_non_recent_authority_fail_closed(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let login = enroll(&store, true).await;
    let old_id = Id::new();
    sqlx::query("INSERT INTO app.browser_logins(id,created_at,auth_epoch,authenticated_at,expires_at) VALUES($1,clock_timestamp()-interval '10 minutes',1,clock_timestamp()-interval '10 minutes',clock_timestamp()+interval '1 hour')")
        .bind(old_id.as_uuid()).execute(&pool).await.unwrap();
    assert!(!store.browser_authority(old_id).await.unwrap().recent());
    assert!(matches!(
        store
            .revoke_trusted_device(old_id, login.device_id.unwrap())
            .await,
        Err(StoreError::RecentAuthenticationRequired)
    ));
    let snapshot = store.authentication_snapshot().await.unwrap();
    let authority = store
        .reauthenticate(
            old_id,
            &snapshot,
            snapshot.database_now.timestamp() / 30 + 1,
        )
        .await
        .unwrap();
    assert!(authority.recent());
    store
        .revoke_trusted_device(old_id, login.device_id.unwrap())
        .await
        .unwrap();
    let expired = Id::new();
    sqlx::query("INSERT INTO app.browser_logins(id,created_at,auth_epoch,authenticated_at,expires_at) VALUES($1,clock_timestamp()-interval '2 hours',1,clock_timestamp()-interval '2 hours',clock_timestamp()-interval '1 hour')")
        .bind(expired.as_uuid()).execute(&pool).await.unwrap();
    assert!(store.browser_authority(expired).await.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn device_pagination_does_not_hide_an_old_active_device(pool: PgPool) {
    let store = Store::from_pool(pool.clone());
    let login = enroll(&store, true).await;
    for n in 0..100 {
        sqlx::query("INSERT INTO app.trusted_devices(token_verifier_ref,label,expires_at,auth_epoch) VALUES($1,$2,clock_timestamp()+interval '1 day',1)")
            .bind(format!("native-test-{n}")).bind(format!("device {n}")).execute(&pool).await.unwrap();
    }
    let first = store.trusted_devices(login.id, None).await.unwrap();
    assert_eq!(first.items.len(), 100);
    assert!(first.next_cursor.is_some());
    let second = store
        .trusted_devices(login.id, first.next_cursor)
        .await
        .unwrap();
    assert_eq!(second.items.len(), 1);
    assert!(second.next_cursor.is_none());
    assert_eq!(second.items[0].id, login.device_id.unwrap());
    assert!(!first.items.iter().any(|item| item.id == second.items[0].id));
}
