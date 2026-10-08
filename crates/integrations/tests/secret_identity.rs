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
    assert!(vault.read(id, "MACHINE_VERIFIER").is_err());
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
        "MACHINE_VERIFIER",
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

#[test]
fn large_ca_ciphertext_reopens_exactly_and_retains_identity_authentication() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let (root, vault) = vault();
    // The vault is opaque byte storage. Actual PEM validation is tested at HTTP.
    let plaintext = b"public-ca-capacity-fixture\n".repeat(4000);
    assert!(plaintext.len() > 65536);
    let id = vault.put("TLS_CA", &plaintext).unwrap();
    let path = root.path().join("secrets").join(id.to_string());
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o777, 0o400);
    let original = fs::read(&path).unwrap();
    assert!(!original.windows(plaintext.len()).any(|bytes| bytes == plaintext));
    assert!(vault.put_at(id, "TLS_CA", b"replacement").is_err());
    assert!(vault.put("RUNTIME", &plaintext).is_err());
    drop(vault);
    let reopened = SecretVault::open(&root.path().join("secrets"), &root.path().join("master.key")).unwrap();
    assert!(reopened.read(id, "TLS_CA").unwrap() == plaintext, "complete native CA bytes changed");
    for purpose in ["RUNTIME", "DOWNSTREAM", "SESSION_KEY", "MACHINE_VERIFIER"] {
        assert!(reopened.read(id, purpose).is_err());
    }
    let alias = Id::new();
    symlink(&path, root.path().join("secrets").join(alias.to_string())).unwrap();
    assert!(reopened.read(alias, "TLS_CA").is_err());
    let mut tampered = original;
    *tampered.last_mut().unwrap() ^= 1;
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(&path, tampered).unwrap();
    assert!(reopened.read(id, "TLS_CA").is_err());
}
