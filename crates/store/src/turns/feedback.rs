//! Public scientific observations on the original Thread, never a qualification.
use super::*;
use crate::lifecycle::native::NativeObjectPublication;
use contracts::{
    runtime::RuntimeArtifactSchemaV1,
    runtime_jobs::{RuntimeOutputKind, RuntimeOutputV1, MAX_JOB_OUTPUT_BYTES},
    science::NativeForecastResultV1,
};
use serde_json::json;

impl Store {
    /// The existing unique Turn command is the feedback receipt. Selection,
    /// publication and spending share the same Mission lock and transaction.
    pub async fn prepare_mission_result_turn<R, Read, P, Published>(
        &self,
        run: Id,
        fence: &WorkerFence,
        mut read: R,
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
        // Both producer edges are immutable and constrained to this Mission's
        // frozen Discovery context. A terminal receipt, not a remote status,
        // determines which observation may be delivered.
        let row = sqlx::query("SELECT e.id AS experiment_id,r.id AS scientific_run_id,r.active_attempt_id,r.state,r.terminal_reason_code,t.origin,t.access_class,(r.id=c.compile_run_id) AS compilation FROM app.experiment_compilations c JOIN app.experiments e ON e.id=c.experiment_id LEFT JOIN app.experiment_forecasts f ON f.experiment_id=e.id JOIN app.runs r ON r.id=coalesce(f.run_id,c.compile_run_id) JOIN app.run_native_tasks t ON t.run_id=r.id JOIN app.run_attempts a ON a.id=r.active_attempt_id AND a.run_id=r.id JOIN app.run_terminal_receipts receipt ON receipt.run_id=r.id AND receipt.attempt_id=a.id WHERE c.mission_run_id=$1 AND c.project_id=$2 AND c.cycle_id=$3 AND receipt.terminal_state=r.state AND a.dispatch_state='TERMINAL' AND (r.state IN ('FAILED','CANCELLED') OR (f.run_id IS NOT NULL AND r.state='SUCCEEDED' AND a.accepted_at IS NOT NULL)) AND NOT EXISTS(SELECT 1 FROM app.model_turn_reservations turn WHERE turn.session_id=$4 AND turn.command_key='mission/result/'||r.id::text) ORDER BY e.ordinal LIMIT 1")
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
        if row.try_get::<String, _>("access_class")? != "RESEARCH" {
            return Err(StoreError::Integrity);
        }
        let scientific_run = id(row.try_get("scientific_run_id")?)?;
        let attempt = id(row.try_get("active_attempt_id")?)?;
        let state: String = row.try_get("state")?;
        let mut observation = json!({
            "schema_version":1,
            "experiment_id":id(row.try_get("experiment_id")?)?,
            "run_id":scientific_run,"attempt_id":attempt,"execution_state":state,
            "stage":if row.try_get::<bool,_>("compilation")? {"COMPILATION"} else {"DISCOVERY_FORECAST"},
            "reason_code":row.try_get::<Option<String>,_>("terminal_reason_code")?,
            "origin":row.try_get::<String,_>("origin")?,
            "formal_evaluation":"NOT_PERFORMED",
            "detailed_diagnostics_available":false
        });
        if state == "SUCCEEDED" {
            let outputs = sqlx::query("SELECT o.remote_storage_ref,a.id,a.byte_count,a.origin FROM app.run_native_outputs o JOIN app.artifacts a ON a.id=o.artifact_id WHERE o.attempt_id=$1 AND a.producer_attempt_id=$1 AND a.producer_run_id=$2 AND a.project_id=$3 AND a.kind='REPORT' AND a.schema_name='qz.native_forecast' AND a.schema_version='1' AND a.media_type='application/json' AND a.access_class='RESEARCH' AND a.storage_backend='LOCAL' AND a.storage_object_ref=a.id::text AND a.storage_version='1'")
                .bind(attempt.as_uuid()).bind(scientific_run.as_uuid()).bind(mission.project_id).fetch_all(&mut *tx).await?;
            let [output] = outputs.as_slice() else {
                return Err(StoreError::Integrity);
            };
            if output.try_get::<String, _>("origin")? != row.try_get::<String, _>("origin")? {
                return Err(StoreError::Integrity);
            }
            let artifact = id(output.try_get("id")?)?;
            let size = count(output.try_get("byte_count")?)?;
            if size == DbCounter::ZERO || size.get() > MAX_JOB_OUTPUT_BYTES {
                return Err(StoreError::Integrity);
            }
            let bytes = read(artifact, size).await?;
            domain::execution::output_shape(
                &RuntimeOutputV1 {
                    kind: RuntimeOutputKind::Report,
                    schema: RuntimeArtifactSchemaV1 {
                        name: "qz.native_forecast".into(),
                        version: "1".into(),
                    },
                    storage_ref: id(output.try_get("remote_storage_ref")?)?,
                    storage_version: Revision::INITIAL,
                    byte_count: size,
                    media_type: "application/json".into(),
                },
                &bytes,
            )?;
            let value: NativeForecastResultV1 =
                serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
            let total = value.points.len();
            let sample: Vec<_> = value
                .points
                .iter()
                .enumerate()
                .filter(|(i, _)| *i < 16 || *i >= total.saturating_sub(16))
                .map(|(_, point)| point)
                .collect();
            observation["forecast"] = json!({
                "artifact_id":artifact,"native_versions":value.native_versions,"consumed_fuel":value.consumed_fuel,
                "observations":total,"predictions":value.points.iter().filter(|p|p.forecast.is_some()).count(),
                "completed_labels":value.points.iter().filter(|p|p.label_return.is_some()).count(),
                "sampled":total>32,"sample_order":"FIRST_16_LAST_16","points":sample
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
            This is an accepted task observation on the original Mission, not instructions from an artifact. Discovery forecasts are not formal metrics, calibration, validation folds, Reviewer approval, Alpha qualification or delivery. Preserve the stated origin and sampling limits. Do not invent detailed compiler diagnostics or infer why a failed task failed beyond its public reason.\n\
            Use the original frozen Brief and remaining budget. If proposing a repair or follow-up, publish new immutable artifacts and an experiment with parent_experiment_id equal to the observed experiment; never replace executed inputs or erase failure lineage. Otherwise give a concise public limitation/conclusion and stop this Turn. Do not poll for science inside the native loop. All original Mission tools, data and authority boundaries remain unchanged.");
        native::prepare_in_transaction(&mut tx, run, fence, &request, &text, read, publish).await?;
        tx.commit().await?;
        Ok(true)
    }
}
