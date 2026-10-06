//! Portable client; the control-plane server keeps its separate Linux entrypoint.
#[path = "../../server/src/agent_schema.rs"]
mod agent_schema;
#[path = "../../server/src/client/mod.rs"]
mod client;
#[path = "../../server/src/service_http.rs"]
mod service_http;

use clap::{Parser, Subcommand};

const OPENAPI: &str = include_str!("../../../contracts/generated/api-v2.openapi.json");

#[derive(Parser)]
#[command(name = "quazonai", bin_name = "quazonai", version = option_env!("QUAZONAI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION")), about = "QuaZonai HTTP client")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Use the authenticated HTTP API with shared native contracts.
    Client(Box<client::Arguments>),
    /// Inspect installed native contracts offline (HTTP API by default).
    Openapi {
        /// Inspect domain DTOs, including scientific artifact content, instead of HTTP contracts.
        #[arg(long)]
        domain: bool,
        #[arg(long)]
        schema: Option<String>,
        #[arg(long, conflicts_with = "schema")]
        list_schemas: bool,
    },
}

#[tokio::main]
async fn main() {
    if let Err(error) = run(Cli::parse()).await {
        client::report(&error);
        std::process::exit(1);
    }
}

async fn run(cli: Cli) -> client::Result<()> {
    match cli.command {
        Command::Client(arguments) => client::run(*arguments).await,
        Command::Openapi {
            domain,
            schema,
            list_schemas,
        } => {
            use std::{borrow::Cow, io::Write};
            let document = if domain {
                Cow::Owned(contracts::openapi_json().map_err(|_| client::Failure::Contract)?)
            } else {
                Cow::Borrowed(OPENAPI)
            };
            let mut output = std::io::stdout().lock();
            if schema.is_some() || list_schemas {
                serde_json::to_writer_pretty(
                    &mut output,
                    &agent_schema::describe(&document, schema.as_deref())?,
                )
                .map_err(|_| client::Failure::Output)?;
                writeln!(output).map_err(|_| client::Failure::Output)?;
            } else {
                output
                    .write_all(document.as_bytes())
                    .map_err(|_| client::Failure::Output)?;
            }
            output.flush().map_err(|_| client::Failure::Output)
        }
    }
}
