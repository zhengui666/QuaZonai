use integrations::secrets::SecretVault;
use std::{fs, os::unix::fs::PermissionsExt};

#[test]
fn published_secret_is_read_only_and_readable_after_reopening_the_vault() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("vault");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let key = base.path().join("master.key");
    SecretVault::initialize_key(&key).unwrap();
    let vault = SecretVault::open(&root, &key).unwrap();
    let id = vault.put("TOTP", b"disposable-reopen-fixture").unwrap();
    drop(vault);
    let metadata = fs::metadata(root.join(id.to_string())).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o400);
    let reopened = SecretVault::open(&root, &key).unwrap();
    assert_eq!(
        reopened.read(id, "TOTP").unwrap(),
        b"disposable-reopen-fixture"
    );
}

#[test]
fn verifier_cleanup_cannot_remove_other_purposes_symlinks_or_unauthenticated_files() {
    use contracts::Id;
    use std::os::unix::fs::symlink;
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("vault");
    fs::create_dir(&root).unwrap();
    fs::set_permissions(&root, fs::Permissions::from_mode(0o700)).unwrap();
    let key = base.path().join("key");
    SecretVault::initialize_key(&key).unwrap();
    let vault = SecretVault::open(&root, &key).unwrap();
    let verifier = vault
        .put("MACHINE_VERIFIER", b"unpublished fixture")
        .unwrap();
    let totp = vault.put("TOTP", b"authentication secret").unwrap();
    let session = vault.put("SESSION_KEY", b"native cookie key").unwrap();
    let arbitrary = Id::new();
    fs::write(
        root.join(arbitrary.to_string()),
        b"not authenticated ciphertext",
    )
    .unwrap();
    let link = Id::new();
    symlink(root.join(verifier.to_string()), root.join(link.to_string())).unwrap();
    assert_eq!(vault.machine_verifier_ids().unwrap(), vec![verifier]);
    for id in [totp, session, arbitrary, link] {
        assert!(vault.remove_unpublished_verifier(id).is_err());
        assert!(root.join(id.to_string()).symlink_metadata().is_ok());
    }
    vault.remove_unpublished_verifier(verifier).unwrap();
    vault.remove_unpublished_verifier(verifier).unwrap();
    assert!(!root.join(verifier.to_string()).exists());
    assert_eq!(vault.read(totp, "TOTP").unwrap(), b"authentication secret");
    assert!(vault.put("UNSUPPORTED", b"not valid").is_err());
}
