use clap::{Args, Parser, Subcommand};
use contracts::Id;
use integrations::{
    artifacts::ArtifactStore,
    authentication::{capability_verifier, random_capability},
    secrets::SecretVault,
};
use server::{AppState, WebPolicy};
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
    /// Serve native stdio MCP for one existing Mission; no DB or Operator authority.
    Mcp {
        #[arg(long)]
        api_origin: String,
        #[arg(long, value_parser = parse_id)]
        project_id: Id,
        #[arg(long, value_parser = parse_id)]
        cycle_id: Id,
        #[arg(long, value_parser = parse_id)]
        run_id: Id,
        #[arg(long, value_parser = parse_id)]
        attempt_id: Id,
        #[arg(long, value_parser = parse_id)]
        brief_id: Id,
        /// Trusted launcher-selected absolute worktree; never read from a tool request.
        #[arg(long)]
        workspace_root: Option<PathBuf>,
        #[arg(long, default_value_t = false)]
        development_http: bool,
    },
    /// Create a NEW private state directory and native encryption/session keys.
    InitState {
        #[arg(long, env = "STATE_DIR", default_value = "var")]
        state_dir: PathBuf,
    },
    /// Migrate a new database using a separate privileged migration identity.
    Migrate {
        #[command(flatten)]
        database: Database,
        #[arg(long)]
        application_role: Option<String>,
    },
    /// Issue a one-use, expiring local initialization capability. Never use remotely.
    Bootstrap {
        #[command(flatten)]
        database: Database,
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
        #[arg(long, env = "DEVELOPMENT_HTTP", default_value_t = false)]
        development_http: bool,
        /// Deployment-only origin/socket allowlist; no credentials or model-selected URLs.
        #[arg(
            long,
            env = "RUNTIME_TARGETS",
            default_value = "[]",
            hide_env_values = true
        )]
        runtime_targets: String,
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
        #[arg(long, env = "DEVELOPMENT_HTTP", default_value_t = false)]
        development_http: bool,
        #[arg(long, env = "WORKER_PARALLELISM", default_value_t = 2)]
        parallelism: usize,
    },
    /// Reconcile only unreferenced machine verifiers; never removes credential history.
    PruneUnpublishedVerifiers {
        #[command(flatten)]
        database: Database,
        #[arg(long, env = "STATE_DIR", default_value = "var")]
        state_dir: PathBuf,
    },
    /// Export native-generated HTTP contracts to stdout without connecting to a DB.
    Openapi,
}
#[derive(Args)]
struct Database {
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,
}

fn parse_id(value: &str) -> Result<Id, &'static str> {
    Id::try_from(value.to_owned()).map_err(|_| "expected a canonical UUIDv7")
}

fn parse_runtime_targets(
    text: &str,
    development_http: bool,
) -> Result<server::runtime_transport::RuntimeTargets, &'static str> {
    if text.len() > 65536 {
        return Err("RUNTIME_TARGETS exceeds deployment configuration limit");
    }
    let targets = serde_json::from_str::<Vec<server::runtime_transport::RuntimeTarget>>(text)
        .map_err(|_| "invalid RUNTIME_TARGETS deployment configuration")?;
    server::runtime_transport::RuntimeTargets::new(targets, development_http)
        .map_err(|_| "RUNTIME_TARGETS contains an unsafe or inconsistent endpoint")
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
        Command::Mcp {
            api_origin,
            project_id,
            cycle_id,
            run_id,
            attempt_id,
            brief_id,
            workspace_root,
            development_http,
        } => {
            let token = std::env::var("QUAZONAI_MCP_TOKEN")
                .map_err(|_| server::mcp::Failure::Configuration)?;
            let binding = server::mcp::MissionBinding {
                project_id,
                cycle_id,
                run_id,
                attempt_id,
                brief_id,
            };
            let mcp =
                server::mcp::MissionMcp::connect(&api_origin, development_http, &token, binding)
                    .await?;
            drop(token);
            let mcp = match workspace_root {
                Some(root) => mcp.with_workspace(&root)?,
                None => mcp,
            };
            mcp.serve_io(tokio::io::stdin(), tokio::io::stdout())
                .await?;
        }
        Command::InitState { state_dir } => initialize_state(&state_dir)?,
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
        Command::Openapi => print!("{}", server::openapi_json()?),
        Command::Migrate {
            database,
            application_role,
        } => {
            let store = Store::connect(&database.database_url).await?;
            store
                .migrate_with_application_role(application_role.as_deref())
                .await?;
            println!("Domain and native session migrations completed. Run serve with the non-owner application identity.");
        }
        Command::Bootstrap { database } => {
            let store = Store::connect(&database.database_url).await?;
            let capability = random_capability();
            let verifier = capability_verifier(&capability)?;
            let issued = store.issue_bootstrap_capability(&verifier).await?;
            // This is the sole authorized display of the raw initialization
            // capability. It is not stored as a command receipt or in a log.
            println!(
                "{}",
                serde_json::json!({"schema_version":1,"capability_id":issued.id,"capability":capability,"expires_at":issued.expires_at})
            );
        }
        Command::Worker {
            database,
            state_dir,
            runtime_targets,
            development_http,
            parallelism,
        } => {
            let targets = parse_runtime_targets(&runtime_targets, development_http)?;
            let store = Store::connect(&database.database_url).await?;
            store.verify_runtime_role().await?;
            store.authentication_snapshot().await?;
            let vault =
                SecretVault::open(&state_dir.join("secrets"), &state_dir.join("master.key"))?;
            let objects = ArtifactStore::open(&state_dir.join("artifacts"))?;
            let worker = server::worker::Worker::new(store, vault, objects, targets, parallelism)?;
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
            database,
            state_dir,
            bind,
            public_url,
            development_http,
            runtime_targets,
        } => {
            let policy = WebPolicy::new(&public_url, bind, development_http)?;
            let targets = parse_runtime_targets(&runtime_targets, development_http)?;
            let (vault, key) = load_state(&state_dir)?;
            let store = Store::connect(&database.database_url).await?;
            store.verify_runtime_role().await?;
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
                    .with_artifact_store(objects)
                    .with_runtime_targets(targets),
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
