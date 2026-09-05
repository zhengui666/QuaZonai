//! W0 probe of the pinned, real Codex binary. Not a production App Server client.
//! Uses a new unauthenticated profile; never opens a user profile or starts a turn.

use std::collections::BTreeSet;
use std::error::Error;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, ExitCode, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

use serde_json::{Value, json};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_FRAME_BYTES: u64 = 1_048_576;
const MAX_CATALOG_PAGES: usize = 128;

struct Scratch(PathBuf);
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct Process(Child);
impl Drop for Process {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn command(binary: &Path, home: &Path, cwd: &Path) -> Command {
    let mut cmd = Command::new(binary);
    // The probe must not inherit provider credentials, a developer profile or proxy.
    cmd.env_clear()
        .env("PATH", "/usr/local/bin:/usr/bin:/bin")
        .env("HOME", home)
        .env("CODEX_HOME", home)
        .env("RUST_LOG", "error")
        .env("LANG", "C.UTF-8")
        .current_dir(cwd)
        .stderr(Stdio::null());
    cmd
}

fn read_frame(reader: &mut impl BufRead) -> Result<Option<Value>> {
    let mut bytes = Vec::new();
    reader
        .take(MAX_FRAME_BYTES + 1)
        .read_until(b'\n', &mut bytes)?;
    if bytes.is_empty() {
        return Ok(None);
    }
    if bytes.len() as u64 > MAX_FRAME_BYTES || bytes.last() != Some(&b'\n') {
        return Err("oversized or unterminated App Server frame".into());
    }
    Ok(Some(serde_json::from_slice(&bytes)?))
}

fn send(stdin: &mut ChildStdin, frame: &Value) -> Result<()> {
    serde_json::to_writer(&mut *stdin, frame)?;
    stdin.write_all(b"\n")?;
    stdin.flush()?;
    Ok(())
}

fn request(
    stdin: &mut ChildStdin,
    rx: &Receiver<std::result::Result<Value, String>>,
    deadline: Instant,
    id: u64,
    method: &str,
    params: Value,
) -> Result<Value> {
    send(
        stdin,
        &json!({"id": id, "method": method, "params": params}),
    )?;
    loop {
        let remaining = deadline
            .checked_duration_since(Instant::now())
            .ok_or("probe deadline exceeded")?;
        let response = rx
            .recv_timeout(remaining)?
            .map_err(|_| "invalid native App Server stream")?;
        if response.get("id") == Some(&json!(id)) {
            if response.get("error").is_some() {
                // Do not log arbitrary server messages or credentials.
                return Err(format!("native request failed: {method}").into());
            }
            return response
                .get("result")
                .cloned()
                .ok_or_else(|| "missing result".into());
        }
        if response.get("id").is_some() {
            return Err("unexpected server request or response ID during read-only probe".into());
        }
        if !response.get("method").is_some_and(Value::is_string) {
            return Err("unrecognized App Server notification".into());
        }
    }
}

fn validate_schemas(path: &Path) -> Result<usize> {
    let mut count = 0;
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            count += validate_schemas(&entry.path())?;
        } else if entry
            .path()
            .extension()
            .is_some_and(|extension| extension == "json")
        {
            let _: Value = serde_json::from_reader(File::open(entry.path())?)?;
            count += 1;
        }
    }
    Ok(count)
}

