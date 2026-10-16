//! Synthetic protocol fixtures. Passing these does not verify live source bytes or PIT.
use super::*;
use parquet::{
    data_type::{ByteArrayType, FixedLenByteArray, FixedLenByteArrayType, Int32Type, Int64Type},
    file::writer::SerializedFileWriter,
    schema::parser::parse_message_type,
};
use std::sync::Arc;

fn raw(value: U256) -> Field {
    Field::Bytes(value.to_le_bytes::<32>().to_vec().into())
}

fn row(block: u64, log: u32, transaction: char) -> Row {
    Row::new(vec![
        ("timestamp".into(), Field::ULong(10)),
        ("block_number".into(), Field::ULong(block)),
        (
            "transaction_hash".into(),
            Field::Str(format!("0x{}", transaction.to_string().repeat(64))),
        ),
        ("log_index".into(), Field::UInt(log)),
        ("contract".into(), Field::Str("CTF_EXCHANGE".into())),
        (
            "order_hash".into(),
            Field::Str(format!("0x{}", "c".repeat(64))),
        ),
        ("maker".into(), Field::Str(format!("0x{}", "a".repeat(40)))),
        ("taker".into(), Field::Str(format!("0x{}", "b".repeat(40)))),
        ("maker_asset_id".into(), Field::Str("0".into())),
        ("taker_asset_id".into(), Field::Str("123".into())),
        ("maker_amount_filled".into(), raw(U256::from(500_001))),
        ("taker_amount_filled".into(), raw(U256::from(1_000_000))),
        ("maker_fee".into(), raw(U256::ZERO)),
        ("taker_fee".into(), raw(U256::from(7))),
        ("protocol_fee".into(), raw(U256::ZERO)),
    ])
}

fn replace(row: &Row, key: &str, value: Field) -> Row {
    Row::new(
        row.get_column_iter()
            .map(|(name, original)| {
                (
                    name.clone(),
                    if name == key {
                        value.clone()
                    } else {
                        original.clone()
                    },
                )
            })
            .collect(),
    )
}

fn instruments(currency: &str) -> BTreeMap<String, InstrumentAny> {
    let mut value = serde_json::to_value(super::super::tests::instrument()).unwrap();
    value["BinaryOption"]["currency"] = currency.into();
    BTreeMap::from([("123".into(), serde_json::from_value(value).unwrap())])
}

#[test]
fn little_endian_raw_units_and_original_identity_are_preserved() {
    let mut quality = Quality::default();
    let (trade, identity) = fill(&row(10, 2, 'd'), &instruments("USDC.e"), &mut quality)
        .unwrap()
        .unwrap();
    assert_eq!(trade.size.as_decimal(), Decimal::ONE);
    assert_eq!(
        trade.price.as_decimal(),
        Decimal::from_str("0.5000").unwrap()
    );
    assert_eq!(quality.rounded_trade_prices, 1);
    assert_eq!(trade.trade_id.as_str(), "137_10_2");
    assert_eq!(identity.transaction_hash, format!("0x{}", "d".repeat(64)));
    assert_eq!(identity.log_index, 2);
    assert_eq!(trade.ts_event, trade.ts_init);
    let (trade, _) = fill(
        &row(u64::MAX, u32::MAX, 'd'),
        &instruments("USDC.e"),
        &mut Quality::default(),
    )
    .unwrap()
    .unwrap();
    assert!(trade.trade_id.as_str().len() <= 36);
}

#[test]
fn raw_binary_is_required_and_never_guessed_from_float_or_text() {
    for value in [
        Field::Double(500_001.0),
        Field::Str("500001".into()),
        Field::Bytes(vec![1; 31].into()),
        Field::Bytes(vec![1; 33].into()),
        raw(U256::ZERO),
        raw(U256::MAX),
    ] {
        let source = replace(&row(10, 1, 'd'), "maker_amount_filled", value);
        assert!(fill(&source, &instruments("USDC.e"), &mut Quality::default()).is_err());
    }
}

#[test]
fn exact_large_integer_keeps_units_beyond_binary_float_precision() {
    let source = replace(
        &row(10, 1, 'd'),
        "taker_amount_filled",
        raw(U256::from(9_007_199_254_740_993_u64)),
    );
    assert_eq!(
        quantity(&source, "taker_amount_filled").unwrap(),
        Decimal::from_str("9007199254.740993").unwrap()
    );
}

