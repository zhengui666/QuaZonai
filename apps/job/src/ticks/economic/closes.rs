//! Official instrument-close records and their original public settlement provenance.
use anyhow::{ensure, Context, Result};
use contracts::{ticks::NativeTradeTickFactorEvaluationRequestV1, DbCounter};
use nautilus_model::{
    data::{DataBatch, InstrumentClose, NautilusDataType},
    enums::InstrumentCloseType,
    identifiers::InstrumentId,
    types::Currency,
};
use nautilus_persistence::{
    backend::parquet::catalog::ParquetDataCatalog, catalog::types::CatalogQuery,
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path},
    str::FromStr,
};

pub const MANIFEST: &str = "instrument-closes.manifest.json";
#[derive(Deserialize, Serialize)]
pub struct CloseFile {
    pub path: String,
    pub bytes: DbCounter,
    pub rows: DbCounter,
}
#[derive(Deserialize, Serialize)]
pub struct Manifest {
    pub schema_version: u32,
    pub format: String,
    pub source_revision: String,
    pub research_source_revision: String,
    pub event_clock: String,
    pub availability: String,
    pub scheduling: String,
    pub start_ns: DbCounter,
    pub end_ns: DbCounter,
    pub rows: DbCounter,
    pub events: Vec<Value>,
    pub source: Value,
    pub files: Vec<CloseFile>,
}
pub struct Close {
    pub instrument_id: InstrumentId,
    pub at: u64,
    pub data: Option<InstrumentClose>,
    pub fee_rate: Option<Decimal>,
    pub fee_currency: Option<Currency>,
}
fn event(value: &Value) -> Result<Close> {
    ensure!(value["type"] == "SETTLEMENT", "INSTRUMENT_CLOSE_EVENT_TYPE");
    let instrument_id = value["instrument_id"]
        .as_str()
        .context("INSTRUMENT_CLOSE_ID")?
        .parse()?;
    let at: DbCounter = serde_json::from_value(value["event_ns"].clone())?;
    let price = value["price"]
        .as_str()
        .map(|price| {
            crate::source_market::price_with_precision(
                &parquet::record::Field::Str(price.into()),
                nautilus_model::types::fixed::FIXED_PRECISION,
            )
        })
        .transpose()?;
    let fee_rate = value["fee_rate"]
        .as_str()
        .map(Decimal::from_str)
        .transpose()?;
    let fee_currency = value["fee_currency"]
        .as_str()
        .and_then(Currency::try_from_str);
    let data = match (price, fee_rate, fee_currency) {
        (Some(price), Some(_), Some(_)) => {
            ensure!(price.as_decimal() > Decimal::ZERO, "INSTRUMENT_CLOSE_PRICE");
            Some(InstrumentClose::new(
                instrument_id,
                price,
                InstrumentCloseType::ContractExpired,
                at.get().into(),
                at.get().into(),
            ))
        }
        _ => None,
    };
    Ok(Close {
        instrument_id,
        at: at.get(),
        data,
        fee_rate,
        fee_currency,
    })
}
pub fn manifest(
    root: &Path,
    request: &NativeTradeTickFactorEvaluationRequestV1,
) -> Result<Option<Manifest>> {
    if !root.join(MANIFEST).try_exists()? {
        return Ok(None);
    }
    let m: Manifest = serde_json::from_reader(fs::File::open(root.join(MANIFEST))?)?;
    ensure!(
        m.schema_version == 1
            && m.format == "NAUTILUS_INSTRUMENT_CLOSE_V1"
            && !m.source_revision.trim().is_empty()
            && m.research_source_revision == request.input.source_revision
            && m.event_clock == "EVENT_TIME_OBSERVED"
            && m.availability == "HISTORICAL_COLLECTOR_AVAILABILITY_UNKNOWN"
            && m.scheduling == "EXCHANGE_EFFECTIVE_TIME"
            && m.start_ns <= request.input.event_start_ns
            && m.end_ns >= request.input.event_end_ns,
        "INSTRUMENT_CLOSE_SOURCE_MISMATCH"
    );
    for file in &m.files {
        ensure!(
            file.path.starts_with("data/instrument_closes/")
                && Path::new(&file.path)
                    .components()
                    .all(|c| matches!(c, Component::Normal(_))),
            "INSTRUMENT_CLOSE_PATH"
        );
        ensure!(
            fs::metadata(root.join(&file.path))?.len() == file.bytes.get(),
            "INSTRUMENT_CLOSE_FILE_SIZE"
        );
    }
    Ok(Some(m))
}
pub fn load(
    root: &Path,
    request: &NativeTradeTickFactorEvaluationRequestV1,
) -> Result<Option<(Vec<Close>, Manifest)>> {
    let Some(m) = manifest(root, request)? else {
        return Ok(None);
    };
    let catalog = ParquetDataCatalog::new(root, None, None, None, None);
    let mut records = BTreeMap::new();
    if m.rows.get() > 0 {
        let mut session = crate::catalog::file_batch_session(
            &catalog,
            CatalogQuery::new(NautilusDataType::InstrumentClose),
            None,
            4096,
        )?;
        while let Some(batch) = session.next_batch()? {
            let DataBatch::InstrumentClose(rows) = batch else {
                anyhow::bail!("INSTRUMENT_CLOSE_NATIVE_TYPE");
            };
            for row in rows.iter() {
                ensure!(
                    records
                        .insert((row.instrument_id, row.ts_init.as_u64()), *row)
                        .is_none(),
                    "INSTRUMENT_CLOSE_DUPLICATE"
                );
            }
        }
    }
    ensure!(
        records.len() as u64 == m.rows.get(),
        "INSTRUMENT_CLOSE_NATIVE_COUNT"
    );
    let mut events = Vec::new();
    for original in &m.events {
        let close = event(original)?;
        if let Some(expected) = close.data {
            let actual = records
                .remove(&(close.instrument_id, close.at))
                .context("INSTRUMENT_CLOSE_NATIVE_MISSING")?;
            ensure!(actual == expected, "INSTRUMENT_CLOSE_NATIVE_MISMATCH");
        }
        if close.at < request.input.event_end_ns.get()
            && request
                .input
                .instrument_ids
                .contains(&close.instrument_id.to_string())
        {
            events.push(close);
        }
    }
    ensure!(records.is_empty(), "INSTRUMENT_CLOSE_NATIVE_UNDECLARED");
    events.sort_by_key(|e| (e.at, e.instrument_id));
    Ok(Some((events, m)))
}

