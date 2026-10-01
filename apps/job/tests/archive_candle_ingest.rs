//! Synthetic frozen archive -> real native BAR/definition roundtrips, offline only.
#![cfg(feature = "catalog-prepare")]
use nautilus_model::{
    data::{Bar, Data},
    identifiers::{InstrumentId, Symbol},
    instruments::{CurrencyPair, InstrumentAny},
    types::{Currency, Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use rust_decimal::Decimal;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Output},
    str::FromStr,
};

const GENERATE: &str = r#"
import datetime,hashlib,io,json,pathlib,sys,zipfile
import binance_vision as b
root,day,interval,count,kind,precision,symbol=sys.argv[1:]
root=pathlib.Path(root); count=int(count)
s=b.Selection(symbol,symbol[:-4],'USDT',day,interval)
p=b.plan(s); width=p['interval_seconds']*10**9; scale=1000 if p['source_timestamp_unit']=='us' else 10**6
start=int(p['start_ns']); possible=86400//p['interval_seconds']
indices=range(possible) if count==-1 else range(count)
rows=[]
for i in indices:
    t=start+i*width
    rows.append(','.join([str(t//scale),'42000.01','42001.02','41999.00','42000.99','0.10000001',str((t+width-scale)//scale),'123456789012345678901234567890.0000000000000000001','4','0.01000001','0.123456789012345678901234567890','0']))
csv=('\r\n'.join(rows)+ ('\r\n' if rows else '')).encode()
stream=io.BytesIO()
with zipfile.ZipFile(stream,'w',compression=zipfile.ZIP_DEFLATED) as z:
    z.comment=b'harmless PK\x01\x02 PK\x06\x06 comment'; z.writestr(p['member_name'],csv)
a=stream.getvalue(); (root/'input.zip').write_bytes(a)
(root/'input.CHECKSUM').write_bytes((hashlib.sha256(a).hexdigest()+' *'+p['archive_name']+'\n').encode())
end=datetime.datetime.fromtimestamp(int(p['end_ns'])//10**9,datetime.timezone.utc).strftime('%Y-%m-%dT%H:%M:%S')
retrieval={'checksum':{'started_at':end+'Z','completed_at':end+'Z'},'archive':{'started_at':end+'Z','completed_at':end+precision+'Z'}}
prov={'kind':kind,'retrieval': None if kind=='UNKNOWN' or precision=='NULL' else retrieval}
(root/'provenance.json').write_text(json.dumps(prov))
(root/'terms.bin').write_bytes(b'Synthetic fixture evidence only')
b.freeze(s,root/'input.zip',root/'input.CHECKSUM',root/'source',provenance_path=root/'provenance.json',evidence_paths={'vision_terms':root/'terms.bin'})
"#;

fn hash(b: &[u8]) -> String {
    format!("{:x}", Sha256::digest(b))
}
fn write(path: &Path, v: &Value) {
    fs::write(path, serde_json::to_vec(v).unwrap()).unwrap();
}
struct Fixture {
    _dir: tempfile::TempDir,
    source: PathBuf,
    definitions: PathBuf,
    output: PathBuf,
    instrument: InstrumentAny,
}
impl Fixture {
    fn new(
        day: &str,
        interval: &str,
        count: i32,
        kind: &str,
        precision: &str,
        symbol: &str,
    ) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let r = Command::new("python3")
            .args(["-B", "-c", GENERATE])
            .arg(directory.path())
            .args([day, interval, &count.to_string(), kind, precision, symbol])
            .env(
                "PYTHONPATH",
                Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtimes/data"),
            )
            .output()
            .unwrap();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        let instrument = InstrumentAny::CurrencyPair(
            CurrencyPair::builder()
                .instrument_id(InstrumentId::from_str(&format!("{symbol}.BINANCE")).unwrap())
                .raw_symbol(Symbol::new(symbol))
                .base_currency(Currency::from_str(&symbol[..symbol.len() - 4]).unwrap())
                .quote_currency(Currency::from_str("USDT").unwrap())
                .price_precision(2)
                .size_precision(8)
                .price_increment(Price::from("0.01"))
                .size_increment(Quantity::from("0.00000001"))
                .maker_fee(Decimal::from_str("0.001").unwrap())
                .taker_fee(Decimal::from_str("0.001").unwrap())
                .ts_event(0_u64.into())
                .ts_init(0_u64.into())
                .build()
                .unwrap(),
        );
        let definitions = directory.path().join("instruments.json");
        write(&definitions, &json!([instrument]));
        Self {
            source: directory.path().join("source"),
            definitions,
            output: directory.path().join("native"),
            instrument,
            _dir: directory,
        }
    }
    fn standard() -> Self {
        Self::new("2025-01-01", "1m", 3, "SYNTHETIC", ".123456789", "BTCUSDT")
    }
    fn run(&self) -> Output {
        Command::new(env!("CARGO_BIN_EXE_catalog-prepare"))
            .args(["ingest-archive-candles", "--acquisition"])
            .arg(self.source.join("archive.json"))
            .arg("--instruments")
            .arg(&self.definitions)
            .arg("--output")
            .arg(&self.output)
            .output()
            .unwrap()
    }
    fn report(&self) -> Value {
        let r = self.run();
        assert!(r.status.success(), "{}", String::from_utf8_lossy(&r.stderr));
        serde_json::from_slice(&r.stdout).unwrap()
    }
    fn reject(&self) {
        let r = self.run();
        assert!(
            !r.status.success(),
            "invalid source passed: {}",
            String::from_utf8_lossy(&r.stdout)
        );
        assert!(!self.output.join("import-report.json").exists());
    }
    fn manifest(&self) -> Value {
        serde_json::from_slice(&fs::read(self.source.join("archive.json")).unwrap()).unwrap()
    }
    fn change_manifest(&self, change: impl FnOnce(&mut Value)) {
        let mut m = self.manifest();
        change(&mut m);
        write(&self.source.join("archive.json"), &m);
    }
    fn change_file(&self, path: &str, b: &[u8]) {
        fs::write(self.source.join(path), b).unwrap();
        self.change_manifest(|m| {
            m["files"][path] = json!({"path":path,"size":b.len(),"sha256":hash(b)})
        });
    }
    fn definitions(&self, change: impl FnOnce(&mut Value)) {
        let mut d = json!([self.instrument]);
        change(&mut d[0]["CurrencyPair"]);
        write(&self.definitions, &d);
    }
    fn bars(&self, kind: &str) -> Vec<Bar> {
        let mut native = ParquetDataCatalog::from_uri(
            self.output.join("catalog").to_str().unwrap(),
            None,
            Some(2),
            None,
            None,
        )
        .unwrap();
        native
            .query::<Bar>(Some(vec![kind.into()]), None, None, None, None, true)
            .unwrap()
            .map(|r| match r.unwrap() {
                Data::Bar(b) => b,
                _ => panic!("not BAR"),
            })
            .collect()
    }
}

#[test]
fn exact_native_roundtrip_retains_original_evidence_and_nine_digit_receipts() {
    let f = Fixture::standard();
    let original = fs::read(f.source.join("records.jsonl")).unwrap();
    let r = f.report();
    assert_eq!(r["bars"], 3);
    assert_eq!(r["source_schema"], "qz.public_archive_acquisition/1");
    assert_eq!(r["receipt_basis"]["ts_init_ns"], "1735776000123456789");
    assert_eq!(r["source_provenance_kind"], "SYNTHETIC");
    assert_eq!(r["historical_availability"], "UNVERIFIED");
    assert_eq!(r["research_qualified"], false);
    assert_eq!(r["registered_in_quazonai"], false);
    let kind = r["native_selection"]["bar_types"][0].as_str().unwrap();
    let bars = f.bars(kind);
    for (i, b) in bars.iter().enumerate() {
        assert_eq!(
            b.ts_event.as_u64(),
            1_735_689_600_000_000_000 + (i as u64 + 1) * 60_000_000_000
        );
        assert_eq!(b.ts_init.as_u64(), 1_735_776_000_123_456_789);
        assert_eq!(b.open, Price::from("42000.01"));
        assert_eq!(b.volume, Quantity::from("0.10000001"));
    }
    let selection = serde_json::from_value(r["native_selection"].clone()).unwrap();
    let loaded = job::catalog::load_catalog(&f.output.join("catalog"), &selection).unwrap();
    assert_eq!(loaded.series[0].bars, bars);
    assert_eq!(
        serde_json::to_value(&loaded.series[0].instrument).unwrap(),
        serde_json::to_value(&f.instrument).unwrap()
    );
    let evidence: Value =
        serde_json::from_slice(&fs::read(f.output.join("source-evidence.json")).unwrap()).unwrap();
    assert_eq!(evidence["acquisition"], f.manifest());
    assert_eq!(evidence["receipt_basis"], r["receipt_basis"]);
    assert_eq!(fs::read(f.source.join("records.jsonl")).unwrap(), original);
    for line in original.split(|b| *b == b'\n').filter(|l| !l.is_empty()) {
        let row: Value = serde_json::from_slice(line).unwrap();
        assert!(row["observed_at"].is_null());
        assert!(row["historical_available_at"].is_null());
    }
    let saved = fs::read(f.output.join("import-report.json")).unwrap();
    assert!(!f.run().status.success());
    assert_eq!(
        fs::read(f.output.join("import-report.json")).unwrap(),
        saved
    );
}
#[test]
fn full_sparse_and_transition_days_keep_source_clock_units() {
    for (day, interval, count, symbol) in [
        ("2024-12-31", "1m", -1, "BTCUSDT"),
        ("2025-01-01", "1d", 1, "ETHUSDT"),
        ("2024-02-29", "12h", 1, "ETHUSDT"),
    ] {
        let f = Fixture::new(
            day,
            interval,
            count,
            "OPERATOR_DECLARED",
            ".000000001",
            symbol,
        );
        let r = f.report();
        assert_eq!(r["bars"], if count == -1 { 1440 } else { count });
        assert_eq!(r["receipt_basis"]["kind"], "OPERATOR_DECLARED_UNVERIFIED");
        assert_eq!(r["source_provenance_kind"], "OPERATOR_DECLARED");
    }
}
#[test]
fn missing_unknown_and_empty_receipts_never_become_import_times() {
    for (kind, precision, count) in [
        ("UNKNOWN", ".1", 3),
        ("SYNTHETIC", "NULL", 3),
        ("OPERATOR_DECLARED", "NULL", 3),
        ("SYNTHETIC", ".1", 0),
    ] {
        Fixture::new("2025-01-01", "1m", count, kind, precision, "BTCUSDT").reject();
    }
    let f = Fixture::standard();
    f.change_manifest(|m| {
        m["files"]
            .as_object_mut()
            .unwrap()
            .remove("provenance.json");
    });
    f.reject();
}
#[test]
fn every_manifest_claim_and_file_hash_is_bound() {
    for case in 0..12 {
        let f = Fixture::standard();
        f.change_manifest(|m| match case {
            0 => m["provider"]["id"] = "coinbase-candles".into(),
            1 => m["selection"]["base_asset"] = "ETH".into(),
            2 => m["source_timestamp_unit"] = "ms".into(),
            3 => m["counts"]["rows"] = 3.into(),
            4 => m["counts"]["missing_bucket_open_ns"] = json!([]),
            5 => m["admission"]["research_qualified"] = true.into(),
            6 => m["files"]["records.jsonl"]["path"] = "../records.jsonl".into(),
            7 => m["files"]["evidence/unknown.bin"] = json!({}),
            8 => m["decoded_member"]["size"] = 0.into(),
            9 => {
                m["provenance"]["retrieval"]["archive"]["completed_at"] =
                    "2025-01-02T00:00:00Z".into()
            }
            10 => m["unrecognized"] = true.into(),
            _ => m["evidence"]["vision_terms"]["status"] = "VERIFIED".into(),
        });
        f.reject();
    }
}
#[test]
fn modified_raw_records_and_evidence_are_refused_even_with_updated_file_hash() {
    for name in [
        "raw/archive.zip",
        "raw/archive.CHECKSUM",
        "records.jsonl",
        "evidence/vision_terms.bin",
    ] {
        let f = Fixture::standard();
        if name == "evidence/vision_terms.bin" {
            fs::write(f.source.join(name), b"changed evidence").unwrap();
        } else {
            f.change_file(name, b"forged");
        }
        f.reject();
    }
    let f = Fixture::standard();
    let b = fs::read_to_string(f.source.join("records.jsonl"))
        .unwrap()
        .replace("42000.01", "42000.02");
    f.change_file("records.jsonl", b.as_bytes());
    f.reject();
}
#[test]
fn source_json_duplicate_members_are_never_overwritten() {
    for name in ["archive.json", "provenance.json"] {
        let f = Fixture::standard();
        let p = f.source.join(name);
        let b = fs::read_to_string(&p).unwrap();
        let changed = if name == "archive.json" {
            b.replacen('{', "{\"schema\":\"forged\",", 1)
        } else {
            b.replacen('{', "{\"kind\":\"UNKNOWN\",", 1)
        };
        if name == "archive.json" {
            fs::write(p, changed).unwrap();
        } else {
            f.change_file(name, changed.as_bytes());
        }
        f.reject();
    }
    let f = Fixture::standard();
    let b = fs::read_to_string(&f.definitions)
        .unwrap()
        .replace("\"maker_fee\":", "\"maker_fee\":\"0\",\"maker_fee\":");
    fs::write(&f.definitions, b).unwrap();
    f.reject();
}
#[test]
fn original_native_definitions_and_price_grids_fail_closed() {
    for case in 0..10 {
        let f = Fixture::standard();
        f.definitions(|d| match case {
            0 => {
                d.as_object_mut().unwrap().remove("maker_fee");
            }
            1 => d["raw_symbol"] = "ETHUSDT".into(),
            2 => d["id"] = "BTCUSDT.COINBASE".into(),
            3 => d["price_increment"] = "0.03".into(),
            4 => d["size_increment"] = "0.00000002".into(),
            5 => d["price_precision"] = 1.into(),
            6 => d["ts_init"] = json!(1_735_776_001_000_000_000_u64),
            7 => d["min_price"] = "50000.00".into(),
            8 => d["max_price"] = "40000.00".into(),
            _ => d["extra_default"] = "invented".into(),
        });
        f.reject();
    }
}
#[test]
fn declaration_clocks_cannot_reorder_day_import_or_publication() {
    for case in 0..7 {
        let f = Fixture::standard();
        let mut m = f.manifest();
        let mut p = m["provenance"].clone();
        match case {
            0 => p["retrieval"]["checksum"]["started_at"] = "2025-01-01T23:59:59Z".into(),
            1 => p["retrieval"]["archive"]["started_at"] = "2025-01-02T00:00:01Z".into(),
            2 => {
                p["retrieval"]["archive"]["completed_at"] = "2025-01-02T00:00:00.1234567890Z".into()
            }
            3 => p["retrieval"]["archive"]["completed_at"] = "2025-01-02T00:00:60Z".into(),
            4 => p["retrieval"]["archive"]["completed_at"] = "2025-01-02T00:00:00+00:00".into(),
            5 => m["imported_at"] = "2025-01-01T23:59:59Z".into(),
            _ => m["published_at"] = "2263-01-01T00:00:00Z".into(),
        }
        write(&f.source.join("archive.json"), &m);
        f.change_file("provenance.json", &serde_json::to_vec(&p).unwrap());
        f.change_manifest(|m| m["provenance"] = p);
        f.reject();
    }
}
#[cfg(unix)]
#[test]
fn symlinked_original_input_is_refused() {
    use std::os::unix::fs::symlink;
    let f = Fixture::standard();
    let path = f.source.join("raw/archive.zip");
    let target = f.source.join("kept.zip");
    fs::rename(&path, &target).unwrap();
    symlink(target, path).unwrap();
    f.reject();
}

#[test]
fn registry_calls_the_real_archive_converter_and_binds_the_source_report() {
    let f = Fixture::standard();
    let result = Command::new("python3")
        .arg("-B")
        .arg(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../runtimes/data/source_plugins.py"))
        .args(["convert", "binance-vision-spot-klines", "--native-bin"])
        .arg(env!("CARGO_BIN_EXE_catalog-prepare"))
        .arg("--acquisition")
        .arg(f.source.join("archive.json"))
        .arg("--instruments")
        .arg(&f.definitions)
        .arg("--output")
        .arg(&f.output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["status"], "NATIVE_ARTIFACTS_VALIDATED");
    assert_eq!(report["native_report"]["bars"], 3);
    assert_eq!(
        report["native_report"]["receipt_basis"]["ts_init_ns"],
        "1735776000123456789"
    );
    assert_eq!(report["admission"]["research_qualified"], false);
}

#[test]
fn preparation_keeps_microsecond_registration_constraint_without_rounding_native_receipts() {
    for (fraction, accepted) in [(".123456", true), (".123456789", false)] {
        let f = Fixture::new("2025-01-01", "1m", 3, "SYNTHETIC", fraction, "BTCUSDT");
        let imported = f.report();
        let root = f.source.parent().unwrap();
        let selection = json!({"dataset_revision_id":contracts::Id::new(),"settlements":[],"selection":imported["native_selection"]});
        let declaration = json!({"schema_version":1,"registered_ref":"synthetic-archive-import",
            "native_snapshot_ref":"synthetic-archive-discovery","storage_version":"fixture-v1",
            "provider_kind":"NAUTILUS_CATALOG","data_kind":"BAR","partition":"DISCOVERY",
            "event_start":"2025-01-01T00:01:00Z","event_end":"2025-01-01T00:04:00Z",
            "available_through":imported["receipt_basis"]["declared_observed_at"],"origin":"FIXTURE","pit_status":"UNVERIFIED","revision_policy":"UNKNOWN",
            "provenance_reference":"SYNTHETIC_ARCHIVE_IMPORT_TEST",
            "availability_provenance":"Explicit synthetic archive completion; no historical availability evidence",
            "universe":{"name":"Synthetic BTCUSDT fixture","calendar_ref":"synthetic-utc","calendar_version":"1",
                "selection_asof":"1970-01-01T00:00:00Z","has_historical_membership":false,
                "coverage_start":"1970-01-01T00:00:00Z","coverage_end":"2025-01-03T00:00:00Z",
                "membership":[{"instrument_id":"BTCUSDT.BINANCE","valid_from":"1970-01-01T00:00:00Z",
                    "valid_until":null,"available_at":"1970-01-01T00:00:00Z","groups":null}]}});
        let selection_path = root.join("selection.json");
        let declaration_path = root.join("declaration.json");
        write(&selection_path, &selection);
        write(&declaration_path, &declaration);
        let output = root.join("prepared");
        let result = Command::new(env!("CARGO_BIN_EXE_catalog-prepare"))
            .arg("--catalog")
            .arg(f.output.join("catalog"))
            .arg("--declaration")
            .arg(declaration_path)
            .arg("--selection")
            .arg(selection_path)
            .arg("--output")
            .arg(&output)
            .output()
            .unwrap();
        assert_eq!(
            result.status.success(),
            accepted,
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(output.join("catalog-metadata.json").is_file(), accepted);
        if accepted {
            let metadata: Value = serde_json::from_slice(&result.stdout).unwrap();
            assert_eq!(metadata["row_count"], "3");
            assert_eq!(metadata["origin"], "FIXTURE");
            assert_eq!(metadata["pit_status"], "UNVERIFIED");
        }
        assert_eq!(
            f.bars("BTCUSDT.BINANCE-1-MINUTE-LAST-EXTERNAL")[0]
                .ts_init
                .as_u64()
                .to_string(),
            imported["receipt_basis"]["ts_init_ns"].as_str().unwrap()
        );
    }
}

#[test]
fn original_tick_versions_survive_and_future_ties_or_fee_changes_refuse() {
    const RECEIVED: u64 = 1_735_776_000_123_456_789;
    for case in 0..4 {
        let f = Fixture::standard();
        let mut old = serde_json::to_value(&f.instrument).unwrap();
        old["CurrencyPair"]["price_increment"] = "0.05".into();
        let mut update = serde_json::to_value(&f.instrument).unwrap();
        let time = match case {
            1 => RECEIVED,
            2 => RECEIVED + 1,
            _ => RECEIVED - 1,
        };
        update["CurrencyPair"]["ts_event"] = time.into();
        update["CurrencyPair"]["ts_init"] = time.into();
        if case == 3 {
            update["CurrencyPair"]["maker_fee"] = "0.002".into();
        }
        let definitions = json!([old, update]);
        write(&f.definitions, &definitions);
        if case == 0 {
            let report = f.report();
            assert_eq!(report["instrument_versions"], 2);
            let evidence: Value =
                serde_json::from_slice(&fs::read(f.output.join("source-evidence.json")).unwrap())
                    .unwrap();
            assert_eq!(evidence["instrument_definitions"], definitions);
            assert_eq!(report["native_readback_verified"], true);
        } else {
            f.reject();
        }
    }
}
