mod agent_schema;
mod historical_export;
use clap::{Args, Parser, Subcommand};
use contracts::Id;
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use server::{client, AppState, WebPolicy};
use std::{
    fs,
    io::{Read, Write},
    net::SocketAddr,
    path::{Path, PathBuf},
};
use store::Store;
use tower_sessions::{cookie::Key, ExpiredDeletion};
use tower_sessions_sqlx_store::PostgresStore;

#[derive(Parser)]
#[command(
    name = "quazonai",
    bin_name = "quazonai",
    version,
    about = "QuaZonai trusted control-plane entrypoint"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Use the authenticated HTTP API with shared native contracts; never opens a database.
    Client(server::client::Arguments),
    /// Create a NEW private state directory and native encryption/session keys.
    InitState {
        #[arg(long, env = "STATE_DIR", default_value = "var")]
        state_dir: PathBuf,
    },
    /// Inspect counts and native foreign keys in a selected read-only old snapshot.
    InspectHistoricalSource {
        #[arg(long, env = "MIGRATION_SOURCE_DATABASE_URL", hide_env_values = true)]
        source_database_url: String,
        /// Create a new private report file; existing reports are never overwritten.
        #[arg(long)]
        output: PathBuf,
    },
    /// Copy explicitly reviewed public historical files into a NEW private export directory.
    ExportHistoricalArtifacts {
        #[arg(long)]
        source_root: PathBuf,
        #[arg(long)]
        selection: PathBuf,
        #[arg(long)]
        output: PathBuf,
    },
    /// Export reviewed old-schema column projections with PostgreSQL native COPY.
    ExportHistoricalRows {
        #[arg(long, env = "MIGRATION_SOURCE_DATABASE_URL", hide_env_values = true)]
        source_database_url: String,
        #[arg(long, value_parser = parse_id)]
        source_installation_id: Id,
        #[arg(long)]
        output: PathBuf,
    },
    /// Explicitly migrate the database; optional grants support a separate runtime role.
    Migrate {
        #[command(flatten)]
        database: Database,
        #[arg(long)]
        application_role: Option<String>,
    },
    /// Offline restore only: invalidate old sessions, devices, grants and machine credentials.
    RecoverAccess {
        #[command(flatten)]
        database: Database,
        /// Retain this ID when retrying after an uncertain result.
        #[arg(long,value_parser=parse_id)]
        recovery_id: contracts::Id,
    },
    /// Run the authenticated HTTP API. This command never runs database DDL.
    Serve {
        #[command(flatten)]
        database: Database,
        #[arg(long, env = "STATE_DIR", default_value = "var")]
        state_dir: PathBuf,
        #[arg(long, env = "BIND", default_value = "127.0.0.1:8080")]
        bind: SocketAddr,
        #[arg(long, env = "PUBLIC_URL")]
        public_url: String,
        /// Optional HTTP origin for native CLI access through a trusted-network proxy.
        #[arg(long, env = "CLI_HTTP_ORIGIN")]
        cli_http_origin: Option<String>,
        #[arg(long, env = "DEVELOPMENT_HTTP", default_value_t = false)]
        development_http: bool,
        /// Trusted Pi Durable service origin; never selected by an HTTP request.
        #[arg(
            long,
            env = "QZ_RESEARCH_ORIGIN",
            default_value = "http://127.0.0.1:8092"
        )]
        research_origin: String,
        /// Deployment-only origin/socket allowlist; no credentials or model-selected URLs.
        #[arg(
            long,
            env = "RUNTIME_TARGETS",
            default_value = "[]",
            hide_env_values = true
        )]
        runtime_targets: String,
        /// Independent deployment allowlist for target-only downstream probes.
        #[arg(
            long,
            env = "DOWNSTREAM_TARGETS",
            default_value = "[]",
            hide_env_values = true
        )]
        downstream_targets: String,
        /// Optional exact native Paper owner bindings. No credentials or HTTP-selected keys.
        #[arg(
            long,
            env = "PAPER_CAPITAL_EXIT_OWNERS",
            default_value = "[]",
            hide_env_values = true
        )]
        paper_capital_exit_owners: String,
        /// Deployment-only export references and absolute directories, frozen at startup.
        #[arg(
            long,
            env = "HISTORICAL_EXPORTS",
            default_value = "[]",
            hide_env_values = true
        )]
        historical_exports: String,
    },
    /// Drive registered native jobs through PGMQ and their exact Runtime identities.
    Worker {
        #[command(flatten)]
        database: Database,
        #[arg(long, env = "STATE_DIR", default_value = "var")]
        state_dir: PathBuf,
        #[arg(
            long,
            env = "RUNTIME_TARGETS",
            default_value = "[]",
            hide_env_values = true
        )]
        runtime_targets: String,
        #[arg(
            long,
            env = "DOWNSTREAM_TARGETS",
            default_value = "[]",
            hide_env_values = true
        )]
        downstream_targets: String,
        #[arg(long, env = "DEVELOPMENT_HTTP", default_value_t = false)]
        development_http: bool,
        #[arg(long, env = "WORKER_PARALLELISM")]
        parallelism: Option<usize>,
    },
    /// Reconcile only unreferenced machine verifiers; never removes credential history.
    PruneUnpublishedVerifiers {
        #[command(flatten)]
        database: Database,
        #[arg(long, env = "STATE_DIR", default_value = "var")]
        state_dir: PathBuf,
    },
    /// Export native-generated contracts offline (HTTP API by default).
    Openapi {
        /// Inspect domain DTOs, including scientific artifact content, instead of HTTP contracts.
        #[arg(long)]
        domain: bool,
        /// Select one native DTO and its complete schema dependency closure.
        #[arg(long)]
        schema: Option<String>,
        /// List installed native schema names without connecting to a service.
        #[arg(long, conflicts_with = "schema")]
        list_schemas: bool,
    },
}
#[derive(Args)]
struct Database {
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,
}

