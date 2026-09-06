//! Execute the actual deployment CLI against disposable native PostgreSQL.
//! A pool/advisory lock is not a cross-connection atomic transaction.
#[path = "../../../crates/store/tests/support/mod.rs"]
mod store_fixture;

use contracts::Id;
use serde_json::{json, Value};
use sqlx::{ConnectOptions, PgPool};
use std::process::Output;
use store::Store;
use time::{Duration, OffsetDateTime};
use tower_sessions::{session::Record, SessionStore};
use tower_sessions_sqlx_store::PostgresStore;

async fn deploy(pool: &PgPool, role: Option<&str>) -> Output {
    let mut command = tokio::process::Command::new(env!("CARGO_BIN_EXE_server"));
    command
        .arg("migrate")
        .arg("--database-url")
        .arg(pool.connect_options().to_url_lossy().as_str())
        .kill_on_drop(true);
    if let Some(role) = role {
        command.arg("--application-role").arg(role);
    }
    tokio::time::timeout(std::time::Duration::from_secs(20), command.output())
        .await
        .expect("migration CLI must finish within the lock timeout")
        .unwrap()
}

async fn version_rows(pool: &PgPool) -> Vec<(i64, Vec<u8>)> {
    sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
        .fetch_all(pool)
        .await
        .unwrap()
}

async fn old_initialized(pool: &PgPool) {
    store_fixture::migrate_before(pool, 202609060006).await;
    sqlx::query("UPDATE app.operator_auth_state SET initialized=true,totp_secret_ref='fixture-only-no-secret',last_accepted_totp_step=1,session_epoch=7,setup_completed_at=clock_timestamp()")
        .execute(pool).await.unwrap();
}

async fn epoch(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT session_epoch::bigint FROM app.operator_auth_state")
        .fetch_one(pool)
        .await
        .unwrap()
}

async fn role(pool: &PgPool) -> String {
    let name = format!("deploy_{}", Id::new().to_string().replace('-', ""));
    sqlx::query(&format!("CREATE ROLE {name} LOGIN PASSWORD 'test-only' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOREPLICATION"))
        .execute(pool).await.unwrap();
    name
}

async fn remove_role(pool: &PgPool, name: &str) {
    sqlx::query(&format!("DROP OWNED BY {name}"))
        .execute(pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {name}"))
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = false)]
async fn nonexistent_runtime_role_does_not_commit_a_partial_new_installation(pool: PgPool) {
    let missing = format!("missing_{}", Id::new());
    let result = deploy(&pool, Some(&missing)).await;
    assert!(!result.status.success());
    let objects: (bool, bool, bool) = sqlx::query_as("SELECT to_regnamespace('app') IS NOT NULL,to_regnamespace('tower_sessions') IS NOT NULL,to_regclass('_sqlx_migrations') IS NOT NULL")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(objects, (false, false, false));
}

