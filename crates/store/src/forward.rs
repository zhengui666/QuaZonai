//! Authenticated downstream weight observations using original immutable command receipts.
use crate::{
    authority::{self, Actor},
    commands, db,
    lifecycle::native::NativeObjectPublication,
    Store, StoreError,
};
use contracts::{
    control::{CommandResult, MachineScope, PrincipalKind},
    forward::*,
    science::{PortfolioCurrentWeightsV1, PortfolioWeightsSourceV1},
    Id,
};
use sqlx::{Postgres, Row, Transaction};

mod admission;
pub(crate) use admission::{limits as evaluation_limits, revalidate};
mod messages;
mod observations;
mod schedule;
mod window;

async fn authority(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    request: &DownstreamWeightsSubmitV1,
) -> Result<Id, StoreError> {
    let machine = authority::machine(tx, actor, true).await?;
    if machine.kind != PrincipalKind::Downstream {
        return Err(StoreError::Forbidden);
    }
    machine.requires(MachineScope::ForwardSubmit)?;
    machine.project(request.project_id)?;
    crate::research::project_for_write(tx, request.project_id).await?;
    let downstream = machine.downstream_id.ok_or(StoreError::Forbidden)?;
    let row = sqlx::query(
        "SELECT enabled,environments FROM app.downstream_integrations WHERE id=$1 FOR SHARE",
    )
    .bind(downstream.as_uuid())
    .fetch_one(&mut **tx)
    .await?;
    let environments: String = row.try_get("environments")?;
    if !row.try_get::<bool, _>("enabled")?
        || (environments != "BOTH" && environments != db::code(&request.environment)?)
    {
        return Err(StoreError::Forbidden);
    }
    Ok(downstream)
}

impl Store {
    pub async fn downstream_weight_snapshots(
        &self,
        actor: &Actor,
        project: Id,
        query: &contracts::control::ListQuery,
    ) -> Result<contracts::control::Page<DownstreamWeightsViewV1>, StoreError> {
        domain::control::list(query)?;
        let mut tx = self.pool.begin().await?;
        crate::evidence::authorize(&mut tx, actor, project).await?;
        sqlx::query("SELECT id FROM app.projects WHERE id=$1")
            .bind(project.as_uuid())
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
        let rows = sqlx::query("SELECT * FROM app.forward_weight_snapshots WHERE project_id=$1 AND ($2::uuid IS NULL OR id<$2) ORDER BY id DESC LIMIT $3")
            .bind(project.as_uuid()).bind(query.cursor.map(Id::as_uuid)).bind(i64::from(query.limit)+1).fetch_all(&mut *tx).await?;
        let items = rows
            .iter()
            .map(|row| {
                Ok(DownstreamWeightsViewV1 {
                    id: db::id(row.try_get("id")?)?,
                    project_id: db::id(row.try_get("project_id")?)?,
                    downstream_id: db::id(row.try_get("downstream_id")?)?,
                    environment: db::enum_value(row, "environment")?,
                    report_artifact_id: db::id(row.try_get("report_artifact_id")?)?,
                    content: serde_json::from_value(row.try_get("content")?)
                        .map_err(|_| StoreError::Integrity)?,
                    received_at: row.try_get("received_at")?,
                })
            })
            .collect::<Result<Vec<_>, StoreError>>()?;
        tx.commit().await?;
        Ok(crate::control::page(items, query.limit, |item| item.id))
    }

