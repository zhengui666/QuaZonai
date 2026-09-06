//! Check actual PostgreSQL login, ownership and ACLs, not a role-name convention.
use contracts::Id;
use sqlx::{postgres::PgPoolOptions, PgPool};
use store::Store;

async fn role(pool: &PgPool) -> String {
    let name = format!("runtime_test_{}", Id::new().to_string().replace('-', ""));
    sqlx::query(&format!("CREATE ROLE {name} LOGIN PASSWORD 'disposable-test-only' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOREPLICATION NOINHERIT"))
        .execute(pool).await.unwrap();
    sqlx::query(&format!("GRANT USAGE ON SCHEMA app TO {name}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(&format!(
        "GRANT SELECT,INSERT,UPDATE ON ALL TABLES IN SCHEMA app TO {name}"
    ))
    .execute(pool)
    .await
    .unwrap();
    name
}

async fn connect(pool: &PgPool, role: &str) -> PgPool {
    PgPool::connect_with(
        pool.connect_options()
            .as_ref()
            .clone()
            .username(role)
            .password("disposable-test-only"),
    )
    .await
    .unwrap()
}

async fn execute(pool: &PgPool, sql: String) {
    sqlx::query(&sql).execute(pool).await.unwrap();
}

async fn remove_role(pool: &PgPool, role: &str) {
    execute(pool, format!("DROP OWNED BY {role}")).await;
    execute(pool, format!("DROP ROLE {role}")).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_checks_every_table_not_just_the_authentication_singleton(pool: PgPool) {
    let name = role(&pool).await;
    let app_pool = connect(&pool, &name).await;
    let store = Store::from_pool(app_pool.clone());
    store.verify_runtime_role().await.unwrap();
    for table in ["run_events", "research_lineages", "model_turn_receipts"] {
        execute(&pool, format!("GRANT TRUNCATE ON app.{table} TO {name}")).await;
        let auth_truncate: bool = sqlx::query_scalar(
            "SELECT has_table_privilege(current_user,'app.operator_auth_state','TRUNCATE')",
        )
        .fetch_one(&app_pool)
        .await
        .unwrap();
        assert!(
            !auth_truncate,
            "the old sampled check would have missed this role"
        );
        assert!(
            store.verify_runtime_role().await.is_err(),
            "TRUNCATE on {table}"
        );
        execute(&pool, format!("REVOKE TRUNCATE ON app.{table} FROM {name}")).await;
        store.verify_runtime_role().await.unwrap();
    }
    execute(&pool, format!("GRANT TRIGGER ON app.run_events TO {name}")).await;
    assert!(store.verify_runtime_role().await.is_err());
    execute(
        &pool,
        format!("REVOKE TRIGGER ON app.run_events FROM {name}"),
    )
    .await;
    store.verify_runtime_role().await.unwrap();
    app_pool.close().await;
    remove_role(&pool, &name).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_rejects_ownership_even_after_schema_create_is_revoked(pool: PgPool) {
    let name = role(&pool).await;
    let app_pool = connect(&pool, &name).await;
    let store = Store::from_pool(app_pool.clone());
    let owner: String = sqlx::query_scalar("SELECT quote_ident(current_user)")
        .fetch_one(&pool)
        .await
        .unwrap();
    for (create, transfer, drop) in [
        (
            "CREATE TABLE app.runtime_role_probe(id integer)",
            format!("ALTER TABLE app.runtime_role_probe OWNER TO {name}"),
            "DROP TABLE app.runtime_role_probe",
        ),
        (
            "CREATE FUNCTION app.runtime_role_probe() RETURNS integer LANGUAGE sql AS 'SELECT 1'",
            format!("ALTER FUNCTION app.runtime_role_probe() OWNER TO {name}"),
            "DROP FUNCTION app.runtime_role_probe()",
        ),
        (
            "CREATE DOMAIN app.runtime_role_probe AS integer",
            format!("ALTER DOMAIN app.runtime_role_probe OWNER TO {name}"),
            "DROP DOMAIN app.runtime_role_probe",
        ),
    ] {
        execute(&pool, create.into()).await;
        execute(&pool, format!("GRANT CREATE ON SCHEMA app TO {name}")).await;
        execute(&pool, transfer).await;
        execute(&pool, format!("REVOKE CREATE ON SCHEMA app FROM {name}")).await;
        assert!(store.verify_runtime_role().await.is_err(), "{create}");
        execute(&pool, drop.into()).await;
        store.verify_runtime_role().await.unwrap();
    }
    execute(&pool, format!("ALTER SCHEMA app OWNER TO {name}")).await;
    execute(&pool, format!("REVOKE CREATE ON SCHEMA app FROM {name}")).await;
    let create: bool =
        sqlx::query_scalar("SELECT has_schema_privilege(current_user,'app','CREATE')")
            .fetch_one(&app_pool)
            .await
            .unwrap();
    assert!(!create);
    assert!(store.verify_runtime_role().await.is_err());
    execute(&pool, format!("ALTER SCHEMA app OWNER TO {owner}")).await;
    store.verify_runtime_role().await.unwrap();
    app_pool.close().await;
    remove_role(&pool, &name).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn inherited_and_settable_roles_cannot_hide_destructive_authority(pool: PgPool) {
    let name = role(&pool).await;
    let delegate = role(&pool).await;
    let app_pool = connect(&pool, &name).await;
    let store = Store::from_pool(app_pool.clone());
    execute(
        &pool,
        format!("GRANT TRUNCATE ON app.run_events TO {delegate}"),
    )
    .await;
    for options in ["INHERIT TRUE, SET FALSE", "INHERIT FALSE, SET TRUE"] {
        execute(&pool, format!("GRANT {delegate} TO {name} WITH {options}")).await;
        assert!(store.verify_runtime_role().await.is_err(), "{options}");
        execute(&pool, format!("REVOKE {delegate} FROM {name}")).await;
        store.verify_runtime_role().await.unwrap();
    }
    execute(
        &pool,
        format!("REVOKE TRUNCATE ON app.run_events FROM {delegate}"),
    )
    .await;
    execute(
        &pool,
        format!("GRANT {delegate} TO {name} WITH INHERIT FALSE, SET TRUE"),
    )
    .await;
    store.verify_runtime_role().await.unwrap();
    execute(&pool, format!("ALTER ROLE {delegate} CREATEDB")).await;
    assert!(store.verify_runtime_role().await.is_err());
    execute(&pool, format!("ALTER ROLE {delegate} NOCREATEDB")).await;
    store.verify_runtime_role().await.unwrap();
    app_pool.close().await;
    remove_role(&pool, &name).await;
    remove_role(&pool, &delegate).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn set_role_does_not_disguise_an_elevated_session_login(pool: PgPool) {
    let name = role(&pool).await;
    let switched_role = name.clone();
    let masked = PgPoolOptions::new()
        .max_connections(1)
        .after_connect(move |connection, _| {
            let name = switched_role.clone();
            Box::pin(async move {
                sqlx::query(&format!("SET ROLE {name}"))
                    .execute(connection)
                    .await?;
                Ok(())
            })
        })
        .connect_with(pool.connect_options().as_ref().clone())
        .await
        .unwrap();
    let current: String = sqlx::query_scalar("SELECT current_user::text")
        .fetch_one(&masked)
        .await
        .unwrap();
    assert_eq!(current, name);
    assert!(Store::from_pool(masked.clone())
        .verify_runtime_role()
        .await
        .is_err());
    masked.close().await;
    remove_role(&pool, &name).await;
}
