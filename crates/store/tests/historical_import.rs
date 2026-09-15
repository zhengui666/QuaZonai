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
                Ok(store::HistoricalImportSource {
                    rows: report.clone(),
                    artifacts: None,
                })
            },
            |reference, id| {
                assert_eq!(reference, request.export_ref);
                Ok(Cursor::new(files[&id].clone()))
            },
            |_| async { panic!("no selected artifacts") },
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
            |_| async { panic!("replay must not publish") },
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
            |_| async { panic!("unauthorized publication") },
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

#[sqlx::test(migrations = "../../migrations")]
async fn report_and_mapping_pages_keep_original_batch_membership(pool: PgPool) {
    use contracts::control::ListQuery;
    source(&pool).await;
    let (store, actor) = support::operator(&pool).await;
    let (source, files) = exported(&pool, Id::new()).await;
    let mut request = HistoricalImportRequestV1 {
        schema_version: SchemaV1,
        export_ref: Id::new(),
        dry_run: true,
    };
    let dry = import(&store, &actor, "dry", &request, &source, &files)
        .await
        .unwrap()
        .resource;
    request.dry_run = false;
    let first = import(&store, &actor, "first", &request, &source, &files)
        .await
        .unwrap()
        .resource;
    let second = import(&store, &actor, "second", &request, &source, &files)
        .await
        .unwrap()
        .resource;
    let page = ListQuery {
        cursor: None,
        limit: 1,
    };
    let reports = store
        .historical_import_reports(&actor, &page)
        .await
        .unwrap();
    assert_eq!(reports.items[0].id, second.id);
    let older = store
        .historical_import_reports(
            &actor,
            &ListQuery {
                cursor: reports.next_cursor,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        older.items.iter().map(|r| r.id).collect::<Vec<_>>(),
        [first.id, dry.id]
    );
    assert!(older.next_cursor.is_none());
    let a = store
        .historical_import_mappings(&actor, second.id, &page)
        .await
        .unwrap();
    let b = store
        .historical_import_mappings(
            &actor,
            second.id,
            &ListQuery {
                cursor: a.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(a.items.len(), 1);
    assert_eq!(b.items.len(), 1);
    assert!(b.next_cursor.is_none());
    assert_ne!(a.items[0].id, b.items[0].id);
    for mapping in a.items.iter().chain(&b.items) {
        assert_eq!(mapping.first_import_id, first.id);
        assert_eq!(
            mapping.key.source_installation_id,
            source.source_installation_id
        );
    }
    assert!(a.items.iter().chain(&b.items).any(|r| r
        .key
        .values
        .get("id")
        .is_some_and(|value| value == "9007199254740993")));
    let original = store
        .historical_import_mappings(
            &actor,
            first.id,
            &ListQuery {
                cursor: None,
                limit: 100,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        original.items.iter().map(|r| r.id).collect::<Vec<_>>(),
        [a.items[0].id, b.items[0].id]
    );
    assert!(store
        .historical_import_mappings(&actor, dry.id, &page)
        .await
        .unwrap()
        .items
        .is_empty());
    assert!(matches!(
        store
            .historical_import_mappings(&actor, Id::new(), &page)
            .await,
        Err(store::StoreError::NotFound)
    ));
    assert!(store
        .historical_import_reports(
            &actor,
            &ListQuery {
                cursor: None,
                limit: 0
            }
        )
        .await
        .is_err());
    let detail = store
        .historical_import_source(&actor, first.id)
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(detail).unwrap(),
        serde_json::to_value(source).unwrap()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_fields_are_scoped_and_native_unicode_pages_reconstruct_exact_values(
    pool: PgPool,
) {
    use contracts::control::ListQuery;
    source(&pool).await;
    sqlx::raw_sql("CREATE TABLE public.clarification_answers(id uuid PRIMARY KEY,question_id uuid NOT NULL,answer_text text NOT NULL,created_at timestamptz NOT NULL); UPDATE public.jobs SET kind=''").execute(&pool).await.unwrap();
    let original = "汉🙂e\u{301}\n".repeat(10_000);
    sqlx::query("INSERT INTO public.clarification_answers VALUES('22222222-2222-4222-8222-222222222222','22222222-2222-4222-8222-222222222223',$1,'2020-01-01')").bind(&original).execute(&pool).await.unwrap();
    let (store, actor) = support::operator(&pool).await;
    let (source, files) = exported(&pool, Id::new()).await;
    let mut request = HistoricalImportRequestV1 {
        schema_version: SchemaV1,
        export_ref: Id::new(),
        dry_run: false,
    };
    let report = import(&store, &actor, "actual", &request, &source, &files)
        .await
        .unwrap()
        .resource;
    request.dry_run = true;
    let dry = import(&store, &actor, "dry", &request, &source, &files)
        .await
        .unwrap()
        .resource;
    let mappings = store
        .historical_import_mappings(
            &actor,
            report.id,
            &ListQuery {
                cursor: None,
                limit: 100,
            },
        )
        .await
        .unwrap();
    let answer = mappings
        .items
        .iter()
        .find(|r| r.key.source_table == "clarification_answers")
        .unwrap();
    let summary = store
        .historical_record_fields(&actor, report.id, answer.id)
        .await
        .unwrap();
    assert_eq!(summary.report_id, report.id);
    assert_eq!(summary.record_id, answer.id);
    assert_eq!(
        summary
            .fields
            .iter()
            .find(|f| f.name == "answer_text")
            .unwrap()
            .character_count
            .unwrap()
            .get(),
        original.chars().count() as u64
    );
    let mut restored = String::new();
    let mut offset = contracts::DbCounter::ZERO;
    let mut pages = 0;
    loop {
        let chunk = store
            .historical_record_field(
                &actor,
                report.id,
                answer.id,
                &HistoricalFieldQueryV1 {
                    name: "answer_text".into(),
                    offset,
                },
            )
            .await
            .unwrap();
        assert_eq!(chunk.offset, offset);
        assert_eq!(
            chunk.total_characters.unwrap().get(),
            original.chars().count() as u64
        );
        let text = chunk.text.unwrap();
        assert!(text.chars().count() <= HISTORICAL_FIELD_CHARS as usize);
        restored.push_str(&text);
        pages += 1;
        if let Some(next) = chunk.next_offset {
            assert!(next > offset);
            offset = next;
        } else {
            break;
        }
    }
    assert!(pages > 1);
    assert_eq!(restored.as_bytes(), original.as_bytes());
    let job = mappings
        .items
        .iter()
        .find(|r| r.key.source_table == "jobs")
        .unwrap();
    for (name, expected) in [
        ("kind", Some("")),
        ("lease_expires_at", None),
        ("id", Some("11111111-1111-4111-8111-111111111111")),
    ] {
        let value = store
            .historical_record_field(
                &actor,
                report.id,
                job.id,
                &HistoricalFieldQueryV1 {
                    name: name.into(),
                    offset: contracts::DbCounter::ZERO,
                },
            )
            .await
            .unwrap();
        assert_eq!(value.text.as_deref(), expected);
        assert_eq!(
            value.total_characters.map(|n| n.get()),
            expected.map(|s| s.chars().count() as u64)
        );
        assert!(value.next_offset.is_none());
    }
    let events = mappings
        .items
        .iter()
        .find(|r| r.key.source_table == "events")
        .unwrap();
    let value = store
        .historical_record_field(
            &actor,
            report.id,
            events.id,
            &HistoricalFieldQueryV1 {
                name: "id".into(),
                offset: contracts::DbCounter::ZERO,
            },
        )
        .await
        .unwrap();
    assert_eq!(value.text.as_deref(), Some("9007199254740993"));
    for name in ["payload", "lease_owner", "id' OR true --"] {
        assert!(matches!(
            store
                .historical_record_field(
                    &actor,
                    report.id,
                    job.id,
                    &HistoricalFieldQueryV1 {
                        name: name.into(),
                        offset: contracts::DbCounter::ZERO
                    }
                )
                .await,
            Err(store::StoreError::NotFound)
        ));
    }
    assert!(matches!(
        store.historical_record_fields(&actor, dry.id, job.id).await,
        Err(store::StoreError::NotFound)
    ));
    assert!(store
        .historical_record_field(
            &actor,
            report.id,
            job.id,
            &HistoricalFieldQueryV1 {
                name: "id".into(),
                offset: contracts::DbCounter::new(i64::MAX as u64).unwrap()
            }
        )
        .await
        .is_err());
    let denied = Actor::Browser {
        login_id: Id::new(),
    };
    assert!(store
        .historical_record_fields(&denied, report.id, job.id)
        .await
        .is_err());
    assert!(store
        .historical_record_field(
            &denied,
            report.id,
            job.id,
            &HistoricalFieldQueryV1 {
                name: "id".into(),
                offset: contracts::DbCounter::ZERO
            }
        )
        .await
        .is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn public_artifact_binding_dry_run_repeat_conflict_and_rollback(pool: PgPool) {
    use contracts::{control::ListQuery, DbCounter};
    use integrations::artifacts::ArtifactStore;
    source(&pool).await;
    sqlx::raw_sql("CREATE TABLE public.mission_artifacts(id uuid PRIMARY KEY,mission_id uuid NOT NULL,turn_id uuid,kind varchar(80) NOT NULL,revision integer NOT NULL,schema_version varchar(40) NOT NULL,state varchar(40) NOT NULL,storage_uri text NOT NULL,metadata jsonb NOT NULL,created_at timestamptz NOT NULL); INSERT INTO public.mission_artifacts SELECT ('00000000-0000-4000-8000-'||lpad(n::text,12,'0'))::uuid,'11111111-1111-4111-8111-111111111111',NULL,'REPORT',1,'1','AVAILABLE','excluded-original-path','{}','2020-01-01' FROM generate_series(1,3) n").execute(&pool).await.unwrap();
    let (store, actor) = support::operator(&pool).await;
    let (rows, files) = exported(&pool, Id::new()).await;
    let bytes = b"\xff\0PUBLIC COPY";
    let mut artifacts:HistoricalArtifactExportV1=serde_json::from_value(serde_json::json!({"schema_version":1,"source_installation_id":rows.source_installation_id,"exported_at":"2020-01-01T00:00:00Z","artifacts":[
        {"identity":{"kind":"ARTIFACT","source_table":"mission_artifacts","source_id":"00000000-0000-4000-8000-000000000001"},"outcome":"COPIED","object_ref":Id::new(),"byte_count":bytes.len().to_string()},
        {"identity":{"kind":"ARTIFACT","source_table":"mission_artifacts","source_id":"00000000-0000-4000-8000-000000000002"},"outcome":"SEALED_RETAINED","object_ref":null,"byte_count":null}
    ]})).unwrap();
    let root = tempfile::tempdir().unwrap();
    let native = ArtifactStore::open(&root.path().join("historical-artifacts")).unwrap();
    let mut request = HistoricalImportRequestV1 {
        schema_version: SchemaV1,
        export_ref: Id::new(),
        dry_run: true,
    };
    for dry in [true, false, false] {
        request.dry_run = dry;
        let key = Id::new().to_string();
        let result = store
            .import_historical_rows(
                &actor,
                &key,
                &request,
                |_| {
                    Ok(store::HistoricalImportSource {
                        rows: rows.clone(),
                        artifacts: Some(artifacts.clone()),
                    })
                },
                |_, id| Ok(Cursor::new(files[&id].clone())),
                |p| {
                    let native = &native;
                    let object = artifacts.artifacts[0].object_ref.unwrap();
                    async move {
                        assert_eq!(p.source_object, object);
                        if !p.dry_run && !p.existing {
                            native.put(p.target.unwrap(), bytes).unwrap();
                        }
                        if !p.dry_run || p.existing {
                            assert_eq!(
                                native.read(p.target.unwrap(), p.byte_count).unwrap(),
                                bytes
                            );
                        }
                        Ok(())
                    }
                },
            )
            .await
            .unwrap();
        let summary = store
            .historical_artifact_summary(&actor, result.resource.id)
            .await
            .unwrap();
        assert_eq!(
            (
                summary.source_records.get(),
                summary.projected_records.get(),
                summary.selected_records.get(),
                summary.readable_records.get(),
                summary.stored_records.get()
            ),
            (3, 3, 2, 1, if dry { 0 } else { 1 })
        );
        let items = store
            .historical_artifact_results(&actor, result.resource.id, &ListQuery::default())
            .await
            .unwrap()
            .items;
        assert_eq!(items.len(), 3);
        let copied = items.iter().find(|r| r.verified_readable).unwrap();
        assert_eq!(copied.stored, !dry);
        if dry {
            assert!(copied.record_id.is_none());
            assert!(root
                .path()
                .join("historical-artifacts")
                .read_dir()
                .unwrap()
                .next()
                .is_none());
        } else {
            let record = copied.record_id.unwrap();
            assert_eq!(
                store
                    .historical_artifact_content(&actor, result.resource.id, record)
                    .await
                    .unwrap()
                    .get(),
                bytes.len() as u64
            );
            assert!(!store
                .discard_unpublished_historical_artifact(record, |_| async {
                    panic!("referenced data cannot be removed")
                })
                .await
                .unwrap());
            let sealed = items
                .iter()
                .find(|r| r.source_outcome == Some(HistoricalArtifactOutcomeV1::SealedRetained))
                .unwrap();
            assert!(matches!(
                store
                    .historical_artifact_content(
                        &actor,
                        result.resource.id,
                        sealed.record_id.unwrap()
                    )
                    .await,
                Err(store::StoreError::NotFound)
            ));
        }
    }
    assert_eq!(
        root.path()
            .join("historical-artifacts")
            .read_dir()
            .unwrap()
            .count(),
        1
    );
    let count_before: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.historical_import_reports")
            .fetch_one(&pool)
            .await
            .unwrap();
    // A same-size native byte mismatch fails the whole batch, not just its item.
    let failed = store
        .import_historical_rows(
            &actor,
            "different-bytes",
            &request,
            |_| {
                Ok(store::HistoricalImportSource {
                    rows: rows.clone(),
                    artifacts: Some(artifacts.clone()),
                })
            },
            |_, id| Ok(Cursor::new(files[&id].clone())),
            |p| {
                let native = &native;
                async move {
                    assert!(p.existing);
                    let original = native.read(p.target.unwrap(), p.byte_count).unwrap();
                    let mut changed = original.clone();
                    changed[0] ^= 1;
                    if changed != original {
                        return Err(store::StoreError::Conflict);
                    }
                    Ok(())
                }
            },
        )
        .await;
    assert!(matches!(failed, Err(store::StoreError::Conflict)));
    artifacts.artifacts[0].identity.source_id =
        "00000000-0000-4000-8000-999999999999".parse().unwrap();
    let failed = store
        .import_historical_rows(
            &actor,
            "foreign-identity",
            &request,
            |_| {
                Ok(store::HistoricalImportSource {
                    rows: rows.clone(),
                    artifacts: Some(artifacts.clone()),
                })
            },
            |_, id| Ok(Cursor::new(files[&id].clone())),
            |_| async { panic!("foreign identity cannot open bytes") },
        )
        .await;
    assert!(matches!(
        failed,
        Err(store::StoreError::Invalid("historical_artifact_identity"))
    ));
    let count_after: i64 = sqlx::query_scalar("SELECT count(*) FROM app.historical_import_reports")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count_before, count_after);
    let active: i64 = sqlx::query_scalar("SELECT count(*) FROM app.artifacts")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(active, 0);
    // PostgreSQL must reject a readable result lacking its measured length.
    let result_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.historical_import_reports LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(sqlx::query("INSERT INTO app.historical_artifact_results(report_id,source_table,source_id,source_outcome,verified_readable,stored) VALUES($1,'mission_artifacts',gen_random_uuid(),'COPIED',true,false)").bind(result_id).execute(&pool).await.is_err());
    let orphan = Id::new();
    native.put(orphan, bytes).unwrap();
    assert!(store
        .discard_unpublished_historical_artifact(orphan, |id| {
            let native = &native;
            async move {
                native.discard_unpublished(id).unwrap();
                Ok(())
            }
        })
        .await
        .unwrap());
    assert!(native
        .read(orphan, DbCounter::new(bytes.len() as u64).unwrap())
        .is_err());
}
