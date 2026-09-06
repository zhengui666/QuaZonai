//! A normal migration return must follow the server's native unlock acknowledgement.
use contracts::Id;
use sqlx::{postgres::PgPoolOptions, PgPool};
use store::{Store, StoreError};

#[sqlx::test(migrations = false)]
async fn failed_upgrade_acknowledges_unlock_before_returning_to_the_caller(pool: PgPool) {
    let name = format!("unlock-{}", Id::new());
    let migrations = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            pool.connect_options()
                .as_ref()
                .clone()
                .application_name(&name),
        )
        .await
        .unwrap();
    let store = Store::from_pool(migrations);
    let missing = format!("absent_{}", Id::new());
    for _ in 0..16 {
        assert!(matches!(
            store.migrate_with_application_role(Some(&missing)).await,
            Err(StoreError::Invalid("application_role_does_not_exist"))
        ));
        let locks: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid WHERE a.application_name=$1 AND l.locktype='advisory'")
            .bind(&name).fetch_one(&pool).await.unwrap();
        assert_eq!(
            locks, 0,
            "no sleep or teardown polling can stand in for the native acknowledgement"
        );
        let partial: bool = sqlx::query_scalar(
            "SELECT to_regnamespace('app') IS NOT NULL OR to_regclass('_sqlx_migrations') IS NOT NULL",
        ).fetch_one(&pool).await.unwrap();
        assert!(!partial, "failure must not publish a partial installation");
    }
    store.migrate().await.unwrap();
    let locks: i64 = sqlx::query_scalar("SELECT count(*) FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid WHERE a.application_name=$1 AND l.locktype='advisory'")
        .bind(name).fetch_one(&pool).await.unwrap();
    assert_eq!(locks, 0);
}
