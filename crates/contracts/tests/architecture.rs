//! Cargo contains the native scientific kernel, not the TypeScript service graph.
use serde_json::Value;
use std::{path::Path, process::Command};

#[test]
fn scientific_workspace_dependencies() {
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
        .expect("read workspace metadata");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    let packages = metadata["packages"].as_array().unwrap();
    let mut names: Vec<_> = packages
        .iter()
        .map(|package| package["name"].as_str().unwrap())
        .collect();
    names.sort_unstable();
    assert_eq!(names, ["contracts", "domain", "job"]);
    for package in packages {
        let allowed: &[&str] = match package["name"].as_str().unwrap() {
            "contracts" => &[],
            "domain" => &["contracts"],
            "job" => &["contracts", "domain"],
            _ => unreachable!(),
        };
        for dependency in package["dependencies"].as_array().unwrap() {
            let name = dependency["name"].as_str().unwrap();
            if dependency["source"].is_null() {
                assert!(
                    allowed.contains(&name),
                    "unexpected local scientific dependency: {name}"
                );
            }
            assert!(
                ![
                    "sqlx",
                    "axum",
                    "bollard",
                    "tower-sessions",
                    "integrations",
                    "store",
                    "server",
                    "runtime"
                ]
                .contains(&name),
                "service dependency retained in native kernel: {name}"
            );
        }
    }
}
