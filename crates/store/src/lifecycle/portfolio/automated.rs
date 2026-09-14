//! Trusted Paper-stage consumption of an original human-frozen policy.
use super::*;
use contracts::{delivery::*, forward::ForwardEnvironmentV1};

pub(super) async fn policy_authority(
    tx: &mut Tx<'_>,
    id: Id,
    package: &TargetPackageV1,
    downstream: Id,
    environment: ForwardEnvironmentV1,
) -> Result<DateTime<Utc>, StoreError> {
    // Live adoption needs the original Forward promotion proof, not a Paper success.
    if environment != ForwardEnvironmentV1::Paper {
        return Err(StoreError::Invalid("automation_live_evidence_required"));
    }
    let row=sqlx::query("SELECT policy.*,p.state,p.current_automation_policy_id FROM app.automation_policies policy JOIN app.projects p ON p.id=policy.project_id WHERE policy.id=$1 FOR UPDATE OF policy")
        .bind(id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    let policy = crate::automation::view(&row)?;
    domain::delivery::automation_policy(&policy.content)?;
    if policy.project_id != package.project_id
        || policy.content.mandate_id != package.mandate_id
        || policy.content.downstream_id != downstream
        || row.try_get::<String, _>("state")? != "ACTIVE"
        || db::optional_id(&row, "current_automation_policy_id")? != Some(id)
        || policy.content.mode == AutomationModeV1::Manual
        || !policy.content.enabled_for_new_rebalances
    {
        return Err(StoreError::Invalid("automation_authority"));
    }
    // Historical SQL/import rows do not acquire human provenance by a UUID copy.
    let native:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.command_receipts WHERE operation='POLICY_AUTHORIZE' AND resource_id=$1 AND response_nonsecret_body->'resource'=$2)")
        .bind(policy.project_id.as_uuid()).bind(db::json(&policy)?).fetch_one(&mut **tx).await?;
    if !native {
        return Err(StoreError::Invalid("automation_authorization_missing"));
    }
    let revoked: Option<DateTime<Utc>> = sqlx::query_scalar(
        "SELECT min(effective_at) FROM app.policy_revocations WHERE automation_policy_id=$1",
    )
    .bind(id.as_uuid())
    .fetch_one(&mut **tx)
    .await?;
    let until = revoked.map_or(policy.content.valid_until, |v| {
        v.min(policy.content.valid_until)
    });
    let current = now(tx).await?;
    if current < policy.authorized_at || current >= until {
        return Err(StoreError::Invalid("automation_expiry"));
    }
    Ok(until)
}
async fn daily_candidates(tx: &mut Tx<'_>, project: Id, downstream: Id) -> Result<i64, StoreError> {
    Ok(sqlx::query_scalar("SELECT count(DISTINCT r.candidate_id) FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE c.project_id=$1 AND h.downstream_id=$2 AND h.offered_at>=(date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC') AND h.offered_at<(date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC')+interval '1 day'")
        .bind(project.as_uuid()).bind(downstream.as_uuid()).fetch_one(&mut **tx).await?)
}
impl Store {
    /// Round-robin scheduling metadata only; this read grants no policy authority.
    pub async fn automation_project_after(
        &self,
        cursor: Option<Id>,
    ) -> Result<Option<Id>, StoreError> {
        let id:Option<uuid::Uuid>=sqlx::query_scalar("SELECT p.id FROM app.projects p JOIN app.automation_policies policy ON policy.id=p.current_automation_policy_id AND policy.project_id=p.id WHERE p.state='ACTIVE' AND policy.mode<>'MANUAL' AND policy.enabled_for_new_rebalances AND policy.valid_until>clock_timestamp() AND ($1::uuid IS NULL OR p.id>$1) ORDER BY p.id LIMIT 1")
            .bind(cursor.map(Id::as_uuid)).fetch_optional(&self.pool).await?;
        id.map(db::id).transpose()
    }
    /// No client or Agent route. Approval and Offer are atomic native facts.
    pub async fn automate_paper<R, Read>(
        &self,
        project: Id,
        mut read: R,
    ) -> Result<Option<HandoffViewV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let project_row=sqlx::query("SELECT current_automation_policy_id FROM app.projects WHERE id=$1 AND state='ACTIVE' FOR UPDATE SKIP LOCKED").bind(project.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(project_row) = project_row else {
            tx.commit().await?;
            return Ok(None);
        };
        let Some(policy_id) = db::optional_id(&project_row, "current_automation_policy_id")? else {
            tx.commit().await?;
            return Ok(None);
        };
        let policy =
            sqlx::query("SELECT * FROM app.automation_policies WHERE id=$1 AND project_id=$2")
                .bind(policy_id.as_uuid())
                .bind(project.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        let policy = crate::automation::view(&policy)?;
        let downstream = policy.content.downstream_id;
        let latest=sqlx::query("SELECT r.id,r.candidate_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE c.project_id=$1 AND c.mandate_id=$2 ORDER BY r.id DESC LIMIT 1").bind(project.as_uuid()).bind(policy.content.mandate_id.as_uuid()).fetch_optional(&mut *tx).await?;
        let Some(latest) = latest else {
            tx.commit().await?;
            return Ok(None);
        };
        let release_id = db::id(latest.try_get("id")?)?;
        let candidate = db::id(latest.try_get("candidate_id")?)?;
        let offered:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id WHERE r.candidate_id=$1 AND h.downstream_id=$2 AND h.environment='PAPER')").bind(candidate.as_uuid()).bind(downstream.as_uuid()).fetch_one(&mut *tx).await?;
        if offered {
            tx.commit().await?;
            return Ok(None);
        }
        let (_, _, package, source_until) =
            approvals::source(&mut tx, release_id, ForwardEnvironmentV1::Paper, &mut read).await?;
        let revision: i64 = sqlx::query_scalar(
            "SELECT revision FROM app.downstream_integrations WHERE id=$1 FOR UPDATE",
        )
        .bind(downstream.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let revision = db::revision(revision)?;
        let until = source_until.min(
            policy_authority(
                &mut tx,
                policy_id,
                &package,
                downstream,
                ForwardEnvironmentV1::Paper,
            )
            .await?,
        );
        if daily_candidates(&mut tx, project, downstream).await?
            >= i64::from(policy.content.max_rebalances_per_day)
        {
            return Err(StoreError::Invalid("automation_daily_quota"));
        }
        let latest_decision:Option<uuid::Uuid>=sqlx::query_scalar("SELECT id FROM app.release_decisions WHERE candidate_id=$1 AND downstream_id=$2 AND environment='PAPER' ORDER BY ordinal DESC LIMIT 1").bind(candidate.as_uuid()).bind(downstream.as_uuid()).fetch_optional(&mut *tx).await?;
        let ordinal = approvals::decision(
            &mut tx,
            candidate,
            downstream,
            ForwardEnvironmentV1::Paper,
            latest_decision.map(db::id).transpose()?,
        )
        .await?;
        let probe = approvals::downstream(
            &mut tx,
            downstream,
            revision,
            ForwardEnvironmentV1::Paper,
            &package,
        )
        .await?;
        let granted_at = now(&mut tx).await?;
        if granted_at < package.valid_from || granted_at >= until {
            return Err(StoreError::Invalid("automation_expiry"));
        }
        let evidence = approvals::freeze_evidence(&mut tx, &package, granted_at).await?;
        let approval = Id::new();
        sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,automation_policy_id,evidence_set_id,granted_at,valid_until,downstream_revision,decision_ordinal,readiness_observation_id) VALUES($1,$2,'PAPER',$3,'FROZEN_POLICY',$4,$5,$6,$7,$8,$9,$10)")
            .bind(approval.as_uuid()).bind(release_id.as_uuid()).bind(downstream.as_uuid()).bind(policy_id.as_uuid()).bind(evidence.as_uuid()).bind(granted_at).bind(until).bind(revision.get() as i64).bind(ordinal).bind(probe.id.as_uuid()).execute(&mut *tx).await?;
        let previous:Option<uuid::Uuid>=sqlx::query_scalar("SELECT h.id FROM app.handoff_offers h JOIN app.releases r ON r.id=h.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE c.project_id=$1 AND c.mandate_id=$2 AND h.downstream_id=$3 AND h.environment='PAPER' ORDER BY h.delivery_sequence DESC LIMIT 1")
            .bind(project.as_uuid()).bind(package.mandate_id.as_uuid()).bind(downstream.as_uuid()).fetch_optional(&mut *tx).await?;
        let result = handoffs::insert_offer(
            &mut tx,
            &HandoffOfferV1 {
                schema_version: SchemaV1,
                release_id,
                approval_id: approval,
                supersedes_handoff_id: previous.map(db::id).transpose()?,
                expires_at: until,
            },
            &mut read,
        )
        .await?;
        policy_authority(
            &mut tx,
            policy_id,
            &package,
            downstream,
            ForwardEnvironmentV1::Paper,
        )
        .await?;
        if now(&mut tx).await? >= until {
            return Err(StoreError::Invalid("automation_expiry"));
        }
        tx.commit().await?;
        Ok(Some(result))
    }
}
