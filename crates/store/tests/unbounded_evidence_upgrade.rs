//! Actual pre-080002 PostgreSQL upgrade, original files and production guards.
//! These controlled fixtures establish storage/authority invariants, not native
//! model execution, REAL market evidence, or scientific qualification.
#[path = "support/data.rs"]
mod data;
mod support;

use contracts::{
    DbCounter, Id, SchemaV1,
    catalogs::{RecordedFeatureFragmentV1, RecordedFeatureInputsV1},
    data::{DatasetView, RecordedFeatureRegisterV1, RecordedFeatureViewV1},
    research::{ArtifactInputRole, DataPartition, InputItemV1, InputPurpose, InputSetCreate},
};
use serde_json::{Value, json};
use sqlx::{PgConnection, PgPool};
use store::Store;

const CUTOVER: i64 = 202610080002;

async fn upgrade_after_shutdown(pool: PgPool) -> PgPool {
    // Mirror deployment, retaining SQLx's test database/connection limits. DDL
    // invalidates old SELECT * plans; do not disable prepared statement caching.
    let options = pool.options().clone();
    let connection_options = pool.connect_options().as_ref().clone();
    pool.close().await;
    let migrations = options
        .clone()
        .connect_with(connection_options.clone())
        .await
        .unwrap();
    Store::from_pool(migrations.clone())
        .migrate()
        .await
        .unwrap();
    migrations.close().await;
    let pool = options.connect_with(connection_options).await.unwrap();
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT success FROM _sqlx_migrations WHERE version=$1")
            .bind(CUTOVER)
            .fetch_one(&pool)
            .await
            .unwrap()
    );
    pool
}

fn sqlstate(error: sqlx::Error, code: &str, message: Option<&str>) {
    let database = error
        .as_database_error()
        .expect("a PostgreSQL rejection, not a client error");
    assert_eq!(database.code().as_deref(), Some(code), "{error:?}");
    if let Some(message) = message {
        assert_eq!(database.message(), message);
    }
}

async fn rows(pool: &PgPool, tables: &[&str]) -> Value {
    let mut result = serde_json::Map::new();
    for table in tables {
        let query = format!(
            "SELECT coalesce(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb) FROM app.{table} t"
        );
        let value: Value = sqlx::query_scalar(sqlx::AssertSqlSafe(query))
            .fetch_one(pool)
            .await
            .unwrap();
        result.insert((*table).into(), value);
    }
    Value::Object(result)
}

async fn authority_schema(pool: &PgPool) -> Value {
    // Definitions and validated foreign keys, not just an empty FK-check query.
    sqlx::query_scalar("SELECT jsonb_build_object(
      'triggers',(SELECT jsonb_agg(jsonb_build_array(c.relname,t.tgname,pg_get_triggerdef(t.oid),t.tgenabled) ORDER BY c.relname,t.tgname)
        FROM pg_trigger t JOIN pg_class c ON c.oid=t.tgrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='app' AND NOT t.tgisinternal),
      'foreign_keys',(SELECT jsonb_agg(jsonb_build_array(c.relname,k.conname,pg_get_constraintdef(k.oid),k.convalidated) ORDER BY c.relname,k.conname)
        FROM pg_constraint k JOIN pg_class c ON c.oid=k.conrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='app' AND k.contype='f'))")
        .fetch_one(pool).await.unwrap()
}

fn count(n: u64) -> DbCounter {
    DbCounter::new(n).unwrap()
}
fn feature_content() -> String {
    concat!("{\"schema_version\":1,\"partition\":\"DISCOVERY\",",
        "\"feature_schema\":[{\"feature_key\":\"price\",\"source_ref\":\"fixture:paired-report\",\"source_key\":\"paired\",",
        "\"availability\":{\"basis\":\"OBSERVED\"},\"max_age_ns\":null}],",
        "\"observations\":[{\"feature_index\":0,\"event_ns\":\"50000000000\",\"observed_available_ns\":\"55000000000\",",
        "\"sequence\":\"0\",\"value\":0.10000000000000002,\"missing_reason\":null},",
        "{\"feature_index\":0,\"event_ns\":\"100000000000\",\"observed_available_ns\":\"110000000000\",",
        "\"sequence\":\"1\",\"value\":null,\"missing_reason\":\"recorded_missing\"}]}").into()
}