    /// The producer holds this same project lock through commit; unknown outcomes
    /// must settle before deciding whether its allocated object is unreferenced.
    pub async fn discard_unpublished_forward_artifact<F, Fut>(
        &self,
        project: Id,
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
        sqlx::query("SELECT id FROM app.projects WHERE id=$1 FOR UPDATE")
            .bind(project.as_uuid())
            .fetch_one(&mut *tx)
            .await?;
        let referenced: bool =
            sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM app.artifacts WHERE id=$1)")
                .bind(artifact.as_uuid())
                .fetch_one(&mut *tx)
                .await?;
        if !referenced {
            discard(artifact).await?;
        }
        tx.commit().await?;
        Ok(!referenced)
    }

    pub async fn submit_downstream_weights<P, Published>(
        &self,
        actor: &Actor,
        request: &DownstreamWeightsSubmitV1,
        publish: P,
    ) -> Result<CommandResult<DownstreamWeightsViewV1>, StoreError>
    where
        P: FnOnce(NativeObjectPublication) -> Published,
        Published: std::future::Future<Output = Result<(), StoreError>>,
    {
        domain::forward::weights(request)?;
        let mut tx = self.pool.begin().await?;
        let downstream = authority(&mut tx, actor, request).await?;
        let environment = db::code(&request.environment)?;
        let scope = format!(
            "DOWNSTREAM:{downstream}:PROJECT:{}:{environment}",
            request.project_id
        );
        let prepared = commands::forward_weights(
            &mut tx,
            scope,
            &request.external_message_id,
            db::json(request)?,
        )
        .await?;
        if let Some(replay) = prepared.replay()? {
            tx.commit().await?;
            return Ok(replay);
        }
        let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let nanos = u64::try_from(now.timestamp_nanos_opt().ok_or(StoreError::Integrity)?)
            .map_err(|_| StoreError::Integrity)?;
        if request.available_ns.get() > nanos || request.valid_until_ns.get() <= nanos {
            return Err(StoreError::Invalid("forward_weights_time"));
        }
        let content = PortfolioCurrentWeightsV1 {
            schema_version: contracts::SchemaV1,
            source: PortfolioWeightsSourceV1::ForwardSnapshot {
                downstream_id: downstream,
                external_message_id: request.external_message_id.clone(),
            },
            asof_ns: request.asof_ns,
            available_ns: request.available_ns,
            valid_until_ns: request.valid_until_ns,
            base_currency: request.base_currency.clone(),
            cash_weight: request.cash_weight.clone(),
            weights: request.weights.clone(),
        };
        let artifact = Id::new();
        let bytes = serde_json::to_vec(&content).map_err(|_| StoreError::Integrity)?;
        let size = i64::try_from(bytes.len()).map_err(|_| StoreError::Integrity)?;
        publish(NativeObjectPublication {
            id: artifact,
            bytes,
        })
        .await?;
        let origin = if request.environment == ForwardEnvironmentV1::Paper {
            "SYNTHETIC"
        } else {
            "REAL"
        };
        sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','qz.portfolio_current_weights','1','LOCAL',$3,'1',$4,'RESEARCH',$5,'IMPORT','REFERENCED')")
            .bind(artifact.as_uuid()).bind(request.project_id.as_uuid()).bind(artifact.to_string()).bind(size).bind(origin).execute(&mut *tx).await?;
        let received = sqlx::query_scalar("INSERT INTO app.forward_weight_snapshots(id,project_id,downstream_id,environment,external_message_id,report_artifact_id,content) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING received_at")
            .bind(prepared.target.as_uuid()).bind(request.project_id.as_uuid()).bind(downstream.as_uuid()).bind(environment).bind(&request.external_message_id).bind(artifact.as_uuid()).bind(db::json(&content)?).fetch_one(&mut *tx).await?;
        if authority(&mut tx, actor, request).await? != downstream {
            return Err(StoreError::Forbidden);
        }
        let checked_at: chrono::DateTime<chrono::Utc> =
            sqlx::query_scalar("SELECT clock_timestamp()")
                .fetch_one(&mut *tx)
                .await?;
        let checked_ns = u64::try_from(
            checked_at
                .timestamp_nanos_opt()
                .ok_or(StoreError::Integrity)?,
        )
        .map_err(|_| StoreError::Integrity)?;
        if request.valid_until_ns.get() <= checked_ns {
            return Err(StoreError::Invalid("forward_weights_time"));
        }
        let resource = DownstreamWeightsViewV1 {
            id: prepared.target,
            project_id: request.project_id,
            downstream_id: downstream,
            environment: request.environment,
            report_artifact_id: artifact,
            content,
            received_at: received,
        };
        let result = commands::finish(&mut tx, prepared, resource, 201).await?;
        tx.commit().await?;
        Ok(result)
    }
}
