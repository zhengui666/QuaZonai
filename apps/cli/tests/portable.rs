use std::process::Command;

fn invoke(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_quazonai"))
        .args(args)
        .output()
        .unwrap()
}

#[test]
fn release_binary_reports_version_and_inspects_contracts_offline() {
    let result = invoke(&["--version"]);
    assert!(result.status.success());
    assert_eq!(
        String::from_utf8(result.stdout).unwrap().trim(),
        format!(
            "quazonai {}",
            option_env!("QUAZONAI_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
        )
    );
    for args in [
        vec!["client", "--help"],
        vec!["openapi", "--list-schemas"],
        vec!["openapi", "--schema", "ArtifactCreate"],
    ] {
        let result = invoke(&args);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(!result.stdout.is_empty());
        assert!(result.stderr.is_empty());
    }
    let schemas: serde_json::Value =
        serde_json::from_slice(&invoke(&["openapi", "--list-schemas"]).stdout).unwrap();
    assert!(schemas["schemas"]
        .as_array()
        .unwrap()
        .iter()
        .any(|name| name == "ArtifactCreate"));
    let result = invoke(&[
        "client",
        "--origin",
        "http://192.0.2.1:8080",
        "--credential-file",
        "missing-credential",
        "--preview",
        "project",
        "list",
    ]);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let preview: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(preview["request_sent"], false);
}
