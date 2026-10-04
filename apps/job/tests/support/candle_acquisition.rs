//! Shared synthetic acquisition bytes for native import and full service acceptance.
//! Provider-shaped fixtures are never original market data or historical PIT evidence.
use nautilus_model::{
    identifiers::{InstrumentId, Symbol},
    instruments::{CurrencyPair, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use rust_decimal::Decimal;
use serde_json::{json, Value};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    str::FromStr,
};

pub const OBSERVED: &str = "2024-01-02T00:00:01Z";
pub const RECEIVED: u64 = 1_704_153_601_000_000_000;
pub const VALUES: [&str; 5] = ["42000.01", "42001.02", "41999.00", "42000.99", "0.10000001"];
const URL: &str = "https://api.exchange.coinbase.com/products/BTC-USD/candles?start=1970-01-01T00%3A00%3A00Z&end=1970-01-01T00%3A03%3A00Z&granularity=60";

pub fn file(path: &str, bytes: &[u8]) -> Value {
    json!({"path":path,"size":bytes.len()})
}
pub fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec(value).unwrap()).unwrap();
}

pub struct Acquisition {
    pub source: PathBuf,
    pub definitions: PathBuf,
    pub manifest: Value,
    pub instrument: InstrumentAny,
}

/// Write into the caller's already-owned fresh directory. All callers execute
/// the unchanged provider descriptor and native import/preparation executables.
pub fn write_acquisition(directory: &Path) -> Acquisition {
    let source = directory.join("acquired");
    fs::create_dir_all(source.join("raw")).unwrap();
    let instrument = InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(InstrumentId::from_str("BTC-USD.COINBASE").unwrap())
            .raw_symbol(Symbol::new("BTC-USD"))
            .base_currency(Currency::from_str("BTC").unwrap())
            .quote_currency(Currency::from_str("USD").unwrap())
            .price_precision(2)
            .size_precision(8)
            .price_increment(Price::from("0.01"))
            .size_increment(Quantity::from("0.00000001"))
            .maker_fee(Decimal::from_str("0.004").unwrap())
            .taker_fee(Decimal::from_str("0.006").unwrap())
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap(),
    );
    let definitions = directory.join("instruments.json");
    fs::write(
        &definitions,
        serde_json::to_vec(&vec![instrument.clone()]).unwrap(),
    )
    .unwrap();
    let terms = b"Synthetic provider terms for deterministic native tests only.";
    fs::write(source.join("source-terms.bin"), terms).unwrap();
    let rows = (0..3).map(|index| json!({
        "selection_time_seconds":index*60,"event_time_seconds":(index+1)*60,
        "open":VALUES[0],"high":VALUES[1],"low":VALUES[2],"close":VALUES[3],"volume":VALUES[4],
        "instrument":"BTC-USD","kind":"OHLCV_CANDLE","interval_seconds":60,
        "observed_at":OBSERVED,"historical_available_at":null,
        "source_response":"raw/0000.json","source_row_index":index
    })).collect::<Vec<_>>();
    let records = rows
        .iter()
        .map(|row| {
            serde_json::to_string(
                &row.as_object()
                    .unwrap()
                    .iter()
                    .collect::<std::collections::BTreeMap<_, _>>(),
            )
            .unwrap()
                + "\n"
        })
        .collect::<String>()
        .into_bytes();
    fs::write(source.join("records.jsonl"), &records).unwrap();
    let raw = format!(
        "[{}]",
        (0..3)
            .map(|index| format!(
                "[{},{},{},{},{},{}]",
                index * 60,
                VALUES[2],
                VALUES[1],
                VALUES[0],
                VALUES[3],
                VALUES[4]
            ))
            .collect::<Vec<_>>()
            .join(",")
    )
    .into_bytes();
    fs::write(source.join("raw/0000.json"), &raw).unwrap();
    // Read the unchanged Python descriptor so this fixture exercises the real
    // frozen acquisition contract rather than a parallel test-only schema.
    let script = "import json,providers; print(json.dumps(providers.PROVIDERS['coinbase-candles'].descriptor))";
    let response = Command::new("python3")
        .args(["-B", "-c", script])
        .env(
            "PYTHONPATH",
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtimes/data"),
        )
        .output()
        .unwrap();
    assert!(response.status.success());
    let provider: Value = serde_json::from_slice(&response.stdout).unwrap();
    let request = json!({"url":URL,"start_seconds":0,"end_seconds":180,"method":"GET"});
    let manifest = json!({
        "schema":"qz.public_acquisition/1", "provider":provider,
        "selection":{"instrument":"BTC-USD","start_seconds":0,"end_seconds":180,"interval_seconds":60},
        "selection_bounds":"[start_seconds,end_seconds)",
        "limits":{"max_response_bytes":4194304,"max_response_total_bytes":33554432,
            "max_requests":128,"max_records":100000,"max_output_bytes":134217728},
        "requests":[request], "admission":{"coverage":"UNPROVEN","historical_availability":"UNVERIFIED",
            "research_qualified":false,"registered_in_quazonai":false,"permission_status":"REQUIRES_INDEPENDENT_REVIEW"},
        "created_at":"2024-01-02T00:00:02Z",
        "source_terms":{"file":file("source-terms.bin",terms),"reference":"https://www.coinbase.com/legal/market_data",
            "evidence_status":"OPERATOR_SUPPLIED_NOT_INDEPENDENTLY_VERIFIED"},
        "responses":[{"request":request,"observation":{"status":200,"headers":{"Content-Type":"application/json"},
            "request_started_at":"2024-01-02T00:00:00Z","retrieved_at":OBSERVED},
            "file":file("raw/0000.json",&raw),"counts":{"source_rows":3,"selected_rows":3,"outside_request_window":0}}],
        "records":file("records.jsonl",&records),"record_count":3,"raw_response_bytes":raw.len(),"observation_status":"OBSERVED"
    });
    write_json(&source.join("acquisition.json"), &manifest);
    Acquisition {
        source,
        definitions,
        manifest,
        instrument,
    }
}