#[test]
fn both_cash_directions_retain_original_outcome_without_yes_normalization() {
    let original = row(10, 1, 'd');
    let sold = replace(
        &replace(
            &replace(
                &replace(&original, "maker_asset_id", Field::Str("123".into())),
                "taker_asset_id",
                Field::Str("0".into()),
            ),
            "maker_amount_filled",
            raw(U256::from(1_000_000)),
        ),
        "taker_amount_filled",
        raw(U256::from(500_001)),
    );
    let a = fill(&original, &instruments("USDC.e"), &mut Quality::default())
        .unwrap()
        .unwrap()
        .0;
    let b = fill(&sold, &instruments("USDC.e"), &mut Quality::default())
        .unwrap()
        .unwrap()
        .0;
    assert_eq!(a.price, b.price);
    assert_eq!(a.size, b.size);
    assert_eq!(b.aggressor_side, AggressorSide::NoAggressor);
}

#[test]
fn exchange_family_determines_original_collateral_and_unknowns_fail() {
    for contract in [
        "CTF_EXCHANGE",
        "NEGRISK_CTF_EXCHANGE",
        "CTF_EXCHANGE_V2",
        "NEGRISK_CTF_EXCHANGE_V2",
    ] {
        let source = replace(&row(10, 1, 'd'), "contract", Field::Str(contract.into()));
        let expected = if contract.ends_with("_V2") {
            "pUSD"
        } else {
            "USDC.e"
        };
        assert!(
            fill(&source, &instruments(expected), &mut Quality::default())
                .unwrap()
                .is_some()
        );
        let wrong = if expected == "pUSD" { "USDC.e" } else { "pUSD" };
        assert!(fill(&source, &instruments(wrong), &mut Quality::default()).is_err());
    }
    let source = replace(&row(10, 1, 'd'), "contract", Field::Str("UNKNOWN".into()));
    assert!(fill(&source, &instruments("USDC.e"), &mut Quality::default()).is_err());
}

#[test]
fn summaries_are_excluded_self_trades_retained_and_unselected_tokens_skipped() {
    for taker in EXCHANGES.iter().chain(v2::EXCHANGES_V2.iter()) {
        let source = replace(&row(10, 1, 'd'), "taker", Field::Str((*taker).into()));
        let mut quality = Quality::default();
        assert!(fill(&source, &instruments("USDC.e"), &mut quality)
            .unwrap()
            .is_none());
        assert_eq!(quality.exchange_summaries_excluded, 1);
    }
    let source = replace(
        &row(10, 1, 'd'),
        "taker",
        Field::Str(format!("0x{}", "a".repeat(40))),
    );
    let mut quality = Quality::default();
    assert!(fill(&source, &instruments("USDC.e"), &mut quality)
        .unwrap()
        .is_some());
    assert_eq!(quality.self_trades, 1);
    let other = replace(&source, "taker_asset_id", Field::Str("456".into()));
    assert!(fill(&other, &instruments("USDC.e"), &mut quality)
        .unwrap()
        .is_none());
}

#[test]
fn malformed_source_identity_and_missing_cash_are_rejected() {
    for (key, value) in [
        ("transaction_hash", Field::Str("d".repeat(64))),
        ("order_hash", Field::Str(format!("0x{}", "G".repeat(64)))),
        ("maker", Field::Str("alice".into())),
        ("block_number", Field::ULong(0)),
        ("log_index", Field::Int(-1)),
        ("log_index", Field::ULong(u64::MAX)),
        ("taker_fee", Field::Double(0.0)),
        ("maker_asset_id", Field::Str("0123".into())),
        ("maker_asset_id", Field::Str("456".into())),
    ] {
        let source = replace(&row(10, 1, 'd'), key, value);
        assert!(
            fill(&source, &instruments("USDC.e"), &mut Quality::default()).is_err(),
            "{key}"
        );
    }
}

// Real Parquet encoding of wholly synthetic records with the published types.
fn fixture(root: &Path, rows: &[Row]) -> Arguments {
    fixture_currency(root, rows, "USDC.e")
}

