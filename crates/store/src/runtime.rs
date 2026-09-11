//! Two short authority transactions around native network I/O. A probe is not
//! scientific evidence and never grants an Agent configuration authority.
use crate::{authority::Actor, commands, db, lifecycle::RuntimeSnapshot, Store, StoreError};
use chrono::{DateTime, Duration, Utc};
use contracts::{
    control::{CommandResult, OperatorOperation},
    runtime::*,
    Id, Revision, SchemaV1,
};
use serde_json::json;
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

type Tx<'a> = Transaction<'a, Postgres>;

pub enum ProbePreparation {
    Replay(Box<CommandResult<RuntimeProbeViewV1>>),
    Pending(Box<ProbeTicket>),
}

/// Not Deserialize/Debug. Only an authorized native read can create this ticket.
pub struct ProbeTicket {
    pub snapshot: RuntimeSnapshot,
    actor: Actor,
    key: String,
    runtime_id: Id,
    revision: Revision,
    started_at: DateTime<Utc>,
}

fn observation(row: &PgRow) -> Result<RuntimeProbeViewV1, StoreError> {
    let document: serde_json::Value = row.try_get("outcome")?;
    let outcome = serde_json::from_value(
        document
            .get("result")
            .cloned()
            .ok_or(StoreError::Integrity)?,
    )
    .map_err(|_| StoreError::Integrity)?;
    Ok(RuntimeProbeViewV1 {
        id: db::id(row.try_get("id")?)?,
        runtime_id: db::id(row.try_get("runtime_id")?)?,
        integration_revision: db::revision(row.try_get("integration_revision")?)?,
        snapshot_artifact_id: db::id(row.try_get("snapshot_artifact_id")?)?,
        observed_at: row.try_get("observed_at")?,
        valid_until: row.try_get("valid_until")?,
        outcome,
    })
}

async fn latest(tx: &mut Tx<'_>, runtime: Id) -> Result<Option<RuntimeProbeViewV1>, StoreError> {
    sqlx::query("SELECT * FROM app.runtime_probe_observations WHERE runtime_id=$1 ORDER BY observed_at DESC,id DESC LIMIT 1")
        .bind(runtime.as_uuid())
        .fetch_optional(&mut **tx)
        .await?
        .as_ref()
        .map(observation)
        .transpose()
}

impl Store {
    pub async fn prepare_runtime_probe(
        &self,
        actor: &Actor,
        key: &str,
        runtime: Id,
        request: &RuntimeProbeRequestV1,
    ) -> Result<ProbePreparation, StoreError> {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::RuntimeProbe,
            key,
            Some(runtime),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(ProbePreparation::Replay(Box::new(result)));
        }
        let row = sqlx::query("SELECT * FROM app.runtime_integrations WHERE id=$1 FOR SHARE")
            .bind(runtime.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let current = db::revision(row.try_get("revision")?)?;
        if current != request.expected_revision {
            return Err(StoreError::RevisionConflict { current });
        }
        if !row.try_get::<bool, _>("enabled")? {
            return Err(domain::DomainError::CapabilityUnavailable("runtime_disabled").into());
        }
        let snapshot = RuntimeSnapshot {
            schema_version: SchemaV1,
            endpoint: row.try_get("endpoint")?,
            credential_ref: row.try_get("credential_ref")?,
            tls_policy: row.try_get("tls_policy")?,
            ca_certificate_ref: row.try_get("ca_certificate_ref")?,
            development_http: row.try_get("development_http")?,
            protocol_version: row.try_get("protocol_version")?,
            allowed_capabilities: row.try_get("allowed_capabilities")?,
        };
        let started_at = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(ProbePreparation::Pending(Box::new(ProbeTicket {
            snapshot,
            actor: actor.clone(),
            key: key.into(),
            runtime_id: runtime,
            revision: current,
            started_at,
        })))
    }

