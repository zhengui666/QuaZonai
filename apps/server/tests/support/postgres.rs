use sqlx::{ConnectOptions, PgPool};

// Native clients use only the disposable SQLx connection, never the host's default DB.
pub fn postgres_tool(pool: &PgPool, tool: &str) -> tokio::process::Command {
    let mut options = pool.connect_options().as_ref().clone();
    let mut command = if let Ok(container) = std::env::var("QZ_TEST_PG_CONTAINER") {
        options = options.host("127.0.0.1").port(5432);
        let mut command = tokio::process::Command::new("docker");
        command.args([
            "exec",
            "-i",
            "--env",
            "PGDATABASE",
            "--env",
            "PGHOST",
            "--env",
            "PGPORT",
            "--env",
            "PGUSER",
            "--env",
            "PGPASSWORD",
            &container,
            tool,
        ]);
        command
    } else {
        tokio::process::Command::new(tool)
    };
    // Libpq's environment default is a database name, not an expanded SQLx URI.
    let url = options.to_url_lossy();
    let encoded = format!(
        "password={}",
        url.password().unwrap_or_default().replace('+', "%2B")
    );
    let password = url::form_urlencoded::parse(encoded.as_bytes())
        .next()
        .unwrap()
        .1
        .into_owned();
    command
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .env("PGHOST", options.get_host())
        .env("PGPORT", options.get_port().to_string())
        .env("PGUSER", options.get_username())
        .env("PGPASSWORD", password)
        .env(
            "PGDATABASE",
            options.get_database().expect("disposable test database"),
        )
        .kill_on_drop(true);
    command
}