fn fixture_currency(root: &Path, rows: &[Row], currency: &str) -> Arguments {
    let root = root.canonicalize().unwrap();
    let cache = root.join("cache");
    let path = cache.join("files/raw/sii.parquet");
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    let declarations = rows[0]
        .get_column_iter()
        .map(|(name, value)| {
            let data_type = match value {
                Field::ULong(_) => "INT64",
                Field::UInt(_) => "INT32",
                Field::Str(_) => "BINARY",
                Field::Bytes(_) => "FIXED_LEN_BYTE_ARRAY (32)",
                _ => panic!("unsupported fixture type"),
            };
            let logical = match value {
                Field::ULong(_) => " (INTEGER(64,false))",
                Field::UInt(_) => " (INTEGER(32,false))",
                Field::Str(_) => " (UTF8)",
                _ => "",
            };
            format!("REQUIRED {data_type} {name}{logical};")
        })
        .collect::<Vec<_>>()
        .join(" ");
    let schema =
        Arc::new(parse_message_type(&format!("message source {{ {declarations} }}")).unwrap());
    let mut writer =
        SerializedFileWriter::new(fs::File::create(&path).unwrap(), schema, Default::default())
            .unwrap();
    let mut group = writer.next_row_group().unwrap();
    for (name, example) in rows[0].get_column_iter() {
        let mut column = group.next_column().unwrap().unwrap();
        match example {
            Field::ULong(_) => {
                let values = rows
                    .iter()
                    .map(|r| match field(r, name).unwrap() {
                        Field::ULong(n) => *n as i64,
                        _ => panic!("type"),
                    })
                    .collect::<Vec<_>>();
                column
                    .typed::<Int64Type>()
                    .write_batch(&values, None, None)
                    .unwrap();
            }
            Field::UInt(_) => {
                let values = rows
                    .iter()
                    .map(|r| match field(r, name).unwrap() {
                        Field::UInt(n) => *n as i32,
                        _ => panic!("type"),
                    })
                    .collect::<Vec<_>>();
                column
                    .typed::<Int32Type>()
                    .write_batch(&values, None, None)
                    .unwrap();
            }
            Field::Str(_) => {
                let values = rows
                    .iter()
                    .map(|r| text(r, name).unwrap().into())
                    .collect::<Vec<_>>();
                column
                    .typed::<ByteArrayType>()
                    .write_batch(&values, None, None)
                    .unwrap();
            }
            Field::Bytes(_) => {
                let values = rows
                    .iter()
                    .map(|r| match field(r, name).unwrap() {
                        Field::Bytes(b) => FixedLenByteArray::from(b.data().to_vec()),
                        _ => panic!("type"),
                    })
                    .collect::<Vec<_>>();
                column
                    .typed::<FixedLenByteArrayType>()
                    .write_batch(&values, None, None)
                    .unwrap();
            }
            _ => panic!("unsupported fixture type"),
        }
        column.close().unwrap();
    }
    group.close().unwrap();
    writer.close().unwrap();
    let size = fs::metadata(&path).unwrap().len();
    let plan_file = serde_json::json!({"path":"raw/sii.parquet", "size":size, "format":"parquet",
        "url":format!("https://huggingface.co/datasets/fixture/sii/resolve/{}/raw/sii.parquet", "a".repeat(40))});
    let mut recorded = plan_file.clone();
    recorded["local_path"] = path.to_str().unwrap().into();
    recorded["cached"] = true.into();
    recorded["resumed_bytes"] = 0.into();
    recorded["validation"] = "PARQUET_ENVELOPE".into();
    let selection = serde_json::json!({"schema":"qz.hf_selection/1", "plan":{
        "schema":"qz.hf_dataset_plan/1", "repository":"fixture/sii", "requested_revision":"main",
        "revision":"a".repeat(40), "license":null, "partition_index":null,
        "request":{"includes":["raw/sii.parquet"], "markets":[], "start_date":null, "end_date":null},
        "selection_bounds":"[start_date,end_date)", "download_granularity":"FILE_PARTITION",
        "coverage":"NOT_ASSERTED", "max_bytes":1000000, "total_bytes":size, "files":[plan_file]},
        "cache_root":cache, "files":[recorded], "retrieved_at":Utc::now(), "downloaded_bytes":0, "cached_files":1});
    let manifest = root.join("selection.json");
    fs::write(&manifest, serde_json::to_vec(&selection).unwrap()).unwrap();
    let definitions = root.join("instruments.json");
    fs::write(
        &definitions,
        serde_json::to_vec(&instruments(currency).into_values().collect::<Vec<_>>()).unwrap(),
    )
    .unwrap();
    Arguments {
        snapshot: None,
        selection: Some(manifest),
        chain_evidence: None,
        format: Format::SiiOrderFilled,
        instruments: definitions,
        start_seconds: 0,
        end_seconds: 240,
        bar_seconds: Some(60),
        output: root.join("native"),
    }
}

