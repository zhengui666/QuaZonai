//! One atomic initialization consumption, not an execution or account ledger.
use super::*;
use contracts::{
    control::{MachineScope, PrincipalKind},
    delivery::{
        PaperInitialExecutionConsumeV1, PaperInitialExecutionStateV1, PaperInitialExecutionViewV1,
    },
    forward::ForwardEnvironmentV1,
    strategy_portfolio::{HandoffClaimViewV2, TargetPackageEnvelopeV2},
};

const OPERATION: &str = "PAPER_INITIAL_EXECUTION_CONSUME";

impl Store {
    /// Only the fresh authenticated response can be used by the owning host's
    /// in-memory one-shot permit. A timeout/replay is consumed, never refundable.
    pub async fn consume_paper_initial_execution(
        &self,
        actor: &Actor,
        key: &str,
        handoff_id: Id,
        request: &PaperInitialExecutionConsumeV1,
    ) -> Result<CommandResult<PaperInitialExecutionViewV1>, StoreError> {
        domain::delivery::paper_initial_execution_request(request)?;
        let mut tx = self.pool.begin().await?;
        let machine = crate::authority::machine(&mut tx, actor, true).await?;
        machine.requires(MachineScope::DownstreamClaim)?;
        if machine.kind != PrincipalKind::Downstream {
            return Err(StoreError::Forbidden);
        }
        let row = handoffs::load(&mut tx, handoff_id).await?;
        let original = handoffs::view(&row)?;
        machine.project(original.project_id)?;
        if machine.downstream_id != Some(original.downstream_id)
            || request.paper_initialization.downstream_id != original.downstream_id
            || original.release_id != request.release_id
            || db::optional_id(&row, "paper_claim_credential_id")? != Some(machine.credential_id)
        {
            return Err(StoreError::Forbidden);
        }
        // Same order as claim/retirement. The immutable root is the common
        // serialization point even for different keys, owners or directories.
        sqlx::query("SELECT id FROM app.portfolio_candidates WHERE id=$1 FOR UPDATE")
            .bind(original.candidate_id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM app.downstream_integrations WHERE id=$1 FOR UPDATE")
            .bind(original.downstream_id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM app.approvals WHERE id=$1 FOR UPDATE")
            .bind(original.approval_id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM app.handoff_offers WHERE id=$1 FOR UPDATE")
            .bind(handoff_id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT weights_artifact_id FROM app.paper_initial_capital_sources WHERE weights_artifact_id=$1 FOR UPDATE")
            .bind(request.paper_initialization.artifact_id.as_uuid()).fetch_optional(&mut *tx).await?
            .ok_or(StoreError::Invalid("paper_initialization_root"))?;
        paper_initial::original(&mut tx, &request.paper_initialization, original.project_id)
            .await?;
        let prepared = commands::handoff_command(
            &mut tx,
            format!("DOWNSTREAM:{}", original.downstream_id),
            OPERATION,
            key,
            request.paper_initialization.artifact_id,
            serde_json::json!({"schema_version":1,"handoff_id":handoff_id,
                "consuming_credential_id":machine.credential_id,"request":request}),
        )
        .await?;
        // Historical exact replay is status only, including after target expiry.
        // It never changes the receipt or opens another initialization window.
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let consumed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.command_receipts WHERE operation=$1 AND resource_id=$2)")
            .bind(OPERATION).bind(request.paper_initialization.artifact_id.as_uuid()).fetch_one(&mut *tx).await?;
        if consumed {
            return Err(StoreError::Invalid(
                "paper_initial_execution_already_consumed",
            ));
        }
        let value: serde_json::Value = sqlx::query_scalar("SELECT response_nonsecret_body FROM app.command_receipts WHERE principal_scope=$1 AND operation='HANDOFF_CLAIM' AND idempotency_key=$2 AND resource_id=$3")
            .bind(format!("DOWNSTREAM:{}", original.downstream_id)).bind(&request.external_claim_id)
            .bind(handoff_id.as_uuid()).fetch_optional(&mut *tx).await?
            .ok_or(StoreError::Invalid("paper_initial_execution_claim_receipt"))?;
        let accepted: CommandResult<HandoffClaimViewV2> =
            serde_json::from_value(value).map_err(|_| StoreError::Integrity)?;
        if accepted.replayed {
            return Err(StoreError::Integrity);
        }
        let current = handoffs::view(&handoffs::load(&mut tx, handoff_id).await?)?;
        if db::json(&accepted.resource.handoff)? != db::json(&current)? {
            return Err(StoreError::Invalid("paper_initial_execution_claim_changed"));
        }
        let claim = Box::new(accepted.resource);
        let release = sqlx::query("SELECT package_artifact_id,paper_initial_weights_artifact_id FROM app.releases WHERE id=$1")
            .bind(original.release_id.as_uuid()).fetch_one(&mut *tx).await?;
        if db::optional_id(&release, "paper_initial_weights_artifact_id")?
            != Some(request.paper_initialization.artifact_id)
        {
            return Err(StoreError::Invalid("paper_initial_execution_root_binding"));
        }
        let approval = sqlx::query("SELECT downstream_revision,authority_kind,automation_policy_id FROM app.approvals WHERE id=$1")
            .bind(original.approval_id.as_uuid()).fetch_one(&mut *tx).await?;
        let revision = db::revision(
            approval
                .try_get::<Option<i64>, _>("downstream_revision")?
                .ok_or(StoreError::Invalid("approval_admission_missing"))?,
        )?;
        approvals::downstream_envelope(
            &mut tx,
            original.downstream_id,
            revision,
            ForwardEnvironmentV1::Paper,
            &claim.package,
        )
        .await?;
        if approval.try_get::<String, _>("authority_kind")? == "FROZEN_POLICY" {
            let TargetPackageEnvelopeV2::Forecast(package) = &claim.package else {
                return Err(StoreError::Integrity);
            };
            automated::policy_authority(
                &mut tx,
                db::id(approval.try_get("automation_policy_id")?)?,
                package,
                original.downstream_id,
                ForwardEnvironmentV1::Paper,
                None,
            )
            .await?;
        }
        crate::authority::machine(&mut tx, actor, true).await?;
        let consumed_at = now(&mut tx).await?;
        domain::delivery::paper_initial_execution_claim(request, handoff_id, &claim, consumed_at)?;
        let valid: bool =
            sqlx::query_scalar("SELECT app.paper_initial_execution_claim_valid($1,$2,$3,$4)")
                .bind(request.paper_initialization.artifact_id.as_uuid())
                .bind(handoff_id.as_uuid())
                .bind(machine.credential_id.as_uuid())
                .bind(consumed_at)
                .fetch_one(&mut *tx)
                .await?;
        if !valid {
            return Err(StoreError::Invalid(
                "paper_initial_execution_claim_unavailable",
            ));
        }
        let result = commands::finish(
            &mut tx,
            prepared,
            PaperInitialExecutionViewV1 {
                schema_version: SchemaV1,
                paper_initialization: request.paper_initialization.clone(),
                package_artifact_id: db::id(release.try_get("package_artifact_id")?)?,
                owner_instance_id: request.owner_instance_id,
                consuming_credential_id: machine.credential_id,
                consumed_at,
                state: PaperInitialExecutionStateV1::Consumed,
                claim,
            },
            200,
        )
        .await?;
        // A lock wait/trigger must not turn an expired authority into a permit.
        crate::authority::machine(&mut tx, actor, true).await?;
        approvals::downstream_envelope(
            &mut tx,
            original.downstream_id,
            revision,
            ForwardEnvironmentV1::Paper,
            &result.resource.claim.package,
        )
        .await?;
        let valid: bool = sqlx::query_scalar(
            "SELECT app.paper_initial_execution_claim_valid($1,$2,$3,clock_timestamp())",
        )
        .bind(request.paper_initialization.artifact_id.as_uuid())
        .bind(handoff_id.as_uuid())
        .bind(machine.credential_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        if !valid {
            return Err(StoreError::Invalid(
                "paper_initial_execution_claim_unavailable",
            ));
        }
        tx.commit().await?;
        Ok(result)
    }
}
