//! Controlled relational Runtime observations for lifecycle tests only.
//! This is NOT a production probe, real OCI execution, or scientific evidence.
#![allow(dead_code)]
#[path = "runtime.rs"]
mod protocol_fixture;
use chrono::{DateTime, Duration, Utc};
use contracts::{
    runtime::{RuntimeCapabilitiesV1, RuntimeImageV1, RuntimeProbeOutcomeV1},
    Id, Revision,
};
use serde_json::json;
use sqlx::{PgPool, Row};

pub async fn configured_capabilities(pool: &PgPool, runtime: Id) -> RuntimeCapabilitiesV1 {
    let kinds: Vec<String> =
        sqlx::query_scalar("SELECT allowed_capabilities FROM app.runtime_integrations WHERE id=$1")
            .bind(runtime.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let mut capabilities = protocol_fixture::capabilities(now);
    let image = capabilities.image_refs[0].image_ref.clone();
    capabilities.job_kinds = kinds
        .into_iter()
        .map(|kind| serde_json::from_value(json!(kind)).unwrap())
        .collect();
    capabilities.image_refs = capabilities
        .job_kinds
        .iter()
        .map(|kind| RuntimeImageV1 {
            job_kind: *kind,
            image_ref: image.clone(),
        })
        .collect();
    domain::runtime::capabilities(&capabilities, now).unwrap();
    capabilities
}

pub async fn ready(pool: &PgPool, runtime: Id) -> Revision {
    let capabilities = configured_capabilities(pool, runtime).await;
    publish(
        pool,
        runtime,
        RuntimeProbeOutcomeV1::Available {
            capabilities: Box::new(capabilities),
        },
        Duration::seconds(60),
    )
    .await
}

pub async fn publish(
    pool: &PgPool,
    runtime: Id,
    outcome: RuntimeProbeOutcomeV1,
    ttl: Duration,
) -> Revision {
    assert!(ttl > Duration::zero() && ttl <= Duration::seconds(60));
    let mut tx = pool.begin().await.unwrap();
    let row = sqlx::query("SELECT revision FROM app.runtime_integrations WHERE id=$1 FOR UPDATE")
        .bind(runtime.as_uuid())
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let revision: i64 = row.try_get("revision").unwrap();
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let artifact = Id::new();
    let document = json!({"schema_version":1,"result":outcome});
    let size = serde_json::to_vec(&document).unwrap().len() as i64;
    sqlx::query("INSERT INTO app.artifacts(id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,'REPORT','application/json','qz.runtime_probe','1','LOCAL',$2,'1',$3,'OPERATOR','FIXTURE','OPERATOR','AUDIT')")
        .bind(artifact.as_uuid()).bind(format!("relational-runtime-fixture/{artifact}"))
        .bind(size).execute(&mut *tx).await.unwrap();
    sqlx::query("INSERT INTO app.runtime_probe_observations(runtime_id,integration_revision,snapshot_artifact_id,observed_at,valid_until,outcome) VALUES($1,$2,$3,$4,$5,$6)")
        .bind(runtime.as_uuid()).bind(revision).bind(artifact.as_uuid())
        .bind(now).bind(now + ttl).bind(document).execute(&mut *tx).await.unwrap();
    sqlx::query(
        "UPDATE app.runtime_integrations SET last_capability_snapshot_artifact_id=$2 WHERE id=$1",
    )
    .bind(runtime.as_uuid())
    .bind(artifact.as_uuid())
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
    revision.to_string().try_into().unwrap()
}