async fn register_dataset(f: &data::Fixture, key: &str, raw: &str, padding: usize) -> DatasetView {
    let mut metadata = data::catalog_fixture::metadata();
    metadata.storage_version = key.into();
    metadata.recorded_feature_inputs = Some(RecordedFeatureInputsV1 {
        schema_version: SchemaV1,
        source_selection_start_ns: count(40_000_000_000),
        source_selection_end_ns: count(101_000_000_000),
        partition: DataPartition::Discovery,
        fragments: vec![RecordedFeatureFragmentV1 {
            part_key: "features-0001".into(),
            byte_count: count(raw.len() as u64),
            observations: count(2),
            min_event_ns: count(50_000_000_000),
            max_event_ns: count(100_000_000_000),
            min_observed_available_ns: count(55_000_000_000),
            max_observed_available_ns: count(110_000_000_000),
        }],
    });
    let mut request = data::request(f);
    request.native_storage_version = key.into();
    let mut bytes = serde_json::to_vec(&metadata).unwrap();
    bytes.extend(std::iter::repeat_n(b' ', padding));
    let dataset = data::complete(f, data::ticket(f, key, &request).await, bytes.clone())
        .await
        .unwrap()
        .resource;
    let artifact = dataset.native_metadata_artifact_id.unwrap();
    assert_eq!(
        f.objects.read(artifact, count(bytes.len() as u64)).unwrap(),
        bytes
    );
    dataset
}

async fn register_feature(
    f: &data::Fixture,
    dataset: Id,
    raw: &str,
    key: &str,
) -> RecordedFeatureViewV1 {
    f.store
        .register_recorded_feature(
            &f.actor,
            key,
            &RecordedFeatureRegisterV1 {
                schema_version: SchemaV1,
                project_id: f.project,
                dataset_revision_id: dataset,
                feature_part_key: "features-0001".into(),
                content: raw.into(),
            },
            |id, size| data::read(f.objects.clone(), id, size),
            |object| data::publish(f.objects.clone(), vec![object]),
        )
        .await
        .unwrap()
        .resource
}

// Relational-only probes of the real production triggers. These rows do not
// pretend to be objects published through Store; full original-byte publication
// and replay are exercised separately above/below with the public Store methods.
async fn cloned_artifact(
    c: &mut PgConnection,
    original: Id,
    changes: Value,
) -> Result<Id, sqlx::Error> {
    let id = Id::new();
    let mut patch = json!({"id":id,"storage_object_ref":id.to_string()});
    patch
        .as_object_mut()
        .unwrap()
        .extend(changes.as_object().unwrap().clone());
    sqlx::query("INSERT INTO app.artifacts SELECT (jsonb_populate_record(NULL::app.artifacts,to_jsonb(a)||$2)).* FROM app.artifacts a WHERE id=$1")
        .bind(original.as_uuid()).bind(patch).execute(c).await?;
    Ok(id)
}

async fn cloned_dataset(
    c: &mut PgConnection,
    original: &DatasetView,
    metadata_size: i64,
    quality_size: i64,
    metadata_changes: Value,
    partition: &str,
) -> Result<(Id, Id), sqlx::Error> {
    let mut changes = json!({"byte_count":metadata_size});
    changes
        .as_object_mut()
        .unwrap()
        .extend(metadata_changes.as_object().unwrap().clone());
    let metadata =
        cloned_artifact(c, original.native_metadata_artifact_id.unwrap(), changes).await?;
    let quality = cloned_artifact(
        c,
        original.quality_artifact_id,
        json!({"byte_count":quality_size}),
    )
    .await?;
    let id = Id::new();
    let patch = json!({"id":id,"native_storage_version":id.to_string(),"quality_artifact_id":quality,"partition_role":partition});
    sqlx::query("INSERT INTO app.dataset_revisions SELECT (jsonb_populate_record(NULL::app.dataset_revisions,to_jsonb(d)||$2)).* FROM app.dataset_revisions d WHERE id=$1")
        .bind(original.id.as_uuid()).bind(patch).execute(&mut *c).await?;
    sqlx::query("INSERT INTO app.dataset_registration_evidence(dataset_revision_id,native_metadata_artifact_id,source_revision,runtime_revision,observed_at) SELECT $2,$3,source_revision,runtime_revision,observed_at FROM app.dataset_registration_evidence WHERE dataset_revision_id=$1")
        .bind(original.id.as_uuid()).bind(id.as_uuid()).bind(metadata.as_uuid()).execute(&mut *c).await?;
    Ok((id, metadata))
}

