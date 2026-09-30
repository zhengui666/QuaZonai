//! Test-owned native acquisition -> plugin -> preparation for browser/Worker acceptance.
//! Never original market data, a permission certification or a live provider call.
#[path = "../tests/support/candle_acquisition.rs"]
mod acquisition;

use anyhow::{ensure, Context, Result};
use nautilus_model::types::{Price, Quantity};
use serde_json::{json, Value};
use std::{fs, path::PathBuf, process::Command};

fn main() -> Result<()> {
    let mut arguments = std::env::args_os().skip(1);
    let root = PathBuf::from(arguments.next().context("new private root required")?);
    let native = PathBuf::from(
        arguments
            .next()
            .context("actual catalog-prepare binary required")?,
    );
    ensure!(arguments.next().is_none() && root.is_absolute() && native.is_absolute());
    ensure!(native.is_file());
    // Fail on any existing root; the harness owns cleanup of an interrupted run.
    fs::create_dir(&root)?;
    let source = acquisition::write_acquisition(&root);
    ensure!(source.manifest["record_count"] == 3);
    ensure!(
        serde_json::to_value([&source.instrument])?
            == serde_json::from_slice::<Value>(&fs::read(&source.definitions)?)?
    );
    let imported = root.join("imported");
    let plugin =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../runtimes/data/source_plugins.py");
    let result = Command::new("python3")
        .arg("-B")
        .arg(plugin)
        .args(["convert", "coinbase-candles", "--native-bin"])
        .arg(&native)
        .arg("--acquisition")
        .arg(source.source.join("acquisition.json"))
        .arg("--instruments")
        .arg(&source.definitions)
        .arg("--output")
        .arg(&imported)
        .output()?;
    ensure!(result.status.success(), "native plugin conversion failed");
    let conversion: Value = serde_json::from_slice(&result.stdout)?;
    ensure!(conversion["status"] == "NATIVE_ARTIFACTS_VALIDATED");
    ensure!(conversion["native_report"]["bars"] == 3);
    ensure!(conversion["admission"]["research_qualified"] == false);
    let selection = acquisition::selection();
    let declaration = acquisition::declaration();
    acquisition::write_json(&root.join("selection.json"), &selection);
    acquisition::write_json(&root.join("declaration.json"), &declaration);
    let prepared = root.join("prepared");
    let result = Command::new(native)
        .arg("--catalog")
        .arg(imported.join("catalog"))
        .arg("--declaration")
        .arg(root.join("declaration.json"))
        .arg("--selection")
        .arg(root.join("selection.json"))
        .arg("--output")
        .arg(&prepared)
        .output()?;
    ensure!(result.status.success(), "native catalog preparation failed");
    let bytes = fs::read(prepared.join("catalog-metadata.json"))?;
    let metadata: Value = serde_json::from_slice(&bytes)?;
    ensure!(metadata == serde_json::from_slice::<Value>(&result.stdout)?);
    ensure!(
        metadata["row_count"] == "3"
            && metadata["origin"] == "FIXTURE"
            && metadata["pit_status"] == "UNVERIFIED"
            && metadata["available_through"] == acquisition::OBSERVED
    );
    let selected = serde_json::from_value::<contracts::execution::NativeDatasetSelectionV1>(
        selection.clone(),
    )?;
    let data = job::catalog::load_catalog(&prepared.join("catalog"), &selected.selection)?;
    ensure!(data.rows == 3 && data.series.len() == 1);
    ensure!(
        serde_json::to_value(&data.series[0].instrument)?
            == serde_json::to_value(&source.instrument)?
    );
    let mut readback = Vec::new();
    for (index, bar) in data.series[0].bars.iter().enumerate() {
        ensure!(bar.ts_event.as_u64() == (index as u64 + 1) * 60_000_000_000);
        ensure!(bar.ts_init.as_u64() == acquisition::RECEIVED);
        ensure!(
            bar.open == Price::from(acquisition::VALUES[0])
                && bar.high == Price::from(acquisition::VALUES[1])
                && bar.low == Price::from(acquisition::VALUES[2])
                && bar.close == Price::from(acquisition::VALUES[3])
                && bar.volume == Quantity::from(acquisition::VALUES[4])
        );
        readback.push(json!({"event_ns":bar.ts_event.as_u64().to_string(),
            "available_ns":bar.ts_init.as_u64().to_string(),"open":bar.open.to_string(),
            "high":bar.high.to_string(),"low":bar.low.to_string(),"close":bar.close.to_string(),
            "volume":bar.volume.to_string()}));
    }
    let report = json!({"schema_version":1,
        "scope":"synthetic acquisition through actual plugin and native preparation; no qualification",
        "catalog_root":prepared.join("catalog"),"metadata_file":prepared.join("catalog-metadata.json"),
        "metadata":metadata,"selection":selection,"conversion":conversion,"native_readback":readback,
        "metadata_sha256":acquisition::hash(&bytes),"original_receipt_ns":acquisition::RECEIVED.to_string()});
    // Publication last. The harness never treats a partially prepared root as ready.
    acquisition::write_json(&root.join("prepared.json"), &report);
    println!("native fixture catalog prepared without scientific qualification");
    Ok(())
}
