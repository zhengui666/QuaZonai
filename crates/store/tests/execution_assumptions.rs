//! Real PostgreSQL/file transactions; controlled sources, never REAL qualification.
#[path = "../../../tests/support/execution_assumptions.rs"]
mod support;
use contracts::{control::ListQuery, Id};
use sqlx::PgPool;
use store::StoreError;
use support::{data, prepare};

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
    for change in 0..3 {
        let mut invalid = request.clone();
        match change {
            0 => invalid.settings.fee_rates[0].maker = "0".parse().unwrap(),
            1 => invalid.dataset_revision_id = Id::new(),
            _ => invalid.settings.base_currency = "EUR".into(),
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
