use contracts::Id;
use integrations::authentication::*;
use integrations::secrets::{SecretError, SecretVault};
use std::{
    fs,
    os::unix::fs::{symlink, PermissionsExt},
};

#[test]
fn native_salted_verifier_and_random_capabilities_do_not_reuse_material() {
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
    let id = vault
        .put("MACHINE_VERIFIER", b"non-production-secret-fixture")
        .unwrap();
    let ciphertext = fs::read(root.join(id.to_string())).unwrap();
    assert!(!ciphertext
        .windows(29)
        .any(|b| b == b"non-production-secret-fixture"));
    assert_eq!(
        vault.read(id, "MACHINE_VERIFIER").unwrap(),
        b"non-production-secret-fixture"
    );
    assert!(matches!(
        vault.read(id, "RUNTIME"),
        Err(SecretError::Authentication)
    ));
    let other = Id::new();
    fs::write(root.join(other.to_string()), &ciphertext).unwrap();
    assert!(matches!(
        vault.read(other, "MACHINE_VERIFIER"),
        Err(SecretError::Authentication)
    ));
    let link = Id::new();
    symlink(&key, root.join(link.to_string())).unwrap();
    assert!(vault.read(link, "MACHINE_VERIFIER").is_err());
    let link_inside = Id::new();
    symlink(id.to_string(), root.join(link_inside.to_string())).unwrap();
    assert!(vault.read(link_inside, "MACHINE_VERIFIER").is_err());
    let mut damaged = ciphertext;
    *damaged.last_mut().unwrap() ^= 1;
    fs::set_permissions(root.join(id.to_string()), fs::Permissions::from_mode(0o600)).unwrap();
    fs::write(root.join(id.to_string()), damaged).unwrap();
    assert!(matches!(
        vault.read(id, "MACHINE_VERIFIER"),
        Err(SecretError::Authentication)
    ));
    assert!(vault.put("MACHINE_VERIFIER", &[]).is_err());
    assert!(vault.put("MACHINE_VERIFIER", &vec![0; 65537]).is_err());
    assert!(vault.put("../escape", b"data").is_err());
    fs::set_permissions(&key, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(SecretVault::open(&root, &key).is_err());
}
