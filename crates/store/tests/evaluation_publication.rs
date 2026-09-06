//! Completed evaluations publish their exact metric membership in one transaction.
mod support;
use contracts::Id;
use sqlx::{PgConnection, PgPool};
use support::*;

fn sqlstate(error: sqlx::Error, expected: &str) {
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some(expected),
        "{error:?}"
    );
}

async fn copy_evaluation(
    connection: &mut PgConnection,
    source: Id,
    execution: &str,
    evidence: &str,
    decision: &str,
) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,$3,$4,$5,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until FROM app.evaluations WHERE id=$2")
        .bind(id.as_uuid()).bind(source.as_uuid()).bind(execution).bind(evidence).bind(decision).execute(connection).await.unwrap();
    id
}

async fn metric(
    connection: &mut PgConnection,
    evaluation: Id,
    artifact: Id,
    code: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.metric_values(evaluation_id,metric_code,scope,value,status,unit,period_start,period_end,observation_count,frequency,method_id,method_version,source_artifact_id) VALUES($1,$2,'total',0.1,'OK','ratio',statement_timestamp()-interval '1 hour',statement_timestamp(),100,'DAY','fixture','1',$3)")
        .bind(evaluation.as_uuid()).bind(code).bind(artifact.as_uuid()).execute(connection).await?;
    Ok(())
}

