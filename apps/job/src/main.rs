//! A one-process native compatibility probe for Issue #62 W0.
use std::error::Error;
use std::fs;
use std::io::Write;
use std::path::Path;

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.len() != 4 || args[1] != "verify-native" || args[2] != "--output" {
        return Err("usage: job verify-native --output NEW_DIRECTORY".into());
    }
    let directory = Path::new(&args[3]);
    // Refuse pre-existing paths and accidental overwrites. This local operator
    // command is not an Agent-controlled path or a published artifact service.
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(directory)?;
    let report = job::probe(directory)?;
    job::write_probe_report(directory, "native-probe.json", &report)?;
    // Publication is the commit point; a closed output pipe must not turn an
    // already published report into a failed process result.
    let _ = writeln!(
        std::io::stdout(),
        "native compatibility probe completed; origin=FIXTURE; deliverable=false"
    );
    Ok(())
}

fn main() {
    if run().is_err() {
        // Never copy an upstream exception/traceback into public job output.
        eprintln!("QZ_NATIVE_PROBE_FAILED (see the last completed probe stage)");
        std::process::exit(1);
    }
}
