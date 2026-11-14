#[path = "../build_identity.rs"]
mod identity;
use serde_json::json;

fn metadata(root: &std::path::Path, model: &std::path::Path) -> serde_json::Value {
    json!({"packages":[
        {"id":"job","name":"job","manifest_path":root},
        {"id":"model","name":"nautilus-model","version":"0.69.0","source":"registry+https://github.com/rust-lang/crates.io-index","manifest_path":model},
        {"id":"dev-model","name":"nautilus-model","version":"99.0.0","source":"git+https://example.invalid/unselected#different","manifest_path":"/unselected/Cargo.toml"}
    ],"resolve":{"nodes":[
        {"id":"job","deps":[{"pkg":"model","dep_kinds":[{"kind":null}]},{"pkg":"dev-model","dep_kinds":[{"kind":"dev"}]}]},
        {"id":"model","deps":[]}, {"id":"dev-model","deps":[]}
    ]}})
}

#[test]
fn selected_graph_ignores_other_workspace_and_dev_only_model() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Cargo.toml");
    std::fs::write(&root, "").unwrap();
    let graph = metadata(&root, &root);
    // The model uses its own manifest, otherwise root selection is ambiguous.
    let model = dir.path().join("model.toml");
    std::fs::write(&model, "").unwrap();
    let mut graph = graph;
    graph["packages"][1]["manifest_path"] = json!(model);
    let actual = identity::from_metadata(&graph, &root).unwrap();
    assert_eq!(
        actual,
        (
            "0.69.0".into(),
            "".into(),
            "registry+https://github.com/rust-lang/crates.io-index".into()
        )
    );
    graph["resolve"]["nodes"][0]["deps"][1]["dep_kinds"][0]["kind"] = json!("build");
    assert!(
        identity::from_metadata(&graph, &root)
            .unwrap_err()
            .contains("ambiguous")
    );
}

#[test]
fn path_provenance_is_explicit_and_not_an_official_git_revision() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Cargo.toml");
    std::fs::write(&root, "").unwrap();
    let local = dir.path().join("local source#1");
    std::fs::create_dir(&local).unwrap();
    let model = local.join("Cargo.toml");
    std::fs::write(&model, "").unwrap();
    let mut graph = metadata(&root, &model);
    graph["packages"][1]["source"] = serde_json::Value::Null;
    let actual = identity::from_metadata(&graph, &root).unwrap();
    assert!(actual.2.starts_with("path+file:///"));
    assert!(actual.2.ends_with("/local%20source%231"));
    assert_eq!(actual.1, "");
}

#[test]
fn git_revision_and_missing_selected_nodes_are_not_guessed() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Cargo.toml");
    std::fs::write(&root, "").unwrap();
    let model = dir.path().join("model.toml");
    std::fs::write(&model, "").unwrap();
    let mut graph = metadata(&root, &model);
    graph["packages"][1]["source"] = json!(
        "git+https://github.com/nautechsystems/nautilus_trader.git?branch=owner#exact-revision"
    );
    assert_eq!(
        identity::from_metadata(&graph, &root).unwrap().1,
        "exact-revision"
    );
    graph["resolve"]["nodes"][0]["deps"][0]["pkg"] = json!("missing");
    assert!(
        identity::from_metadata(&graph, &root)
            .unwrap_err()
            .contains("node missing")
    );
}
