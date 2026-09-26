//! Interactive owner login and one private, atomically replaced connection profile.
use super::{body, media, read_file, verify, write_json, Arguments, Connection, Failure, Result};
use contracts::{
    auth::{CliLogin, CliLoginResult},
    http::Problem,
    Id, SchemaV1,
};
use reqwest::{header, Client, Response, Url};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, File, OpenOptions},
    io::{BufRead, IsTerminal, Read, Write},
    path::{Path, PathBuf},
    time::Duration,
};

#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Profile {
    schema_version: SchemaV1,
    pub origin: String,
    pub token: String,
    pub development_http: bool,
    pub ca_certificate: Option<PathBuf>,
}

fn profile_path() -> Result<PathBuf> {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            std::env::var_os("HOME")
                .filter(|value| !value.is_empty())
                .map(|home| PathBuf::from(home).join(".config"))
        })
        .ok_or(Failure::Configuration)?;
    if !base.is_absolute() {
        return Err(Failure::Configuration);
    }
    Ok(base.join("quazonai/client.json"))
}

impl Profile {
    pub fn load() -> Result<Self> {
        Self::load_from(&profile_path()?)
    }

    fn load_from(path: &PathBuf) -> Result<Self> {
        if !path.try_exists().map_err(|_| Failure::Configuration)? {
            return Err(Failure::LoginRequired);
        }
        let profile: Self = serde_json::from_slice(&read_file(path, 16 * 1024, true)?)
            .map_err(|_| Failure::Configuration)?;
        origin(&profile.origin, profile.development_http)?;
        if !device_token(&profile.token)? {
            return Err(Failure::Credential);
        }
        Ok(profile)
    }

