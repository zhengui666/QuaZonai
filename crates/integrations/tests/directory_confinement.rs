//! Real Linux filesystem boundaries under the kernel and both native fallbacks.
//! All adversarial targets are disposable siblings inside one private TempDir.
#![cfg(target_os = "linux")]
#![forbid(unsafe_code)]

use cap_std::{ambient_authority, fs::Dir};
use contracts::{DbCounter, Id};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use rustix::fs::{openat2, Mode, OFlags, ResolveFlags, CWD};
use seccompiler::{BpfProgram, SeccompAction, SeccompFilter};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::{symlink, PermissionsExt},
    process::Command,
};

const CHILD_MODE: &str = "QUAZONAI_CAP_CONFINEMENT_TEST_CHILD";

fn isolated(test: &str, mode: &str) {
    if std::env::var(CHILD_MODE).ok().as_deref() != Some(mode) {
        // Fresh exec avoids cap-primitives' process-wide backend cache and keeps
        // the irreversible test-only seccomp filter away from all other tests.
        let child = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--nocapture", "--test-threads=1"])
            .env_clear()
            .env(CHILD_MODE, mode)
            .output()
            .unwrap();
        assert!(
            child.status.success(),
            "{mode}: child failed\n{}\n{}",
            String::from_utf8_lossy(&child.stdout),
            String::from_utf8_lossy(&child.stderr)
        );
        assert!(String::from_utf8_lossy(&child.stdout).contains("CONFINEMENT_MATRIX_PASSED"));
        return;
    }
    let errno = match mode {
        "native" => None,
        "enosys" => Some(libc::ENOSYS),
        "eperm" => Some(libc::EPERM),
        _ => panic!("unknown test backend"),
    };
    if let Some(errno) = errno {
        let filter: BpfProgram = SeccompFilter::new(
            BTreeMap::from([(libc::SYS_openat2, Vec::new())]),
            SeccompAction::Allow,
            SeccompAction::Errno(errno as u32),
            std::env::consts::ARCH.try_into().unwrap(),
        )
        .unwrap()
        .try_into()
        .unwrap();
        seccompiler::apply_filter(&filter).unwrap();
        let result = openat2(
            CWD,
            ".",
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::CLOEXEC,
            Mode::empty(),
            ResolveFlags::empty(),
        );
        assert_eq!(result.unwrap_err().raw_os_error(), errno);
    }
    matrix();
    println!("CONFINEMENT_MATRIX_PASSED backend={mode}");
}

fn matrix() {
    let base = tempfile::tempdir().unwrap();
    let root = base.path().join("allowed");
    let outside = base.path().join("outside-fixture");
    fs::create_dir(&root).unwrap();
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("sentinel"), b"unchanged fixture").unwrap();
    fs::create_dir(root.join("inner")).unwrap();
    fs::write(root.join("inner/value"), b"allowed bytes").unwrap();
    let dir = Dir::open_ambient_dir(&root, ambient_authority()).unwrap();

    // The precise upstream advisory pattern: an absolute link reached through
    // another link whose target ends in '/', plus ordinary escaping variants.
    symlink(&outside, root.join("absolute-link")).unwrap();
    symlink("absolute-link/", root.join("trailing-link")).unwrap();
    symlink("../outside-fixture", root.join("relative-escape")).unwrap();
    symlink("relative-escape/", root.join("relative-trailing")).unwrap();
    symlink("inner/", root.join("inside-link")).unwrap();
    for path in [
        "absolute-link",
        "absolute-link/",
        "trailing-link",
        "trailing-link/",
        "relative-escape",
        "relative-trailing",
        "../outside-fixture",
    ] {
        assert!(dir.open_dir(path).is_err(), "escaped through {path}");
        assert!(dir.read(format!("{path}/sentinel")).is_err());
        assert!(dir.create(format!("{path}/unexpected")).is_err());
    }
    assert_eq!(dir.read("inner/value").unwrap(), b"allowed bytes");
    assert_eq!(dir.read("inside-link/value").unwrap(), b"allowed bytes");
    let inside = dir.open_dir("inside-link").unwrap();
    assert_eq!(inside.read("value").unwrap(), b"allowed bytes");
    assert_eq!(
        fs::read(outside.join("sentinel")).unwrap(),
        b"unchanged fixture"
    );
    assert!(!outside.join("unexpected").exists());

    // Exercise the actual product adapters under the same backend, not just an
    // upstream Dir. No production secret, database, model or user path is read.
    let artifacts = ArtifactStore::open(&base.path().join("artifacts")).unwrap();
    let id = Id::new();
    artifacts.put(id, b"native object").unwrap();
    assert_eq!(
        artifacts.read(id, DbCounter::new(13).unwrap()).unwrap(),
        b"native object"
    );
    assert!(artifacts.put(id, b"replacement").is_err());

    let vault_root = base.path().join("vault");
    fs::create_dir(&vault_root).unwrap();
    fs::set_permissions(&vault_root, fs::Permissions::from_mode(0o700)).unwrap();
    let key = base.path().join("test-master.key");
    SecretVault::initialize_key(&key).unwrap();
    let vault = SecretVault::open(&vault_root, &key).unwrap();
    let secret = vault.put("TOTP", b"disposable-test-value").unwrap();
    assert_eq!(
        vault.read(secret, "TOTP").unwrap(),
        b"disposable-test-value"
    );
    assert!(vault.read(secret, "SESSION_KEY").is_err());
}

#[test]
fn native_directory_confinement() {
    isolated("native_directory_confinement", "native");
}

#[test]
fn enosys_fallback_directory_confinement() {
    isolated("enosys_fallback_directory_confinement", "enosys");
}

#[test]
fn eperm_fallback_directory_confinement() {
    isolated("eperm_fallback_directory_confinement", "eperm");
}
