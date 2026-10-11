//! Read-only original Agent execution facts. No launch configuration or writer.
use crate::{
    authority::{self, Actor},
    db, Store, StoreError,
};
use contracts::{
    control::{ListQuery, MachineScope, Page},
    Id,
};
use serde_json::Value;

impl Store {
    /// Original identity/model/receipt metadata only; not reconstructed conversation
    /// history. Artifact bytes remain behind their existing access-class checks.
    pub async fn historical_agent_run(&self, actor: &Actor, run: Id) -> Result<Value, StoreError> {
        let snapshot = self.get_run(actor, run).await?;
        if snapshot.kind != contracts::runs::RunKind::AgentResearch {
            return Err(StoreError::NotFound);
        }
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, snapshot.project_id, MachineScope::RunRead).await?;
        let session: Option<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',s.id,'run_id',s.run_id,'cycle_id',s.cycle_id,'role',s.role,'profile_id',s.profile_id,'profile_revision',s.profile_revision::text,'thread_id',s.thread_id,'codex_version',s.codex_version,'protocol_schema_version',s.protocol_schema_version,'observed_model',s.observed_model,'observed_provider',s.observed_provider,'observed_effort',s.observed_effort,'observed_service_tier',s.observed_service_tier,'created_at',s.created_at) FROM app.codex_sessions s WHERE s.run_id=$1")
            .bind(run.as_uuid()).fetch_optional(&mut *tx).await?;
        tx.commit().await?;
        Ok(
            serde_json::json!({"schema_version":1,"source":"CODEX","run":snapshot,"session":session}),
        )
    }

    /// Bounded original turn facts. Absent terminal/usage stays absent; no inferred
    /// cancellation, fake zero usage, replays, prompt generation or summary rewriting.
    pub async fn historical_agent_turns(
        &self,
        actor: &Actor,
        run: Id,
        query: &ListQuery,
    ) -> Result<Page<Value>, StoreError> {
        domain::control::list(query)?;
        let snapshot = self.get_run(actor, run).await?;
        if snapshot.kind != contracts::runs::RunKind::AgentResearch {
            return Err(StoreError::NotFound);
        }
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, snapshot.project_id, MachineScope::RunRead).await?;
        let values: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',r.id,'run_id',r.run_id,'session_id',r.session_id,'ordinal',r.ordinal::text,'turn_kind',r.turn_kind,'reserved_tokens',r.reserved_tokens::text,'reserved_cost',r.reserved_cost::text,'cost_currency',r.cost_currency,'created_at',r.created_at,'native_turn_id',b.native_turn_id,'terminal',to_jsonb(t)-'reservation_id','usage',to_jsonb(u)-'reservation_id'-'actual_tokens'-'actual_cost' || CASE WHEN u.reservation_id IS NULL THEN NULL ELSE jsonb_build_object('actual_tokens',u.actual_tokens::text,'actual_cost',u.actual_cost::text) END,'summary_artifact_id',summary.artifact_id) FROM app.model_turn_reservations r LEFT JOIN app.model_turn_bindings b ON b.reservation_id=r.id LEFT JOIN app.model_turn_terminals t ON t.reservation_id=r.id LEFT JOIN app.model_turn_receipts u ON u.reservation_id=r.id LEFT JOIN app.model_turn_summaries summary ON summary.reservation_id=r.id AND EXISTS(SELECT 1 FROM app.artifacts a WHERE a.id=summary.artifact_id AND a.access_class='RESEARCH') WHERE r.run_id=$1 AND ($2::uuid IS NULL OR r.id<$2) ORDER BY r.id DESC LIMIT $3")
            .bind(run.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let mut result = Vec::with_capacity(values.len());
        for value in values {
            let id = Id::try_from(
                value
                    .get("id")
                    .and_then(Value::as_str)
                    .ok_or(StoreError::Integrity)?
                    .to_owned(),
            )
            .map_err(|_| StoreError::Integrity)?;
            result.push((id, value));
        }
        let paged = crate::control::page(result, query.limit, |entry| entry.0);
        tx.commit().await?;
        Ok(Page {
            schema_version: paged.schema_version,
            items: paged.items.into_iter().map(|entry| entry.1).collect(),
            next_cursor: paged.next_cursor,
        })
    }

    /// Original saved profile metadata, never host paths, credentials or startup options.
    pub async fn historical_agent_profile(
        &self,
        actor: &Actor,
        profile: Id,
    ) -> Result<Value, StoreError> {
        let mut tx = self.pool.begin().await?;
        crate::settings::read_authority(&mut tx, actor).await?;
        let value = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'name',name,'revision',revision::text,'created_at',created_at,'updated_at',updated_at,'connection_mode',connection_mode,'profile_origin',profile_origin,'use_default_model_settings',use_default_model_settings,'saved_model',saved_model,'saved_reasoning_effort',saved_reasoning_effort,'saved_fast_mode',saved_fast_mode) FROM app.codex_profiles WHERE id=$1")
            .bind(profile.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        tx.commit().await?;
        Ok(value)
    }

    /// Original degradation/cooldown facts are workflow inputs, not an implicit scheduler.
    pub async fn historical_degradation_wakes(
        &self,
        actor: &Actor,
        project: Id,
        query: &ListQuery,
    ) -> Result<Page<Value>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        authority::read_project(&mut tx, actor, project, MachineScope::ResearchRead).await?;
        let rows: Vec<(uuid::Uuid, Value)> = sqlx::query_as("SELECT id,to_jsonb(w) FROM app.wake_events w WHERE project_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let rows = rows
            .into_iter()
            .map(|(id, value)| Ok((db::id(id)?, value)))
            .collect::<Result<Vec<_>, StoreError>>()?;
        let paged = crate::control::page(rows, query.limit, |entry| entry.0);
        tx.commit().await?;
        Ok(Page {
            schema_version: paged.schema_version,
            items: paged.items.into_iter().map(|entry| entry.1).collect(),
            next_cursor: paged.next_cursor,
        })
    }
}