    #[cfg(unix)]
    fn save(&self, path: &Path) -> Result<()> {
        use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
        let parent = path.parent().ok_or(Failure::Configuration)?;
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(parent)
            .map_err(|_| Failure::Configuration)?;
        let metadata = fs::symlink_metadata(parent).map_err(|_| Failure::Configuration)?;
        if !metadata.is_dir() || metadata.permissions().mode() & 0o077 != 0 {
            return Err(Failure::Credential);
        }
        let temporary = parent.join(format!(".client-{}.tmp", Id::new()));
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&temporary)
            .map_err(|_| Failure::Configuration)?;
        let result = (|| {
            serde_json::to_writer(&mut file, self).map_err(|_| Failure::Configuration)?;
            file.write_all(b"\n")
                .and_then(|_| file.sync_all())
                .map_err(|_| Failure::Configuration)?;
            fs::rename(&temporary, path).map_err(|_| Failure::Configuration)?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| Failure::Configuration)
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    #[cfg(not(unix))]
    fn save(&self, _: &Path) -> Result<()> {
        Err(Failure::Configuration)
    }
}

pub(super) fn origin(value: &str, development_http: bool) -> Result<Url> {
    crate::WebPolicy::new(value, ([127, 0, 0, 1], 0).into(), development_http)
        .map_err(|_| Failure::Configuration)?;
    Url::parse(value).map_err(|_| Failure::Configuration)
}

pub(super) fn device_token(value: &str) -> Result<bool> {
    if value.starts_with("qzc.") {
        integrations::authentication::cli_token(value).map_err(|_| Failure::Credential)?;
        Ok(true)
    } else {
        integrations::authentication::machine_token(value).map_err(|_| Failure::Credential)?;
        Ok(false)
    }
}

pub(super) fn http_client(
    origin: &Url,
    certificate: Option<&PathBuf>,
    mut headers: header::HeaderMap,
) -> Result<Client> {
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
    if let Some(path) = certificate {
        let bytes = read_file(path, 65536, false)?;
        let certificates =
            reqwest::Certificate::from_pem_bundle(&bytes).map_err(|_| Failure::Configuration)?;
        if certificates.is_empty() || origin.scheme() != "https" {
            return Err(Failure::Configuration);
        }
        builder = builder.tls_built_in_root_certs(false);
        for certificate in certificates {
            builder = builder.add_root_certificate(certificate);
        }
    }
    builder.build().map_err(|_| Failure::Configuration)
}

pub(super) async fn login(arguments: &Arguments, name: Option<&str>, replace: bool) -> Result<()> {
    if arguments.preview
        || arguments.credential_file.is_some()
        || arguments.operator_grant.is_some()
        || arguments.idempotency_key.is_some()
    {
        return Err(Failure::Input);
    }
    let path = profile_path()?;
    let mut development_http = arguments.development_http;
    let mut certificate = arguments.ca_certificate.clone();
    let previous = if replace {
        writeln!(std::io::stderr().lock(), "Replacing the local connection; any previous CLI device remains managed in authentication settings.")
            .map_err(|_| Failure::Output)?;
        None
    } else {
        match Profile::load_from(&path) {
            Ok(profile) => Some(profile),
            Err(Failure::LoginRequired) => None,
            Err(error) => return Err(error),
        }
    };
    if let Some(mut profile) = previous {
        development_http |= profile.development_http;
        certificate = certificate
            .or(profile.ca_certificate.clone())
            .as_ref()
            .map(fs::canonicalize)
            .transpose()
            .map_err(|_| Failure::Configuration)?;
        if let Some(address) = &arguments.origin {
            if origin(address, development_http)?
                .origin()
                .ascii_serialization()
                != profile.origin
            {
                return Err(Failure::ReplaceRequired);
            }
        }
        let connection = Connection::connect(
            profile.origin.clone(),
            profile.token.clone(),
            development_http,
            certificate.clone(),
        )?;
        let request = super::commands::Command::Identity.request_for(true)?;
        let response = connection.send(&request, None, None).await?;
        match connection.checked(response, 200).await {
            Err(Failure::Rejected(problem))
                if problem.status == 401 && problem.code == "AUTHENTICATION_FAILED" => {}
            Err(error) => return Err(error),
            Ok(response) => {
                media(&response, "application/json")?;
                let bytes = body(response, 16 * 1024).await?;
                verify(&bytes, &connection.credential)?;
                let device: contracts::auth::CliDevice =
                    serde_json::from_slice(&bytes).map_err(|_| Failure::Contract)?;
                if name.is_some_and(|name| name != device.name) {
                    return Err(Failure::ReplaceRequired);
                }
                if profile.development_http != development_http
                    || profile.ca_certificate != certificate
                {
                    profile.development_http = development_http;
                    profile.ca_certificate = certificate;
                    profile.save(&path)?;
                }
                return write_json(&device);
            }
        }
    }
    // Never accept a password through redirected stdin, argv or an environment variable.
    if !std::io::stdin().is_terminal() {
        return Err(Failure::TerminalRequired);
    }
    let address = match &arguments.origin {
        Some(value) => value.clone(),
        None => {
            let mut stderr = std::io::stderr().lock();
            write!(stderr, "QuaZonai frontend address: ")
                .and_then(|_| stderr.flush())
                .map_err(|_| Failure::Output)?;
            let mut value = String::new();
            std::io::stdin()
                .lock()
                .take(2049)
                .read_line(&mut value)
                .map_err(|_| Failure::Input)?;
            if value.len() > 2048 {
                return Err(Failure::Input);
            }
            value.trim().to_owned()
        }
    };
    let url = origin(&address, development_http)?;
    let ca_certificate = certificate
        .as_ref()
        .map(fs::canonicalize)
        .transpose()
        .map_err(|_| Failure::Configuration)?;
    let client = http_client(&url, ca_certificate.as_ref(), header::HeaderMap::new())?;
    let native_name = fs::read_to_string("/proc/sys/kernel/hostname")
        .or_else(|_| fs::read_to_string("/etc/hostname"))
        .unwrap_or_default();
    let name = name.unwrap_or(native_name.trim());
    if name.trim().is_empty() || name.chars().count() > 100 || name.chars().any(char::is_control) {
        return Err(Failure::Input);
    }
    let password =
        rpassword::prompt_password("QuaZonai password: ").map_err(|_| Failure::TerminalRequired)?;
    if password.len() < 8 || password.len() > 1024 {
        return Err(Failure::Input);
    }
    let mut endpoint = url.clone();
    endpoint.set_path("/api/v2/auth/cli/login");
    let response = client
        .post(endpoint)
        .header(header::ORIGIN, url.origin().ascii_serialization())
        .json(&CliLogin {
            schema_version: SchemaV1,
            password: password.clone(),
            name: name.to_owned(),
        })
        .send()
        .await
        .map_err(|_| Failure::Unavailable)?;
    let response = checked_login(response, &password).await?;
    media(&response, "application/json")?;
    let bytes = body(response, 16 * 1024).await?;
    let result = login_result(&bytes, &password, name)?;
    Profile {
        schema_version: SchemaV1,
        origin: url.origin().ascii_serialization(),
        token: result.token,
        development_http,
        ca_certificate,
    }
    .save(&path)?;
    write_json(&result.device)
}

fn login_result(bytes: &[u8], password: &str, name: &str) -> Result<CliLoginResult> {
    // Parse the closed DTO first so duplicates/unknown fields cannot disappear
    // when inspecting values. Check both received spelling and normalized output:
    // timestamps become UTC and IDs become lowercase during serialization.
    let result: CliLoginResult = serde_json::from_slice(bytes).map_err(|_| Failure::Contract)?;
    for value in [
        serde_json::from_slice(bytes).map_err(|_| Failure::Contract)?,
        serde_json::to_value(&result).map_err(|_| Failure::Contract)?,
    ] {
        if password_value(&value, password) {
            return Err(Failure::Contract);
        }
    }
    let token =
        integrations::authentication::cli_token(&result.token).map_err(|_| Failure::Contract)?;
    if result.device.id != token.public_token_id || result.device.name != name {
        return Err(Failure::Contract);
    }
    verify(
        &serde_json::to_vec(&result.device).map_err(|_| Failure::Contract)?,
        &result.token,
    )?;
    Ok(result)
}

fn password_value(value: &serde_json::Value, password: &str) -> bool {
    match value {
        serde_json::Value::String(text) => text.contains(password),
        serde_json::Value::Array(values) => {
            values.iter().any(|value| password_value(value, password))
        }
        serde_json::Value::Object(fields) => {
            fields.values().any(|value| password_value(value, password))
        }
        _ => false,
    }
}

async fn checked_login(response: Response, password: &str) -> Result<Response> {
    let status = response.status().as_u16();
    if status == 201 {
        return Ok(response);
    }
    if !(400..=599).contains(&status) {
        return Err(Failure::Contract);
    }
    media(&response, "application/problem+json")?;
    let bytes = body(response, 16 * 1024).await?;
    Err(Failure::Rejected(Box::new(login_problem(
        &bytes, status, password,
    )?)))
}

fn login_problem(bytes: &[u8], status: u16, password: &str) -> Result<Problem> {
    // Closed DTOs reject duplicate/unknown keys without confusing key names with
    // a submitted password. Only arbitrary response strings need redaction.
    let mut problem: Problem = serde_json::from_slice(bytes).map_err(|_| Failure::Contract)?;
    if problem.status != status || !problem.kind.starts_with("urn:quazonai:problem:") {
        return Err(Failure::Contract);
    }
    if problem.request_id.to_string().contains(password) {
        return Err(Failure::Contract);
    }
    if problem
        .current_revision
        .is_some_and(|revision| String::from(revision).contains(password))
    {
        problem.current_revision = None;
    }
    let redact = |text: &mut String| {
        if text.contains(password) {
            text.clear();
        }
    };
    if status == 401
        && problem.code == "AUTHENTICATION_FAILED"
        && problem.kind == "urn:quazonai:problem:authentication-failed"
    {
        // This is a protocol constant even when it happens to be the password.
        problem.title = "AUTHENTICATION_FAILED".into();
    } else {
        redact(&mut problem.kind);
        redact(&mut problem.code);
        redact(&mut problem.title);
    }
    redact(&mut problem.detail);
    for field in &mut problem.field_errors {
        redact(&mut field.field);
        redact(&mut field.code);
        redact(&mut field.message);
    }
    for action in &mut problem.safe_next_actions {
        redact(action);
    }
    Ok(problem)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    #[test]
    fn login_success_checks_original_and_normalized_values_without_rejecting_protocol_keys() {
        let id: Id = "018fc823-8e40-7abc-8abc-abcdef123456"
            .to_owned()
            .try_into()
            .unwrap();
        let secret = integrations::authentication::random_capability();
        let token = integrations::authentication::format_cli_token(id, &secret).unwrap();
        let native = serde_json::json!({
            "device":{"schema_version":1,"id":id,"name":"CLI acceptance",
                "created_at":"2026-09-24T00:00:00Z","last_used_at":"2026-09-25T00:00:00Z"},
            "token":token
        });
        let bytes = serde_json::to_vec(&native).unwrap();
        for password in ["schema_version", "created_at", "last_used_at"] {
            assert!(login_result(&bytes, password, "CLI acceptance").is_ok());
        }
        for password in [secret, token, "CLI acceptance".to_owned()] {
            assert!(login_result(&bytes, &password, "CLI acceptance").is_err());
        }
        let mut uppercase = native.clone();
        uppercase["device"]["id"] = serde_json::json!(id.to_string().to_uppercase());
        uppercase["token"] = serde_json::json!(native["token"]
            .as_str()
            .unwrap()
            .replace(&id.to_string(), &id.to_string().to_uppercase()));
        let uppercase = serde_json::to_vec(&uppercase).unwrap();
        assert!(!String::from_utf8_lossy(&uppercase).contains(&id.to_string()));
        for password in [id.to_string(), id.to_string().to_uppercase()] {
            assert!(login_result(&uppercase, &password, "CLI acceptance").is_err());
        }
        for field in ["created_at", "last_used_at"] {
            let mut reflected = native.clone();
            let original = "2026-09-26T03:02:03+02:00";
            let normalized = "2026-09-26T01:02:03Z";
            reflected["device"][field] = serde_json::json!(original);
            let bytes = serde_json::to_vec(&reflected).unwrap();
            let typed: CliLoginResult = serde_json::from_slice(&bytes).unwrap();
            assert_eq!(
                serde_json::to_value(&typed).unwrap()["device"][field],
                normalized
            );
            assert!(!String::from_utf8_lossy(&bytes).contains(normalized));
            assert!(login_result(&bytes, original, "CLI acceptance").is_err());
            assert!(login_result(&bytes, normalized, "CLI acceptance").is_err());
            let escaped = original
                .chars()
                .map(|c| format!("\\u{:04x}", c as u32))
                .collect::<String>();
            let escaped = String::from_utf8(bytes)
                .unwrap()
                .replace(original, &escaped);
            assert!(!escaped.contains(original));
            assert!(login_result(escaped.as_bytes(), original, "CLI acceptance").is_err());
        }
        let mut mismatched = native.clone();
        mismatched["device"]["id"] = serde_json::json!(Id::new());
        assert!(login_result(
            &serde_json::to_vec(&mismatched).unwrap(),
            "schema_version",
            "CLI acceptance"
        )
        .is_err());
        let text = String::from_utf8(bytes).unwrap();
        for invalid in [
            format!("{{\"unknown\":true,{}", &text[1..]),
            text.replace(
                "\"schema_version\":1",
                "\"schema_version\":1,\"schema_version\":1",
            ),
            text.replace("qzc.", "qz2."),
        ] {
            assert!(login_result(invalid.as_bytes(), "schema_version", "CLI acceptance").is_err());
        }
        assert!(login_result(text.as_bytes(), "schema_version", "different device").is_err());
    }

    #[test]
    fn password_errors_preserve_protocol_coincidences_but_redact_arbitrary_reflections() {
        let native = serde_json::json!({
            "type":"urn:quazonai:problem:authentication-failed", "title":"AUTHENTICATION_FAILED",
            "status":401, "code":"AUTHENTICATION_FAILED", "detail":"Password not accepted",
            "request_id":Id::new(), "retryable":false, "field_errors":[], "safe_next_actions":[]
        });
        let bytes = serde_json::to_vec(&native).unwrap();
        for password in [
            "request_id",
            "AUTHENTICATION_FAILED",
            "authentication-failed",
            "Password not accepted",
        ] {
            let problem = login_problem(&bytes, 401, password).unwrap();
            assert_eq!(problem.code, "AUTHENTICATION_FAILED");
            assert_eq!(problem.kind, "urn:quazonai:problem:authentication-failed");
        }
        let password = "reflected-password";
        let mut reflected = native.clone();
        reflected["title"] = serde_json::json!(password);
        reflected["detail"] = serde_json::json!(format!("prefix {password} suffix"));
        reflected["field_errors"] =
            serde_json::json!([{"field":password,"code":password,"message":password}]);
        reflected["safe_next_actions"] = serde_json::json!([password]);
        let problem =
            login_problem(&serde_json::to_vec(&reflected).unwrap(), 401, password).unwrap();
        assert!(!serde_json::to_string(&problem).unwrap().contains(password));
        let mut reflected_scalar = native.clone();
        reflected_scalar["current_revision"] = serde_json::json!("12345678");
        assert!(login_problem(
            &serde_json::to_vec(&reflected_scalar).unwrap(),
            401,
            "12345678"
        )
        .unwrap()
        .current_revision
        .is_none());
        assert!(login_problem(&bytes, 401, native["request_id"].as_str().unwrap()).is_err());
        let text = String::from_utf8(bytes).unwrap();
        for invalid in [
            format!("{{\"status\":401,{}", &text[1..]),
            format!("{{\"unknown\":true,{}", &text[1..]),
            text.replace("\"field_errors\":[]", "\"field_errors\":[{\"field\":\"one\",\"field\":\"two\",\"code\":\"code\",\"message\":\"message\"}]"),
            text.replace("\"request_id\":", "\"request_id\":null,\"discarded_id\":"),
        ] {
            assert!(login_problem(invalid.as_bytes(), 401, password).is_err());
        }
        assert!(login_problem(text.as_bytes(), 403, password).is_err());
    }

    #[test]
    fn profile_is_private_atomic_and_rejects_a_scoped_or_exposed_credential() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("quazonai/client.json");
        assert!(matches!(
            Profile::load_from(&path),
            Err(Failure::LoginRequired)
        ));
        let token = integrations::authentication::format_cli_token(
            Id::new(),
            &integrations::authentication::random_capability(),
        )
        .unwrap();
        let mut profile = Profile {
            schema_version: SchemaV1,
            origin: "https://localhost".into(),
            token,
            development_http: false,
            ca_certificate: None,
        };
        profile.save(&path).unwrap();
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            fs::metadata(path.parent().unwrap())
                .unwrap()
                .permissions()
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(Profile::load_from(&path).unwrap().token, profile.token);
        profile.token = integrations::authentication::format_cli_token(
            Id::new(),
            &integrations::authentication::random_capability(),
        )
        .unwrap();
        profile.save(&path).unwrap();
        assert_eq!(Profile::load_from(&path).unwrap().token, profile.token);
        assert_eq!(fs::read_dir(path.parent().unwrap()).unwrap().count(), 1);
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
        assert!(matches!(
            Profile::load_from(&path),
            Err(Failure::Credential)
        ));
        profile.token = integrations::authentication::format_machine_token(
            Id::new(),
            &integrations::authentication::random_capability(),
        )
        .unwrap();
        profile.save(&path).unwrap();
        assert!(matches!(
            Profile::load_from(&path),
            Err(Failure::Credential)
        ));
    }
}
