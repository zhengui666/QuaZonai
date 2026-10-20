//! Price/volume model inputs, not ABI recovery or historical execution definitions.
use super::{event_key, field, hf_selection, seconds, text, v2, SourceRowContext};
use alloy_core::primitives::U256;
use anyhow::{ensure, Context, Result};
use nautilus_model::{
    data::TradeTick,
    enums::AggressorSide,
    identifiers::{InstrumentId, TradeId},
    types::{Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use parquet::{
    file::reader::{FileReader, SerializedFileReader},
    record::{Field, Row},
};
use rust_decimal::{Decimal, RoundingStrategy};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashSet},
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

const MODEL: &str = "HF_FLOAT64_SHORTEST_DECIMAL_ROUND_HALF_UP_6DP";

#[derive(clap::Args)]
pub struct Arguments {
    #[arg(long)]
    selection: PathBuf,
    #[arg(long)]
    start_seconds: u64,
    #[arg(long)]
    end_seconds: u64,
    /// New data-only output, not a registered dataset or venue catalog.
    #[arg(long)]
    output: PathBuf,
}

fn amount(row: &Row, name: &str) -> Result<(Decimal, bool)> {
    let Field::Double(value) = field(row, name)? else {
        anyhow::bail!("MODEL_FLOAT64_REQUIRED");
    };
    ensure!(value.is_finite() && *value > 0.0, "MODEL_AMOUNT_INVALID");
    let original = Decimal::from_str(&value.to_string())?;
    let modeled = original.round_dp_with_strategy(6, RoundingStrategy::MidpointAwayFromZero);
    let units = modeled
        .checked_mul(Decimal::from(1_000_000))
        .context("MODEL_AMOUNT_RANGE")?
        .normalize()
        .to_string()
        .parse::<u64>()?;
    ensure!(units > 0 && units < (1_u64 << 52), "MODEL_AMOUNT_RANGE");
    Ok((modeled, original != modeled))
}

pub fn prepare(args: &Arguments) -> Result<Value> {
    ensure!(!args.output.exists(), "MODEL_OUTPUT_EXISTS");
    ensure!(
        args.start_seconds < args.end_seconds
            && args.start_seconds.is_multiple_of(60)
            && args.end_seconds.is_multiple_of(60)
            && args.end_seconds <= u64::try_from(chrono::Utc::now().timestamp())?,
        "MODEL_COMPLETE_MINUTE_WINDOW_REQUIRED"
    );
    let mut selection = hf_selection::load(&args.selection, args.start_seconds, args.end_seconds)?;
    ensure!(
        selection.repository == contracts::recorded_fees::POLYMARKET_V2_SOURCE,
        "MODEL_V2_SOURCE_REQUIRED"
    );
    selection.files.sort();
    let parent = args
        .output
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let staging = tempfile::Builder::new()
        .prefix(".model-bars-")
        .tempdir_in(parent)?;
    let root = staging.path().join("catalog");
    fs::create_dir(&root)?;
    let catalog = ParquetDataCatalog::from_uri(
        root.to_str().context("MODEL_PATH_ENCODING")?,
        None,
        Some(4096),
        None,
        None,
    )?;
    let mut identities = BTreeSet::new();
    let (
        mut scanned,
        mut outside,
        mut summaries,
        mut trades_count,
        mut bars_count,
        mut changed_quantity,
        mut changed_cash,
        mut self_trades,
    ) = (0_u64, 0_u64, 0_u64, 0_u64, 0_u64, 0_u64, 0_u64, 0_u64);
    let mut previous_range: Option<(u64, u64)> = None;
    for file in &selection.files {
        let relative = file.strip_prefix(&selection.files_root)?.to_string_lossy();
        ensure!(
            selection.file_states.get(file) == Some(&hf_selection::file_state(file)?),
            "SELECTION_FILE_CHANGED"
        );
        let reader = SerializedFileReader::new(fs::File::open(file)?)?;
        let projection = if reader.metadata().file_metadata().num_rows() == 0 {
            None
        } else {
            let schema = reader
                .metadata()
                .file_metadata()
                .schema_descr()
                .root_schema();
            let fields = [
                "id",
                "timestamp",
                "maker",
                "taker",
                "token_asset_id",
                "token_amount",
                "usdc_amount",
            ]
            .iter()
            .map(|name| {
                schema
                    .get_fields()
                    .iter()
                    .find(|field| field.name() == *name)
                    .cloned()
                    .context("MODEL_SOURCE_FIELD_REQUIRED")
            })
            .collect::<Result<Vec<_>>>()?;
            Some(
                parquet::schema::types::Type::group_type_builder(schema.name())
                    .with_fields(fields)
                    .build()?,
            )
        };
        // Bound ordinary row storage to one source file; no whole-catalog forecast arrays.
        let mut series = BTreeMap::<InstrumentId, Vec<TradeTick>>::new();
        let mut events = HashSet::new();
        let mut first_second = u64::MAX;
        let mut last_second = 0;
        let mut first_block = u64::MAX;
        let mut last_block = 0;
        for (index, row) in reader.get_row_iter(projection)?.enumerate() {
            let parsed = (|| -> Result<()> {
                let row = row?;
                scanned += 1;
                let at = seconds(&row, "timestamp")?;
                if at < args.start_seconds || at >= args.end_seconds {
                    outside += 1;
                    return Ok(());
                }
                let id = text(&row, "id")?;
                let key = event_key(id)?;
                let taker = text(&row, "taker")?;
                if v2::EXCHANGES_V2
                    .iter()
                    .any(|address| address.eq_ignore_ascii_case(taker))
                {
                    summaries += 1;
                    return Ok(());
                }
                ensure!(events.insert(key), "MODEL_DUPLICATE_SOURCE_EVENT");
                self_trades += u64::from(text(&row, "maker")?.eq_ignore_ascii_case(taker));
                let token = text(&row, "token_asset_id")?;
                let number = U256::from_str(token)?;
                ensure!(
                    !number.is_zero() && number.to_string() == token,
                    "MODEL_CANONICAL_TOKEN_REQUIRED"
                );
                let (quantity, changed) = amount(&row, "token_amount")?;
                changed_quantity += u64::from(changed);
                let (cash, changed) = amount(&row, "usdc_amount")?;
                changed_cash += u64::from(changed);
                let ratio = cash.checked_div(quantity).context("MODEL_PRICE_RANGE")?;
                ensure!(
                    ratio > Decimal::ZERO && ratio <= Decimal::ONE,
                    "MODEL_PRICE_RANGE"
                );
                let price = Price::from_decimal_dp(ratio, 9)?;
                ensure!(
                    price.as_decimal() > Decimal::ZERO,
                    "MODEL_PRICE_PRECISION_LOSS"
                );
                let size = Quantity::from_decimal_dp(quantity, 6)?;
                ensure!(
                    size.as_decimal() == quantity,
                    "MODEL_QUANTITY_PRECISION_LOSS"
                );
                let instrument: InstrumentId = format!("{token}.POLYMARKET").parse()?;
                let at_ns = super::super::epoch_ns(at)?;
                series.entry(instrument).or_default().push(TradeTick::new(
                    instrument,
                    price,
                    size,
                    AggressorSide::NoAggressor,
                    TradeId::new_checked(id)?,
                    at_ns,
                    at_ns,
                ));
                first_second = first_second.min(at);
                last_second = last_second.max(at);
                first_block = first_block.min(key.0);
                last_block = last_block.max(key.0);
                Ok(())
            })();
            parsed
                .with_context(|| SourceRowContext::new("MODEL_SOURCE_ROW", &relative, index + 1))?;
        }
        if !series.is_empty() {
            // Source files must be ordered disjoint minute/block partitions, not overlapping shards.
            if let Some((second, block)) = previous_range {
                ensure!(
                    second / 60 < first_second / 60 && block < first_block,
                    "MODEL_SOURCE_PARTITIONS_OVERLAP"
                );
            }
            previous_range = Some((last_second, last_block));
            for (id, mut trades) in series {
                trades.sort_by_key(|trade| {
                    (
                        trade.ts_event,
                        event_key(trade.trade_id.as_str()).expect("validated ID"),
                    )
                });
                let bars = super::aggregate_series(&trades, 60, 9, 6)?;
                catalog.write_to_parquet(&trades, None, None, None)?;
                catalog.write_to_parquet(&bars, None, None, None)?;
                trades_count += trades.len() as u64;
                bars_count += bars.len() as u64;
                identities.insert(id);
            }
        }
        ensure!(
            selection.file_states.get(file) == Some(&hf_selection::file_state(file)?),
            "SELECTION_FILE_CHANGED"
        );
        eprintln!("model-bars: files processed; trades={trades_count} bars={bars_count}");
    }
    ensure!(trades_count > 0, "MODEL_EMPTY_SELECTION");
    let report = json!({"schema_version":1,"source":selection.repository,"revision":selection.revision,
        "source_observed_at":selection.retrieved_at,"amount_model":MODEL,
        "selection":{"start_seconds":args.start_seconds,"end_seconds":args.end_seconds,"bar_seconds":60},
        "scanned_rows":scanned,"outside_window_rows":outside,"exchange_summaries_excluded":summaries,
        "trades":trades_count,"bars":bars_count,"instrument_identities":identities.len(),"instrument_definitions":0,
        "quantized_quantity_rows":changed_quantity,"quantized_cash_rows":changed_cash,"self_trades":self_trades,
        "model_price_precision":9,"model_size_precision":6,
        "catalog":args.output.join("catalog"),"source_selection":selection.manifest,
        "historical_availability":"UNVERIFIED_EVENT_TIME_PROXY","coverage":"UNPROVEN","registered_in_quazonai":false,
        "qualification":"MODEL_INPUT_ONLY_NOT_ABI_RECOVERY_OR_HISTORICAL_EXECUTION_DEFINITIONS",
        "fees":"NOT_PREPARED_USE_ORIGINAL_SAME_ID_RECORDED_FEES_SEPARATELY"});
    fs::write(
        staging.path().join("import-report.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    hf_selection::unchanged(&selection.file_states)?;
    rustix::fs::renameat_with(
        rustix::fs::CWD,
        staging.path(),
        rustix::fs::CWD,
        &args.output,
        rustix::fs::RenameFlags::NOREPLACE,
    )
    .map_err(std::io::Error::from)?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn model_normalization_is_explicit_and_does_not_claim_exact_recovery() {
        let row = |value| Row::new(vec![("token_amount".into(), Field::Double(value))]);
        assert_eq!(
            amount(&row(1.0000000000000002), "token_amount").unwrap(),
            (Decimal::ONE, true)
        );
        assert_eq!(
            amount(&row(0.0000015), "token_amount").unwrap().0,
            Decimal::from_str("0.000002").unwrap()
        );
        assert_eq!(amount(&row(2.564101), "token_amount").unwrap().1, false);
        for value in [
            0.0,
            -1.0,
            0.0000001,
            f64::NAN,
            f64::INFINITY,
            9_007_199_254.740992,
        ] {
            assert!(amount(&row(value), "token_amount").is_err());
        }
        assert!(amount(
            &Row::new(vec![("token_amount".into(), Field::Null)]),
            "token_amount"
        )
        .is_err());
    }
}
