//! A one-process native compatibility probe for Issue #62 W0.
use std::error::Error;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 || args[1] != "verify-native" || args[2] != "--output" {
        return Err("usage: qz-job verify-native --output NEW_DIRECTORY".into());
    }
    let directory = Path::new(&args[3]);
    // Refuse pre-existing paths and accidental overwrites. This local operator
    // command is not an Agent-controlled path or a published artifact service.
    fs::create_dir(directory)?;
    let report = qz_job::probe(directory)?;
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("native-probe.json"))?;
    serde_json::to_writer_pretty(&mut file, &report)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    println!("native compatibility probe completed; origin=FIXTURE; deliverable=false");
    Ok(())
}

fn main() {
    if run().is_err() {
        // Never copy an upstream exception/traceback into public job output.
        eprintln!("QZ_NATIVE_PROBE_FAILED (see the last completed probe stage)");
        std::process::exit(1);
    }
}
