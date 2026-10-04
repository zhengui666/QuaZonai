//! Offline evaluator utility. Never starts a model or produces a LIVE report.
#[path = "agent_evaluation/controller.rs"]
mod controller;

use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(about = "Freeze/replay evaluator-owned PROTOCOL_ONLY evidence; no live attestation")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Freeze exact suite/source/scenario bytes into a new output directory.
    Freeze {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Replay public observation declarations against an externally retained lock digest.
    Replay {
        #[arg(long)]
        input: PathBuf,
        #[arg(long)]
        lock: PathBuf,
        #[arg(long)]
        lock_sha256: String,
        #[arg(long)]
        observations: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
}

fn main() {
    let cli = match Cli::try_parse() {
        Ok(cli) => cli,
        Err(error) => {
            if matches!(
                error.kind(),
                clap::error::ErrorKind::DisplayHelp | clap::error::ErrorKind::DisplayVersion
            ) {
                let _ = error.print();
                return;
            }
            eprintln!("Invalid evaluation arguments; use --help");
            std::process::exit(2);
        }
    };
    let result = match cli.command {
        Command::Freeze { input, output } => controller::freeze(&input, &output),
        Command::Replay {
            input,
            lock,
            lock_sha256,
            observations,
            output,
        } => controller::replay(&input, &lock, &lock_sha256, &observations, &output),
    };
    match result {
        Ok(summary) => println!("{summary}"),
        Err(error) => {
            // Error messages are fixed categories, never raw source/observation bytes.
            eprintln!("Invalid evaluation pack: {error}");
            std::process::exit(2);
        }
    }
}
