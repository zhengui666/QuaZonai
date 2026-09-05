//! Test-only stdio compatibility probe against the pinned native Codex binary.
//! No model inference, authentication, mutation API, or Agent loop is implemented.
//! This does not replace the protected real-account and same-thread tool tests.
use std::collections::BTreeSet;
use std::error::Error;
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::path::PathBuf;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{json, Value};

type Result<T> = std::result::Result<T, Box<dyn Error>>;
const MAX_FRAME: u64 = 2 * 1024 * 1024;

struct NativeServer {
    child: Child,
    input: ChildStdin,
    messages: Receiver<std::result::Result<Value, &'static str>>,
    next_id: u64,
    deadline: Instant,
}

impl NativeServer {
    fn start(binary: &str, home: &PathBuf) -> Result<Self> {
        let mut child = Command::new(binary)
            .arg("app-server")
            .env_clear()
            .env("PATH", std::env::var("PATH")?)
            .env("HOME", home)
            .env("CODEX_HOME", home)
            .env("RUST_LOG", "off")
            .current_dir(home)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let input = child.stdin.take().ok_or("MISSING_STDIN")?;
        let output = child.stdout.take().ok_or("MISSING_STDOUT")?;
        let (sender, messages) = mpsc::sync_channel(4);
        // Bounded frames and bounded queue: no unbounded notification transcript.
        thread::spawn(move || {
            let mut reader = BufReader::new(output);
            loop {
                let mut frame = Vec::new();
                let read = Read::take(&mut reader, MAX_FRAME + 1).read_until(b'\n', &mut frame);
                let value = match read {
                    Ok(0) => break,
                    Ok(n) if n as u64 <= MAX_FRAME && frame.last() == Some(&b'\n') => {
                        serde_json::from_slice(&frame).map_err(|_| "INVALID_NATIVE_FRAME")
                    }
                    _ => Err("INVALID_NATIVE_FRAME_SIZE"),
                };
                let failed = value.is_err();
                if sender.send(value).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            child,
            input,
            messages,
            next_id: 1,
            deadline: Instant::now() + Duration::from_secs(60),
        })
    }

    fn send(&mut self, value: Value) -> Result<()> {
        serde_json::to_writer(&mut self.input, &value)?;
        self.input.write_all(b"\n")?;
        self.input.flush()?;
        Ok(())
    }

    fn request(&mut self, method: &str, params: Value) -> Result<Value> {
        let id = self.next_id;
        self.next_id += 1;
        self.send(json!({"id": id, "method": method, "params": params}))?;
        for _ in 0..1024 {
            let remaining = self.deadline.saturating_duration_since(Instant::now());
            let message = self.messages.recv_timeout(remaining)??;
            // Never permit a server-initiated tool or approval in this probe.
            if message.get("id").is_some() && message.get("method").is_some() {
                return Err("UNEXPECTED_NATIVE_SERVER_REQUEST".into());
            }
            if message.get("id") == Some(&json!(id)) {
                if message.get("error").is_some() {
                    return Err("NATIVE_REQUEST_REJECTED".into());
                }
                return message
                    .get("result")
                    .cloned()
                    .ok_or("MISSING_NATIVE_RESULT".into());
            }
            if message.get("id").is_some() {
                return Err("UNEXPECTED_RESPONSE_ID".into());
            }
            // Public probe output contains no raw notifications, reasoning, auth
            // state, account identity or upstream error strings.
        }
        Err("EXCESSIVE_NATIVE_NOTIFICATIONS".into())
    }
}

impl Drop for NativeServer {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
        // Dropping the receiver unblocks the bounded reader on all error paths.
    }
}

