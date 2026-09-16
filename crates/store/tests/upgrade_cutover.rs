//! Exercise the supported upgrade entrypoint with actual pre-upgrade writers.
mod support;
use contracts::Id;
use sqlx::{postgres::PgPoolOptions, PgConnection, PgPool};
use store::{Store, StoreError};
use support::*;

fn sqlstate(error: sqlx::Error, code: &str) {
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some(code),
        "{error:?}"
    );
}
async fn upgrade_pool(pool: &PgPool) -> (PgPool, String) {
    let name = format!("upgrade-{}", Id::new());
    let dedicated = PgPoolOptions::new()
        .max_connections(1)
        .connect_with(
            pool.connect_options()
                .as_ref()
                .clone()
                .application_name(&name),
        )
        .await
        .unwrap();
    (dedicated, name)
}
async fn waiting_backend(pool: &PgPool, name: &str) -> i32 {
    for _ in 0..500 {
        let backend: Option<i32> = sqlx::query_scalar(
            "SELECT pid FROM pg_stat_activity WHERE application_name=$1 AND wait_event_type='Lock'",
        )
        .bind(name)
        .fetch_optional(pool)
        .await
        .unwrap();
        if let Some(pid) = backend {
            return pid;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("upgrader did not reach a real native lock wait");
}
async fn no_locks(pool: &PgPool, name: &str) {
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM pg_locks l JOIN pg_stat_activity a ON a.pid=l.pid WHERE a.application_name=$1 AND l.locktype='advisory'")
        .bind(name).fetch_one(pool).await.unwrap();
    assert_eq!(
        count, 0,
        "migration connection must not retain session locks"
    );
}
async fn copy_evaluation(c: &mut PgConnection, base: Id, new: Id) {
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until FROM app.evaluations WHERE id=$2")
        .bind(new.as_uuid()).bind(base.as_uuid()).execute(c).await.unwrap();
}
async fn late_metric(pool: &PgPool, evaluation: Id, artifact: Id) {
    sqlstate(sqlx::query("INSERT INTO app.metric_values(evaluation_id,metric_code,scope,value,status,unit,period_start,period_end,observation_count,frequency,method_id,method_version,source_artifact_id) VALUES($1,'late','all',1,'OK','ratio',now()-interval '1 hour',now(),10,'DAY','fixture','1',$2)")
        .bind(evaluation.as_uuid()).bind(artifact.as_uuid()).execute(pool).await.unwrap_err(),"23000");
}
async fn initialized(pool: &PgPool) -> Id {
    let store = Store::from_pool(pool.clone());
    let cap = store
        .issue_bootstrap_capability("$argon2id$native-adapter-verified-fixture")
        .await
        .unwrap();
    let binding = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ";
    let e = store
        .start_enrollment(cap.id, &cap.verifier, Id::new(), binding)
        .await
        .unwrap();
    store
        .confirm_enrollment(
            e.id,
            binding,
            e.secret_ref,
            e.database_now.timestamp() / 30,
            true,
            Some("fixture"),
        )
        .await
        .unwrap()
        .id
}

#[sqlx::test(migrations = false)]
async fn native_runner_initializes_and_rechecks_without_changing_applied_checksums(pool: PgPool) {
    let (upgrades, name) = upgrade_pool(&pool).await;
    let store = Store::from_pool(upgrades.clone());
    store.migrate().await.unwrap();
    let before: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    store.migrate().await.unwrap();
    let after: Vec<(i64, Vec<u8>)> =
        sqlx::query_as("SELECT version,checksum FROM _sqlx_migrations ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        after.len(),
        sqlx::migrate!("../../migrations").iter().count()
    );
    let epoch: i64 =
        sqlx::query_scalar("SELECT session_epoch::bigint FROM app.operator_auth_state")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        epoch, 1,
        "an empty installation does not revoke a nonexistent user"
    );
    no_locks(&pool, &name).await;
}

#[sqlx::test(migrations = false)]
async fn upgrade_locks_auth_and_evidence_before_running_any_pending_migration(pool: PgPool) {
    migrate_before(&pool, 202609060005).await;
    let f = fixture(&pool, budget()).await;
    let (_, _, base) = portfolio(&pool, &f).await;
    initialized(&pool).await;
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=5")
        .execute(&pool)
        .await
        .unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE app.wake_events IN ROW EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let (upgrades, name) = upgrade_pool(&pool).await;
    let migrating = tokio::spawn(async move { Store::from_pool(upgrades).migrate().await });
    let pid = waiting_backend(&pool, &name).await;
    for table in [
        "app.operator_auth_state",
        "app.evaluations",
        "app.degradation_observations",
    ] {
        let protected:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=$1 AND relation=$2::regclass AND mode='ShareRowExclusiveLock' AND granted)")
            .bind(pid).bind(table).fetch_one(&pool).await.unwrap();
        assert!(
            protected,
            "{table} must be protected before pending SQL runs"
        );
    }
    let published: bool =
        sqlx::query_scalar("SELECT to_regclass('app.evaluation_publications') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!published);
    let mut auth = pool.acquire().await.unwrap();
    let auth_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *auth)
        .await
        .unwrap();
    let stale = tokio::spawn(async move {
        sqlx::query("UPDATE app.operator_auth_state SET session_epoch=2")
            .execute(&mut *auth)
            .await
    });
    wait_for_database_lock(&pool, auth_pid).await;
    let mut writer = pool.acquire().await.unwrap();
    let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *writer)
        .await
        .unwrap();
    let evaluation = Id::new();
    let insert = tokio::spawn(async move { copy_evaluation(&mut writer, base, evaluation).await });
    wait_for_database_lock(&pool, writer_pid).await;
    blocker.commit().await.unwrap();
    migrating.await.unwrap().unwrap();
    sqlstate(stale.await.unwrap().unwrap_err(), "23514");
    insert.await.unwrap();
    late_metric(&pool, evaluation, f.report).await;
    no_locks(&pool, &name).await;
}

