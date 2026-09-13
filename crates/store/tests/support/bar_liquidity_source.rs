//! Real original Store/file source rereads, not market or qualification evidence.

#[sqlx::test(migrations = "../../migrations")]
async fn portfolio_reads_original_research_partitions_only_when_explicitly_requested(
    pool: sqlx::PgPool,
) {
    use crate::data_validation::dataset_bindings;
    use chrono::{DateTime, Utc};
    use contracts::{
        research::{DataPartition, DataUse, InputItemV1, InputPurpose, InputSetCreate},
        runtime_jobs::RuntimeInputV1,
    };
    let f = data::setup(&pool, None).await;
    let dataset = data::complete(
        &f,
        data::ticket(&f, "original-research-only", &data::request(&f)).await,
        serde_json::to_vec(&data::catalog_fixture::metadata()).unwrap(),
    )
    .await
    .unwrap()
    .resource;
    let issued: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let grant = f
        .store
        .create_data_grant(
            &f.actor,
            "portfolio-paper-license",
            &contracts::data::DataGrantCreate {
                schema_version: SchemaV1,
                source_id: f.source.id,
                license_reference: "controlled research and paper fixture".into(),
                evidence_artifact_id: f.proof,
                allowed_uses: DataUse::ResearchAndPaper,
                valid_from: issued - chrono::Duration::hours(1),
                valid_until: None,
            },
        )
        .await
        .unwrap()
        .resource;
    let mut items = Vec::new();
    for (index, role) in [DataPartition::Discovery, DataPartition::Validation]
        .into_iter()
        .enumerate()
    {
        let mut metadata = data::catalog_fixture::metadata();
        metadata.partition = role;
        metadata.storage_version = format!("portfolio-v{index}");
        let mut registration = data::request(&f);
        registration.grant_id = grant.id;
        registration.native_storage_version = metadata.storage_version.clone();
        let dataset = data::complete(
            &f,
            data::ticket(&f, &format!("portfolio-dataset-{index}"), &registration).await,
            serde_json::to_vec(&metadata).unwrap(),
        )
        .await
        .unwrap()
        .resource;
        items.push(InputItemV1::Dataset {
            dataset_revision_id: dataset.id,
            role,
        });
    }
    let intent = InputSetCreate {
        schema_version: SchemaV1,
        project_id: f.project,
        purpose: InputPurpose::Portfolio,
        decision_cutoff: data::catalog_fixture::instant(300),
        items,
    };
    let mut unlicensed = intent.clone();
    unlicensed.items = vec![InputItemV1::Dataset {
        dataset_revision_id: dataset.id,
        role: DataPartition::Discovery,
    }];
    assert!(f
        .store
        .create_input_set(
            &f.actor,
            "research-only-cannot-authorize-portfolio",
            &unlicensed
        )
        .await
        .is_err());
    let input = f
        .store
        .create_input_set(&f.actor, "portfolio-research-input", &intent)
        .await
        .unwrap()
        .resource
        .header
        .id;
    let mut tx = pool.begin().await.unwrap();
    let bound = dataset_bindings(
        &mut tx,
        input,
        f.project,
        f.runtime.id,
        &[InputPurpose::Portfolio],
        &mut |id, size| data::read(f.objects.clone(), id, size),
    )
    .await
    .unwrap();
    assert_eq!(bound.len(), 2);
    for (binding, item) in bound.iter().zip(&intent.items) {
        let InputItemV1::Dataset {
            dataset_revision_id,
            role,
        } = item
        else {
            unreachable!()
        };
        assert_eq!(binding.selection.dataset_revision_id, *dataset_revision_id);
        assert_eq!(binding.metadata.partition, *role);
        assert!(
            matches!(&binding.input, RuntimeInputV1::Dataset { revision_id, role: actual, .. } if revision_id == dataset_revision_id && actual == role)
        );
        assert_eq!(
            binding.selection.selection.decision_cutoff_ns.get(),
            300_000_000_000
        );
    }
    tx.rollback().await.unwrap();
    for purposes in [
        vec![InputPurpose::Discovery, InputPurpose::Validation],
        vec![InputPurpose::Forward],
        vec![InputPurpose::Sealed],
    ] {
        let mut tx = pool.begin().await.unwrap();
        assert!(dataset_bindings(
            &mut tx,
            input,
            f.project,
            f.runtime.id,
            &purposes,
            &mut |_, _| async { panic!("unrequested purpose cannot read source bytes") },
        )
        .await
        .is_err());
        tx.rollback().await.unwrap();
    }
    let mut tx = pool.begin().await.unwrap();
    let objects = &f.objects;
    assert!(matches!(
        dataset_bindings(
            &mut tx,
            input,
            f.project,
            f.runtime.id,
            &[InputPurpose::Portfolio],
            &mut |id, size| async move {
                let bytes = data::read(objects.clone(), id, size).await?;
                let text = String::from_utf8(bytes).unwrap();
                assert!(text.contains("\"row_count\":\"3\""));
                Ok(text
                    .replace("\"row_count\":\"3\"", "\"row_count\":\"4\"")
                    .into_bytes())
            },
        )
        .await,
        Err(StoreError::Integrity)
    ));
    tx.rollback().await.unwrap();
    let validation = contracts::data::DataValidateRequest {
        schema_version: SchemaV1,
        project_id: f.project,
        input_set_id: input,
        runtime_id: f.runtime.id,
        expected_runtime_revision: f.runtime.revision,
        limits: contracts::lifecycle::JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: contracts::DbCounter::new(10).unwrap(),
            wall_seconds: 60,
            memory_mib: 512,
            output_bytes: contracts::DbCounter::new(65_536).unwrap(),
        },
    };
    assert!(f
        .store
        .start_data_validation(
            &f.actor,
            "portfolio-is-not-alpha-validation",
            &validation,
            |_, _| async { panic!("standalone DATA_VALIDATE has no Portfolio authority") },
            |_| async { panic!("wrong purpose cannot publish a task") },
        )
        .await
        .is_err());
    let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.evaluations),(SELECT count(*) FROM app.qualifications)").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
}

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
