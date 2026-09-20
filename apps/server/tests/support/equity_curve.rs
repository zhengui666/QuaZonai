//! Real HTTP, PostgreSQL, immutable artifacts and original terminal adoption.
//! The upstream Study protocol fixture is controlled; real Nautilus is tested by job.
use super::*;
use contracts::equity_curve::*;

pub(super) async fn verify(
    context: (
        &PgPool,
        &Store,
        &store::authority::Actor,
        &cycle_support::Fixture,
    ),
    transport: (&reqwest::Client, &str, &str),
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    candidate: Id,
    cancelled: Id,
) {
    let (pool, store, actor, f) = context;
    let (client, origin, token) = transport;
    let url = |id: Id| format!("{origin}/api/v2/evaluations/{id}/equity-curve");
    assert_eq!(
        client.get(url(cancelled)).send().await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let response = client
        .get(url(cancelled))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    assert!(response.headers()[reqwest::header::CACHE_CONTROL]
        .to_str()
        .unwrap()
        .contains("no-store"));
    let view: EquityCurveV1 = response.json().await.unwrap();
    assert!(matches!(
        view.curve,
        EquityCurveDataV1::Unavailable {
            reason_code: EquityUnavailableReason::SimulationFailed
        }
    ));
    for query in [
        "start_ns=2&end_ns=1",
        "start_ns=01",
        "start_ns=9223372036854775808",
        "resolution=UNKNOWN",
        "artifact_id=other",
    ] {
        assert_eq!(
            client
                .get(format!("{}?{query}", url(cancelled)))
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let other_kind: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM app.evaluations WHERE project_id=$1 AND evaluation_kind='FORWARD' LIMIT 1",
    )
    .bind(f.data.project.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap();
    for id in [Id::new(), other_kind.to_string().try_into().unwrap()] {
        assert_eq!(
            client
                .get(url(id))
                .bearer_auth(token)
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::NOT_FOUND
        );
    }
    for infeasible in [false, true] {
        let id = Box::pin(qualified_portfolio::equity_curve_checks::publish(
            context, build, candidate, infeasible,
        ))
        .await;
        let before: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.evaluations),(SELECT count(*) FROM app.artifacts)")
            .fetch_one(pool).await.unwrap();
        let response = client.get(url(id)).bearer_auth(token).send().await.unwrap();
        assert_eq!(
            response.status(),
            reqwest::StatusCode::OK,
            "{}",
            response.text().await.unwrap()
        );
        let view: EquityCurveV1 = client
            .get(url(id))
            .bearer_auth(token)
            .send()
            .await
            .unwrap()
            .json()
            .await
            .unwrap();
        assert_eq!(view.project_id, f.data.project);
        assert_eq!(view.candidate_id, candidate);
        assert_eq!(view.evaluation_id, id);
        let encoded = serde_json::to_string(&view).unwrap();
        for forbidden in [
            "canonical_result",
            "portfolio_snapshots",
            "native_reports",
            "account_id",
            "orders",
            "positions",
            "storage_uri",
        ] {
            assert!(
                !encoded.contains(forbidden),
                "unexpected raw report field {forbidden}"
            );
        }
        match view.curve {
            EquityCurveDataV1::Ready {
                source_artifact_id,
                series,
            } => {
                assert!(!infeasible);
                assert!(!series.points.is_empty());
                let source_size: i64 =
                    sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
                        .bind(source_artifact_id.as_uuid())
                        .fetch_one(pool)
                        .await
                        .unwrap();
                let source: contracts::science::NativePortfolioStudyResultV1 =
                    serde_json::from_slice(
                        &f.read(
                            source_artifact_id,
                            DbCounter::new(source_size as u64).unwrap(),
                        )
                        .await
                        .unwrap(),
                    )
                    .unwrap();
                let expected = domain::execution::portfolio_equity_curve(
                    &source.simulation_request.unwrap(),
                    &source.simulation.unwrap(),
                    &Default::default(),
                )
                .unwrap();
                assert_eq!(series.points, expected.points);
                let first = series.points[0].timestamp_ns.get();
                let response: EquityCurveV1 = client
                    .get(format!("{}?start_ns={first}&end_ns={first}", url(id)))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap()
                    .json()
                    .await
                    .unwrap();
                let EquityCurveDataV1::Ready {
                    series: bounded, ..
                } = response.curve
                else {
                    panic!("native point");
                };
                assert_eq!(bounded.points, vec![series.points[0].clone()]);
                assert_eq!(bounded.starting_capital, series.starting_capital);
                let raw = client
                    .get(format!(
                        "{origin}/api/v2/artifacts/{source_artifact_id}/content"
                    ))
                    .bearer_auth(token)
                    .send()
                    .await
                    .unwrap();
                assert!(
                    !raw.status().is_success(),
                    "read projection must not open raw artifacts"
                );
            }
            EquityCurveDataV1::Unavailable { reason_code } => {
                assert!(infeasible);
                assert_eq!(reason_code, EquityUnavailableReason::NoSimulation);
            }
        }
        let after: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.evaluations),(SELECT count(*) FROM app.artifacts)")
            .fetch_one(pool).await.unwrap();
        assert_eq!(
            before, after,
            "viewing equity creates no work or account state"
        );
    }
    let _ = actor;
}
