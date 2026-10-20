//! Prepare conditional cost replay from original source fields and same-ID native quantities.
//! No fee formula, quantity renormalization, instrument definition or qualification is inferred.
use super::{event_key, field, hf_selection, seconds, text, v2, SourceRowContext};
use anyhow::{ensure, Context, Result};
use bigdecimal::BigDecimal;
use contracts::{recorded_fees::*, science::NativeBarSelectionV1, DbCounter, SchemaV1};
use nautilus_model::data::{Bar, TradeTick};
use nautilus_persistence::backend::catalog::urisafe_instrument_id;
use nautilus_serialization::arrow::DecodeFromRecordBatch;
use parquet::{
    arrow::arrow_reader::ParquetRecordBatchReaderBuilder,
    file::reader::{FileReader, SerializedFileReader},
    record::{Field, Row},
};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    path::{Path, PathBuf},
    str::FromStr,
};

const MINUTE: u64 = 60_000_000_000;

#[derive(clap::Args)]
pub struct Arguments {
    /// Original cached HF selection, outside the researcher-readable catalog.
    #[arg(long)]
    pub selection: PathBuf,
    /// Original data-only native catalog; never modified by this command.
    #[arg(long)]
    pub catalog: PathBuf,
    /// NativeBarSelectionV1 for the caller's actual temporal/market selection.
    #[arg(long)]
    pub bar_selection: PathBuf,
    /// New recorded-fees directory; existing paths are not overwritten.
    #[arg(long)]
    pub output: PathBuf,
}

// Use the official path encoder and safe Arrow decoder, without traversing unrelated catalog files.
fn native<T: DecodeFromRecordBatch>(
    root: &Path,
    kind: &str,
    id: &str,
    mut accept: impl FnMut(T) -> Result<()>,
) -> Result<()> {
    let directory = root.join("data").join(kind).join(urisafe_instrument_id(id));
    let mut paths = fs::read_dir(directory)?
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<std::io::Result<Vec<_>>>()?;
    paths.retain(|path| path.extension().is_some_and(|ext| ext == "parquet"));
    paths.sort();
    ensure!(!paths.is_empty(), "RECORDED_FEE_NATIVE_FILES_REQUIRED");
    for path in paths {
        let builder = ParquetRecordBatchReaderBuilder::try_new(fs::File::open(path)?)?;
        let metadata = builder.schema().metadata().clone();
        for batch in builder.with_batch_size(4096).build()? {
            for item in T::decode_batch(&metadata, batch?)? {
                accept(item)?;
            }
        }
    }
    Ok(())
}

struct Original {
    instrument_id: String,
    event: u64,
    quantity: contracts::DecimalValue,
    record: Option<RecordedTradeFeeV1>,
}

fn original(row: &Row, name: &str) -> Result<String> {
    let Field::Double(value) = field(row, name)? else {
        anyhow::bail!("RECORDED_FEE_SOURCE_FLOAT_REQUIRED:{name}");
    };
    ensure!(value.is_finite(), "RECORDED_FEE_SOURCE_NONFINITE:{name}");
    Ok(value.to_string())
}

