//! Real PostgreSQL negative cases verify SQLSTATE, not merely "some error".
//! Fixtures bypass service policy only inside ephemeral sqlx-created databases.
mod support;
use contracts::Id;
use sqlx::PgPool;
use support::*;

fn sqlstate(error: sqlx::Error, expected: &str) {
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some(expected),
        "{error:?}"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_scalar_domains_do_not_round_or_accept_unknown_uuid_variants(pool: PgPool) {
    for value in [
        "99999999999999999999.999999999999999999",
        "-99999999999999999999.999999999999999999",
        "0.000000000000000001",
    ] {
        let returned: String = sqlx::query_scalar("SELECT ($1::text::app.decimal_value)::text")
            .bind(value)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(returned, value);
    }
    for value in [
        "100000000000000000000",
        "0.0000000000000000001",
        "NaN",
        "Infinity",
        "-Infinity",
    ] {
        sqlstate(
            sqlx::query("SELECT $1::text::app.decimal_value")
                .bind(value)
                .execute(&pool)
                .await
                .unwrap_err(),
            "23514",
        );
    }
    for value in [
        "00000000-0000-4000-8000-000000000000",
        "00000000-0000-7000-0000-000000000000",
    ] {
        sqlstate(
            sqlx::query("SELECT $1::text::app.identity")
                .bind(value)
                .execute(&pool)
                .await
                .unwrap_err(),
            "23514",
        );
    }
    sqlx::query("SELECT $1::uuid::app.identity")
        .bind(Id::new().as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("SELECT NULL::app.identity")
        .execute(&pool)
        .await
        .unwrap();
    for value in ["0", "65535"] {
        sqlx::query("SELECT $1::text::app.uint16")
            .bind(value)
            .execute(&pool)
            .await
            .unwrap();
    }
    sqlstate(
        sqlx::query("SELECT 65536::app.uint16")
            .execute(&pool)
            .await
            .unwrap_err(),
        "23514",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn artifact_storage_identity_cannot_be_relabelled_as_real_or_public(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let query="INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) SELECT uuidv7(),project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,'DELIVERY','REAL',created_by,retention_class FROM app.artifacts WHERE id=$1";
    sqlstate(
        sqlx::query(query)
            .bind(f.artifact.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23505",
    );
    sqlstate(
        sqlx::query("UPDATE app.artifacts SET origin='REAL' WHERE id=$1")
            .bind(f.artifact.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23000",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn input_order_is_unique_and_publication_freezes_members(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let input = Id::new();
    sqlx::query("INSERT INTO app.input_sets(id,project_id,purpose,decision_cutoff) VALUES($1,$2,'DISCOVERY',clock_timestamp())").bind(input.as_uuid()).bind(f.project.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO app.input_set_items(input_set_id,artifact_id,role,ordinal) VALUES($1,$2,'PARAMETERS',0)").bind(input.as_uuid()).bind(f.artifact.as_uuid()).execute(&pool).await.unwrap();
    for ordinal in [0, 1] {
        sqlstate(sqlx::query("INSERT INTO app.input_set_items(input_set_id,artifact_id,role,ordinal) VALUES($1,$2,'PARAMETERS',$3)").bind(input.as_uuid()).bind(f.artifact.as_uuid()).bind(ordinal).execute(&pool).await.unwrap_err(),"23505");
    }
    sqlx::query("UPDATE app.input_sets SET frozen_at=clock_timestamp() WHERE id=$1")
        .bind(input.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlstate(sqlx::query("INSERT INTO app.input_set_items(input_set_id,artifact_id,role,ordinal) VALUES($1,$2,'PARAMETERS',2)").bind(input.as_uuid()).bind(f.artifact.as_uuid()).execute(&pool).await.unwrap_err(),"23000");
    sqlstate(
        sqlx::query("UPDATE app.input_sets SET frozen_at=NULL WHERE id=$1")
            .bind(input.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23000",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_cannot_borrow_another_candidates_pass_evaluation(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (m, a, eval) = portfolio(&pool, &f).await;
    let b = candidate(&pool, &f, m).await;
    sqlstate(release(&pool, &f, m, b, eval).await.unwrap_err(), "23503");
    release(&pool, &f, m, a, eval).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn paper_approval_cannot_authorize_live_or_another_downstream(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (m, c, e) = portfolio(&pool, &f).await;
    let r = release(&pool, &f, m, c, e).await.unwrap();
    let d = Id::new();
    let other = Id::new();
    for id in [d, other] {
        sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'fixture','https://example.invalid','fixture','{fixture}','BOTH',true)").bind(id.as_uuid()).execute(&pool).await.unwrap();
    }
    let evidence = support::approval_inputs(&pool, &f, e).await;
    let approval = Id::new();
    sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) VALUES($1,$2,'PAPER',$3,'OPERATOR',$4,now(),now()+interval '1 hour')").bind(approval.as_uuid()).bind(r.as_uuid()).bind(d.as_uuid()).bind(evidence.as_uuid()).execute(&pool).await.unwrap();
    let query="INSERT INTO app.handoff_offers(release_id,approval_id,downstream_id,environment,delivery_sequence,state,offered_at,expires_at) VALUES($1,$2,$3,$4,1,'OFFERED',now(),now()+interval '1 hour')";
    for (target, environment) in [(d, "LIVE"), (other, "PAPER")] {
        sqlstate(
            sqlx::query(query)
                .bind(r.as_uuid())
                .bind(approval.as_uuid())
                .bind(target.as_uuid())
                .bind(environment)
                .execute(&pool)
                .await
                .unwrap_err(),
            "23503",
        );
    }
    sqlx::query(query)
        .bind(r.as_uuid())
        .bind(approval.as_uuid())
        .bind(d.as_uuid())
        .bind("PAPER")
        .execute(&pool)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn new_database_migrations_are_repeatable_without_legacy_side_effects(pool: PgPool) {
    let store = store::Store::from_pool(pool.clone());
    store.migrate().await.unwrap();
    let tables:i64=sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema='app' AND table_type='BASE TABLE'").fetch_one(&pool).await.unwrap();
    assert_eq!(tables, 70);
    let windows_exist: bool =
        sqlx::query_scalar("SELECT to_regclass('app.machine_auth_rate_windows') IS NOT NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(windows_exist);
    let applied: Vec<i64> =
        sqlx::query_scalar("SELECT version FROM _sqlx_migrations WHERE success ORDER BY version")
            .fetch_all(&pool)
            .await
            .unwrap();
    let expected: Vec<_> = sqlx::migrate!("../../migrations")
        .iter()
        .map(|migration| migration.version)
        .collect();
    assert_eq!(applied, expected);
}

#[sqlx::test(migrations = "../../migrations")]
async fn operator_grant_consumption_is_once_and_bound_to_its_command_target(pool: PgPool) {
    // Only relational authority is tested here. A real authenticator must issue
    // grants after TOTP; constructing a test row is not authentication evidence.
    let f = fixture(&pool, budget()).await;
    let principal = Id::new();
    let credential = Id::new();
    sqlx::query("INSERT INTO app.machine_principals(id,name,kind,project_id,enabled,credential_epoch) VALUES($1,'test CLI','CLI',$2,true,1)")
        .bind(principal.as_uuid()).bind(f.project.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by) VALUES($1,$2,'fixture-public-id','fixture-verifier',1,'{RESEARCH_READ}',now(),now()+interval '1 hour','OPERATOR')")
        .bind(credential.as_uuid()).bind(principal.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("UPDATE app.operator_auth_state SET initialized=true,totp_secret_ref='fixture',setup_completed_at=now() WHERE singleton").execute(&pool).await.unwrap();
    let grant = Id::new();
    sqlx::query("INSERT INTO app.operator_command_grants(id,credential_id,operation,target_id,auth_epoch,authenticated_at,expires_at,normalized_nonsecret_request) VALUES($1,$2,'RELEASE_APPROVE',$3,1,now(),now()+interval '5 minutes','{\"schema_version\":1}')")
        .bind(grant.as_uuid()).bind(credential.as_uuid()).bind(f.run.as_uuid()).execute(&pool).await.unwrap();
    let receipt = Id::new();
    let other_receipt = Id::new();
    for (receipt_id, resource_id) in [(receipt, f.run), (other_receipt, f.project)] {
        sqlx::query("INSERT INTO app.command_receipts(id,principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status) VALUES($1,$4,'RELEASE_APPROVE',$2,'{\"schema_version\":1}',$3,201)")
            .bind(receipt_id.as_uuid()).bind(receipt_id.to_string()).bind(resource_id.as_uuid()).bind(format!("CREDENTIAL:{credential}")).execute(&pool).await.unwrap();
    }
    let insert = "INSERT INTO app.operator_command_consumptions(grant_id,command_receipt_id,operation,target_id) VALUES($1,$2,'RELEASE_APPROVE',$3)";
    for target in [f.run, f.project] {
        sqlstate(
            sqlx::query(insert)
                .bind(grant.as_uuid())
                .bind(other_receipt.as_uuid())
                .bind(target.as_uuid())
                .execute(&pool)
                .await
                .unwrap_err(),
            "23503",
        );
    }
    sqlx::query(insert)
        .bind(grant.as_uuid())
        .bind(receipt.as_uuid())
        .bind(f.run.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlstate(
        sqlx::query(insert)
            .bind(grant.as_uuid())
            .bind(receipt.as_uuid())
            .bind(f.run.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23505",
    );
    sqlstate(
        sqlx::query("DELETE FROM app.operator_command_consumptions WHERE grant_id=$1")
            .bind(grant.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23000",
    );
}