fn verify(binary: &Path, output: &Path) -> Result<()> {
    if !binary.is_absolute() || !binary.is_file() || !output.is_absolute() {
        return Err("binary and new output directory must be absolute paths".into());
    }
    fs::create_dir(output)?;
    let scratch = Scratch(output.join(".unauthenticated-profile"));
    fs::create_dir(&scratch.0)?;
    let version = command(binary, &scratch.0, &scratch.0)
        .arg("--version")
        .output()?;
    if !version.status.success() {
        return Err("native version command failed".into());
    }
    let version = String::from_utf8(version.stdout)?;
    if version.trim() != "codex-cli 0.144.4" {
        return Err("probe requires the pinned Codex 0.144.4 binary".into());
    }
    let schema_dir = output.join("native-schema");
    let generated = command(binary, &scratch.0, &scratch.0)
        .args(["app-server", "generate-json-schema", "--out"])
        .arg(&schema_dir)
        .stdout(Stdio::null())
        .status()?;
    if !generated.success() || validate_schemas(&schema_dir)? == 0 {
        return Err("native protocol schema generation failed".into());
    }
    let mut process = Process(
        command(binary, &scratch.0, &scratch.0)
            .args(["app-server", "--listen", "stdio://"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()?,
    );
    let stdout = process.0.stdout.take().ok_or("missing native stdout")?;
    let mut stdin = process.0.stdin.take().ok_or("missing native stdin")?;
    let (tx, rx) = mpsc::sync_channel(16);
    let reader = std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            match read_frame(&mut reader) {
                Ok(Some(frame)) => {
                    if tx.send(Ok(frame)).is_err() {
                        break;
                    }
                }
                Ok(None) => break,
                Err(_) => {
                    let _ = tx.send(Err("invalid stream".into()));
                    break;
                }
            }
        }
    });
    let deadline = Instant::now() + Duration::from_secs(60);
    let initialized = request(
        &mut stdin,
        &rx,
        deadline,
        1,
        "initialize",
        json!({
            "clientInfo": {"name": "qz_w0_probe", "title": "QuaZonai W0 probe", "version": "0.2.0-dev.1"}
        }),
    )?;
    if !initialized.get("userAgent").is_some_and(Value::is_string) {
        return Err("initialize did not return the native server identity".into());
    }
    send(&mut stdin, &json!({"method": "initialized"}))?;
    let account = request(
        &mut stdin,
        &rx,
        deadline,
        2,
        "account/read",
        json!({"refreshToken": false}),
    )?;
    if account.get("account") != Some(&Value::Null) {
        return Err("fresh probe profile must not be authenticated".into());
    }

    let mut models = Vec::new();
    let mut seen_models = BTreeSet::new();
    let mut seen_cursors = BTreeSet::new();
    let mut cursor = Value::Null;
    let mut pages = 0;
    loop {
        if pages >= MAX_CATALOG_PAGES {
            return Err("catalog did not terminate within the probe page budget".into());
        }
        let page = request(
            &mut stdin,
            &rx,
            deadline,
            3 + pages as u64,
            "model/list",
            json!({
                "limit": 1, "cursor": cursor, "includeHidden": false
            }),
        )?;
        pages += 1;
        for model in page
            .get("data")
            .and_then(Value::as_array)
            .ok_or("model/list missing data")?
        {
            let id = model
                .get("id")
                .and_then(Value::as_str)
                .ok_or("model ID missing")?;
            if id.is_empty() || !seen_models.insert(id.to_owned()) {
                return Err("empty or duplicate native model ID".into());
            }
            let options = model
                .get("supportedReasoningEfforts")
                .and_then(Value::as_array)
                .ok_or("native reasoning capability list missing")?;
            let mut efforts = Vec::new();
            for option in options {
                let effort = option
                    .get("reasoningEffort")
                    .and_then(Value::as_str)
                    .filter(|effort| !effort.is_empty())
                    .ok_or("empty reasoning effort")?;
                efforts.push(effort.to_owned());
            }
            models.push(json!({"model_id": id, "reasoning_efforts": efforts}));
        }
        match page.get("nextCursor") {
            Some(Value::Null) => break,
            Some(Value::String(next)) if !next.is_empty() && seen_cursors.insert(next.clone()) => {
                cursor = Value::String(next.clone());
            }
            _ => return Err("missing, invalid or repeated native catalog cursor".into()),
        }
    }
    drop(stdin);
    drop(rx);
    let exit_deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if let Some(exit) = process.0.try_wait()? {
            if !exit.success() {
                return Err("native App Server failed during shutdown".into());
            }
            break;
        }
        if Instant::now() >= exit_deadline {
            return Err("native App Server did not shut down after EOF".into());
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    reader.join().map_err(|_| "native stream reader panicked")?;
    drop(scratch);
    let report = json!({
        "schema_version": 1, "kind": "W0_CODEX_PROTOCOL_PROBE",
        "codex_version": version.trim(), "transport": "stdio",
        "native_schema_files": validate_schemas(&schema_dir)?,
        "unauthenticated_profile": true, "catalog_complete": true,
        "model_pages": pages, "models": models, "native_shutdown": "SUCCESS",
        "scope": "Unauthenticated native handshake/catalog/schema only; not login or model-turn acceptance"
    });
    serde_json::to_writer_pretty(File::create(output.join("evidence.json"))?, &report)?;
    println!("{report}");
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<_> = std::env::args_os().collect();
    if args.len() != 3 {
        eprintln!("Usage: codex-probe <absolute-codex-binary> <new-absolute-output-directory>");
        return ExitCode::from(2);
    }
    match verify(Path::new(&args[1]), Path::new(&args[2])) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Codex W0 probe failed: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::read_frame;
    use std::io::Cursor;

    #[test]
    fn rejects_truncated_invalid_and_oversized_frames() {
        assert!(
            read_frame(&mut Cursor::new(b"{\"id\":1}\n"))
                .unwrap()
                .is_some()
        );
        assert!(read_frame(&mut Cursor::new(b"{\"id\":1}")).is_err());
        assert!(read_frame(&mut Cursor::new(b"not-json\n")).is_err());
        assert!(read_frame(&mut Cursor::new(vec![b'a'; 1_048_577])).is_err());
        assert!(read_frame(&mut Cursor::new(b"")).unwrap().is_none());
    }
}
