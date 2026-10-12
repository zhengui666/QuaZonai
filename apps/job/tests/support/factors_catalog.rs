//! Synthetic mixed-asset Parquet data, built from the existing native catalog helpers.
use crate::market::{count, equity_market, forecast_request, market, INTERVAL_NS};
use contracts::{factors::NativeFactorEvaluationRequestV1, SchemaV1};
use job::catalog::load_catalog;
use nautilus_model::{
    data::Bar,
    types::{Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;

/// Dense CurrencyPair plus sparse Equity; no simulation is run or target returned.
/// late_labels preserves original event clocks while delaying original availability.
pub fn mixed_catalog(
    future_price_shift: f64,
    late_labels: bool,
) -> (tempfile::TempDir, NativeFactorEvaluationRequestV1) {
    let (fx_root, fx_request) = market("0", 36);
    let (eq_root, eq_request) = equity_market("0", 36);
    let fx = load_catalog(fx_root.path(), &fx_request.selection)
        .unwrap()
        .series
        .remove(0);
    let equity = load_catalog(eq_root.path(), &eq_request.selection)
        .unwrap()
        .series
        .remove(0);
    let root = tempfile::tempdir().unwrap();
    let catalog =
        ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    let mut forecast = forecast_request(&fx_request);
    forecast.selection.bar_types.clear();
    forecast.selection.maximum_rows = 54;
    for (asset, series) in [fx, equity].into_iter().enumerate() {
        catalog.write_instruments(vec![series.instrument]).unwrap();
        forecast
            .selection
            .bar_types
            .push(series.bar_type.to_string());
        let bars = series
            .bars
            .iter()
            .take(if asset == 0 { 36 } else { 18 })
            .enumerate()
            .map(|(index, original)| {
                let minute = if asset == 0 { index + 1 } else { index * 2 + 1 } as u64;
                let event = minute * INTERVAL_NS;
                let mut available = event + 1;
                if late_labels && asset == 0 {
                    for delayed in [9, 21] {
                        if (delayed..=delayed + 2).contains(&minute) {
                            available = (delayed + 2) * INTERVAL_NS + 1;
                        }
                    }
                }
                let price = original.close.as_f64()
                    + if minute >= 26 {
                        future_price_shift
                    } else {
                        0.0
                    };
                let volume = 10 + ((index * index + asset * 3) % 13) * 10;
                Bar::new_checked(
                    series.bar_type,
                    Price::from(format!("{price:.5}").as_str()),
                    Price::from(format!("{:.5}", price + 0.0002).as_str()),
                    Price::from(format!("{:.5}", price - 0.0002).as_str()),
                    Price::from(format!("{price:.5}").as_str()),
                    Quantity::from(volume.to_string().as_str()),
                    event.into(),
                    available.into(),
                )
                .unwrap()
            })
            .collect::<Vec<_>>();
        catalog.write_to_parquet(&bars, None, None, None).unwrap();
    }
    (
        root,
        NativeFactorEvaluationRequestV1 {
            schema_version: SchemaV1,
            forecast,
            train_end_ns: count(10 * INTERVAL_NS),
            validation_end_ns: count(22 * INTERVAL_NS),
        },
    )
}
