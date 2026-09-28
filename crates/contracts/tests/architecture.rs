//! Keep .opensdlc/architecture.md package ownership executable without another dependency tool.
use serde_json::Value;
use std::{path::Path, process::Command};

#[test]
fn workspace_dependencies_follow_design_boundaries() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()
        .unwrap();
    // Resolve the actual native graph without fetching irrelevant target packages.
    let rustc = Command::new(Path::new(env!("CARGO")).with_file_name("rustc"))
        .arg("-vV")
        .output()
        .expect("read native Rust host");
    assert!(rustc.status.success());
    let version = String::from_utf8(rustc.stdout).unwrap();
    let host = version
        .lines()
        .find_map(|line| line.strip_prefix("host: "))
        .unwrap();
    let output = Command::new(env!("CARGO"))
        .current_dir(&root)
        .args([
            "metadata",
            "--locked",
            "--filter-platform",
            host,
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
    for package in packages {
        let name = package["name"].as_str().unwrap();
        match package["source"].as_str() {
            None => assert!(
                members.contains(&package["id"]),
                "Third-party source must not be vendored or path-patched: {name}"
            ),
            Some(source) => assert!(
                source == "registry+https://github.com/rust-lang/crates.io-index"
                    || source == "git+https://github.com/maxcountryman/tower-sessions-stores?rev=d18c9bf76f1d4fb73130dbe5aa643197f14b5d2d#d18c9bf76f1d4fb73130dbe5aa643197f14b5d2d",
                "Review and document the official upstream source for {name}: {source}"
            ),
        }
    }
    // Catch unused component copies as well as dependencies in the resolved graph.
    let files = Command::new("git")
        .current_dir(&root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
            "Cargo.toml",
            "**/Cargo.toml",
            "**/package.json",
        ])
        .output()
        .expect("list repository dependency manifests");
    assert!(files.status.success());
    for file in files
        .stdout
        .split(|byte| *byte == 0)
        .filter(|file| !file.is_empty())
    {
        let file = std::str::from_utf8(file).unwrap();
        assert!(
            matches!(
                file,
                "Cargo.toml" | "apps/web/package.json" | "runtimes/codex/package.json"
            ) || packages
                .iter()
                .any(|package| members.contains(&package["id"])
                    && Path::new(package["manifest_path"].as_str().unwrap()) == root.join(file)),
            "Only QuaZonai component manifests belong in the repository: {file}"
        );
    }
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
            "quazonai-cli" => &["contracts", "integrations"],
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
