//! Test-only relational fixture, not a seed path or evidence of a user workflow.
#![allow(dead_code)]
use chrono::{DateTime, Duration, Utc};
use contracts::{budget::BudgetV1, DbCounter, Id, Revision};
use domain::admission::{CostEstimate, TurnKind};
use serde_json::json;
use sqlx::PgPool;
use store::turns::{TurnRequest, WorkerFence};

pub struct Fixture {
    pub project: Id,
    pub cycle: Id,
    pub run: Id,
    pub session: Id,
    pub profile: Id,
    pub input_set: Id,
    pub artifact: Id,
    pub report: Id,
    pub budget: BudgetV1,
    pub fence: WorkerFence,
    pub deadline: DateTime<Utc>,
}

pub fn budget() -> BudgetV1 {
    serde_json::from_value(json!({
        "schema_version":1,"max_experiments":20,"max_parallel_runs":2,
        "max_turns_per_mission":3,"max_repair_turns":1,"max_wall_seconds":3600,
        "max_cpu_seconds":"7200","max_memory_mib":4096,"max_output_bytes":"67108864",
        "max_cycles_per_day":3,"min_cycle_interval_seconds":60,"max_tokens":"100",
        "max_cost_decimal":"10","cost_currency":"USD","cost_enforcement":"ESTIMATED"
    }))
    .unwrap()
}

