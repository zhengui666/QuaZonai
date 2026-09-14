//! Short authority transactions around native I/O; observations grant no delivery.
use crate::{authority::Actor, commands, db, Store, StoreError};
use chrono::{DateTime, Duration, Utc};
use contracts::{
    control::{CommandResult, OperatorOperation},
    delivery::*,
    Id, Revision, SchemaV1,
};
use serde_json::json;
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};

pub enum ProbePreparation {
    Replay(Box<CommandResult<DownstreamProbeViewV1>>),
    Pending(Box<ProbeTicket>),
}

#[derive(Clone)]
pub struct DownstreamSnapshot {
    pub endpoint: String,
    pub credential_ref: Id,
    pub development_http: bool,
}

/// Native authority creates this ticket; it is never accepted from wire data.
pub struct ProbeTicket {
    pub snapshot: DownstreamSnapshot,
    actor: Actor,
    key: String,
    downstream_id: Id,
    revision: Revision,
    started_at: DateTime<Utc>,
}

fn observation(row: &PgRow) -> Result<DownstreamProbeViewV1, StoreError> {
    let document: serde_json::Value = row.try_get("outcome")?;
    Ok(DownstreamProbeViewV1 {
        id: db::id(row.try_get("id")?)?,
        downstream_id: db::id(row.try_get("downstream_id")?)?,
        integration_revision: db::revision(row.try_get("integration_revision")?)?,
        snapshot_artifact_id: db::id(row.try_get("snapshot_artifact_id")?)?,
        started_at: row.try_get("started_at")?,
        observed_at: row.try_get("observed_at")?,
        valid_until: row.try_get("valid_until")?,
        outcome: serde_json::from_value(
            document
                .get("result")
                .cloned()
                .ok_or(StoreError::Integrity)?,
        )
        .map_err(|_| StoreError::Integrity)?,
    })
}

async fn latest(
    tx: &mut Transaction<'_, Postgres>,
    downstream: Id,
) -> Result<Option<DownstreamProbeViewV1>, StoreError> {
    sqlx::query("SELECT * FROM app.downstream_probe_observations WHERE downstream_id=$1 ORDER BY started_at DESC,id DESC LIMIT 1")
        .bind(downstream.as_uuid()).fetch_optional(&mut **tx).await?.as_ref().map(observation).transpose()
}

fn encode(
    outcome: &DownstreamProbeOutcomeV1,
    started: DateTime<Utc>,
    now: DateTime<Utc>,
) -> Result<Vec<u8>, StoreError> {
    if now < started || now > started + Duration::seconds(20) {
        return Err(domain::DomainError::CapabilityUnavailable("downstream_probe_expired").into());
    }
    if let DownstreamProbeOutcomeV1::Available { capabilities } = outcome {
        domain::delivery::downstream_capabilities(capabilities, now)?;
        if capabilities.checked_at < started - Duration::seconds(5) {
            return Err(domain::DomainError::CapabilityUnavailable(
                "downstream_observation_predates_probe",
            )
            .into());
        }
    }
    let bytes = serde_json::to_vec(&json!({"schema_version":1,"result":outcome}))
        .map_err(|_| StoreError::Integrity)?;
    if bytes.len() > 64 * 1024 {
        return Err(StoreError::Invalid("downstream_probe_artifact_size"));
    }
    Ok(bytes)
}

