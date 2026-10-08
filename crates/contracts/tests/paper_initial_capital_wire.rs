//! Structural virtual-capital wire tests; no account, funding or market evidence.
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio_fixture;
use contracts::{
    delivery::PackageOriginV1,
    forward::DownstreamWeightsSubmitV1,
    portfolio::{AllocationInputV1, PortfolioBuildWeightsV1},
    science::{PaperInitializationRefV1, PortfolioCurrentWeightsV1, PortfolioWeightsSourceV1},
    strategy_portfolio::FreshPaperCashV1,
    Id,
};
use serde_json::{json, Value};

fn historical_weights() -> PortfolioCurrentWeightsV1 {
    let input: AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    portfolio_fixture::request(&input).current_weights
}

fn root() -> PaperInitializationRefV1 {
    PaperInitializationRefV1 {
        artifact_id: Id::new(),
        downstream_id: Id::new(),
        trader_id: "TRADER-001".into(),
        account_id: "SIM-001".into(),
    }
}

#[test]
fn explicit_initial_capital_intent_accepts_scope_but_not_amounts_or_reset_ids() {
    let wire = json!({"kind":"PAPER_INITIAL_CAPITAL","downstream_id":Id::new(),
        "trader_id":"TRADER-001","account_id":"SIM-001"});
    let parsed: PortfolioBuildWeightsV1 = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), wire);
    for field in [
        "starting_capital",
        "base_currency",
        "initialization_id",
        "session_id",
        "mandate_id",
        "force_reset",
    ] {
        let mut changed = wire.clone();
        changed[field] = json!("caller-supplied");
        assert!(
            serde_json::from_value::<PortfolioBuildWeightsV1>(changed).is_err(),
            "{field}"
        );
    }
    for field in ["downstream_id", "trader_id", "account_id"] {
        let mut missing = wire.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<PortfolioBuildWeightsV1>(missing).is_err(),
            "{field}"
        );
    }
}

#[test]
fn old_weights_keep_original_bytes_shape_and_no_inferred_initialization() {
    let old = serde_json::to_value(historical_weights()).unwrap();
    assert!(old.get("paper_initialization").is_none());
    let restored: PortfolioCurrentWeightsV1 = serde_json::from_value(old.clone()).unwrap();
    assert!(restored.paper_initialization.is_none());
    assert_eq!(serde_json::to_value(restored).unwrap(), old);
}

#[test]
fn declared_model_condition_and_root_are_explicit_and_exact() {
    let mut weights = historical_weights();
    let root = root();
    weights.paper_initialization = Some(root.clone());
    weights.source = PortfolioWeightsSourceV1::PaperInitialCapital {
        account_start: FreshPaperCashV1 {
            downstream_id: root.downstream_id,
            trader_id: root.trader_id.clone(),
            account_id: root.account_id.clone(),
            base_currency: "USD".into(),
            starting_capital: "1000".parse().unwrap(),
            execution_assumptions_id: Id::new(),
        },
    };
    weights.asof_ns = contracts::DbCounter::new(9_007_199_254_740_993).unwrap();
    let wire = serde_json::to_value(&weights).unwrap();
    let restored: PortfolioCurrentWeightsV1 = serde_json::from_value(wire.clone()).unwrap();
    assert_eq!(restored, weights);
    assert_eq!(wire["asof_ns"], json!("9007199254740993"));
    for field in ["paper_initialization", "source"] {
        let mut changed = wire.clone();
        changed[field]["unrecognized"] = json!(true);
        assert!(serde_json::from_value::<PortfolioCurrentWeightsV1>(changed).is_err());
    }
    for field in ["artifact_id", "downstream_id", "trader_id", "account_id"] {
        let mut changed = wire.clone();
        changed["paper_initialization"]
            .as_object_mut()
            .unwrap()
            .remove(field);
        assert!(serde_json::from_value::<PortfolioCurrentWeightsV1>(changed).is_err());
    }
    assert_eq!(
        serde_json::to_value(PackageOriginV1::Synthetic).unwrap(),
        json!("SYNTHETIC")
    );
    assert_ne!(
        serde_json::to_value(PackageOriginV1::Demo).unwrap(),
        json!("SYNTHETIC")
    );
}

#[test]
fn snapshot_lineage_is_only_a_reference_and_historical_omission_is_preserved() {
    let weights = historical_weights();
    let wire = json!({"schema_version":1,"project_id":Id::new(),"environment":"PAPER",
        "external_message_id":"native-observation","asof_ns":weights.asof_ns,
        "available_ns":weights.available_ns,"valid_until_ns":weights.valid_until_ns,
        "base_currency":weights.base_currency,"cash_weight":weights.cash_weight,"weights":weights.weights});
    let old: DownstreamWeightsSubmitV1 = serde_json::from_value(wire.clone()).unwrap();
    assert!(old.paper_initialization.is_none());
    assert_eq!(serde_json::to_value(old).unwrap(), wire);
    let mut linked = wire;
    linked["paper_initialization"] = serde_json::to_value(root()).unwrap();
    let parsed: DownstreamWeightsSubmitV1 = serde_json::from_value(linked.clone()).unwrap();
    assert_eq!(serde_json::to_value(parsed).unwrap(), linked);
    linked["paper_initialization"]["starting_capital"] = Value::String("invented".into());
    assert!(serde_json::from_value::<DownstreamWeightsSubmitV1>(linked).is_err());
}
