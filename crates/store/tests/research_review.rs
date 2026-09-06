//! Independent review regressions against real PostgreSQL. No market or PASS evidence.
#[path = "../../../tests/support/research.rs"]
mod support;
use contracts::{research::*, Id};
use serde_json::json;
use sqlx::{PgConnection, PgPool};
use store::{authority::Actor, Store, StoreError};
use support::*;

async fn policy_fixture(pool: &PgPool) -> (Store, Actor, ResearchFixture, EvaluationPolicyView) {
    let (store, actor) = operator(pool).await;
    let f = setup(pool, &store, &actor).await;
    let input = store
        .create_input_set(&actor, "input", &f.input(InputPurpose::Validation))
        .await
        .unwrap()
        .resource
        .header
        .id;
    let policy = store
        .create_evaluation_policy(&actor, "policy", &f.policy(input))
        .await
        .unwrap()
        .resource;
    (store, actor, f, policy)
}
async fn clone_policy(c: &mut PgConnection, base: Id, id: Id, family: Id, root: Id) {
    sqlx::query("INSERT INTO app.evaluation_policies(id,project_id,version,selection_rule,split_policy,metric_requirements,minimum_observations,maximum_missing_fraction,require_real_data,required_capabilities,maximum_sealed_uses_per_lineage,validity_seconds) SELECT $1,project_id,version+1,selection_rule||$3,split_policy,metric_requirements,minimum_observations,maximum_missing_fraction,require_real_data,required_capabilities,maximum_sealed_uses_per_lineage,validity_seconds FROM app.evaluation_policies WHERE id=$2")
        .bind(id.as_uuid()).bind(base.as_uuid()).bind(json!({"family_id":family,"root_lineage_id":root})).execute(c).await.unwrap();
}
async fn insert_family(
    c: &mut PgConnection,
    f: &ResearchFixture,
    policy: Id,
    family: Id,
    root: Id,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.experiment_families(id,project_id,root_lineage_id,question,selection_policy_id) VALUES($1,$2,$3,'independent fixture',$4)")
        .bind(family.as_uuid()).bind(f.project.as_uuid()).bind(root.as_uuid()).bind(policy.as_uuid()).execute(c).await?;
    Ok(())
}
fn sqlstate(e: sqlx::Error, expected: &str) {
    assert_eq!(
        e.as_database_error().and_then(|e| e.code()).as_deref(),
        Some(expected),
        "{e:?}"
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn research_only_grant_cannot_publish_portfolio_or_forward_inputs(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let f = setup(&pool, &store, &actor).await;
    for purpose in [InputPurpose::Portfolio, InputPurpose::Forward] {
        let request = f.input(purpose);
        let error = store
            .create_input_set(&actor, purpose.code(), &request)
            .await
            .unwrap_err();
        let StoreError::Domain(domain::DomainError::Fields(fields)) = error else {
            panic!("unexpected {error:?}")
        };
        assert_eq!(fields[0].field, "items.0.dataset_revision_id");
        assert_eq!(fields[0].code, "DATA_USE_PURPOSE_NOT_AUTHORIZED");
    }
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.input_sets),(SELECT count(*) FROM app.input_set_items),(SELECT count(*) FROM app.command_receipts WHERE operation='INPUT_SET_CREATE')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
    for purpose in [
        InputPurpose::Discovery,
        InputPurpose::Validation,
        InputPurpose::Sealed,
    ] {
        store
            .create_input_set(&actor, purpose.code(), &f.input(purpose))
            .await
            .unwrap();
    }
}
#[sqlx::test(migrations = "../../migrations")]
async fn policy_cannot_name_another_familys_identity(pool: PgPool) {
    let (_, _, f, p) = policy_fixture(&pool).await;
    let id = Id::new();
    let family = Id::new();
    let mut tx = pool.begin().await.unwrap();
    clone_policy(
        &mut tx,
        p.id,
        id,
        p.selection_rule.family_id,
        p.selection_rule.root_lineage_id,
    )
    .await;
    insert_family(&mut tx, &f, id, family, p.selection_rule.root_lineage_id)
        .await
        .unwrap();
    sqlstate(tx.commit().await.unwrap_err(), "23503");
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM app.evaluation_policies WHERE id=$1)"
    )
    .bind(id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap());
}
#[sqlx::test(migrations = "../../migrations")]
async fn immutable_policy_cannot_commit_without_its_family(pool: PgPool) {
    let (_, _, _, p) = policy_fixture(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    clone_policy(
        &mut tx,
        p.id,
        Id::new(),
        Id::new(),
        p.selection_rule.root_lineage_id,
    )
    .await;
    sqlstate(tx.commit().await.unwrap_err(), "23503");
}
#[sqlx::test(migrations = "../../migrations")]
async fn a_policy_cannot_be_borrowed_by_an_extra_family(pool: PgPool) {
    let (_, _, f, p) = policy_fixture(&pool).await;
    let mut tx = pool.begin().await.unwrap();
    insert_family(
        &mut tx,
        &f,
        p.id,
        Id::new(),
        p.selection_rule.root_lineage_id,
    )
    .await
    .unwrap();
    sqlstate(tx.commit().await.unwrap_err(), "23503");
}
#[sqlx::test(migrations = "../../migrations")]
async fn a_valid_pair_can_be_created_family_first_in_one_transaction(pool: PgPool) {
    let (_, _, f, p) = policy_fixture(&pool).await;
    let id = Id::new();
    let family = Id::new();
    let mut tx = pool.begin().await.unwrap();
    insert_family(&mut tx, &f, id, family, p.selection_rule.root_lineage_id)
        .await
        .unwrap();
    clone_policy(&mut tx, p.id, id, family, p.selection_rule.root_lineage_id).await;
    tx.commit().await.unwrap();
}
#[sqlx::test(migrations = "../../migrations")]
async fn policy_cannot_relabel_its_family_root_lineage(pool: PgPool) {
    let (_, _, f, p) = policy_fixture(&pool).await;
    let id = Id::new();
    let family = Id::new();
    let root = Id::new();
    sqlx::query("INSERT INTO app.research_lineages(id,origin,reason) VALUES($1,'NEW','separate fixture root')").bind(root.as_uuid()).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    clone_policy(&mut tx, p.id, id, family, root).await;
    insert_family(&mut tx, &f, id, family, p.selection_rule.root_lineage_id)
        .await
        .unwrap();
    sqlstate(tx.commit().await.unwrap_err(), "23503");
}

#[sqlx::test(migrations = "../../migrations")]
async fn higher_grant_levels_allow_preparation_but_keep_revocations_authoritative(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    for allowed in [DataUse::ResearchAndPaper, DataUse::ResearchPaperLive] {
        let f = setup_with_use(&pool, &store, &actor, allowed).await;
        for purpose in [
            InputPurpose::Discovery,
            InputPurpose::Validation,
            InputPurpose::Sealed,
            InputPurpose::Portfolio,
            InputPurpose::Forward,
        ] {
            store
                .create_input_set(&actor, &Id::new().to_string(), &f.input(purpose))
                .await
                .unwrap();
        }
        sqlx::query("INSERT INTO app.data_use_revocations(grant_id,effective_at,reason_code,reason) VALUES($1,clock_timestamp(),'TEST','native fixture revocation')").bind(f.grant.as_uuid()).execute(&pool).await.unwrap();
        let StoreError::Domain(domain::DomainError::Fields(fields)) = store
            .create_input_set(
                &actor,
                &Id::new().to_string(),
                &f.input(InputPurpose::Forward),
            )
            .await
            .unwrap_err()
        else {
            panic!("not a field refusal")
        };
        assert_eq!(fields[0].code, "DATA_USE_NOT_AUTHORIZED");
    }
}
#[sqlx::test(migrations = "../../migrations")]
async fn generated_binding_columns_cannot_be_overridden(pool: PgPool) {
    let (_, _, _, p) = policy_fixture(&pool).await;
    let family: uuid::Uuid =
        sqlx::query_scalar("SELECT family_id FROM app.evaluation_policies WHERE id=$1")
            .bind(p.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(family, p.selection_rule.family_id.as_uuid());
    sqlstate(sqlx::query("INSERT INTO app.evaluation_policies(id,project_id,version,selection_rule,split_policy,metric_requirements,minimum_observations,maximum_missing_fraction,require_real_data,required_capabilities,maximum_sealed_uses_per_lineage,validity_seconds,family_id) SELECT $1,project_id,version+1,selection_rule,split_policy,metric_requirements,minimum_observations,maximum_missing_fraction,require_real_data,required_capabilities,maximum_sealed_uses_per_lineage,validity_seconds,$2 FROM app.evaluation_policies WHERE id=$3")
        .bind(Id::new().as_uuid()).bind(Id::new().as_uuid()).bind(p.id.as_uuid()).execute(&pool).await.unwrap_err(),"428C9");
}

async fn old_schema(pool: &PgPool) {
    let mut migrations = sqlx::migrate!("../../migrations");
    migrations.migrations = migrations
        .iter()
        .filter(|m| m.version <= 202609060013)
        .cloned()
        .collect::<Vec<_>>()
        .into();
    migrations.run(pool).await.unwrap();
}
// Raw historical facts for upgrade tests, not a production writer or license bypass.
async fn old_pair(pool: &PgPool, wrong: bool) -> Id {
    let (store, actor) = operator(pool).await;
    let f = setup(pool, &store, &actor).await;
    let root: uuid::Uuid =
        sqlx::query_scalar("SELECT root_lineage_id FROM app.projects WHERE id=$1")
            .bind(f.project.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    let policy = Id::new();
    let family = Id::new();
    let named = if wrong { Id::new() } else { family };
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.evaluation_policies(id,project_id,version,selection_rule,split_policy,metric_requirements,minimum_observations,maximum_missing_fraction,require_real_data,required_capabilities,maximum_sealed_uses_per_lineage,validity_seconds) VALUES($1,$2,1,$3,'{\"schema_version\":1}','[]',1,0,true,'{}',1,3600)")
        .bind(policy.as_uuid()).bind(f.project.as_uuid()).bind(json!({"schema_version":1,"family_id":named,"root_lineage_id":root})).execute(&mut *tx).await.unwrap();
    insert_family(
        &mut tx,
        &f,
        policy,
        family,
        Id::try_from(root.to_string()).unwrap(),
    )
    .await
    .unwrap();
    tx.commit().await.unwrap();
    policy
}
#[sqlx::test(migrations = false)]
async fn native_upgrade_preserves_valid_existing_policy_facts_and_adds_deferred_constraints(
    pool: PgPool,
) {
    old_schema(&pool).await;
    let id = old_pair(&pool, false).await;
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM app.evaluation_policies p WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    Store::from_pool(pool.clone()).migrate().await.unwrap();
    let after:serde_json::Value=sqlx::query_scalar("SELECT to_jsonb(p)-'family_id'-'root_lineage_id' FROM app.evaluation_policies p WHERE id=$1").bind(id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    let constraints:Vec<(bool,bool)>=sqlx::query_as("SELECT condeferrable,condeferred FROM pg_constraint WHERE conname IN ('policy_exact_family','family_exact_policy') AND connamespace='app'::regnamespace").fetch_all(&pool).await.unwrap();
    assert_eq!(constraints, vec![(true, true), (true, true)]);
    Store::from_pool(pool.clone()).migrate().await.unwrap();
}
#[sqlx::test(migrations = false)]
async fn invalid_historical_pair_aborts_upgrade_without_rewriting_evidence(pool: PgPool) {
    old_schema(&pool).await;
    let id = old_pair(&pool, true).await;
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM app.evaluation_policies p WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let error = Store::from_pool(pool.clone()).migrate().await.unwrap_err();
    let StoreError::Migration(sqlx::migrate::MigrateError::ExecuteMigration(e, version)) = error
    else {
        panic!("unexpected upgrade failure {error:?}")
    };
    assert_eq!(version, 202609060015);
    sqlstate(e, "23503");
    let after: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(p) FROM app.evaluation_policies p WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=202609060015)"
    )
    .fetch_one(&pool)
    .await
    .unwrap());
}