async fn cloned_feature(
    c: &mut PgConnection,
    original: Id,
    dataset: Id,
    metadata: Id,
    changes: Value,
) -> Result<Id, sqlx::Error> {
    let id = cloned_artifact(c, original, changes).await?;
    let patch = json!({"artifact_id":id,"dataset_revision_id":dataset,"native_metadata_artifact_id":metadata,"feature_part_key":id.to_string()});
    sqlx::query("INSERT INTO app.feature_artifact_sources SELECT (jsonb_populate_record(NULL::app.feature_artifact_sources,to_jsonb(s)||$2)).* FROM app.feature_artifact_sources s WHERE artifact_id=$1")
        .bind(original.as_uuid()).bind(patch).execute(c).await?;
    Ok(id)
}

#[sqlx::test(migrations = false)]
async fn upgrade_080002_preserves_frozen_sources_and_large_original_feature_bytes(pool: PgPool) {
    support::migrate_before(&pool, CUTOVER).await;
    let mut f = data::setup(&pool, None).await;
    let original = feature_content();
    let dataset = register_dataset(&f, "before-upgrade", &original, 0).await;
    let feature = register_feature(&f, dataset.id, &original, "original-feature").await;
    let input = f
        .store
        .create_input_set(
            &f.actor,
            "original-frozen-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.project,
                purpose: InputPurpose::Discovery,
                decision_cutoff: data::catalog_fixture::instant(300),
                items: vec![
                    InputItemV1::Dataset {
                        dataset_revision_id: dataset.id,
                        role: DataPartition::Discovery,
                    },
                    InputItemV1::Artifact {
                        artifact_id: feature.artifact_id,
                        role: ArtifactInputRole::Parameters,
                    },
                ],
            },
        )
        .await
        .unwrap()
        .resource;
    let tables = [
        "artifacts",
        "dataset_revisions",
        "dataset_registration_evidence",
        "feature_artifact_sources",
        "input_sets",
        "input_set_items",
        "command_receipts",
    ];
    let before = rows(&pool, &tables).await;
    let guards = authority_schema(&pool).await;
    for (metadata, quality) in [(1_048_577, 1), (1, 1_049_601)] {
        let mut tx = pool.begin().await.unwrap();
        sqlstate(
            cloned_dataset(&mut tx, &dataset, metadata, quality, json!({}), "DISCOVERY")
                .await
                .unwrap_err(),
            "23514",
            Some("native dataset registration evidence mismatch"),
        );
        tx.rollback().await.unwrap();
    }
    let mut tx = pool.begin().await.unwrap();
    sqlstate(
        cloned_feature(
            &mut tx,
            feature.artifact_id,
            dataset.id,
            dataset.native_metadata_artifact_id.unwrap(),
            json!({"byte_count":2_097_153}),
        )
        .await
        .unwrap_err(),
        "23514",
        Some("recorded feature source binding mismatch"),
    );
    tx.rollback().await.unwrap();
    assert_eq!(rows(&pool, &tables).await, before);

    let pool = upgrade_after_shutdown(pool).await;
    f.store = Store::from_pool(pool.clone());
    assert_eq!(authority_schema(&pool).await, guards);
    assert_eq!(rows(&pool, &tables).await, before);
    assert_eq!(
        register_feature(&f, dataset.id, &original, "original-feature")
            .await
            .artifact_id,
        feature.artifact_id
    );
    assert_eq!(
        f.objects
            .read(feature.artifact_id, count(original.len() as u64))
            .unwrap(),
        original.as_bytes()
    );
    assert_eq!(
        rows(&pool, &tables).await,
        before,
        "replay cannot rewrite the frozen original"
    );

    let mut large = original.clone();
    large.extend(std::iter::repeat_n(' ', 2 * 1024 * 1024 + 1));
    let added = register_dataset(&f, "after-upgrade-large", &large, 1024 * 1024 + 1).await;
    let large_feature = register_feature(&f, added.id, &large, "large-feature").await;
    assert_eq!(
        f.objects
            .read(large_feature.artifact_id, count(large.len() as u64))
            .unwrap(),
        large.as_bytes()
    );
    assert_eq!(
        register_feature(&f, added.id, &large, "large-feature")
            .await
            .artifact_id,
        large_feature.artifact_id
    );

    let mut tx = pool.begin().await.unwrap();
    let (large_dataset, metadata) = cloned_dataset(
        &mut tx,
        &dataset,
        1_048_577,
        1_049_601,
        json!({}),
        "DISCOVERY",
    )
    .await
    .unwrap();
    cloned_feature(
        &mut tx,
        feature.artifact_id,
        large_dataset,
        metadata,
        json!({"byte_count":2_097_153}),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    for changes in [
        json!({"access_class":"RESEARCH"}),
        json!({"origin":"SYNTHETIC"}),
        json!({"created_by":"IMPORT"}),
        json!({"storage_object_ref":"different-original"}),
        json!({"byte_count":0}),
    ] {
        let mut tx = pool.begin().await.unwrap();
        sqlstate(
            cloned_dataset(
                &mut tx,
                &dataset,
                1_048_577,
                1_049_601,
                changes,
                "DISCOVERY",
            )
            .await
            .unwrap_err(),
            "23514",
            Some("native dataset registration evidence mismatch"),
        );
        tx.rollback().await.unwrap();
    }
    for (partition, changes, wrong_metadata) in [
        ("SEALED", json!({"byte_count":2_097_153}), false),
        (
            "DISCOVERY",
            json!({"byte_count":2_097_153,"access_class":"OPERATOR"}),
            false,
        ),
        (
            "DISCOVERY",
            json!({"byte_count":2_097_153,"created_by":"RUNTIME"}),
            false,
        ),
        ("DISCOVERY", json!({"byte_count":0}), false),
        ("DISCOVERY", json!({"byte_count":2_097_153}), true),
    ] {
        let mut tx = pool.begin().await.unwrap();
        let (target, metadata) = cloned_dataset(
            &mut tx,
            &dataset,
            1_048_577,
            1_049_601,
            json!({}),
            partition,
        )
        .await
        .unwrap();
        let metadata = if wrong_metadata {
            dataset.native_metadata_artifact_id.unwrap()
        } else {
            metadata
        };
        sqlstate(
            cloned_feature(&mut tx, feature.artifact_id, target, metadata, changes)
                .await
                .unwrap_err(),
            "23514",
            Some("recorded feature source binding mismatch"),
        );
        tx.rollback().await.unwrap();
    }
    for (query, id) in [
        (
            "DELETE FROM app.feature_artifact_sources WHERE artifact_id=$1",
            feature.artifact_id,
        ),
        (
            "UPDATE app.dataset_registration_evidence SET source_revision=source_revision+1 WHERE dataset_revision_id=$1",
            dataset.id,
        ),
        (
            "DELETE FROM app.input_set_items WHERE input_set_id=$1",
            input.header.id,
        ),
        (
            "UPDATE app.artifacts SET byte_count=byte_count+1 WHERE id=$1",
            feature.artifact_id,
        ),
    ] {
        sqlstate(
            sqlx::query(query)
                .bind(id.as_uuid())
                .execute(&pool)
                .await
                .unwrap_err(),
            "23000",
            None,
        );
    }
    assert_eq!(
        f.objects
            .read(feature.artifact_id, count(original.len() as u64))
            .unwrap(),
        original.as_bytes()
    );
}

