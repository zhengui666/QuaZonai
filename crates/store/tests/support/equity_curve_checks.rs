//! Original admissions and publication with controlled protocol observations.
#![allow(dead_code)]
use super::*;

pub(crate) async fn publish(
    context: (
        &PgPool,
        &Store,
        &store::authority::Actor,
        &cycle_support::Fixture,
    ),
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    candidate: Id,
    infeasible: bool,
) -> Id {
    let (pool, store, actor, f) = context;
    let run = Box::pin(store.start_portfolio_study(
        actor,
        &format!("equity-read-study-{infeasible}"),
        &contracts::portfolio::PortfolioStudyRequestV1 {
            schema_version: SchemaV1,
            candidate_id: candidate,
            cycle_id: build.cycle_id,
            runtime_id: build.runtime_id,
            expected_runtime_revision: build.expected_runtime_revision,
            limits: build.limits.clone(),
        },
        |id, size| f.read(id, size),
        |object| {
            std::future::ready(
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity),
            )
        },
    ))
    .await
    .unwrap()
    .resource;
    let message = validation_publication::message(pool, run.id).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "equity-read", 60)
        .await
        .unwrap()
    else {
        panic!("original Study admission");
    };
    let job = store.native_job(run.id, &lease.fence).await.unwrap();
    Box::pin(study_result::complete(
        pool, store, f, &lease, &job, infeasible, false,
    ))
    .await;
    let evaluation = validation_publication::publish(store, f, run.id)
        .await
        .unwrap()
        .resource;
    store.acknowledge_run(&message).await.unwrap();
    evaluation
}

pub(crate) async fn check_projection(
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    evaluation: Id,
    candidate: Id,
    infeasible: bool,
) {
    use contracts::equity_curve::*;
    let view = store
        .evaluation_equity_curve(actor, evaluation, &Default::default(), |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    assert_eq!(view.evaluation_id, evaluation);
    assert_eq!(view.candidate_id, candidate);
    assert_eq!(view.project_id, f.data.project);
    match view.curve {
        EquityCurveDataV1::Ready {
            source_artifact_id,
            series,
        } => {
            assert!(!infeasible);
            assert!(!series.points.is_empty());
            assert!(series.points.len() <= MAX_EQUITY_POINTS);
            assert!(series.points.iter().all(|point| point.value.is_some()));
            // Metadata of the native report remains restricted; only its projection
            // is readable. A normal artifact download cannot replace this endpoint.
            assert!(store
                .artifact_content(actor, source_artifact_id)
                .await
                .is_err());
        }
        EquityCurveDataV1::Unavailable { .. } => assert!(infeasible),
    }
    assert!(store
        .evaluation_equity_curve(
            actor,
            evaluation,
            &EquityCurveQuery {
                start_ns: Some(DbCounter::new(2).unwrap()),
                end_ns: Some(DbCounter::new(1).unwrap()),
                resolution: EquityResolution::Auto,
            },
            |_, _| async { panic!("invalid range cannot read artifacts") }
        )
        .await
        .is_err());
}
