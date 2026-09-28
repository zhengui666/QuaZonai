//! Native HTTP CLI over shared Rust contracts. Never opens a database or an application vault.
mod commands;
mod preview;
mod session;
mod watch;

use crate::service_http::{self, body, media, verify, MAX_JSON_BYTES};
use clap::Args;
use contracts::{artifacts::ArtifactView, http::Problem, Id};
use reqwest::{header, Client, Method, Response, Url};
use std::{
    fmt,
    fs::File,
    io::{Read, Write},
    path::PathBuf,
    time::Duration,
};

#[derive(Args)]
pub struct Arguments {
    /// Frontend origin; pair with --credential-file to use an existing scoped credential.
    #[arg(long)]
    pub origin: Option<String>,
    /// Private existing scoped qz2 machine token file; otherwise reuse the saved login.
    #[arg(long)]
    pub credential_file: Option<PathBuf>,
    /// Optional native CA bundle. There is no unverified-TLS mode.
    #[arg(long)]
    pub ca_certificate: Option<PathBuf>,
    /// Explicit local-console HTTP only; the server must also permit it.
    #[arg(long)]
    pub development_http: bool,
    /// Validate the local request and print a redacted plan; never contacts the server.
    #[arg(long, global = true)]
    pub preview: bool,
    /// Required for writes. Keep the same key and input after an unknown result.
    #[arg(long, global = true)]
    pub idempotency_key: Option<String>,
    /// Single-use human grant for scoped machine credentials; saved owner devices need none.
    #[arg(long, global = true)]
    pub operator_grant: Option<String>,
    #[command(subcommand)]
    pub command: commands::Command,
}

pub type Result<T> = std::result::Result<T, Failure>;

// No raw reqwest/I/O/Serde error is retained: those can include private URLs or values.
#[derive(Debug)]
pub enum Failure {
    Configuration,
    LoginRequired,
    TerminalRequired,
    ReplaceRequired,
    Credential,
    Input,
    IdempotencyRequired,
    OperatorGrantRequired,
    Unavailable,
    Contract,
    ResponseLimit,
    ResetRequired,
    Output,
    Rejected(Box<Problem>),
}
impl fmt::Display for Failure {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Configuration => "CLI_CONFIGURATION_INVALID",
            Self::LoginRequired => "CLI_LOGIN_REQUIRED",
            Self::TerminalRequired => "CLI_LOGIN_REQUIRES_TERMINAL",
            Self::ReplaceRequired => "CLI_LOGIN_REPLACE_REQUIRED",
            Self::Credential => "CLI_CREDENTIAL_INVALID",
            Self::Input => "CLI_INPUT_INVALID",
            Self::IdempotencyRequired => "CLI_IDEMPOTENCY_KEY_REQUIRED",
            Self::OperatorGrantRequired => "CLI_OPERATOR_GRANT_REQUIRED",
            Self::Unavailable => "CLI_SERVER_UNAVAILABLE_OR_RESULT_UNKNOWN",
            Self::Contract => "CLI_RESPONSE_CONTRACT_INVALID",
            Self::ResponseLimit => "CLI_RESPONSE_LIMIT",
            Self::ResetRequired => "CLI_EVENT_CURSOR_RESET_REQUIRED",
            Self::Output => "CLI_OUTPUT_UNAVAILABLE",
            Self::Rejected(_) => "CLI_SERVER_REJECTED_REQUEST",
        })
    }
}
impl std::error::Error for Failure {}

impl From<service_http::Failure> for Failure {
    fn from(value: service_http::Failure) -> Self {
        match value {
            service_http::Failure::Configuration => Self::Configuration,
            service_http::Failure::Unavailable => Self::Unavailable,
            service_http::Failure::Contract => Self::Contract,
            service_http::Failure::ResponseLimit => Self::ResponseLimit,
            service_http::Failure::Rejected(problem) => Self::Rejected(problem),
        }
    }
}