async fn settled_turn(pool: &PgPool, f: &support::Fixture, key: &str) -> Id {
    use store::turns::{DispatchDecision, TurnOutcome, UsageReceipt};
    let store = Store::from_pool(pool.clone());
    let reservation = store
        .reserve_turn(f.run, &f.fence, &f.request(key))
        .await
        .unwrap();
    assert!(matches!(
        store
            .claim_turn_dispatch(reservation.id, &f.fence)
            .await
            .unwrap(),
        DispatchDecision::Send { .. }
    ));
    store
        .bind_native_turn(reservation.id, &f.fence, &format!("native/{key}"))
        .await
        .unwrap();
    store
        .settle_turn(
            reservation.id,
            &f.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Succeeded,
                actual_tokens: count(12),
                actual_cost: Some("0.25".parse().unwrap()),
                currency: Some("USD".into()),
                reason_code: "NATIVE_OBSERVED".into(),
            },
        )
        .await
        .unwrap();
    reservation.id
}

async fn insert_summary(
    c: &mut PgConnection,
    f: &support::Fixture,
    reservation: Id,
    bytes: i64,
    changes: Value,
) -> Result<Id, sqlx::Error> {
    let mut patch = json!({
        "schema_name":"qz.mission_summary", "storage_version":"1", "byte_count":bytes,
        "access_class":"RESEARCH", "origin":"SYNTHETIC", "created_by":"RUNTIME",
    });
    patch
        .as_object_mut()
        .unwrap()
        .extend(changes.as_object().unwrap().clone());
    let artifact = cloned_artifact(c, f.report, patch).await?;
    sqlx::query("INSERT INTO app.model_turn_summaries(reservation_id,artifact_id,native_item_id) VALUES($1,$2,'original-item')")
        .bind(reservation.as_uuid()).bind(artifact.as_uuid()).execute(c).await?;
    Ok(artifact)
}

