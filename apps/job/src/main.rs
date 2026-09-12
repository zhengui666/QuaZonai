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
    /// Apply the immutable native wall deadline before executing the fixed job entrypoint.
    RunBounded,
    /// Execute one typed native operation. Root overrides are trusted local CLI only.
    Execute {
        #[arg(long, default_value = "/input")]
        input_root: PathBuf,
        #[arg(long, default_value = "/output")]
        output_root: PathBuf,
    },
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
    /// Execute every independent native fold; stdout remains restricted evidence.
    ValidateAlpha {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        model: PathBuf,
    },
    /// Apply a frozen model without fitting; stdout is restricted held-out evidence.
    EvaluateSealedAlpha {
        #[arg(long)]
        catalog: PathBuf,
        #[arg(long)]
        model: PathBuf,
        #[arg(long)]
        calibration: Option<PathBuf>,
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

fn model_bytes(path: &Path, maximum_bytes: usize) -> Result<Vec<u8>> {
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > maximum_bytes as u64 {
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
    file.take(maximum_bytes as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > maximum_bytes {
        return Err("native model file limit".into());
    }
    Ok(bytes)
}

fn run(operation: Operation) -> Result<()> {
    match operation {
        Operation::RunBounded => {
            job::bounded::run()?;
            Ok(())
        }
        Operation::Execute {
            input_root,
            output_root,
        } => {
            job::managed::execute(&input_root, &output_root)?;
            Ok(())
        }
        Operation::Allocate => output(&job::allocate(&input()?)?),
        Operation::Forecast { catalog, model } => output(&job::forecast::forecast(
            &catalog,
            &input()?,
            &model_bytes(&model, job::signals::MAX_SIGNAL_MODULE_BYTES)?,
        )?),
        Operation::ValidateAlpha { catalog, model } => output(&job::validation::validate_alpha(
            &catalog,
            &input()?,
            &model_bytes(&model, job::signals::MAX_SIGNAL_MODULE_BYTES)?,
        )?),
        Operation::EvaluateSealedAlpha {
            catalog,
            model,
            calibration,
        } => {
            let calibration = calibration
                .map(
                    |p| -> Result<contracts::science::NativeFrozenCalibrationV1> {
                        Ok(serde_json::from_slice(&model_bytes(&p, 8 * 1024 * 1024)?)?)
                    },
                )
                .transpose()?;
            output(&job::validation::evaluate_sealed_alpha(
                &catalog,
                &input()?,
                &model_bytes(&model, job::signals::MAX_SIGNAL_MODULE_BYTES)?,
                calibration.as_ref(),
            )?)
        }
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
