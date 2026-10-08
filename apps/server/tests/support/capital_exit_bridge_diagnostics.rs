//! Failure-only capture for the explicitly selected disposable native bridges.
//! Never read the bridge stdin, environment, authorization headers or config.
use std::path::Path;

pub fn failure(directory: &Path, label: &str, secrets: &[&str], files: &[&str]) -> String {
    let destination = std::env::var_os("QZ_TEST_EVIDENCE").map(std::path::PathBuf::from);
    let mut report = String::new();
    for name in files {
        let bytes = match std::fs::read(directory.join(name)) {
            Ok(bytes) => bytes,
            Err(error) => {
                report.push_str(&format!("\n{name}: unavailable ({error})"));
                continue;
            }
        };
        let mut text = String::from_utf8_lossy(&bytes).into_owned();
        // Redact before either persisting or truncating, so a token cannot be
        // exposed as a partial suffix in the bounded test-log diagnostic.
        for secret in secrets.iter().filter(|value| !value.is_empty()) {
            text = text.replace(*secret, "[REDACTED]");
        }
        if let Some(destination) = &destination {
            if let Err(error) = std::fs::write(destination.join(format!("{label}-{name}")), &text) {
                report.push_str(&format!("\n{name}: evidence copy failed ({error})"));
            }
        }
        let mut begin = text.len().saturating_sub(4096);
        while !text.is_char_boundary(begin) {
            begin += 1;
        }
        report.push_str(&format!(
            "\n{name} ({} bytes; bounded tail):\n{}",
            text.len(),
            &text[begin..]
        ));
    }
    report
}
