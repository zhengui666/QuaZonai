//! Explicit, interactive native-account acceptance. Never reads existing profiles or auth files.
use clap::Parser;
use server::codex_native::{Account, Client, Launch, LoginCancellationStatus, Observation};
use std::{collections::BTreeMap, io::IsTerminal, path::PathBuf, time::Duration};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Parser)]
#[command(about = "Protected native Codex login acceptance in a disposable profile")]
struct Arguments {
    #[arg(long)]
    codex_binary: PathBuf,
    /// Exercise real start/cancel and empty-account logout only; does NOT pass complete T07 acceptance.
    #[arg(long)]
    cancel_only: bool,
}

fn launch(binary: &std::path::Path, root: &std::path::Path) -> Launch {
    Launch {
        binary: binary.to_path_buf(),
        home: root.to_path_buf(),
        codex_home: root.to_path_buf(),
        working_directory: root.to_path_buf(),
        executable_path: std::env::var_os("PATH").unwrap_or_default(),
        native_environment: BTreeMap::new(),
        custom_provider: None,
    }
}

// Match the production account owner: an already-observed success wins cancellation.
async fn cancel_and_reconcile(client: &mut Client, login_id: &str) -> Result<bool> {
    let cancelled = client.cancel_login(login_id).await?;
    let observations = client.observations(Duration::ZERO).await?;
    if observations.iter().any(|value| matches!(value, Observation::LoginCompleted { login_id: id, success: true } if id == login_id)) {
        println!("phase=login_completed_before_cancel");
        return Ok(true);
    }
    match cancelled.status {
        LoginCancellationStatus::Canceled => Ok(false),
        LoginCancellationStatus::NotFound => Err("NATIVE_LOGIN_CANCELLATION_UNCONFIRMED".into()),
    }
}

async fn exercise(client: &mut Client, cancel_only: bool) -> Result<()> {
    if client.account().await?.account.is_some() {
        return Err("DISPOSABLE_PROFILE_ALREADY_AUTHENTICATED".into());
    }
    println!("phase=start_cancel");
    let login = client.device_login().await?;
    if cancel_and_reconcile(client, &login.login_id).await? {
        return Err("NATIVE_LOGIN_NOT_CANCELED".into());
    }
    if client.account().await?.account.is_some() {
        return Err("CANCELED_LOGIN_BECAME_AUTHENTICATED".into());
    }
    if cancel_only {
        return Ok(());
    }
    println!("phase=interactive_login");
    let login = client.device_login().await?;
    // main refuses redirected input/output in this mode. Never emit codes in batch logs.
    println!(
        "Open {} and enter {}",
        login.verification_url, login.user_code
    );
    let completed = tokio::time::timeout(Duration::from_secs(300), async {
        loop {
            for observation in client.observations(Duration::from_secs(1)).await? {
                if let Observation::LoginCompleted { login_id, success } = observation {
                    if login_id == login.login_id {
                        return Ok::<_, Box<dyn std::error::Error>>(success);
                    }
                }
            }
        }
    });
    let outcome: Result<bool> = tokio::select! {
        result = completed => result.map_err(|_| "INTERACTIVE_LOGIN_TIMED_OUT".into()).and_then(|value| value),
        _ = tokio::signal::ctrl_c() => Err("INTERACTIVE_LOGIN_INTERRUPTED".into()),
    };
    let success = match outcome {
        Ok(success) => success,
        Err(error) => {
            if !cancel_and_reconcile(client, &login.login_id).await? {
                return Err(error);
            }
            true
        }
    };
    if !success
        || !matches!(
            client.account().await?.account,
            Some(Account::Chatgpt { .. })
        )
    {
        return Err("NATIVE_CHATGPT_LOGIN_NOT_CONFIRMED".into());
    }
    Ok(())
}

async fn run(args: Arguments) -> Result<()> {
    if !args.cancel_only && !(std::io::stdin().is_terminal() && std::io::stdout().is_terminal()) {
        return Err("FULL_ACCEPTANCE_REQUIRES_A_PRIVATE_INTERACTIVE_TERMINAL".into());
    }
    let root = tempfile::tempdir()?;
    std::fs::write(
        root.path().join("config.toml"),
        "cli_auth_credentials_store = \"file\"\nforced_login_method = \"chatgpt\"\n",
    )?;
    let mut client = Client::start(launch(&args.codex_binary, root.path())).await?;
    let result = exercise(&mut client, args.cancel_only).await;
    // Stop the process that owned the login before cleanup. Even an uncertain
    // cancellation cannot write a late authentication result after this close.
    client.close().await?;
    // Reopening observes native persistence, not an in-memory success flag.
    println!("phase=restart_status");
    let mut client = Client::start(launch(&args.codex_binary, root.path())).await?;
    let account = client.account().await?.account;
    let expected = if args.cancel_only {
        account.is_none()
    } else {
        matches!(account, Some(Account::Chatgpt { .. }))
    };
    let result = if result.is_ok() && !expected {
        Err("NATIVE_RESTART_ACCOUNT_STATE_MISMATCH".into())
    } else {
        result
    };
    // Cleanup also runs after interruption/failure, using a process with no pending login.
    println!(
        "phase={}",
        if args.cancel_only {
            "empty_account_logout"
        } else {
            "logout"
        }
    );
    client.logout().await?;
    client.close().await?;
    let mut client = Client::start(launch(&args.codex_binary, root.path())).await?;
    let signed_out = client.account().await?.account.is_none();
    client.close().await?;
    if !signed_out {
        return Err("NATIVE_LOGOUT_DID_NOT_PERSIST".into());
    }
    root.close()?;
    result?;
    println!("native_version={} start_cancel=passed empty_account_logout={} logout_restart={} full_login={} temporary_profile_removed=true", server::codex_native::VERSION, if args.cancel_only { "passed" } else { "not_run" }, if args.cancel_only { "not_run" } else { "passed" }, if args.cancel_only { "not_run" } else { "passed" });
    Ok(())
}

#[tokio::main]
async fn main() {
    if let Err(error) = run(Arguments::parse()).await {
        // Client errors are already safe projections; never print native frames/accounts.
        eprintln!("PROTECTED_LOGIN_ACCEPTANCE_FAILED: {error}");
        std::process::exit(1);
    }
}
