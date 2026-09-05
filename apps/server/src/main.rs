use clap::{Args, Parser, Subcommand};
use contracts::Id;
use integrations::{
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
    },
    /// Export native-generated HTTP contracts to stdout without connecting to a DB.
    Openapi,
}
#[derive(Args)]
struct Database {
    #[arg(long, env = "DATABASE_URL", hide_env_values = true)]
    database_url: String,
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
        .with_env_filter("server=info")
        .json()
        .init();
    if let Err(error) = execute(cli.command).await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn execute(command: Command) -> Result<(), Box<dyn std::error::Error>> {
    match command {
        Command::InitState { state_dir } => initialize_state(&state_dir)?,
        Command::Openapi => print!("{}", server::openapi_json()?),
        Command::Migrate {
            database,
            application_role,
        } => {
            let store = Store::connect(&database.database_url).await?;
            let pool = store.native_pool();
            let mut guard = pool.begin().await?;
            // Native transaction-scoped lock also serializes the third-party
            // session backend's own migration, without copying its DDL.
            tower_sessions_sqlx_store::sqlx::query("SELECT pg_advisory_xact_lock(63062026)")
                .execute(&mut *guard)
                .await?;
            store.migrate().await?;
            PostgresStore::new(pool.clone()).migrate().await?;
            if let Some(role) = application_role {
                let exists: bool = tower_sessions_sqlx_store::sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM pg_roles WHERE rolname=$1)",
                )
                .bind(&role)
                .fetch_one(&pool)
                .await?;
                if !exists {
                    return Err("application role does not exist; create it with native PostgreSQL administration first".into());
                }
                let quoted: String =
                    tower_sessions_sqlx_store::sqlx::query_scalar("SELECT quote_ident($1)")
                        .bind(role)
                        .fetch_one(&pool)
                        .await?;
                for statement in [format!("GRANT USAGE ON SCHEMA app,tower_sessions,pgmq TO {quoted}"),
                    format!("GRANT SELECT,INSERT,UPDATE ON ALL TABLES IN SCHEMA app TO {quoted}"),
                    format!("GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA tower_sessions,pgmq TO {quoted}"),
                    format!("GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA pgmq TO {quoted}")]{
                    tower_sessions_sqlx_store::sqlx::query(&statement).execute(&pool).await?;
                }
            }
            guard.commit().await?;
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
        Command::Serve {
            database,
            state_dir,
            bind,
            public_url,
            development_http,
        } => {
            let policy = WebPolicy::new(&public_url, bind, development_http)?;
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
            let app = server::router(AppState::new(store, vault, policy), key);
            let listener = tokio::net::TcpListener::bind(bind).await?;
            tracing::info!(address=%listener.local_addr()?,"authenticated HTTP API listening");
            let result = axum::serve(listener, app)
                .with_graceful_shutdown(async {
                    let _ = tokio::signal::ctrl_c().await;
                })
                .await;
            cleanup_task.abort();
            result?;
        }
    }
    Ok(())
}
