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
        validate_local_dependencies(package, &workspace_names)
            .unwrap_or_else(|reason| panic!("{reason}"));
    }
}

/// Follow Cargo's feature edges without executing Cargo. `dependency?/feature`
/// is weak: by itself it cannot activate an optional dependency.
fn feature_activates_dependency(
    features: &serde_json::Map<String, Value>,
    start: &str,
    dependency_key: &str,
    stop_at_paper: bool,
) -> bool {
    let mut pending = vec![start.to_owned()];
    let mut visited = std::collections::BTreeSet::new();
    while let Some(edge) = pending.pop() {
        if stop_at_paper && edge == "native-paper" {
            continue;
        }
        if edge == format!("dep:{dependency_key}")
            || edge == dependency_key
            || edge
                .strip_prefix(dependency_key)
                .is_some_and(|suffix| suffix.starts_with('/'))
        {
            return true;
        }
        if !visited.insert(edge.clone()) {
            continue;
        }
        if let Some(children) = features.get(&edge).and_then(Value::as_array) {
            pending.extend(children.iter().filter_map(Value::as_str).map(str::to_owned));
        }
    }
    false
}

fn job_paper_integration_dependency(package: &Value, dependency: &Value) -> Result<(), String> {
    let invalid =
        || "job -> integrations is limited to a normal optional native-paper dependency".to_owned();
    if !dependency["kind"].is_null() || dependency["optional"] != true {
        return Err(invalid());
    }
    let key = dependency["rename"].as_str().unwrap_or("integrations");
    let features = package["features"].as_object().ok_or_else(invalid)?;
    let direct = format!("dep:{key}");
    if !features
        .get("native-paper")
        .and_then(Value::as_array)
        .is_some_and(|edges| {
            edges
                .iter()
                .any(|edge| edge.as_str() == Some(direct.as_str()))
        })
    {
        return Err(invalid());
    }
    for (feature, edges) in features {
        if feature == "native-paper" || !feature_activates_dependency(features, feature, key, false)
        {
            continue;
        }
        // This is the only currently declared Paper acceptance feature. New
        // feature names require an explicit ownership review, not a suffix rule.
        let paper_test = feature == "native-paper-test"
            && edges.as_array().is_some_and(|edges| {
                edges
                    .iter()
                    .any(|edge| edge.as_str() == Some("native-paper"))
            })
            && !feature_activates_dependency(features, feature, key, true);
        if !paper_test {
            return Err(format!(
                "job feature {feature} activates integrations outside native-paper"
            ));
        }
    }
    Ok(())
}

