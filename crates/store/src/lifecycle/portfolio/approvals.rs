//! Original Release authority, source eligibility and exact human admission.
use super::*;
use contracts::{delivery::*, forward::ForwardEnvironmentV1};
use sqlx::postgres::PgRow;

fn view(row: &PgRow) -> Result<ApprovalViewV1, StoreError> {
    Ok(ApprovalViewV1 {
        id: db::id(row.try_get("id")?)?,
        created_at: row.try_get("created_at")?,
        project_id: db::id(row.try_get("project_id")?)?,
        candidate_id: db::id(row.try_get("candidate_id")?)?,
        release_id: db::id(row.try_get("release_id")?)?,
        downstream_id: db::id(row.try_get("downstream_id")?)?,
        environment: db::enum_value(row, "environment")?,
        authority_kind: row.try_get("authority_kind")?,
        automation_policy_id: db::optional_id(row, "automation_policy_id")?,
        evidence_set_id: db::id(row.try_get("evidence_set_id")?)?,
        granted_at: row.try_get("granted_at")?,
        valid_until: row.try_get("valid_until")?,
        downstream_revision: row
            .try_get::<Option<i64>, _>("downstream_revision")?
            .map(db::revision)
            .transpose()?,
        decision_ordinal: row
            .try_get::<Option<i32>, _>("decision_ordinal")?
            .map(|v| u32::try_from(v).map_err(|_| StoreError::Integrity))
            .transpose()?,
        readiness_observation_id: db::optional_id(row, "readiness_observation_id")?,
    })
}

pub(super) async fn downstream(
    tx: &mut Tx<'_>,
    downstream_id: Id,
    revision: contracts::Revision,
    environment: ForwardEnvironmentV1,
    package: &TargetPackageV1,
) -> Result<DownstreamProbeViewV1, StoreError> {
    let readiness = crate::downstream::readiness(tx, downstream_id).await?;
    if readiness.integration_revision != revision {
        return Err(StoreError::RevisionConflict {
            current: readiness.integration_revision,
        });
    }
    if readiness.state != DownstreamReadinessState::Available
        || !readiness.available_environments.contains(&environment)
        || !readiness
            .available_package_versions
            .contains(&package.package_schema_version)
    {
        return Err(
            domain::DomainError::CapabilityUnavailable("downstream_delivery_unavailable").into(),
        );
    }
    let probe = readiness.latest_observation.ok_or(StoreError::Integrity)?;
    let DownstreamProbeOutcomeV1::Available { capabilities } = &probe.outcome else {
        return Err(StoreError::Integrity);
    };
    if !package
        .compatible_market_capabilities
        .iter()
        .all(|v| capabilities.market_capability_versions.contains(v))
    {
        return Err(
            domain::DomainError::CapabilityUnavailable("downstream_market_contract").into(),
        );
    }
    Ok(probe)
}

pub(super) async fn decision(
    tx: &mut Tx<'_>,
    candidate: Id,
    downstream: Id,
    environment: ForwardEnvironmentV1,
    expected: Option<Id>,
) -> Result<i32, StoreError> {
    let row=sqlx::query("SELECT id,ordinal,decision FROM app.release_decisions WHERE candidate_id=$1 AND downstream_id=$2 AND environment=$3 ORDER BY ordinal DESC LIMIT 1")
        .bind(candidate.as_uuid()).bind(downstream.as_uuid()).bind(db::code(&environment)?).fetch_optional(&mut **tx).await?;
    let id = row.as_ref().map(|r| db::id(r.try_get("id")?)).transpose()?;
    if id != expected {
        return Err(StoreError::Conflict);
    }
    if let Some(row) = row {
        if row.try_get::<String, _>("decision")? != "REOPEN" {
            return Err(StoreError::Invalid("candidate_rejected"));
        }
        Ok(row.try_get("ordinal")?)
    } else {
        Ok(0)
    }
}

