//! Actual local export CLI with disposable files, never a user's historical backup.
use contracts::{imports::*, Id};
use integrations::artifacts::ArtifactStore;
use serde_json::json;
use std::{fs, path::Path};

fn identity(number: u64) -> serde_json::Value {
    json!({"kind":"ARTIFACT","source_table":"mission_artifacts","source_id":format!("00000000-0000-4000-8000-{number:012x}")})
}
async fn export(root: &Path, selection: &Path, output: &Path) -> bool {
    tokio::process::Command::new(env!("CARGO_BIN_EXE_server"))
        .arg("export-historical-artifacts")
        .arg("--source-root")
        .arg(root)
        .arg("--selection")
        .arg(selection)
        .arg("--output")
        .arg(output)
        .kill_on_drop(true)
        .output()
        .await
        .unwrap()
        .status
        .success()
}

#[tokio::test]
async fn copies_exact_binary_bytes_and_reports_unread_items_without_source_paths() {
    use std::os::unix::fs::symlink;
    let parent = tempfile::tempdir().unwrap();
    let root = parent.path().join("old");
    fs::create_dir(&root).unwrap();
    let bytes = b"\xff\0OLD_PUBLIC_BYTES_SENTINEL";
    fs::write(root.join("original.bin"), bytes).unwrap();
    fs::write(root.join("empty.bin"), b"").unwrap();
    symlink("original.bin", root.join("linked.bin")).unwrap();
    let selection = parent.path().join("selection.json");
    let source_installation_id = Id::new();
    let items = [
        ("original.bin", "COPY_PUBLIC"),
        ("missing.bin", "COPY_PUBLIC"),
        ("../outside.bin", "COPY_PUBLIC"),
        ("linked.bin", "COPY_PUBLIC"),
        ("never-open-sealed.bin", "SEALED_RETAINED"),
        ("never-open-unreviewed.bin", "MANUAL_REVIEW_REQUIRED"),
        ("empty.bin", "COPY_PUBLIC"),
    ].into_iter().enumerate().map(|(n,(path,disposition))| json!({"identity":identity(n as u64 + 1),"relative_path":path,"disposition":disposition})).collect::<Vec<_>>();
    fs::write(&selection, serde_json::to_vec(&json!({"schema_version":1,"source_installation_id":source_installation_id,"artifacts":items})).unwrap()).unwrap();
    let output = parent.path().join("export");
    assert!(export(&root, &selection, &output).await);
    let report_bytes = fs::read(output.join("report.json")).unwrap();
    let report: HistoricalArtifactExportV1 = serde_json::from_slice(&report_bytes).unwrap();
    assert_eq!(report.source_installation_id, source_installation_id);
    use HistoricalArtifactOutcomeV1::*;
    assert_eq!(
        report
            .artifacts
            .iter()
            .map(|r| r.outcome)
            .collect::<Vec<_>>(),
        [
            Copied,
            Missing,
            Unsupported,
            Unreadable,
            SealedRetained,
            ManualReviewRequired,
            Unsupported
        ]
    );
    let copied = &report.artifacts[0];
    assert_eq!(
        copied.identity.source_id.to_string(),
        "00000000-0000-4000-8000-000000000001"
    );
    let objects = ArtifactStore::open(&output.join("objects")).unwrap();
    assert_eq!(
        objects
            .read(copied.object_ref.unwrap(), copied.byte_count.unwrap())
            .unwrap(),
        bytes
    );
    assert!(report.artifacts[1..]
        .iter()
        .all(|r| r.object_ref.is_none() && r.byte_count.is_none()));
    assert_eq!(fs::read_dir(output.join("objects")).unwrap().count(), 1);
    let report_text = String::from_utf8(report_bytes.clone()).unwrap();
    for omitted in [
        "original.bin",
        "outside.bin",
        "never-open",
        "OLD_PUBLIC_BYTES_SENTINEL",
        root.to_str().unwrap(),
    ] {
        assert!(!report_text.contains(omitted));
    }
    assert_eq!(fs::read(root.join("original.bin")).unwrap(), bytes);
    assert!(!export(&root, &selection, &output).await);
    assert_eq!(fs::read(output.join("report.json")).unwrap(), report_bytes);
}

#[tokio::test]
async fn duplicate_or_unknown_source_identity_cannot_create_an_export() {
    let parent = tempfile::tempdir().unwrap();
    let selection = parent.path().join("selection.json");
    let item =
        json!({"identity":identity(1),"relative_path":null,"disposition":"MANUAL_REVIEW_REQUIRED"});
    fs::write(
        &selection,
        serde_json::to_vec(
            &json!({"schema_version":1,"source_installation_id":Id::new(),"artifacts":[item,item]}),
        )
        .unwrap(),
    )
    .unwrap();
    let output = parent.path().join("duplicate");
    assert!(!export(parent.path(), &selection, &output).await);
    assert!(!output.exists());
    let mut unknown = item;
    unknown["identity"]["source_table"] = json!("credentials");
    fs::write(
        &selection,
        serde_json::to_vec(
            &json!({"schema_version":1,"source_installation_id":Id::new(),"artifacts":[unknown]}),
        )
        .unwrap(),
    )
    .unwrap();
    assert!(!export(parent.path(), &selection, &output).await);
    assert!(!output.exists());
}
