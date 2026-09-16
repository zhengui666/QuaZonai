//! Actual catalog and Wasmi execution; no generated forecast is substituted by a fixture result.
#[path = "support/market.rs"]
mod market;
use contracts::science::ForecastMissingReason;
use job::forecast::forecast;
use market::{count, forecast_request, market, module, INTERVAL_NS};

#[test]
fn catalog_features_produce_causal_predictions_and_separate_completed_labels() {
    let (directory, simulation) = market("0", 20);
    let request = forecast_request(&simulation);
    let wasm = module("local.get 0 local.get 1 f64.div f64.const 1 f64.sub");
    let result = forecast(directory.path(), &request, &wasm).unwrap();
    assert_eq!(result.points.len(), 40);
    assert_eq!(result.points[0].forecast, None);
    assert_eq!(
        result.points[0].forecast_reason,
        Some(ForecastMissingReason::IndicatorWarmup)
    );
    for index in [0, 1, 20, 21] {
        let warmup = &result.points[index];
        assert_eq!(warmup.forecast, None);
        assert_eq!(warmup.label_return, None);
        assert_eq!(warmup.label_available_ns, None);
        assert_eq!(
            warmup.label_reason,
            Some(ForecastMissingReason::IndicatorWarmup)
        );
    }
    assert!((result.points[2].forecast.unwrap() - (1.003 / 1.002 - 1.0)).abs() < 1e-12);
    assert!((result.points[2].label_return.unwrap() - (1.005 / 1.003 - 1.0)).abs() < 1e-12);
    assert_eq!(
        result.points[2].label_available_ns,
        Some(count(5 * INTERVAL_NS + 1))
    );
    assert_eq!(result.points[19].label_return, None);
    assert_eq!(
        result.points[19].label_reason,
        Some(ForecastMissingReason::LabelNotComplete)
    );
    assert!(result.points[19].forecast.is_some());
    assert!(result.consumed_fuel.get() > 0);
}

#[test]
fn extending_the_future_does_not_change_the_same_past_prediction() {
    let (directory, simulation) = market("0", 20);
    let request = forecast_request(&simulation);
    let wasm = module("local.get 2 local.get 3 f64.sub");
    let full = forecast(directory.path(), &request, &wasm).unwrap();
    let mut earlier = request;
    earlier.selection.event_end_ns = count(11 * INTERVAL_NS);
    earlier.selection.decision_cutoff_ns = earlier.selection.event_end_ns;
    let earlier = forecast(directory.path(), &earlier, &wasm).unwrap();
    for point in earlier.points {
        let same = full
            .points
            .iter()
            .find(|p| p.instrument_id == point.instrument_id && p.ordinal == point.ordinal)
            .unwrap();
        assert_eq!(point.forecast, same.forecast);
        assert_eq!(point.available_ns, same.available_ns);
    }
}

#[test]
fn native_model_memory_is_not_shared_between_assets_and_whole_task_fuel_is_not_reset() {
    let (directory, simulation) = market("0", 20);
    let request = forecast_request(&simulation);
    let wasm = wat::parse_str("(module (global $n (mut f64) (f64.const 0)) (func (export \"predict\") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) global.get $n f64.const 1 f64.add global.set $n global.get $n))").unwrap();
    let full = forecast(directory.path(), &request, &wasm).unwrap();
    assert_eq!(full.points[2].forecast, Some(1.0));
    assert_eq!(full.points[22].forecast, Some(1.0));
    let mut one = request.clone();
    one.selection.bar_types.truncate(1);
    let one = forecast(directory.path(), &one, &wasm).unwrap();
    assert!(full.consumed_fuel > one.consumed_fuel);
    let mut limited = request;
    limited.parameters.total_fuel = one.consumed_fuel;
    assert!(forecast(directory.path(), &limited, &wasm).is_err());
}

#[test]
fn invalid_warmup_parameters_traps_and_nonfinite_predictions_are_not_published() {
    let (directory, simulation) = market("0", 20);
    let mut request = forecast_request(&simulation);
    request.parameters.fast_period = request.parameters.slow_period;
    assert!(forecast(directory.path(), &request, &module("f64.const 0")).is_err());
    request = forecast_request(&simulation);
    request.parameters.slow_period = 100;
    assert!(forecast(directory.path(), &request, &module("f64.const 0")).is_err());
    request = forecast_request(&simulation);
    for code in [
        "f64.const nan",
        "unreachable",
        "(loop $again (br $again)) f64.const 0",
    ] {
        assert!(forecast(directory.path(), &request, &module(code)).is_err());
    }
}
