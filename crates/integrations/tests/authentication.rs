use contracts::Id;
use integrations::authentication::*;
use integrations::secrets::{SecretError, SecretVault};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
};

#[test]
fn native_totp_matches_rfc6238_sha1_vectors_and_rejects_bad_wire_codes() {
    // RFC 6238 Appendix B, SHA-1 vectors reduced to the configured six digits.
    // https://www.rfc-editor.org/rfc/rfc6238#appendix-B
    let secret = b"12345678901234567890";
    for (time, code) in [
        (59, "287082"),
        (1111111109, "081804"),
        (1111111111, "050471"),
        (1234567890, "005924"),
        (2000000000, "279037"),
        (20000000000, "353130"),
    ] {
        assert_eq!(accepted_step(secret, code, time).unwrap(), Some(time / 30));
        assert_eq!(
            accepted_step(secret, code, time / 30 * 30 + 30).unwrap(),
            Some(time / 30)
        );
        assert_eq!(
            accepted_step(secret, code, time / 30 * 30 + 60).unwrap(),
            None
        );
    }
    for code in [
        "",
        "12345",
        "1234567",
        "１２３４５６",
        " 287082",
        "287082\n",
        "abcdef",
    ] {
        assert_eq!(accepted_step(secret, code, 59).unwrap(), None);
    }
    assert_eq!(accepted_step(secret, "287082", -1).unwrap(), None);
    assert_eq!(accepted_step(secret, "287082", 0).unwrap(), None);
    assert!(accepted_step(b"short", "287082", 59).is_err());
}

#[test]
fn native_salted_verifier_and_generated_secret_do_not_reuse_material() {
    let first = random_capability();
    let second = random_capability();
    assert_eq!(first.len(), 43);
    assert_ne!(first, second);
    let verifier = capability_verifier(&first).unwrap();
    assert!(verifier.starts_with("$argon2id$"));
    assert_ne!(verifier, capability_verifier(&first).unwrap());
    assert!(verify_capability(&first, &verifier));
    assert!(!verify_capability(&second, &verifier));
    assert!(!verify_capability("", &verifier));
    assert!(!verify_capability(&first, "invalid-verifier"));
    assert!(capability_verifier("invalid").is_err());
    let a = new_totp_secret().unwrap();
    let b = new_totp_secret().unwrap();
    assert!(a.len() >= 20);
    assert_ne!(a, b);
    let uri = provisioning_uri(&a).unwrap();
    assert!(uri.starts_with("otpauth://totp/"));
    assert!(uri.contains("issuer=QuaZonai"));
}

#[test]
fn encrypted_vault_rejects_tamper_purpose_substitution_and_symlink_escape() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("vault");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let key = base.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    assert!(SecretVault::initialize_key(&key).is_err());
    let vault = SecretVault::open(&root, &key).unwrap();
    let id = vault.put("TOTP", b"non-production-secret-fixture").unwrap();
    let ciphertext = fs::read(root.join(id.to_string())).unwrap();
    assert!(!ciphertext
        .windows(29)
        .any(|b| b == b"non-production-secret-fixture"));
    assert_eq!(
        vault.read(id, "TOTP").unwrap(),
        b"non-production-secret-fixture"
    );
    assert!(matches!(
        vault.read(id, "RUNTIME"),
        Err(SecretError::Authentication)
    ));
    let other = Id::new();
    fs::write(root.join(other.to_string()), &ciphertext).unwrap();
    assert!(matches!(
        vault.read(other, "TOTP"),
        Err(SecretError::Authentication)
    ));
    let link = Id::new();
    symlink(&key, root.join(link.to_string())).unwrap();
    assert!(vault.read(link, "TOTP").is_err());
    let link_inside = Id::new();
    symlink(id.to_string(), root.join(link_inside.to_string())).unwrap();
    assert!(vault.read(link_inside, "TOTP").is_err());
    let mut damaged = ciphertext;
    *damaged.last_mut().unwrap() ^= 1;
    fs::set_permissions(root.join(id.to_string()), fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join(id.to_string()), damaged).unwrap();
    assert!(matches!(
        vault.read(id, "TOTP"),
        Err(SecretError::Authentication)
    ));
    assert!(vault.put("TOTP", &[]).is_err());
    assert!(vault.put("TOTP", &vec![0; 65537]).is_err());
    assert!(vault.put("../escape", b"data").is_err());
    fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(SecretVault::open(&root, &key).is_err());
}
