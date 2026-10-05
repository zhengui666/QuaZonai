//! Execute the actual one-job-per-process boundary, without inherited credentials.
use std::{
    ffi::OsStr,
    io::Write,
    process::{Command, Output, Stdio},
};

pub fn command(args: &[&OsStr], value: &impl serde::Serialize) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_job"))
        .args(args)
        .env_clear()
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(value).unwrap())
        .unwrap();
    let output = child.wait_with_output().unwrap();
    if !output.status.success() {
        eprintln!(
            "native stage={}; status={}; stderr: {}",
            args.first()
                .copied()
                .unwrap_or(OsStr::new("unknown"))
                .to_string_lossy(),
            output.status,
            String::from_utf8_lossy(&output.stderr)
        );
    }
    output
}