fn probe() -> Result<()> {
    let binary = std::env::var("CODEX_NATIVE_BIN")?;
    let directory = PathBuf::from(std::env::var("CODEX_PROBE_DIR")?);
    fs::create_dir(&directory)?;
    let home = directory.join("native-home");
    fs::create_dir(&home)?;
    let mut server = NativeServer::start(&binary, &home)?;
    let initialized = server.request(
        "initialize",
        json!({"clientInfo": {"name": "qz_w0_contract", "title": "QZ native contract probe", "version": "2.0.0-dev.1"}}),
    )?;
    let user_agent = initialized
        .get("userAgent")
        .and_then(Value::as_str)
        .ok_or("MISSING_NATIVE_VERSION")?;
    if !user_agent.contains("0.144.4") {
        return Err("NATIVE_CODEX_VERSION_MISMATCH".into());
    }
    server.send(json!({"method": "initialized"}))?;
    let account = server.request("account/read", json!({"refreshToken": false}))?;
    // An isolated fixture HOME must not silently inherit a real credential.
    if account.get("account") != Some(&Value::Null) {
        return Err("UNEXPECTED_ACCOUNT_IN_FIXTURE_HOME".into());
    }

    let mut cursor = None::<String>;
    let mut cursors = BTreeSet::new();
    let mut model_ids = BTreeSet::new();
    let mut models = Vec::new();
    let mut pages = 0;
    loop {
        if pages >= 128 {
            return Err("NATIVE_MODEL_PAGINATION_LIMIT".into());
        }
        let mut params = json!({"limit": 1, "includeHidden": true});
        if let Some(value) = &cursor {
            params["cursor"] = json!(value);
        }
        let page = server.request("model/list", params)?;
        for model in page
            .get("data")
            .and_then(Value::as_array)
            .ok_or("MISSING_MODELS")?
        {
            let id = model
                .get("id")
                .and_then(Value::as_str)
                .ok_or("MISSING_MODEL_ID")?;
            if !model_ids.insert(id.to_owned()) {
                return Err("DUPLICATE_MODEL_IN_PAGINATION".into());
            }
            models.push(model.clone());
        }
        pages += 1;
        cursor = page
            .get("nextCursor")
            .and_then(Value::as_str)
            .map(str::to_owned);
        match &cursor {
            None => break,
            Some(value) if !cursors.insert(value.clone()) => {
                return Err("REPEATED_NATIVE_CURSOR".into())
            }
            _ => {}
        }
    }
    if models.is_empty() {
        return Err("EMPTY_NATIVE_MODEL_CATALOG".into());
    }
    let base = json!({"cwd": home.to_str().ok_or("INVALID_HOME")?, "approvalPolicy": "never", "sandbox": "read-only", "ephemeral": true});
    let default_thread = server.request("thread/start", base.clone())?;
    let default_id = default_thread
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or("MISSING_DEFAULT_THREAD")?;
    let model = models
        .iter()
        .find(|model| model["isDefault"] == true)
        .ok_or("MISSING_DEFAULT_MODEL")?;
    let effort = model
        .pointer("/supportedReasoningEfforts/0/reasoningEffort")
        .and_then(Value::as_str)
        .ok_or("MISSING_NATIVE_EFFORT")?;
    let mut configured = base;
    // model is deliberately omitted: reasoning effort is independent of an
    // explicit model override. No URL, provider or API key is supplied.
    configured["config"] = json!({"model_reasoning_effort": effort});
    let effort_thread = server.request("thread/start", configured)?;
    let effort_id = effort_thread
        .pointer("/thread/id")
        .and_then(Value::as_str)
        .ok_or("MISSING_EFFORT_THREAD")?;
    if default_id == effort_id || home.join("auth.json").exists() {
        return Err("NATIVE_THREAD_OR_AUTH_ISOLATION_FAILED".into());
    }
    drop(server);
    // No account identifier, thread ID, model reasoning, config dump, or secret.
    let report = json!({"schema_version": 1, "origin": "FIXTURE", "deliverable": false,
        "codex_version": "0.144.4", "model_pages": pages, "model_count": models.len(),
        "initialize": true, "unauthenticated_account_read": true,
        "default_thread_start": true, "effort_without_model_thread_start": true,
        "real_account_acceptance": false, "same_thread_inference_tool_loop": false});
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(directory.join("codex-probe.json"))?;
    serde_json::to_writer_pretty(file, &report)?;
    println!(
        "native Codex contract probe completed; origin=FIXTURE; real-account acceptance=false"
    );
    Ok(())
}

fn main() {
    if probe().is_err() {
        eprintln!("NATIVE_CODEX_CONTRACT_FAILED");
        std::process::exit(1);
    }
}
