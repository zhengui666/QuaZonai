//! Actual HTTP authority/intake/registration boundary; no real account execution.
use super::*;
fn paper_body(project: Id) -> Value {
    let mut value = observation(project);
    value["binding"]["native_trader_id"] = json!("QZEXIT-001");
    value["binding"]["native_account_id"] = json!("QZEXIT-001");
    value["snapshot"]["account_id"] = json!("QZEXIT-001");
    json!({"schema_version":2,"native_client_id":"QZ-EXIT-SANDBOX","observation":value})
}
fn changed_configuration(a: &AccountFixture) -> server::paper_capital_exit::PaperCapitalExitOwners {
    server::paper_capital_exit::PaperCapitalExitOwners::new(vec![
        server::paper_capital_exit::PaperCapitalExitOwnerConfiguration {
            schema_version: SchemaV1,
            project_id: a.project,
            downstream_id: a.downstream,
            native_trader_id: "QZEXIT-001".into(),
            native_account_id: "QZEXIT-001".into(),
            native_client_id: "QZ-EXIT-SANDBOX".into(),
            native_version: "0.63.0".into(),
            venue: "QZEXIT".into(),
            collateral_currency: "USDC".into(),
            instrument_id: "OTHER.QZEXIT".into(),
            controlled_strategy_ids: vec!["EXIT-FIXTURE-001".into()],
        },
    ])
    .unwrap()
}
#[sqlx::test(migrations = "../../migrations")]
async fn paper_capital_exit_disabled_intake_preserves_original_receipt_without_registration_header(
    pool: PgPool,
) {
    let a = fixture(&pool).await;
    let response = a
        .http
        .post(format!("{}{SUBMIT_CLIENT}", a.origin))
        .bearer_auth(&a.token)
        .json(&paper_body(a.project))
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert!(
        !response
            .headers()
            .contains_key(server::paper_capital_exit::REGISTERED_SOURCE_HEADER)
    );
    let body: Value = response.json().await.unwrap();
    assert_eq!(body["schema_version"], 2);
    assert!(body.get("capital_exit_owner").is_none());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.managed_capital_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
#[sqlx::test(migrations = "../../migrations")]
async fn paper_capital_exit_enabled_receipt_ack_requires_successful_exact_source_registration(
    pool: PgPool,
) {
    let a = fixture_with_paper_exit(&pool, true).await;
    let request = paper_body(a.project);
    let response = a
        .http
        .post(format!("{}{SUBMIT_CLIENT}", a.origin))
        .bearer_auth(&a.token)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    let header = server::paper_capital_exit::REGISTERED_SOURCE_HEADER;
    assert_eq!(response.headers().get_all(header).iter().count(), 1);
    let registered = response.headers()[header].to_str().unwrap().to_owned();
    let original: Value = response.json().await.unwrap();
    assert_eq!(original["resource"]["source_id"], registered);
    assert_eq!(original["resource"]["observation"], request["observation"]);
    let source: Id = serde_json::from_value(original["resource"]["source_id"].clone()).unwrap();
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM app.managed_capital_reservations WHERE paper_account_source_id=$1 AND managed_account_key=$2").bind(source.as_uuid()).bind(format!("paper-native:{source}")).fetch_one(&pool).await.unwrap();
    assert_eq!(count, 1);
    // Same authenticated native packet is committed already. A conflicting
    // deployment config must fail registration, not return a false success ACK.
    let (other_origin, _listener) =
        client::listen_with_paper_owners(&a.f, changed_configuration(&a)).await;
    let failed = a
        .http
        .post(format!("{other_origin}{SUBMIT_CLIENT}"))
        .bearer_auth(&a.token)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(failed.status(), StatusCode::CONFLICT);
    assert!(!failed.headers().contains_key(header));
    let retried = a
        .http
        .post(format!("{}{SUBMIT_CLIENT}", a.origin))
        .bearer_auth(&a.token)
        .json(&request)
        .send()
        .await
        .unwrap();
    assert_eq!(retried.status(), StatusCode::CREATED);
    assert_eq!(retried.headers()[header].to_str().unwrap(), registered);
    let retried: Value = retried.json().await.unwrap();
    assert_eq!(retried["replayed"], true);
    assert_eq!(retried["resource"], original["resource"]);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.native_account_sources")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.managed_capital_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
