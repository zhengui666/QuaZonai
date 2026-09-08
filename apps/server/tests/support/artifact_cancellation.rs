//! Deterministic native database barrier after filesystem publication.
use super::*;

#[sqlx::test(migrations = "../../migrations")]
async fn cancelling_http_waiter_keeps_publication_transaction_and_capacity_alive(pool: PgPool) {
    let (f, cookie, project, slots) = setup(&pool).await;
    let bytes = "// publication survives a cancelled HTTP waiter\n";
    let body = upload(project, bytes);
    let key = "cancel-after-native-publish";
    let object_root = f._state.path().join("artifacts");
    // A test-local trigger establishes the race order using a real PG lock.
    sqlx::raw_sql(
        "CREATE FUNCTION public.pause_artifact_publication_for_test() RETURNS trigger \
         LANGUAGE plpgsql AS $$ BEGIN \
           PERFORM pg_advisory_xact_lock(6263, 908); RETURN NEW; \
         END $$; \
         CREATE TRIGGER pause_artifact_publication_for_test \
         BEFORE INSERT ON app.artifacts FOR EACH ROW \
         EXECUTE FUNCTION public.pause_artifact_publication_for_test();",
    )
    .execute(&pool)
    .await
    .unwrap();
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT pg_advisory_xact_lock(6263, 908)")
        .execute(&mut *blocker)
        .await
        .unwrap();
    let app = f.app.clone();
    let post = request(
        "POST",
        "/api/v2/artifacts",
        key,
        body.clone(),
        Some(&cookie),
        None,
    );
    let waiter = tokio::spawn(async move { app.oneshot(post).await.unwrap() });
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM pg_locks \
                 WHERE locktype='advisory' AND NOT granted \
                   AND database=(SELECT oid FROM pg_database WHERE datname=current_database()) \
                   AND classid=6263::oid AND objid=908::oid AND objsubid=2)",
            )
            .fetch_one(&pool)
            .await
            .unwrap();
            if waiting {
                break;
            }
            assert!(
                !waiter.is_finished(),
                "upload returned before the native barrier"
            );
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("metadata INSERT must reach the PostgreSQL barrier");
    let objects: Vec<_> = std::fs::read_dir(&object_root)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect();
    assert_eq!(objects.len(), 1);
    let object = &objects[0];
    let id: Id = object
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    assert_eq!(std::fs::read(object).unwrap(), bytes.as_bytes());
    let before: i64 = sqlx::query_scalar("SELECT count(*) FROM app.artifacts WHERE id=$1")
        .bind(id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(before, 0, "uncommitted metadata must remain invisible");
    assert_eq!(slots.available_permits(), 3);
    waiter.abort();
    assert!(waiter.await.unwrap_err().is_cancelled());
    assert_eq!(
        slots.available_permits(),
        3,
        "HTTP cancellation must retain the publication permit"
    );
    blocker.commit().await.unwrap();
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let persisted: bool = sqlx::query_scalar(
                "SELECT EXISTS(SELECT 1 FROM app.artifacts a \
                 JOIN app.command_receipts c ON c.resource_id=a.id \
                 WHERE a.id=$1 AND c.operation='ARTIFACT_SUBMIT' AND c.idempotency_key=$2)",
            )
            .bind(id.as_uuid())
            .bind(key)
            .fetch_one(&pool)
            .await
            .unwrap();
            if persisted && slots.available_permits() == 4 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(5)).await;
        }
    })
    .await
    .expect("owned publication must commit its metadata and receipt");
    let replay = send(
        &f,
        "POST",
        "/api/v2/artifacts",
        key,
        body,
        Some(&cookie),
        None,
    )
    .await;
    assert_eq!(replay.status, StatusCode::CREATED, "{}", replay.body);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"]["id"], id.to_string());
    assert_eq!(
        replay.body["resource"]["byte_count"],
        bytes.len().to_string()
    );
    let totals: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.artifacts WHERE id=$1), \
                (SELECT count(*) FROM app.command_receipts \
                 WHERE resource_id=$1 AND operation='ARTIFACT_SUBMIT' AND idempotency_key=$2)",
    )
    .bind(id.as_uuid())
    .bind(key)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(totals, (1, 1));
    assert_eq!(std::fs::read_dir(&object_root).unwrap().count(), 1);
    assert_eq!(std::fs::read(object).unwrap(), bytes.as_bytes());
    assert_eq!(slots.available_permits(), 4);
}