pub(super) async fn source<R, Read>(
    tx: &mut Tx<'_>,
    release_id: Id,
    environment: ForwardEnvironmentV1,
    read: &mut R,
) -> Result<(Id, Id, TargetPackageV1, DateTime<Utc>), StoreError>
where
    R: FnMut(Id, DbCounter) -> Read,
    Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
{
    let row=sqlx::query("SELECT r.*,c.project_id,c.run_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id JOIN app.artifacts a ON a.id=r.package_artifact_id AND a.project_id=c.project_id AND a.origin='REAL' AND a.schema_name='qz.target_package' AND a.schema_version='1' WHERE r.id=$1 AND r.environment='REAL'")
            .bind(release_id.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    let project = db::id(row.try_get("project_id")?)?;
    crate::research::project_for_write(tx, project).await?;
    let candidate = db::id(row.try_get("candidate_id")?)?;
    sqlx::query("SELECT id FROM app.portfolio_candidates WHERE id=$1 FOR UPDATE")
        .bind(candidate.as_uuid())
        .fetch_one(&mut **tx)
        .await?;
    let bytes = validation::read_document(
        tx,
        db::id(row.try_get("package_artifact_id")?)?,
        None,
        "qz.target_package",
        8 * 1024 * 1024,
        read,
    )
    .await?;
    let original: TargetPackageV1 =
        serde_json::from_slice(&bytes).map_err(|_| StoreError::Integrity)?;
    if original.valid_from != row.try_get::<DateTime<Utc>, _>("valid_from")?
        || original.valid_until != row.try_get::<DateTime<Utc>, _>("valid_until")?
        || original.compatible_market_capabilities.first()
            != Some(&row.try_get::<String, _>("market_capability_version")?)
    {
        return Err(StoreError::Integrity);
    }
    let intent = ReleaseCreateV1 {
        schema_version: SchemaV1,
        candidate_id: candidate,
        evaluation_id: db::id(row.try_get("evaluation_id")?)?,
    };
    let mut current = release::package(tx, project, &intent, release_id, read).await?;
    let mut until = current.valid_until.min(original.valid_until);
    let datasets: Vec<_> = original
        .input_revision_refs
        .iter()
        .map(|id| id.as_uuid())
        .collect();
    let grants=sqlx::query("SELECT g.id,g.allowed_uses,g.valid_until FROM app.data_use_grants g WHERE g.id IN (SELECT d.data_use_grant_id FROM app.dataset_revisions d WHERE d.id=ANY($1::uuid[])) ORDER BY g.id FOR UPDATE")
            .bind(datasets).fetch_all(&mut **tx).await?;
    if grants.is_empty() {
        return Err(StoreError::Integrity);
    }
    for grant in grants {
        let allowed = grant.try_get::<String, _>("allowed_uses")?;
        if allowed != "RESEARCH_PAPER_LIVE"
            && (environment == ForwardEnvironmentV1::Live || allowed != "RESEARCH_AND_PAPER")
        {
            return Err(StoreError::Invalid("approval_data_use"));
        }
        until = until.min(grant.try_get("valid_until")?);
    }
    current.valid_from = original.valid_from;
    current.valid_until = original.valid_until;
    if db::json(&current)? != db::json(&original)? {
        return Err(StoreError::Integrity);
    }
    // Files and time can change while callbacks run: reuse the same full source
    // validation before granting, never trust only an earlier successful read.
    let mut final_package = release::package(tx, project, &intent, release_id, read).await?;
    let until = until.min(final_package.valid_until);
    final_package.valid_from = original.valid_from;
    final_package.valid_until = original.valid_until;
    if db::json(&final_package)? != db::json(&original)? {
        return Err(StoreError::Integrity);
    }
    let latest_bytes = validation::read_document(
        tx,
        db::id(row.try_get("package_artifact_id")?)?,
        None,
        "qz.target_package",
        8 * 1024 * 1024,
        read,
    )
    .await?;
    if latest_bytes != bytes {
        return Err(StoreError::Integrity);
    }
    Ok((project, candidate, original, until))
}

impl Store {
    pub async fn release_approvals(
        &self,
        actor: &Actor,
        release: Id,
        query: &contracts::control::ListQuery,
    ) -> Result<contracts::control::Page<ApprovalViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        let project: uuid::Uuid = sqlx::query_scalar("SELECT c.project_id FROM app.releases r JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE r.id=$1")
            .bind(release.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(project)?).await?;
        let rows = sqlx::query("SELECT a.*,r.candidate_id,c.project_id FROM app.approvals a JOIN app.releases r ON r.id=a.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE a.release_id=$1 AND ($2::uuid IS NULL OR a.id<$2) ORDER BY a.id DESC LIMIT $3")
            .bind(release.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows.iter().map(view).collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |v| v.id))
    }

    pub async fn approval(&self, actor: &Actor, id: Id) -> Result<ApprovalViewV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT a.*,r.candidate_id,c.project_id FROM app.approvals a JOIN app.releases r ON r.id=a.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE a.id=$1")
            .bind(id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(row.try_get("project_id")?)?).await?;
        let result = view(&row)?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn approve_release<R, Read>(
        &self,
        actor: &Actor,
        key: &str,
        release_id: Id,
        request: &ReleaseApproveV1,
        mut read: R,
    ) -> Result<CommandResult<ApprovalViewV1>, StoreError>
    where
        R: FnMut(Id, DbCounter) -> Read,
        Read: std::future::Future<Output = Result<Vec<u8>, StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::ReleaseApprove,
            key,
            Some(release_id),
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let (project, candidate, original, until) =
            source(&mut tx, release_id, request.environment, &mut read).await?;
        let ordinal = decision(
            &mut tx,
            candidate,
            request.downstream_id,
            request.environment,
            request.expected_latest_decision_id,
        )
        .await?;
        downstream(
            &mut tx,
            request.downstream_id,
            request.expected_downstream_revision,
            request.environment,
            &original,
        )
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let granted_at = now(&mut tx).await?;
        if granted_at < original.valid_from
            || request.valid_until <= granted_at
            || request.valid_until > until
        {
            return Err(StoreError::Invalid("approval_expiry"));
        }
        let evidence_set_id = freeze_evidence(&mut tx, &original, granted_at).await?;
        let probe = downstream(
            &mut tx,
            request.downstream_id,
            request.expected_downstream_revision,
            request.environment,
            &original,
        )
        .await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let granted_at = now(&mut tx).await?;
        if granted_at >= request.valid_until || granted_at >= until {
            return Err(StoreError::Invalid("approval_expiry"));
        }
        let approved=sqlx::query("INSERT INTO app.approvals(release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until,downstream_revision,decision_ordinal,readiness_observation_id) VALUES($1,$2,$3,'OPERATOR',$4,$5,$6,$7,$8,$9) RETURNING *")
            .bind(release_id.as_uuid()).bind(db::code(&request.environment)?).bind(request.downstream_id.as_uuid()).bind(evidence_set_id.as_uuid()).bind(granted_at).bind(request.valid_until).bind(request.expected_downstream_revision.get() as i64).bind(ordinal).bind(probe.id.as_uuid()).fetch_one(&mut *tx).await?;
        let resource = ApprovalViewV1 {
            id: db::id(approved.try_get("id")?)?,
            created_at: approved.try_get("created_at")?,
            project_id: project,
            candidate_id: candidate,
            release_id,
            downstream_id: request.downstream_id,
            environment: request.environment,
            authority_kind: "OPERATOR".into(),
            automation_policy_id: None,
            evidence_set_id,
            granted_at,
            valid_until: approved.try_get("valid_until")?,
            downstream_revision: Some(request.expected_downstream_revision),
            decision_ordinal: Some(ordinal as u32),
            readiness_observation_id: Some(probe.id),
        };
        let result = commands::finish(&mut tx, prepared, resource, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}

