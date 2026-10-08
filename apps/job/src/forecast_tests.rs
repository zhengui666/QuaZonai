//! Native synthetic bars and Wasmi execution, not market-performance evidence.
use super::*;
use nautilus_model::{
    data::{Bar, BarType},
    instruments::{
        stubs::{audusd_sim, gbpusd_sim},
        InstrumentAny,
    },
    types::{Price, Quantity},
};

fn fixture(rows: usize) -> (NativeMarketData, NativeForecastRequestV1) {
    let series = [audusd_sim(), gbpusd_sim()]
        .into_iter()
        .map(|instrument| {
            let bar_type: BarType = format!("{}-1-MINUTE-LAST-EXTERNAL", instrument.id())
                .parse()
                .unwrap();
            let bars = (1..=rows)
                .map(|i| {
                    let price = Price::new(1.0 + i as f64 * 0.001, 5);
                    Bar::new(
                        bar_type,
                        price,
                        price,
                        price,
                        price,
                        Quantity::from("100"),
                        (i as u64 * 60_000_000_000).into(),
                        (i as u64 * 60_000_000_000 + 1).into(),
                    )
                })
                .collect();
            NativeBarSeries {
                instrument: InstrumentAny::CurrencyPair(instrument),
                instrument_updates: Vec::new(),
                bar_type,
                bars,
            }
        })
        .collect::<Vec<_>>();
    let end = counter((rows as u64 + 1) * 60_000_000_000).unwrap();
    let request = NativeForecastRequestV1 {
        schema_version: SchemaV1,
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: series.iter().map(|s| s.bar_type.to_string()).collect(),
            event_start_ns: DbCounter::ZERO,
            event_end_ns: end,
            decision_cutoff_ns: end,
            maximum_rows: (rows * 2) as u32,
        },
        parameters: NativeForecastParametersV1 {
            schema_version: SchemaV1,
            fast_period: 2,
            slow_period: 3,
            label_horizon_observations: 2,
            total_fuel: Some(counter(1_000_000).unwrap()),
        },
    };
    (
        NativeMarketData {
            series,
            rows: rows * 2,
        },
        request,
    )
}

fn module(body: &str) -> Vec<u8> {
    wat::parse_str(format!(
        r#"(module
        (global $n (mut i32) (i32.const 0)) (memory 1)
        (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64)
            global.get $n i32.const 1 i32.add global.set $n
            i32.const 0 global.get $n i32.store
            {body}))"#
    ))
    .unwrap()
}

fn same_failure(market: &NativeMarketData, request: &NativeForecastRequestV1, wasm: &[u8]) {
    let full = forecast_market(market, request, wasm)
        .err()
        .expect("full path must fail");
    let latest = forecast_latest_market(market, request, wasm)
        .err()
        .expect("latest path must fail");
    assert_eq!(full.to_string(), latest.to_string());
}

#[test]
fn latest_retention_preserves_stateful_predictions_fuel_and_asset_isolation() {
    let wasm = module("i32.const 0 i32.load f64.convert_i32_s local.get 0 f64.add");
    for rows in [3, 4, 20, 200] {
        let (market, mut request) = fixture(rows);
        for horizon in [1, 2, 100] {
            request.parameters.label_horizon_observations = horizon;
            let full = forecast_market(&market, &request, &wasm).unwrap();
            let (fuel, latest) = forecast_latest_market(&market, &request, &wasm).unwrap();
            let expected = full
                .points
                .chunk_by(|a, b| a.instrument_id == b.instrument_id)
                .map(|points| points.last().unwrap())
                .collect::<Vec<_>>();
            assert_eq!(
                serde_json::to_value(&latest).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
            assert_eq!(fuel, full.consumed_fuel);
            assert_eq!(latest.len(), market.series.len());
            assert_eq!(latest.capacity(), market.series.len());
            assert_eq!(latest[0].forecast, latest[1].forecast);
            assert_eq!(latest[0].ordinal as usize, rows - 1);
        }
    }
}

#[test]
fn latest_retention_checks_discarded_predictions_and_total_task_fuel() {
    let (market, mut request) = fixture(20);
    for body in [
        "global.get $n i32.const 3 i32.eq if unreachable end f64.const 1",
        "global.get $n i32.const 3 i32.eq if (result f64) f64.const nan else f64.const 1 end",
    ] {
        same_failure(&market, &request, &module(body));
    }
    let wasm = module("global.get $n f64.convert_i32_s");
    let full = forecast_market(&market, &request, &wasm).unwrap();
    request.parameters.total_fuel = Some(counter(full.consumed_fuel.unwrap().get() / 2).unwrap());
    same_failure(&market, &request, &wasm);
}

#[test]
fn latest_retention_checks_warmup_and_discarded_clock_and_label_values() {
    let wasm = module("f64.const 1");
    let (market, request) = fixture(2);
    same_failure(&market, &request, &wasm);
    let (mut market, request) = fixture(20);
    market.series[0].bars[0].ts_event = u64::MAX.into();
    same_failure(&market, &request, &wasm);
    let (mut market, request) = fixture(20);
    market.series[0].bars[4].ts_init = u64::MAX.into();
    same_failure(&market, &request, &wasm);
    let (mut market, request) = fixture(20);
    market.series[0].bars[2].close = Price::from("0.00000");
    same_failure(&market, &request, &wasm);
}
