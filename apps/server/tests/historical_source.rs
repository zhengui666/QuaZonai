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
    sqlx::raw_sql("CREATE TABLE precision_sample(id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY, amount numeric(30,12), recorded_at timestamp(3) with time zone, derived numeric GENERATED ALWAYS AS (amount * 2) STORED, removed text DEFAULT 'default-marker-never-export'); ALTER TABLE precision_sample DROP COLUMN removed")
        .execute(&pool).await.unwrap();
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
    let parent = report.tables.iter().find(|t| t.table == "parents").unwrap();
    assert_eq!(parent.columns[1].name, "revision");
    assert_eq!(parent.columns[1].postgres_type, "bigint");
    assert!(parent.columns[1].nullable);
    let precision = report
        .tables
        .iter()
        .find(|t| t.table == "precision_sample")
        .unwrap();
    assert_eq!(precision.columns.len(), 4);
    assert!(precision.columns[0].identity);
    assert!(!precision.columns[0].nullable);
    assert_eq!(precision.columns[1].postgres_type, "numeric(30,12)");
    assert_eq!(
        precision.columns[2].postgres_type,
        "timestamp(3) with time zone"
    );
    assert!(precision.columns[3].generated);
    assert!(!String::from_utf8(std::fs::read(&output).unwrap())
        .unwrap()
        .contains("default-marker-never-export"));
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