struct Connection {
    client: Client,
    origin: Url,
    credential: String,
}

fn read_file(path: &PathBuf, maximum: usize, private: bool) -> Result<Vec<u8>> {
    let file = File::open(path).map_err(|_| Failure::Configuration)?;
    let metadata = file.metadata().map_err(|_| Failure::Configuration)?;
    if !metadata.is_file() || metadata.len() == 0 || metadata.len() > maximum as u64 {
        return Err(Failure::Configuration);
    }
    #[cfg(unix)]
    if private {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(Failure::Credential);
        }
    }
    #[cfg(not(unix))]
    if private {
        return Err(Failure::Configuration);
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(maximum as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|_| Failure::Configuration)?;
    if bytes.len() != metadata.len() as usize {
        return Err(Failure::Configuration);
    }
    Ok(bytes)
}

impl Connection {
    fn open(args: &Arguments) -> Result<Self> {
        let (origin, credential, development_http, ca_certificate) =
            match (&args.origin, &args.credential_file) {
                (Some(origin), Some(path)) => {
                    let bytes = read_file(path, 256, true)?;
                    let bytes = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
                    let token = std::str::from_utf8(bytes)
                        .map_err(|_| Failure::Credential)?
                        .to_owned();
                    integrations::authentication::machine_token(&token)
                        .map_err(|_| Failure::Credential)?;
                    (
                        origin.clone(),
                        token,
                        args.development_http,
                        args.ca_certificate.clone(),
                    )
                }
                (None, None) => {
                    let profile = session::Profile::load()?;
                    (
                        profile.origin,
                        profile.token,
                        args.development_http || profile.development_http,
                        args.ca_certificate.clone().or(profile.ca_certificate),
                    )
                }
                _ => return Err(Failure::Configuration),
            };
        Self::connect(origin, credential, development_http, ca_certificate)
    }

    fn connect(
        origin: String,
        credential: String,
        development_http: bool,
        ca_certificate: Option<PathBuf>,
    ) -> Result<Self> {
        let origin = session::origin(&origin, development_http)?;
        session::device_token(&credential)?;
        let headers = service_http::bearer(&credential).map_err(|_| Failure::Credential)?;
        Ok(Self {
            client: session::http_client(&origin, ca_certificate.as_ref(), headers)?,
            origin,
            credential,
        })
    }

    fn is_device(&self) -> bool {
        self.credential.starts_with("qzc.")
    }

    fn url(&self, request: &commands::Request) -> Result<Url> {
        let mut url = self.origin.clone();
        if !request.route.starts_with("/api/v2/") || request.route.contains(['?', '#', '\\']) {
            return Err(Failure::Input);
        }
        url.set_path(&request.route);
        if !request.query.is_empty() {
            url.query_pairs_mut().extend_pairs(
                request
                    .query
                    .iter()
                    .map(|(name, value)| (name.as_str(), value.as_str())),
            );
        }
        Ok(url)
    }