#[sqlx::test(migrations = false)]
async fn evaluation_committing_before_the_cutover_is_included_in_the_backfill(pool: PgPool) {
    migrate_before(&pool, 202609060005).await;
    let f = fixture(&pool, budget()).await;
    let (_, _, base) = portfolio(&pool, &f).await;
    let evaluation = Id::new();
    let mut writer = pool.begin().await.unwrap();
    copy_evaluation(&mut writer, base, evaluation).await;
    let (upgrades, name) = upgrade_pool(&pool).await;
    let migrating = tokio::spawn(async move { Store::from_pool(upgrades).migrate().await });
    waiting_backend(&pool, &name).await;
    writer.commit().await.unwrap();
    migrating.await.unwrap().unwrap();
    late_metric(&pool, evaluation, f.report).await;
    let missing:i64=sqlx::query_scalar("SELECT count(*) FROM app.evaluations e LEFT JOIN app.evaluation_publications p ON p.evaluation_id=e.id WHERE p.evaluation_id IS NULL").fetch_one(&pool).await.unwrap();
    assert_eq!(missing, 0);
}

#[sqlx::test(migrations = false)]
async fn upgrade_invalidates_all_historical_browser_epochs_once_not_current_plus_one(pool: PgPool) {
    migrate_before(&pool, 202609060005).await;
    let old_login = initialized(&pool).await;
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=50")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app.browser_logins(auth_epoch,authenticated_at,expires_at) VALUES(50,now(),now()+interval '1 hour')").execute(&pool).await.unwrap();
    // Represents the unsafe pre-guard writer: it really can lower this old schema.
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=1")
        .execute(&pool)
        .await
        .unwrap();
    let store = Store::from_pool(pool.clone());
    store.browser_authority(old_login).await.unwrap();
    store.migrate().await.unwrap();
    let epoch: i64 =
        sqlx::query_scalar("SELECT session_epoch::bigint FROM app.operator_auth_state")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(epoch, 51);
    assert!(matches!(
        store.browser_authority(old_login).await,
        Err(StoreError::AuthenticationRequired)
    ));
    let receipt:serde_json::Value=sqlx::query_scalar("SELECT normalized_nonsecret_request FROM app.command_receipts WHERE operation='AUTH_UPGRADE_INVALIDATE'").fetch_one(&pool).await.unwrap();
    assert_eq!(receipt["previous_epoch"], "1");
    assert_eq!(receipt["historical_max_epoch"], "50");
    assert_eq!(receipt["new_epoch"], "51");
    store.migrate().await.unwrap();
    let epoch_after: i64 =
        sqlx::query_scalar("SELECT session_epoch::bigint FROM app.operator_auth_state")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(epoch_after, 51);
}

