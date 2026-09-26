//! Real terminal -> native CLI -> HTTP/PostgreSQL login and saved-device authority.
#[path = "support/client.rs"]
#[allow(dead_code)] // Reuse the real listener without the legacy scoped-token helpers.
mod client;
#[allow(dead_code)]
mod support;
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt, path::Path, process::Stdio, time::Duration};
use tokio::{io::AsyncWriteExt, process::Command};

// Python's stdlib supplies a controlling PTY, so rpassword exercises real terminal
// echo suppression. Every credential below belongs only to the disposable test.
const TERMINAL: &str = r#"
import errno, json, os, pty, select, signal, sys, termios, time
config = json.load(sys.stdin)
pid, fd = pty.fork()
if pid == 0:
    os.execve(sys.argv[1], [sys.argv[1], 'client', '--development-http', 'login', '--name', 'CLI acceptance'] + (['--replace'] if config['replace'] else []), {'XDG_CONFIG_HOME': config['directory']})
transcript = b''
address_sent = password_sent = False
status = None
try:
    deadline = time.monotonic() + 25
    while time.monotonic() < deadline:
        if select.select([fd], [], [], 0.1)[0]:
            try:
                data = os.read(fd, 65536)
            except OSError as error:
                if error.errno == errno.EIO: break
                raise
            if not data: break
            transcript += data
        if b'QuaZonai frontend address: ' in transcript and not address_sent:
            os.write(fd, (config['origin'] + '\n').encode())
            address_sent = True
        if b'QuaZonai password: ' in transcript and not password_sent:
            if termios.tcgetattr(fd)[3] & termios.ECHO: continue
            os.write(fd, (config['password'] + '\n').encode())
            password_sent = True
    if time.monotonic() >= deadline:
        os.kill(pid, signal.SIGKILL)
    _, status = os.waitpid(pid, 0)
    password_output = transcript.split(b'QuaZonai password: ', 1)[-1]
    assert config['password'].encode() not in [line.strip() for line in password_output.splitlines()], 'terminal echoed password'
    assert address_sent and password_sent, 'login did not request both interactive inputs'
    print(json.dumps({'exit': os.waitstatus_to_exitcode(status), 'transcript': transcript.decode()}))
finally:
    os.close(fd)
"#;

async fn login(directory: &Path, origin: &str, password: &str, replace: bool) -> Value {
    let mut child = Command::new("python3")
        .args(["-c", TERMINAL, env!("CARGO_BIN_EXE_server")])
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    let input =
        json!({"directory":directory,"origin":origin,"password":password,"replace":replace});
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&input).unwrap())
        .await
        .unwrap();
    let result = tokio::time::timeout(Duration::from_secs(30), child.wait_with_output())
        .await
        .unwrap()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}

async fn saved(directory: &Path, arguments: &[&str], body: Value) -> std::process::Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_server"))
        .arg("client")
        .current_dir(directory)
        .args(arguments)
        .env_clear()
        .env("XDG_CONFIG_HOME", directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    if !body.is_null() {
        child
            .stdin
            .take()
            .unwrap()
            .write_all(&serde_json::to_vec(&body).unwrap())
            .await
            .unwrap();
    }
    child.stdin.take();
    tokio::time::timeout(Duration::from_secs(25), child.wait_with_output())
        .await
        .unwrap()
        .unwrap()
}

#[tokio::test]
async fn login_requires_a_terminal_and_missing_connection_is_actionable() {
    let directory = tempfile::tempdir().unwrap();
    let result = saved(directory.path(), &["login"], Value::Null).await;
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("CLI_LOGIN_REQUIRES_TERMINAL"));
    let result = saved(directory.path(), &["identity"], Value::Null).await;
    assert!(!result.status.success());
    assert!(String::from_utf8_lossy(&result.stderr).contains("CLI_LOGIN_REQUIRED"));
    assert!(!directory.path().join("quazonai/client.json").exists());
}

