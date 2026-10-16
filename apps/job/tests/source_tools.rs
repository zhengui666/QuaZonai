//! The installed dispatcher and standalone binaries must expose the same native CLIs.
#![cfg(all(feature = "catalog-prepare", feature = "polymarket-history"))]
use std::{ffi::OsStr, fs, process::Command};

use nautilus_model::{
    data::{Bar, BarType, Data},
    instruments::InstrumentAny,
    types::{Price, Quantity},
};
use nautilus_persistence::backend::catalog::ParquetDataCatalog;
use serde_json::{json, Value};

#[path = "support/polymarket.rs"]
mod prediction;

fn equivalent(name: &str, args: &[&OsStr], code: i32) {
    let binary = match name {
        "catalog-prepare" => env!("CARGO_BIN_EXE_catalog-prepare"),
        "polymarket-history" => env!("CARGO_BIN_EXE_polymarket-history"),
        _ => unreachable!(),
    };
    let standalone = Command::new(binary).args(args).output().unwrap();
    let selected = Command::new(env!("CARGO_BIN_EXE_source-tools"))
        .arg(name)
        .args(args)
        .output()
        .unwrap();
    assert_eq!(standalone.status.code(), Some(code), "{name} {args:?}");
    assert_eq!(selected.status, standalone.status, "{name} {args:?}");
    assert_eq!(selected.stdout, standalone.stdout, "{name} {args:?}");
    assert_eq!(selected.stderr, standalone.stderr, "{name} {args:?}");
}

#[test]
fn original_parsers_help_versions_errors_and_exit_codes_are_preserved() {
    for name in ["catalog-prepare", "polymarket-history"] {
        for flag in ["--help", "--version"] {
            equivalent(name, &[OsStr::new(flag)], 0);
        }
        equivalent(name, &[], 2);
        equivalent(name, &[OsStr::new("--not-an-option")], 2);
    }
    for sub in ["fetch", "import", "archive", "chain", "capture"] {
        equivalent(
            "polymarket-history",
            &[OsStr::new(sub), OsStr::new("--help")],
            0,
        );
        equivalent("polymarket-history", &[OsStr::new(sub)], 2);
    }
    for sub in ["ingest-candles", "ingest-archive-candles"] {
        equivalent(
            "catalog-prepare",
            &[OsStr::new(sub), OsStr::new("--help")],
            0,
        );
        equivalent("catalog-prepare", &[OsStr::new(sub)], 2);
        equivalent(
            "catalog-prepare",
            &[OsStr::new(sub), OsStr::new("--not-an-option")],
            2,
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let missing = directory
        .path()
        .join("missing [行情] ' ; $(no-command).json");
    let output = directory.path().join("output [行情]");
    equivalent(
        "polymarket-history",
        &[
            OsStr::new("import"),
            OsStr::new("--input"),
            missing.as_os_str(),
            OsStr::new("--output"),
            output.as_os_str(),
        ],
        1,
    );
    equivalent(
        "catalog-prepare",
        &[
            OsStr::new("--catalog"),
            missing.as_os_str(),
            OsStr::new("--declaration"),
            missing.as_os_str(),
            OsStr::new("--selection"),
            missing.as_os_str(),
            OsStr::new("--output"),
            output.as_os_str(),
        ],
        1,
    );
    for sub in ["ingest-candles", "ingest-archive-candles"] {
        equivalent(
            "catalog-prepare",
            &[
                OsStr::new(sub),
                OsStr::new("--acquisition"),
                missing.as_os_str(),
                OsStr::new("--instruments"),
                missing.as_os_str(),
                OsStr::new("--output"),
                output.as_os_str(),
            ],
            1,
        );
    }
    assert!(!output.exists());
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_keep_the_original_failure_contract() {
    use std::os::unix::ffi::OsStringExt;
    let path = std::ffi::OsString::from_vec(b"/missing-native-path-\xff".to_vec());
    equivalent(
        "polymarket-history",
        &[
            OsStr::new("import"),
            OsStr::new("--input"),
            &path,
            OsStr::new("--output"),
            &path,
        ],
        1,
    );
    for sub in ["ingest-candles", "ingest-archive-candles"] {
        equivalent(
            "catalog-prepare",
            &[
                OsStr::new(sub),
                OsStr::new("--acquisition"),
                &path,
                OsStr::new("--instruments"),
                &path,
                OsStr::new("--output"),
                &path,
            ],
            1,
        );
    }
}

#[test]
fn unknown_dispatch_does_not_enter_an_operator() {
    for args in [vec![], vec!["unknown"], vec!["--help"]] {
        let result = Command::new(env!("CARGO_BIN_EXE_source-tools"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        assert_eq!(
            result.stderr,
            b"source-tools: expected catalog-prepare or polymarket-history\n"
        );
    }
}

#[test]
fn history_dispatch_writes_original_native_bars_and_unqualified_evidence() {
    let directory = tempfile::tempdir().unwrap();
    let input = directory.path().join("synthetic history [行情].json");
    let output = directory.path().join("native output [行情]");
    let instruments = prediction::instruments("0.0", 600 * prediction::STEP);
    let instrument = instruments[0].clone();
    let bar_type: BarType = format!("{}-1-MINUTE-LAST-EXTERNAL", prediction::IDS[0])
        .parse()
        .unwrap();
    let price = Price::from("0.4200");
    let bar = Bar::new_checked(
        bar_type,
        price,
        price,
        price,
        price,
        Quantity::from("2.000000"),
        prediction::STEP.into(),
        (prediction::STEP + 1).into(),
    )
    .unwrap();
    let source = json!({
        "schema_version": 1, "source_reference": "FIXTURE: offline dispatcher import",
        "source_observed_at": "2024-01-01T00:00:00Z", "source_metadata": {"origin": "FIXTURE"},
        "instruments": [instrument], "bars": [bar]
    });
    fs::write(&input, serde_json::to_vec(&source).unwrap()).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_source-tools"))
        .args(["polymarket-history", "import", "--input"])
        .arg(&input)
        .arg("--output")
        .arg(&output)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(report["bars"], 1);
    assert_eq!(report["coverage"], "UNPROVEN");
    assert_eq!(report["historical_availability"], "UNVERIFIED");
    assert_eq!(report["registered_in_quazonai"], false);
    let evidence: Value =
        serde_json::from_slice(&fs::read(output.join("source-evidence.json")).unwrap()).unwrap();
    assert_eq!(evidence["instruments"], source["instruments"]);
    assert_eq!(evidence["bars"], source["bars"]);
    let mut native = ParquetDataCatalog::from_uri(
        output.join("catalog").to_str().unwrap(),
        None,
        Some(2),
        None,
        None,
    )
    .unwrap();
    let definitions: Vec<InstrumentAny> = native.instruments(None, None, None).unwrap();
    assert_eq!(
        serde_json::to_value(definitions).unwrap(),
        source["instruments"]
    );
    let rows = native
        .query::<Bar>(None, None, None, None, None, true)
        .unwrap()
        .map(|r| r.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(rows.len(), 1);
    assert!(matches!(rows[0], Data::Bar(actual) if actual == bar));
}
