//! Exact native object publication, not a separate cryptographic implementation.
use contracts::Id;
use integrations::secrets::SecretVault;
use std::{fs, os::unix::fs::DirBuilderExt};

fn vault() -> (tempfile::TempDir, SecretVault) {
    let root = tempfile::tempdir().unwrap();
    let objects = root.path().join("secrets");
    fs::DirBuilder::new().mode(0o700).create(&objects).unwrap();
    let key = root.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    let vault = SecretVault::open(&objects, &key).unwrap();
    (root, vault)
}

#[test]
fn caller_allocated_native_id_is_immutable_and_authenticated_by_purpose() {
    let (root, vault) = vault();
    let id = Id::new();
    assert_eq!(
        vault.put_at(id, "RUNTIME", b"first-native-secret").unwrap(),
        id
    );
    let original = fs::read(root.path().join("secrets").join(id.to_string())).unwrap();
    assert!(vault.put_at(id, "RUNTIME", b"replacement").is_err());
    assert!(vault.put_at(id, "DOWNSTREAM", b"replacement").is_err());
    assert_eq!(
        fs::read(root.path().join("secrets").join(id.to_string())).unwrap(),
        original
    );
    assert_eq!(vault.read(id, "RUNTIME").unwrap(), b"first-native-secret");
    assert!(vault.read(id, "DOWNSTREAM").is_err());
    assert!(vault.read(id, "TOTP").is_err());
    assert!(!original
        .windows(b"first-native-secret".len())
        .any(|bytes| bytes == b"first-native-secret"));
}

#[test]
fn ca_reference_does_not_grant_session_or_provider_secret_authority() {
    let (_root, vault) = vault();
    let id = vault.put("TLS_CA", b"native-ca-fixture-bytes").unwrap();
    assert_eq!(
        vault.read(id, "TLS_CA").unwrap(),
        b"native-ca-fixture-bytes"
    );
    for purpose in [
        "TOTP",
        "RUNTIME",
        "DOWNSTREAM",
        "CUSTOM_PROVIDER",
        "SESSION_KEY",
        "MACHINE_VERIFIER",
    ] {
        assert!(vault.read(id, purpose).is_err());
    }
    assert!(vault.put("UNKNOWN_PURPOSE", b"secret").is_err());
    assert!(vault.put_at(Id::new(), "TLS_CA", b"").is_err());
}