#[sqlx::test(migrations = "../../migrations")]
async fn password_login_accepts_json_field_names_and_revocation_ends_device_authority(
    pool: PgPool,
) {
    let f = support::fixture(pool).await;
    let (origin, _listener) = client::listen(&f).await;
    let http = reqwest::Client::new();
    // A valid password can equal a wire field name without being echoed as data.
    let password = "schema_version";
    let setup = http
        .post(format!("{origin}/api/v2/auth/setup"))
        .header("origin", &origin)
        .json(&json!({"schema_version":1,"password":password,"remember_device":false}))
        .send()
        .await
        .unwrap();
    assert!(setup.status().is_success());
    let cookie = setup
        .headers()
        .get("set-cookie")
        .unwrap()
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    let directory = tempfile::tempdir().unwrap();
    for wrong_password in [
        "wrong-cli-login-password",
        "request_id",
        "AUTHENTICATION_FAILED",
    ] {
        let wrong = login(directory.path(), &origin, wrong_password, false).await;
        assert_ne!(wrong["exit"], 0);
        assert!(wrong["transcript"]
            .as_str()
            .unwrap()
            .contains("AUTHENTICATION_FAILED"));
        assert!(!wrong["transcript"]
            .as_str()
            .unwrap()
            .contains("CLI_RESPONSE_CONTRACT_INVALID"));
    }
    let path = directory.path().join("quazonai/client.json");
    assert!(!path.exists());
    let result = login(directory.path(), &origin, password, false).await;
    assert_eq!(result["exit"], 0, "{}", result["transcript"]);
    let profile: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(profile["origin"], origin);
    assert_eq!(profile["development_http"], true);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    assert!(profile.get("password").is_none());
    assert!(profile
        .as_object()
        .unwrap()
        .values()
        .all(|value| value.as_str() != Some(password)));
    let token = profile["token"].as_str().unwrap();
    assert!(token.starts_with("qzc."));
    assert!(!result["transcript"].as_str().unwrap().contains(token));
    let explicit = directory.path().join("explicit-device-token");
    fs::write(&explicit, token).unwrap();
    fs::set_permissions(&explicit, fs::Permissions::from_mode(0o600)).unwrap();
    let rejected = client::invoke(&origin, &explicit, &["identity"], Value::Null).await;
    assert!(!rejected.status.success());
    assert!(String::from_utf8_lossy(&rejected.stderr).contains("CLI_CREDENTIAL_INVALID"));
    assert!(!String::from_utf8_lossy(&rejected.stderr).contains(token));
    assert!(rejected.stdout.is_empty());
    let identity = saved(directory.path(), &["identity"], Value::Null).await;
    assert!(
        identity.status.success(),
        "{}",
        String::from_utf8_lossy(&identity.stderr)
    );
    let device: contracts::auth::CliDevice = serde_json::from_slice(&identity.stdout).unwrap();
    assert_eq!(device.name, "CLI acceptance");
    assert!(!String::from_utf8_lossy(&identity.stdout).contains(token));
    let again = saved(directory.path(), &["login"], Value::Null).await;
    assert!(again.status.success());
    let again: contracts::auth::CliDevice = serde_json::from_slice(&again.stdout).unwrap();
    assert_eq!(again.id, device.id);
    assert_eq!(
        serde_json::from_slice::<Value>(&fs::read(&path).unwrap()).unwrap(),
        profile
    );
    let switched = saved(
        directory.path(),
        &["--origin", "https://another.example", "login"],
        Value::Null,
    )
    .await;
    assert!(!switched.status.success());
    assert!(String::from_utf8_lossy(&switched.stderr).contains("CLI_LOGIN_REPLACE_REQUIRED"));
    let listed = http
        .get(format!("{origin}/api/v2/auth/cli/devices"))
        .header("cookie", &cookie)
        .send()
        .await
        .unwrap()
        .json::<Vec<contracts::auth::CliDevice>>()
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].id, device.id);
    let body = json!({"schema_version":1,"name":"Password device project","description":"", "fork_from_project_id":null});
    let preview = saved(
        directory.path(),
        &[
            "--preview",
            "--idempotency-key",
            "owner-device-project",
            "project",
            "create",
        ],
        body.clone(),
    )
    .await;
    assert!(
        preview.status.success(),
        "{}",
        String::from_utf8_lossy(&preview.stderr)
    );
    let preview: Value = serde_json::from_slice(&preview.stdout).unwrap();
    assert_eq!(preview["requires_operator_grant"], false);
    assert_eq!(preview["request_sent"], false);
    let create = saved(
        directory.path(),
        &[
            "--idempotency-key",
            "owner-device-project",
            "project",
            "create",
        ],
        body,
    )
    .await;
    assert!(
        create.status.success(),
        "{}",
        String::from_utf8_lossy(&create.stderr)
    );
    let revoke = http
        .delete(format!("{origin}/api/v2/auth/cli/devices/{}", device.id))
        .header("origin", &origin)
        .header("cookie", cookie)
        .send()
        .await
        .unwrap();
    assert!(revoke.status().is_success());
    let identity = saved(directory.path(), &["identity"], Value::Null).await;
    assert!(!identity.status.success());
    assert!(String::from_utf8_lossy(&identity.stderr).contains("AUTHENTICATION_FAILED"));
    assert!(!String::from_utf8_lossy(&identity.stderr).contains(token));
    assert!(identity.stdout.is_empty());
    let renewed = login(directory.path(), &origin, password, false).await;
    assert_eq!(renewed["exit"], 0, "{}", renewed["transcript"]);
    let identity = saved(directory.path(), &["identity"], Value::Null).await;
    assert!(identity.status.success());
    let renewed: contracts::auth::CliDevice = serde_json::from_slice(&identity.stdout).unwrap();
    assert_ne!(renewed.id, device.id);
    let valid_profile = fs::read_to_string(&path).unwrap();
    for (invalid, mode) in [
        ("{", 0o600),
        ("{\"schema_version\":0}", 0o600),
        (valid_profile.as_str(), 0o644),
    ] {
        fs::write(&path, invalid).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(mode)).unwrap();
        assert!(!saved(directory.path(), &["login"], Value::Null)
            .await
            .status
            .success());
        let replaced = login(directory.path(), &origin, password, true).await;
        assert_eq!(replaced["exit"], 0, "{}", replaced["transcript"]);
        assert_eq!(
            fs::metadata(&path).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert!(saved(directory.path(), &["identity"], Value::Null)
            .await
            .status
            .success());
    }
}

