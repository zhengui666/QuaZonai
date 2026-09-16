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
    child.wait_with_output().unwrap()
}
