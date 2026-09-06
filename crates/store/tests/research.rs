//! Real PostgreSQL preparation transactions. Fixtures do not represent deliverable data.
#[path = "../../../tests/support/research.rs"]
mod support;
use contracts::research::*;
use sqlx::PgPool;
use store::StoreError;
use support::*;
fn code(error: StoreError, expected: &str) {
    let StoreError::Domain(domain::DomainError::Fields(fields)) = error else {
        panic!("expected {expected}, got {error:?}")
    };
    assert_eq!(fields[0].code, expected);
}
async fn count(pool: &PgPool, table: &str) -> i64 {
    // Test-owned table names, never user interpolation.
    sqlx::query_scalar(&format!("SELECT count(*) FROM app.{table}"))
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn publication_is_atomic_ordered_idempotent_and_cannot_gain_late_members(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    let request = f.input(InputPurpose::Validation);
    let (a, b) = tokio::join!(
        store.create_input_set(&actor, "input-one", &request),
        store.create_input_set(&actor, "input-one", &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.header.id, b.resource.header.id);
    assert_eq!(a.resource.header.decision_cutoff, request.decision_cutoff);
    assert_eq!(a.resource.items[0].item, request.items[0]);
    assert_eq!(a.resource.items[1].item, request.items[1]);
    assert_eq!(a.resource.items[0].origin, DataOrigin::Fixture);
    assert_eq!(a.resource.items[0].pit_status, Some(PitStatus::Unverified));
    assert_eq!(count(&pool, "input_sets").await, 1);
    assert_eq!(count(&pool, "input_set_items").await, 2);
    let body = serde_json::to_string(&a.resource).unwrap();
    assert!(!body.contains("native-fixture"));
    assert!(!body.contains("storage_object_ref"));
    let err=sqlx::query("INSERT INTO app.input_set_items(input_set_id,dataset_revision_id,role,ordinal) VALUES($1,$2,'FORWARD',2)")
        .bind(a.resource.header.id.as_uuid()).bind(f.forward.as_uuid()).execute(&pool).await.unwrap_err();
    assert_eq!(
        err.as_database_error().unwrap().code().as_deref(),
        Some("23000")
    );
    let mut changed = request.clone();
    changed.items.reverse();
    assert!(matches!(
        store.create_input_set(&actor, "input-one", &changed).await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert_eq!(count(&pool, "input_sets").await, 1);
    // Source revocation never turns reading the immutable original success into a new consume.
    sqlx::query("INSERT INTO app.data_use_revocations(grant_id,effective_at,reason_code,reason) VALUES($1,clock_timestamp(),'TEST','fixture revoked')")
        .bind(f.grant.as_uuid()).execute(&pool).await.unwrap();
    assert!(
        store
            .create_input_set(&actor, "input-one", &request)
            .await
            .unwrap()
            .replayed
    );
    code(
        store
            .create_input_set(&actor, "input-new", &request)
            .await
            .unwrap_err(),
        "DATA_USE_NOT_AUTHORIZED",
    );
    assert_eq!(count(&pool, "input_sets").await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_inputs_leave_no_head_members_or_receipt_and_no_scope_escape(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    let other = setup(&pool, &store, &actor).await;
    let base = f.input(InputPurpose::Validation);
    let receipts = count(&pool, "command_receipts").await;
    let mut r = base.clone();
    r.items.push(r.items[0].clone());
    code(
        store
            .create_input_set(&actor, "duplicate", &r)
            .await
            .unwrap_err(),
        "DUPLICATE_INPUT",
    );
    let mut r = base.clone();
    r.items[1] = InputItemV1::Artifact {
        artifact_id: other.artifact,
        role: ArtifactInputRole::Parameters,
    };
    code(
        store
            .create_input_set(&actor, "cross", &r)
            .await
            .unwrap_err(),
        "ARTIFACT_BINDING",
    );
    let mut r = base.clone();
    r.items[0] = InputItemV1::Dataset {
        dataset_revision_id: f.sealed,
        role: DataPartition::Validation,
    };
    code(
        store
            .create_input_set(&actor, "sealed-alias", &r)
            .await
            .unwrap_err(),
        "DATASET_PARTITION_OR_ASOF",
    );
    let mut r = base.clone();
    r.items[0] = InputItemV1::Dataset {
        dataset_revision_id: f.sealed,
        role: DataPartition::Sealed,
    };
    code(
        store
            .create_input_set(&actor, "sealed-purpose", &r)
            .await
            .unwrap_err(),
        "PARTITION_PURPOSE_MISMATCH",
    );
    let mut r = base.clone();
    r.decision_cutoff = "2019-01-01T00:00:00Z".parse().unwrap();
    code(
        store
            .create_input_set(&actor, "future-data", &r)
            .await
            .unwrap_err(),
        "DATASET_PARTITION_OR_ASOF",
    );
    let mut r = base.clone();
    r.decision_cutoff = "2099-01-01T00:00:00Z".parse().unwrap();
    code(
        store
            .create_input_set(&actor, "future-cutoff", &r)
            .await
            .unwrap_err(),
        "FUTURE_CUTOFF",
    );
    let mut r = base.clone();
    r.decision_cutoff = "2020-01-03T00:00:00.000000001Z".parse().unwrap();
    code(
        store
            .create_input_set(&actor, "lost-precision", &r)
            .await
            .unwrap_err(),
        "DATABASE_TIME_PRECISION",
    );
    let mut r = base.clone();
    r.items[1] = InputItemV1::Artifact {
        artifact_id: f.artifact,
        role: ArtifactInputRole::Signals,
    };
    code(
        store
            .create_input_set(&actor, "role-lie", &r)
            .await
            .unwrap_err(),
        "ARTIFACT_BINDING",
    );
    assert_eq!(count(&pool, "input_sets").await, 0);
    assert_eq!(count(&pool, "input_set_items").await, 0);
    assert_eq!(count(&pool, "command_receipts").await, receipts);
    store
        .create_input_set(&actor, "valid", &base)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn source_disable_and_grant_revocation_are_rechecked_after_real_lock_waits(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    let request = f.input(InputPurpose::Validation);
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("UPDATE app.data_sources SET enabled=false WHERE id=$1")
        .bind(f.source.as_uuid())
        .execute(&mut *blocker)
        .await
        .unwrap();
    let (s, a, r) = (store.clone(), actor.clone(), request.clone());
    let task = tokio::spawn(async move { s.create_input_set(&a, "source-disabled", &r).await });
    wait_for_query_lock(&pool, "SELECT id,runtime_id,enabled FROM app.data_sources").await;
    blocker.commit().await.unwrap();
    code(task.await.unwrap().unwrap_err(), "SOURCE_DISABLED");
    sqlx::query("UPDATE app.data_sources SET enabled=true WHERE id=$1")
        .bind(f.source.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.data_use_revocations(grant_id,effective_at,reason_code,reason) VALUES($1,clock_timestamp(),'TEST','revoked in race')")
        .bind(f.grant.as_uuid()).execute(&mut *blocker).await.unwrap();
    let (s, a, r) = (store.clone(), actor.clone(), request.clone());
    let task = tokio::spawn(async move { s.create_input_set(&a, "revoked", &r).await });
    wait_for_query_lock(
        &pool,
        "SELECT id,source_id,valid_from,valid_until FROM app.data_use_grants",
    )
    .await;
    blocker.commit().await.unwrap();
    code(task.await.unwrap().unwrap_err(), "DATA_USE_NOT_AUTHORIZED");
    assert_eq!(count(&pool, "input_sets").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn consumer_first_grant_lock_serializes_revocation_without_losing_history(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    let mut consumer = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM app.data_use_grants WHERE id=$1 FOR SHARE")
        .bind(f.grant.as_uuid())
        .execute(&mut *consumer)
        .await
        .unwrap();
    let p = pool.clone();
    let grant = f.grant;
    let task = tokio::spawn(async move {
        sqlx::query("INSERT INTO app.data_use_revocations(grant_id,effective_at,reason_code,reason) VALUES($1,clock_timestamp(),'TEST','serialized')").bind(grant.as_uuid()).execute(&p).await
    });
    wait_for_query_lock(&pool, "INSERT INTO app.data_use_revocations").await;
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM app.data_use_revocations")
        .fetch_one(&mut *consumer)
        .await
        .unwrap();
    assert_eq!(n, 0);
    consumer.commit().await.unwrap();
    task.await.unwrap().unwrap();
    assert_eq!(count(&pool, "data_use_revocations").await, 1);
    assert_eq!(count(&pool, "data_use_grants").await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn policies_allocate_one_immutable_family_and_keep_exact_intent_on_replay(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    let input = store
        .create_input_set(&actor, "input", &f.input(InputPurpose::Validation))
        .await
        .unwrap()
        .resource
        .header
        .id;
    let request = f.policy(input);
    let (a, b) = tokio::join!(
        store.create_evaluation_policy(&actor, "policy", &request),
        store.create_evaluation_policy(&actor, "policy", &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.version, 1);
    assert_eq!(
        a.resource.selection_rule.family_id,
        b.resource.selection_rule.family_id
    );
    assert_eq!(
        a.resource.metric_requirements[0]
            .threshold_low
            .as_ref()
            .unwrap()
            .as_decimal()
            .to_plain_string(),
        "0.10000000000000001"
    );
    assert_eq!(count(&pool, "evaluation_policies").await, 1);
    assert_eq!(count(&pool, "experiment_families").await, 1);
    let root: String =
        sqlx::query_scalar("SELECT root_lineage_id::text FROM app.projects WHERE id=$1")
            .bind(f.project.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(a.resource.selection_rule.root_lineage_id.to_string(), root);
    let body = serde_json::to_value(
        store
            .evaluation_policy(&actor, a.resource.id)
            .await
            .unwrap(),
    )
    .unwrap();
    assert_eq!(body, serde_json::to_value(&a.resource).unwrap());
    let err = sqlx::query("UPDATE app.evaluation_policies SET require_real_data=false WHERE id=$1")
        .bind(a.resource.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().unwrap().code().as_deref(),
        Some("23000")
    );
    let mut r = request.clone();
    r.question.push('!');
    assert!(matches!(
        store.create_evaluation_policy(&actor, "policy", &r).await,
        Err(StoreError::IdempotencyConflict)
    ));
    let second = store
        .create_evaluation_policy(&actor, "policy-two", &r)
        .await
        .unwrap();
    assert_eq!(second.resource.version, 2);
    assert_ne!(
        second.resource.selection_rule.family_id,
        a.resource.selection_rule.family_id
    );
    assert_eq!(
        second.resource.selection_rule.root_lineage_id, a.resource.selection_rule.root_lineage_id,
        "family creation cannot reset exposure lineage"
    );
    let list = store
        .evaluation_policies(
            &actor,
            &ResearchListQuery {
                project_id: f.project,
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(list.items[0].id, second.resource.id);
    assert!(list.next_cursor.is_some());
    let next = store
        .evaluation_policies(
            &actor,
            &ResearchListQuery {
                project_id: f.project,
                cursor: list.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(next.items[0].id, a.resource.id);
    assert!(next.next_cursor.is_none());
    assert_eq!(
        count(&pool, "evaluations").await,
        0,
        "registration is not native execution or PASS"
    );
    assert_eq!(count(&pool, "runs").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn policy_binding_rejects_foreign_input_assumptions_and_wrong_sealed_identity(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    let other = setup(&pool, &store, &actor).await;
    let input = store
        .create_input_set(&actor, "input", &f.input(InputPurpose::Validation))
        .await
        .unwrap()
        .resource
        .header
        .id;
    let foreign = store
        .create_input_set(&actor, "foreign", &other.input(InputPurpose::Validation))
        .await
        .unwrap()
        .resource
        .header
        .id;
    let base = f.policy(input);
    let receipts = count(&pool, "command_receipts").await;
    let mut r = base.clone();
    r.comparison_input_set_id = foreign;
    code(
        store
            .create_evaluation_policy(&actor, "cross-input", &r)
            .await
            .unwrap_err(),
        "PROJECT_MISMATCH",
    );
    let mut r = base.clone();
    r.execution_assumptions_id = other.assumptions;
    code(
        store
            .create_evaluation_policy(&actor, "cross-fees", &r)
            .await
            .unwrap_err(),
        "PROJECT_MISMATCH",
    );
    let mut r = base.clone();
    r.split_policy.sealed_revision_id = f.validation;
    code(
        store
            .create_evaluation_policy(&actor, "reuse-validation", &r)
            .await
            .unwrap_err(),
        "SEALED_COMPARISON_BINDING",
    );
    let mut r = base.clone();
    r.split_policy.sealed_revision_id = f.discovery;
    code(
        store
            .create_evaluation_policy(&actor, "bad-role", &r)
            .await
            .unwrap_err(),
        "DATASET_PARTITION_OR_ASOF",
    );
    let mut r = base.clone();
    r.validity_seconds = "9223372036854775807".to_string().try_into().unwrap();
    code(
        store
            .create_evaluation_policy(&actor, "overflow", &r)
            .await
            .unwrap_err(),
        "TIME_RANGE",
    );
    assert_eq!(count(&pool, "evaluation_policies").await, 0);
    assert_eq!(count(&pool, "experiment_families").await, 0);
    assert_eq!(count(&pool, "command_receipts").await, receipts);
    let sealed = store
        .create_input_set(&actor, "sealed", &f.input(InputPurpose::Sealed))
        .await
        .unwrap()
        .resource
        .header
        .id;
    let mut r = base;
    r.selection.evaluation_kind = SelectionEvaluationKind::Sealed;
    r.comparison_input_set_id = sealed;
    store
        .create_evaluation_policy(&actor, "sealed-policy", &r)
        .await
        .unwrap();
    r.split_policy.sealed_revision_id = other.sealed;
    code(
        store
            .create_evaluation_policy(&actor, "unrelated-sealed", &r)
            .await
            .unwrap_err(),
        "SEALED_COMPARISON_BINDING",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn archived_projects_reject_preparation_without_publication(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    sqlx::query(
        "UPDATE app.projects SET state='ARCHIVED',archived_at=clock_timestamp() WHERE id=$1",
    )
    .bind(f.project.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    assert!(matches!(
        store
            .create_input_set(&actor, "archived", &f.input(InputPurpose::Discovery))
            .await,
        Err(StoreError::Domain(domain::DomainError::AdmissionClosed))
    ));
    assert_eq!(count(&pool, "input_sets").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_receipt_transaction_does_not_publish_policy_family_or_input(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    let input = store
        .create_input_set(&actor, "input", &f.input(InputPurpose::Validation))
        .await
        .unwrap()
        .resource
        .header
        .id;
    sqlx::raw_sql("CREATE FUNCTION app.fail_preparation_receipt() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation IN ('INPUT_SET_CREATE','EVALUATION_POLICY_CREATE') THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='disposable test failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER fail_preparation_receipt BEFORE INSERT ON app.command_receipts FOR EACH ROW EXECUTE FUNCTION app.fail_preparation_receipt();").execute(&pool).await.unwrap();
    for err in [
        store
            .create_input_set(&actor, "receipt-fail", &f.input(InputPurpose::Discovery))
            .await
            .unwrap_err(),
        store
            .create_evaluation_policy(&actor, "receipt-fail", &f.policy(input))
            .await
            .unwrap_err(),
    ] {
        let StoreError::Database(err) = err else {
            panic!("expected deliberate database failure")
        };
        assert_eq!(
            err.as_database_error().unwrap().code().as_deref(),
            Some("23514")
        );
    }
    assert_eq!(count(&pool, "input_sets").await, 1);
    assert_eq!(count(&pool, "input_set_items").await, 2);
    assert_eq!(count(&pool, "evaluation_policies").await, 0);
    assert_eq!(count(&pool, "experiment_families").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn historical_or_future_grants_and_invalid_pit_never_authorize_new_preparation(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    for (version, start, end, pit, expected) in [
        (
            2,
            -172800_i64,
            -86400_i64,
            "UNVERIFIED",
            "DATA_USE_NOT_AUTHORIZED",
        ),
        (3, 86400, 172800, "UNVERIFIED", "DATA_USE_NOT_AUTHORIZED"),
        (4, -86400, 86400, "INVALID", "DATASET_PARTITION_OR_ASOF"),
    ] {
        let grant = contracts::Id::new();
        let dataset = contracts::Id::new();
        sqlx::query("INSERT INTO app.data_use_grants(id,source_id,version,license_reference,evidence_artifact_id,allowed_uses,valid_from,valid_until,authorized_by) VALUES($1,$2,$3,'fixture',$4,'RESEARCH',clock_timestamp()+$5*interval '1 second',clock_timestamp()+$6*interval '1 second','OPERATOR')")
            .bind(grant.as_uuid()).bind(f.source.as_uuid()).bind(version).bind(f.artifact.as_uuid()).bind(start as f64).bind(end as f64).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO app.dataset_revisions(id,source_id,data_use_grant_id,native_snapshot_ref,native_storage_version,universe_version_id,schema_version,data_kind,partition_role,event_start,event_end,available_through,row_count,timezone,quality_artifact_id,pit_status,revision_policy,origin) SELECT $1,source_id,$2,$3,native_storage_version,universe_version_id,schema_version,data_kind,partition_role,event_start,event_end,available_through,row_count,timezone,quality_artifact_id,$4,revision_policy,origin FROM app.dataset_revisions WHERE id=$5")
            .bind(dataset.as_uuid()).bind(grant.as_uuid()).bind(format!("fixture/{dataset}")).bind(pit).bind(f.validation.as_uuid()).execute(&pool).await.unwrap();
        let mut r = f.input(InputPurpose::Validation);
        r.items[0] = InputItemV1::Dataset {
            dataset_revision_id: dataset,
            role: DataPartition::Validation,
        };
        code(
            store
                .create_input_set(&actor, &format!("negative-{version}"), &r)
                .await
                .unwrap_err(),
            expected,
        );
    }
    assert_eq!(count(&pool, "input_sets").await, 0);
    // The original currently valid grant and explicitly non-deliverable fixture still work.
    store
        .create_input_set(&actor, "positive", &f.input(InputPurpose::Validation))
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn distinct_policy_commands_allocate_distinct_versions_without_resetting_lineage(
    pool: PgPool,
) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    let input = store
        .create_input_set(&actor, "input", &f.input(InputPurpose::Validation))
        .await
        .unwrap()
        .resource
        .header
        .id;
    let request = f.policy(input);
    let (a, b) = tokio::join!(
        store.create_evaluation_policy(&actor, "policy-a", &request),
        store.create_evaluation_policy(&actor, "policy-b", &request)
    );
    let (a, b) = (a.unwrap().resource, b.unwrap().resource);
    let mut versions = [a.version, b.version];
    versions.sort();
    assert_eq!(versions, [1, 2]);
    assert_ne!(a.id, b.id);
    assert_ne!(a.selection_rule.family_id, b.selection_rule.family_id);
    assert_eq!(
        a.selection_rule.root_lineage_id,
        b.selection_rule.root_lineage_id
    );
    assert_eq!(count(&pool, "evaluation_policies").await, 2);
    assert_eq!(count(&pool, "experiment_families").await, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_disable_and_policy_reconsumption_cannot_reuse_a_stale_preflight(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    let input = store
        .create_input_set(&actor, "input", &f.input(InputPurpose::Validation))
        .await
        .unwrap()
        .resource
        .header
        .id;
    let request = f.policy(input);
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("UPDATE app.runtime_integrations SET enabled=false WHERE id=$1")
        .bind(f.runtime.as_uuid())
        .execute(&mut *blocker)
        .await
        .unwrap();
    let (s, a, r) = (store.clone(), actor.clone(), request);
    let task =
        tokio::spawn(async move { s.create_evaluation_policy(&a, "stale-runtime", &r).await });
    wait_for_query_lock(&pool, "SELECT id,enabled FROM app.runtime_integrations").await;
    blocker.commit().await.unwrap();
    code(task.await.unwrap().unwrap_err(), "SOURCE_DISABLED");
    assert_eq!(count(&pool, "evaluation_policies").await, 0);
    assert_eq!(count(&pool, "experiment_families").await, 0);
    sqlx::query("UPDATE app.runtime_integrations SET enabled=true WHERE id=$1")
        .bind(f.runtime.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.data_use_revocations(grant_id,effective_at,reason_code,reason) VALUES($1,clock_timestamp(),'TEST','revoked')").bind(f.grant.as_uuid()).execute(&mut *blocker).await.unwrap();
    let (s, a, r) = (store.clone(), actor.clone(), f.policy(input));
    let task = tokio::spawn(async move { s.create_evaluation_policy(&a, "stale-grant", &r).await });
    wait_for_query_lock(
        &pool,
        "SELECT id,source_id,valid_from,valid_until FROM app.data_use_grants",
    )
    .await;
    blocker.commit().await.unwrap();
    code(task.await.unwrap().unwrap_err(), "DATA_USE_NOT_AUTHORIZED");
    assert_eq!(
        count(&pool, "input_sets").await,
        1,
        "do not erase old preparation history"
    );
    assert_eq!(count(&pool, "evaluation_policies").await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn zero_argument_revision_guard_preserves_native_identity_and_binding_checks(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    // Execute the actual unchanged original function to demonstrate the prior bug,
    // then reinstall only the additive repair. No migration history is edited.
    let original = include_str!("../../../migrations/202609050001_domain.sql");
    let original = original
        .split("CREATE FUNCTION app.guard_revision()")
        .nth(1)
        .unwrap()
        .split("CREATE FUNCTION app.guard_frozen_parent()")
        .next()
        .unwrap();
    sqlx::raw_sql(&format!(
        "CREATE OR REPLACE FUNCTION app.guard_revision(){original}"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let err = sqlx::query("UPDATE app.runtime_integrations SET enabled=false WHERE id=$1")
        .bind(f.runtime.as_uuid())
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        err.as_database_error().unwrap().code().as_deref(),
        Some("22004")
    );
    let repair = include_str!("../../../migrations/202609060012_research_contracts.sql")
        .split("CREATE OR REPLACE FUNCTION app.guard_revision()")
        .nth(1)
        .unwrap();
    sqlx::raw_sql(&format!(
        "CREATE OR REPLACE FUNCTION app.guard_revision(){repair}"
    ))
    .execute(&pool)
    .await
    .unwrap();
    let revision: i64 = sqlx::query_scalar(
        "UPDATE app.runtime_integrations SET enabled=false WHERE id=$1 RETURNING revision",
    )
    .bind(f.runtime.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revision, 2);
    let downstream = contracts::Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'fixture','https://fixture.invalid','not-secret','{}','PAPER',true)").bind(downstream.as_uuid()).execute(&pool).await.unwrap();
    let revision: i64 = sqlx::query_scalar(
        "UPDATE app.downstream_integrations SET enabled=false WHERE id=$1 RETURNING revision",
    )
    .bind(downstream.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(revision, 2);
    for (sql,id) in [("UPDATE app.runtime_integrations SET id=uuidv7() WHERE id=$1",f.runtime),
        ("UPDATE app.downstream_integrations SET created_at=created_at-interval '1 day' WHERE id=$1",downstream),
        ("UPDATE app.data_sources SET native_catalog_ref='changed' WHERE id=$1",f.source)] {
        let err=sqlx::query(sql).bind(id.as_uuid()).execute(&pool).await.unwrap_err();assert_eq!(err.as_database_error().unwrap().code().as_deref(),Some("23000"));
    }
}
