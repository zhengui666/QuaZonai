//! Structural transport-response binding only, not a server receipt or native engine proof.
#[path = "../../../tests/support/paper_execution_consume.rs"]
mod fixture;
use contracts::{
    delivery::{HandoffStateV1, PackageOriginV1},
    forward::ForwardEnvironmentV1,
    science::PortfolioWeightsSourceV1,
    strategy_portfolio::{HandoffClaimViewV2, TargetPackageEnvelopeV2},
    Id,
};
use domain::delivery::{
    paper_initial_execution_claim, paper_initial_execution_request,
    paper_initial_execution_response,
};

fn package(claim: &mut HandoffClaimViewV2) -> &mut contracts::delivery::ForecastTargetPackageV2 {
    let TargetPackageEnvelopeV2::Forecast(package) = &mut claim.package else {
        unreachable!()
    };
    package
}

#[test]
fn response_must_bind_current_process_and_the_entire_original_claim() {
    let (request, claim, result, now) = fixture::example();
    paper_initial_execution_request(&request).unwrap();
    paper_initial_execution_claim(&request, claim.handoff.id, &claim, now).unwrap();
    paper_initial_execution_response(&request, &claim, request.owner_instance_id, &result, now)
        .unwrap();
    let mut replay = result.clone();
    replay.replayed = true;
    assert!(paper_initial_execution_response(
        &request,
        &claim,
        request.owner_instance_id,
        &replay,
        now
    )
    .is_err());
    assert!(paper_initial_execution_response(&request, &claim, Id::new(), &result, now).is_err());
    let mut edited = claim.clone();
    package(&mut edited).targets[0].target_weight = "0.0001".parse().unwrap();
    assert!(paper_initial_execution_response(
        &request,
        &edited,
        request.owner_instance_id,
        &result,
        now
    )
    .is_err());
    let mut edited = claim.clone();
    package(&mut edited).execution_settings.fee_rates[0].taker = "0.01".parse().unwrap();
    assert!(paper_initial_execution_response(
        &request,
        &edited,
        request.owner_instance_id,
        &result,
        now
    )
    .is_err());
}

#[test]
fn root_scope_target_claim_and_owner_mismatches_cannot_be_silently_rebound() {
    let (original, claim, result, now) = fixture::example();
    for case in 0..7 {
        let mut request = original.clone();
        match case {
            0 => request.paper_initialization.artifact_id = Id::new(),
            1 => request.paper_initialization.downstream_id = Id::new(),
            2 => request.paper_initialization.trader_id = "OTHER-001".into(),
            3 => request.paper_initialization.account_id = "OTHER-001".into(),
            4 => request.release_id = Id::new(),
            5 => request.external_claim_id = "other-claim".into(),
            _ => request.owner_instance_id = Id::new(),
        }
        assert!(
            paper_initial_execution_response(
                &request,
                &claim,
                original.owner_instance_id,
                &result,
                now
            )
            .is_err(),
            "case {case}"
        );
    }
    assert!(paper_initial_execution_claim(&original, Id::new(), &claim, now).is_err());
    let mut response = result.clone();
    response.resource.paper_initialization.artifact_id = Id::new();
    assert!(paper_initial_execution_response(
        &original,
        &claim,
        original.owner_instance_id,
        &response,
        now
    )
    .is_err());
    let mut response = result;
    response.resource.owner_instance_id = Id::new();
    assert!(paper_initial_execution_response(
        &original,
        &claim,
        original.owner_instance_id,
        &response,
        now
    )
    .is_err());
}

#[test]
fn consumed_at_is_real_receipt_time_and_never_extends_the_original_ttl() {
    let (request, claim, result, now) = fixture::example();
    let mut future = result.clone();
    future.resource.consumed_at = now + chrono::Duration::nanoseconds(1);
    assert!(paper_initial_execution_response(
        &request,
        &claim,
        request.owner_instance_id,
        &future,
        now
    )
    .is_err());
    let mut past = result.clone();
    past.resource.consumed_at =
        claim.handoff.claimed_at.unwrap() - chrono::Duration::nanoseconds(1);
    assert!(paper_initial_execution_response(
        &request,
        &claim,
        request.owner_instance_id,
        &past,
        now
    )
    .is_err());
    assert!(paper_initial_execution_response(
        &request,
        &claim,
        request.owner_instance_id,
        &result,
        claim.handoff.expires_at
    )
    .is_err());
    let mut shortened = claim.clone();
    package(&mut shortened).valid_until = now;
    assert!(paper_initial_execution_claim(&request, claim.handoff.id, &shortened, now).is_err());
}

#[test]
fn live_real_demo_or_noninitial_holdings_never_become_new_paper_funding() {
    let (request, original, _, now) = fixture::example();
    for case in 0..12 {
        let mut changed = original.clone();
        match case {
            0 => changed.handoff.environment = ForwardEnvironmentV1::Live,
            1 => changed.handoff.state = HandoffStateV1::Offered,
            2 => package(&mut changed).environment_origin = PackageOriginV1::Real,
            3 => package(&mut changed).environment_origin = PackageOriginV1::Demo,
            4 => package(&mut changed).source.build_environment = ForwardEnvironmentV1::Live,
            5 => package(&mut changed).source.current_weights_artifact_id = Id::new(),
            6 => package(&mut changed).current_weights.paper_initialization = None,
            7 => {
                package(&mut changed).current_weights.source =
                    PortfolioWeightsSourceV1::LastTarget {
                        candidate_id: Id::new(),
                    }
            }
            8 => package(&mut changed).current_weights.weights[0].weight = "0.01".parse().unwrap(),
            9 => package(&mut changed).current_weights.cash_weight = "0.99".parse().unwrap(),
            10 => package(&mut changed).execution_settings.starting_capital = "1".parse().unwrap(),
            _ => {
                package(&mut changed).current_weights_source =
                    contracts::portfolio::CandidateWeightsSourceV1::LastTarget
            }
        }
        assert!(
            paper_initial_execution_claim(&request, original.handoff.id, &changed, now).is_err(),
            "case {case}"
        );
    }
}

#[test]
fn request_rejects_empty_padded_or_control_bearing_claim_and_scope_aliases() {
    let (request, _, _, _) = fixture::example();
    for bad in ["", " original", "original ", "bad\nclaim"] {
        let mut changed = request.clone();
        changed.external_claim_id = bad.into();
        assert!(paper_initial_execution_request(&changed).is_err());
    }
    let mut oversized = request.clone();
    oversized.external_claim_id = "x".repeat(201);
    assert!(paper_initial_execution_request(&oversized).is_err());
    let mut scope = request;
    scope.paper_initialization.trader_id = "TRADER-001 ".into();
    assert!(paper_initial_execution_request(&scope).is_err());
}
