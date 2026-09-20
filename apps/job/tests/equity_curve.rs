//! Real native execution; projection assertions never substitute a mock engine.
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;
use contracts::{
    equity_curve::{EquityCurveQuery, EquityResolution},
    science::{NativeSimulationRequestV1, NativeSimulationResultV1},
    DecimalValue,
};
use domain::execution::portfolio_equity_curve;
use std::{collections::BTreeMap, path::Path};

fn simulate(root: &Path, request: &NativeSimulationRequestV1) -> NativeSimulationResultV1 {
    let output = native::command(
        &["simulate".as_ref(), "--catalog".as_ref(), root.as_os_str()],
        request,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn native_fees_positions_and_intraday_values_reach_the_exact_display_projection() {
    for fee in ["0", "0.001"] {
        let (directory, request) = market::market(fee, 20);
        let result = simulate(directory.path(), &request);
        assert!(result.orders.get() >= 4);
        // Intraday curves do not require a usable daily-return statistic.
        assert!(result.returns.is_empty());
        let series =
            portfolio_equity_curve(&request, &result, &EquityCurveQuery::default()).unwrap();
        assert_eq!(series.resolution, EquityResolution::Native);
        assert!(!series.sampled);
        assert!(series.points.len() > 2);
        assert_eq!(series.base_currency, request.settings.base_currency);
        assert_eq!(series.starting_capital, request.settings.starting_capital);
        let mut original = BTreeMap::new();
        for row in result.canonical_result["portfolio_snapshots"]
            .as_array()
            .unwrap()
        {
            let time = row["ts_event"].as_str().unwrap().parse::<u64>().unwrap();
            let amount: DecimalValue = row["total_equity"][0]
                .as_str()
                .unwrap()
                .strip_suffix(&format!(" {}", series.base_currency))
                .unwrap()
                .parse()
                .unwrap();
            if let Some(previous) = original.insert(time, amount.clone()) {
                assert_eq!(previous, amount);
            }
        }
        assert_eq!(series.points.len(), original.len());
        for point in &series.points {
            assert_eq!(
                point.value.as_ref(),
                original.get(&point.timestamp_ns.get())
            );
            assert!(point.reason_code.is_none());
        }
        assert!(series
            .points
            .windows(2)
            .any(|pair| pair[0].value != pair[1].value));
        if fee != "0" {
            assert!(series
                .points
                .iter()
                .any(|point| point.value.as_ref().unwrap().as_decimal()
                    < series.starting_capital.as_decimal()));
        }
        let middle = &series.points[series.points.len() / 2];
        let selected = portfolio_equity_curve(
            &request,
            &result,
            &EquityCurveQuery {
                start_ns: Some(middle.timestamp_ns),
                end_ns: Some(middle.timestamp_ns),
                resolution: EquityResolution::Native,
            },
        )
        .unwrap();
        assert_eq!(selected.points, vec![middle.clone()]);
        assert_eq!(selected.starting_capital, series.starting_capital);
    }
}

#[test]
fn exact_snapshot_projection_retains_zero_negative_precision_and_rejects_conflicts() {
    let (directory, request) = market::market("0", 20);
    let result = simulate(directory.path(), &request);
    // Controlled mutations test transport limits, not native performance claims.
    for value in [
        "0",
        "-0.000000000000000001",
        "99999999999999999999.999999999999999999",
    ] {
        let mut sample = result.clone();
        let mut row = sample.canonical_result["portfolio_snapshots"][0].clone();
        row["total_equity"][0] =
            serde_json::json!(format!("{value} {}", request.settings.base_currency));
        sample.canonical_result["portfolio_snapshots"] = serde_json::json!([row.clone(), row]);
        let projected =
            portfolio_equity_curve(&request, &sample, &EquityCurveQuery::default()).unwrap();
        assert_eq!(projected.points.len(), 1);
        assert_eq!(projected.points[0].value, Some(value.parse().unwrap()));
        sample.canonical_result["portfolio_snapshots"][1]["total_equity"][0] =
            serde_json::json!(format!("42 {}", request.settings.base_currency));
        assert!(portfolio_equity_curve(&request, &sample, &EquityCurveQuery::default()).is_err());
    }
    for (field, value) in [
        ("account_id", serde_json::json!("OTHER")),
        ("total_equity", serde_json::json!(["10 EUR"])),
        ("total_equity", serde_json::json!(["NaN USD"])),
    ] {
        let mut sample = result.clone();
        sample.canonical_result["portfolio_snapshots"][0][field] = value;
        assert!(portfolio_equity_curve(&request, &sample, &EquityCurveQuery::default()).is_err());
    }
    let mut shuffled = result.clone();
    shuffled.canonical_result["portfolio_snapshots"]
        .as_array_mut()
        .unwrap()
        .reverse();
    assert_eq!(
        portfolio_equity_curve(&request, &shuffled, &EquityCurveQuery::default())
            .unwrap()
            .points,
        portfolio_equity_curve(&request, &result, &EquityCurveQuery::default())
            .unwrap()
            .points,
    );
}
