//! Fixed exec bridge strips user-manager environment before the official native binary.
use super::{NativeFailure, Result};
use std::{collections::BTreeSet, path::Path};

pub(super) fn environment_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 200
        && name
            .bytes()
            .enumerate()
            .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
}
/// Trusted local launcher only. No HTTP route, provider call or arbitrary command arguments.
/// Environment VALUES travel via native service environment, never argv or logs.
#[cfg(unix)]
pub fn exec(binary: &Path, names: &[String]) -> Result<()> {
    use std::os::unix::process::CommandExt;
    if !binary.is_absolute()
        || !binary.is_file()
        || names.len() > 128
        || names.is_empty()
        || names.iter().any(|name| !environment_name(name))
        || names.iter().collect::<BTreeSet<_>>().len() != names.len()
    {
        return Err(NativeFailure::Configuration);
    }
    let values = names
        .iter()
        .map(|name| {
            std::env::var_os(name)
                .map(|value| (name, value))
                .ok_or(NativeFailure::Configuration)
        })
        .collect::<Result<Vec<_>>>()?;
    let _error = std::process::Command::new(binary)
        .arg("app-server")
        .env_clear()
        .envs(values)
        .exec();
    Err(NativeFailure::Unavailable)
}