async fn projected_source(pool: &PgPool) {
    source(pool).await;
    // Independent native definitions matching the old 0029 models, not generated from
    // the new projection allowlist. Unknown fixture tables remain explicitly unsupported.
    sqlx::raw_sql("CREATE TABLE promotion_policy_gates(policy_version_id uuid NOT NULL,metric_code varchar(100) NOT NULL,comparator varchar(20) NOT NULL,threshold numeric(20,8) NOT NULL,ordinal integer NOT NULL); INSERT INTO promotion_policy_gates VALUES('11111111-1111-4111-8111-111111111111','quote,\"line\n中文','>=',123456789012.12345678,1); CREATE TABLE archive_manifest_shards(id uuid NOT NULL,manifest_id uuid NOT NULL,shard_key varchar(40) NOT NULL,source_url text NOT NULL,coverage_start timestamptz NOT NULL,coverage_end timestamptz NOT NULL,size_bytes bigint,state varchar(40) NOT NULL,observed_at timestamptz NOT NULL); INSERT INTO archive_manifest_shards VALUES('11111111-1111-4111-8111-111111111112','11111111-1111-4111-8111-111111111111','','EXCLUDED_SOURCE_SENTINEL','2020-01-01T01:02:03.123456+08:00','2020-01-02T01:02:03.123456+08:00',9007199254740993,'READY','2020-01-03T01:02:03.123456+08:00'),('11111111-1111-4111-8111-111111111113','11111111-1111-4111-8111-111111111111','second','EXCLUDED_SOURCE_SENTINEL','2020-01-01T01:02:03.123456+08:00','2020-01-02T01:02:03.123456+08:00',NULL,'READY','2020-01-03T01:02:03.123456+08:00')")
        .execute(pool).await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn historical_native_copy_preserves_decimal_bigint_null_utf8_and_time(pool: PgPool) {
    use contracts::{imports::HistoricalRowExportV1, Id};
    use sqlx::Row;
    projected_source(&pool).await;
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("projection");
    let source_installation_id = Id::new();
    let result = tokio::process::Command::new(env!("CARGO_BIN_EXE_server"))
        .arg("export-historical-rows")
        .arg("--source-installation-id")
        .arg(source_installation_id.to_string())
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
    assert!(result.status.success(), "native row export must succeed");
    let report: HistoricalRowExportV1 =
        serde_json::from_slice(&std::fs::read(output.join("report.json")).unwrap()).unwrap();
    assert_eq!(report.source_installation_id, source_installation_id);
    assert!(report.missing_tables.contains(&"research_programs".into()));
    let gates = report
        .tables
        .iter()
        .find(|t| t.table == "promotion_policy_gates")
        .unwrap();
    let shards = report
        .tables
        .iter()
        .find(|t| t.table == "archive_manifest_shards")
        .unwrap();
    assert_eq!(gates.projected_rows.get(), 1);
    assert_eq!(shards.projected_rows.get(), 2);
    assert_eq!(shards.excluded_columns.len(), 1);
    assert_eq!(shards.excluded_columns[0].column, "source_url");
    let mut connection = pool.acquire().await.unwrap();
    sqlx::raw_sql("CREATE TEMP TABLE restored_gates(policy_version_id uuid,metric_code varchar(100),comparator varchar(20),threshold numeric(20,8),ordinal integer); CREATE TEMP TABLE restored_shards(id uuid,manifest_id uuid,shard_key varchar(40),coverage_start timestamptz,coverage_end timestamptz,size_bytes bigint,state varchar(40),observed_at timestamptz)").execute(&mut *connection).await.unwrap();
    for (table, target) in [(gates, "restored_gates"), (shards, "restored_shards")] {
        let bytes =
            std::fs::read(output.join(format!("{}.csv", table.object_ref.unwrap()))).unwrap();
        assert_eq!(bytes.len() as u64, table.byte_count.unwrap().get());
        assert!(!String::from_utf8_lossy(&bytes).contains("EXCLUDED_SOURCE_SENTINEL"));
        let mut copy = connection
            .copy_in_raw(&format!(
                "COPY {target} FROM STDIN WITH (FORMAT CSV,HEADER true)"
            ))
            .await
            .unwrap();
        copy.send(bytes).await.unwrap();
        assert_eq!(copy.finish().await.unwrap(), table.source_rows.get());
    }
    let restored =
        sqlx::query("SELECT metric_code,threshold::text AS threshold FROM restored_gates")
            .fetch_one(&mut *connection)
            .await
            .unwrap();
    assert_eq!(
        restored.get::<String, _>("metric_code"),
        "quote,\"line\n中文"
    );
    assert_eq!(
        restored.get::<String, _>("threshold"),
        "123456789012.12345678"
    );
    let same: bool = sqlx::query_scalar("SELECT NOT EXISTS((SELECT id,manifest_id,shard_key,coverage_start,coverage_end,size_bytes,state,observed_at FROM public.archive_manifest_shards EXCEPT ALL SELECT * FROM restored_shards) UNION ALL (SELECT * FROM restored_shards EXCEPT ALL SELECT id,manifest_id,shard_key,coverage_start,coverage_end,size_bytes,state,observed_at FROM public.archive_manifest_shards))").fetch_one(&mut *connection).await.unwrap();
    assert!(
        same,
        "native COPY must preserve all selected values exactly"
    );
    assert!(
        report
            .tables
            .iter()
            .find(|t| t.table == "parents")
            .unwrap()
            .unsupported_schema
    );
}

#[sqlx::test(migrations = false)]
async fn historical_copy_rejects_structural_drift_and_native_write_failure(pool: PgPool) {
    use contracts::{imports::HistoricalExclusionReasonV1, Id};
    projected_source(&pool).await;
    sqlx::query("ALTER TABLE promotion_policy_gates ADD COLUMN unexpected text DEFAULT 'NEVER_PROJECT_EXTRA_COLUMN'").execute(&pool).await.unwrap();
    let store = store::Store::from_pool(pool.clone());
    let mut bytes = Vec::new();
    let report = store
        .export_historical_rows(Id::new(), |_, chunk| {
            bytes.extend_from_slice(chunk);
            Ok(())
        })
        .await
        .unwrap();
    let gates = report
        .tables
        .iter()
        .find(|t| t.table == "promotion_policy_gates")
        .unwrap();
    assert!(gates.unsupported_schema);
    assert!(gates.object_ref.is_none());
    assert_eq!(gates.projected_rows.get(), 0);
    assert!(gates
        .excluded_columns
        .iter()
        .all(|c| c.reason == HistoricalExclusionReasonV1::UnsupportedSchema));
    assert!(!String::from_utf8_lossy(&bytes).contains("NEVER_PROJECT_EXTRA_COLUMN"));
    // Linux's native full device exercises an actual ENOSPC write, without filling a disk.
    use std::io::Write;
    let mut full = std::fs::OpenOptions::new()
        .write(true)
        .open("/dev/full")
        .unwrap();
    let failed = store
        .export_historical_rows(Id::new(), |_, chunk| {
            full.write_all(chunk)
                .map_err(|_| store::StoreError::Invalid("native_write_failed"))
        })
        .await;
    assert!(matches!(
        failed,
        Err(store::StoreError::Invalid("native_write_failed"))
    ));
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM archive_manifest_shards")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 2);
}
