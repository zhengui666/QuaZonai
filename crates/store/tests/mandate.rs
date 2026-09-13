//! Real PostgreSQL commands; native observations are explicitly controlled fixtures.
#[path = "../../../tests/support/mandate.rs"]
mod support;
use contracts::{
    control::ListQuery,
    portfolio::*,
    runtime::{RuntimeProbeFailure, RuntimeProbeOutcomeV1},
    Id,
};
use sqlx::PgPool;
use store::StoreError;
use support::research_support as research;

#[sqlx::test(migrations = "../../migrations")]
async fn variance_bound_requires_both_native_adapter_and_cone_without_partial_writes(pool: PgPool) {
    let (store, actor) = research::operator(&pool).await;
    let mut request = support::request(&pool, &store, &actor).await;
    request.content.constraints.max_ex_ante_risk = Some("0.0001".parse().unwrap());
    let original: serde_json::Value = sqlx::query_scalar("SELECT o.outcome FROM app.runtime_probe_observations o JOIN app.runtime_integrations r ON r.last_capability_snapshot_artifact_id=o.snapshot_artifact_id WHERE r.id=$1")
        .bind(request.runtime_id.as_uuid()).fetch_one(&pool).await.unwrap();
    let RuntimeProbeOutcomeV1::Available { capabilities } =
        serde_json::from_value(original["result"].clone()).unwrap()
    else {
        panic!("original controlled observation")
    };
    for mutation in 0..3 {
        let mut cap = capabilities.clone();
        if mutation != 0 {
            cap.engine_versions
                .insert("portfolio-variance-bound".into(), "1".into());
        }
        if mutation != 1 {
            cap.solver_capabilities.push("SECOND_ORDER_CONE".into());
        }
        request.expected_runtime_revision = support::observation::publish(
            &pool,
            request.runtime_id,
            RuntimeProbeOutcomeV1::Available { capabilities: cap },
            chrono::Duration::seconds(60),
        )
        .await;
        let result = store
            .create_mandate(&actor, "variance-bound", &request)
            .await;
        if mutation < 2 {
            assert!(matches!(
                result,
                Err(StoreError::Domain(
                    domain::DomainError::CapabilityUnavailable("portfolio_variance_bound")
                ))
            ));
            let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.portfolio_mandates")
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(count, 0);
        } else {
            let saved = result.unwrap().resource;
            assert_eq!(saved.version, 1);
            assert_eq!(
                store.mandate(&actor, saved.id).await.unwrap().content,
                request.content
            );
        }
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn portfolio_admission_without_its_running_cycle_never_publishes_or_charges(pool: PgPool) {
    let (store, actor) = research::operator(&pool).await;
    let source = support::request(&pool, &store, &actor).await;
    let mandate = store
        .create_mandate(&actor, "mandate", &source)
        .await
        .unwrap()
        .resource;
    let request = PortfolioBuildRequestV1 {
        schema_version: contracts::SchemaV1,
        cycle_id: Id::new(),
        mandate_id: mandate.id,
        input_set_id: Id::new(),
        runtime_id: source.runtime_id,
        expected_runtime_revision: source.expected_runtime_revision,
        current_weights_source: contracts::portfolio::PortfolioBuildWeightsV1::ForwardSnapshot {
            snapshot_id: Id::new(),
        },
        environment: contracts::forward::ForwardEnvironmentV1::Paper,
        members: vec![
            PortfolioMemberSelectionV1 {
                qualification_id: Id::new(),
                ensemble_weight: "0.5".parse().unwrap(),
            },
            PortfolioMemberSelectionV1 {
                qualification_id: Id::new(),
                ensemble_weight: "0.5".parse().unwrap(),
            },
        ],
        limits: contracts::lifecycle::JobLimitsV1 {
            schema_version: contracts::SchemaV1,
            experiments: 0,
            cpu_seconds: contracts::DbCounter::new(10).unwrap(),
            wall_seconds: 10,
            memory_mib: 64,
            output_bytes: contracts::DbCounter::new(1024).unwrap(),
        },
    };
    assert!(matches!(
        store
            .start_portfolio_build(
                &actor,
                "missing-cycle",
                &request,
                |_, _| async { panic!("no original source reads without admitted Cycle") },
                |_| async { panic!("no files without admitted Cycle") }
            )
            .await,
        Err(StoreError::Invalid("portfolio_cycle"))
    ));
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.portfolio_build_tasks),(SELECT count(*) FROM app.runs WHERE kind='PORTFOLIO_BUILD'),(SELECT count(*) FROM app.command_receipts WHERE operation='PORTFOLIO_BUILD')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn mandate_versions_are_atomic_immutable_and_original_receipts_replay(pool: PgPool) {
    let (store, actor) = research::operator(&pool).await;
    let request = support::request(&pool, &store, &actor).await;
    let (a, b) = tokio::join!(
        store.create_mandate(&actor, "same", &request),
        store.create_mandate(&actor, "same", &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.version, 1);
    assert_eq!(
        store.mandate(&actor, a.resource.id).await.unwrap().content,
        request.content
    );
    let (a, b) = tokio::join!(
        store.create_mandate(&actor, "a", &request),
        store.create_mandate(&actor, "b", &request)
    );
    let mut versions = [a.unwrap().resource.version, b.unwrap().resource.version];
    versions.sort();
    assert_eq!(versions, [2, 3]);
    let page = store
        .mandates(
            &actor,
            request.project_id,
            &ListQuery {
                cursor: None,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    let last = store
        .mandates(
            &actor,
            request.project_id,
            &ListQuery {
                cursor: page.next_cursor,
                limit: 2,
            },
        )
        .await
        .unwrap();
    assert_eq!(last.items.len(), 1);
    let id = last.items[0].id;
    assert!(
        sqlx::query("UPDATE app.portfolio_mandates SET capital_assumption=101 WHERE id=$1")
            .bind(id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    let mut different = request.clone();
    different.content.exposure_tolerance = "0.00001".parse().unwrap();
    assert!(matches!(
        store.create_mandate(&actor, "same", &different).await,
        Err(StoreError::IdempotencyConflict)
    ));
    support::observation::publish(
        &pool,
        request.runtime_id,
        RuntimeProbeOutcomeV1::Unavailable {
            reason: RuntimeProbeFailure::Unavailable,
        },
        chrono::Duration::seconds(60),
    )
    .await;
    assert!(
        store
            .create_mandate(&actor, "same", &request)
            .await
            .unwrap()
            .replayed
    );
    assert!(store
        .create_mandate(&actor, "unavailable", &request)
        .await
        .is_err());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.portfolio_mandates")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_mandate_models_references_and_revisions_leave_no_partial_version(pool: PgPool) {
    let (store, actor) = research::operator(&pool).await;
    let request = support::request(&pool, &store, &actor).await;
    for mutation in 0..7 {
        let mut bad = request.clone();
        match mutation {
            0 => bad.content.capital_assumption = "99".parse().unwrap(),
            1 => bad.content.constraints.transaction_costs_ref = Id::new(),
            2 => bad.content.required_evaluation_policy_id = Id::new(),
            3 => bad.content.risk_measure = AllocationRisk::Cvar,
            4 => bad.content.optimizer = bad.content.alpha_ensemble.clone(),
            5 => bad.expected_runtime_revision = "999".to_owned().try_into().unwrap(),
            _ => bad.content.universe_version_id = Id::new(),
        }
        assert!(
            store.create_mandate(&actor, "retry", &bad).await.is_err(),
            "mutation {mutation}"
        );
    }
    let count:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.portfolio_mandates),(SELECT count(*) FROM app.command_receipts WHERE operation='MANDATE_CREATE')").fetch_one(&pool).await.unwrap();
    assert_eq!(count, (0, 0));
    assert_eq!(
        store
            .create_mandate(&actor, "retry", &request)
            .await
            .unwrap()
            .resource
            .version,
        1
    );
}
