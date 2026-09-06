//! Real PostgreSQL publication, exact ownership and cursor-atomicity regressions.
//! Fixture PASS rows are relationship tests, not scientific qualification.
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

struct Alpha {
    experiment: Id,
    version: Id,
    qualification: Id,
}
async fn alpha(pool: &PgPool, f: &Fixture) -> Alpha {
    let family = Id::new();
    let experiment = Id::new();
    let alpha = Id::new();
    let version = Id::new();
    let evaluation = Id::new();
    let qualification = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.experiment_families(id,project_id,root_lineage_id,question,selection_policy_id) SELECT $1,p.id,p.root_lineage_id,'fixture',b.evaluation_policy_id FROM app.projects p JOIN app.research_briefs b ON b.project_id=p.id WHERE p.id=$2")
        .bind(family.as_uuid()).bind(f.project.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.experiments(id,project_id,cycle_id,family_id,ordinal,hypothesis,expected_failure_modes,proposal_artifact_id,code_artifact_id,parameter_artifact_id,trial_source,run_id,outcome,conclusion_artifact_id) VALUES($1,$2,$3,$4,(SELECT coalesce(max(ordinal),0)+1 FROM app.experiments WHERE cycle_id=$3),'fixture','fixture',$5,$5,$5,'OPERATOR',$6,'SUPPORTED',$5)")
        .bind(experiment.as_uuid()).bind(f.project.as_uuid()).bind(f.cycle.as_uuid()).bind(family.as_uuid())
        .bind(f.artifact.as_uuid()).bind(f.run.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "INSERT INTO app.alphas(id,project_id,name,lifecycle) VALUES($1,$2,'fixture','RESEARCH')",
    )
    .bind(alpha.as_uuid())
    .bind(f.project.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap();
    sqlx::query("INSERT INTO app.alpha_versions(id,project_id,alpha_id,version,experiment_id,root_lineage_id,code_artifact_id,signal_contract_version,signal_kind,horizon_kind,horizon_value,forecast_unit,runtime_image_ref) SELECT $1,p.id,$2,1,$3,p.root_lineage_id,$4,'1','SCORE','FIXED_BARS',1,'UNITLESS_SCORE','fixture' FROM app.projects p WHERE p.id=$5")
        .bind(version.as_uuid()).bind(alpha.as_uuid()).bind(experiment.as_uuid())
        .bind(f.artifact.as_uuid()).bind(f.project.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_alpha_version_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,$2,$3,$4,b.evaluation_policy_id,$5,'SEALED','SUCCEEDED','VALID','PASS',$6,$6,statement_timestamp(),statement_timestamp()+interval '1 hour' FROM app.research_briefs b WHERE b.project_id=$2")
        .bind(evaluation.as_uuid()).bind(f.project.as_uuid()).bind(version.as_uuid())
        .bind(f.input_set.as_uuid()).bind(f.run.as_uuid()).bind(f.artifact.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.qualifications(id,alpha_version_id,policy_id,qualifying_evaluation_id,granted_at,valid_until) SELECT $1,$2,policy_id,id,concluded_at,valid_until FROM app.evaluations WHERE id=$3")
        .bind(qualification.as_uuid()).bind(version.as_uuid()).bind(evaluation.as_uuid()).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    Alpha {
        experiment,
        version,
        qualification,
    }
}

async fn candidate_header(connection: &mut PgConnection, f: &Fixture, mandate: Id) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.portfolio_candidates(id,project_id,mandate_id,input_set_id,decision_asof,run_id,solver_status,evidence_status,diagnostics_artifact_id,current_weights_source) VALUES($1,$2,$3,$4,statement_timestamp(),$5,'OPTIMAL','VALID',$6,'NONE')")
        .bind(id.as_uuid()).bind(f.project.as_uuid()).bind(mandate.as_uuid()).bind(f.input_set.as_uuid())
        .bind(f.run.as_uuid()).bind(f.artifact.as_uuid()).execute(connection).await.unwrap();
    id
}
async fn member(
    connection: &mut PgConnection,
    candidate: Id,
    alpha: &Alpha,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.candidate_alphas(candidate_id,alpha_version_id,qualification_id,ensemble_weight,forecast_unit,coverage_fraction) VALUES($1,$2,$3,1,'UNITLESS_SCORE',1)")
        .bind(candidate.as_uuid()).bind(alpha.version.as_uuid()).bind(alpha.qualification.as_uuid())
        .execute(connection).await?;
    Ok(())
}
async fn target(
    connection: &mut PgConnection,
    candidate: Id,
    instrument: &str,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.candidate_targets(candidate_id,instrument_id,target_weight,currency,asof,valid_until) VALUES($1,$2,0.5,'USD',statement_timestamp(),statement_timestamp()+interval '1 hour')")
        .bind(candidate.as_uuid()).bind(instrument).execute(connection).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_members_cannot_borrow_qualified_alphas_from_another_project(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let other = fixture(&pool, budget()).await;
    let foreign = alpha(&pool, &other).await;
    let (mandate, _, _) = portfolio(&pool, &f).await;
    let mut tx = pool.begin().await.unwrap();
    let candidate = candidate_header(&mut tx, &f, mandate).await;
    sqlstate(
        member(&mut tx, candidate, &foreign).await.unwrap_err(),
        "23503",
    );
    tx.rollback().await.unwrap();
    let local = alpha(&pool, &f).await;
    let mut tx = pool.begin().await.unwrap();
    let candidate = candidate_header(&mut tx, &f, mandate).await;
    member(&mut tx, candidate, &local).await.unwrap();
    target(&mut tx, candidate, "TEST.X").await.unwrap();
    tx.commit().await.unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.candidate_alphas WHERE candidate_id=$1")
            .bind(candidate.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_membership_seals_at_commit_and_cannot_be_reopened(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let first = alpha(&pool, &f).await;
    let second = alpha(&pool, &f).await;
    let (mandate, _, _) = portfolio(&pool, &f).await;
    let mut tx = pool.begin().await.unwrap();
    let candidate = candidate_header(&mut tx, &f, mandate).await;
    member(&mut tx, candidate, &first).await.unwrap();
    target(&mut tx, candidate, "FIRST.X").await.unwrap();
    tx.commit().await.unwrap();
    let mut connection = pool.acquire().await.unwrap();
    sqlstate(
        member(&mut connection, candidate, &second)
            .await
            .unwrap_err(),
        "23000",
    );
    sqlstate(
        target(&mut connection, candidate, "SECOND.X")
            .await
            .unwrap_err(),
        "23000",
    );
    sqlstate(
        sqlx::query("DELETE FROM app.candidate_publications WHERE candidate_id=$1")
            .bind(candidate.as_uuid())
            .execute(&mut *connection)
            .await
            .unwrap_err(),
        "23000",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn an_evaluation_seals_candidate_members_even_before_the_creation_transaction_commits(
    pool: PgPool,
) {
    let f = fixture(&pool, budget()).await;
    let (mandate, _, evaluation) = portfolio(&pool, &f).await;
    let mut tx = pool.begin().await.unwrap();
    let candidate = candidate_header(&mut tx, &f, mandate).await;
    target(&mut tx, candidate, "FIRST.X").await.unwrap();
    sqlx::query("INSERT INTO app.evaluations(project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT project_id,$1,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until FROM app.evaluations WHERE id=$2")
        .bind(candidate.as_uuid()).bind(evaluation.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlstate(
        target(&mut tx, candidate, "LATE.X").await.unwrap_err(),
        "23000",
    );
    tx.rollback().await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn alpha_and_family_cannot_reset_the_project_lineage(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let original = alpha(&pool, &f).await;
    let new_lineage = Id::new();
    sqlx::query("INSERT INTO app.research_lineages(id,origin,reason) VALUES($1,'NEW','fixture')")
        .bind(new_lineage.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlstate(
        sqlx::query("INSERT INTO app.alpha_versions(project_id,alpha_id,version,experiment_id,root_lineage_id,code_artifact_id,signal_contract_version,signal_kind,horizon_kind,horizon_value,forecast_unit,runtime_image_ref) SELECT project_id,alpha_id,version+1,experiment_id,$1,code_artifact_id,signal_contract_version,signal_kind,horizon_kind,horizon_value,forecast_unit,runtime_image_ref FROM app.alpha_versions WHERE id=$2")
            .bind(new_lineage.as_uuid()).bind(original.version.as_uuid()).execute(&pool).await.unwrap_err(),
        "23503",
    );
    sqlstate(
        sqlx::query("INSERT INTO app.experiment_families(project_id,root_lineage_id,question,selection_policy_id) SELECT project_id,$1,'fixture',evaluation_policy_id FROM app.research_briefs WHERE project_id=$2")
            .bind(new_lineage.as_uuid()).bind(f.project.as_uuid()).execute(&pool).await.unwrap_err(),
        "23503",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn experiment_self_and_multirow_cycles_fail_while_ordinary_ancestry_is_retained(
    pool: PgPool,
) {
    let f = fixture(&pool, budget()).await;
    let a = alpha(&pool, &f).await;
    let self_parent = Id::new();
    let insert = "INSERT INTO app.experiments(id,project_id,cycle_id,family_id,parent_experiment_id,ordinal,hypothesis,expected_failure_modes,proposal_artifact_id,trial_source,outcome) SELECT $1,project_id,cycle_id,family_id,$2,ordinal+1,'fixture','fixture',proposal_artifact_id,'OPERATOR','PENDING' FROM app.experiments WHERE id=$3";
    sqlstate(
        sqlx::query(insert)
            .bind(self_parent.as_uuid())
            .bind(self_parent.as_uuid())
            .bind(a.experiment.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23514",
    );
    let child = Id::new();
    sqlx::query(insert)
        .bind(child.as_uuid())
        .bind(a.experiment.as_uuid())
        .bind(a.experiment.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let left = Id::new();
    let right = Id::new();
    sqlstate(
        sqlx::query("INSERT INTO app.experiments(id,project_id,cycle_id,family_id,parent_experiment_id,ordinal,hypothesis,expected_failure_modes,proposal_artifact_id,trial_source,outcome) SELECT v.id,e.project_id,e.cycle_id,e.family_id,v.parent,e.ordinal+v.ordinal_offset,'fixture','fixture',e.proposal_artifact_id,'OPERATOR','PENDING' FROM app.experiments e CROSS JOIN (VALUES($1::uuid,$2::uuid,2),($2::uuid,$1::uuid,3)) AS v(id,parent,ordinal_offset) WHERE e.id=$3")
            .bind(left.as_uuid()).bind(right.as_uuid()).bind(a.experiment.as_uuid()).execute(&pool).await.unwrap_err(),
        "23514",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn consumed_experiment_results_cannot_change_beneath_alpha_versions(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let a = alpha(&pool, &f).await;
    for change in [
        "code_artifact_id=NULL",
        "parameter_artifact_id=NULL",
        "run_id=NULL",
        "outcome='REJECTED'",
        "outcome_reason='changed result'",
        "conclusion_artifact_id=NULL",
    ] {
        let sql = format!("UPDATE app.experiments SET {change} WHERE id=$1");
        sqlstate(
            sqlx::query(&sql)
                .bind(a.experiment.as_uuid())
                .execute(&pool)
                .await
                .unwrap_err(),
            "23000",
        );
    }
    // An unconsumed proposal with no evaluation Run can still receive results.
    let draft = Id::new();
    sqlx::query("INSERT INTO app.experiments(id,project_id,cycle_id,family_id,ordinal,hypothesis,expected_failure_modes,proposal_artifact_id,trial_source,outcome) SELECT $1,project_id,cycle_id,family_id,ordinal+1,'fixture','fixture',proposal_artifact_id,'OPERATOR','PENDING' FROM app.experiments WHERE id=$2")
        .bind(draft.as_uuid()).bind(a.experiment.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("UPDATE app.experiments SET outcome='INCONCLUSIVE' WHERE id=$1")
        .bind(draft.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
}

async fn downstream(pool: &PgPool) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'fixture','https://example.invalid','fixture','{fixture}','BOTH',true)")
        .bind(id.as_uuid()).execute(pool).await.unwrap();
    id
}
async fn approval(
    pool: &PgPool,
    release: Id,
    policy: Id,
    downstream: Id,
    inputs: Id,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.approvals(release_id,environment,downstream_id,authority_kind,automation_policy_id,evidence_set_id,granted_at,valid_until) VALUES($1,'PAPER',$2,'FROZEN_POLICY',$3,$4,statement_timestamp(),statement_timestamp()+interval '1 hour')")
        .bind(release.as_uuid()).bind(downstream.as_uuid()).bind(policy.as_uuid())
        .bind(inputs.as_uuid()).execute(pool).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn policy_approval_is_bound_to_its_exact_mandate_project_and_downstream(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (mandate, candidate, evaluation) = portfolio(&pool, &f).await;
    let release = release(&pool, &f, mandate, candidate, evaluation)
        .await
        .unwrap();
    let d = downstream(&pool).await;
    let other_d = downstream(&pool).await;
    let policy = Id::new();
    sqlx::query("INSERT INTO app.automation_policies(id,project_id,mode,mandate_id,downstream_id,required_paper_observations,minimum_paper_elapsed_seconds,max_feedback_age_seconds,promotion_metric_requirements,degradation_metric_requirements,authorized_at,valid_until,enabled_for_new_rebalances,max_rebalances_per_day) VALUES($1,$2,'AUTO_HANDOFF',$3,$4,1,1,60,'[]','[]',statement_timestamp(),statement_timestamp()+interval '1 hour',true,1)")
        .bind(policy.as_uuid()).bind(f.project.as_uuid()).bind(mandate.as_uuid()).bind(d.as_uuid()).execute(&pool).await.unwrap();
    approval(&pool, release, policy, d, f.input_set)
        .await
        .unwrap();
    sqlstate(
        approval(&pool, release, policy, other_d, f.input_set)
            .await
            .unwrap_err(),
        "23503",
    );
    let other = fixture(&pool, budget()).await;
    let (other_m, other_c, other_e) = portfolio(&pool, &other).await;
    let other_r = support::release(&pool, &other, other_m, other_c, other_e)
        .await
        .unwrap();
    sqlstate(
        approval(&pool, other_r, policy, d, other.input_set)
            .await
            .unwrap_err(),
        "23503",
    );
}

async fn forward_inputs(pool: &PgPool, f: &Fixture) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.input_sets(id,project_id,purpose,decision_cutoff,frozen_at) VALUES($1,$2,'FORWARD',statement_timestamp(),statement_timestamp())")
        .bind(id.as_uuid()).bind(f.project.as_uuid()).execute(pool).await.unwrap();
    id
}
async fn forward_window(
    pool: &PgPool,
    release: Id,
    evaluation: Id,
    inputs: Id,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.forward_evidence_windows(release_id,input_set_id,evaluation_id,window_start,window_end,complete_observations,is_contiguous,freshness_deadline) VALUES($1,$2,$3,statement_timestamp()-interval '1 hour',statement_timestamp(),100,true,statement_timestamp()+interval '1 hour')")
        .bind(release.as_uuid()).bind(inputs.as_uuid()).bind(evaluation.as_uuid()).execute(pool).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn forward_evidence_cannot_borrow_another_subject_kind_or_input_snapshot(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (mandate, candidate, evaluation) = portfolio(&pool, &f).await;
    let release = release(&pool, &f, mandate, candidate, evaluation)
        .await
        .unwrap();
    sqlstate(
        forward_window(&pool, release, evaluation, f.input_set)
            .await
            .unwrap_err(),
        "23503",
    );
    let inputs = forward_inputs(&pool, &f).await;
    let different_inputs = forward_inputs(&pool, &f).await;
    let forward = Id::new();
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,project_id,subject_candidate_id,$2,policy_id,run_id,'FORWARD',execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until FROM app.evaluations WHERE id=$3")
        .bind(forward.as_uuid()).bind(inputs.as_uuid()).bind(evaluation.as_uuid()).execute(&pool).await.unwrap();
    forward_window(&pool, release, forward, inputs)
        .await
        .unwrap();
    sqlstate(
        forward_window(&pool, release, forward, different_inputs)
            .await
            .unwrap_err(),
        "23503",
    );
    let other_c = support::candidate(&pool, &f, mandate).await;
    let wrong_subject = Id::new();
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,project_id,$2,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until FROM app.evaluations WHERE id=$3")
        .bind(wrong_subject.as_uuid()).bind(other_c.as_uuid()).bind(forward.as_uuid()).execute(&pool).await.unwrap();
    sqlstate(
        forward_window(&pool, release, wrong_subject, inputs)
            .await
            .unwrap_err(),
        "23503",
    );
}

async fn manifest(pool: &PgPool, f: &Fixture, attempt: Id) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,'REPORT','application/json','fixture','1','LOCAL',$5,'1',1,'RESEARCH','FIXTURE','RUNTIME','AUDIT')")
        .bind(id.as_uuid()).bind(f.project.as_uuid()).bind(f.run.as_uuid()).bind(attempt.as_uuid())
        .bind(format!("fixture/{id}")).execute(pool).await.unwrap();
    id
}

#[sqlx::test(migrations = "../../migrations")]
async fn accepted_manifest_requires_exact_producer_and_cannot_be_replaced(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let second_attempt = Id::new();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,dispatch_state,runtime_state) VALUES($1,$2,2,'worker-B',1,$3,'ACKNOWLEDGED','RUNNING')")
        .bind(second_attempt.as_uuid()).bind(f.run.as_uuid()).bind(f.deadline).execute(&pool).await.unwrap();
    let other_manifest = manifest(&pool, &f, second_attempt).await;
    sqlstate(
        sqlx::query("UPDATE app.run_attempts SET accepted_at=statement_timestamp() WHERE id=$1")
            .bind(f.fence.attempt_id.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23514",
    );
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("UPDATE app.run_attempts SET result_manifest_artifact_id=$2,accepted_at=statement_timestamp() WHERE id=$1")
        .bind(f.fence.attempt_id.as_uuid()).bind(other_manifest.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlstate(tx.commit().await.unwrap_err(), "23503");
    let correct = manifest(&pool, &f, f.fence.attempt_id).await;
    sqlx::query("UPDATE app.run_attempts SET result_manifest_artifact_id=$2,accepted_at=statement_timestamp() WHERE id=$1")
        .bind(f.fence.attempt_id.as_uuid()).bind(correct.as_uuid()).execute(&pool).await.unwrap();
    for field in ["accepted_at", "result_manifest_artifact_id"] {
        let sql = format!("UPDATE app.run_attempts SET {field}=NULL WHERE id=$1");
        sqlstate(
            sqlx::query(&sql)
                .bind(f.fence.attempt_id.as_uuid())
                .execute(&pool)
                .await
                .unwrap_err(),
            "23000",
        );
    }
}

async fn new_mission(pool: &PgPool, f: &Fixture) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,deadline_at,queued_at) VALUES($1,$2,$3,'AGENT_RESEARCH',$4,'QUEUED',$5,statement_timestamp())")
        .bind(id.as_uuid()).bind(f.project.as_uuid()).bind(f.cycle.as_uuid()).bind(f.input_set.as_uuid())
        .bind(f.deadline).execute(pool).await.unwrap();
    id
}
async fn session(
    connection: &mut PgConnection,
    f: &Fixture,
    run: Id,
    revision: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.codex_sessions(project_id,cycle_id,run_id,role,profile_id,profile_revision,thread_id,codex_version,protocol_schema_version,requested_settings,native_history_ref) SELECT project_id,cycle_id,$2,role,profile_id,$3,$4,codex_version,protocol_schema_version,requested_settings,native_history_ref FROM app.codex_sessions WHERE id=$1")
        .bind(f.session.as_uuid()).bind(run.as_uuid()).bind(revision).bind(run.to_string())
        .execute(connection).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn session_creation_freezes_only_the_current_locked_profile_revision(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let run = new_mission(&pool, &f).await;
    let mut connection = pool.acquire().await.unwrap();
    sqlstate(
        session(&mut connection, &f, run, 99).await.unwrap_err(),
        "23503",
    );
    session(&mut connection, &f, run, 1).await.unwrap();
    let old: serde_json::Value =
        sqlx::query_scalar("SELECT requested_settings FROM app.codex_sessions WHERE run_id=$1")
            .bind(run.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE app.codex_profiles SET saved_model='fixture-change' WHERE id=$1")
        .bind(f.profile.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let next = new_mission(&pool, &f).await;
    sqlstate(
        session(&mut connection, &f, next, 1).await.unwrap_err(),
        "23503",
    );
    session(&mut connection, &f, next, 2).await.unwrap();
    let (saved_revision, saved): (i64, serde_json::Value) = sqlx::query_as("SELECT profile_revision::bigint,requested_settings FROM app.codex_sessions WHERE run_id=$1")
        .bind(run.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(saved_revision, 1);
    assert_eq!(saved, old);
    sqlstate(
        sqlx::query("UPDATE app.codex_sessions SET requested_settings='{\"schema_version\":1,\"changed\":true}' WHERE run_id=$1")
            .bind(run.as_uuid()).execute(&pool).await.unwrap_err(),
        "23000",
    );
}

async fn event(connection: &mut PgConnection, run: Id, seq: i64) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.run_events(run_id,seq,event_type,schema_version,payload,occurred_at) VALUES($1,$2,'fixture',1,'{\"schema_version\":1}',statement_timestamp())")
        .bind(run.as_uuid()).bind(seq).execute(connection).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn event_and_cursor_commit_or_rollback_together_without_gaps(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let mut connection = pool.acquire().await.unwrap();
    sqlstate(event(&mut connection, f.run, 2).await.unwrap_err(), "23514");
    sqlstate(
        sqlx::query("UPDATE app.runs SET last_event_seq=1 WHERE id=$1")
            .bind(f.run.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23000",
    );
    sqlx::query("INSERT INTO app.run_events(run_id,seq,event_type,schema_version,payload,occurred_at) VALUES($1,1,'fixture',1,'{\"schema_version\":1}',statement_timestamp()),($1,2,'fixture',1,'{\"schema_version\":1}',statement_timestamp())")
        .bind(f.run.as_uuid()).execute(&pool).await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    event(&mut tx, f.run, 3).await.unwrap();
    tx.rollback().await.unwrap();
    let (cursor, count): (i64, i64) = sqlx::query_as("SELECT last_event_seq::bigint,(SELECT count(*) FROM app.run_events WHERE run_id=$1) FROM app.runs WHERE id=$1")
        .bind(f.run.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!((cursor, count), (2, 2));
    sqlstate(
        sqlx::query("UPDATE app.runs SET last_event_seq=0 WHERE id=$1")
            .bind(f.run.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23000",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_event_append_waits_for_prior_transaction_commit(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let mut first = pool.begin().await.unwrap();
    event(&mut first, f.run, 1).await.unwrap();
    let append = async {
        let mut connection = pool.acquire().await.unwrap();
        event(&mut connection, f.run, 2).await
    };
    let release = async {
        let mut waiting = false;
        for _ in 0..500 {
            waiting = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'INSERT INTO app.run_events%')")
                .fetch_one(&pool).await.unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(waiting, "event append must wait for its predecessor commit");
        first.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(append, release);
    result.unwrap();
    let cursor: i64 = sqlx::query_scalar("SELECT last_event_seq::bigint FROM app.runs WHERE id=$1")
        .bind(f.run.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(cursor, 2);
}