fn parse_id(value: &str) -> Result<Id, &'static str> {
    Id::try_from(value.to_owned()).map_err(|_| "expected a canonical UUIDv7")
}

fn parse_integration_targets(
    text: &str,
    development_http: bool,
) -> Result<server::runtime_transport::RuntimeTargets, &'static str> {
    let targets = serde_json::from_str::<Vec<server::runtime_transport::RuntimeTarget>>(text)
        .map_err(|_| "invalid integration targets deployment configuration")?;
    server::runtime_transport::RuntimeTargets::new(targets, development_http)
        .map_err(|_| "integration targets contain an unsafe or inconsistent endpoint")
}

async fn shutdown_signal() {
    #[cfg(unix)]
    {
        if let Ok(mut terminate) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            tokio::select! {
                _ = tokio::signal::ctrl_c() => {},
                _ = terminate.recv() => {},
            }
            return;
        }
    }
    let _ = tokio::signal::ctrl_c().await;
}

fn private_dir(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        fs::DirBuilder::new().mode(0o700).create(path)?;
    }
    #[cfg(not(unix))]
    {
        return Err("supported deployment requires Unix filesystem permissions".into());
    }
    Ok(())
}
fn initialize_state(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    private_dir(root)?;
    private_dir(&root.join("secrets"))?;
    ArtifactStore::open(&root.join("artifacts"))?;
    SecretVault::initialize_key(&root.join("master.key"))?;
    let vault = SecretVault::open(&root.join("secrets"), &root.join("master.key"))?;
    let key = Key::generate();
    let reference = vault.put("SESSION_KEY", key.master())?;
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(root.join("session-key.ref"))?;
    writeln!(file, "{reference}")?;
    file.sync_all()?;
    fs::File::open(root)?.sync_all()?;
    println!("Private state initialized. Back up the master key separately from the database and encrypted objects.");
    Ok(())
}
fn load_state(root: &Path) -> Result<(SecretVault, Key), Box<dyn std::error::Error>> {
    let vault = SecretVault::open(&root.join("secrets"), &root.join("master.key"))?;
    let file = fs::File::open(root.join("session-key.ref"))?;
    if !file.metadata()?.is_file() || file.metadata()?.len() > 80 {
        return Err("invalid session-key reference".into());
    }
    let mut text = String::new();
    file.take(81).read_to_string(&mut text)?;
    let reference =
        Id::try_from(text.trim().to_owned()).map_err(|_| "invalid session-key reference")?;
    let key = vault.read(reference, "SESSION_KEY")?;
    if key.len() != 64 {
        return Err("invalid native cookie key length".into());
    }
    Ok((vault, Key::from(&key)))
}

