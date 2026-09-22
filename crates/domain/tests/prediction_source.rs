//! Source and target contracts only. Native simulation owns trading and payout.
use contracts::{settlement::*, DbCounter};
use domain::prediction;
use serde_json::{json, Value};

fn group() -> NativeSettlementGroupV1 {
    NativeSettlementGroupV1 {
        condition_id: "condition".into(),
        source_reference: "SYNTHETIC_SOURCE_TEST".into(),
        outcomes: [
            ("condition-1.POLYMARKET", "1"),
            ("condition-2.POLYMARKET", "0"),
        ]
        .into_iter()
        .map(|(id, value)| NativeSettlementOutcomeV1 {
            instrument_id: id.into(),
            close_price: value.parse().unwrap(),
            ts_event: DbCounter::new(100).unwrap(),
            ts_init: DbCounter::new(120).unwrap(),
        })
        .collect(),
    }
}

#[test]
fn complete_payout_vectors_preserve_collateral_and_identity() {
    for prices in [["1", "0"], ["0", "1"], ["0.5", "0.5"], ["0.8", "0.2"]] {
        let mut source = group();
        for (outcome, value) in source.outcomes.iter_mut().zip(prices) {
            outcome.close_price = value.parse().unwrap();
        }
        prediction::settlements(&[source]).unwrap();
    }
    for mutation in 0..8 {
        let mut source = group();
        match mutation {
            0 => source.outcomes[1].close_price = "1".parse().unwrap(),
            1 => {
                for o in &mut source.outcomes {
                    o.close_price = "0.8".parse().unwrap();
                }
            }
            2 => {
                source.outcomes.pop();
            }
            3 => source.outcomes[1].instrument_id = source.outcomes[0].instrument_id.clone(),
            4 => source.outcomes[1].instrument_id = "foreign-2.POLYMARKET".into(),
            5 => source.outcomes[0].ts_init = DbCounter::new(99).unwrap(),
            6 => source.source_reference.clear(),
            _ => source.outcomes[0].close_price = "-0.1".parse().unwrap(),
        }
        assert!(
            prediction::settlements(&[source]).is_err(),
            "mutation {mutation}"
        );
    }
}

fn definition(token: &str, expiry: u64) -> Value {
    json!({"BinaryOption": {"id":format!("condition-{token}.POLYMARKET"), "raw_symbol":token,
        "currency":"pUSD", "activation_ns":10, "expiration_ns":expiry,
        "info":{"condition_id":"condition", "token_id":token}}})
}

#[test]
fn earliest_contract_expiry_bounds_all_target_publication_paths() {
    let definitions = vec![definition("1", 100), definition("2", 150)];
    let ids = vec![
        "condition-1.POLYMARKET".into(),
        "condition-2.POLYMARKET".into(),
    ];
    prediction::target_window(&definitions, &ids, 10, 100).unwrap();
    prediction::target_window(&definitions, &ids, 99, 100).unwrap();
    for (asof, until) in [(9, 100), (90, 101), (100, 110), (101, 120), (90, 90)] {
        assert!(prediction::target_window(&definitions, &ids, asof, until).is_err());
    }
    assert!(
        prediction::target_window(&definitions, &["foreign.POLYMARKET".into()], 10, 90).is_err()
    );
}

#[test]
fn a_single_traded_token_keeps_its_sibling_and_future_evidence_remains_unavailable() {
    let groups = vec![group()];
    let ids = vec![groups[0].outcomes[0].instrument_id.clone()];
    let selected = prediction::scoped_settlements(&groups, &ids);
    assert_eq!(selected[0].outcomes.len(), 2);
    assert!(
        prediction::visible_settlements(&groups, &ids, DbCounter::new(119).unwrap()).is_empty()
    );
    assert_eq!(
        prediction::visible_settlements(&groups, &ids, DbCounter::new(120).unwrap()),
        groups
    );
}
