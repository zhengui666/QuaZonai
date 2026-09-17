//! Freeze the original trial ledger in the Mission ACK transaction. PostgreSQL
//! owns sorting and membership; no new queue, estimator, or mutable winner list.
use crate::{authority::Actor, control::page, db, evidence, Store, StoreError};
use contracts::{
    control::{ListQuery, Page},
    cycles::{CycleSelectionTrialV1, CycleSelectionV1},
    research::SelectionEvaluationKind,
    runs::RunSnapshotV1,
    DbCounter, Id, SchemaV1,
};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
type Tx<'a> = Transaction<'a, Postgres>;

/// Caller already holds project -> cycle -> Run. Other Cycle writers also take
/// the project lock, so this captures one complete registered Family history.
pub(crate) async fn freeze(tx: &mut Tx<'_>, run: &RunSnapshotV1) -> Result<(), StoreError> {
    let researcher: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM app.run_missions WHERE run_id=$1 AND role='RESEARCHER')",
    )
    .bind(run.id.as_uuid())
    .fetch_one(&mut **tx)
    .await?;
    if !researcher {
        return Ok(());
    }
    let cycle = run.cycle_id.ok_or(StoreError::Integrity)?;
    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.cycle_selections WHERE cycle_id=$1 AND research_run_id=$2)")
        .bind(cycle.as_uuid()).bind(run.id.as_uuid()).fetch_one(&mut **tx).await?;
    if exists {
        return Ok(());
    }
    // Cancellation may settle before independent Validation publication. ACK
    // must wait for it, not freeze a false absence or resubmit the scientific job.
    let pending: bool = sqlx::query_scalar(sqlx::AssertSqlSafe(format!("SELECT EXISTS(
        SELECT 1 FROM app.runs r JOIN app.run_native_tasks n ON n.run_id=r.id
        WHERE r.cycle_id=$1 AND (NOT EXISTS(
            SELECT 1 FROM app.run_terminal_receipts t LEFT JOIN app.run_attempts a ON a.id=t.attempt_id
            WHERE t.run_id=r.id AND t.terminal_state=r.state AND t.attempt_id IS NOT DISTINCT FROM r.active_attempt_id
              AND (a.id IS NULL OR a.dispatch_state='TERMINAL') AND (r.state<>'SUCCEEDED' OR a.accepted_at IS NOT NULL))
          OR EXISTS(SELECT 1 FROM app.experiment_validations v WHERE v.run_id=r.id
              AND NOT EXISTS(SELECT 1 FROM ({}) ev WHERE ev.run_id=v.run_id))))", evidence::EVALUATION)))
        .bind(cycle.as_uuid()).fetch_one(&mut **tx).await?;
    if pending {
        return Err(StoreError::Conflict);
    }
    let policy_id: uuid::Uuid = sqlx::query_scalar("SELECT b.evaluation_policy_id FROM app.research_cycles c JOIN app.research_briefs b ON b.id=c.brief_id AND b.state='FROZEN' WHERE c.id=$1")
        .bind(cycle.as_uuid()).fetch_one(&mut **tx).await?;
    let policy = crate::research::frozen_policy(tx, db::id(policy_id)?).await?;
    if policy.selection_rule.evaluation_kind != SelectionEvaluationKind::WalkForward {
        return Err(domain::DomainError::CapabilityUnavailable("sealed_trial_selection").into());
    }
    // Do not compare the materialized Run InputSet UUID: it also contains that
    // trial's own MODEL/PARAMETERS. Compare the original frozen base instead.
    let query = format!(
        r#"
WITH rule AS (SELECT $3::jsonb AS value), formal AS ({}), observed AS (
 SELECT e.id AS experiment_id,e.cycle_id AS source_cycle_id,c.compile_run_id,
   f.run_id AS discovery_run_id,v.run_id AS validation_run_id,av.id AS alpha_version_id,
   ev.id AS evaluation_id,m.id AS selection_metric_id,r.id AS execution_run_id,r.state AS execution_state,m.value,
   CASE WHEN ev.subject_alpha_version_id=av.id AND ev.decision='PASS' AND ev.valid_until>clock_timestamp()
     THEN CASE WHEN av.signal_kind='EXPECTED_RETURN' AND av.calibration_id IS NULL THEN av.id
       WHEN av.signal_kind='SCORE' AND calibration.id IS NOT NULL THEN reviewed.id END
     END AS review_alpha_version_id,
   CASE
    WHEN x.validation_input_set_id IS DISTINCT FROM (rule.value->>'comparison_input_set_id')::uuid
      OR b.execution_assumptions_id IS DISTINCT FROM (rule.value->>'execution_assumptions_id')::uuid
      OR p.selection_rule->>'comparison_input_set_id' IS DISTINCT FROM rule.value->>'comparison_input_set_id'
      OR p.selection_rule->>'execution_assumptions_id' IS DISTINCT FROM rule.value->>'execution_assumptions_id'
      OR p.selection_rule->>'evaluation_kind' IS DISTINCT FROM rule.value->>'evaluation_kind' THEN 'INCOMPARABLE_INPUT'
    WHEN ev.id IS NULL AND (
      r.state NOT IN ('SUCCEEDED','FAILED','CANCELLED')
      OR (v.run_id IS NOT NULL AND r.state IN ('SUCCEEDED','FAILED','CANCELLED'))
      OR (e.cycle_id<>$1 AND e.outcome='PENDING' AND (r.id IS NULL OR r.state='SUCCEEDED')
        AND original.state NOT IN ('COMPLETED','CANCELLED','FAILED')
        AND NOT EXISTS(SELECT 1 FROM app.run_missions mission JOIN app.run_terminal_receipts t ON t.run_id=mission.run_id
          WHERE mission.cycle_id=e.cycle_id AND mission.role='RESEARCHER'))
    ) THEN 'UNFINISHED'
    WHEN r.id IS NULL THEN 'NOT_EXECUTED'
    WHEN r.state='FAILED' THEN 'EXECUTION_FAILED'
    WHEN r.state='CANCELLED' THEN 'EXECUTION_CANCELLED'
    WHEN ev.id IS NULL THEN 'NO_FORMAL_EVALUATION'
    WHEN ev.execution_status<>'SUCCEEDED' OR ev.evidence_status<>'VALID' THEN 'INVALID_EVIDENCE'
    WHEN EXISTS(SELECT 1 FROM jsonb_array_elements(p.metric_requirements) required
      WHERE (required->>'required')::boolean AND NOT EXISTS(SELECT 1 FROM app.metric_values actual
        WHERE actual.evaluation_id=ev.id AND actual.metric_code=required->>'metric_code'
          AND actual.scope=required->>'scope' AND actual.status='OK'
          AND actual.observation_count>=(required->>'minimum_observations')::bigint
          AND required->'method_allowlist' ? actual.method_id)) THEN 'REQUIRED_METRIC_MISSING'
    WHEN m.id IS NULL OR m.status<>'OK' OR NOT (m.value>'-Infinity'::float8 AND m.value<'Infinity'::float8)
      THEN 'SELECTION_METRIC_MISSING'
    ELSE 'ELIGIBLE' END AS reason
 FROM app.experiments e
 JOIN app.experiment_families family ON family.id=e.family_id AND family.project_id=e.project_id
 JOIN app.research_cycles original ON original.id=e.cycle_id
 JOIN app.research_briefs b ON b.id=original.brief_id
 JOIN app.evaluation_policies p ON p.id=b.evaluation_policy_id
 LEFT JOIN app.brief_execution_contexts x ON x.brief_id=b.id
 LEFT JOIN app.experiment_compilations c ON c.experiment_id=e.id
 LEFT JOIN app.experiment_forecasts f ON f.experiment_id=e.id
 LEFT JOIN app.experiment_validations v ON v.experiment_id=e.id
 LEFT JOIN app.command_receipts created ON created.principal_scope='MISSION:'||c.mission_run_id::text
   AND created.operation='RESEARCH_ALPHA_CREATE' AND created.idempotency_key=e.id::text
 LEFT JOIN app.alpha_versions av ON av.id=created.resource_id AND av.experiment_id=e.id AND av.project_id=e.project_id
 LEFT JOIN app.runs r ON r.id=coalesce(v.run_id,f.run_id,c.compile_run_id,e.run_id)
 LEFT JOIN formal ev ON ev.run_id=v.run_id AND ev.subject_alpha_version_id=v.alpha_version_id AND ev.policy_id=v.policy_id
 LEFT JOIN app.alpha_versions reviewed ON reviewed.alpha_id=av.alpha_id AND reviewed.version=av.version+1
 LEFT JOIN app.calibrations calibration ON calibration.id=reviewed.calibration_id AND calibration.validation_evaluation_id=ev.id
 CROSS JOIN rule
 LEFT JOIN app.metric_values m ON m.evaluation_id=ev.id AND m.metric_code=rule.value->>'metric_code'
   AND m.scope=rule.value->>'metric_scope' AND m.method_id=rule.value->>'method_id'
   AND m.method_version=rule.value->>'method_version' AND m.unit=rule.value->>'unit' AND m.frequency=rule.value->>'frequency'
 WHERE e.project_id=$2 AND e.family_id=(rule.value->>'family_id')::uuid
   AND family.root_lineage_id=(rule.value->>'root_lineage_id')::uuid
), ranked AS (
 SELECT experiment_id,row_number() OVER(ORDER BY
   CASE WHEN $3->>'direction'='MAXIMIZE' THEN value END DESC,
   CASE WHEN $3->>'direction'='MINIMIZE' THEN value END ASC,experiment_id ASC) AS rank
 FROM observed WHERE reason='ELIGIBLE'
)
INSERT INTO app.cycle_selection_trials(cycle_id,experiment_id,source_cycle_id,compile_run_id,
 discovery_run_id,validation_run_id,alpha_version_id,evaluation_id,selection_metric_id,
 execution_run_id,execution_state,reason,rank,selected,unfinished,review_alpha_version_id)
SELECT $1,o.experiment_id,o.source_cycle_id,o.compile_run_id,o.discovery_run_id,o.validation_run_id,
 o.alpha_version_id,o.evaluation_id,o.selection_metric_id,o.execution_run_id,o.execution_state,o.reason,r.rank,
 coalesce(r.rank<=($3->>'candidate_count')::integer,false),o.reason='UNFINISHED',
 CASE WHEN r.rank<=($3->>'candidate_count')::integer THEN o.review_alpha_version_id END
FROM observed o LEFT JOIN ranked r USING(experiment_id)
"#,
        evidence::EVALUATION
    );
    sqlx::query(sqlx::AssertSqlSafe(query))
        .bind(cycle.as_uuid())
        .bind(run.project_id.as_uuid())
        .bind(db::json(&policy.selection_rule)?)
        .execute(&mut **tx)
        .await?;
    sqlx::query("INSERT INTO app.cycle_selections(cycle_id,project_id,research_run_id,policy_id,status,trial_count,eligible_count,selected_count,unfinished_count)
        SELECT $1,$2,$3,$4,CASE WHEN count(*) FILTER(WHERE selected)=$5 AND count(*) FILTER(WHERE unfinished)=0 THEN 'COMPLETE' ELSE 'INCONCLUSIVE' END,
          count(*),count(rank),count(*) FILTER(WHERE selected),count(*) FILTER(WHERE unfinished)
        FROM app.cycle_selection_trials WHERE cycle_id=$1")
        .bind(cycle.as_uuid()).bind(run.project_id.as_uuid()).bind(run.id.as_uuid()).bind(policy_id)
        .bind(i64::from(policy.selection_rule.candidate_count)).execute(&mut **tx).await?;
    Ok(())
}

fn count(value: i64) -> Result<DbCounter, StoreError> {
    DbCounter::new(u64::try_from(value).map_err(|_| StoreError::Integrity)?)
        .map_err(|_| StoreError::Integrity)
}

async fn read(tx: &mut Tx<'_>, actor: &Actor, cycle: Id) -> Result<CycleSelectionV1, StoreError> {
    let project: uuid::Uuid =
        sqlx::query_scalar("SELECT project_id FROM app.research_cycles WHERE id=$1")
            .bind(cycle.as_uuid())
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::NotFound)?;
    evidence::authorize(tx, actor, db::id(project)?).await?;
    let row = sqlx::query("SELECT s.*,p.selection_rule FROM app.cycle_selections s JOIN app.evaluation_policies p ON p.id=s.policy_id WHERE s.cycle_id=$1")
        .bind(cycle.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    Ok(CycleSelectionV1 {
        schema_version: SchemaV1,
        cycle_id: cycle,
        project_id: db::id(project)?,
        research_run_id: db::id(row.try_get("research_run_id")?)?,
        policy_id: db::id(row.try_get("policy_id")?)?,
        rule: serde_json::from_value(row.try_get("selection_rule")?)
            .map_err(|_| StoreError::Integrity)?,
        created_at: row.try_get("created_at")?,
        status: db::enum_value(&row, "status")?,
        trial_count: count(row.try_get("trial_count")?)?,
        eligible_count: count(row.try_get("eligible_count")?)?,
        selected_count: count(row.try_get("selected_count")?)?,
        unfinished_count: count(row.try_get("unfinished_count")?)?,
    })
}

fn trial(row: &PgRow) -> Result<CycleSelectionTrialV1, StoreError> {
    Ok(CycleSelectionTrialV1 {
        schema_version: SchemaV1,
        cycle_id: db::id(row.try_get("cycle_id")?)?,
        experiment_id: db::id(row.try_get("experiment_id")?)?,
        source_cycle_id: db::id(row.try_get("source_cycle_id")?)?,
        execution_run_id: db::optional_id(row, "execution_run_id")?,
        compile_run_id: db::optional_id(row, "compile_run_id")?,
        discovery_run_id: db::optional_id(row, "discovery_run_id")?,
        validation_run_id: db::optional_id(row, "validation_run_id")?,
        alpha_version_id: db::optional_id(row, "alpha_version_id")?,
        review_alpha_version_id: db::optional_id(row, "review_alpha_version_id")?,
        evaluation_id: db::optional_id(row, "original_evaluation_id")?,
        execution_state: row
            .try_get::<Option<String>, _>("execution_state")?
            .map(|v| {
                serde_json::from_value(serde_json::Value::String(v))
                    .map_err(|_| StoreError::Integrity)
            })
            .transpose()?,
        reason: db::enum_value(row, "reason")?,
        rank: row
            .try_get::<Option<i64>, _>("rank")?
            .map(count)
            .transpose()?,
        selected: row.try_get("selected")?,
        unfinished: row.try_get("unfinished")?,
        selection_metric: row
            .try_get::<Option<uuid::Uuid>, _>("selection_metric_id")?
            .map(|_| evidence::metric(row))
            .transpose()?,
    })
}

impl Store {
    pub async fn cycle_selection(
        &self,
        actor: &Actor,
        cycle: Id,
    ) -> Result<CycleSelectionV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let result = read(&mut tx, actor, cycle).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn cycle_selection_trials(
        &self,
        actor: &Actor,
        cycle: Id,
        query: &ListQuery,
    ) -> Result<Page<CycleSelectionTrialV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        read(&mut tx, actor, cycle).await?;
        let rows = sqlx::query("SELECT t.*,t.evaluation_id AS original_evaluation_id,m.* FROM app.cycle_selection_trials t LEFT JOIN app.metric_values m ON m.id=t.selection_metric_id WHERE t.cycle_id=$1 AND ($2::uuid IS NULL OR t.experiment_id>$2) ORDER BY t.experiment_id ASC LIMIT $3")
            .bind(cycle.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let result = page(
            rows.iter().map(trial).collect::<Result<Vec<_>, _>>()?,
            query.limit,
            |v| v.experiment_id,
        );
        tx.commit().await?;
        Ok(result)
    }
}