    async fn send(
        &self,
        request: &commands::Request,
        key: Option<&str>,
        operator_grant: Option<&str>,
    ) -> Result<Response> {
        let mut call = self
            .client
            .request(request.method.clone(), self.url(request)?);
        if request.method != Method::GET {
            let key = key.ok_or(Failure::IdempotencyRequired)?;
            let mut headers = header::HeaderMap::new();
            headers.insert(
                "idempotency-key",
                header::HeaderValue::from_str(key).map_err(|_| Failure::Input)?,
            );
            crate::access::idempotency_key(&headers).map_err(|_| Failure::Input)?;
            call = call.headers(headers);
            if let Some(body) = &request.body {
                call = call
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body.clone());
            }
        }
        if request.operator && operator_grant.is_none() {
            return Err(Failure::OperatorGrantRequired);
        }
        if let Some(grant) = operator_grant {
            let id: Id = grant.to_owned().try_into().map_err(|_| Failure::Input)?;
            call = call.header("x-operator-grant", id.to_string());
        }
        if let commands::Output::Binary { .. } = &request.output {
            call = call.header(
                header::ACCEPT,
                "application/octet-stream, application/json, text/plain",
            );
        }
        if let commands::Output::Events {
            run,
            after,
            seconds,
            ..
        } = &request.output
        {
            watch::cursor(*run, after.as_deref())?;
            call = call
                .header(header::ACCEPT, "text/event-stream")
                .timeout(Duration::from_secs(u64::from(*seconds) + 5));
            if let Some(after) = after {
                call = call.header("last-event-id", after);
            }
        }
        Ok(service_http::send(call).await?)
    }

    async fn checked(&self, response: Response, expected: u16) -> Result<Response> {
        Ok(service_http::checked(response, expected, &self.credential).await?)
    }

    async fn historical_artifact_metadata(
        &self,
        report: Id,
        record: Id,
    ) -> Result<contracts::imports::HistoricalArtifactResultV1> {
        let request = commands::Command::Migrate(commands::Migrate::Artifact {
            id: report.to_string(),
            record: record.to_string(),
        })
        .request()?;
        let response = self
            .checked(self.send(&request, None, None).await?, 200)
            .await?;
        media(&response, "application/json")?;
        let bytes = body(response, MAX_JSON_BYTES).await?;
        let metadata: contracts::imports::HistoricalArtifactResultV1 =
            service_http::decode(&bytes, &self.credential)?;
        if metadata.report_id != report
            || metadata.record_id != Some(record)
            || !metadata.stored
            || !metadata.verified_readable
            || metadata.source_outcome
                != Some(contracts::imports::HistoricalArtifactOutcomeV1::Copied)
            || metadata
                .byte_count
                .is_none_or(|n| n.get() == 0 || n.get() > 64 * 1024 * 1024)
        {
            return Err(Failure::Contract);
        }
        Ok(metadata)
    }
    async fn artifact_metadata(&self, id: Id) -> Result<ArtifactView> {
        let request = commands::Command::Artifact(commands::Artifact::Show { id: id.to_string() })
            .request()?;
        let response = self
            .checked(self.send(&request, None, None).await?, 200)
            .await?;
        media(&response, "application/json")?;
        let bytes = body(response, MAX_JSON_BYTES).await?;
        let metadata: ArtifactView = service_http::decode(&bytes, &self.credential)?;
        if metadata.id != id || metadata.byte_count.get() > 64 * 1024 * 1024 {
            return Err(Failure::Contract);
        }
        Ok(metadata)
    }
}

fn write_json(value: &impl serde::Serialize) -> Result<()> {
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, value).map_err(|_| Failure::Output)?;
    writeln!(output)
        .and_then(|_| output.flush())
        .map_err(|_| Failure::Output)
}

