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
    assert_eq!(reopened.read(id, "TOTP").unwrap(), b"disposable-reopen-fixture");
}