async fn historical_copy(c: &mut PgConnection, bytes: i64) -> Result<(Id, Id), sqlx::Error> {
    let report = Id::new();
    let record = Id::new();
    let installation = Id::new();
    sqlx::query("INSERT INTO app.historical_import_reports(id,export_ref,source_installation_id,dry_run,source_report,result) VALUES($1,$2,$3,false,'{\"schema_version\":1}','{\"schema_version\":1}')")
        .bind(report.as_uuid()).bind(Id::new().as_uuid()).bind(installation.as_uuid()).execute(&mut *c).await?;
    sqlx::query("INSERT INTO app.historical_records(id,source_installation_id,source_table,original_key,fields,first_import_id,disposition) VALUES($1,$2,'mission_artifacts',$3,'{\"original\":\"preserved\"}',$4,'READ_ONLY_HISTORY')")
        .bind(record.as_uuid()).bind(installation.as_uuid()).bind(json!({"id":record})).bind(report.as_uuid()).execute(&mut *c).await?;
    sqlx::query("INSERT INTO app.historical_artifact_copies(record_id,byte_count,first_import_id) VALUES($1,$2,$3)")
        .bind(record.as_uuid()).bind(bytes).bind(report.as_uuid()).execute(&mut *c).await?;
    sqlx::query("INSERT INTO app.historical_artifact_results(report_id,source_table,source_id,record_id,source_outcome,verified_readable,stored,byte_count) VALUES($1,'mission_artifacts',$2,$2,'COPIED',true,true,$3)")
        .bind(report.as_uuid()).bind(record.as_uuid()).bind(bytes).execute(&mut *c).await?;
    Ok((report, record))
}

