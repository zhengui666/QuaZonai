//! Native archive round trip of the original controlled qualification/delivery graph.
//! This does not prove real-market provenance or remote execution reconciliation.
use sqlx::PgPool;
use std::{collections::BTreeMap, path::Path, process::Stdio};
use tokio::io::AsyncWriteExt;

async fn graph(pool: &PgPool) -> BTreeMap<String, serde_json::Value> {
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname='app' ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    let mut result = BTreeMap::new();
    for table in tables {
        let quoted = table.replace('"', "\"\"");
        let rows = sqlx::query_scalar(&format!(
            "SELECT COALESCE(jsonb_agg(to_jsonb(t) ORDER BY to_jsonb(t)::text),'[]'::jsonb) FROM app.\"{quoted}\" t"
        ))
        .fetch_one(pool)
        .await
        .unwrap();
        result.insert(table, rows);
    }
    result
}

pub async fn check(pool: &PgPool, objects: &Path, actor: &store::authority::Actor) {
    let before = graph(pool).await;
    for required in [
        "qualifications",
        "releases",
        "handoff_transfers",
        "forward_messages",
    ] {
        assert!(
            !before[required].as_array().unwrap().is_empty(),
            "archive must contain original qualified delivery and feedback"
        );
    }
    let directory = tempfile::tempdir().unwrap();
    let archive = directory.path().join("objects.tar");
    let saved = tokio::process::Command::new("tar")
        .env_clear()
        .arg("-C")
        .arg(objects)
        .arg("-cf")
        .arg(&archive)
        .arg(".")
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(
        saved.status.success() && saved.stderr.is_empty(),
        "test object archive failed"
    );
    let dump = super::postgres::postgres_tool(pool, "pg_dump")
        .args(["--format=custom", "--no-owner", "--no-privileges"])
        .output()
        .await
        .unwrap();
    assert!(
        dump.status.success() && dump.stderr.is_empty(),
        "test graph archive failed"
    );
    assert!(dump.stdout.starts_with(b"PGDMP"));
    let database = format!(
        "graph_restore_{}",
        contracts::Id::new().to_string().replace('-', "")
    );
    sqlx::query(&format!("CREATE DATABASE {database} TEMPLATE template0"))
        .execute(pool)
        .await
        .unwrap();
    let restored_pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect_with(pool.connect_options().as_ref().clone().database(&database))
        .await
        .unwrap();
    let mut child = super::postgres::postgres_tool(&restored_pool, "pg_restore")
        .args([
            "--dbname",
            "",
            "--single-transaction",
            "--exit-on-error",
            "--no-owner",
            "--no-privileges",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut input = child.stdin.take().unwrap();
    let (written, restored) = tokio::join!(
        async {
            input.write_all(&dump.stdout).await?;
            input.shutdown().await
        },
        child.wait_with_output()
    );
    assert!(written.is_ok());
    let restored = restored.unwrap();
    assert!(
        restored.status.success() && restored.stderr.is_empty(),
        "test graph restore failed"
    );
    // Equality failures never print rows, archives or credential-bearing test metadata.
    assert!(
        graph(&restored_pool).await == before,
        "restored business graph differs"
    );
    let output = directory.path().join("objects");
    std::fs::create_dir(&output).unwrap();
    let unpacked = tokio::process::Command::new("tar")
        .env_clear()
        .arg("-C")
        .arg(&output)
        .arg("-xf")
        .arg(&archive)
        .kill_on_drop(true)
        .output()
        .await
        .unwrap();
    assert!(
        unpacked.status.success() && unpacked.stderr.is_empty(),
        "test object restore failed"
    );
    let restored_objects = integrations::artifacts::ArtifactStore::open(&output).unwrap();
    let ids: Vec<(uuid::Uuid, i64)> = sqlx::query_as(
        "SELECT a.id,a.byte_count FROM app.artifacts a WHERE a.id IN (SELECT package_artifact_id FROM app.releases UNION SELECT report_artifact_id FROM app.forward_messages) ORDER BY a.id",
    ).fetch_all(&restored_pool).await.unwrap();
    assert!(!ids.is_empty());
    for (id, size) in ids {
        let id: contracts::Id = id.to_string().try_into().unwrap();
        let size = contracts::DbCounter::new(size.try_into().unwrap()).unwrap();
        let bytes = restored_objects.read(id, size).unwrap();
        assert!(
            bytes == std::fs::read(objects.join(id.to_string())).unwrap(),
            "restored original artifact differs"
        );
    }
    let source_store = store::Store::from_pool(pool.clone());
    let restored_store = store::Store::from_pool(restored_pool.clone());
    let source_objects = integrations::artifacts::ArtifactStore::open(objects).unwrap();
    let streams: Vec<(uuid::Uuid, String)> = sqlx::query_as(
        "SELECT DISTINCT handoff_id,stream_id FROM app.forward_messages ORDER BY handoff_id,stream_id",
    )
    .fetch_all(&restored_pool)
    .await
    .unwrap();
    assert!(!streams.is_empty());
    for (handoff, stream_id) in streams {
        let handoff: contracts::Id = handoff.to_string().try_into().unwrap();
        let query = contracts::forward::ForwardWindowQueryV1 { stream_id };
        let source = source_store
            .forward_window(actor, handoff, &query, |id, size| {
                std::future::ready(
                    source_objects
                        .read(id, size)
                        .map_err(|_| store::StoreError::Integrity),
                )
            })
            .await
            .unwrap();
        let recovered = restored_store
            .forward_window(actor, handoff, &query, |id, size| {
                std::future::ready(
                    restored_objects
                        .read(id, size)
                        .map_err(|_| store::StoreError::Integrity),
                )
            })
            .await
            .unwrap();
        assert!(
            serde_json::to_value(source).unwrap() == serde_json::to_value(recovered).unwrap(),
            "restored original Forward projection differs"
        );
        let original = source_store.handoff(actor, handoff).await.unwrap();
        let recovered = restored_store.handoff(actor, handoff).await.unwrap();
        assert!(
            serde_json::to_value(original).unwrap() == serde_json::to_value(recovered).unwrap(),
            "restored original Claim history differs"
        );
    }
    assert!(
        graph(&restored_pool).await == before,
        "restored reads mutated the graph"
    );
    assert!(
        graph(pool).await == before,
        "source business graph changed during recovery"
    );
    restored_pool.close().await;
    sqlx::query(&format!("DROP DATABASE {database}"))
        .execute(pool)
        .await
        .unwrap();
}
