//! Real PostgreSQL/file transactions; controlled sources, never REAL qualification.
#[path = "../../../tests/support/native_liquidity.rs"]
mod bar_liquidity;
#[path = "../../../tests/support/execution_assumptions.rs"]
mod support;
use contracts::{control::ListQuery, Id};
use sqlx::PgPool;
use store::StoreError;
use support::{data, prepare};

#[sqlx::test(migrations = "../../migrations")]
async fn rolling_policy_freezes_original_file_without_inventing_snapshot_or_expiry(pool: PgPool) {
    let (f, mut request) = prepare(&pool, data::setup(&pool, None).await).await;
    request.rolling_liquidity = Some(contracts::science::NativeRollingBarLiquidityPolicyV1 {
        schema_version: contracts::SchemaV1,
        maximum_age_seconds: 3600,
        participation_limit: "0.123456789012345678".parse().unwrap(),
    });
    for case in 0..4 {
        let mut invalid = request.clone();
        let policy = invalid.rolling_liquidity.as_mut().unwrap();
        match case {
            0 => policy.maximum_age_seconds = 0,
            1 => policy.participation_limit = "0".parse().unwrap(),
            2 => policy.participation_limit = "1.000000000000000001".parse().unwrap(),
            _ => {
                invalid.bar_liquidity =
                    Some(contracts::execution_assumptions::BarLiquidityAssumptionV1 {
                        schema_version: contracts::SchemaV1,
                        report_artifact_id: Id::new(),
                        maximum_age_seconds: 3600,
                        participation_limit: "0.1".parse().unwrap(),
                    })
            }
        }
        assert!(f
            .store
            .create_execution_assumptions(
                &f.actor,
                "invalid",
                &invalid,
                |id, size| data::read(f.objects.clone(), id, size),
                |_| async { panic!("invalid policy cannot publish") }
            )
            .await
            .is_err());
    }
    let mut publications = 0;
    let created = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "rolling",
            &request,
            |id, size| data::read(f.objects.clone(), id, size),
            |object| {
                publications += 1;
                let objects = f.objects.clone();
                async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }
            },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(publications, 2);
    assert!(created.bar_liquidity.is_none());
    assert!(created.bar_liquidity_valid_until.is_none());
    assert_eq!(created.rolling_liquidity, request.rolling_liquidity);
    let artifact = created.rolling_liquidity_artifact_id.unwrap();
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1 AND kind='PARAMETERS' AND schema_name='qz.rolling_bar_liquidity' AND schema_version='1' AND origin='SYNTHETIC' AND access_class='RESEARCH'")
        .bind(artifact.as_uuid()).fetch_one(&pool).await.unwrap();
    let original: contracts::science::NativeRollingBarLiquidityPolicyV1 = serde_json::from_slice(
        &f.objects
            .read(artifact, size.to_string().try_into().unwrap())
            .unwrap(),
    )
    .unwrap();
    assert_eq!(Some(original), request.rolling_liquidity);
    let replay = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "rolling",
            &request,
            |_, _| async { panic!("replay must not read") },
            |_| async { panic!("replay must not publish") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        replay.resource.rolling_liquidity_artifact_id,
        Some(artifact)
    );
    assert!(sqlx::query("UPDATE app.execution_assumption_sources SET rolling_liquidity=NULL WHERE assumptions_id=$1").bind(created.id.as_uuid()).execute(&pool).await.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn bar_liquidity_requires_original_native_output_and_freezes_its_expiry(pool: PgPool) {
    use contracts::execution_assumptions::BarLiquidityAssumptionV1;
    let (f, mut request) = prepare(&pool, data::setup(&pool, None).await).await;
    let dataset = f
        .store
        .get_dataset_revision(&f.actor, request.dataset_revision_id)
        .await
        .unwrap();
    request.bar_liquidity = Some(BarLiquidityAssumptionV1 {
        schema_version: contracts::SchemaV1,
        report_artifact_id: dataset.quality_artifact_id,
        maximum_age_seconds: u32::MAX,
        participation_limit: "0.1".parse().unwrap(),
    });
    let forged = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "registration-is-not-measurement",
            &request,
            |id, size| data::read(f.objects.clone(), id, size),
            |_| async { panic!("unmeasured source publishes nothing") },
        )
        .await;
    assert!(matches!(
        forged,
        Err(StoreError::Invalid("bar_liquidity_native_source"))
    ));
    let report =
        bar_liquidity::measured_report(&pool, &f.store, &f.actor, &f.objects, &request).await;
    request.bar_liquidity.as_mut().unwrap().report_artifact_id = report;
    let mut expired = request.clone();
    expired.bar_liquidity.as_mut().unwrap().maximum_age_seconds = 1;
    assert!(f
        .store
        .create_execution_assumptions(
            &f.actor,
            "expired",
            &expired,
            |id, size| data::read(f.objects.clone(), id, size),
            |_| async { panic!("expired source publishes nothing") }
        )
        .await
        .is_err());
    let created = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "measured",
            &request,
            |id, size| data::read(f.objects.clone(), id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(created.bar_liquidity, request.bar_liquidity);
    let expected = data::catalog_fixture::instant(180 + i64::from(u32::MAX));
    assert_eq!(created.bar_liquidity_valid_until, Some(expected));
    assert_eq!(
        f.store
            .execution_assumption(&f.actor, created.id)
            .await
            .unwrap()
            .bar_liquidity_valid_until,
        Some(expected)
    );
    let replay = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "measured",
            &request,
            |_, _| async { panic!("replay reads nothing") },
            |_| async { panic!("replay writes nothing") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.bar_liquidity, created.bar_liquidity);
    assert!(sqlx::query("UPDATE app.execution_assumption_sources SET bar_liquidity_valid_until=clock_timestamp() WHERE assumptions_id=$1").bind(created.id.as_uuid()).execute(&pool).await.is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_sources_models_receipts_and_immutable_settings_are_bound(pool: PgPool) {
    let (f, request) = prepare(&pool, data::setup(&pool, None).await).await;
    let created = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "create",
            &request,
            |id, size| data::read(f.objects.clone(), id, size),
            |object| {
                let objects = f.objects.clone();
                async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }
            },
        )
        .await
        .unwrap();
    let result = &created.resource;
    assert_eq!(result.dataset_revision_id, request.dataset_revision_id);
    assert_eq!(result.input_set_id, request.input_set_id);
    assert_eq!(
        serde_json::to_value(&result.settings).unwrap(),
        serde_json::to_value(&request.settings).unwrap()
    );
    let stored: serde_json::Value = sqlx::query_scalar(
        "SELECT settings FROM app.execution_assumption_sources WHERE assumptions_id=$1",
    )
    .bind(result.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(stored, serde_json::to_value(&request.settings).unwrap());
    assert!(sqlx::query(
        "UPDATE app.execution_assumption_sources SET settings='{}' WHERE assumptions_id=$1"
    )
    .bind(result.id.as_uuid())
    .execute(&pool)
    .await
    .is_err());
    assert!(
        sqlx::query("DELETE FROM app.execution_assumptions WHERE id=$1")
            .bind(result.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    assert_eq!(
        f.store
            .execution_assumption(&f.actor, result.id)
            .await
            .unwrap()
            .id,
        result.id
    );
    let page = f
        .store
        .execution_assumptions(
            &f.actor,
            f.project,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);
    assert!(page.next_cursor.is_none());
    let replay = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "create",
            &request,
            |_, _| async { panic!("replay must not read sources") },
            |_| async { panic!("replay must not publish objects") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        serde_json::to_value(replay.resource).unwrap(),
        serde_json::to_value(result).unwrap()
    );
    let mut changed = request.clone();
    changed.settlement_rule_ref = "different".into();
    assert!(matches!(
        f.store
            .create_execution_assumptions(
                &f.actor,
                "create",
                &changed,
                |_, _| async { panic!() },
                |_| async { panic!() }
            )
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn forged_fees_wrong_sources_and_failed_object_publication_leave_no_assumption(pool: PgPool) {
    let (f, request) = prepare(&pool, data::setup(&pool, None).await).await;
    for change in 0..4 {
        let mut invalid = request.clone();
        match change {
            0 => invalid.settings.fee_rates[0].maker = "0".parse().unwrap(),
            1 => invalid.dataset_revision_id = Id::new(),
            2 => invalid.settings.base_currency = "EUR".into(),
            _ => invalid.settings.account_kind = contracts::science::NativeAccountKind::Cash,
        }
        assert!(f
            .store
            .create_execution_assumptions(
                &f.actor,
                "invalid",
                &invalid,
                |id, size| data::read(f.objects.clone(), id, size),
                |_| async { panic!("invalid sources must never publish") }
            )
            .await
            .is_err());
    }
    assert!(f
        .store
        .create_execution_assumptions(
            &f.actor,
            "publish-failed",
            &request,
            |id, size| data::read(f.objects.clone(), id, size),
            |_| async { Err(StoreError::Integrity) }
        )
        .await
        .is_err());
    let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.execution_assumption_sources),(SELECT count(*) FROM app.artifacts WHERE schema_name='qz.native_simulation_settings'),(SELECT count(*) FROM app.command_receipts WHERE operation='EXECUTION_ASSUMPTIONS_CREATE')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
}
