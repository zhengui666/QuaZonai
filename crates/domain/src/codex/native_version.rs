//! Read the build-version token returned by the native Codex initializer.
//! The observed version is a binding for persisted sessions, not a release gate.

pub fn valid_codex_version(version: &str) -> bool {
    !version.is_empty()
        && version.len() <= 64
        && version
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._+-".contains(&byte))
}

pub fn verified_codex_version<'a>(
    user_agent: &'a str,
    expected_originator: &str,
) -> Result<&'a str, &'static str> {
    // The native product prefix has no leading whitespace or control bytes.
    // Restrict only that prefix: platform/terminal suffixes are not versions.
    let product = user_agent
        .split_once(' ')
        .map_or(user_agent, |(product, _)| product);
    let (originator, observed) = product
        .split_once('/')
        .ok_or("NATIVE_CODEX_VERSION_MISMATCH")?;
    if originator != expected_originator || !valid_codex_version(observed) {
        return Err("NATIVE_CODEX_VERSION_MISMATCH");
    }
    Ok(observed)
}

#[cfg(test)]
mod tests {
    use super::{valid_codex_version, verified_codex_version};

    #[test]
    fn returns_the_observed_build_token_for_current_and_future_versions() {
        let agent = "qz_w0_contract/0.156.1 (Debian 18; x86_64) terminal/1.0";
        assert_eq!(
            verified_codex_version(agent, "qz_w0_contract"),
            Ok("0.156.1")
        );
        assert_eq!(
            verified_codex_version("codex_cli_rs/0.999.0-alpha.9+local", "codex_cli_rs"),
            Ok("0.999.0-alpha.9+local")
        );
        assert!(valid_codex_version("2027.1.0"));
    }

    #[test]
    fn rejects_malformed_versions_wrong_products_and_missing_versions() {
        for agent in [
            "wrong/0.144.4 (Linux)",
            "qz_w0_contract/0.144.4/other (Linux)",
            "qz_w0_contract/0.144.4\t(Linux)",
            "qz_w0_contract/0.144.4\n(Linux)",
            " qz_w0_contract/0.144.4 (Linux)",
            "qz_w0_contract/ (0.144.4)",
            "qz_w0_contract (0.144.4)",
            "0.144.4",
            "",
        ] {
            assert_eq!(
                verified_codex_version(agent, "qz_w0_contract"),
                Err("NATIVE_CODEX_VERSION_MISMATCH"),
                "{agent:?}"
            );
        }
        assert!(!valid_codex_version(&"x".repeat(65)));
    }
}
