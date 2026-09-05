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
        .bind(policy.as_uuid()).bind(project.as_uuid()).bind(json!({"schema_version":1})).execute(&mut *tx).await.unwrap();
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
    Fixture {
        project,
        cycle,
        run,
        session,
        profile,
        input_set,
        artifact,
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