fn validate_local_dependencies(package: &Value, workspace_names: &[&str]) -> Result<(), String> {
    let name = package["name"].as_str().unwrap();
    let allowed: &[&str] = match name {
        "contracts" => &[],
        "domain" | "integrations" => &["contracts"],
        "store" | "job" => &["contracts", "domain"],
        "runtime" => &["contracts", "domain", "integrations"],
        "quazonai-cli" => &["contracts", "integrations"],
        "server" => &["contracts", "domain", "store", "integrations"],
        _ => {
            return Err(format!(
            "Document ownership of new workspace package {name} in .opensdlc/architecture.md first"
        ))
        }
    };
    for dependency in package["dependencies"].as_array().unwrap() {
        // Cargo reports the original package name even for renamed dependencies.
        let target = dependency["name"].as_str().unwrap();
        if name == "job" && target == "integrations" {
            // Check before the general test-helper exemption: this narrow edge
            // is normal/optional only, never an unrestricted dev/build shortcut.
            job_paper_integration_dependency(package, dependency)?;
            continue;
        }
        if dependency["kind"] == "dev" {
            continue;
        }
        if workspace_names.contains(&target) && !allowed.contains(&target) {
            return Err(format!(
                "Forbidden workspace dependency: {name} -> {target}"
            ));
        }
        if matches!(name, "contracts" | "domain")
            && [
                "axum",
                "reqwest",
                "sqlx",
                "bollard",
                "rmcp",
                "tower",
                "tower-sessions",
            ]
            .contains(&target)
        {
            return Err(format!(
                "Transport/persistence belongs outside {name}: {target}"
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod paper_dependency_gate {
    use super::*;
    use serde_json::json;
    const WORKSPACE: &[&str] = &[
        "contracts",
        "domain",
        "integrations",
        "job",
        "server",
        "store",
    ];

    fn package() -> Value {
        json!({"name": "job", "dependencies": [
            {"name": "contracts", "kind": null, "optional": false, "rename": null},
            {"name": "domain", "kind": null, "optional": false, "rename": null},
            {"name": "integrations", "kind": null, "optional": true, "rename": null}
        ], "features": {
            "default": [], "scientific": [], "native-node-observer": [],
            "native-paper": ["native-node-observer", "dep:integrations"],
            "native-paper-test": ["native-paper"]
        }})
    }

    #[test]
    fn accepts_only_the_declared_paper_gate_and_its_test_feature() {
        assert!(validate_local_dependencies(&package(), WORKSPACE).is_ok());
        let mut weak = package();
        weak["features"]["scientific"] = json!(["integrations?/some-helper"]);
        assert!(validate_local_dependencies(&weak, WORKSPACE).is_ok());
        let mut missing_gate = package();
        missing_gate["features"]["native-paper"] = json!([]);
        assert!(validate_local_dependencies(&missing_gate, WORKSPACE).is_err());
    }

    #[test]
    fn rejects_nonoptional_build_and_dev_edges() {
        for kind in [Value::Null, json!("build"), json!("dev")] {
            let mut value = package();
            value["dependencies"][2]["optional"] = json!(kind != Value::Null);
            value["dependencies"][2]["kind"] = kind;
            assert!(validate_local_dependencies(&value, WORKSPACE).is_err());
        }
    }

    #[test]
    fn rejects_default_and_scientific_transitive_activation() {
        for feature in ["default", "scientific"] {
            for edge in [
                "native-paper",
                "native-paper-test",
                "dep:integrations",
                "integrations/extra",
            ] {
                let mut value = package();
                value["features"][feature] = json!([edge]);
                assert!(
                    validate_local_dependencies(&value, WORKSPACE).is_err(),
                    "{feature}: {edge}"
                );
            }
            let mut cyclic = package();
            cyclic["features"][feature] = json!(["first"]);
            cyclic["features"]["first"] = json!(["second"]);
            cyclic["features"]["second"] = json!(["first", "native-paper-test"]);
            assert!(feature_activates_dependency(
                cyclic["features"].as_object().unwrap(),
                feature,
                "integrations",
                false
            ));
            assert!(validate_local_dependencies(&cyclic, WORKSPACE).is_err());
        }
    }

    #[test]
    fn rejects_unrelated_direct_and_transitive_feature_activation() {
        for edge in [
            "dep:integrations",
            "integrations",
            "integrations/extra",
            "native-paper-test",
        ] {
            let mut value = package();
            value["features"]["catalog-prepare"] = json!([edge]);
            assert!(
                validate_local_dependencies(&value, WORKSPACE).is_err(),
                "{edge}"
            );
        }
        let mut bypass = package();
        bypass["features"]["native-paper-test"] = json!(["native-paper", "dep:integrations"]);
        assert!(validate_local_dependencies(&bypass, WORKSPACE).is_err());
    }

    #[test]
    fn dependency_rename_cannot_bypass_feature_reachability() {
        let mut renamed = package();
        renamed["dependencies"][2]["rename"] = json!("paper_auth");
        renamed["features"]["native-paper"] = json!(["dep:paper_auth"]);
        assert!(validate_local_dependencies(&renamed, WORKSPACE).is_ok());
        renamed["features"]["scientific"] = json!(["paper_auth/extra"]);
        assert!(validate_local_dependencies(&renamed, WORKSPACE).is_err());
    }

    #[test]
    fn paper_exception_does_not_allow_job_server_or_store_edges() {
        for target in ["server", "store"] {
            let mut value = package();
            value["dependencies"].as_array_mut().unwrap().push(json!({
                "name": target, "kind": null, "optional": true, "rename": null
            }));
            value["features"]["native-paper"]
                .as_array_mut()
                .unwrap()
                .push(json!(format!("dep:{target}")));
            assert!(
                validate_local_dependencies(&value, WORKSPACE).is_err(),
                "{target}"
            );
        }
    }
}