#[tokio::test]
async fn repeated_login_persists_the_replacement_ca_without_registering_another_device() {
    use tokio::io::{AsyncBufReadExt, BufReader};
    let directory = tempfile::tempdir().unwrap();
    let root = directory.path();
    // Real native TLS with a disposable CA; the fixture accepts only device GETs.
    fs::write(root.join("extensions.cnf"), "subjectAltName=DNS:localhost\nbasicConstraints=critical,CA:FALSE\nkeyUsage=critical,digitalSignature\nextendedKeyUsage=serverAuth\n").unwrap();
    for arguments in [
        vec![
            "req",
            "-x509",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "ca.key",
            "-out",
            "ca.pem",
            "-days",
            "1",
            "-subj",
            "/CN=CLI regression CA",
            "-addext",
            "basicConstraints=critical,CA:TRUE",
        ],
        vec![
            "req",
            "-new",
            "-newkey",
            "ec",
            "-pkeyopt",
            "ec_paramgen_curve:P-256",
            "-nodes",
            "-keyout",
            "server.key",
            "-out",
            "server.csr",
            "-subj",
            "/CN=localhost",
        ],
        vec![
            "x509",
            "-req",
            "-in",
            "server.csr",
            "-CA",
            "ca.pem",
            "-CAkey",
            "ca.key",
            "-CAcreateserial",
            "-out",
            "server.pem",
            "-days",
            "1",
            "-extfile",
            "extensions.cnf",
        ],
    ] {
        assert!(Command::new("openssl")
            .current_dir(root)
            .args(arguments)
            .output()
            .await
            .unwrap()
            .status
            .success());
    }
    let device_id = contracts::Id::new();
    let token = integrations::authentication::format_cli_token(
        device_id,
        &integrations::authentication::random_capability(),
    )
    .unwrap();
    let device = json!({"schema_version":1,"id":device_id,"name":"TLS device",
        "created_at":"2026-09-26T00:00:00Z","last_used_at":"2026-09-26T00:00:00Z"});
    let mut listener = Command::new("python3")
        .current_dir(root)
        .env_clear()
        .args([
            "-c",
            r#"
import http.server, json, ssl, sys
config = json.load(sys.stdin)
class Handler(http.server.BaseHTTPRequestHandler):
    def do_GET(self):
        assert self.path == '/api/v2/auth/cli/session'
        assert self.headers['Authorization'] == 'Bearer ' + config['token']
        assert self.headers['Cookie'] is None
        body = json.dumps(config['device']).encode()
        self.send_response(200)
        self.send_header('Content-Type', 'application/json')
        self.send_header('Content-Length', str(len(body)))
        self.end_headers()
        self.wfile.write(body)
    def log_message(self, *args): pass
server = http.server.HTTPServer(('127.0.0.1', 0), Handler)
context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
context.load_cert_chain('server.pem', 'server.key')
server.socket = context.wrap_socket(server.socket, server_side=True)
print(server.server_port, flush=True)
server.serve_forever()
"#,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .unwrap();
    listener
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(&json!({"token":token,"device":device})).unwrap())
        .await
        .unwrap();
    let mut line = String::new();
    tokio::time::timeout(
        Duration::from_secs(5),
        BufReader::new(listener.stdout.take().unwrap()).read_line(&mut line),
    )
    .await
    .unwrap()
    .unwrap();
    let port: u16 = line.trim().parse().unwrap();
    let path = root.join("quazonai/client.json");
    fs::create_dir(path.parent().unwrap()).unwrap();
    fs::set_permissions(path.parent().unwrap(), fs::Permissions::from_mode(0o700)).unwrap();
    fs::write(
        &path,
        serde_json::to_vec(&json!({"schema_version":1,
        "origin":format!("https://localhost:{port}"),"token":token,"development_http":false,
        "ca_certificate":root.join("obsolete.pem")}))
        .unwrap(),
    )
    .unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let confirmed = saved(root, &["--ca-certificate", "ca.pem", "login"], Value::Null).await;
    assert!(
        confirmed.status.success(),
        "{}",
        String::from_utf8_lossy(&confirmed.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&confirmed.stdout).unwrap(),
        device
    );
    let profile: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
    assert_eq!(
        profile["ca_certificate"],
        fs::canonicalize(root.join("ca.pem"))
            .unwrap()
            .to_str()
            .unwrap()
    );
    assert_eq!(profile["token"], token);
    assert_eq!(
        fs::metadata(&path).unwrap().permissions().mode() & 0o777,
        0o600
    );
    let identity = saved(root, &["identity"], Value::Null).await;
    assert!(
        identity.status.success(),
        "{}",
        String::from_utf8_lossy(&identity.stderr)
    );
    assert_eq!(
        serde_json::from_slice::<Value>(&identity.stdout).unwrap(),
        device
    );
    listener.kill().await.unwrap();
    listener.wait().await.unwrap();
}