impl Store {
    pub async fn prepare_downstream_probe(
        &self,
        actor: &Actor,
        key: &str,
        downstream: Id,
        request: &DownstreamProbeRequestV1,
    ) -> Result<ProbePreparation, StoreError> {
        let mut tx = self.pool.begin().await?;
        let prepared = commands::operator(
            &mut tx,
            actor,
            OperatorOperation::DownstreamProbe,
            key,
            Some(downstream),
            db::json(request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(ProbePreparation::Replay(Box::new(result)));
        }
        let row = sqlx::query("SELECT * FROM app.downstream_integrations WHERE id=$1 FOR SHARE")
            .bind(downstream.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let revision = db::revision(row.try_get("revision")?)?;
        if revision != request.expected_revision {
            return Err(StoreError::RevisionConflict { current: revision });
        }
        if !row.try_get::<bool, _>("enabled")? {
            return Err(domain::DomainError::CapabilityUnavailable("downstream_disabled").into());
        }
        let snapshot = DownstreamSnapshot {
            endpoint: row.try_get("endpoint")?,
            credential_ref: row
                .try_get::<String, _>("credential_ref")?
                .try_into()
                .map_err(|_| StoreError::Integrity)?,
            development_http: row.try_get("development_http")?,
        };
        let started_at = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(ProbePreparation::Pending(Box::new(ProbeTicket {
            snapshot,
            actor: actor.clone(),
            key: key.into(),
            downstream_id: downstream,
            revision,
            started_at,
        })))
    }

    pub async fn complete_downstream_probe<F, Fut>(
        &self,
        ticket: ProbeTicket,
        outcome: DownstreamProbeOutcomeV1,
        publish: F,
    ) -> Result<CommandResult<DownstreamProbeViewV1>, StoreError>
    where
        F: FnOnce(Id, Vec<u8>) -> Fut,
        Fut: std::future::Future<Output = Result<(), StoreError>>,
    {
        let mut tx = self.pool.begin().await?;
        let request = DownstreamProbeRequestV1 {
            schema_version: SchemaV1,
            expected_revision: ticket.revision,
        };
        let prepared = commands::operator(
            &mut tx,
            &ticket.actor,
            OperatorOperation::DownstreamProbe,
            &ticket.key,
            Some(ticket.downstream_id),
            db::json(&request)?,
        )
        .await?;
        if let Some(result) = prepared.replay()? {
            tx.commit().await?;
            return Ok(result);
        }
        let row = sqlx::query(
            "SELECT revision,enabled FROM app.downstream_integrations WHERE id=$1 FOR UPDATE",
        )
        .bind(ticket.downstream_id.as_uuid())
        .fetch_one(&mut *tx)
        .await?;
        let revision = db::revision(row.try_get("revision")?)?;
        if revision != ticket.revision {
            return Err(StoreError::RevisionConflict { current: revision });
        }
        if !row.try_get::<bool, _>("enabled")? {
            return Err(domain::DomainError::CapabilityUnavailable("downstream_disabled").into());
        }
        let now = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let bytes = encode(&outcome, ticket.started_at, now)?;
        let size = bytes.len() as i64;
        let artifact = Id::new();
        publish(artifact, bytes).await?;
        commands::recheck_authority(&mut tx, &ticket.actor, &prepared).await?;
        let observed_at: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        encode(&outcome, ticket.started_at, observed_at)?;
        sqlx::query("INSERT INTO app.artifacts(id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,'REPORT','application/json','qz.downstream_probe','1','LOCAL',$2,'1',$3,'OPERATOR','REAL','OPERATOR','AUDIT')")
            .bind(artifact.as_uuid()).bind(artifact.to_string()).bind(size).execute(&mut *tx).await?;
        let row = sqlx::query("INSERT INTO app.downstream_probe_observations(downstream_id,integration_revision,snapshot_artifact_id,started_at,observed_at,valid_until,outcome) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING *")
            .bind(ticket.downstream_id.as_uuid()).bind(revision.get() as i64).bind(artifact.as_uuid()).bind(ticket.started_at).bind(observed_at).bind(ticket.started_at+Duration::seconds(60)).bind(json!({"schema_version":1,"result":outcome})).fetch_one(&mut *tx).await?;
        commands::recheck_authority(&mut tx, &ticket.actor, &prepared).await?;
        let result = commands::finish(&mut tx, prepared, observation(&row)?, 200).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn downstream_readiness(
        &self,
        actor: &Actor,
        downstream: Id,
    ) -> Result<DownstreamReadinessV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        crate::settings::read_authority(&mut tx, actor).await?;
        let result = readiness(&mut tx, downstream).await?;
        tx.commit().await?;
        Ok(result)
    }
}

pub(crate) async fn readiness(
    tx: &mut Transaction<'_, Postgres>,
    downstream: Id,
) -> Result<DownstreamReadinessV1, StoreError> {
    let row = sqlx::query("SELECT revision,enabled,accepted_package_versions,environments FROM app.downstream_integrations WHERE id=$1 FOR SHARE")
            .bind(downstream.as_uuid()).fetch_optional(&mut **tx).await?.ok_or(StoreError::NotFound)?;
    let revision = db::revision(row.try_get("revision")?)?;
    let observed = latest(tx, downstream).await?;
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let mut versions = Vec::new();
    let mut environments = Vec::new();
    let state = if !row.try_get::<bool, _>("enabled")? {
        DownstreamReadinessState::Disabled
    } else if let Some(probe) = &observed {
        if probe.integration_revision != revision
            || probe.valid_until <= now
            || probe.observed_at > now
        {
            DownstreamReadinessState::Stale
        } else if let DownstreamProbeOutcomeV1::Available { capabilities } = &probe.outcome {
            if capabilities.accepting_targets {
                let configured: Vec<String> = row.try_get("accepted_package_versions")?;
                for version in &capabilities.accepted_package_versions {
                    if configured.contains(&db::code(version)?) {
                        versions.push(*version);
                    }
                }
                let configured: String = row.try_get("environments")?;
                for environment in &capabilities.environments {
                    if configured == "BOTH" || db::code(environment)? == configured {
                        environments.push(*environment);
                    }
                }
            }
            if versions.is_empty() || environments.is_empty() {
                versions.clear();
                environments.clear();
                DownstreamReadinessState::Unavailable
            } else {
                DownstreamReadinessState::Available
            }
        } else {
            DownstreamReadinessState::Unavailable
        }
    } else {
        DownstreamReadinessState::NotChecked
    };
    Ok(DownstreamReadinessV1 {
        schema_version: SchemaV1,
        downstream_id: downstream,
        integration_revision: revision,
        state,
        latest_observation: observed,
        available_package_versions: versions,
        available_environments: environments,
    })
}