/// Original fixed candle clocks; the source-local identity is mapped at service admission.
pub fn selection() -> Value {
    json!({"dataset_revision_id":contracts::Id::new(),"settlements":[],"selection":{
        "schema_version":1,"bar_types":["BTC-USD.COINBASE-1-MINUTE-LAST-EXTERNAL"],
        "event_start_ns":"60000000000","event_end_ns":"240000000000",
        "decision_cutoff_ns":RECEIVED.to_string(),"maximum_rows":3}})
}

/// Claims are explicit fixture declarations; measured fields are never authored.
pub fn declaration() -> Value {
    json!({"schema_version":1,"registered_ref":"synthetic-candle-import",
        "native_snapshot_ref":"synthetic-candle-import-discovery","storage_version":"fixture-v1",
        "provider_kind":"NAUTILUS_CATALOG","data_kind":"BAR","partition":"DISCOVERY",
        "event_start":"1970-01-01T00:01:00Z","event_end":"1970-01-01T00:04:00Z",
        "available_through":OBSERVED,"origin":"FIXTURE","pit_status":"UNVERIFIED","revision_policy":"UNKNOWN",
        "provenance_reference":"SYNTHETIC_NATIVE_CANDLE_IMPORT_TEST",
        "availability_provenance":"Synthetic REST batch observed late; no historical publication evidence",
        "universe":{"name":"Synthetic BTC-USD fixture","calendar_ref":"synthetic-utc","calendar_version":"1",
            "selection_asof":"1970-01-01T00:00:00Z","has_historical_membership":false,
            "coverage_start":"1970-01-01T00:00:00Z","coverage_end":"2024-01-03T00:00:00Z",
            "membership":[{"instrument_id":"BTC-USD.COINBASE","valid_from":"1970-01-01T00:00:00Z",
                "valid_until":null,"available_at":"1970-01-01T00:00:00Z","groups":null}]}})
}
