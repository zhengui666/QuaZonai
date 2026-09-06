//! Immutable relational provenance. Fixtures remain FIXTURE, never release authorization.
mod support;
use contracts::Id;
use sqlx::{PgConnection, PgPool};
use support::*;

async fn copy_report(
    pool: &PgPool,
    f: &Fixture,
    project: Option<Id>,
    run: Option<Id>,
    kind: &str,
) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) SELECT $1,$2,$3,$4,media_type,schema_name,schema_version,storage_backend,$5,storage_version,byte_count,access_class,origin,created_by,retention_class FROM app.artifacts WHERE id=$6")
        .bind(id.as_uuid()).bind(project.map(Id::as_uuid)).bind(run.map(Id::as_uuid)).bind(kind)
        .bind(format!("fixture/{id}")).bind(f.report.as_uuid()).execute(pool).await.unwrap();
    id
}
async fn evaluation(
    c: &mut PgConnection,
    source: Id,
    report: Id,
    methods: Id,
) -> Result<Id, sqlx::Error> {
    let id = Id::new();
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,$3,$4,concluded_at,valid_until FROM app.evaluations WHERE id=$2")
        .bind(id.as_uuid()).bind(source.as_uuid()).bind(report.as_uuid()).bind(methods.as_uuid()).execute(c).await?;
    Ok(id)
}
async fn inputs(pool: &PgPool, project: Id, purpose: &str, frozen: bool, artifacts: &[Id]) -> Id {
    let id = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.input_sets(id,project_id,purpose,decision_cutoff) VALUES($1,$2,$3,clock_timestamp())")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(purpose).execute(&mut *tx).await.unwrap();
    for (index, artifact) in artifacts.iter().enumerate() {
        sqlx::query("INSERT INTO app.input_set_items(input_set_id,artifact_id,role,ordinal) VALUES($1,$2,'REPORT',$3)")
            .bind(id.as_uuid()).bind(artifact.as_uuid()).bind(index as i32).execute(&mut *tx).await.unwrap();
    }
    if frozen {
        sqlx::query("UPDATE app.input_sets SET frozen_at=clock_timestamp() WHERE id=$1")
            .bind(id.as_uuid())
            .execute(&mut *tx)
            .await
            .unwrap();
    }
    tx.commit().await.unwrap();
    id
}
async fn approval(
    pool: &PgPool,
    release: Id,
    downstream: Id,
    inputs: Id,
) -> Result<Id, sqlx::Error> {
    let id = Id::new();
    sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) VALUES($1,$2,'PAPER',$3,'OPERATOR',$4,clock_timestamp(),clock_timestamp()+interval '1 hour')")
        .bind(id.as_uuid()).bind(release.as_uuid()).bind(downstream.as_uuid()).bind(inputs.as_uuid()).execute(pool).await?;
    Ok(id)
}

