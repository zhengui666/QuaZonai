//! The retired study executor cannot regenerate schedules or change original inputs.
#[path = "support/command.rs"]
mod native;
#[test]
fn retired_study_cli_cannot_execute_or_modify_original_inputs() {
    let directory = tempfile::tempdir().unwrap();
    let original = directory.path().join("original-catalog-and-objects");
    std::fs::write(&original, b"preserve original inputs").unwrap();
    let output = native::command(
        &[
            "study-portfolio".as_ref(),
            "--catalog".as_ref(),
            directory.path().as_os_str(),
            "--objects".as_ref(),
            directory.path().as_os_str(),
        ],
        &serde_json::json!({}),
    );
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(String::from_utf8_lossy(&output.stderr).contains("study-portfolio"));
    assert_eq!(
        std::fs::read(&original).unwrap(),
        b"preserve original inputs"
    );
    assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
}
