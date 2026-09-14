//! Disposable source-shaped PostgreSQL; this is not the user's real old snapshot.
use contracts::imports::HistoricalSourceInspectionV1;
use sqlx::{ConnectOptions, PgPool};

async fn source(pool: &PgPool) {
    sqlx::raw_sql("CREATE TABLE public.alembic_version(version_num text NOT NULL); INSERT INTO public.alembic_version VALUES('0029_portfolio_candidate_exposure'); CREATE TABLE public.parents(id integer,revision bigint,UNIQUE(id,revision)); INSERT INTO public.parents VALUES(1,9007199254740993); CREATE TABLE public.children(parent_id integer,revision bigint); INSERT INTO public.children VALUES(1,9007199254740993),(1,9007199254740992),(NULL,9007199254740993); ALTER TABLE public.children ADD CONSTRAINT original_pair FOREIGN KEY(parent_id,revision) REFERENCES public.parents(id,revision) NOT VALID; CREATE TABLE public.full_children(parent_id integer,revision bigint); INSERT INTO public.full_children VALUES(NULL,NULL),(1,NULL),(1,9007199254740993); ALTER TABLE public.full_children ADD CONSTRAINT original_full_pair FOREIGN KEY(parent_id,revision) REFERENCES public.parents(id,revision) MATCH FULL NOT VALID;")
        .execute(pool).await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn historical_source_native_sql_checks_composite_values_and_null_semantics(pool: PgPool) {
    source(&pool).await;
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("inspection.json");
    let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_server"))
        .arg("inspect-historical-source")
        .arg("--output")
        .arg(&output)
        .env(
            "MIGRATION_SOURCE_DATABASE_URL",
            pool.connect_options().to_url_lossy().as_str(),
        )
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(
        result.status.success(),
        "source inspection CLI must succeed"
    );
    let report: HistoricalSourceInspectionV1 =
        serde_json::from_slice(&std::fs::read(&output).unwrap()).unwrap();
    assert_eq!(report.foreign_keys.len(), 2);
    assert!(report.foreign_keys.iter().all(|k| k.orphan_rows.get() == 1));
    assert_eq!(
        report
            .tables
            .iter()
            .find(|t| t.table == "children")
            .unwrap()
            .rows
            .get(),
        3
    );
    let unchanged: i64 = sqlx::query_scalar("SELECT count(*) FROM public.children")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(unchanged, 3);
    let before = std::fs::read(&output).unwrap();
    let repeated = tokio::process::Command::new(env!("CARGO_BIN_EXE_server"))
        .arg("inspect-historical-source")
        .arg("--output")
        .arg(&output)
        .env(
            "MIGRATION_SOURCE_DATABASE_URL",
            pool.connect_options().to_url_lossy().as_str(),
        )
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(!repeated.status.success());
    assert_eq!(std::fs::read(output).unwrap(), before);
}

#[sqlx::test(migrations = false)]
async fn historical_source_rejects_unknown_version_and_hidden_rows(pool: PgPool) {
    source(&pool).await;
    let store = store::Store::from_pool(pool.clone());
    sqlx::query("UPDATE public.alembic_version SET version_num='unknown'")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.inspect_historical_source().await,
        Err(store::StoreError::Invalid("historical_source_version"))
    ));
    sqlx::query(
        "UPDATE public.alembic_version SET version_num='0029_portfolio_candidate_exposure'",
    )
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("ALTER TABLE public.parents ENABLE ROW LEVEL SECURITY")
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.inspect_historical_source().await,
        Err(store::StoreError::Invalid(
            "historical_source_row_visibility"
        ))
    ));
}
