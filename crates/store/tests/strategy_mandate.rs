//! PostgreSQL verification with controlled runtime metadata, not native strategy execution.
#[path = "../../../tests/support/strategy_mandate.rs"]
mod support;
use contracts::{
    control::ListQuery, runtime::RuntimeProbeOutcomeV1, strategy_portfolio::MandateViewEnvelopeV2,
    Id,
};
use sqlx::PgPool;
use store::StoreError;

#[sqlx::test(migrations = "../../migrations")]
async fn strategy_mandate_replays_atomic_versions_and_preserves_branch_read_shapes(pool: PgPool) {
    let (f, request) = support::prepare(&pool, support::data::setup(&pool, None).await).await;
    let source = f
        .store
        .execution_assumption(&f.actor, request.content.execution_assumptions_id)
        .await
        .unwrap();
    assert_eq!(
        source.settings.account_kind,
        contracts::science::NativeAccountKind::Margin
    );
    assert_eq!(source.settings.leverage, "1".parse().unwrap());
    let (a, b) = tokio::join!(
        f.store.create_strategy_mandate(&f.actor, "same", &request),
        f.store.create_strategy_mandate(&f.actor, "same", &request),
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.version, 1);
    let MandateViewEnvelopeV2::Strategy(read) = f
        .store
        .mandate_envelope(&f.actor, a.resource.id)
        .await
        .unwrap()
    else {
        panic!("strategy read branch")
    };
    assert_eq!(
        serde_json::to_value(&read).unwrap(),
        serde_json::to_value(&a.resource).unwrap()
    );
    assert!(matches!(
        f.store.mandate(&f.actor, a.resource.id).await,
        Err(StoreError::NotFound)
    ));
    assert!(f
        .store
        .mandates(&f.actor, request.project_id, &ListQuery::default())
        .await
        .unwrap()
        .items
        .is_empty());
    let second = f
        .store
        .create_strategy_mandate(&f.actor, "second", &request)
        .await
        .unwrap()
        .resource;
    assert_eq!(second.version, 2);
    let first_page = f
        .store
        .mandates_envelope(
            &f.actor,
            request.project_id,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(first_page.items.len(), 1);
    assert_eq!(first_page.next_cursor, Some(second.id));
    let next = f
        .store
        .mandates_envelope(
            &f.actor,
            request.project_id,
            &ListQuery {
                cursor: first_page.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(next.items.len(), 1);
    assert!(next.next_cursor.is_none());
    let mut changed = request.clone();
    changed.content.target_ttl_seconds += 1;
    assert!(matches!(
        f.store
            .create_strategy_mandate(&f.actor, "same", &changed)
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert!(
        sqlx::query("UPDATE app.portfolio_mandates SET strategy_content='{}' WHERE id=$1")
            .bind(a.resource.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    support::observation::publish(
        &pool,
        request.runtime_id,
        RuntimeProbeOutcomeV1::Unavailable {
            reason: contracts::runtime::RuntimeProbeFailure::Unavailable,
        },
        chrono::Duration::seconds(60),
    )
    .await;
    assert!(
        f.store
            .create_strategy_mandate(&f.actor, "same", &request)
            .await
            .unwrap()
            .replayed
    );
    assert!(f
        .store
        .create_strategy_mandate(&f.actor, "unavailable", &request)
        .await
        .is_err());
}

#[sqlx::test(migrations = "../../migrations")]
async fn strategy_mandate_rejects_unbound_capital_costs_runtime_and_unsupported_risk_atomically(
    pool: PgPool,
) {
    let (f, request) = support::prepare(&pool, support::data::setup(&pool, None).await).await;
    for mutation in 0..7 {
        let mut bad = request.clone();
        match mutation {
            0 => bad.content.capital_assumption = "99".parse().unwrap(),
            1 => bad.content.base_currency = "EUR".into(),
            2 => bad.content.constraints.transaction_costs_ref = Id::new(),
            3 => bad.content.universe_version_id = Id::new(),
            4 => bad.expected_runtime_revision = "999".to_owned().try_into().unwrap(),
            5 => bad.content.constraints.max_ex_ante_risk = Some("0.01".parse().unwrap()),
            _ => bad.content.exposure_tolerance = "0.00001".parse().unwrap(),
        }
        assert!(
            f.store
                .create_strategy_mandate(&f.actor, "retry", &bad)
                .await
                .is_err(),
            "mutation {mutation}"
        );
    }
    let count:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.portfolio_mandates),(SELECT count(*) FROM app.command_receipts WHERE operation='MANDATE_CREATE')").fetch_one(&pool).await.unwrap();
    assert_eq!(count, (0, 0));
    assert_eq!(
        f.store
            .create_strategy_mandate(&f.actor, "retry", &request)
            .await
            .unwrap()
            .resource
            .version,
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn strategy_keeps_native_account_market_validation_and_rejects_leveraged_sources(
    pool: PgPool,
) {
    let (f, mut execution) =
        support::execution_request(&pool, support::data::setup(&pool, None).await).await;
    assert_eq!(
        execution.settings.account_kind,
        contracts::science::NativeAccountKind::Margin
    );
    execution.settings.account_kind = contracts::science::NativeAccountKind::Cash;
    let rejected = f
        .store
        .create_execution_assumptions(
            &f.actor,
            "unsupported-currency-pair-account",
            &execution,
            |id, size| support::data::read(f.objects.clone(), id, size),
            |_| async { panic!("invalid account-market settings cannot publish") },
        )
        .await;
    assert!(matches!(
        rejected,
        Err(StoreError::Domain(
            domain::DomainError::CapabilityUnavailable("execution_assumption_account_instrument")
        ))
    ));
    execution.settings.account_kind = contracts::science::NativeAccountKind::Margin;
    execution.settings.leverage = "2".parse().unwrap();
    let (f, request) = support::from_execution(&pool, f, execution).await;
    let source = f
        .store
        .execution_assumption(&f.actor, request.content.execution_assumptions_id)
        .await
        .unwrap();
    assert_eq!(
        source.settings.account_kind,
        contracts::science::NativeAccountKind::Margin
    );
    assert_eq!(source.settings.leverage, "2".parse().unwrap());
    assert!(matches!(
        f.store
            .create_strategy_mandate(&f.actor, "no-leverage", &request)
            .await,
        Err(StoreError::Domain(
            domain::DomainError::CapabilityUnavailable("strategy_account_model")
        ))
    ));
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.portfolio_mandates),(SELECT count(*) FROM app.command_receipts WHERE operation='MANDATE_CREATE')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0));
}
