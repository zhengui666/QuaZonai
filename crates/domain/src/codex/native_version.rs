//! Read the build-version token returned by the pinned native Codex initializer.
//! Upstream rust-v0.144.4 formats it as `originator/CARGO_PKG_VERSION (...)`.

pub fn verified_codex_version<'a>(
    user_agent: &'a str,
    expected_originator: &str,
    expected_version: &str,
) -> Result<&'a str, &'static str> {
    // The native product prefix has no leading whitespace or control bytes.
    // Restrict only that prefix: platform/terminal suffixes are not versions.
    let product = user_agent
        .split_once(' ')
        .map_or(user_agent, |(product, _)| product);
    let (originator, observed) = product
        .split_once('/')
        .ok_or("NATIVE_CODEX_VERSION_MISMATCH")?;
    if originator != expected_originator || observed != expected_version || observed.is_empty() {
        return Err("NATIVE_CODEX_VERSION_MISMATCH");
    }
    Ok(observed)
}

#[cfg(test)]
mod tests {
    use super::verified_codex_version;

    #[test]
    fn returns_the_observed_exact_build_token() {
        let agent = "qz_w0_contract/0.144.4 (Debian 18; x86_64) terminal/1.0";
        assert_eq!(
            verified_codex_version(agent, "qz_w0_contract", "0.144.4"),
            Ok("0.144.4")
        );
        assert_eq!(
            verified_codex_version("codex_cli_rs/0.144.4", "codex_cli_rs", "0.144.4"),
            Ok("0.144.4")
        );
    }

    #[test]
    fn rejects_substrings_suffixes_wrong_products_and_missing_versions() {
        for agent in [
            "qz_w0_contract/0.144.40 (Linux)",
            "qz_w0_contract/10.144.4 (Linux)",
            "qz_w0_contract/0.144.4-alpha (Linux)",
            "qz_w0_contract/0.144.4+other (Linux)",
            "qz_w0_contract/0.144.3 (version 0.144.4)",
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
                verified_codex_version(agent, "qz_w0_contract", "0.144.4"),
                Err("NATIVE_CODEX_VERSION_MISMATCH"),
                "{agent:?}"
            );
        }
    }
}