#[tokio::main]
async fn main() {
    let cli = Cli::parse();
    // Authentication arguments, headers and bodies are never logged.
    tracing_subscriber::fmt()
        .with_target(false)
        .with_env_filter("off,server=info")
        .with_writer(std::io::stderr)
        .json()
        .init();
    if let Err(error) = execute(cli.command).await {
        if let Some(error) = error.downcast_ref::<server::client::Failure>() {
            server::client::report(error);
        } else {
            eprintln!("{error}");
        }
        std::process::exit(1);
    }
}

async fn execute(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::Client(arguments) => server::client::run(arguments).await?,
        Command::InitState { state_dir } => initialize_state(&state_dir)?,
        Command::InspectHistoricalSource {
            source_database_url,
            output,
        } => {
            use std::os::unix::fs::OpenOptionsExt;
            let source = Store::connect(&source_database_url)
                .await
                .map_err(|_| std::io::Error::other("historical source connection failed"))?;
            let report = source.inspect_historical_source().await.map_err(|_| std::io::Error::other("historical source inspection failed; source must be a supported readable snapshot"))?;
            let bytes = serde_json::to_vec_pretty(&report)?;
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(output)?;
            file.write_all(&bytes)?;
            file.sync_all()?;
            println!("Historical source inspection saved; this is not an import or artifact-readability result.");
        }
        Command::ExportHistoricalArtifacts {
            source_root,
            selection,
            output,
        } => {
            historical_export::export(&source_root, &selection, &output)
                .map_err(|_| std::io::Error::other("historical artifact export failed; inspect the local source and new output directory"))?;
            println!("Historical artifact report saved. Review every outcome; selection coverage is not database coverage or import completion.");
        }
        Command::ExportHistoricalRows {
            source_database_url,
            source_installation_id,
            output,
        } => {
            let source = Store::connect(&source_database_url)
                .await
                .map_err(|_| std::io::Error::other("historical source connection failed"))?;
            historical_export::export_rows(&source, source_installation_id, &output)
                .await
                .map_err(|_| {
                    std::io::Error::other(
                        "historical row export failed; no complete report is available",
                    )
                })?;
            println!("Historical row projection report saved. Review missing tables and excluded fields; this is not import completion.");
        }
        Command::PruneUnpublishedVerifiers {
            database,
            state_dir,
        } => {
            let store = Store::connect(&database.database_url).await?;
            let (vault, _) = load_state(&state_dir)?;
            let removed =
                server::secrets::prune_unpublished_verifiers(&store, std::sync::Arc::new(vault))
                    .await?;
            println!(
                "{}",
                serde_json::json!({"schema_version":1,"removed_unpublished_verifiers":removed})
            );
        }
        Command::Openapi {
            domain,
            schema,
            list_schemas,
        } => {
            let document = if domain {
                contracts::openapi_json()?
            } else {
                server::openapi_json()?
            };
            if schema.is_none() && !list_schemas {
                print!("{document}");
            } else {
                println!("{}", agent_schema::describe(&document, schema.as_deref())?);
            }
        }
        Command::Migrate {
            database,
            application_role,
        } => {
            let store = Store::connect(&database.database_url).await?;
            store
                .migrate_with_application_role(application_role.as_deref())
                .await?;
            println!("Domain and native session migrations completed.");
        }
        Command::RecoverAccess {
            database,
            recovery_id,
        } => {
            let store = Store::connect(&database.database_url).await?;
            println!("{}", store.invalidate_restored_access(recovery_id).await?);
        }
        Command::Worker {
            database,
            state_dir,
            runtime_targets,
            downstream_targets,
            development_http,
            parallelism,
        } => {
            let targets = parse_integration_targets(&runtime_targets, development_http)?;
            let downstream_targets =
                parse_integration_targets(&downstream_targets, development_http)?;
            let store = Store::connect(&database.database_url).await?;
            store.authentication_snapshot().await?;
            let vault =
                SecretVault::open(&state_dir.join("secrets"), &state_dir.join("master.key"))?;
            let objects = ArtifactStore::open(&state_dir.join("artifacts"))?;
            let worker = server::worker::Worker::new(store, vault, objects, targets, parallelism)?
                .with_downstream_targets(downstream_targets);
            let (shutdown, observed) = tokio::sync::watch::channel(false);
            let run = worker.run(observed);
            tokio::pin!(run);
            tokio::select! {
                result = &mut run => result?,
                () = shutdown_signal() => {
                    let _ = shutdown.send(true);
                    run.await?;
                }
            }
        }
        Command::Serve {
            paper_capital_exit_owners,
            historical_exports,
            database,
            state_dir,
            bind,
            public_url,
            cli_http_origin,
            development_http,
            runtime_targets,
            downstream_targets,
            research_origin,
        } => {
            let research_service = server::durable::ResearchService::new(&research_origin)?;
            let paper_capital_exit_owners =
                server::paper_capital_exit::PaperCapitalExitOwners::parse(
                    &paper_capital_exit_owners,
                )?;
            let registrations = serde_json::from_str(&historical_exports)
                .map_err(|_| "invalid historical export registrations")?;
            let historical_exports = server::migrations::HistoricalExports::load(registrations)?;
            let policy = WebPolicy::new(&public_url, bind, development_http)?
                .with_cli_http_origin(cli_http_origin.as_deref())?;
            let targets = parse_integration_targets(&runtime_targets, development_http)?;
            let downstream_targets =
                parse_integration_targets(&downstream_targets, development_http)?;
            let (vault, key) = load_state(&state_dir)?;
            let store = Store::connect(&database.database_url).await?;
            store.authentication_snapshot().await?;
            let cleanup = PostgresStore::new(store.native_pool());
            let cleanup_task = tokio::spawn(async move {
                let mut interval = tokio::time::interval(std::time::Duration::from_secs(3600));
                loop {
                    interval.tick().await;
                    if cleanup.delete_expired().await.is_err() {
                        tracing::warn!("native session cleanup failed");
                    }
                }
            });
            let objects = ArtifactStore::open(&state_dir.join("artifacts"))?;
            let app = server::router(
                AppState::new(store, vault, policy)
                    .with_paper_capital_exit_owners(paper_capital_exit_owners)
                    .with_historical_exports(historical_exports)
                    .with_artifact_store(objects)
                    .with_historical_artifact_store(ArtifactStore::open(
                        &state_dir.join("historical-artifacts"),
                    )?)
                    .with_research_service(research_service)
                    .with_runtime_targets(targets)
                    .with_downstream_targets(downstream_targets),
                key,
            );
            let listener = tokio::net::TcpListener::bind(bind).await?;
            tracing::info!(address=%listener.local_addr()?,"authenticated HTTP API listening");
            let result = axum::serve(listener, app)
                .with_graceful_shutdown(shutdown_signal())
                .await;
            cleanup_task.abort();
            result?;
        }
    }
    Ok(())
}
