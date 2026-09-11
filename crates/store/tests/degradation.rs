//! A Wake observation may consume only the exact immutable forward evidence.
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
async fn policy(pool: &PgPool, project: Id, mandate: Id) -> Id {
    let downstream = Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'fixture','https://example.invalid','fixture','{fixture}','BOTH',true)")
        .bind(downstream.as_uuid()).execute(pool).await.unwrap();
    let policy = Id::new();
    sqlx::query("INSERT INTO app.automation_policies(id,project_id,mode,mandate_id,downstream_id,required_paper_observations,minimum_paper_elapsed_seconds,max_feedback_age_seconds,promotion_metric_requirements,degradation_metric_requirements,authorized_at,valid_until,enabled_for_new_rebalances,max_rebalances_per_day) VALUES($1,$2,'AUTO_HANDOFF',$3,$4,1,1,60,'[]','[]',statement_timestamp(),statement_timestamp()+interval '1 hour',true,1)")
        .bind(policy.as_uuid()).bind(project.as_uuid()).bind(mandate.as_uuid()).bind(downstream.as_uuid()).execute(pool).await.unwrap();
    policy
}
async fn forward(pool: &PgPool, f: &Fixture, base: Id) -> (Id, Id) {
    let inputs = Id::new();
    sqlx::query("INSERT INTO app.input_sets(id,project_id,purpose,decision_cutoff,frozen_at) VALUES($1,$2,'FORWARD',clock_timestamp(),clock_timestamp())")
        .bind(inputs.as_uuid()).bind(f.project.as_uuid()).execute(pool).await.unwrap();
    let evaluation = Id::new();
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,project_id,subject_candidate_id,$2,policy_id,run_id,'FORWARD',execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until FROM app.evaluations WHERE id=$3")
        .bind(evaluation.as_uuid()).bind(inputs.as_uuid()).bind(base.as_uuid()).execute(pool).await.unwrap();
    (inputs, evaluation)
}
async fn window(pool: &PgPool, release: Id, inputs: Id, evaluation: Id) {
    sqlx::query("INSERT INTO app.forward_evidence_windows(release_id,input_set_id,evaluation_id,window_start,window_end,complete_observations,is_contiguous,freshness_deadline) VALUES($1,$2,$3,statement_timestamp()-interval '1 hour',statement_timestamp(),100,true,statement_timestamp()+interval '1 hour')")
        .bind(release.as_uuid()).bind(inputs.as_uuid()).bind(evaluation.as_uuid()).execute(pool).await.unwrap();
}
async fn observation(
    pool: &PgPool,
    project: Id,
    release: Id,
    evaluation: Id,
    policy: Id,
    class: &str,
) -> Result<Id, sqlx::Error> {
    let id = Id::new();
    sqlx::query("INSERT INTO app.degradation_observations(id,project_id,release_id,evaluation_id,policy_id,classification,reason_codes,observed_at) VALUES($1,$2,$3,$4,$5,$6,'{fixture}',clock_timestamp())")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(release.as_uuid()).bind(evaluation.as_uuid()).bind(policy.as_uuid()).bind(class).execute(pool).await?;
    Ok(id)
}

#[sqlx::test(migrations = "../../migrations")]
async fn degradation_is_bound_to_the_project_release_candidate_policy_and_mandate(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (mandate, candidate, base) = portfolio(&pool, &f).await;
    let r = release(&pool, &f, mandate, candidate, base).await.unwrap();
    let p = policy(&pool, f.project, mandate).await;
    let (inputs, evaluation) = forward(&pool, &f, base).await;
    window(&pool, r, inputs, evaluation).await;
    for class in ["HEALTHY", "WATCH", "DEGRADED", "INSUFFICIENT_DATA"] {
        let observation = observation(&pool, f.project, r, evaluation, p, class)
            .await
            .unwrap();
        sqlx::query("INSERT INTO app.wake_events(project_id,observation_id,trigger,state,not_before,reason) VALUES($1,$2,'DEGRADATION','PENDING',clock_timestamp(),'fixture')")
            .bind(f.project.as_uuid()).bind(observation.as_uuid()).execute(&pool).await.unwrap();
    }
    let other = fixture(&pool, budget()).await;
    let (other_m, other_c, other_base) = portfolio(&pool, &other).await;
    let other_r = release(&pool, &other, other_m, other_c, other_base)
        .await
        .unwrap();
    let other_p = policy(&pool, other.project, other_m).await;
    let (other_i, other_e) = forward(&pool, &other, other_base).await;
    window(&pool, other_r, other_i, other_e).await;
    for (project, release, evaluation, policy) in [
        (f.project, other_r, other_e, p),
        (f.project, r, other_e, p),
        (f.project, r, evaluation, other_p),
        (other.project, r, evaluation, other_p),
    ] {
        sqlstate(
            observation(&pool, project, release, evaluation, policy, "DEGRADED")
                .await
                .unwrap_err(),
            "23503",
        );
    }
    // Same project is necessary but insufficient: mandate authorization is exact.
    let different_m = Id::new();
    sqlx::query("INSERT INTO app.portfolio_mandates(id,project_id,version,objective,risk_measure,base_currency,capital_assumption,universe_version_id,covariance_estimator,alpha_ensemble,optimizer,constraints,rebalance_schedule,required_evaluation_policy_id,execution_assumptions_id,exposure_tolerance) SELECT $1,project_id,version+1,objective,risk_measure,base_currency,capital_assumption,universe_version_id,covariance_estimator,alpha_ensemble,optimizer,constraints,rebalance_schedule,required_evaluation_policy_id,execution_assumptions_id,exposure_tolerance FROM app.portfolio_mandates WHERE id=$2")
        .bind(different_m.as_uuid()).bind(mandate.as_uuid()).execute(&pool).await.unwrap();
    let different_p = policy(&pool, f.project, different_m).await;
    sqlstate(
        observation(&pool, f.project, r, evaluation, different_p, "DEGRADED")
            .await
            .unwrap_err(),
        "23503",
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.degradation_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 4);
}

