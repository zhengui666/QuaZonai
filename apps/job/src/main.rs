//! One-process native science entrypoints. No database or delivery authority.
use clap::{Parser, Subcommand};
use serde::{de::DeserializeOwned, Serialize};
use std::{
    error::Error,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};

#[derive(Parser)]
#[command(
    name = "job",
    version,
    about = "Bounded native research computations; no delivery authority"
)]
struct Arguments {
    #[command(subcommand)]
    command: Operation,
}

#[derive(Subcommand)]
enum Operation {
    /// Run native compatibility fixtures in a new private directory.
    VerifyNative {
        #[arg(long)]
        output: PathBuf,
    },
    /// Solve one frozen allocation request read from stdin.
    Allocate,
    /// Predict using one immutable native catalog and a bounded Wasm artifact.
    Forecast {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        model: PathBuf,
    },
    /// Replay frozen target weights in one native simulated account.
    Simulate {
        #[arg(long)]
        catalog: PathBuf,
    },
}

type Result<T> = std::result::Result<T, Box<dyn Error>>;

fn input<T: DeserializeOwned>() -> Result<T> {
    const MAX_INPUT_BYTES: u64 = 8 * 1024 * 1024;
    let mut bytes = Vec::new();
    std::io::stdin()
        .lock()
        .take(MAX_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > MAX_INPUT_BYTES {
        return Err("native job input limit".into());
    }
    Ok(serde_json::from_slice(&bytes)?)
}

fn output<T: Serialize>(value: &T) -> Result<()> {
    let mut stream = std::io::stdout().lock();
    serde_json::to_writer(&mut stream, value)?;
    writeln!(stream)?;
    stream.flush()?;
    Ok(())
}

fn model_bytes(path: &Path) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > job::signals::MAX_SIGNAL_MODULE_BYTES as u64 {
        return Err("native model file limit".into());
    }
    // Only the runtime's registered read-only model mount is passed here. These
    // local CLI arguments are not exposed as arbitrary HTTP/MCP filesystem reads.
    let mut options = fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(
            (rustix::fs::OFlags::NOFOLLOW | rustix::fs::OFlags::NONBLOCK).bits() as i32,
        );
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err("native model is not a file".into());
    }
    let mut bytes = Vec::new();
    file.take(job::signals::MAX_SIGNAL_MODULE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > job::signals::MAX_SIGNAL_MODULE_BYTES {
        return Err("native model file limit".into());
    }
    Ok(bytes)
}

fn run(operation: Operation) -> Result<()> {
    match operation {
        Operation::Allocate => output(&job::allocate(&input()?)?),
        Operation::Forecast { catalog, model } => output(&job::forecast::forecast(
            &catalog,
            &input()?,
            &model_bytes(&model)?,
        )?),
        Operation::Simulate { catalog } => output(&job::simulation::simulate(&catalog, &input()?)?),
        Operation::VerifyNative { output: directory } => {
            let mut builder = fs::DirBuilder::new();
            #[cfg(unix)]
            {
                use std::os::unix::fs::DirBuilderExt;
                builder.mode(0o700);
            }
            builder.create(&directory)?;
            let report = job::probe(&directory)?;
            job::write_probe_report(&directory, "native-probe.json", &report)?;
            // Publication already committed; a closed output pipe does not undo it.
            let _ = writeln!(
                std::io::stdout(),
                "native compatibility probe completed; origin=FIXTURE; deliverable=false"
            );
            Ok(())
        }
    }
}

fn main() {
    let args = Arguments::parse();
    if run(args.command).is_err() {
        // No upstream tracebacks, host paths, input contents or secrets on this channel.
        eprintln!("QZ_NATIVE_JOB_FAILED");
        std::process::exit(1);
    }
}
