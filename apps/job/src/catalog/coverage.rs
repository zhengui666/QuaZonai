//! Read-only projection of Nautilus catalog file coverage, not a data quality verdict.
use super::open_catalog;
use anyhow::{ensure, Result};
use contracts::{catalog_coverage::*, DbCounter, SchemaV1};
use nautilus_model::data::{BarType, NautilusDataType};
use std::path::Path;

fn intervals(values: Vec<(u64, u64)>) -> Result<Vec<NativeCatalogIntervalV1>> {
    values
        .into_iter()
        .map(|(start, end)| {
            Ok(NativeCatalogIntervalV1 {
                start_ns: DbCounter::new(start).map_err(anyhow::Error::msg)?,
                end_ns: DbCounter::new(end).map_err(anyhow::Error::msg)?,
            })
        })
        .collect()
}

pub fn query(
    root: &Path,
    request: NativeCatalogCoverageRequestV1,
) -> Result<NativeCatalogCoverageResultV1> {
    ensure!(
        request.start_ns <= request.end_ns,
        "CATALOG_COVERAGE_RANGE_INVALID"
    );
    ensure!(
        !request.bar_types.is_empty() || !request.selectors.is_empty(),
        "CATALOG_COVERAGE_SELECTORS_REQUIRED"
    );
    let mut selectors = request
        .bar_types
        .iter()
        .map(|value| {
            Ok(NativeCatalogSelectorV1 {
                data_type: "Bar".into(),
                identifier: value.parse::<BarType>()?.to_string(),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    selectors.extend(request.selectors.clone());
    // Use the official type parser rather than a QuaZonai market-data allowlist.
    let selectors = selectors
        .into_iter()
        .map(|selector| {
            ensure!(
                !selector.identifier.is_empty(),
                "CATALOG_COVERAGE_IDENTIFIER_REQUIRED"
            );
            let kind = selector.data_type.parse::<NautilusDataType>()?;
            Ok((
                kind.clone(),
                NativeCatalogSelectorV1 {
                    data_type: kind.to_string(),
                    ..selector
                },
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    let catalog = open_catalog(root)?;
    let series = selectors
        .into_iter()
        .map(|(kind, selector)| {
            let bar_type = if kind == NautilusDataType::Bar {
                selector
                    .identifier
                    .parse::<BarType>()
                    .ok()
                    .map(|value| value.to_string())
            } else {
                None
            };
            let data_type = kind.into();
            Ok(NativeCatalogSeriesCoverageV1 {
                file_intervals: intervals(
                    catalog.get_intervals(&data_type, Some(&selector.identifier))?,
                )?,
                missing_intervals: intervals(catalog.get_missing_intervals_for_request(
                    request.start_ns.get(),
                    request.end_ns.get(),
                    &data_type,
                    Some(&selector.identifier),
                )?)?,
                bar_type,
                selector,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(NativeCatalogCoverageResultV1 {
        schema_version: SchemaV1,
        request,
        timestamp_basis: "ts_init".into(),
        interval_bounds: "inclusive".into(),
        coverage_basis: "native_file_intervals_not_event_completeness".into(),
        series,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_model::{
        data::Bar,
        types::{Price, Quantity},
    };
    use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;

    #[test]
    fn native_file_coverage_keeps_receive_time_exact_type_and_inclusive_bounds() {
        let root = tempfile::tempdir().unwrap();
        let mut catalog =
            ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, None, None, None)
                .unwrap();
        let kind = "BTC/USDT.BINANCE-1-MINUTE-LAST-EXTERNAL";
        let p = Price::from("1.0");
        // Missing events inside a file are deliberately NOT classified as gaps.
        let bars = [10_u64, 20].map(|time| {
            Bar::new(
                kind.parse().unwrap(),
                p,
                p,
                p,
                p,
                Quantity::from("1.0"),
                (time - 5).into(),
                time.into(),
            )
        });
        catalog.write_to_parquet(&bars, None, None, None).unwrap();
        let request = NativeCatalogCoverageRequestV1 {
            bar_types: vec![
                kind.into(),
                "BTC/USDT.BINANCE-5-MINUTE-LAST-EXTERNAL".into(),
            ],
            selectors: vec![],
            start_ns: DbCounter::new(9).unwrap(),
            end_ns: DbCounter::new(21).unwrap(),
        };
        let result = query(root.path(), request).unwrap();
        let bounds = |values: &[NativeCatalogIntervalV1]| {
            values
                .iter()
                .map(|v| (v.start_ns.get(), v.end_ns.get()))
                .collect::<Vec<_>>()
        };
        assert_eq!(bounds(&result.series[0].file_intervals), vec![(10, 20)]);
        assert_eq!(
            bounds(&result.series[0].missing_intervals),
            vec![(9, 9), (21, 21)]
        );
        assert_eq!(bounds(&result.series[1].missing_intervals), vec![(9, 21)]);
        assert_eq!(result.timestamp_basis, "ts_init");
        assert_eq!(
            serde_json::to_value(result).unwrap()["request"]["start_ns"],
            "9"
        );
    }

    #[test]
    fn reversed_bounds_fail_before_catalog_access() {
        let result = query(
            Path::new("/nonexistent"),
            NativeCatalogCoverageRequestV1 {
                bar_types: vec![],
                selectors: vec![],
                start_ns: DbCounter::new(2).unwrap(),
                end_ns: DbCounter::new(1).unwrap(),
            },
        );
        assert_eq!(
            result.unwrap_err().to_string(),
            "CATALOG_COVERAGE_RANGE_INVALID"
        );
    }
    #[test]
    fn official_trade_and_quote_selectors_keep_distinct_exact_nanosecond_coverage() {
        use nautilus_model::{
            data::{QuoteTick, TradeTick},
            enums::AggressorSide,
            identifiers::{InstrumentId, TradeId},
        };
        let root = tempfile::tempdir().unwrap();
        let mut catalog =
            ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, None, None, None)
                .unwrap();
        let instrument = InstrumentId::from("BTC/USDT.BINANCE");
        let time = 1_790_000_000_000_000_001_u64;
        let price = Price::from("100.0");
        let size = Quantity::from("1.0");
        let trade = TradeTick::new(
            instrument,
            price,
            size,
            AggressorSide::Buy,
            TradeId::from("one"),
            (time - 1).into(),
            time.into(),
        );
        let quote = QuoteTick::new(
            instrument,
            price,
            price,
            size,
            size,
            time.into(),
            (time + 2).into(),
        );
        catalog
            .write_to_parquet(&[trade], None, None, None)
            .unwrap();
        catalog
            .write_to_parquet(&[quote], None, None, None)
            .unwrap();
        let result = query(root.path(), serde_json::from_value(serde_json::json!({
            "selectors":[{"data_type":"trades","identifier":instrument.to_string()}, {"data_type":"QuoteTick","identifier":instrument.to_string()}],
            "start_ns":time.to_string(), "end_ns":(time+2).to_string()
        })).unwrap()).unwrap();
        assert_eq!(result.series[0].selector.data_type, "TradeTick");
        assert_eq!(result.series[0].file_intervals[0].start_ns.get(), time);
        assert_eq!(
            result.series[0].missing_intervals[0].start_ns.get(),
            time + 1
        );
        assert_eq!(result.series[1].file_intervals[0].start_ns.get(), time + 2);
        assert_eq!(result.series[1].missing_intervals[0].end_ns.get(), time + 1);
        assert!(result.series.iter().all(|row| row.bar_type.is_none()));
        assert_eq!(
            serde_json::to_value(result).unwrap()["series"][0]["file_intervals"][0]["start_ns"],
            time.to_string()
        );
    }

    #[test]
    fn unsupported_native_type_is_not_reported_as_an_empty_catalog() {
        let request = serde_json::from_value(serde_json::json!({
            "selectors":[{"data_type":"NOT_A_NATIVE_TYPE","identifier":"BTC/USDT.BINANCE"}],
            "start_ns":"1", "end_ns":"2"
        }))
        .unwrap();
        assert!(query(Path::new("/nonexistent"), request)
            .unwrap_err()
            .to_string()
            .contains("Invalid `NautilusDataType`"));
    }
}