#[sqlx::test(migrations = false)]
async fn overflow_rolls_back_the_whole_batch_and_closes_native_migration_locks(pool: PgPool) {
    migrate_before(&pool, 202609060005).await;
    initialized(&pool).await;
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=$1")
        .bind(i64::MAX)
        .execute(&pool)
        .await
        .unwrap();
    let (upgrades, name) = upgrade_pool(&pool).await;
    let error = Store::from_pool(upgrades).migrate().await.unwrap_err();
    match error {
        StoreError::Migration(sqlx::migrate::MigrateError::ExecuteMigration(e, 202609060006)) => {
            sqlstate(e, "22003")
        }
        e => panic!("unexpected failure: {e:?}"),
    }
    let absent: bool =
        sqlx::query_scalar("SELECT to_regclass('app.evaluation_publications') IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(absent, "0005 must also roll back when a later file fails");
    let last: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(last, 202609060004);
    no_locks(&pool, &name).await;
}

#[sqlx::test(migrations = false)]
async fn cancelling_an_upgrade_releases_its_cutover_and_session_locks(pool: PgPool) {
    migrate_before(&pool, 202609060005).await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE app.wake_events IN ROW EXCLUSIVE MODE")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let (upgrades, name) = upgrade_pool(&pool).await;
    let task = tokio::spawn(async move { Store::from_pool(upgrades).migrate().await });
    waiting_backend(&pool, &name).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    blocker.rollback().await.unwrap();
    let mut connection = pool.acquire().await.unwrap();
    sqlx::query("SET lock_timeout='1s'")
        .execute(&mut *connection)
        .await
        .unwrap();
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=session_epoch")
        .execute(&mut *connection)
        .await
        .unwrap();
    no_locks(&pool, &name).await;
    Store::from_pool(pool.clone()).migrate().await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn incompatible_observation_committing_before_validation_aborts_the_entire_upgrade(
    pool: PgPool,
) {
    migrate_before(&pool, 202609060005).await;
    let f = fixture(&pool, budget()).await;
    let (m, c, e) = portfolio(&pool, &f).await;
    let r = release(&pool, &f, m, c, e).await.unwrap();
    let downstream = Id::new();
    let policy = Id::new();
    let observation = Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'fixture','https://example.invalid','fixture','{fixture}','BOTH',true)").bind(downstream.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO app.automation_policies(id,project_id,mode,mandate_id,downstream_id,required_paper_observations,minimum_paper_elapsed_seconds,max_feedback_age_seconds,promotion_metric_requirements,degradation_metric_requirements,authorized_at,valid_until,enabled_for_new_rebalances,max_rebalances_per_day) VALUES($1,$2,'AUTO_HANDOFF',$3,$4,1,1,60,'[]','[]',clock_timestamp(),clock_timestamp()+interval '1 hour',true,1)").bind(policy.as_uuid()).bind(f.project.as_uuid()).bind(m.as_uuid()).bind(downstream.as_uuid()).execute(&pool).await.unwrap();
    let mut writer = pool.begin().await.unwrap();
    // Old schema admits a non-FORWARD evaluation and no corresponding window.
    sqlx::query("INSERT INTO app.degradation_observations(id,project_id,release_id,evaluation_id,policy_id,classification,reason_codes,observed_at) VALUES($1,$2,$3,$4,$5,'DEGRADED','{fixture}',clock_timestamp())").bind(observation.as_uuid()).bind(f.project.as_uuid()).bind(r.as_uuid()).bind(e.as_uuid()).bind(policy.as_uuid()).execute(&mut *writer).await.unwrap();
    let (upgrades, name) = upgrade_pool(&pool).await;
    let migration = tokio::spawn(async move { Store::from_pool(upgrades).migrate().await });
    waiting_backend(&pool, &name).await;
    writer.commit().await.unwrap();
    match migration.await.unwrap().unwrap_err() {
        StoreError::Migration(sqlx::migrate::MigrateError::ExecuteMigration(e, 202609060005)) => {
            sqlstate(e, "23514")
        }
        e => panic!("unexpected failure: {e:?}"),
    }
    let preserved: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.degradation_observations WHERE id=$1)")
            .bind(observation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        preserved,
        "migration must not discard incompatible historical facts"
    );
    let version: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(version, 202609060004);
    let markers: bool =
        sqlx::query_scalar("SELECT to_regclass('app.evaluation_publications') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!markers);
    no_locks(&pool, &name).await;
}
