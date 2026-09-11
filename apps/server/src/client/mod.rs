//! Native HTTP CLI over shared Rust contracts. Never opens a database or an application vault.
mod commands;
mod watch;

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
    /// Explicit control-plane origin, without a path, query, fragment or user info.
    #[arg(long)]
    pub origin: String,
    /// Private file containing an existing qz2 machine credential, not a browser cookie.
    #[arg(long)]
    pub credential_file: PathBuf,
    /// Optional native CA bundle. There is no unverified-TLS mode.
    #[arg(long)]
    pub ca_certificate: Option<PathBuf>,
    /// Explicit literal-loopback development only; the server must also permit it.
    #[arg(long)]
    pub development_http: bool,
    /// Required for writes. Keep the same key and input after an unknown result.
    #[arg(long, global = true)]
    pub idempotency_key: Option<String>,
    /// Recent single-use human grant for this exact command and credential.
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
        domain::settings::endpoint(&args.origin, args.development_http)
            .map_err(|_| Failure::Configuration)?;
        let origin = Url::parse(&args.origin).map_err(|_| Failure::Configuration)?;
        let bytes = read_file(&args.credential_file, 256, true)?;
        let bytes = bytes.strip_suffix(b"\n").unwrap_or(&bytes);
        let credential = std::str::from_utf8(bytes)
            .map_err(|_| Failure::Credential)?
            .to_owned();
        integrations::authentication::machine_token(&credential)
            .map_err(|_| Failure::Credential)?;
        let mut authorization = header::HeaderValue::from_str(&format!("Bearer {credential}"))
            .map_err(|_| Failure::Credential)?;
        authorization.set_sensitive(true);
        let mut headers = header::HeaderMap::new();
        headers.insert(header::AUTHORIZATION, authorization);
        headers.insert(
            header::ACCEPT,
            header::HeaderValue::from_static("application/json"),
        );
        headers.insert(
            header::ACCEPT_ENCODING,
            header::HeaderValue::from_static("identity"),
        );
        let mut builder = Client::builder()
            .default_headers(headers)
            .redirect(reqwest::redirect::Policy::none())
            .retry(reqwest::retry::never())
            .no_proxy()
            .no_gzip()
            .no_brotli()
            .no_deflate()
            .no_zstd()
            .connect_timeout(Duration::from_secs(3))
            .timeout(Duration::from_secs(20));
        if let Some(path) = &args.ca_certificate {
            let bytes = read_file(path, 65536, false)?;
            let certificates = reqwest::Certificate::from_pem_bundle(&bytes)
                .map_err(|_| Failure::Configuration)?;
            if certificates.is_empty() || origin.scheme() != "https" {
                return Err(Failure::Configuration);
            }
            builder = builder.tls_built_in_root_certs(false);
            for certificate in certificates {
                builder = builder.add_root_certificate(certificate);
            }
        }
        Ok(Self {
            client: builder.build().map_err(|_| Failure::Configuration)?,
            origin,
            credential,
        })
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
        call.send().await.map_err(|_| Failure::Unavailable)
    }

    async fn checked(&self, response: Response, expected: u16) -> Result<Response> {
        let status = response.status().as_u16();
        if status == expected {
            return Ok(response);
        }
        if !(400..=599).contains(&status) {
            return Err(Failure::Contract);
        }
        media(&response, "application/problem+json")?;
        let bytes = body(response, 1024 * 1024).await?;
        verify(&bytes, &self.credential)?;
        let problem: Problem = serde_json::from_slice(&bytes).map_err(|_| Failure::Contract)?;
        if problem.status != status || !problem.kind.starts_with("urn:quazonai:problem:") {
            return Err(Failure::Contract);
        }
        Err(Failure::Rejected(Box::new(problem)))
    }

    async fn artifact_metadata(&self, id: Id) -> Result<ArtifactView> {
        let request = commands::Command::Artifact(commands::Artifact::Show { id: id.to_string() })
            .request()?;
        let response = self
            .checked(self.send(&request, None, None).await?, 200)
            .await?;
        media(&response, "application/json")?;
        let bytes = body(response, 1024 * 1024).await?;
        verify(&bytes, &self.credential)?;
        let metadata: ArtifactView =
            serde_json::from_slice(&bytes).map_err(|_| Failure::Contract)?;
        if metadata.id != id || metadata.byte_count.get() > 64 * 1024 * 1024 {
            return Err(Failure::Contract);
        }
        Ok(metadata)
    }
}

fn media(response: &Response, expected: &str) -> Result<()> {
    if response
        .headers()
        .get_all(header::CONTENT_TYPE)
        .iter()
        .count()
        != 1
    {
        return Err(Failure::Contract);
    }
    let mut encodings = response.headers().get_all(header::CONTENT_ENCODING).iter();
    if encodings.next().is_some_and(|value| value != "identity") || encodings.next().is_some() {
        return Err(Failure::Contract);
    }
    let value = response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|value| value.to_str().ok())
        .ok_or(Failure::Contract)?;
    let mut parts = value.split(';');
    if !parts
        .next()
        .is_some_and(|part| part.trim().eq_ignore_ascii_case(expected))
    {
        return Err(Failure::Contract);
    }
    if parts.any(|part| !part.trim().eq_ignore_ascii_case("charset=utf-8")) {
        return Err(Failure::Contract);
    }
    Ok(())
}

async fn body(mut response: Response, maximum: usize) -> Result<Vec<u8>> {
    if response
        .content_length()
        .is_some_and(|size| size > maximum as u64)
    {
        return Err(Failure::ResponseLimit);
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|_| Failure::Unavailable)? {
        if chunk.len() > maximum.saturating_sub(bytes.len()) {
            return Err(Failure::ResponseLimit);
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}
fn verify(bytes: &[u8], credential: &str) -> Result<()> {
    crate::runtime_transport::verify_native_json(bytes, credential).map_err(|_| Failure::Contract)
}
fn write_json(value: &impl serde::Serialize) -> Result<()> {
    let mut output = std::io::stdout().lock();
    serde_json::to_writer(&mut output, value).map_err(|_| Failure::Output)?;
    writeln!(output)
        .and_then(|_| output.flush())
        .map_err(|_| Failure::Output)
}

pub async fn run(arguments: Arguments) -> Result<()> {
    let connection = Connection::open(&arguments)?;
    let request = arguments.command.request()?;
    // A download is bound to the same immutable ID and its declared bytes/media,
    // not an assumed octet-stream response or a caller-chosen secondary URL.
    let metadata = match &request.output {
        commands::Output::Binary { id } => Some(connection.artifact_metadata(*id).await?),
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
            let bytes = body(response, 1024 * 1024).await?;
            verify(&bytes, &connection.credential)?;
            write_json(&decode(&bytes)?)
        }
        commands::Output::Binary { .. } => {
            let metadata = metadata.ok_or(Failure::Contract)?;
            media(&response, &metadata.media_type)?;
            let maximum =
                usize::try_from(metadata.byte_count.get()).map_err(|_| Failure::ResponseLimit)?;
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