#[sqlx::test(migrations = false)]
async fn incompatible_native_session_table_rolls_back_domain_upgrade_and_epoch(pool: PgPool) {
    old_initialized(&pool).await;
    let versions = version_rows(&pool).await;
    sqlx::raw_sql("CREATE SCHEMA tower_sessions; CREATE TABLE tower_sessions.session(id text PRIMARY KEY,data text NOT NULL,expiry_date timestamptz NOT NULL); INSERT INTO tower_sessions.session VALUES('keep','user-data',clock_timestamp()+interval '1 day')")
        .execute(&pool).await.unwrap();
    let result = deploy(&pool, None).await;
    assert!(!result.status.success());
    assert_eq!(
        epoch(&pool).await,
        7,
        "failed command cannot log everyone out"
    );
    assert_eq!(versions, version_rows(&pool).await);
    let preserved: String =
        sqlx::query_scalar("SELECT data FROM tower_sessions.session WHERE id='keep'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(preserved, "user-data");
    let newer: bool =
        sqlx::query_scalar("SELECT to_regclass('app.machine_auth_windows') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!newer);
}

#[sqlx::test(migrations = false)]
async fn late_grant_failure_rolls_back_native_schema_epoch_and_earlier_grants(pool: PgPool) {
    old_initialized(&pool).await;
    let versions = version_rows(&pool).await;
    let name = role(&pool).await;
    // Inject failure at the LAST real GRANT, not before domain migration runs.
    sqlx::raw_sql("CREATE FUNCTION public.reject_final_grant() RETURNS event_trigger LANGUAGE plpgsql AS $$ BEGIN IF current_query() LIKE 'GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA pgmq%' THEN IF NOT EXISTS(SELECT 1 FROM app.operator_auth_state WHERE session_epoch>7) THEN RAISE EXCEPTION 'fixture did not reach the pending epoch migration'; END IF; RAISE EXCEPTION 'fixture refuses final sequence grant'; END IF; END $$; CREATE EVENT TRIGGER reject_final_grant ON ddl_command_end WHEN TAG IN ('GRANT') EXECUTE FUNCTION public.reject_final_grant()")
        .execute(&pool).await.unwrap();
    let result = deploy(&pool, Some(&name)).await;
    assert!(!result.status.success());
    assert_eq!(epoch(&pool).await, 7);
    assert_eq!(versions, version_rows(&pool).await);
    let native_schema: bool =
        sqlx::query_scalar("SELECT to_regnamespace('tower_sessions') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !native_schema,
        "native session DDL is in the same transaction"
    );
    let authorized: bool = sqlx::query_scalar("SELECT has_schema_privilege($1,'app','USAGE')")
        .bind(&name)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!authorized, "the earlier schema GRANT must also roll back");
    sqlx::raw_sql(
        "DROP EVENT TRIGGER reject_final_grant; DROP FUNCTION public.reject_final_grant()",
    )
    .execute(&pool)
    .await
    .unwrap();
    assert!(deploy(&pool, Some(&name)).await.status.success());
    assert_eq!(epoch(&pool).await, 8);
    remove_role(&pool, &name).await;
}

async fn schema_contract(pool: &PgPool, table: &str) -> (Value, Vec<String>) {
    let columns = sqlx::query_scalar("SELECT jsonb_agg(jsonb_build_array(attname,format_type(atttypid,atttypmod),attnotnull,attidentity,attgenerated) ORDER BY attnum) FROM pg_attribute WHERE attrelid=$1::regclass AND attnum>0 AND NOT attisdropped")
        .bind(table).fetch_one(pool).await.unwrap();
    let constraints = sqlx::query_scalar("SELECT pg_get_constraintdef(oid) FROM pg_constraint WHERE conrelid=$1::regclass ORDER BY contype,conname")
        .bind(table).fetch_all(pool).await.unwrap();
    (columns, constraints)
}

#[sqlx::test(migrations = false)]
async fn committed_schema_matches_upstream_and_supports_native_crud_as_runtime(pool: PgPool) {
    let name = role(&pool).await;
    assert!(deploy(&pool, Some(&name)).await.status.success());
    PostgresStore::new(pool.clone())
        .with_schema_name("upstream_reference")
        .unwrap()
        .migrate()
        .await
        .unwrap();
    assert_eq!(
        schema_contract(&pool, "tower_sessions.session").await,
        schema_contract(&pool, "upstream_reference.session").await
    );
    let runtime = PgPool::connect_with(
        pool.connect_options()
            .as_ref()
            .clone()
            .username(&name)
            .password("test-only"),
    )
    .await
    .unwrap();
    Store::from_pool(runtime.clone())
        .verify_runtime_role()
        .await
        .unwrap();
    let native = PostgresStore::new(runtime.clone());
    let mut record = Record {
        id: Default::default(),
        data: [("test".into(), json!(1))].into(),
        expiry_date: OffsetDateTime::now_utc() + Duration::hours(1),
    };
    native.create(&mut record).await.unwrap();
    assert_eq!(
        native.load(&record.id).await.unwrap().unwrap().data["test"],
        1
    );
    record.data.insert("test".into(), json!(2));
    native.save(&record).await.unwrap();
    assert_eq!(
        native.load(&record.id).await.unwrap().unwrap().data["test"],
        2
    );
    native.delete(&record.id).await.unwrap();
    assert!(native.load(&record.id).await.unwrap().is_none());
    let snapshots = version_rows(&pool).await;
    assert!(deploy(&pool, Some(&name)).await.status.success());
    assert_eq!(snapshots, version_rows(&pool).await);
    runtime.close().await;
    remove_role(&pool, &name).await;
}

#[sqlx::test(migrations = false)]
async fn concurrent_complete_deployments_use_one_native_migration_lock(pool: PgPool) {
    let name = role(&pool).await;
    let (a, b) = tokio::join!(deploy(&pool, Some(&name)), deploy(&pool, Some(&name)));
    assert!(a.status.success(), "{}", String::from_utf8_lossy(&a.stderr));
    assert!(b.status.success(), "{}", String::from_utf8_lossy(&b.stderr));
    assert_eq!(
        version_rows(&pool).await.len(),
        sqlx::migrate!("../../migrations").iter().count()
    );
    let locks:i64=sqlx::query_scalar("SELECT count(*) FROM pg_locks WHERE locktype='advisory' AND database=(SELECT oid FROM pg_database WHERE datname=current_database())")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(locks, 0);
    remove_role(&pool, &name).await;
}

#[sqlx::test(migrations = false)]
async fn behavior_changing_session_definitions_roll_back_the_complete_migration(pool: PgPool) {
    old_initialized(&pool).await;
    PostgresStore::new(pool.clone()).migrate().await.unwrap();
    let mut record = Record {
        id: Default::default(),
        data: [("preserve".into(), json!({"user": "original"}))].into(),
        expiry_date: OffsetDateTime::now_utc() + Duration::days(1),
    };
    PostgresStore::new(pool.clone())
        .create(&mut record)
        .await
        .unwrap();
    let versions = version_rows(&pool).await;
    for (change, undo) in [
        ("ALTER TABLE tower_sessions.session ADD CONSTRAINT blocks_insert CHECK (length(id)=0) NOT VALID", "ALTER TABLE tower_sessions.session DROP CONSTRAINT blocks_insert"),
        ("CREATE FUNCTION tower_sessions.ignore_write() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RETURN NULL; END $$; CREATE TRIGGER ignore_write BEFORE INSERT ON tower_sessions.session FOR EACH ROW EXECUTE FUNCTION tower_sessions.ignore_write()", "DROP TRIGGER ignore_write ON tower_sessions.session; DROP FUNCTION tower_sessions.ignore_write()"),
        ("ALTER TABLE tower_sessions.session ENABLE ROW LEVEL SECURITY", "ALTER TABLE tower_sessions.session DISABLE ROW LEVEL SECURITY"),
        ("ALTER TABLE tower_sessions.session FORCE ROW LEVEL SECURITY", "ALTER TABLE tower_sessions.session NO FORCE ROW LEVEL SECURITY"),
        ("CREATE POLICY inactive_policy ON tower_sessions.session USING (false)", "DROP POLICY inactive_policy ON tower_sessions.session"),
        ("ALTER TABLE tower_sessions.session SET UNLOGGED", "ALTER TABLE tower_sessions.session SET LOGGED"),
        ("CREATE RULE ignore_insert AS ON INSERT TO tower_sessions.session DO INSTEAD NOTHING", "DROP RULE ignore_insert ON tower_sessions.session"),
        ("ALTER TABLE tower_sessions.session ADD CONSTRAINT unexpected_unique UNIQUE(data)", "ALTER TABLE tower_sessions.session DROP CONSTRAINT unexpected_unique"),
        ("CREATE UNIQUE INDEX unexpected_unique ON tower_sessions.session(data)", "DROP INDEX tower_sessions.unexpected_unique"),
        ("ALTER TABLE tower_sessions.session ALTER COLUMN expiry_date TYPE timestamptz(0)", "ALTER TABLE tower_sessions.session ALTER COLUMN expiry_date TYPE timestamptz"),
        ("ALTER TABLE tower_sessions.session ALTER COLUMN data SET DEFAULT ''::bytea", "ALTER TABLE tower_sessions.session ALTER COLUMN data DROP DEFAULT"),
        ("CREATE TABLE tower_sessions.parent(id text NOT NULL,data bytea NOT NULL,expiry_date timestamptz NOT NULL); ALTER TABLE tower_sessions.session INHERIT tower_sessions.parent", "ALTER TABLE tower_sessions.session NO INHERIT tower_sessions.parent; DROP TABLE tower_sessions.parent"),
        ("CREATE TABLE tower_sessions.child() INHERITS (tower_sessions.session)", "DROP TABLE tower_sessions.child"),
        ("ALTER TABLE tower_sessions.session ALTER COLUMN id TYPE text COLLATE \"C\"", "ALTER TABLE tower_sessions.session ALTER COLUMN id TYPE text COLLATE \"default\""),
    ] {
        sqlx::raw_sql(change).execute(&pool).await.unwrap();
        let snapshot: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM tower_sessions.session s").fetch_one(&pool).await.unwrap();
        let result = deploy(&pool, None).await;
        assert!(!result.status.success(), "migration accepted {change}");
        assert!(String::from_utf8_lossy(&result.stderr).contains("native_session_schema_incompatible"), "{change}: {}", String::from_utf8_lossy(&result.stderr));
        assert_eq!(epoch(&pool).await, 7, "{change}");
        assert_eq!(versions, version_rows(&pool).await, "{change}");
        let after: Value = sqlx::query_scalar("SELECT to_jsonb(s) FROM tower_sessions.session s").fetch_one(&pool).await.unwrap();
        assert_eq!(snapshot, after, "failed migration must not rewrite user sessions");
        sqlx::raw_sql(undo).execute(&pool).await.unwrap();
    }
    // Safe native lookup indexes are not data-changing constraints.
    sqlx::query("CREATE INDEX session_expiry_lookup ON tower_sessions.session(expiry_date)")
        .execute(&pool)
        .await
        .unwrap();
    assert!(deploy(&pool, None).await.status.success());
    assert_eq!(epoch(&pool).await, 8);
    let native = PostgresStore::new(pool.clone());
    assert_eq!(
        native.load(&record.id).await.unwrap().unwrap().data,
        record.data
    );
    record.data.insert("after".into(), json!(true));
    native.save(&record).await.unwrap();
    assert_eq!(
        native.load(&record.id).await.unwrap().unwrap().data,
        record.data
    );
    native.delete(&record.id).await.unwrap();
    assert!(native.load(&record.id).await.unwrap().is_none());
}