async fn count(pool: &PgPool, evaluation: Id) -> i64 {
    sqlx::query_scalar("SELECT count(*) FROM app.metric_values WHERE evaluation_id=$1")
        .bind(evaluation.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn evaluation_and_metrics_publish_atomically_and_cannot_gain_late_evidence(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (_, _, base) = portfolio(&pool, &f).await;
    let mut tx = pool.begin().await.unwrap();
    let evaluation = copy_evaluation(&mut tx, base, "SUCCEEDED", "VALID", "PASS").await;
    metric(&mut tx, evaluation, f.artifact, "metric_a")
        .await
        .unwrap();
    metric(&mut tx, evaluation, f.artifact, "metric_b")
        .await
        .unwrap();
    assert_eq!(
        count(&pool, evaluation).await,
        0,
        "uncommitted metrics are not visible to another connection"
    );
    tx.commit().await.unwrap();
    let mut connection = pool.acquire().await.unwrap();
    sqlstate(
        metric(&mut connection, evaluation, f.artifact, "late")
            .await
            .unwrap_err(),
        "23000",
    );
    assert_eq!(count(&pool, evaluation).await, 2);
    for sql in [
        "DELETE FROM app.evaluation_publications WHERE evaluation_id=$1",
        "UPDATE app.evaluation_publications SET evaluation_id=evaluation_id WHERE evaluation_id=$1",
        "UPDATE app.metric_values SET value=1.0 WHERE evaluation_id=$1",
        "DELETE FROM app.metric_values WHERE evaluation_id=$1",
    ] {
        sqlstate(
            sqlx::query(sql)
                .bind(evaluation.as_uuid())
                .execute(&pool)
                .await
                .unwrap_err(),
            "23000",
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn all_completed_statuses_are_sealed_including_empty_or_failed_evaluations(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (_, _, base) = portfolio(&pool, &f).await;
    let mut connection = pool.acquire().await.unwrap();
    for (execution, evidence, decision) in [
        ("SUCCEEDED", "VALID", "PASS"),
        ("SUCCEEDED", "INVALID", "REJECT"),
        ("SUCCEEDED", "INCOMPLETE", "INCONCLUSIVE"),
        ("FAILED", "UNSUPPORTED", "INCONCLUSIVE"),
        ("CANCELLED", "INCOMPLETE", "INCONCLUSIVE"),
    ] {
        let evaluation =
            copy_evaluation(&mut connection, base, execution, evidence, decision).await;
        sqlstate(
            metric(&mut connection, evaluation, f.artifact, "late")
                .await
                .unwrap_err(),
            "23000",
        );
        assert_eq!(count(&pool, evaluation).await, 0);
    }
    sqlstate(
        metric(&mut connection, Id::new(), f.artifact, "missing")
            .await
            .unwrap_err(),
        "23503",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn consuming_an_evaluation_seals_its_metrics_before_transaction_commit(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (_, _, base) = portfolio(&pool, &f).await;
    let mut tx = pool.begin().await.unwrap();
    let evaluation = copy_evaluation(&mut tx, base, "SUCCEEDED", "VALID", "PASS").await;
    metric(&mut tx, evaluation, f.artifact, "before")
        .await
        .unwrap();
    sqlx::query("INSERT INTO app.calibrations(estimator_kind,estimator_version,model_artifact_id,train_input_set_id,fit_end_available_at,output_unit,horizon_kind,validation_evaluation_id) VALUES('fixture','1',$1,$2,clock_timestamp(),'ratio','FIXED_BARS',$3)")
        .bind(f.artifact.as_uuid()).bind(f.input_set.as_uuid()).bind(evaluation.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlstate(
        metric(&mut tx, evaluation, f.artifact, "after-consumption")
            .await
            .unwrap_err(),
        "23000",
    );
    tx.rollback().await.unwrap();
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.evaluations WHERE id=$1)")
            .bind(evaluation.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        !exists,
        "the failed aggregate/consumer transaction must leave no partial evaluation"
    );
    assert_eq!(count(&pool, evaluation).await, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn metric_insertion_rechecks_publication_after_a_real_row_lock_wait(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (_, _, evaluation) = portfolio(&pool, &f).await;
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM app.evaluations WHERE id=$1 FOR UPDATE")
        .bind(evaluation.as_uuid())
        .execute(&mut *blocker)
        .await
        .unwrap();
    let mut connection = pool.acquire().await.unwrap();
    let backend: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let insert = metric(&mut connection, evaluation, f.artifact, "late");
    let release = async {
        wait_for_database_lock(&pool, backend).await;
        blocker.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(insert, release);
    sqlstate(result.unwrap_err(), "23000");
    assert_eq!(count(&pool, evaluation).await, 0);
}

#[sqlx::test(migrations = false)]
async fn upgrade_seals_existing_metrics_without_rewriting_any_evidence(pool: PgPool) {
    migrate_before(&pool, 202609060005).await;
    let f = fixture(&pool, budget()).await;
    let (_, _, evaluation) = portfolio(&pool, &f).await;
    let mut connection = pool.acquire().await.unwrap();
    metric(&mut connection, evaluation, f.artifact, "original")
        .await
        .unwrap();
    let query="SELECT jsonb_build_object('evaluation',to_jsonb(e),'metrics',(SELECT jsonb_agg(to_jsonb(m) ORDER BY m.id) FROM app.metric_values m WHERE m.evaluation_id=e.id)) FROM app.evaluations e WHERE e.id=$1";
    let before: serde_json::Value = sqlx::query_scalar(query)
        .bind(evaluation.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let after: serde_json::Value = sqlx::query_scalar(query)
        .bind(evaluation.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, after);
    sqlstate(
        metric(&mut connection, evaluation, f.artifact, "late")
            .await
            .unwrap_err(),
        "23000",
    );
    assert_eq!(count(&pool, evaluation).await, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_and_allocation_evaluation_keep_their_deferred_atomic_creation_order(
    pool: PgPool,
) {
    let f = fixture(&pool, budget()).await;
    let (mandate, _, base) = portfolio(&pool, &f).await;
    let candidate = Id::new();
    let evaluation = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.portfolio_candidates(id,project_id,mandate_id,input_set_id,decision_asof,run_id,solver_status,evidence_status,diagnostics_artifact_id,current_weights_source,allocation_evaluation_id) VALUES($1,$2,$3,$4,clock_timestamp(),$5,'OPTIMAL','VALID',$6,'NONE',$7)")
        .bind(candidate.as_uuid()).bind(f.project.as_uuid()).bind(mandate.as_uuid()).bind(f.input_set.as_uuid())
        .bind(f.run.as_uuid()).bind(f.artifact.as_uuid()).bind(evaluation.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,project_id,$3,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until FROM app.evaluations WHERE id=$2")
        .bind(evaluation.as_uuid()).bind(base.as_uuid()).bind(candidate.as_uuid()).execute(&mut *tx).await.unwrap();
    metric(&mut tx, evaluation, f.artifact, "allocation")
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let mut connection = pool.acquire().await.unwrap();
    sqlstate(
        metric(&mut connection, evaluation, f.artifact, "late")
            .await
            .unwrap_err(),
        "23000",
    );
    assert_eq!(count(&pool, evaluation).await, 1);
}
