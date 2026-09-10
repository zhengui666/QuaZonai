//! Native Runtime service and diagnostics. No environment-sourced broker, model or DB credentials.
use clap::{Parser, Subcommand};
use runtime::{
    config::{credential, RuntimeConfig},
    http::RuntimeApi,
    supervisor::RuntimeService,
    Failure, Result,
};
use std::{io::Write, path::PathBuf, sync::Arc};
use tokio::{net::TcpListener, sync::watch};
use utoipa::OpenApi;

#[derive(Parser)]
#[command(
    name = "runtime",
    version,
    about = "Durable isolated native jobs; no trading or qualification authority"
)]
struct Arguments {
    #[command(subcommand)]
    command: Operation,
}

#[derive(Subcommand)]
enum Operation {
    /// Listen only on the configured loopback address behind the deployment TLS proxy.
    Serve {
        #[arg(long)]
        config: PathBuf,
    },
    /// Export the exact Rust-owned Runtime OpenAPI without reading configuration or secrets.
    Openapi,
    /// Inspect the native engine and pinned images. Does not submit work or change the journal.
    Doctor {
        #[arg(long)]
        config: PathBuf,
    },
}

fn output(value: &impl serde::Serialize) -> Result<()> {
    let mut stream = std::io::stdout().lock();
    serde_json::to_writer(&mut stream, value)?;
    writeln!(stream)?;
    stream.flush()?;
    Ok(())
}

async fn signal() {
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        .expect("native SIGTERM registration");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = terminate.recv() => {}
    }
}

async fn serve(config: RuntimeConfig) -> Result<()> {
    let secret = credential(&config.credential_file)?;
    let bind = config.bind;
    let service = RuntimeService::open(config).await?;
    let listener = TcpListener::bind(bind).await?;
    let app = runtime::http::router(service.clone(), secret)?;
    let (shutdown, receive) = watch::channel(false);
    let mut http_shutdown = receive.clone();
    let mut worker = tokio::spawn(service.clone().run(receive));
    let mut server = tokio::spawn(async move {
        axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                while !*http_shutdown.borrow() {
                    if http_shutdown.changed().await.is_err() {
                        break;
                    }
                }
            })
            .await
    });
    enum End {
        Signal,
        Worker(std::result::Result<Result<()>, tokio::task::JoinError>),
        Http(std::result::Result<std::io::Result<()>, tokio::task::JoinError>),
    }
    tracing::info!(
        "native runtime listener started; readiness requires an actual capability probe"
    );
    let end = tokio::select! {
        _ = signal() => End::Signal,
        result = &mut worker => End::Worker(result),
        result = &mut server => End::Http(result),
    };
    let _ = shutdown.send(true);
    let result = match end {
        End::Signal => {
            server.await.map_err(|_| Failure::Integrity)??;
            worker.await.map_err(|_| Failure::Integrity)?
        }
        End::Worker(result) => {
            server.await.map_err(|_| Failure::Integrity)??;
            result.map_err(|_| Failure::Integrity)?
        }
        End::Http(result) => {
            worker.await.map_err(|_| Failure::Integrity)??;
            result
                .map_err(|_| Failure::Integrity)?
                .map_err(Failure::from)
        }
    };
    service.journal().close().await;
    result
}

async fn run(operation: Operation) -> Result<()> {
    match operation {
        Operation::Openapi => output(&RuntimeApi::openapi()),
        Operation::Serve { config } => serve(RuntimeConfig::read(&config)?).await,
        Operation::Doctor { config } => {
            let config = Arc::new(RuntimeConfig::read(&config)?);
            let catalogs = config.validate()?;
            let engine = runtime::engine::NativeEngine::new(config)?;
            output(&engine.capabilities(&catalogs).await?)
        }
    }
}

#[tokio::main]
async fn main() {
    let args = Arguments::parse();
    tracing_subscriber::fmt()
        .json()
        .with_writer(std::io::stderr)
        .with_max_level(tracing::Level::INFO)
        .init();
    if run(args.command).await.is_err() {
        eprintln!("QZ_RUNTIME_COMMAND_FAILED");
        std::process::exit(1);
    }
}
