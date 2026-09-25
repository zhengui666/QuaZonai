//! Keep .opensdlc/architecture.md package ownership executable without another dependency tool.
use serde_json::Value;
use std::{path::Path, process::Command};

#[test]
fn workspace_dependencies_follow_design_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let output = Command::new(env!("CARGO"))
        .current_dir(root)
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--no-deps",
            "--format-version=1",
        ])
        .output()
        .expect("run native Cargo metadata");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let members = metadata["workspace_members"].as_array().unwrap();
    let workspace_names: Vec<_> = packages
        .iter()
        .filter(|package| members.contains(&package["id"]))
        .map(|package| package["name"].as_str().unwrap())
        .collect();

    for package in packages
        .iter()
        .filter(|package| members.contains(&package["id"]))
    {
        let name = package["name"].as_str().unwrap();
        let allowed: &[&str] = match name {
            "contracts" => &[],
            "domain" | "integrations" => &["contracts"],
            "store" | "job" => &["contracts", "domain"],
            "runtime" => &["contracts", "domain", "integrations"],
            "server" => &["contracts", "domain", "store", "integrations"],
            _ => panic!(
                "Document ownership of new workspace package {name} in .opensdlc/architecture.md first"
            ),
        };
        for dependency in package["dependencies"].as_array().unwrap() {
            // Native scientific/HTTP fixtures intentionally share test helpers.
            if dependency["kind"] == "dev" {
                continue;
            }
            // Cargo reports the original name even for renamed dependencies.
            let target = dependency["name"].as_str().unwrap();
            if workspace_names.contains(&target) {
                assert!(
                    allowed.contains(&target),
                    "Forbidden workspace dependency: {name} -> {target}"
                );
            }
            if matches!(name, "contracts" | "domain") {
                assert!(
                    ![
                        "axum",
                        "reqwest",
                        "sqlx",
                        "bollard",
                        "rmcp",
                        "tower",
                        "tower-sessions"
                    ]
                    .contains(&target),
                    "Transport/persistence belongs outside {name}: {target}"
                );
            }
        }
    }
}