pub async fn fixture(pool: &PgPool, budget: BudgetV1) -> Fixture {
    let project = Id::new();
    let lineage = Id::new();
    let artifact = Id::new();
    let universe = Id::new();
    let assumptions = Id::new();
    let policy = Id::new();
    let family = Id::new();
    let brief = Id::new();
    let cycle = Id::new();
    let input_set = Id::new();
    let profile = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.research_lineages(id,origin,reason) VALUES($1,'NEW','isolated SQL regression fixture')")
        .bind(lineage.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.projects(id,root_lineage_id,name,state,created_by) VALUES($1,$2,'fixture','ACTIVE','OPERATOR')")
        .bind(project.as_uuid()).bind(lineage.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PARAMETERS','application/json','fixture','1','LOCAL',$3,'1',1,'RESEARCH','FIXTURE','OPERATOR','AUDIT')")
        .bind(artifact.as_uuid()).bind(project.as_uuid()).bind(format!("fixture/{artifact}")).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.universe_versions(id,name,membership_artifact_id,instrument_definition_artifact_id,calendar_ref,calendar_version,selection_asof,has_historical_membership,coverage_start,coverage_end) VALUES($1,'fixture',$2,$2,'fixture','1',clock_timestamp(),true,clock_timestamp()-interval '1 day',clock_timestamp())")
        .bind(universe.as_uuid()).bind(artifact.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.execution_assumptions(id,venue_capability_ref,engine_image_ref,price_type,starting_capital,base_currency,fee_schedule_artifact_id,slippage_model,fill_model,cost_assumption_status,calendar_version,settlement_rule_ref) VALUES($1,'fixture','fixture','BAR',100,'USD',$2,$3,$3,'CONSERVATIVE_ASSUMPTION','1','fixture')")
        .bind(assumptions.as_uuid()).bind(artifact.as_uuid()).bind(json!({"schema_version":1})).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.evaluation_policies(id,project_id,version,selection_rule,split_policy,metric_requirements,minimum_observations,maximum_missing_fraction,require_real_data,required_capabilities,maximum_sealed_uses_per_lineage,validity_seconds) VALUES($1,$2,1,$3,$3,'[]',1,0,true,'{}',1,3600)")
        .bind(policy.as_uuid()).bind(project.as_uuid()).bind(json!({"schema_version":1,"family_id":family,"root_lineage_id":lineage})).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.experiment_families(id,project_id,root_lineage_id,question,selection_policy_id) VALUES($1,$2,$3,'isolated SQL regression fixture',$4)")
        .bind(family.as_uuid()).bind(project.as_uuid()).bind(lineage.as_uuid()).bind(policy.as_uuid()).execute(&mut *tx).await.unwrap();
    let stop = json!({"schema_version":1,"stop_on_qualified_count":1,"stop_on_budget":true,"stop_on_no_improvement_trials":null,"stop_on_invalid_data":true});
    sqlx::query("INSERT INTO app.research_briefs(id,project_id,version,hypothesis,economic_rationale,universe_version_id,target_kind,horizon_kind,horizon_value,base_currency,evaluation_policy_id,execution_assumptions_id,budget,stop_rule,state,frozen_at) VALUES($1,$2,1,'fixture','fixture',$3,'SCORE','FIXED_BARS',1,'USD',$4,$5,$6,$7,'FROZEN',clock_timestamp())")
        .bind(brief.as_uuid()).bind(project.as_uuid()).bind(universe.as_uuid()).bind(policy.as_uuid()).bind(assumptions.as_uuid())
        .bind(serde_json::to_value(&budget).unwrap()).bind(stop).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.input_sets(id,project_id,purpose,decision_cutoff) VALUES($1,$2,'DISCOVERY',clock_timestamp())")
        .bind(input_set.as_uuid()).bind(project.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.input_set_items(input_set_id,artifact_id,role,ordinal) VALUES($1,$2,'PARAMETERS',0)")
        .bind(input_set.as_uuid()).bind(artifact.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE app.input_sets SET frozen_at=clock_timestamp() WHERE id=$1")
        .bind(input_set.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app.research_cycles(id,project_id,brief_id,ordinal,trigger,state,budget_snapshot) VALUES($1,$2,$3,1,'OPERATOR','RUNNING',$4)")
        .bind(cycle.as_uuid()).bind(project.as_uuid()).bind(brief.as_uuid()).bind(serde_json::to_value(&budget).unwrap()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.codex_profiles(id,name,connection_mode,profile_origin,codex_home_ref,use_default_model_settings,saved_fast_mode) VALUES($1,'fixture','SYSTEM','MANAGED_VOLUME',$2,true,false)")
        .bind(profile.as_uuid()).bind(format!("fixture/{profile}")).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    let (run, session, fence, deadline) = mission(pool, project, cycle, input_set, profile).await;
    let report = report_artifact(pool, project, run, fence.attempt_id).await;
    Fixture {
        project,
        cycle,
        run,
        session,
        profile,
        input_set,
        artifact,
        report,
        budget,
        fence,
        deadline,
    }
}

pub async fn mission(
    pool: &PgPool,
    project: Id,
    cycle: Id,
    input_set: Id,
    profile: Id,
) -> (Id, Id, WorkerFence, DateTime<Utc>) {
    let run = Id::new();
    let session = Id::new();
    let attempt = Id::new();
    let now: DateTime<Utc> =
        sqlx::query_scalar("SELECT date_trunc('microseconds',clock_timestamp())")
            .fetch_one(pool)
            .await
            .unwrap();
    let deadline = now + Duration::minutes(30);
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,deadline_at,queued_at,current_attempt_no,active_attempt_id) VALUES($1,$2,$3,'AGENT_RESEARCH',$4,'RUNNING',$5,$6,1,$7)")
        .bind(run.as_uuid()).bind(project.as_uuid()).bind(cycle.as_uuid()).bind(input_set.as_uuid()).bind(deadline).bind(now).bind(attempt.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,dispatch_state,runtime_state) VALUES($1,$2,1,'worker-A',1,$3,'ACKNOWLEDGED','RUNNING')")
        .bind(attempt.as_uuid()).bind(run.as_uuid()).bind(deadline).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.codex_sessions(id,project_id,cycle_id,run_id,role,profile_id,profile_revision,thread_id,codex_version,protocol_schema_version,requested_settings,native_history_ref) VALUES($1,$2,$3,$4,'RESEARCHER',$5,1,$6,'fixture','fixture',$7,'fixture')")
        .bind(session.as_uuid()).bind(project.as_uuid()).bind(cycle.as_uuid()).bind(run.as_uuid()).bind(profile.as_uuid()).bind(format!("native/{run}"))
        .bind(json!({"schema_version":1})).execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    (
        run,
        session,
        WorkerFence {
            attempt_id: attempt,
            worker_owner_id: "worker-A".into(),
            owner_epoch: Revision::INITIAL,
        },
        deadline,
    )
}

impl Fixture {
    pub fn request(&self, key: &str) -> TurnRequest {
        TurnRequest {
            command_key: key.into(),
            turn_kind: TurnKind::Research,
            tokens: DbCounter::new(40).unwrap(),
            estimated_cost: Some(CostEstimate {
                currency: "USD".into(),
                amount: "1.25".parse().unwrap(),
            }),
            request_artifact_id: self.artifact,
            deadline_at: self.deadline,
        }
    }
}

pub async fn candidate(pool: &PgPool, f: &Fixture, mandate: Id) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.portfolio_candidates(id,project_id,mandate_id,input_set_id,decision_asof,run_id,solver_status,evidence_status,diagnostics_artifact_id,current_weights_source) VALUES($1,$2,$3,$4,clock_timestamp(),$5,'OPTIMAL','VALID',$6,'NONE')")
        .bind(id.as_uuid()).bind(f.project.as_uuid()).bind(mandate.as_uuid()).bind(f.input_set.as_uuid()).bind(f.run.as_uuid()).bind(f.artifact.as_uuid()).execute(pool).await.unwrap();
    id
}
pub async fn portfolio(pool: &PgPool, f: &Fixture) -> (Id, Id, Id) {
    let mandate = Id::new();
    sqlx::query("INSERT INTO app.portfolio_mandates(id,project_id,version,objective,risk_measure,base_currency,capital_assumption,universe_version_id,covariance_estimator,alpha_ensemble,optimizer,constraints,rebalance_schedule,required_evaluation_policy_id,execution_assumptions_id,exposure_tolerance) SELECT $1,b.project_id,1,'MIN_RISK','VARIANCE','USD',100,b.universe_version_id,'{\"schema_version\":1}','{\"schema_version\":1}','{\"schema_version\":1}','{\"schema_version\":1}','{\"schema_version\":1}',b.evaluation_policy_id,b.execution_assumptions_id,0.00001 FROM app.research_briefs b WHERE b.project_id=$2")
        .bind(mandate.as_uuid()).bind(f.project.as_uuid()).execute(pool).await.unwrap();
    let candidate = candidate(pool, f, mandate).await;
    let evaluation = Id::new();
    sqlx::query("INSERT INTO app.evaluations(id,project_id,subject_candidate_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT $1,$2,$3,$4,b.evaluation_policy_id,$5,'PORTFOLIO','SUCCEEDED','VALID','PASS',$6,$6,clock_timestamp(),clock_timestamp()+interval '1 hour' FROM app.research_briefs b WHERE b.project_id=$2")
        .bind(evaluation.as_uuid()).bind(f.project.as_uuid()).bind(candidate.as_uuid()).bind(f.input_set.as_uuid()).bind(f.run.as_uuid()).bind(f.report.as_uuid()).execute(pool).await.unwrap();
    (mandate, candidate, evaluation)
}

/// Non-deliverable example. Even its immutable object has the actual Package
/// role; a Parameters row must never stand in for a Package.
pub async fn release(
    pool: &PgPool,
    f: &Fixture,
    mandate: Id,
    candidate: Id,
    evaluation: Id,
) -> Result<Id, sqlx::Error> {
    release_metadata(pool, f, mandate, candidate, evaluation, "DEMO", "FIXTURE").await
}

/// Relational positive-control metadata only. No files, real market observations,
/// scientific qualification or product delivery are generated by this helper.
/// This is compiled only into integration tests; there is no runtime bypass or
/// relabelling of the FIXTURE object used by `release`.
pub async fn delivery_release_metadata(
    pool: &PgPool,
    f: &Fixture,
    mandate: Id,
    candidate: Id,
    evaluation: Id,
) -> Result<Id, sqlx::Error> {
    release_metadata(pool, f, mandate, candidate, evaluation, "REAL", "REAL").await
}

async fn release_metadata(
    pool: &PgPool,
    f: &Fixture,
    mandate: Id,
    candidate: Id,
    evaluation: Id,
    environment: &str,
    origin: &str,
) -> Result<Id, sqlx::Error> {
    let id = Id::new();
    let package = Id::new();
    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PACKAGE','application/json','qz.target_package','1','LOCAL',$3,'1',1,'DELIVERY',$4,'OPERATOR','AUDIT')")
        .bind(package.as_uuid()).bind(f.project.as_uuid()).bind(format!("isolated-relational-test/{package}"))
        .bind(origin).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO app.releases(id,candidate_id,package_artifact_id,package_schema_version,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,environment) VALUES($1,$2,$3,'1',$4,$5,'fixture',now(),now(),now()+interval '1 hour',$6)")
        .bind(id.as_uuid()).bind(candidate.as_uuid()).bind(package.as_uuid()).bind(mandate.as_uuid())
        .bind(evaluation.as_uuid()).bind(environment).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(id)
}

/// Native SQLx resolves and verifies historical migrations; only the input
/// directory is reduced. The current migrations and their checksums are intact.
pub async fn migrate_before(pool: &PgPool, exclusive_version: i64) {
    let directory = std::env::temp_dir().join(format!("quazonai-history-{}", Id::new()));
    std::fs::create_dir(&directory).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        let version: i64 = name.split('_').next().unwrap().parse().unwrap();
        if version < exclusive_version {
            std::fs::copy(entry.path(), directory.join(name)).unwrap();
        }
    }
    let old = sqlx::migrate::Migrator::new(directory.as_path())
        .await
        .unwrap();
    std::fs::remove_dir_all(directory).unwrap();
    old.run(pool).await.unwrap();
}

/// Observe a real lock wait, rather than assuming a sleep created a race.
pub async fn wait_for_database_lock(pool: &PgPool, backend: i32) {
    for _ in 0..500 {
        let waiting: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE pid=$1 AND wait_event_type='Lock')",
        )
        .bind(backend)
        .fetch_one(pool)
        .await
        .unwrap();
        if waiting {
            return;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    panic!("backend {backend} did not enter the required native lock wait");
}

/// Assert the native failure class, not just any connection/query error.
pub fn sqlstate(error: sqlx::Error, expected: &str) {
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some(expected),
        "{error:?}"
    );
}

/// A structural fixture from the evaluated run. Never REAL or deliverable.
pub async fn report_artifact(pool: &PgPool, project: Id, run: Id, attempt: Id) -> Id {
    let report = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,'REPORT','application/json','fixture.evaluation','1','LOCAL',$5,'1',1,'EVALUATOR_ONLY','FIXTURE','RUNTIME','AUDIT')")
        .bind(report.as_uuid()).bind(project.as_uuid()).bind(run.as_uuid()).bind(attempt.as_uuid())
        .bind(format!("fixture/{report}")).execute(pool).await.unwrap();
    report
}
/// Frozen approval context includes the exact reports, not the research inputs.
pub async fn approval_inputs(pool: &PgPool, f: &Fixture, evaluation: Id) -> Id {
    let inputs = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.input_sets(id,project_id,purpose,decision_cutoff) VALUES($1,$2,'PORTFOLIO',clock_timestamp())")
        .bind(inputs.as_uuid()).bind(f.project.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.input_set_items(input_set_id,artifact_id,role,ordinal) SELECT $1,artifact,'REPORT',(row_number() OVER (ORDER BY artifact)-1)::integer FROM (SELECT report_artifact_id AS artifact FROM app.evaluations WHERE id=$2 UNION SELECT method_versions_artifact_id FROM app.evaluations WHERE id=$2) reports")
        .bind(inputs.as_uuid()).bind(evaluation.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE app.input_sets SET frozen_at=clock_timestamp() WHERE id=$1")
        .bind(inputs.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    inputs
}

/// Only a relational fixture. The test database has no native market data or
/// report object; REAL metadata here is never product acceptance evidence.
pub async fn forward_report_metadata(pool: &PgPool, project: Id) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','qz.forward_report','1','LOCAL',$3,'1',32,'EVALUATOR_ONLY','REAL','OPERATOR','AUDIT')")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(format!("relational-fixture/{id}")).execute(pool).await.unwrap();
    id
}