#[sqlx::test(migrations = "../../migrations")]
async fn evaluation_reports_must_come_from_the_exact_project_run_and_role(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let other = fixture(&pool, budget()).await;
    let (_, _, base) = portfolio(&pool, &f).await;
    let (other_run, _, _, _) = mission(&pool, f.project, f.cycle, f.input_set, f.profile).await;
    let wrong_run = copy_report(&pool, &f, Some(f.project), Some(other_run), "REPORT").await;
    let missing_run = copy_report(&pool, &f, Some(f.project), None, "REPORT").await;
    let no_project = copy_report(&pool, &f, None, None, "REPORT").await;
    let wrong_kind = copy_report(&pool, &f, Some(f.project), Some(f.run), "LOG").await;
    let mut c = pool.acquire().await.unwrap();
    for wrong in [
        other.report,
        wrong_run,
        missing_run,
        no_project,
        wrong_kind,
        f.artifact,
    ] {
        sqlstate(
            evaluation(&mut c, base, wrong, f.report).await.unwrap_err(),
            "23503",
        );
        sqlstate(
            evaluation(&mut c, base, f.report, wrong).await.unwrap_err(),
            "23503",
        );
    }
    let second = copy_report(&pool, &f, Some(f.project), Some(f.run), "REPORT").await;
    evaluation(&mut c, base, f.report, second).await.unwrap();
    evaluation(&mut c, base, f.report, f.report).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.evaluations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        3
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn metric_source_cannot_lend_unrelated_evidence_to_a_valid_report(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let other = fixture(&pool, budget()).await;
    let (_, _, base) = portfolio(&pool, &f).await;
    let metric_sql="INSERT INTO app.metric_values(evaluation_id,metric_code,scope,value,status,unit,period_start,period_end,observation_count,frequency,method_id,method_version,source_artifact_id) VALUES($1,'mean','total',0.1,'OK','ratio',now()-interval '1 hour',now(),30,'DAY','fixture','1',$2)";
    for bad in [other.report, f.artifact] {
        let mut tx = pool.begin().await.unwrap();
        let e = evaluation(&mut tx, base, f.report, f.report).await.unwrap();
        sqlstate(
            sqlx::query(metric_sql)
                .bind(e.as_uuid())
                .bind(bad.as_uuid())
                .execute(&mut *tx)
                .await
                .unwrap_err(),
            "23503",
        );
        tx.rollback().await.unwrap();
    }
    for kind in ["REPORT", "METRICS"] {
        let source = copy_report(&pool, &f, Some(f.project), Some(f.run), kind).await;
        let mut tx = pool.begin().await.unwrap();
        let e = evaluation(&mut tx, base, f.report, f.report).await.unwrap();
        sqlx::query(metric_sql)
            .bind(e.as_uuid())
            .bind(source.as_uuid())
            .execute(&mut *tx)
            .await
            .unwrap();
        tx.commit().await.unwrap();
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.metric_values")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn approvals_require_frozen_release_project_context_with_the_exact_reports(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let other = fixture(&pool, budget()).await;
    let (m, c, e) = portfolio(&pool, &f).await;
    let r = support::delivery_release_metadata(&pool, &f, m, c, e)
        .await
        .unwrap();
    let downstream = Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'fixture','https://example.invalid','fixture','{fixture}','BOTH',true)")
        .bind(downstream.as_uuid()).execute(&pool).await.unwrap();
    let unrelated = copy_report(&pool, &f, Some(f.project), Some(f.run), "REPORT").await;
    for invalid in [
        inputs(&pool, other.project, "PORTFOLIO", true, &[other.report]).await,
        inputs(&pool, f.project, "PORTFOLIO", false, &[f.report]).await,
        inputs(&pool, f.project, "DISCOVERY", true, &[f.report]).await,
        inputs(&pool, f.project, "PORTFOLIO", true, &[]).await,
        inputs(&pool, f.project, "FORWARD", true, &[unrelated]).await,
    ] {
        sqlstate(
            approval(&pool, r, downstream, invalid).await.unwrap_err(),
            "23503",
        );
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.approvals")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    for purpose in ["PORTFOLIO", "FORWARD"] {
        let valid = inputs(&pool, f.project, purpose, true, &[f.report]).await;
        approval(&pool, r, downstream, valid).await.unwrap();
    }
}

#[sqlx::test(migrations = false)]
async fn upgrade_refuses_invalid_original_artifacts_without_rewriting_history(pool: PgPool) {
    migrate_before(&pool, 202609060010).await;
    let f = fixture(&pool, budget()).await;
    let (_, _, base) = portfolio(&pool, &f).await;
    let mut c = pool.acquire().await.unwrap();
    let invalid = evaluation(&mut c, base, f.artifact, f.report)
        .await
        .unwrap();
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(e) FROM app.evaluations e WHERE id=$1")
            .bind(invalid.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(store::Store::from_pool(pool.clone())
        .migrate()
        .await
        .is_err());
    let after: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(e) FROM app.evaluations e WHERE id=$1")
            .bind(invalid.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=202609060010)"
    )
    .fetch_one(&pool)
    .await
    .unwrap());
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT to_regprocedure('app.guard_evaluation_artifacts()') IS NULL"
    )
    .fetch_one(&pool)
    .await
    .unwrap());
}