#[sqlx::test(migrations = false)]
async fn upgrade_080002_keeps_turn_roles_and_historical_readability_guards(pool: PgPool) {
    support::migrate_before(&pool, CUTOVER).await;
    let f = support::fixture(&pool, support::budget()).await;
    let original_turn = settled_turn(&pool, &f, "original").await;
    let future_turn = settled_turn(&pool, &f, "large").await;
    let directory = tempfile::tempdir().unwrap();
    let original_bytes = serde_json::to_vec(
        &json!({"schema_version":1,"native_turn_id":"native/original","text":"original evidence"}),
    )
    .unwrap();
    let mut tx = pool.begin().await.unwrap();
    let original_artifact = insert_summary(
        &mut tx,
        &f,
        original_turn,
        original_bytes.len() as i64,
        json!({}),
    )
    .await
    .unwrap();
    let (original_report, original_record) = historical_copy(&mut tx, 1).await.unwrap();
    tx.commit().await.unwrap();
    std::fs::write(
        directory.path().join(original_artifact.to_string()),
        &original_bytes,
    )
    .unwrap();
    let large_bytes = serde_json::to_vec(
        &json!({"schema_version":1,"native_turn_id":"native/large","text":"x".repeat(1_048_577)}),
    )
    .unwrap();
    let tables = [
        "artifacts",
        "codex_sessions",
        "model_turn_reservations",
        "model_turn_dispatches",
        "model_turn_bindings",
        "model_turn_terminals",
        "model_turn_receipts",
        "model_turn_summaries",
        "input_sets",
        "input_set_items",
        "historical_import_reports",
        "historical_records",
        "historical_artifact_copies",
        "historical_artifact_results",
    ];
    let before = rows(&pool, &tables).await;
    let guards = authority_schema(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    sqlstate(
        insert_summary(
            &mut tx,
            &f,
            future_turn,
            large_bytes.len() as i64,
            json!({}),
        )
        .await
        .unwrap_err(),
        "23514",
        Some("public summary requires the exact successful native Turn, role and producer"),
    );
    tx.rollback().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlstate(
        historical_copy(&mut tx, 67_108_865).await.unwrap_err(),
        "23514",
        None,
    );
    tx.rollback().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    sqlstate(sqlx::query("INSERT INTO app.historical_artifact_results(report_id,source_table,source_id,source_outcome,verified_readable,stored,byte_count) VALUES($1,'mission_artifacts',$2,'COPIED',true,false,67108865)")
        .bind(original_report.as_uuid()).bind(Id::new().as_uuid()).execute(&mut *tx).await.unwrap_err(), "23514", None);
    tx.rollback().await.unwrap();
    assert_eq!(rows(&pool, &tables).await, before);

    let pool = upgrade_after_shutdown(pool).await;
    assert_eq!(rows(&pool, &tables).await, before);
    assert_eq!(authority_schema(&pool).await, guards);
    assert_eq!(
        std::fs::read(directory.path().join(original_artifact.to_string())).unwrap(),
        original_bytes
    );
    for changes in [
        json!({"access_class":"EVALUATOR_ONLY"}),
        json!({"producer_run_id":null,"producer_attempt_id":null}),
        json!({"origin":"FIXTURE"}),
        json!({"storage_object_ref":"not-the-original-id"}),
        json!({"byte_count":0}),
    ] {
        let mut tx = pool.begin().await.unwrap();
        sqlstate(
            insert_summary(&mut tx, &f, future_turn, large_bytes.len() as i64, changes)
                .await
                .unwrap_err(),
            "23514",
            Some("public summary requires the exact successful native Turn, role and producer"),
        );
        tx.rollback().await.unwrap();
    }
    let mut tx = pool.begin().await.unwrap();
    sqlstate(
        insert_summary(&mut tx, &f, Id::new(), large_bytes.len() as i64, json!({}))
            .await
            .unwrap_err(),
        "23514",
        Some("public summary requires the exact successful native Turn, role and producer"),
    );
    tx.rollback().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let large_artifact = insert_summary(
        &mut tx,
        &f,
        future_turn,
        large_bytes.len() as i64,
        json!({}),
    )
    .await
    .unwrap();
    historical_copy(&mut tx, 67_108_865).await.unwrap();
    tx.commit().await.unwrap();
    std::fs::write(
        directory.path().join(large_artifact.to_string()),
        &large_bytes,
    )
    .unwrap();
    assert_eq!(
        std::fs::read(directory.path().join(large_artifact.to_string())).unwrap(),
        large_bytes
    );
    let original: Value = sqlx::query_scalar(
        "SELECT to_jsonb(s) FROM app.model_turn_summaries s WHERE reservation_id=$1",
    )
    .bind(original_turn.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(original["artifact_id"], original_artifact.to_string());
    for (readable, size, accepted) in [
        (true, Some(67_108_865i64), true),
        (false, None, true),
        (true, None, false),
        (false, Some(1), false),
        (true, Some(0), false),
    ] {
        let mut tx = pool.begin().await.unwrap();
        let result = sqlx::query("INSERT INTO app.historical_artifact_results(report_id,source_table,source_id,source_outcome,verified_readable,stored,byte_count) VALUES($1,'mission_artifacts',$2,$3,$4,false,$5)")
            .bind(original_report.as_uuid()).bind(Id::new().as_uuid()).bind(if readable { "COPIED" } else { "MISSING" }).bind(readable).bind(size).execute(&mut *tx).await;
        if accepted {
            result.unwrap();
        } else {
            sqlstate(result.unwrap_err(), "23514", None);
        }
        tx.rollback().await.unwrap();
    }
    let mut tx = pool.begin().await.unwrap();
    sqlstate(
        historical_copy(&mut tx, 0).await.unwrap_err(),
        "23514",
        None,
    );
    tx.rollback().await.unwrap();
    sqlstate(sqlx::query("INSERT INTO app.historical_artifact_copies(record_id,byte_count,first_import_id) VALUES($1,67108865,$2)")
        .bind(Id::new().as_uuid()).bind(original_report.as_uuid()).execute(&pool).await.unwrap_err(),"23503",None);
    for (query, id) in [
        (
            "DELETE FROM app.model_turn_summaries WHERE reservation_id=$1",
            original_turn,
        ),
        (
            "UPDATE app.model_turn_receipts SET actual_tokens=999 WHERE reservation_id=$1",
            original_turn,
        ),
        (
            "UPDATE app.historical_artifact_copies SET byte_count=2 WHERE record_id=$1",
            original_record,
        ),
        (
            "DELETE FROM app.historical_artifact_results WHERE report_id=$1",
            original_report,
        ),
        (
            "UPDATE app.codex_sessions SET role='INDEPENDENT_REVIEWER' WHERE id=$1",
            f.session,
        ),
    ] {
        sqlstate(
            sqlx::query(query)
                .bind(id.as_uuid())
                .execute(&pool)
                .await
                .unwrap_err(),
            "23000",
            None,
        );
    }
}

// Exercise CHECKs copied directly from the actual migrated PostgreSQL tables.
// LIKE does not copy FKs/triggers: these isolated boundary probes intentionally
// do not claim admission/publication of a native task, experiment or review.
// Real table producer/FK/immutability boundaries are tested in the cases above.
async fn constraint_probe(
    pool: &PgPool,
    table: &str,
    row: &Value,
    field: &str,
    value: Value,
    expected_error: Option<&str>,
) {
    let mut tx = pool.begin().await.unwrap();
    sqlx::raw_sql(sqlx::AssertSqlSafe(format!("CREATE TEMP TABLE qz_evidence_constraint_probe (LIKE app.{table} INCLUDING CONSTRAINTS) ON COMMIT DROP")))
        .execute(&mut *tx).await.unwrap();
    let mut changed = row.clone();
    changed[field] = value.clone();
    let query = format!(
        "INSERT INTO qz_evidence_constraint_probe SELECT (jsonb_populate_record(NULL::app.{table},$1)).*"
    );
    let result = sqlx::query(sqlx::AssertSqlSafe(query))
        .bind(&changed)
        .execute(&mut *tx)
        .await;
    if let Some(code) = expected_error {
        sqlstate(result.unwrap_err(), code, None);
    } else {
        assert_eq!(result.unwrap().rows_affected(), 1);
        let observed: Value =
            sqlx::query_scalar("SELECT to_jsonb(p) FROM qz_evidence_constraint_probe p")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!(
            observed[field], value,
            "the complete collection, including its tail, must survive"
        );
        // Probe the same CHECK on UPDATE as well as INSERT, without weakening
        // the production table's independent immutable UPDATE prohibition.
        let update = format!("UPDATE qz_evidence_constraint_probe SET {field}=$1");
        // jsonb_populate_record maps a JSON null field to SQL NULL on INSERT.
        // Match that on UPDATE: binding Value::Null directly stores JSONB null,
        // which is not an allowed nullable nonempty array.
        let update_value = (!value.is_null()).then_some(&value);
        assert_eq!(
            sqlx::query(sqlx::AssertSqlSafe(update.as_str()))
                .bind(update_value)
                .execute(&mut *tx)
                .await
                .unwrap()
                .rows_affected(),
            1
        );
        if value.is_null() {
            let is_null = format!("SELECT {field} IS NULL FROM qz_evidence_constraint_probe");
            assert!(
                sqlx::query_scalar::<_, bool>(sqlx::AssertSqlSafe(is_null))
                    .fetch_one(&mut *tx)
                    .await
                    .unwrap()
            );
            // SQL NULL remains permitted, but a JSONB null must still fail the
            // shape CHECK both before and after removing the collection cap.
            sqlstate(
                sqlx::query(sqlx::AssertSqlSafe(update.as_str()))
                    .bind(&value)
                    .execute(&mut *tx)
                    .await
                    .unwrap_err(),
                "23514",
                None,
            );
        }
    }
    tx.rollback().await.unwrap();
}

#[sqlx::test(migrations = false)]
async fn upgrade_080002_and_later_collection_checks_remove_only_upper_bounds(pool: PgPool) {
    support::migrate_before(&pool, CUTOVER).await;
    let fixture = support::fixture(&pool, support::budget()).await;
    let policy: Value =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM app.evaluation_policies p WHERE project_id=$1")
            .bind(fixture.project.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(policy["sealed_metric_requirements"].is_null());
    assert!(policy["portfolio_metric_requirements"].is_null());
    let id = Id::new();
    let at = "2026-10-08T00:00:00Z";
    let task = json!({"run_id":id,"parameters_artifact_id":id,"input_bindings":[{"kind":"ARTIFACT"}],"image_ref":"controlled-native-image","cpu":1,"capability_snapshot_artifact_id":id,"output_schemas":[{"name":"qz.fixture","version":"1"}],"origin":"FIXTURE","access_class":"RESEARCH","created_at":at});
    let external = json!({"experiment_id":id,"compile_run_id":id,"dataset_revision_id":id,"feature_artifact_ids":[id],"request":{"schema_version":1},"evaluation_request":{"schema_version":1},"created_at":at});
    let review = json!({"reservation_id":id,"summary_artifact_id":id,"decision":"INCONCLUSIVE","reasons":["controlled-boundary"],"created_at":at});
    let probes = [
        ("run_native_tasks", &task, "input_bindings", 256, false),
        ("run_native_tasks", &task, "output_schemas", 64, false),
        (
            "external_experiment_requests",
            &external,
            "feature_artifact_ids",
            16,
            false,
        ),
        ("mission_reviews", &review, "reasons", 32, false),
        (
            "evaluation_policies",
            &policy,
            "sealed_metric_requirements",
            64,
            true,
        ),
        (
            "evaluation_policies",
            &policy,
            "portfolio_metric_requirements",
            64,
            true,
        ),
    ];
    let expanded = |n: usize| Value::Array((0..n).map(|i| json!({"original_ordinal":i})).collect());
    for (table, row, field, old_max, nullable) in &probes {
        constraint_probe(&pool, table, row, field, expanded(*old_max), None).await;
        constraint_probe(
            &pool,
            table,
            row,
            field,
            expanded(*old_max + 1),
            Some("23514"),
        )
        .await;
        constraint_probe(&pool, table, row, field, json!([]), Some("23514")).await;
        constraint_probe(&pool, table, row, field, json!({}), Some("23514")).await;
        constraint_probe(
            &pool,
            table,
            row,
            field,
            Value::Null,
            if *nullable { None } else { Some("23502") },
        )
        .await;
    }
    let before = rows(
        &pool,
        &[
            "evaluation_policies",
            "experiment_families",
            "input_sets",
            "input_set_items",
        ],
    )
    .await;
    let guards = authority_schema(&pool).await;
    let pool = upgrade_after_shutdown(pool).await;
    // The policy collections are removed by the separate additive 080004;
    // testing latest migration here does not attribute them to 080002.
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT success FROM _sqlx_migrations WHERE version=202610080004"
        )
        .fetch_one(&pool)
        .await
        .unwrap()
    );
    assert_eq!(
        rows(
            &pool,
            &[
                "evaluation_policies",
                "experiment_families",
                "input_sets",
                "input_set_items"
            ]
        )
        .await,
        before
    );
    assert_eq!(authority_schema(&pool).await, guards);
    for (table, row, field, old_max, nullable) in &probes {
        constraint_probe(&pool, table, row, field, expanded(*old_max + 1), None).await;
        constraint_probe(&pool, table, row, field, expanded(*old_max * 2 + 1), None).await;
        constraint_probe(&pool, table, row, field, json!([]), Some("23514")).await;
        constraint_probe(&pool, table, row, field, json!({}), Some("23514")).await;
        constraint_probe(
            &pool,
            table,
            row,
            field,
            Value::Null,
            if *nullable { None } else { Some("23502") },
        )
        .await;
    }
    // Policy mutability is not relaxed just because its collection can grow.
    let policy_id = policy["id"].as_str().unwrap();
    sqlstate(sqlx::query("UPDATE app.evaluation_policies SET sealed_metric_requirements=$2 WHERE id=$1::text::uuid")
        .bind(policy_id).bind(expanded(65)).execute(&pool).await.unwrap_err(),"23000",Some("immutable domain record"));
}
