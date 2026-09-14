//! Original approval consumption; an offer is not a transfer or execution.
use super::*;
use contracts::{
    control::{MachineScope, PrincipalKind},
    delivery::*,
};
use sqlx::postgres::PgRow;

fn view(row: &PgRow) -> Result<HandoffViewV1, StoreError> {
    Ok(HandoffViewV1 {
        id: db::id(row.try_get("id")?)?,
        project_id: db::id(row.try_get("project_id")?)?,
        candidate_id: db::id(row.try_get("candidate_id")?)?,
        mandate_id: db::id(row.try_get("mandate_id")?)?,
        release_id: db::id(row.try_get("release_id")?)?,
        approval_id: db::id(row.try_get("approval_id")?)?,
        downstream_id: db::id(row.try_get("downstream_id")?)?,
        environment: db::enum_value(row, "environment")?,
        delivery_sequence: counter(row.try_get("delivery_sequence")?)?,
        revision: db::revision(row.try_get("revision")?)?,
        state: db::enum_value(row, "state")?,
        supersedes_handoff_id: db::optional_id(row, "supersedes_handoff_id")?,
        offered_at: row.try_get("offered_at")?,
        expires_at: row.try_get("expires_at")?,
        claimed_at: row.try_get("claimed_at")?,
        external_claim_id: row.try_get("external_claim_id")?,
        acknowledged_at: row.try_get("acknowledged_at")?,
    })
}
async fn load(tx: &mut Tx<'_>, id: Id) -> Result<PgRow, StoreError> {
    sqlx::query("SELECT h.*,c.project_id,c.id AS candidate_id,c.mandate_id FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE h.id=$1")
        .bind(id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)
}