pub fn prepare(args: &Arguments) -> Result<Value> {
    ensure!(!args.output.exists(), "RECORDED_FEE_OUTPUT_EXISTS");
    let selected: NativeBarSelectionV1 = super::read_json(&args.bar_selection)?;
    let types = crate::catalog::selected_types(&selected)?;
    let root = crate::catalog::catalog_root(&args.catalog)?;
    let mut bars = BTreeMap::new();
    let mut tokens = BTreeSet::new();
    let mut rows = 0;
    for kind in types {
        let id = kind.instrument_id().to_string();
        ensure!(
            kind.to_string().ends_with("-1-MINUTE-LAST-EXTERNAL")
                && kind.instrument_id().venue.as_str() == "POLYMARKET",
            "RECORDED_FEE_ONE_MINUTE_POLYMARKET_REQUIRED"
        );
        let token = kind.instrument_id().symbol.to_string();
        ensure!(
            !token.is_empty()
                && !token.starts_with('0')
                && token.bytes().all(|byte| byte.is_ascii_digit()),
            "RECORDED_FEE_DATA_ONLY_TOKEN_ID_REQUIRED"
        );
        tokens.insert(token);
        let mut previous = None;
        let mut observed = false;
        native::<Bar>(&root, "bars", &kind.to_string(), |bar| {
            ensure!(bar.bar_type == kind, "RECORDED_FEE_NATIVE_ID_MISMATCH");
            let end = bar.ts_event.as_u64();
            let available = bar.ts_init.as_u64();
            if end < selected.event_start_ns.get()
                || end >= selected.event_end_ns.get()
                || available < selected.event_start_ns.get()
                || available > selected.decision_cutoff_ns.get()
            {
                return Ok(());
            }
            crate::catalog::validate_price_bar(&bar, previous.as_ref())?;
            ensure!(
                end == available && end >= MINUTE && end.is_multiple_of(MINUTE),
                "RECORDED_FEE_BAR_CLOCK_MISMATCH"
            );
            ensure!(rows < selected.maximum_rows as usize, "CATALOG_ROW_LIMIT");
            rows += 1;
            observed = true;
            let volume = BigDecimal::from_str(&bar.volume.as_decimal().to_string())?;
            ensure!(
                bars.insert((id.clone(), end), (volume, BigDecimal::from(0)))
                    .is_none(),
                "RECORDED_FEE_DUPLICATE_BAR"
            );
            previous = Some(bar);
            Ok(())
        })?;
        ensure!(observed, "CATALOG_EMPTY_SELECTION");
    }
    let first = bars
        .keys()
        .map(|(_, end)| *end - MINUTE)
        .min()
        .context("CATALOG_EMPTY_SELECTION")?;
    let last = bars
        .keys()
        .map(|(_, end)| *end)
        .max()
        .context("CATALOG_EMPTY_SELECTION")?;
    let selection =
        hf_selection::load(&args.selection, first / 1_000_000_000, last / 1_000_000_000)?;
    ensure!(
        selection.repository == POLYMARKET_V2_SOURCE,
        "RECORDED_FEE_SOURCE_REQUIRED"
    );
    let mut originals = HashMap::<String, Original>::new();
    for token in &tokens {
        let id = format!("{token}.POLYMARKET");
        native::<TradeTick>(&root, "trades", &id, |tick| {
            ensure!(
                tick.instrument_id.to_string() == id,
                "RECORDED_FEE_NATIVE_ID_MISMATCH"
            );
            let event = tick.ts_event.as_u64();
            let end = (event / MINUTE + 1)
                .checked_mul(MINUTE)
                .context("RECORDED_FEE_TIME_RANGE")?;
            let Some((_, total)) = bars.get_mut(&(id.clone(), end)) else {
                return Ok(());
            };
            ensure!(
                tick.ts_init.as_u64() <= selected.decision_cutoff_ns.get(),
                "RECORDED_FEE_NATIVE_FROM_FUTURE"
            );
            ensure!(
                !tick.size.is_undefined(),
                "RECORDED_FEE_NATIVE_QUANTITY_UNDEFINED"
            );
            let quantity: contracts::DecimalValue = tick
                .size
                .as_decimal()
                .to_string()
                .parse()
                .map_err(anyhow::Error::msg)?;
            ensure!(quantity.is_positive(), "RECORDED_FEE_RECORD_INVALID");
            *total += quantity.as_decimal();
            let event_id = tick.trade_id.to_string();
            event_key(&event_id)?;
            ensure!(
                originals
                    .insert(
                        event_id,
                        Original {
                            instrument_id: id.clone(),
                            event,
                            quantity,
                            record: None
                        }
                    )
                    .is_none(),
                "RECORDED_FEE_DUPLICATE_NATIVE_ID"
            );
            Ok(())
        })?;
    }
    ensure!(
        bars.values().all(|(volume, total)| volume == total),
        SourceRowContext::new("RECORDED_FEE_BAR_QUANTITY_MISMATCH", "native", 0)
    );
    ensure!(!originals.is_empty(), "RECORDED_FEE_NATIVE_TRADES_REQUIRED");
    for path in &selection.files {
        let reader = SerializedFileReader::new(fs::File::open(path)?)?;
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
                "taker",
                "timestamp",
                "token_asset_id",
                "token_amount",
                "usdc_amount",
                "price",
                "fee_usdc",
                "maker_direction",
                "taker_direction",
            ]
            .iter()
            .map(|name| {
                schema
                    .get_fields()
                    .iter()
                    .find(|field| field.name() == *name)
                    .cloned()
                    .with_context(|| format!("RECORDED_FEE_SOURCE_FIELD_REQUIRED:{name}"))
            })
            .collect::<Result<Vec<_>>>()?;
            Some(
                parquet::schema::types::Type::group_type_builder(schema.name())
                    .with_fields(fields)
                    .build()?,
            )
        };
        for row in reader.get_row_iter(projection)? {
            let row = row?;
            if !tokens.contains(text(&row, "token_asset_id")?) {
                continue;
            }
            let taker = text(&row, "taker")?;
            if v2::EXCHANGES_V2
                .iter()
                .any(|address| address.eq_ignore_ascii_case(taker))
            {
                continue;
            }
            let id = text(&row, "id")?;
            let Some(native) = originals.get_mut(id) else {
                continue;
            };
            ensure!(
                native.instrument_id == format!("{}.POLYMARKET", text(&row, "token_asset_id")?),
                "RECORDED_FEE_SOURCE_ID_MISMATCH"
            );
            let event = seconds(&row, "timestamp")?
                .checked_mul(1_000_000_000)
                .context("RECORDED_FEE_TIME_RANGE")?;
            ensure!(event == native.event, "RECORDED_FEE_SOURCE_CLOCK_MISMATCH");
            let record = RecordedTradeFeeV1 {
                id: id.into(),
                instrument_id: native.instrument_id.clone(),
                ts_event_ns: DbCounter::new(event).map_err(anyhow::Error::msg)?,
                source_quantity: original(&row, "token_amount")?
                    .parse()
                    .map_err(anyhow::Error::msg)?,
                quantity: native.quantity.clone(),
                fee_usdc: original(&row, "fee_usdc")?
                    .parse()
                    .map_err(anyhow::Error::msg)?,
                usdc_amount: original(&row, "usdc_amount")?
                    .parse()
                    .map_err(anyhow::Error::msg)?,
                price: original(&row, "price")?
                    .parse()
                    .map_err(anyhow::Error::msg)?,
                maker_direction: text(&row, "maker_direction")?.into(),
                taker_direction: text(&row, "taker_direction")?.into(),
            };
            ensure!(
                record.source_quantity.is_positive()
                    && record.usdc_amount.is_positive()
                    && record.price.is_positive()
                    && record.fee_usdc.to_decimal() >= BigDecimal::from(0)
                    && !record.maker_direction.is_empty()
                    && !record.taker_direction.is_empty(),
                "RECORDED_FEE_RECORD_INVALID"
            );
            if let Some(previous) = &native.record {
                ensure!(*previous == record, "RECORDED_FEE_CONFLICTING_SOURCE_ID");
            } else {
                native.record = Some(record);
            }
        }
        ensure!(
            selection.file_states.get(path) == Some(&hf_selection::file_state(path)?),
            "SELECTION_FILE_CHANGED"
        );
    }
    let count = originals.len();
    let mut parts = BTreeMap::<String, Vec<RecordedTradeFeeV1>>::new();
    for native in originals.into_values() {
        let record = native.record.with_context(|| {
            SourceRowContext::new("RECORDED_FEE_SOURCE_RECORD_MISSING", "source", 0)
        })?;
        parts.entry(native.instrument_id).or_default().push(record);
    }
    // No output is created until every original native trade has a matching source cost record.
    fs::create_dir(&args.output)?;
    for (id, records) in &mut parts {
        records.sort_by(|a, b| (a.ts_event_ns, &a.id).cmp(&(b.ts_event_ns, &b.id)));
        let source = RecordedTradeFeesV1 {
            schema_version: SchemaV1,
            source: selection.repository.clone(),
            revision: selection.revision.clone(),
            currency: "pUSD".into(),
            records: std::mem::take(records),
        };
        fs::write(
            args.output.join(format!("{id}.json")),
            serde_json::to_vec(&source)?,
        )?;
    }
    Ok(
        json!({"source": selection.repository, "revision": selection.revision, "records": count,
        "instruments": parts.len(), "bars": rows, "output": args.output,
        "purpose": "CONDITIONAL_RECORDED_COST_INPUTS_NOT_HISTORICAL_FEE_ELIGIBILITY"}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn original_costs_keep_explicit_zero_and_reject_missing_or_nonfinite() {
        let row = |value| Row::new(vec![("fee_usdc".into(), value)]);
        assert_eq!(original(&row(Field::Double(0.0)), "fee_usdc").unwrap(), "0");
        assert_eq!(
            original(&row(Field::Double(0.12345600000000001)), "fee_usdc").unwrap(),
            "0.12345600000000001"
        );
        for value in [
            Field::Null,
            Field::Double(f64::NAN),
            Field::Double(f64::INFINITY),
        ] {
            assert!(original(&row(value), "fee_usdc").is_err());
        }
    }
}