#[test]
fn raw_parquet_selection_deduplicates_sorts_and_preserves_tx_log_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let args = fixture(
        directory.path(),
        &[row(10, 10, 'd'), row(10, 2, 'e'), row(10, 2, 'e')],
    );
    let archive = prepare(&args).unwrap();
    assert_eq!(archive.trades.len(), 2);
    assert_eq!(archive.bars.len(), 1);
    assert_eq!(archive.trades[0].trade_id.as_str(), "137_10_2");
    assert_eq!(archive.source_metadata["quality"]["duplicate_rows"], 1);
    assert_eq!(
        archive.source_metadata["sii_event_identities"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert_eq!(
        archive.source_metadata["clock_basis"],
        "REQUEST_SELECTION_AT_NOT_HISTORICAL_AVAILABILITY"
    );
    assert!(archive.source_metadata.get("instruments_sha256").is_none());
}

#[test]
fn conflicting_native_or_transaction_log_identities_fail_instead_of_doubling_volume() {
    for rows in [
        vec![row(10, 2, 'd'), row(10, 2, 'e')],
        vec![row(10, 2, 'd'), row(11, 2, 'd')],
        vec![
            row(10, 2, 'd'),
            replace(
                &row(10, 2, 'd'),
                "maker_amount_filled",
                raw(U256::from(600_000)),
            ),
        ],
    ] {
        let directory = tempfile::tempdir().unwrap();
        let args = fixture(directory.path(), &rows);
        assert!(prepare(&args).is_err());
    }
}

#[test]
fn same_order_hash_is_not_a_duplicate_fill_and_half_open_window_is_preserved() {
    let directory = tempfile::tempdir().unwrap();
    let outside = replace(&row(11, 1, 'e'), "timestamp", Field::ULong(240));
    let args = fixture(
        directory.path(),
        &[row(10, 2, 'd'), row(10, 3, 'd'), outside],
    );
    let archive = prepare(&args).unwrap();
    assert_eq!(archive.trades.len(), 2);
    assert_eq!(archive.source_metadata["quality"]["duplicate_rows"], 0);
}

#[test]
fn v1_and_v2_raw_parquet_publish_and_read_back_native_records_without_qualification() {
    use nautilus_model::data::Data;
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;
    for (contract, currency) in [
        ("CTF_EXCHANGE", "USDC.e"),
        ("NEGRISK_CTF_EXCHANGE_V2", "pUSD"),
    ] {
        let directory = tempfile::tempdir().unwrap();
        let original = replace(&row(10, 2, 'd'), "contract", Field::Str(contract.into()));
        let args = fixture_currency(directory.path(), &[original], currency);
        let archive = prepare(&args).unwrap();
        let trade = archive.trades[0];
        let expected_metadata = archive.source_metadata.clone();
        let report = super::super::super::import(archive, &args.output).unwrap();
        assert_eq!(report.trades, 1);
        assert_eq!(report.bars, 1);
        assert_eq!(report.historical_availability, "UNVERIFIED");
        assert!(!report.registered_in_quazonai);
        let evidence: serde_json::Value =
            read_json(&args.output.join("source-evidence.json")).unwrap();
        assert_eq!(evidence["source_metadata"], expected_metadata);
        let root = args.output.join("catalog");
        let mut catalog =
            ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, None, None, None).unwrap();
        assert_eq!(
            catalog.instruments(None, None, None).unwrap()[0]
                .quote_currency()
                .code
                .as_str(),
            currency
        );
        let readback = catalog
            .query::<TradeTick>(None, None, None, None, None, true)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(readback, vec![Data::Trade(trade)]);
        let bars = catalog
            .query::<Bar>(None, None, None, None, None, true)
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(bars.len(), 1);
    }
}
