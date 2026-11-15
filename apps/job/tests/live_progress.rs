//! Retired progress environment must not silently recreate the old simulation path.
#[test]
fn retired_simulation_cannot_publish_legacy_progress() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("progress.ndjson");
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_job"))
        .arg("simulate")
        .env_clear()
        .env("QZ_PROGRESS_PATH", &path)
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unrecognized subcommand"));
    assert!(
        !path.exists(),
        "Retired environment variable cannot imply a supported snapshot stream"
    );
}
