//! Image identity syntax retained for immutable native job inputs, not OCI orchestration.
pub fn pinned_image(value: &str) -> bool {
    let native_sha256 = |value: &str| {
        value.strip_prefix("sha256:").is_some_and(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
        })
    };
    // A local Docker content-addressed image ID is not a distribution repository.
    if value.starts_with("sha256:") {
        return native_sha256(value);
    }
    if value.len() > 512 || !value.bytes().all(|byte| (b'!'..=b'~').contains(&byte)) {
        return false;
    }
    // Reuse the OCI/Docker distribution parser; URL or filesystem-shaped strings
    // are not image names merely because each individual character looks allowed.
    oci_spec::distribution::Reference::try_from(value)
        .ok()
        .and_then(|reference| reference.digest().map(native_sha256))
        .unwrap_or(false)
}
