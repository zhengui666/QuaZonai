//! Synthetic native planning regression, not a strategy or historical source attestation.
#[path = "support/market.rs"]
mod market;

use contracts::{portfolio::NativeModelRefV1, science::NativePortfolioBuildResultV1};
use nautilus_model::instruments::InstrumentAny;
use nautilus_persistence::backend::catalog::ParquetDataCatalog;

#[test]
fn planning_uses_the_tick_known_at_decision_even_after_the_last_observed_bar() {
    let (root, mut request, model) = market::portfolio();
    let NativeModelRefV1::NautilusDefaultFill { parameters, .. } =
        &mut request.execution_settings.fill_model
    else {
        unreachable!()
    };
    parameters.prob_slippage = "0.5".parse().unwrap();
    let build = || -> NativePortfolioBuildResultV1 {
        job::portfolio::build(root.path(), &request, |id| {
            if id == request.mandate.constraints.transaction_costs_ref {
                Ok(serde_json::to_vec(&request.execution_settings)?)
            } else if id == request.current_weights_artifact_id {
                Ok(serde_json::to_vec(&request.current_weights)?)
            } else {
                Ok(model.clone())
            }
        })
        .unwrap()
    };
    let before = build();
    let catalog =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    let originals = catalog.instruments(None, None, None).unwrap();
    let cutoff = request.selection.decision_cutoff_ns.get();
    let mut updates: Vec<InstrumentAny> = Vec::new();
    for original in originals {
        for (at, tick) in [(cutoff - 1, "0.00010"), (cutoff + 1, "0.00100")] {
            let mut definition = serde_json::to_value(&original).unwrap();
            definition["CurrencyPair"]["ts_event"] = at.into();
            definition["CurrencyPair"]["ts_init"] = at.into();
            definition["CurrencyPair"]["price_increment"] = tick.into();
            updates.push(serde_json::from_value(definition).unwrap());
        }
    }
    catalog.write_instruments(updates).unwrap();
    let after = build();
    for ((old, new), asset) in before
        .slippage_references
        .iter()
        .zip(&after.slippage_references)
        .zip(&after.input.assets)
    {
        assert_eq!(new.close_price, old.close_price);
        assert_eq!(new.event_ns, old.event_ns);
        assert!(old.available_ns.get() < cutoff - 1);
        assert_eq!(new.available_ns.get(), cutoff - 1);
        assert_eq!(new.price_increment, "0.00010".parse().unwrap());
        assert!(asset.transaction_cost_rate.is_positive());
    }
}
