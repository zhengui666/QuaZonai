#![cfg(feature = "catalog-prepare")]
#[path = "support/market.rs"]
mod market;
#[path = "support/command.rs"]
mod native;
use contracts::{catalog_prefix::NativeCatalogPrefixResultV1, science::NativeBarSelectionV1};
use nautilus_model::{data::Bar, instruments::InstrumentAny, types::Price};
use nautilus_persistence::backend::parquet::catalog::ParquetDataCatalog;
use std::path::Path;

fn rewritten(root: &Path, full: &NativeBarSelectionV1, mutation: usize) -> tempfile::TempDir {
    let mut loaded = job::catalog::load_catalog(root, full).unwrap();
    if mutation == 1 || mutation == 2 {
        let index = if mutation == 1 { 1 } else { 10 };
        let row = loaded.series[0].bars[index];
        let price = Price::from("100.25000");
        loaded.series[0].bars[index] = Bar::new_checked(
            row.bar_type,
            price,
            price,
            price,
            price,
            row.volume,
            row.ts_event,
            row.ts_init,
        )
        .unwrap();
    }
    if mutation == 3 {
        loaded.execution_fees.as_mut().unwrap().rates[0].maker = "0.0001".parse().unwrap();
    }
    if mutation == 5 {
        let InstrumentAny::CurrencyPair(pair) = &mut loaded.series[0].instrument else {
            panic!("fixture currency pair")
        };
        pair.margin_init = rust_decimal::Decimal::new(5, 1);
    }
    if mutation == 4 {
        loaded.series[0].bars[1].ts_init = (loaded.series[0].bars[1].ts_init.as_u64() + 1).into();
    }
    let result = tempfile::tempdir().unwrap();
    let output =
        ParquetDataCatalog::from_uri(result.path().to_str().unwrap(), None, Some(16), None, None)
            .unwrap();
    if let Some(source) = &loaded.execution_fees {
        std::fs::write(
            result
                .path()
                .join(contracts::catalogs::MAKER_TAKER_FEES_FILE),
            serde_json::to_vec(source).unwrap(),
        )
        .unwrap();
    }
    for series in loaded.series {
        output.write_instruments(vec![series.instrument]).unwrap();
        output
            .write_to_parquet(&series.bars, None, None, None)
            .unwrap();
    }
    result
}

#[test]
fn digest_binds_native_values_definitions_and_clocks_but_not_paths_or_future_rows() {
    let (root, simulation) = market::market("0", 12);
    let full = simulation.selection;
    let mut prefix = full.clone();
    prefix.event_end_ns = market::count(6 * market::INTERVAL_NS);
    prefix.decision_cutoff_ns = prefix.event_end_ns;
    let original = job::catalog_prefix::digest(root.path(), &prefix).unwrap();
    assert!(original.row_count.get() > 0);
    assert_eq!(original.series_count, 2);
    assert_eq!(original.content_sha256.len(), 64);
    for mutation in 0..6 {
        let copied = rewritten(root.path(), &full, mutation);
        let candidate = job::catalog_prefix::digest(copied.path(), &prefix).unwrap();
        if mutation == 0 || mutation == 2 {
            assert_eq!(
                serde_json::to_value(&candidate).unwrap(),
                serde_json::to_value(&original).unwrap(),
                "copy/future mutation {mutation}"
            );
        } else {
            assert_ne!(
                candidate.content_sha256, original.content_sha256,
                "past mark/fee/definition/availability mutation {mutation}"
            );
        }
    }
    let output = native::command(
        &[
            "catalog-prefix-digest".as_ref(),
            "--catalog".as_ref(),
            root.path().as_os_str(),
        ],
        &prefix,
    );
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let actual: NativeCatalogPrefixResultV1 = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(actual.content_sha256, original.content_sha256);
}
