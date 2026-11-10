//! The installed dispatcher and standalone binaries must expose the same native CLIs.
#![cfg(feature = "catalog-prepare")]
use std::{ffi::OsStr, process::Command};

fn equivalent(name: &str, args: &[&OsStr], code: i32) {
    let binary = match name {
        "catalog-prepare" => env!("CARGO_BIN_EXE_catalog-prepare"),
        _ => unreachable!(),
    };
    let standalone = Command::new(binary).args(args).output().unwrap();
    let selected = Command::new(env!("CARGO_BIN_EXE_source-tools"))
        .arg(name)
        .args(args)
        .output()
        .unwrap();
    assert_eq!(standalone.status.code(), Some(code), "{name} {args:?}");
    assert_eq!(selected.status, standalone.status, "{name} {args:?}");
    assert_eq!(selected.stdout, standalone.stdout, "{name} {args:?}");
    assert_eq!(selected.stderr, standalone.stderr, "{name} {args:?}");
}

#[test]
fn original_parsers_help_versions_errors_and_exit_codes_are_preserved() {
    for name in ["catalog-prepare"] {
        for flag in ["--help", "--version"] {
            equivalent(name, &[OsStr::new(flag)], 0);
        }
        equivalent(name, &[], 2);
        equivalent(name, &[OsStr::new("--not-an-option")], 2);
    }

    let directory = tempfile::tempdir().unwrap();
    let missing = directory
        .path()
        .join("missing [行情] ' ; $(no-command).json");
    let output = directory.path().join("output [行情]");
    equivalent(
        "catalog-prepare",
        &[
            OsStr::new("--catalog"),
            missing.as_os_str(),
            OsStr::new("--declaration"),
            missing.as_os_str(),
            OsStr::new("--selection"),
            missing.as_os_str(),
            OsStr::new("--output"),
            output.as_os_str(),
        ],
        1,
    );

    assert!(!output.exists());
}

#[cfg(unix)]
#[test]
fn non_utf8_paths_keep_the_original_failure_contract() {
    use std::os::unix::ffi::OsStringExt;
    let path = std::ffi::OsString::from_vec(b"/missing-native-path-\xff".to_vec());
    equivalent(
        "catalog-prepare",
        &[
            OsStr::new("--catalog"),
            &path,
            OsStr::new("--declaration"),
            &path,
            OsStr::new("--selection"),
            &path,
            OsStr::new("--output"),
            &path,
        ],
        1,
    );
}

#[test]
fn unknown_dispatch_does_not_enter_an_operator() {
    for args in [vec![], vec!["unknown"], vec!["--help"]] {
        let result = Command::new(env!("CARGO_BIN_EXE_source-tools"))
            .args(args)
            .output()
            .unwrap();
        assert_eq!(result.status.code(), Some(2));
        assert!(result.stdout.is_empty());
        assert_eq!(result.stderr, b"source-tools: expected catalog-prepare\n");
    }
}