fn revocation_view(row: &PgRow) -> Result<ApprovalRevocationViewV1, StoreError> {
    Ok(ApprovalRevocationViewV1 {
        id: db::id(row.try_get("id")?)?,
        approval_id: db::id(row.try_get("approval_id")?)?,
        created_at: row.try_get("created_at")?,
        effective_at: row.try_get("effective_at")?,
        reason_code: row.try_get("reason_code")?,
        reason: row.try_get("reason")?,
    })
}
impl Store {
    pub async fn approval_revocations(
        &self,
        actor: &Actor,
        id: Id,
        query: &contracts::control::ListQuery,
    ) -> Result<contracts::control::Page<ApprovalRevocationViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        let project:uuid::Uuid=sqlx::query_scalar("SELECT c.project_id FROM app.approvals a JOIN app.releases r ON r.id=a.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE a.id=$1")
            .bind(id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        crate::evidence::authorize(&mut tx, actor, db::id(project)?).await?;
        let rows=sqlx::query("SELECT * FROM app.approval_revocations WHERE approval_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(id.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .iter()
            .map(revocation_view)
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |v| v.id))
    }

    pub async fn revoke_approval(
        &self,
        actor: &Actor,
        key: &str,
        id: Id,
        request: &ApprovalRevokeV1,
    ) -> Result<CommandResult<ApprovalRevocationViewV1>, StoreError> {
        domain::delivery::decision_reason(&request.reason_code, &request.reason)?;
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::ApprovalRevoke,
            key,
            Some(id),
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let row=sqlx::query("SELECT c.id,c.project_id FROM app.approvals a JOIN app.releases r ON r.id=a.release_id JOIN app.portfolio_candidates c ON c.id=r.candidate_id WHERE a.id=$1")
            .bind(id.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(row.try_get::<uuid::Uuid, _>("project_id")?)
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM app.portfolio_candidates WHERE id=$1 FOR UPDATE")
            .bind(row.try_get::<uuid::Uuid, _>("id")?)
            .fetch_one(&mut *tx)
            .await?;
        sqlx::query("SELECT id FROM app.approvals WHERE id=$1 FOR UPDATE")
            .bind(id.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let latest: Option<uuid::Uuid> = sqlx::query_scalar(
            "SELECT id FROM app.approval_revocations WHERE approval_id=$1 ORDER BY id DESC LIMIT 1",
        )
        .bind(id.as_uuid())
        .fetch_optional(&mut *tx)
        .await?;
        if latest != request.expected_latest_revocation_id.map(Id::as_uuid) {
            return Err(StoreError::Conflict);
        }
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let current = now(&mut tx).await?;
        let effective = request.effective_at.unwrap_or(current);
        if effective < current {
            return Err(StoreError::Invalid("revocation_time"));
        }
        let row=sqlx::query("INSERT INTO app.approval_revocations(approval_id,effective_at,reason_code,reason) VALUES($1,$2,$3,$4) RETURNING *")
            .bind(id.as_uuid()).bind(effective).bind(&request.reason_code).bind(&request.reason).fetch_one(&mut *tx).await?;
        // A new future date never postpones an earlier immutable revocation.
        sqlx::query("UPDATE app.handoff_offers h SET state='REVOKED' WHERE h.approval_id=$1 AND h.state='OFFERED' AND EXISTS(SELECT 1 FROM app.approval_revocations r WHERE r.approval_id=h.approval_id AND r.effective_at<=clock_timestamp())")
            .bind(id.as_uuid()).execute(&mut *tx).await?;
        commands::recheck_authority(&mut tx, actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, revocation_view(&row)?, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}

pub(super) async fn freeze_evidence(
    tx: &mut Tx<'_>,
    original: &TargetPackageV1,
    granted_at: DateTime<Utc>,
) -> Result<Id, StoreError> {
    let evidence_set_id = Id::new();
    let reports:Vec<uuid::Uuid>=sqlx::query_scalar("SELECT DISTINCT artifact FROM app.evaluations e CROSS JOIN LATERAL unnest(ARRAY[e.report_artifact_id,e.method_versions_artifact_id]) artifact WHERE e.id=$1 AND e.subject_candidate_id=$2 AND e.project_id=$3")
            .bind(original.evaluation_refs[0].as_uuid()).bind(original.candidate_id.as_uuid()).bind(original.project_id.as_uuid()).fetch_all(&mut **tx).await?;
    if !(1..=2).contains(&reports.len()) {
        return Err(StoreError::Integrity);
    }
    let evidence = contracts::research::InputSetCreate {
        schema_version: SchemaV1,
        project_id: original.project_id,
        purpose: contracts::research::InputPurpose::Portfolio,
        decision_cutoff: granted_at,
        items: reports
            .into_iter()
            .map(|id| {
                Ok(contracts::research::InputItemV1::Artifact {
                    artifact_id: db::id(id)?,
                    role: contracts::research::ArtifactInputRole::Report,
                })
            })
            .collect::<Result<_, StoreError>>()?,
    };
    crate::research::insert_frozen_input(tx, evidence_set_id, &evidence).await?;
    Ok(evidence_set_id)
}
