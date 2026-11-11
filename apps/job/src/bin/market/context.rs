use super::*;
use parquet::record::Field;
use std::fs::File;

struct Output {
    writer: csv::Writer<File>,
    path: String,
    rows: u64,
    first: Option<u64>,
    last: Option<(u64, String, String)>,
}

fn price(field: &Field) -> Result<String> {
    let value = if matches!(field, Field::Null) {
        String::new()
    } else {
        records::scalar(field)?
    };
    job::ticks::economic::context_decimal(&value)?;
    Ok(value)
}

pub(super) fn run(
    manifest_path: &Path,
    instruments_path: &Path,
    research_revision: &str,
    output: &Path,
) -> Result<serde_json::Value> {
    let manifest: Manifest = json(manifest_path)?;
    let source: serde_json::Value = json(manifest_path)?;
    ensure!(
        manifest.cryptohftdata()
            && manifest.coverage.as_deref() == Some("OBSERVED_PUBLISHED_FILES")
            && !manifest.revision.trim().is_empty()
            && !research_revision.trim().is_empty(),
        "RECEIVED_CONTEXT_SOURCE_REQUIRED"
    );
    let definitions = job::catalog::currencies::read_instruments(instruments_path)?;
    let mut aliases: BTreeMap<String, InstrumentId> = json::<serde_json::Value>(instruments_path)?
        .get("symbol_ids")
        .map(|value| serde_json::from_value(value.clone()))
        .transpose()?
        .unwrap_or_default();
    let native_ids = definitions
        .iter()
        .map(Instrument::id)
        .collect::<BTreeSet<_>>();
    for definition in &definitions {
        if let Some(previous) = aliases.insert(definition.raw_symbol().to_string(), definition.id())
        {
            ensure!(previous == definition.id(), "NATIVE_SYMBOL_ID_CONFLICT");
        }
    }
    let symbols = source["selection"]["symbols"]
        .as_array()
        .context("CONTEXT_SYMBOL_SELECTION_REQUIRED")?;
    let mut ids = BTreeSet::new();
    for symbol in symbols {
        let id = aliases
            .get(symbol.as_str().context("CONTEXT_SYMBOL_INVALID")?)
            .context("CONTEXT_SYMBOL_UNMAPPED")?;
        ensure!(native_ids.contains(id), "CONTEXT_NATIVE_DEFINITION_MISSING");
        ids.insert(*id);
    }
    ensure!(
        !ids.is_empty() && !manifest.files.is_empty(),
        "CONTEXT_SELECTION_EMPTY"
    );
    let start = date_ns(manifest.start_date)?;
    let end = date_ns(manifest.end_date)?;
    ensure!(start < end, "CONTEXT_WINDOW_INVALID");
    let root = manifest.root(manifest_path)?;
    fs::create_dir(output).context("OUTPUT_MUST_BE_NEW")?;
    let catalog = output.join("catalog");
    fs::create_dir(&catalog)?;
    let mut outputs = BTreeMap::new();
    for (index, id) in ids.iter().enumerate() {
        let path = format!("context-prices-{index:03}.csv");
        let mut writer = csv::Writer::from_writer(
            fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(catalog.join(&path))?,
        );
        writer.write_record(["event_ns", "oracle_px", "mark_px"])?;
        outputs.insert(
            *id,
            Output {
                writer,
                path,
                rows: 0,
                first: None,
                last: None,
            },
        );
    }
    let mut files = manifest.files.iter().collect::<Vec<_>>();
    files.sort_by(|a, b| a.path.cmp(&b.path));
    for file in files {
        let stream = manifest.stream(file.stream.as_deref().context("SOURCE_STREAM_MISSING")?)?;
        ensure!(
            stream.kind == "context"
                && stream
                    .columns
                    .get("available_at")
                    .is_some_and(|name| name == "received_time")
                && stream.available_time_unit.as_deref() == Some("ns"),
            "RECEIVED_CONTEXT_CLOCK_REQUIRED"
        );
        let path = source_file(&root, file)?;
        let mut rows = BTreeMap::new();
        records::read_rows(&path, &stream.format, |row| {
            let at = records::timestamp(stream.field(&row, "available_at")?, "ns")?;
            if at < start || at >= end || !stream.matches(&row)? {
                return Ok(());
            }
            let symbol = records::scalar(stream.field(&row, "symbol")?)?;
            let id = *aliases.get(&symbol).context("CONTEXT_SYMBOL_UNMAPPED")?;
            ensure!(
                outputs.contains_key(&id),
                "CONTEXT_SYMBOL_OUTSIDE_SELECTION"
            );
            let prices = (
                price(stream.field(&row, "index_price")?)?,
                price(stream.field(&row, "mark_price")?)?,
            );
            if let Some(previous) = rows.insert((id, at), prices.clone()) {
                ensure!(previous == prices, "CONTEXT_RECEIPT_CONFLICT");
            }
            Ok(())
        })?;
        for ((id, at), (oracle, mark)) in rows {
            let target = outputs.get_mut(&id).unwrap();
            if let Some((previous, previous_oracle, previous_mark)) = &target.last {
                ensure!(at >= *previous, "CONTEXT_RECEIPT_ORDER_INVALID");
                if at == *previous {
                    ensure!(
                        oracle == *previous_oracle && mark == *previous_mark,
                        "CONTEXT_RECEIPT_CONFLICT"
                    );
                    continue;
                }
            }
            target
                .writer
                .write_record([at.to_string(), oracle.clone(), mark.clone()])?;
            target.rows += 1;
            target.first.get_or_insert(at);
            target.last = Some((at, oracle, mark));
        }
    }
    let mut inventory = Vec::new();
    let mut total = 0;
    for (id, mut target) in outputs {
        target.writer.flush()?;
        total += target.rows;
        inventory.push(serde_json::json!({"instrument_id":id.to_string(), "path":target.path,
            "bytes":fs::metadata(catalog.join(&target.path))?.len().to_string(), "rows":target.rows.to_string(),
            "first_event_ns":target.first.map(|at|at.to_string()), "last_event_ns":target.last.map(|(at,_,_)|at.to_string())}));
    }
    ensure!(total > 0, "CONTEXT_SELECTION_EMPTY");
    publish(
        &catalog.join("context-prices.manifest.json"),
        &serde_json::json!({
            "schema_version":1, "format":"RECEIVED_TIME_CONTEXT_PRICE_CSV_V1", "clock":"RECEIVED_TIME_OBSERVED",
            "source_revision":manifest.revision, "research_source_revision":research_revision,
            "start_ns":start.to_string(), "end_ns":end.to_string(), "rows":total.to_string(),
            "instrument_ids":ids.iter().map(ToString::to_string).collect::<Vec<_>>(), "files":inventory,
            "missing":manifest.missing, "source_reference":manifest.source_reference,
            "source_columns":{"event_ns":"received_time", "oracle_px":"index_price", "mark_px":"mark_price"},
        }),
    )?;
    let report = serde_json::json!({"status":"IMPORTED", "readback_records":total, "clock":"RECEIVED_TIME_OBSERVED",
        "source_revision":manifest.revision, "research_source_revision":research_revision,
        "missing_symbols":{}, "rejected_rows":{}, "missing_partitions":manifest.missing,
        "limitations":["PROVIDER_RECEIPT_NOT_LOCAL_NETWORK_LATENCY", "CONTEXT_RATE_SAMPLES_ARE_NOT_SETTLEMENT_HISTORY"]});
    publish(&output.join("import-report.json"), &report)?;
    Ok(report)
}