/// Convert original historical exchange events; scheduling does not claim collector receipt time.
pub fn import(source_path: &Path, instruments_path: &Path, output: &Path) -> Result<Value> {
    let source: Value = serde_json::from_reader(fs::File::open(source_path)?)?;
    let instruments: Value = serde_json::from_reader(fs::File::open(instruments_path)?)?;
    let instruments = crate::catalog::currencies::instruments_from_value(instruments)?;
    let events = source["events"]
        .as_array()
        .context("INSTRUMENT_CLOSE_SOURCE_EVENTS")?
        .clone();
    ensure!(
        source["event_clock"] == "EVENT_TIME_OBSERVED"
            && source["historical_received_ns"].is_null(),
        "INSTRUMENT_CLOSE_SOURCE_CLOCK"
    );
    let mut closes = Vec::new();
    for value in &events {
        let close = event(value)?;
        ensure!(
            instruments.iter().any(|i| {
                use nautilus_model::instruments::Instrument;
                i.id() == close.instrument_id
            }),
            "INSTRUMENT_CLOSE_DEFINITION"
        );
        if let Some(data) = close.data {
            closes.push(data);
        }
    }
    closes.sort_by_key(|close| (close.instrument_id, close.ts_init));
    fs::create_dir(output).context("OUTPUT_MUST_BE_NEW")?;
    let catalog = ParquetDataCatalog::new(output, None, None, None, None);
    for batch in closes.chunk_by(|a, b| {
        a.instrument_id == b.instrument_id && a.close_price.precision == b.close_price.precision
    }) {
        catalog.write_to_parquet(batch, None, None, None)?;
    }
    fn files(root: &Path, directory: &Path, result: &mut Vec<CloseFile>) -> Result<()> {
        if !directory.exists() {
            return Ok(());
        }
        for entry in fs::read_dir(directory)? {
            let path = entry?.path();
            if path.is_dir() {
                files(root, &path, result)?;
            } else {
                let reader = parquet::file::serialized_reader::SerializedFileReader::new(
                    fs::File::open(&path)?,
                )?;
                use parquet::file::reader::FileReader;
                result.push(CloseFile {
                    path: path
                        .strip_prefix(root)?
                        .to_str()
                        .context("INSTRUMENT_CLOSE_PATH")?
                        .into(),
                    bytes: DbCounter::new(path.metadata()?.len()).map_err(anyhow::Error::msg)?,
                    rows: DbCounter::new(reader.metadata().file_metadata().num_rows() as u64)
                        .map_err(anyhow::Error::msg)?,
                });
            }
        }
        Ok(())
    }
    let mut native_files = Vec::new();
    files(
        output,
        &output.join("data/instrument_closes"),
        &mut native_files,
    )?;
    native_files.sort_by(|a, b| a.path.cmp(&b.path));
    let m = Manifest {
        schema_version: 1,
        format: "NAUTILUS_INSTRUMENT_CLOSE_V1".into(),
        source_revision: source["source_revision"]
            .as_str()
            .context("INSTRUMENT_CLOSE_REVISION")?
            .into(),
        research_source_revision: source["research_source_revision"]
            .as_str()
            .context("INSTRUMENT_CLOSE_RESEARCH_REVISION")?
            .into(),
        event_clock: "EVENT_TIME_OBSERVED".into(),
        availability: "HISTORICAL_COLLECTOR_AVAILABILITY_UNKNOWN".into(),
        scheduling: "EXCHANGE_EFFECTIVE_TIME".into(),
        start_ns: serde_json::from_value(source["window"]["start_ns"].clone())?,
        end_ns: serde_json::from_value(source["window"]["end_ns"].clone())?,
        rows: DbCounter::new(closes.len() as u64).map_err(anyhow::Error::msg)?,
        events,
        source,
        files: native_files,
    };
    fs::write(output.join(MANIFEST), serde_json::to_vec_pretty(&m)?)?;
    Ok(
        serde_json::json!({"status":"IMPORTED","rows":m.rows,"source_revision":m.source_revision,"output":output}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_conversion_preserves_event_provenance_and_unknown_settlements() {
        use nautilus_model::instruments::{
            stubs::{currency_pair_btcusdt, currency_pair_ethusdt},
            Instrument, InstrumentAny,
        };
        let directory = tempfile::tempdir().unwrap();
        let instrument = InstrumentAny::CurrencyPair(currency_pair_btcusdt());
        let second = InstrumentAny::CurrencyPair(currency_pair_ethusdt());
        let source_path = directory.path().join("source.json");
        let definitions = directory.path().join("instruments.json");
        fs::write(
            &definitions,
            serde_json::to_vec(&vec![instrument.clone(), second.clone()]).unwrap(),
        )
        .unwrap();
        let source = serde_json::json!({"source_revision":"public-lifecycle", "research_source_revision":"fixed-source",
        "event_clock":"EVENT_TIME_OBSERVED", "historical_received_ns":null, "window":{"start_ns":"0","end_ns":"80"},
        "coverage":{"complete":false}, "events":[
            {"type":"SETTLEMENT","instrument_id":instrument.id().to_string(),"event_ns":"30","price":"0.026296","fee":"0","fee_rate":"0","fee_currency":"USDT","transaction_hash":"public-original"},
            {"type":"SETTLEMENT","instrument_id":instrument.id().to_string(),"event_ns":"40","price":null,"fee_rate":null,"fee_currency":"USDT"},
            {"type":"SETTLEMENT","instrument_id":second.id().to_string(),"event_ns":"50","price":"81.323","fee":"0","fee_rate":"0","fee_currency":"USDT"}
        ]});
        fs::write(&source_path, serde_json::to_vec(&source).unwrap()).unwrap();
        let output = directory.path().join("native");
        import(&source_path, &definitions, &output).unwrap();
        let mut request = crate::ticks::economic::tests::request();
        request.input.instrument_ids = vec![instrument.id().to_string(), second.id().to_string()];
        let manifest = manifest(&output, &request).unwrap().unwrap();
        assert_eq!(manifest.source, source);
        assert_eq!(manifest.rows.get(), 2);
        assert_eq!(manifest.files.len(), 2);
        let (events, _) = load(&output, &request).unwrap().unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(
            events[0].data.unwrap().close_price.as_decimal(),
            Decimal::from_str("0.026296").unwrap()
        );
        assert_eq!(events[0].data.unwrap().ts_init.as_u64(), 30);
        assert!(events[1].data.is_none());
        request.input.source_revision = "another-price-history".into();
        assert!(load(&output, &request).is_err());
    }
}
