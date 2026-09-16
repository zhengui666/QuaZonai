//! Native filesystem/PG failure recovery. The injected database failure is confined
//! to each sqlx test database; no source, production database or storage hook is mocked.
use super::*;

#[sqlx::test(migrations = "../../migrations")]
async fn failed_postpublication_probe_is_reclaimed_and_the_same_key_can_retry(pool: PgPool) {
    let (f, cookie, tls, runtime) = setup(pool.clone(), true).await;
    sqlx::raw_sql("CREATE FUNCTION app.reject_runtime_probe_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'native_probe_failure_fixture'; END $$; CREATE TRIGGER reject_runtime_probe_fixture BEFORE INSERT ON app.runtime_probe_observations FOR EACH ROW EXECUTE FUNCTION app.reject_runtime_probe_fixture();")
        .execute(&pool).await.unwrap();
    let path = format!(
        "/api/v2/integrations/runtimes/{}/probe",
        runtime["id"].as_str().unwrap()
    );
    let intent = json!({"schema_version":1,"expected_revision":runtime["revision"]});
    for _ in 0..3 {
        let failed = command(
            &f,
            &cookie,
            "reclaimed-probe",
            "POST",
            &path,
            intent.clone(),
        )
        .await;
        assert!(failed.status.is_server_error());
        assert!(!failed
            .body
            .to_string()
            .contains("native_probe_failure_fixture"));
        assert!(!failed.body.to_string().contains(SECRET));
        let counts: (i64, i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runtime_probe_observations),(SELECT count(*) FROM app.command_receipts WHERE operation='RUNTIME_PROBE'),(SELECT count(*) FROM app.artifacts WHERE schema_name='qz.runtime_probe')")
            .fetch_one(&pool).await.unwrap();
        assert_eq!(counts, (0, 0, 0));
        assert_eq!(
            std::fs::read_dir(f._state.path().join("artifacts"))
                .unwrap()
                .count(),
            0,
            "a known failed publication must not accumulate native snapshot files"
        );
    }
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 3);
    sqlx::raw_sql("DROP TRIGGER reject_runtime_probe_fixture ON app.runtime_probe_observations; DROP FUNCTION app.reject_runtime_probe_fixture();")
        .execute(&pool).await.unwrap();
    let accepted = command(
        &f,
        &cookie,
        "reclaimed-probe",
        "POST",
        &path,
        intent.clone(),
    )
    .await;
    assert_eq!(accepted.status, StatusCode::OK);
    assert_eq!(accepted.body["replayed"], false);
    let replay = command(&f, &cookie, "reclaimed-probe", "POST", &path, intent).await;
    assert_eq!(replay.status, StatusCode::OK);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], accepted.body["resource"]);
    assert_eq!(tls.server.requests.load(Ordering::SeqCst), 4);
    assert_eq!(
        std::fs::read_dir(f._state.path().join("artifacts"))
            .unwrap()
            .count(),
        1
    );
    let artifact: Id = accepted.body["resource"]["snapshot_artifact_id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    assert!(!f
        .store
        .discard_unpublished_operator_artifact(artifact, |_| async {
            panic!("a committed probe artifact must never reach the discard callback")
        })
        .await
        .unwrap());
    assert!(f
        ._state
        .path()
        .join("artifacts")
        .join(artifact.to_string())
        .is_file());
}

#[sqlx::test(migrations = "../../migrations")]
async fn reclamation_waits_for_the_original_publication_transaction_and_retains_its_commit(
    pool: PgPool,
) {
    let (f, _cookie, _tls, _runtime) = setup(pool.clone(), true).await;
    let artifact = Id::new();
    let objects = ArtifactStore::open(&f._state.path().join("artifacts")).unwrap();
    objects
        .put(artifact, b"native publication fixture")
        .unwrap();
    let mut publishing = pool.begin().await.unwrap();
    sqlx::query("SELECT singleton FROM app.operator_auth_state WHERE singleton FOR UPDATE")
        .fetch_one(&mut *publishing)
        .await
        .unwrap();
    sqlx::query("INSERT INTO app.artifacts(id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,'REPORT','application/json','qz.runtime_probe','1','LOCAL',$2,'1',26,'OPERATOR','FIXTURE','OPERATOR','AUDIT')")
        .bind(artifact.as_uuid()).bind(artifact.to_string()).execute(&mut *publishing).await.unwrap();
    let store = f.store.clone();
    let (entered, started) = tokio::sync::oneshot::channel();
    let mut cleanup = tokio::spawn(async move {
        entered.send(()).unwrap();
        store
            .discard_unpublished_operator_artifact(artifact, |_| async {
                panic!("an uncertain-but-committed publication must be retained")
            })
            .await
    });
    started.await.unwrap();
    assert!(
        tokio::time::timeout(std::time::Duration::from_millis(30), &mut cleanup)
            .await
            .is_err(),
        "cleanup must not run ahead of the publisher's held authority lock"
    );
    publishing.commit().await.unwrap();
    assert!(!cleanup.await.unwrap().unwrap());
    assert_eq!(
        objects.read(artifact, DbCounter::new(26).unwrap()).unwrap(),
        b"native publication fixture"
    );
}
