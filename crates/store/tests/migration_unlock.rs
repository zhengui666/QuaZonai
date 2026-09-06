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

#[sqlx::test(migrations = false)]
async fn review_migration_has_no_request_timeout_and_does_not_change_pool_defaults(pool: PgPool) {
    sqlx::raw_sql("CREATE FUNCTION public.delay_migration_ddl() RETURNS event_trigger LANGUAGE plpgsql AS $$ BEGIN PERFORM pg_sleep(0.025); END $$; CREATE EVENT TRIGGER delay_migration_ddl ON ddl_command_start WHEN TAG IN ('CREATE TABLE') EXECUTE FUNCTION public.delay_migration_ddl();")
        .execute(&pool).await.unwrap();
    let short = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(|c, _| {
            Box::pin(async move {
                sqlx::query("SET statement_timeout='10ms'")
                    .execute(c)
                    .await?;
                Ok(())
            })
        })
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    Store::from_pool(short.clone())
        .migrate()
        .await
        .expect("deployment migration must not inherit request timeout");
    let default: String = sqlx::query_scalar("SHOW statement_timeout")
        .fetch_one(&short)
        .await
        .unwrap();
    assert_eq!(
        default, "10ms",
        "the dedicated migration connection must not change ordinary pool defaults"
    );
}