async fn admission<R, Read>(
    tx: &mut Tx<'_>,
    approval_id: Id,
    release_id: Id,
    read: &mut R,
) -> Result<(PgRow, TargetPackageV1, DateTime<Utc>), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    // Resolve immutable binding before taking the shared project/source lock order.
    let approval = sqlx::query("SELECT * FROM app.approvals WHERE id=$1 AND release_id=$2")
        .bind(approval_id.as_uuid())
        .bind(release_id.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
    let environment = db::enum_value(&approval, "environment")?;
    let downstream = db::id(approval.try_get("downstream_id")?)?;
    let (_, candidate, package, source_until) =
        approvals::source(tx, release_id, environment, read).await?;
    // Native downstream row serializes its sequence, including offers from other projects.
    sqlx::query("SELECT id FROM app.downstream_integrations WHERE id=$1 FOR UPDATE")
        .bind(downstream.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
    sqlx::query("SELECT id FROM app.approvals WHERE id=$1 FOR UPDATE")
        .bind(approval_id.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
    if approval.try_get::<String, _>("authority_kind")? != "OPERATOR" {
        return Err(StoreError::Invalid("offer_policy_authority"));
    }
    approval
        .try_get::<Option<i64>, _>("downstream_revision")?
        .ok_or(StoreError::Invalid("approval_admission_missing"))?;
    let ordinal = approval
        .try_get::<Option<i32>, _>("decision_ordinal")?
        .ok_or(StoreError::Invalid("approval_admission_missing"))?;
    if db::optional_id(&approval, "readiness_observation_id")?.is_none() {
        return Err(StoreError::Invalid("approval_admission_missing"));
    }
    let decision=sqlx::query("SELECT ordinal,decision FROM app.release_decisions WHERE candidate_id=$1 AND downstream_id=$2 AND environment=$3 ORDER BY ordinal DESC LIMIT 1")
            .bind(candidate.as_uuid()).bind(downstream.as_uuid()).bind(db::code(&environment)?).fetch_optional(&mut **tx).await?;
    if decision
        .as_ref()
        .map(|r| r.try_get::<i32, _>("ordinal"))
        .transpose()?
        .unwrap_or(0)
        != ordinal
        || decision
            .as_ref()
            .is_some_and(|r| !matches!(r.try_get::<String, _>("decision").as_deref(), Ok("REOPEN")))
    {
        return Err(StoreError::Invalid("approval_decision_changed"));
    }
    let evidence: bool = sqlx::query_scalar("SELECT app.approval_evidence_valid($1,$2)")
        .bind(release_id.as_uuid())
        .bind(approval.try_get::<uuid::Uuid, _>("evidence_set_id")?)
        .fetch_one(&mut **tx)
        .await?;
    if !evidence {
        return Err(StoreError::Integrity);
    }
    let revoked: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT min(effective_at) FROM app.approval_revocations WHERE approval_id=$1",
    )
    .bind(approval_id.as_uuid())
    .fetch_one(&mut **tx)
    .await?;
    let mut until = source_until.min(approval.try_get("valid_until")?);
    if let Some(revoked) = revoked {
        until = until.min(revoked);
    }
    Ok((approval, package, until))
}

impl Store {
    pub async fn handoff(&self, actor: &Actor, id: Id) -> Result<HandoffViewV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let result = view(&load(&mut tx, id).await?)?;
        match actor {
            Actor::Browser { .. } => {
                crate::authority::browser(&mut tx, actor, false, false).await?
            }
            Actor::Machine { .. } => {
                let machine = crate::authority::machine(&mut tx, actor, false).await?;
                machine.project(result.project_id)?;
                match machine.kind {
                    PrincipalKind::Cli => machine.requires(MachineScope::ResearchRead)?,
                    PrincipalKind::Downstream
                        if machine.downstream_id == Some(result.downstream_id)
                            && (machine.scopes.contains(&MachineScope::DownstreamClaim)
                                || machine.scopes.contains(&MachineScope::DownstreamAck)) => {}
                    _ => return Err(StoreError::Forbidden),
                }
            }
        }
        tx.commit().await?;
        Ok(result)
    }

    pub async fn offer_handoff<R, Read>(
        &self,
        actor: &Actor,
        key: &str,
        request: &HandoffOfferV1,
        mut read: R,
    ) -> Result<CommandResult<HandoffViewV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::HandoffOffer,
            key,
            Some(request.approval_id),
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let (approval, package, until) =
            admission(&mut tx, request.approval_id, request.release_id, &mut read).await?;
        let project = package.project_id;
        let candidate = package.candidate_id;
        let downstream = db::id(approval.try_get("downstream_id")?)?;
        let environment = db::enum_value(&approval, "environment")?;
        let revision = approval.try_get::<i64, _>("downstream_revision")?;
        let duplicate:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id WHERE h.downstream_id=$1 AND h.environment=$2 AND (h.release_id=$3 OR (r.candidate_id=$4 AND h.claimed_at IS NOT NULL)))")
            .bind(downstream.as_uuid()).bind(db::code(&environment)?).bind(request.release_id.as_uuid()).bind(candidate.as_uuid()).fetch_one(&mut *tx).await?;
        if duplicate {
            return Err(StoreError::Conflict);
        }
        let latest=sqlx::query("SELECT h.id,h.state FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE c.project_id=$1 AND c.mandate_id=$2 AND h.downstream_id=$3 AND h.environment=$4 ORDER BY h.delivery_sequence DESC LIMIT 1 FOR UPDATE OF h")
            .bind(project.as_uuid()).bind(package.mandate_id.as_uuid()).bind(downstream.as_uuid()).bind(db::code(&environment)?).fetch_optional(&mut *tx).await?;
        let latest_id = latest
            .as_ref()
            .map(|r| db::id(r.try_get("id")?))
            .transpose()?;
        if latest_id != request.supersedes_handoff_id {
            return Err(StoreError::Conflict);
        }
        let sequence:i64=sqlx::query_scalar("SELECT coalesce(max(delivery_sequence),0)+1 FROM app.handoff_offers WHERE downstream_id=$1 AND environment=$2")
            .bind(downstream.as_uuid()).bind(db::code(&environment)?).fetch_one(&mut *tx).await?;
        approvals::downstream(
            &mut tx,
            downstream,
            db::revision(revision)?,
            environment,
            &package,
        )
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let offered_at = now(&mut tx).await?;
        if offered_at < package.valid_from
            || offered_at < approval.try_get::<DateTime<Utc>, _>("granted_at")?
            || request.expires_at <= offered_at
            || request.expires_at > until
        {
            return Err(StoreError::Invalid("offer_expiry"));
        }
        if let Some(previous) = latest {
            if previous.try_get::<String, _>("state")? == "OFFERED" {
                sqlx::query("UPDATE app.handoff_offers SET state='REVOKED' WHERE id=$1")
                    .bind(previous.try_get::<uuid::Uuid, _>("id")?)
                    .execute(&mut *tx)
                    .await?;
            }
        }
        let id = Id::new();
        sqlx::query("INSERT INTO app.handoff_offers(id,release_id,approval_id,downstream_id,environment,delivery_sequence,state,offered_at,expires_at,supersedes_handoff_id) VALUES($1,$2,$3,$4,$5,$6,'OFFERED',$7,$8,$9)")
            .bind(id.as_uuid()).bind(request.release_id.as_uuid()).bind(request.approval_id.as_uuid()).bind(downstream.as_uuid()).bind(db::code(&environment)?).bind(sequence).bind(offered_at).bind(request.expires_at).bind(request.supersedes_handoff_id.map(|v|v.as_uuid())).execute(&mut *tx).await?;
        approvals::downstream(
            &mut tx,
            downstream,
            db::revision(revision)?,
            environment,
            &package,
        )
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        if now(&mut tx).await? >= request.expires_at {
            return Err(StoreError::Invalid("offer_expiry"));
        }
        let resource = view(&load(&mut tx, id).await?)?;
        let result = commands::finish(&mut tx, prepared, resource, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}