pub async fn run(arguments: Arguments) -> Result<()> {
    if let commands::Command::Login { ref name, replace } = arguments.command {
        return session::login(&arguments, name.as_deref(), replace).await;
    }
    if arguments.preview {
        // Explicit legacy previews still never open the credential file. Saved
        // connections read only their private local profile, never the server.
        let profile = if arguments.origin.is_none() && arguments.credential_file.is_none() {
            Some(session::Profile::load()?)
        } else {
            None
        };
        let origin = arguments
            .origin
            .as_deref()
            .or_else(|| profile.as_ref().map(|p| p.origin.as_str()))
            .ok_or(Failure::Configuration)?;
        let device = profile.is_some();
        let request = arguments.command.request_for(device)?;
        return write_json(&preview::inspect(
            &request,
            origin,
            arguments.development_http || profile.as_ref().is_some_and(|p| p.development_http),
            arguments.idempotency_key.as_deref(),
            arguments.operator_grant.as_deref(),
        )?);
    }
    let connection = Connection::open(&arguments)?;
    let request = arguments.command.request_for(connection.is_device())?;
    // A download is bound to the same immutable ID and its declared bytes/media,
    // not an assumed octet-stream response or a caller-chosen secondary URL.
    let metadata = match &request.output {
        commands::Output::Binary { id, report: None } => {
            let metadata = connection.artifact_metadata(*id).await?;
            Some((metadata.media_type, metadata.byte_count))
        }
        commands::Output::Binary {
            id,
            report: Some(report),
        } => {
            let metadata = connection
                .historical_artifact_metadata(*report, *id)
                .await?;
            Some((
                "application/octet-stream".to_owned(),
                metadata.byte_count.ok_or(Failure::Contract)?,
            ))
        }
        _ => None,
    };
    let response = connection
        .checked(
            connection
                .send(
                    &request,
                    arguments.idempotency_key.as_deref(),
                    arguments.operator_grant.as_deref(),
                )
                .await?,
            request.status,
        )
        .await?;
    match request.output {
        commands::Output::Json(decode) => {
            media(&response, "application/json")?;
            let bytes = body(response, MAX_JSON_BYTES).await?;
            verify(&bytes, &connection.credential)?;
            write_json(&decode(&bytes)?)
        }
        commands::Output::Binary { .. } => {
            let metadata = metadata.ok_or(Failure::Contract)?;
            media(&response, &metadata.0)?;
            let maximum = usize::try_from(metadata.1.get()).map_err(|_| Failure::ResponseLimit)?;
            let bytes = body(response, maximum).await?;
            if bytes.len() != maximum
                || bytes
                    .windows(connection.credential.len())
                    .any(|part| part == connection.credential.as_bytes())
            {
                return Err(Failure::Contract);
            }
            let mut output = std::io::stdout().lock();
            output
                .write_all(&bytes)
                .and_then(|_| output.flush())
                .map_err(|_| Failure::Output)
        }
        commands::Output::Events {
            run,
            after,
            seconds,
            events,
        } => {
            media(&response, "text/event-stream")?;
            watch::stream(
                response,
                &connection.credential,
                run,
                after,
                seconds,
                events,
            )
            .await
        }
    }
}

/// Print only a validated common Problem or a closed local code, never a native error.
pub fn report(error: &Failure) {
    let value = match error {
        Failure::Rejected(problem) => serde_json::to_value(problem).ok(),
        _ => Some(
            serde_json::json!({"schema_version":1,"code":error.to_string(),"request_sent_again":false}),
        ),
    };
    if let Some(value) = value {
        let mut stderr = std::io::stderr().lock();
        let _ = serde_json::to_writer(&mut stderr, &value);
        let _ = writeln!(stderr);
    }
}

#[cfg(test)]
mod origin_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[tokio::test]
    async fn cli_accepts_the_same_local_origin_as_the_browser() {
        let directory = tempfile::tempdir().unwrap();
        let file = directory.path().join("disposable-cli-token");
        let secret = integrations::authentication::random_capability();
        let token = integrations::authentication::format_machine_token(Id::new(), &secret).unwrap();
        std::fs::write(&file, token).unwrap();
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o600)).unwrap();
        for (origin, development_http, accepted) in [
            ("http://localhost:8081", true, true),
            ("http://127.0.0.1:8080", true, true),
            ("http://[::1]:8081", true, true),
            ("https://localhost", false, true),
            ("https://127.0.0.1", false, true),
            ("http://localhost:8081", false, false),
            ("https://qz.example", false, true),
            ("http://192.168.1.1:8081", true, false),
            ("http://localhost:8081/path", true, false),
            ("http://user:pass@localhost:8081", true, false),
        ] {
            let arguments = Arguments {
                origin: Some(origin.into()),
                credential_file: Some(file.clone()),
                ca_certificate: None,
                development_http,
                preview: false,
                idempotency_key: None,
                operator_grant: None,
                command: commands::Command::Project(commands::Project::List(commands::List {
                    cursor: None,
                    limit: 1,
                })),
            };
            assert_eq!(Connection::open(&arguments).is_ok(), accepted, "{origin}");
        }
    }
}