    /// Network I/O has ended. Only the receipt owner invokes bounded local
    /// publication; a concurrent replay never creates an additional object.
    /// Unknown commits retain objects rather than deleting possible references.
    pub async fn complete_runtime_probe<F, Fut>(
        &self,
        ticket: ProbeTicket,
        outcome: RuntimeProbeOutcomeV1,
        publish: F,
    ) -> Result<CommandResult<RuntimeProbeViewV1>, StoreError>
    where
        F: FnOnce(Id, Vec<u8>) -> Fut,
        Fut: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let request = RuntimeProbeRequestV1 {
            schema_version: SchemaV1,
            expected_revision: ticket.revision,
        };
        let prepared = commands::operator(
            &mut tx,
            &ticket.actor,
            OperatorOperation::RuntimeProbe,
            &ticket.key,
            Some(ticket.runtime_id),
            db::json(&request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let row = sqlx::query(
            "SELECT revision,enabled FROM app.runtime_integrations WHERE id=$1 FOR UPDATE",
        )
        .bind(ticket.runtime_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let current = db::revision(row.try_get("revision")?)?;
        if current != ticket.revision {
            return Err(StoreError::RevisionConflict { current });
        }
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        if !row.try_get::<bool, _>("enabled")?
            || now < ticket.started_at
            || now > ticket.started_at + Duration::seconds(20)
        {
            return Err(
                domain::DomainError::CapabilityUnavailable("probe_expired_or_disabled").into(),
            );
        }
        if let RuntimeProbeOutcomeV1::Available { capabilities } = &outcome {
            domain::runtime::capabilities(capabilities, now)?;
            if capabilities.checked_at < ticket.started_at - Duration::seconds(5) {
                return Err(domain::DomainError::CapabilityUnavailable(
                    "runtime_observation_predates_probe",
                )
                .into());
            }
        }
        let document = json!({"schema_version":1,"result":outcome});
        let exact = serde_json::to_vec(&document).map_err(|_| StoreError::Integrity)?;
        if exact.is_empty() || exact.len() > 1024 * 1024 {
            return Err(StoreError::Invalid("runtime_probe_artifact_size"));
        }
        let bytes = exact.len() as i64;
        let artifact = Id::new();
        publish(artifact, exact).await?;
        commands::recheck_authority(&mut tx, &ticket.actor, &prepared).await?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        if now < ticket.started_at || now > ticket.started_at + Duration::seconds(20) {
            return Err(
                domain::DomainError::CapabilityUnavailable("probe_expired_or_disabled").into(),
            );
        }
        sqlx::query("INSERT INTO app.artifacts(id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,'REPORT','application/json','qz.runtime_probe','1','LOCAL',$2,'1',$3,'OPERATOR','REAL','OPERATOR','AUDIT')")
            .bind(artifact.as_uuid()).bind(artifact.to_string()).bind(bytes)
            .execute(&mut *tx).await?;
        let id = Id::new();
        let row = sqlx::query("INSERT INTO app.runtime_probe_observations(id,runtime_id,integration_revision,snapshot_artifact_id,observed_at,valid_until,outcome) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING *")
            .bind(id.as_uuid()).bind(ticket.runtime_id.as_uuid()).bind(current.get() as i64)
            .bind(artifact.as_uuid()).bind(now).bind(ticket.started_at + Duration::seconds(60)).bind(document)
            .fetch_one(&mut *tx).await?;
        sqlx::query("UPDATE app.runtime_integrations SET last_capability_snapshot_artifact_id=$2 WHERE id=$1")
            .bind(ticket.runtime_id.as_uuid()).bind(artifact.as_uuid()).execute(&mut *tx).await?;
        commands::recheck_authority(&mut tx, &ticket.actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, observation(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Only a trusted Operator publisher supplies an ID allocated by its failed
    /// publication attempt. No HTTP/CLI caller can use this as an artifact delete.
    /// Reacquiring the exact original command lock waits out an uncertain commit;
    /// a committed metadata row or native object reference always wins retention.
    pub async fn discard_unpublished_operator_artifact<F, Fut>(
        &self,
        artifact: Id,
        discard: F,
    ) -> Result<bool, StoreError>
    where
        F: FnOnce(Id) -> Fut,
        Fut: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET LOCAL lock_timeout = '5s'")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SELECT singleton FROM app.operator_auth_state WHERE singleton FOR UPDATE")
            .fetch_one(&mut *tx)
            .await?;
        // A separate statement after the authority lock observes every completed
        // original publication. Check native references as well as the row ID.
        let referenced: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.artifacts WHERE id=$1 OR (storage_backend='LOCAL' AND storage_object_ref=$2))")
            .bind(artifact.as_uuid())
            .bind(artifact.to_string())
            .fetch_one(&mut *tx)
            .await?;
        if referenced {
            tx.commit().await?;
            return Ok(false);
        }
        discard(artifact).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn runtime_readiness(
        &self,
        actor: &Actor,
        runtime: Id,
    ) -> Result<RuntimeReadinessV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        crate::settings::read_authority(&mut tx, actor).await?;
        let row = sqlx::query("SELECT revision,enabled,allowed_capabilities FROM app.runtime_integrations WHERE id=$1 FOR SHARE")
            .bind(runtime.as_uuid()).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        let revision = db::revision(row.try_get("revision")?)?;
        let observed = latest(&mut tx, runtime).await?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let mut kinds = Vec::new();
        let state = if !row.try_get::<bool, _>("enabled")? {
            RuntimeReadinessState::Disabled
        } else if let Some(probe) = &observed {
            if probe.integration_revision != revision || probe.valid_until <= now {
                RuntimeReadinessState::Stale
            } else if let RuntimeProbeOutcomeV1::Available { capabilities } = &probe.outcome {
                let configured: Vec<String> = row.try_get("allowed_capabilities")?;
                for kind in &capabilities.job_kinds {
                    if configured.contains(&db::code(kind)?) {
                        kinds.push(*kind);
                    }
                }
                RuntimeReadinessState::Available
            } else {
                RuntimeReadinessState::Unavailable
            }
        } else {
            RuntimeReadinessState::NotChecked
        };
        tx.commit().await?;
        Ok(RuntimeReadinessV1 {
            schema_version: SchemaV1,
            runtime_id: runtime,
            integration_revision: revision,
            state,
            latest_observation: observed,
            available_job_kinds: kinds,
        })
    }
}

/// Fresh admissions and their first wire dispatch share this native gate.
/// Reconciliation of a possibly sent remote job must not call this helper.
pub(crate) async fn require_job(
    tx: &mut Transaction<'_, Postgres>,
    runtime: Id,
    revision: Revision,
    kind: contracts::runs::RunKind,
    limits: &contracts::lifecycle::JobLimitsV1,
) -> Result<(), StoreError> {
    let capabilities = require_capabilities(tx, runtime, revision, kind).await?;
    domain::runtime::job_limits(&capabilities, limits)?;
    Ok(())
}

/// Trusted admission paths call this under their existing short transaction.
/// No network I/O, implied grant or client-supplied capability is accepted.
pub async fn require_capabilities(
    tx: &mut Transaction<'_, Postgres>,
    runtime: Id,
    revision: Revision,
    kind: contracts::runs::RunKind,
) -> Result<RuntimeCapabilitiesV1, StoreError> {
    let row = sqlx::query("SELECT revision,enabled,allowed_capabilities FROM app.runtime_integrations WHERE id=$1 FOR SHARE")
        .bind(runtime.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    if !row.try_get::<bool, _>("enabled")?
        || db::revision(row.try_get("revision")?)? != revision
        || !row
            .try_get::<Vec<String>, _>("allowed_capabilities")?
            .contains(&db::code(&kind)?)
    {
        return Err(
            domain::DomainError::CapabilityUnavailable("runtime_job_kind_or_revision").into(),
        );
    }
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let probe = latest(tx, runtime)
        .await?
        .ok_or(domain::DomainError::CapabilityUnavailable(
            "runtime_not_probed",
        ))?;
    if probe.integration_revision != revision || probe.valid_until <= now {
        return Err(domain::DomainError::CapabilityUnavailable("runtime_probe_stale").into());
    }
    match probe.outcome {
        RuntimeProbeOutcomeV1::Available { capabilities }
            if capabilities.job_kinds.contains(&kind) =>
        {
            Ok(*capabilities)
        }
        _ => Err(domain::DomainError::CapabilityUnavailable("runtime_probe_unavailable").into()),
    }
}