#[sqlx::test(migrations = "../../migrations")]
async fn degradation_requires_forward_kind_and_a_window_for_the_exact_evaluation_input(
    pool: PgPool,
) {
    let f = fixture(&pool, budget()).await;
    let (m, c, base) = portfolio(&pool, &f).await;
    let r = release(&pool, &f, m, c, base).await.unwrap();
    let p = policy(&pool, f.project, m).await;
    sqlstate(
        observation(&pool, f.project, r, base, p, "DEGRADED")
            .await
            .unwrap_err(),
        "23503",
    );
    let (i, e) = forward(&pool, &f, base).await;
    sqlstate(
        observation(&pool, f.project, r, e, p, "DEGRADED")
            .await
            .unwrap_err(),
        "23503",
    );
    window(&pool, r, i, e).await;
    observation(&pool, f.project, r, e, p, "DEGRADED")
        .await
        .unwrap();
    let (different_i, different_e) = forward(&pool, &f, base).await;
    // Identical subject but another input snapshot/evaluation cannot borrow w.
    sqlstate(
        observation(&pool, f.project, r, different_e, p, "DEGRADED")
            .await
            .unwrap_err(),
        "23503",
    );
    window(&pool, r, different_i, different_e).await;
    observation(&pool, f.project, r, different_e, p, "DEGRADED")
        .await
        .unwrap();
    let second = candidate(&pool, &f, m).await;
    let wrong_e = Id::new();
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,project_id,$2,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until FROM app.evaluations WHERE id=$3")
        .bind(wrong_e.as_uuid()).bind(second.as_uuid()).bind(e.as_uuid()).execute(&pool).await.unwrap();
    sqlstate(
        observation(&pool, f.project, r, wrong_e, p, "DEGRADED")
            .await
            .unwrap_err(),
        "23503",
    );
}

#[sqlx::test(migrations = false)]
async fn upgrade_fails_without_relabeling_invalid_historical_degradation(pool: PgPool) {
    migrate_before(&pool, 202609060005).await;
    let f = fixture(&pool, budget()).await;
    let (m, _, _) = portfolio(&pool, &f).await;
    let p = policy(&pool, f.project, m).await;
    let other = fixture(&pool, budget()).await;
    let (other_m, other_c, other_e) = portfolio(&pool, &other).await;
    let other_r = release(&pool, &other, other_m, other_c, other_e)
        .await
        .unwrap();
    let id = observation(&pool, f.project, other_r, other_e, p, "DEGRADED")
        .await
        .unwrap();
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(o) FROM app.degradation_observations o WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut failed = pool.acquire().await.unwrap();
    match sqlx::migrate!("../../migrations")
        .run(&mut *failed)
        .await
        .unwrap_err()
    {
        sqlx::migrate::MigrateError::ExecuteMigration(error, 202609060005) => {
            sqlstate(error, "23514")
        }
        other => panic!("unexpected migration failure {other:?}"),
    }
    failed.close().await.unwrap();
    let after: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(o) FROM app.degradation_observations o WHERE id=$1")
            .bind(id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let rolled_back: bool =
        sqlx::query_scalar("SELECT to_regclass('app.evaluation_publications') IS NULL")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        rolled_back,
        "SQLx must roll back the entire failed migration"
    );
}
