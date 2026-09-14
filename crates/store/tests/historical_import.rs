//! Controlled old public tables plus a fresh app schema; not the user's old backup.
#[path = "../../../tests/support/research.rs"]
mod support;
use contracts::{imports::*, Id, SchemaV1};
use sqlx::{PgPool, Row};
use std::{collections::BTreeMap, io::Cursor};
use store::{authority::Actor, Store};

async fn source(pool: &PgPool) {
    sqlx::raw_sql("CREATE TABLE public.alembic_version(version_num text NOT NULL); INSERT INTO public.alembic_version VALUES('0029_portfolio_candidate_exposure'); CREATE TABLE public.jobs(id uuid PRIMARY KEY,kind varchar(100) NOT NULL,resource_type varchar(100) NOT NULL,resource_id uuid NOT NULL,state varchar(20) NOT NULL,payload jsonb NOT NULL,attempt integer NOT NULL,available_at timestamptz NOT NULL,lease_owner varchar(200),lease_expires_at timestamptz,last_error text,created_at timestamptz NOT NULL,updated_at timestamptz NOT NULL); INSERT INTO public.jobs VALUES('11111111-1111-4111-8111-111111111111','RESEARCH','RESEARCH','11111111-1111-4111-8111-111111111112','READY','{}',0,'2020-01-01',NULL,NULL,NULL,'2020-01-01','2020-01-01'); CREATE TABLE public.events(id bigint PRIMARY KEY,kind varchar(100) NOT NULL,aggregate_type varchar(100) NOT NULL,aggregate_id uuid REFERENCES public.jobs(id),actor_kind varchar(40) NOT NULL,actor_metadata jsonb NOT NULL,payload jsonb NOT NULL,created_at timestamptz NOT NULL); INSERT INTO public.events VALUES(9007199254740993,'CREATED','JOB','11111111-1111-4111-8111-111111111111','OPERATOR','{}','{}','2020-01-01')").execute(pool).await.unwrap();
}
async fn exported(
    pool: &PgPool,
    installation: Id,
) -> (HistoricalRowExportV1, BTreeMap<Id, Vec<u8>>) {
    let mut files = BTreeMap::<Id, Vec<u8>>::new();
    let report = Store::from_pool(pool.clone())
        .export_historical_rows(installation, |id, chunk| {
            files.entry(id).or_default().extend_from_slice(chunk);
            Ok(())
        })
        .await
        .unwrap();
    (report, files)
}
async fn import(
    store: &Store,
    actor: &Actor,
    key: &str,
    request: &HistoricalImportRequestV1,
    report: &HistoricalRowExportV1,
    files: &BTreeMap<Id, Vec<u8>>,
) -> Result<contracts::control::CommandResult<HistoricalImportReportV1>, store::StoreError> {
    store
        .import_historical_rows(
            actor,
            key,
            request,
            |reference| {
                assert_eq!(reference, request.export_ref);
                Ok(report.clone())
            },
            |reference, id| {
                assert_eq!(reference, request.export_ref);
                Ok(Cursor::new(files[&id].clone()))
            },
        )
        .await
}
async fn records(pool: &PgPool) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM app.historical_records")
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn dry_run_then_import_replays_original_keys_without_new_business_authority(pool: PgPool) {
    source(&pool).await;
    let (store, actor) = support::operator(&pool).await;
    let (report, files) = exported(&pool, Id::new()).await;
    let mut request = HistoricalImportRequestV1 {
        schema_version: SchemaV1,
        export_ref: Id::new(),
        dry_run: true,
    };
    let dry = import(&store, &actor, "dry", &request, &report, &files)
        .await
        .unwrap();
    assert_eq!(dry.resource.projected_rows.get(), 2);
    assert_eq!(dry.resource.new_rows.get(), 0);
    assert_eq!(records(&pool).await, 0);
    assert!(dry.resource.manual_review_required);
    request.dry_run = false;
    let first = import(&store, &actor, "actual", &request, &report, &files)
        .await
        .unwrap();
    assert_eq!(first.resource.new_rows.get(), 2);
    assert_eq!(first.resource.checked_relationships.get(), 1);
    assert_eq!(records(&pool).await, 2);
    let replay = store
        .import_historical_rows(
            &actor,
            "actual",
            &request,
            |_| panic!("replay must not reload"),
            |_, _| -> Result<Cursor<Vec<u8>>, store::StoreError> {
                panic!("replay must not reopen CSV")
            },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, first.resource.id);
    let again = import(&store, &actor, "other-key", &request, &report, &files)
        .await
        .unwrap();
    assert_eq!(again.resource.new_rows.get(), 0);
    assert_eq!(again.resource.existing_rows.get(), 2);
    assert_eq!(records(&pool).await, 2);
    let job=sqlx::query("SELECT original_key,fields,disposition,uuid_extract_version(id) AS version FROM app.historical_records WHERE source_table='jobs'").fetch_one(&pool).await.unwrap();
    assert_eq!(
        job.get::<serde_json::Value, _>("original_key")["id"],
        "11111111-1111-4111-8111-111111111111"
    );
    assert_eq!(job.get::<serde_json::Value, _>("fields")["state"], "READY");
    assert_eq!(job.get::<String, _>("disposition"), "READ_ONLY_HISTORY");
    assert_eq!(job.get::<i16, _>("version"), 7);
    let event_key: String = sqlx::query_scalar(
        "SELECT original_key->>'id' FROM app.historical_records WHERE source_table='events'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(event_key, "9007199254740993");
    let runs: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(runs, 0);
    let saved = store
        .historical_import_report(&actor, first.resource.id)
        .await
        .unwrap();
    assert_eq!(saved.id, first.resource.id);
    assert!(sqlx::query("UPDATE app.historical_records SET fields='{}'")
        .execute(&pool)
        .await
        .is_err());
    assert!(sqlx::query("DELETE FROM app.historical_import_reports")
        .execute(&pool)
        .await
        .is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn conflict_after_a_prior_table_insert_rolls_back_history_members_and_receipt(pool: PgPool) {
    source(&pool).await;
    let (store, actor) = support::operator(&pool).await;
    let installation = Id::new();
    let (report, files) = exported(&pool, installation).await;
    let request = HistoricalImportRequestV1 {
        schema_version: SchemaV1,
        export_ref: Id::new(),
        dry_run: false,
    };
    import(&store, &actor, "first", &request, &report, &files)
        .await
        .unwrap();
    sqlx::raw_sql("INSERT INTO public.events SELECT 9007199254740994,kind,aggregate_type,aggregate_id,actor_kind,actor_metadata,payload,created_at FROM public.events; UPDATE public.jobs SET state='SUCCEEDED'").execute(&pool).await.unwrap();
    let (changed, files) = exported(&pool, installation).await;
    assert!(matches!(
        import(&store, &actor, "conflict", &request, &changed, &files).await,
        Err(store::StoreError::Conflict)
    ));
    assert_eq!(records(&pool).await, 2);
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.historical_import_reports),(SELECT count(*) FROM app.historical_import_members),(SELECT count(*) FROM app.command_receipts WHERE idempotency_key='conflict')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 2, 0));
    let state: String = sqlx::query_scalar(
        "SELECT fields->>'state' FROM app.historical_records WHERE source_table='jobs'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(state, "READY");
}

#[sqlx::test(migrations = "../../migrations")]
async fn auth_precedes_source_access_and_modified_relation_cannot_be_imported(pool: PgPool) {
    source(&pool).await;
    let (store, actor) = support::operator(&pool).await;
    let (report, mut files) = exported(&pool, Id::new()).await;
    let request = HistoricalImportRequestV1 {
        schema_version: SchemaV1,
        export_ref: Id::new(),
        dry_run: false,
    };
    let denied = store
        .import_historical_rows(
            &Actor::Browser {
                login_id: Id::new(),
            },
            "denied",
            &request,
            |_| panic!("unauthorized source access"),
            |_, _| -> Result<Cursor<Vec<u8>>, store::StoreError> {
                panic!("unauthorized file access")
            },
        )
        .await;
    assert!(denied.is_err());
    let event = report
        .tables
        .iter()
        .find(|t| t.table == "events")
        .unwrap()
        .object_ref
        .unwrap();
    let bytes = String::from_utf8(files[&event].clone())
        .unwrap()
        .replace(
            "11111111-1111-4111-8111-111111111111",
            "11111111-1111-4111-8111-111111111112",
        )
        .into_bytes();
    files.insert(event, bytes);
    assert!(matches!(
        import(&store, &actor, "bad-relation", &request, &report, &files).await,
        Err(store::StoreError::Invalid("historical_import_relationship"))
    ));
    assert_eq!(records(&pool).await, 0);
    let reports: i64 = sqlx::query_scalar("SELECT count(*) FROM app.historical_import_reports")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reports, 0);
}