impl Store {
    pub async fn claim_handoff<R, Read>(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &HandoffClaimV1,
        mut read: R,
    ) -> Result<CommandResult<HandoffClaimViewV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        commands::key(&request.external_claim_id)?;
        if key != request.external_claim_id {
            return Err(StoreError::Invalid("claim_idempotency_key"));
        }
        let mut tx = self.pool.begin().await?;
        let machine = crate::authority::machine(&mut tx, actor, true).await?;
        machine.requires(MachineScope::DownstreamClaim)?;
        if machine.kind != PrincipalKind::Downstream {
            return Err(StoreError::Forbidden);
        }
        let original = view(&load(&mut tx, id).await?)?;
        machine.project(original.project_id)?;
        if machine.downstream_id != Some(original.downstream_id) {
            return Err(StoreError::Forbidden);
        }
        sqlx::query("SELECT id FROM app.portfolio_candidates WHERE id=$1 FOR UPDATE")
            .bind(original.candidate_id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM app.downstream_integrations WHERE id=$1 FOR UPDATE")
            .bind(original.downstream_id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let prepared = commands::handoff_claim(
            &mut tx,
            format!("DOWNSTREAM:{}", original.downstream_id),
            key,
            id,
            serde_json::json!({"schema_version":1,"handoff_id":id,"request":request}),
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        if original.state != HandoffStateV1::Offered {
            return Err(StoreError::Conflict);
        }
        let (approval, package, until) = admission(
            &mut tx,
            original.approval_id,
            original.release_id,
            &mut read,
        )
        .await?;
        sqlx::query("SELECT id FROM app.handoff_offers WHERE id=$1 FOR UPDATE")
            .bind(id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let current = view(&load(&mut tx, id).await?)?;
        if current.state != HandoffStateV1::Offered {
            return Err(StoreError::Conflict);
        }
        if request.package_schema_version != package.package_schema_version {
            return Err(StoreError::Invalid("claim_package_version"));
        }
        let revision = db::revision(approval.try_get("downstream_revision")?)?;
        approvals::downstream(
            &mut tx,
            current.downstream_id,
            revision,
            current.environment,
            &package,
        )
        .await?;
        crate::authority::machine(&mut tx, actor, true).await?;
        let accepted = now(&mut tx).await?;
        if accepted < current.offered_at
            || accepted < package.valid_from
            || accepted < approval.try_get::<DateTime<Utc>, _>("granted_at")?
            || accepted >= current.expires_at
            || accepted >= until
        {
            return Err(StoreError::Invalid("claim_expiry"));
        }
        sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED',external_claim_id=$2,claimed_at=clock_timestamp() WHERE id=$1")
            .bind(id.as_uuid()).bind(&request.external_claim_id).execute(&mut *tx).await?;
        // SQL triggers record the transfer and may wait; expiry and credential checks
        // after the write still roll back state, transfer and receipt together.
        approvals::downstream(
            &mut tx,
            current.downstream_id,
            revision,
            current.environment,
            &package,
        )
        .await?;
        crate::authority::machine(&mut tx, actor, true).await?;
        let accepted = now(&mut tx).await?;
        if accepted >= current.expires_at || accepted >= until {
            return Err(StoreError::Invalid("claim_expiry"));
        }
        let handoff = view(&load(&mut tx, id).await?)?;
        let result = commands::finish(
            &mut tx,
            prepared,
            HandoffClaimViewV1 { handoff, package },
            200,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Trusted worker maintenance only. Claim independently checks the DB clock.
    pub async fn expire_handoffs(&self) -> Result<u64, StoreError> {
        let changed=sqlx::query("WITH expired AS (SELECT id FROM app.handoff_offers WHERE state='OFFERED' AND expires_at<=clock_timestamp() ORDER BY expires_at,id LIMIT 128 FOR UPDATE SKIP LOCKED) UPDATE app.handoff_offers h SET state='EXPIRED' FROM expired e WHERE h.id=e.id AND h.state='OFFERED' AND h.expires_at<=clock_timestamp()")
            .execute(&self.pool).await?;
        Ok(changed.rows_affected())
    }
}
