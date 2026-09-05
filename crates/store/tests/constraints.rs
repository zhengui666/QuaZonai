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

async fn candidate(pool: &PgPool, f: &Fixture, mandate: Id) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.portfolio_candidates(id,project_id,mandate_id,input_set_id,decision_asof,run_id,solver_status,evidence_status,diagnostics_artifact_id,current_weights_source) VALUES($1,$2,$3,$4,clock_timestamp(),$5,'OPTIMAL','VALID',$6,'NONE')")
        .bind(id.as_uuid()).bind(f.project.as_uuid()).bind(mandate.as_uuid()).bind(f.input_set.as_uuid()).bind(f.run.as_uuid()).bind(f.artifact.as_uuid()).execute(pool).await.unwrap();
    id
}
async fn portfolio(pool: &PgPool, f: &Fixture) -> (Id, Id, Id) {
    let mandate = Id::new();
    sqlx::query("INSERT INTO app.portfolio_mandates(id,project_id,version,objective,risk_measure,base_currency,capital_assumption,universe_version_id,covariance_estimator,alpha_ensemble,optimizer,constraints,rebalance_schedule,required_evaluation_policy_id,execution_assumptions_id,exposure_tolerance) SELECT $1,b.project_id,1,'MIN_RISK','VARIANCE','USD',100,b.universe_version_id,'{\"schema_version\":1}','{\"schema_version\":1}','{\"schema_version\":1}','{\"schema_version\":1}','{\"schema_version\":1}',b.evaluation_policy_id,b.execution_assumptions_id,0.00001 FROM app.research_briefs b WHERE b.project_id=$2")
        .bind(mandate.as_uuid()).bind(f.project.as_uuid()).execute(pool).await.unwrap();
    let candidate = candidate(pool, f, mandate).await;
    let evaluation = Id::new();
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,$2,$3,$4,b.evaluation_policy_id,$5,'PORTFOLIO','SUCCEEDED','VALID','PASS',$6,$6,clock_timestamp(),clock_timestamp()+interval '1 hour' FROM app.research_briefs b WHERE b.project_id=$2")
        .bind(evaluation.as_uuid()).bind(f.project.as_uuid()).bind(candidate.as_uuid()).bind(f.input_set.as_uuid()).bind(f.run.as_uuid()).bind(f.artifact.as_uuid()).execute(pool).await.unwrap();
    (mandate, candidate, evaluation)
}

async fn release(
    pool: &PgPool,
    f: &Fixture,
    mandate: Id,
    candidate: Id,
    evaluation: Id,
) -> Result<Id, sqlx::Error> {
    let id = Id::new();
    sqlx::query("INSERT INTO app.releases(id,candidate_id,package_artifact_id,package_schema_version,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,environment) VALUES($1,$2,$3,'fixture',$4,$5,'fixture',now(),now(),now()+interval '1 hour','DEMO')")
        .bind(id.as_uuid()).bind(candidate.as_uuid()).bind(f.artifact.as_uuid()).bind(mandate.as_uuid()).bind(evaluation.as_uuid()).execute(pool).await?;
    Ok(id)
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
    let approval = Id::new();
    sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) VALUES($1,$2,'PAPER',$3,'OPERATOR',$4,now(),now()+interval '1 hour')").bind(approval.as_uuid()).bind(r.as_uuid()).bind(d.as_uuid()).bind(f.input_set.as_uuid()).execute(&pool).await.unwrap();
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
    assert_eq!(tables, 65);
    let migration_count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(migration_count, 3);
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
    let grant = Id::new();
    sqlx::query("INSERT INTO app.operator_command_grants(id,credential_id,operation,target_id,auth_epoch,authenticated_at,expires_at) VALUES($1,$2,'RELEASE_APPROVE',$3,1,now(),now()+interval '5 minutes')")
        .bind(grant.as_uuid()).bind(credential.as_uuid()).bind(f.run.as_uuid()).execute(&pool).await.unwrap();
    let receipt = Id::new();
    let other_receipt = Id::new();
    for (receipt_id, resource_id) in [(receipt, f.run), (other_receipt, f.project)] {
        sqlx::query("INSERT INTO app.command_receipts(id,principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status) VALUES($1,'test-cli','RELEASE_APPROVE',$2,'{\"schema_version\":1}',$3,201)")
            .bind(receipt_id.as_uuid()).bind(receipt_id.to_string()).bind(resource_id.as_uuid()).execute(&pool).await.unwrap();
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
