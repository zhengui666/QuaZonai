//! Real original Store/file source rereads, not market or qualification evidence.
#[path = "../../../../tests/support/native_liquidity.rs"]
mod measurement;
#[path = "../../../../tests/support/execution_assumptions.rs"]
mod support;
use crate::StoreError;
use contracts::{execution_assumptions::BarLiquidityAssumptionV1, SchemaV1};
use sqlx::PgPool;
use support::data;

#[sqlx::test(migrations = "../../migrations")]
async fn frozen_liquidity_rereads_original_report_and_distinguishes_corruption(pool: PgPool) {
    let (f, mut request) = support::prepare(&pool, data::setup(&pool, None).await).await;
    let report =
        measurement::measured_report(&pool, &f.store, &f.actor, &f.objects, &request).await;
    request.bar_liquidity = Some(BarLiquidityAssumptionV1 {
        schema_version: SchemaV1,
        report_artifact_id: report,
        maximum_age_seconds: u32::MAX,
        participation_limit: "0.1".parse().unwrap(),
    });
    let created = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "measured-source",
            &request,
            |id, size| data::read(f.objects.clone(), id, size),
            |o| {
                std::future::ready(
                    f.objects
                        .put(o.id, &o.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    let mut tx = pool.begin().await.unwrap();
    let source = super::frozen(
        &mut tx,
        f.project,
        request.runtime_id,
        created.id,
        &mut |id, size| data::read(f.objects.clone(), id, size),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        source.binding.assumption,
        request.bar_liquidity.clone().unwrap()
    );
    assert_eq!(
        source.binding.source.dataset_revision_id,
        request.dataset_revision_id
    );
    assert_eq!(
        source.report.datasets[0]
            .last_bar_notionals
            .as_ref()
            .unwrap()[0]
            .notional_value,
        "1000".parse().unwrap()
    );
    tx.rollback().await.unwrap();
    let mut tx = pool.begin().await.unwrap();
    let broken = super::frozen(
        &mut tx,
        f.project,
        request.runtime_id,
        created.id,
        &mut |id, size| {
            let objects = f.objects.clone();
            async move {
                let bytes = data::read(objects, id, size).await?;
                if id != report {
                    return Ok(bytes);
                }
                let text = String::from_utf8(bytes).unwrap();
                // Same byte length, immutable original currency changed.
                Ok(text
                    .replace("\"currency\":\"USD\"", "\"currency\":\"EUR\"")
                    .into_bytes())
            }
        },
    )
    .await;
    assert!(matches!(broken, Err(StoreError::Integrity)));
    tx.rollback().await.unwrap();
    let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let event = source.report.datasets[0]
        .last_bar_notionals
        .as_ref()
        .unwrap()[0]
        .event_ns
        .get()
        / 1_000_000_000;
    request.bar_liquidity.as_mut().unwrap().maximum_age_seconds =
        u32::try_from(now.timestamp() as u64 - event + 3).unwrap();
    let short = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "short-lived-measured-source",
            &request,
            |id, size| data::read(f.objects.clone(), id, size),
            |o| {
                std::future::ready(
                    f.objects
                        .put(o.id, &o.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    let until = short.bar_liquidity_valid_until.unwrap();
    tokio::time::sleep(
        (until - chrono::Utc::now()).to_std().unwrap() + std::time::Duration::from_millis(1),
    )
    .await;
    let mut tx = pool.begin().await.unwrap();
    let expired = super::frozen(
        &mut tx,
        f.project,
        request.runtime_id,
        short.id,
        &mut |id, size| data::read(f.objects.clone(), id, size),
    )
    .await;
    assert!(matches!(
        expired,
        Err(StoreError::Invalid("bar_liquidity_expired"))
    ));
    tx.rollback().await.unwrap();
    assert_eq!(
        f.store
            .execution_assumption(&f.actor, short.id)
            .await
            .unwrap()
            .bar_liquidity_valid_until,
        Some(until)
    );
}
