//! Native verifier references only; the trusted launcher owns secret generation.
use super::*;

impl Store {
    /// No public issuance DTO or selectable scopes. An unknown commit is retried
    /// only with the same native token/verifier identities, never a fresh secret.
    pub async fn issue_mission_credential(
        &self,
        run: Id,
        owner: &WorkerFence,
        public_token: Id,
        verifier: Id,
    ) -> Result<Id, StoreError> {
        let mut tx = self.pool.begin().await?;
        let locked = lock_run(&mut tx, run).await?;
        fence(&mut tx, &locked.run, owner).await?;
        if !locked.admission_open()
            || !matches!(
                locked.run.state,
                RunState::Dispatching | RunState::Running | RunState::Reconciling
            )
        {
            return Err(DomainError::AdmissionClosed.into());
        }
        current_profile(&mut tx, run).await?;
        let role: String = sqlx::query_scalar("SELECT role FROM app.run_missions WHERE run_id=$1")
            .bind(run.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let mut scopes = vec![
            MachineScope::ResearchRead,
            MachineScope::ArtifactSubmit,
            MachineScope::EvidenceRead,
            MachineScope::RunRead,
        ];
        if role == "RESEARCHER" {
            scopes.push(MachineScope::ExperimentSubmit);
        }
        let scopes: Vec<_> = scopes.into_iter().map(MachineScope::code).collect();
        // The locked Run serializes this service's sole principal allocation.
        // Historical unrelated Mission rows are not rewritten or deduplicated.
        let principals = sqlx::query("SELECT id,enabled,credential_epoch FROM app.machine_principals WHERE kind='MISSION' AND run_id=$1 LIMIT 2 FOR UPDATE")
            .bind(run.as_uuid()).fetch_all(&mut *tx).await?;
        if principals.len() > 1 {
            return Err(StoreError::Integrity);
        }
        if let Some(p) = principals.first() {
            if !p.try_get::<bool, _>("enabled")? {
                return Err(StoreError::Forbidden);
            }
        }
        if let Some(c) = sqlx::query("SELECT c.id,c.verifier_ref,c.issuer_attempt_id,c.issuer_owner_epoch,c.scope_codes,c.principal_epoch,c.expires_at,p.credential_epoch,p.run_id FROM app.machine_credentials c JOIN app.machine_principals p ON p.id=c.principal_id WHERE c.public_token_id=$1 FOR SHARE OF c")
            .bind(public_token.to_string()).fetch_optional(&mut *tx).await? {
            if c.try_get::<Option<uuid::Uuid>,_>("run_id")? != Some(run.as_uuid())
                || c.try_get::<String,_>("verifier_ref")? != verifier.to_string()
                || c.try_get::<Option<uuid::Uuid>,_>("issuer_attempt_id")? != Some(owner.attempt_id.as_uuid())
                || c.try_get::<Option<i64>,_>("issuer_owner_epoch")? != Some(owner.owner_epoch.get() as i64)
                || c.try_get::<i64,_>("principal_epoch")? != c.try_get::<i64,_>("credential_epoch")?
                || c.try_get::<Vec<String>,_>("scope_codes")? != scopes
            { return Err(StoreError::Conflict); }
            fence(&mut tx,&locked.run,owner).await?;
            let time=now(&mut tx).await?;
            let revoked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.machine_credential_revocations WHERE credential_id=$1 AND effective_at<=$2)")
                .bind(c.try_get::<uuid::Uuid,_>("id")?).bind(time).fetch_one(&mut *tx).await?;
            if revoked { return Err(StoreError::Conflict); }
            if locked.run.deadline_at<=time || c.try_get::<DateTime<Utc>,_>("expires_at")?<=time { return Err(DomainError::AdmissionClosed.into()); }
            let id=db::id(c.try_get("id")?)?;
            tx.commit().await?;
            return Ok(id);
        }
        let (principal, epoch) = if let Some(p) = principals.first() {
            let principal: uuid::Uuid = p.try_get("id")?;
            let already:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.machine_credentials WHERE principal_id=$1 AND issuer_attempt_id=$2 AND issuer_owner_epoch=$3)")
                .bind(principal).bind(owner.attempt_id.as_uuid()).bind(owner.owner_epoch.get() as i64).fetch_one(&mut *tx).await?;
            if already {
                return Err(StoreError::Conflict);
            }
            let epoch:i64=sqlx::query_scalar("UPDATE app.machine_principals SET credential_epoch=credential_epoch+1 WHERE id=$1 RETURNING credential_epoch")
                .bind(principal).fetch_one(&mut *tx).await?;
            (principal, epoch)
        } else {
            let principal:uuid::Uuid=sqlx::query_scalar("INSERT INTO app.machine_principals(name,kind,project_id,run_id,enabled,credential_epoch) VALUES($1,'MISSION',$2,$3,true,1) RETURNING id")
                .bind(format!("Mission {run}")).bind(locked.run.project_id.as_uuid()).bind(run.as_uuid()).fetch_one(&mut *tx).await?;
            (principal, 1)
        };
        // Read the clock after all native authority locks, including the principal.
        fence(&mut tx, &locked.run, owner).await?;
        if locked.run.deadline_at <= now(&mut tx).await? {
            return Err(DomainError::AdmissionClosed.into());
        }
        let credential:uuid::Uuid=sqlx::query_scalar("INSERT INTO app.machine_credentials(principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by,issuer_attempt_id,issuer_owner_epoch) VALUES($1,$2,$3,$4,$5,clock_timestamp(),$6,'MISSION_SERVICE',$7,$8) RETURNING id")
            .bind(principal).bind(public_token.to_string()).bind(verifier.to_string()).bind(epoch).bind(scopes).bind(locked.run.deadline_at)
            .bind(owner.attempt_id.as_uuid()).bind(owner.owner_epoch.get() as i64).fetch_one(&mut *tx).await?;
        let credential = db::id(credential)?;
        tx.commit().await?;
        Ok(credential)
    }
}
