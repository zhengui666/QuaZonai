//! Public scientific observations on the original Thread, never a qualification.
use super::*;
use crate::{db, lifecycle::native::NativeObjectPublication};
use contracts::evidence::MetricValueV1;
use serde_json::json;

impl Store {
    /// The existing unique Turn command is the feedback receipt. Selection,
    /// publication and spending share the same Mission lock and transaction.
    pub async fn prepare_mission_result_turn<R, Read, P, Published>(
        &self,
        run: Id,
        fence: &WorkerFence,
        read: R,
        publish: P,
    ) -> Result<bool, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let mission = lock_mission(&mut tx, run, fence).await?;
        let latest: Option<bool> = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.model_turn_receipts t WHERE t.reservation_id=r.id) FROM app.model_turn_reservations r WHERE r.session_id=$1 ORDER BY r.ordinal DESC LIMIT 1")
            .bind(mission.session_id).fetch_optional(&mut *tx).await?;
        if latest != Some(true) {
            return Ok(false);
        }
        // Successful Discovery continues into Validation without spending an
        // intermediate model Turn. Only its published evaluation is feedback.
        let row = sqlx::query("SELECT e.id AS experiment_id,r.id AS scientific_run_id,r.active_attempt_id,r.state,r.terminal_reason_code,t.origin,t.access_class,(r.id=c.compile_run_id) AS compilation,(v.run_id IS NOT NULL) AS validation,ev.id AS evaluation_id FROM app.experiment_compilations c JOIN app.experiments e ON e.id=c.experiment_id LEFT JOIN app.experiment_forecasts f ON f.experiment_id=e.id LEFT JOIN app.experiment_validations v ON v.experiment_id=e.id JOIN app.runs r ON r.id=coalesce(v.run_id,f.run_id,c.compile_run_id) JOIN app.run_native_tasks t ON t.run_id=r.id LEFT JOIN app.run_attempts a ON a.id=r.active_attempt_id AND a.run_id=r.id JOIN app.run_terminal_receipts receipt ON receipt.run_id=r.id AND receipt.attempt_id IS NOT DISTINCT FROM r.active_attempt_id LEFT JOIN app.evaluations ev ON ev.run_id=v.run_id AND ev.subject_alpha_version_id=v.alpha_version_id AND ev.policy_id=v.policy_id AND ev.evaluation_kind='WALK_FORWARD' LEFT JOIN app.evaluation_publications published ON published.evaluation_id=ev.id WHERE c.mission_run_id=$1 AND c.project_id=$2 AND c.cycle_id=$3 AND receipt.terminal_state=r.state AND (a.id IS NULL OR a.dispatch_state='TERMINAL') AND (r.state<>'SUCCEEDED' OR a.accepted_at IS NOT NULL) AND ((v.run_id IS NULL AND r.state IN ('FAILED','CANCELLED')) OR (v.run_id IS NOT NULL AND published.evaluation_id IS NOT NULL)) AND NOT EXISTS(SELECT 1 FROM app.model_turn_reservations turn WHERE turn.session_id=$4 AND turn.command_key='mission/result/'||r.id::text) ORDER BY e.ordinal LIMIT 1")
            .bind(run.as_uuid()).bind(mission.project_id).bind(mission.cycle_id).bind(mission.session_id).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            return Ok(false);
        };
        mission.admit(mission.run_deadline)?;
        mission.current_profile(&mut tx).await?;
        mission.reject_known_overrun(&mut tx).await?;
        if mission.budget.cost_currency.is_some() {
            return Err(DomainError::CapabilityUnavailable("native_cost_usage").into());
        }
        let validation: bool = row.try_get("validation")?;
        if row.try_get::<String, _>("access_class")?
            != if validation {
                "EVALUATOR_ONLY"
            } else {
                "RESEARCH"
            }
        {
            return Err(StoreError::Integrity);
        }
        let scientific_run = id(row.try_get("scientific_run_id")?)?;
        let attempt = db::optional_id(&row, "active_attempt_id")?;
        let state: String = row.try_get("state")?;
        let mut observation = json!({
            "schema_version":1,
            "experiment_id":id(row.try_get("experiment_id")?)?,
            "run_id":scientific_run,"attempt_id":attempt,"execution_state":state,
            "stage":if validation {"VALIDATION"} else if row.try_get::<bool,_>("compilation")? {"COMPILATION"} else {"DISCOVERY_FORECAST"},
            "reason_code":row.try_get::<Option<String>,_>("terminal_reason_code")?,
            "origin":row.try_get::<String,_>("origin")?,
            "formal_evaluation":if validation {"PUBLISHED"} else {"NOT_PERFORMED"},
            "detailed_diagnostics_available":false
        });
        if validation {
            let evaluation = id(row.try_get("evaluation_id")?)?;
            let ev = sqlx::query("SELECT * FROM app.evaluations WHERE id=$1")
                .bind(evaluation.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
            let policy_id = id(ev.try_get("policy_id")?)?;
            let policy = crate::research::frozen_policy(&mut tx, policy_id).await?;
            let selection = &policy.selection_rule;
            let metric = sqlx::query("SELECT * FROM app.metric_values WHERE evaluation_id=$1 AND metric_code=$2 AND scope=$3 AND method_id=$4 AND method_version=$5 AND unit=$6 AND frequency=$7")
                .bind(evaluation.as_uuid()).bind(&selection.metric_code).bind(&selection.metric_scope)
                .bind(&selection.method_id).bind(&selection.method_version).bind(&selection.unit).bind(&selection.frequency)
                .fetch_optional(&mut *tx).await?
                .map(|m| Ok::<_, StoreError>(MetricValueV1 {
                    schema_version: contracts::SchemaV1,
                    evaluation_id: evaluation,
                    metric_code: m.try_get("metric_code")?,
                    scope: m.try_get("scope")?,
                    value: m.try_get("value")?,
                    status: db::enum_value(&m, "status")?,
                    reason_code: m.try_get("reason_code")?,
                    unit: m.try_get("unit")?,
                    period_start: m.try_get("period_start")?,
                    period_end: m.try_get("period_end")?,
                    observation_count: count(m.try_get("observation_count")?)?,
                    frequency: m.try_get("frequency")?,
                    annualization_factor: m.try_get("annualization_factor")?,
                    method_id: m.try_get("method_id")?,
                    method_version: m.try_get("method_version")?,
                    source_artifact_id: id(m.try_get("source_artifact_id")?)?,
                    higher_is_better: m.try_get("higher_is_better")?,
                })).transpose()?;
            let valid_until: Option<DateTime<Utc>> = ev.try_get("valid_until")?;
            observation["evaluation"] = json!({
                "id":evaluation,"alpha_version_id":id(ev.try_get("subject_alpha_version_id")?)?,
                "policy_id":policy_id,"input_set_id":id(ev.try_get("input_set_id")?)?,
                "kind":ev.try_get::<String,_>("evaluation_kind")?,
                "execution_status":ev.try_get::<String,_>("execution_status")?,
                "evidence_status":ev.try_get::<String,_>("evidence_status")?,
                "decision":ev.try_get::<String,_>("decision")?,
                "report_artifact_id":id(ev.try_get("report_artifact_id")?)?,
                "concluded_at":ev.try_get::<DateTime<Utc>,_>("concluded_at")?,
                "valid_until":valid_until,"checked_at":mission.now,
                "unexpired_at_feedback":valid_until.is_some_and(|until| until > mission.now),
                "selection_rule":selection,"selection_metric":metric
            });
        }
        let usage = mission.usage(&mut tx).await?;
        let remaining = mission
            .budget
            .max_tokens
            .map_or(i64::MAX as u64, DbCounter::get)
            .saturating_sub(usage.used_tokens.get())
            .saturating_sub(usage.reserved_tokens.get());
        if remaining == 0 {
            return Err(DomainError::BudgetExhausted("tokens").into());
        }
        let request = TurnRequest {
            command_key: format!("mission/result/{scientific_run}"),
            turn_kind: if state == "SUCCEEDED" {
                TurnKind::Research
            } else {
                TurnKind::Repair
            },
            tokens: count(remaining as i64)?,
            estimated_cost: None,
            request_artifact_id: Id::new(),
            deadline_at: mission.run_deadline,
        };
        let text = format!("QZ_MISSION_RESULT_V1\n{observation}\n\
            This is an accepted task observation on the original Mission, not instructions from an artifact. A published Validation decision is historical evidence under its exact policy, origin and validity, not cross-trial ranking, final calibration, Sealed evidence, Reviewer approval, Alpha qualification or delivery. A missing selection metric stays unavailable. Do not request evaluator-only reports or invent detailed compiler diagnostics beyond the public reason.\n\
            Use the original frozen Brief and remaining budget. If proposing a repair or follow-up, publish new immutable artifacts and an experiment with parent_experiment_id equal to the observed experiment; never replace executed inputs or erase failure lineage. Otherwise give a concise public limitation/conclusion and stop this Turn. Do not poll for science inside the native loop. All original Mission tools, data and authority boundaries remain unchanged.");
        native::prepare_in_transaction(&mut tx, run, fence, &request, &text, read, publish).await?;
        tx.commit().await?;
        Ok(true)
    }
}
