//! Real PostgreSQL parent-lock, version and authorization boundaries.
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
use chrono::{Duration, Utc};
use contracts::{brief::*, control::*, research::DataPartition, Id, SchemaV1};
use serde_json::json;
use sqlx::PgPool;
use store::{authority::Actor, Store, StoreError};
async fn setup(
    pool: &PgPool,
) -> (
    Store,
    Actor,
    research_support::ResearchFixture,
    BriefCreateIntent,
) {
    let (store, actor) = research_support::operator(pool).await;
    let data = research_support::setup(pool, &store, &actor).await;
    let request = brief_support::request(&store, &actor, &data).await;
    (store, actor, data, request)
}
fn update(view: &BriefView) -> BriefUpdate {
    BriefUpdate {
        schema_version: SchemaV1,
        expected_revision: view.revision,
        content: view.content.clone(),
        bindings: view.bindings.clone(),
    }
}
fn same(a: &BriefView, b: &BriefView) {
    assert_eq!(
        serde_json::to_value(a).unwrap(),
        serde_json::to_value(b).unwrap()
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_creates_have_one_original_receipt_and_monotonic_versions(pool: PgPool) {
    let (store, actor, _, request) = setup(&pool).await;
    let (a, b) = tokio::join!(
        store.create_brief(&actor, "same", &request),
        store.create_brief(&actor, "same", &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    same(&a.resource, &b.resource);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.version, 1);
    let (a, b) = tokio::join!(
        store.create_brief(&actor, "a", &request),
        store.create_brief(&actor, "b", &request)
    );
    let mut versions = [a.unwrap().resource.version, b.unwrap().resource.version];
    versions.sort();
    assert_eq!(versions, [2, 3]);
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.research_briefs),(SELECT count(*) FROM app.command_receipts WHERE operation='BRIEF_CREATE')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (3, 3));
    let mut wrong = request.clone();
    wrong.request.content.hypothesis = "different intent".into();
    assert!(matches!(
        store.create_brief(&actor, "same", &wrong).await,
        Err(StoreError::IdempotencyConflict)
    ));
}
#[sqlx::test(migrations = "../../migrations")]
async fn compare_and_swap_replaces_draft_members_atomically_and_replays_original(pool: PgPool) {
    let (store, actor, data, request) = setup(&pool).await;
    let first = store
        .create_brief(&actor, "create", &request)
        .await
        .unwrap()
        .resource;
    let mut a = update(&first);
    a.content.hypothesis = "new a".into();
    a.bindings = vec![BriefBindingV1 {
        dataset_revision_id: data.validation,
        role: DataPartition::Validation,
        access_policy: DataAccess::ResearchRead,
    }];
    let mut b = a.clone();
    b.content.hypothesis = "new b".into();
    let (l, r) = tokio::join!(
        store.update_brief(&actor, "a", first.id, &a),
        store.update_brief(&actor, "b", first.id, &b)
    );
    assert_ne!(l.is_ok(), r.is_ok());
    let winner = if let Ok(v) = l {
        assert!(matches!(r,Err(StoreError::RevisionConflict{current}) if current.get()==2));
        v
    } else {
        assert!(matches!(l,Err(StoreError::RevisionConflict{current}) if current.get()==2));
        r.unwrap()
    };
    assert_eq!(winner.resource.bindings.len(), 1);
    assert_eq!(
        winner.resource.bindings[0].dataset_revision_id,
        data.validation
    );
    assert_eq!(winner.resource.version, 1);
    let replay = store
        .create_brief(&actor, "create", &request)
        .await
        .unwrap();
    assert!(replay.replayed);
    same(&replay.resource, &first);
    let read = store.brief(&actor, first.id).await.unwrap();
    same(&read, &winner.resource);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE operation='BRIEF_UPDATE'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}
#[sqlx::test(migrations = "../../migrations")]
async fn member_failure_rolls_back_content_revision_and_original_receipt(pool: PgPool) {
    let (store, actor, data, request) = setup(&pool).await;
    let first = store
        .create_brief(&actor, "create", &request)
        .await
        .unwrap()
        .resource;
    let mut patch = update(&first);
    patch.content.hypothesis = "must roll back".into();
    patch.bindings = vec![BriefBindingV1 {
        dataset_revision_id: data.validation,
        role: DataPartition::Validation,
        access_policy: DataAccess::MetadataOnly,
    }];
    sqlx::raw_sql("CREATE FUNCTION public.fail_brief_member() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected member write failure'; END $$; CREATE TRIGGER fail_member BEFORE INSERT ON app.brief_data_bindings FOR EACH ROW EXECUTE FUNCTION public.fail_brief_member();").execute(&pool).await.unwrap();
    assert!(matches!(
        store.update_brief(&actor, "patch", first.id, &patch).await,
        Err(StoreError::Database(_))
    ));
    same(&store.brief(&actor, first.id).await.unwrap(), &first);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE operation='BRIEF_UPDATE'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER fail_member ON app.brief_data_bindings")
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        !store
            .update_brief(&actor, "patch", first.id, &patch)
            .await
            .unwrap()
            .replayed
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn frozen_brief_rejects_all_member_and_parent_writes(pool: PgPool) {
    let (store, actor, data, request) = setup(&pool).await;
    let first = store
        .create_brief(&actor, "create", &request)
        .await
        .unwrap()
        .resource;
    sqlx::query(
        "UPDATE app.research_briefs SET state='FROZEN',frozen_at=clock_timestamp() WHERE id=$1",
    )
    .bind(first.id.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    let frozen = store.brief(&actor, first.id).await.unwrap();
    assert_eq!(frozen.state, BriefState::Frozen);
    for sql in [
        "DELETE FROM app.brief_data_bindings WHERE brief_id=$1",
        "UPDATE app.brief_data_bindings SET access_policy='METADATA_ONLY' WHERE brief_id=$1",
    ] {
        assert!(sqlx::query(sql)
            .bind(first.id.as_uuid())
            .execute(&pool)
            .await
            .is_err());
    }
    assert!(sqlx::query("INSERT INTO app.brief_data_bindings(brief_id,dataset_revision_id,role,access_policy) VALUES($1,$2,'VALIDATION','METADATA_ONLY')").bind(first.id.as_uuid()).bind(data.validation.as_uuid()).execute(&pool).await.is_err());
    assert!(store
        .update_brief(&actor, "immutable", first.id, &update(&frozen))
        .await
        .is_err());
    same(&store.brief(&actor, first.id).await.unwrap(), &frozen);
}
#[sqlx::test(migrations = "../../migrations")]
async fn references_partition_currency_and_archived_project_fail_without_orphans(pool: PgPool) {
    let (store, actor, _, request) = setup(&pool).await;
    let second = research_support::setup(&pool, &store, &actor).await;
    let other = brief_support::request(&store, &actor, &second).await;
    let original = serde_json::to_value(&request).unwrap();
    for (pointer, value) in [
        ("/request/content/universe_version_id", json!(Id::new())),
        (
            "/request/content/evaluation_policy_id",
            json!(other.request.content.evaluation_policy_id),
        ),
        (
            "/request/content/execution_assumptions_id",
            json!(second.assumptions),
        ),
        ("/request/content/benchmark_ref", json!(Id::new())),
        ("/request/content/base_currency", json!("EUR")),
        (
            "/request/bindings/0/dataset_revision_id",
            json!(second.discovery),
        ),
        ("/request/bindings/0/role", json!("VALIDATION")),
        ("/request/supersedes_id", json!(Id::new())),
    ] {
        let mut v = original.clone();
        *v.pointer_mut(pointer).unwrap() = value;
        let v: BriefCreateIntent = serde_json::from_value(v).unwrap();
        assert!(
            store
                .create_brief(&actor, &Id::new().to_string(), &v)
                .await
                .is_err(),
            "{pointer}"
        );
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.research_briefs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query(
        "UPDATE app.projects SET state='ARCHIVED',archived_at=clock_timestamp() WHERE id=$1",
    )
    .bind(request.project_id.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    assert!(store
        .create_brief(&actor, "archived", &request)
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE operation='BRIEF_CREATE'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}
#[sqlx::test(migrations = "../../migrations")]
async fn pagination_and_superseding_keep_original_identity(pool: PgPool) {
    let (store, actor, _, mut request) = setup(&pool).await;
    let first = store
        .create_brief(&actor, "first", &request)
        .await
        .unwrap()
        .resource;
    request.request.supersedes_id = Some(first.id);
    let second = store
        .create_brief(&actor, "second", &request)
        .await
        .unwrap()
        .resource;
    assert_eq!(second.version, 2);
    assert_eq!(second.supersedes_id, Some(first.id));
    let page = store
        .briefs(
            &actor,
            request.project_id,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].id, second.id);
    assert_eq!(page.next_cursor, Some(second.id));
    let tail = store
        .briefs(
            &actor,
            request.project_id,
            &ListQuery {
                cursor: page.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(tail.items[0].id, first.id);
    assert!(tail.next_cursor.is_none());
    assert!(
        sqlx::query("UPDATE app.research_briefs SET supersedes_id=NULL WHERE id=$1")
            .bind(second.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_parent_freeze_wins_over_later_member_write(pool: PgPool) {
    let (store, actor, data, request) = setup(&pool).await;
    let first = store
        .create_brief(&actor, "create", &request)
        .await
        .unwrap()
        .resource;
    let mut tx = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE app.research_briefs SET state='FROZEN',frozen_at=clock_timestamp() WHERE id=$1",
    )
    .bind(first.id.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap();
    let db = pool.clone();
    let task = tokio::spawn(async move {
        sqlx::query("INSERT INTO app.brief_data_bindings(brief_id,dataset_revision_id,role,access_policy) VALUES($1,$2,'VALIDATION','METADATA_ONLY')").bind(first.id.as_uuid()).bind(data.validation.as_uuid()).execute(&db).await
    });
    research_support::wait_for_query_lock(&pool, "INSERT INTO app.brief_data_bindings").await;
    tx.commit().await.unwrap();
    assert!(task.await.unwrap().is_err());
    assert_eq!(
        store.brief(&actor, first.id).await.unwrap().bindings.len(),
        2
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn current_machine_scope_is_read_only_and_exact_project(pool: PgPool) {
    let (store, actor, data, request) = setup(&pool).await;
    let brief = store
        .create_brief(&actor, "create", &request)
        .await
        .unwrap()
        .resource;
    let principal = store
        .create_principal(
            &actor,
            "p",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "reader".into(),
                kind: AssignablePrincipalKind::Automation,
                project_id: Some(data.project),
                downstream_id: None,
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let verifier = Id::new();
    let public = Id::new();
    let issue = CredentialIssue {
        schema_version: SchemaV1,
        scope_codes: vec![MachineScope::ResearchRead],
        expires_at: Utc::now() + Duration::hours(1),
    };
    let issued = match store
        .prepare_credential_issuance(&actor, "token", principal.id, &issue)
        .await
        .unwrap()
    {
        store::control::CredentialPreparation::New(prepared) => {
            prepared.publish(public, verifier).await.unwrap().resource
        }
        _ => panic!("new issuance required"),
    };
    let machine = store
        .machine_challenge(issued.public_token_id)
        .await
        .unwrap()
        .verified_actor(None);
    same(&store.brief(&machine, brief.id).await.unwrap(), &brief);
    assert!(matches!(
        store.create_brief(&machine, "bad", &request).await,
        Err(StoreError::Forbidden)
    ));
    let other = research_support::setup(&pool, &store, &actor).await;
    assert!(matches!(
        store
            .briefs(
                &machine,
                other.project,
                &ListQuery {
                    cursor: None,
                    limit: 10
                }
            )
            .await,
        Err(StoreError::NotFound)
    ));
    store
        .revoke_credential(
            &actor,
            "revoke",
            issued.id,
            &CredentialRevoke {
                schema_version: SchemaV1,
                reason: "done".into(),
            },
        )
        .await
        .unwrap();
    assert!(store.brief(&machine, brief.id).await.is_err());
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE operation='BRIEF_CREATE'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn real_non_owner_can_replace_only_draft_bindings_with_deployment_grants(pool: PgPool) {
    let (admin, actor, data, request) = setup(&pool).await;
    let role = format!("brief_test_{}", Id::new().to_string().replace('-', ""));
    sqlx::query(&format!("CREATE ROLE {role} LOGIN PASSWORD 'disposable-test-only' NOSUPERUSER NOCREATEDB NOCREATEROLE NOBYPASSRLS NOREPLICATION NOINHERIT")).execute(&pool).await.unwrap();
    admin
        .migrate_with_application_role(Some(&role))
        .await
        .unwrap();
    let native = PgPool::connect_with(
        pool.connect_options()
            .as_ref()
            .clone()
            .username(&role)
            .password("disposable-test-only"),
    )
    .await
    .unwrap();
    let store = Store::from_pool(native.clone());
    store.verify_runtime_role().await.unwrap();
    let created = store
        .create_brief(&actor, "nonowner-create", &request)
        .await
        .unwrap()
        .resource;
    let mut patch = update(&created);
    patch.bindings = vec![BriefBindingV1 {
        dataset_revision_id: data.validation,
        role: DataPartition::Validation,
        access_policy: DataAccess::MetadataOnly,
    }];
    let edited = store
        .update_brief(&actor, "nonowner-patch", created.id, &patch)
        .await
        .unwrap()
        .resource;
    assert_eq!(edited.revision.get(), 2);
    assert_eq!(edited.bindings.len(), 1);
    let dangerous:i64=sqlx::query_scalar("SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='app' AND c.relkind IN ('r','p') AND ((c.relname<>'brief_data_bindings' AND has_table_privilege(current_user,c.oid,'DELETE')) OR has_table_privilege(current_user,c.oid,'TRUNCATE') OR has_table_privilege(current_user,c.oid,'TRIGGER'))").fetch_one(&native).await.unwrap();
    assert_eq!(dangerous, 0);
    sqlx::query(
        "UPDATE app.research_briefs SET state='FROZEN',frozen_at=clock_timestamp() WHERE id=$1",
    )
    .bind(created.id.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    assert!(
        sqlx::query("DELETE FROM app.brief_data_bindings WHERE brief_id=$1")
            .bind(created.id.as_uuid())
            .execute(&native)
            .await
            .is_err()
    );
    native.close().await;
    sqlx::query(&format!("DROP OWNED BY {role}"))
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query(&format!("DROP ROLE {role}"))
        .execute(&pool)
        .await
        .unwrap();
}
