//! Upgrade real SQLite state without discarding native identities or original bytes.
use sqlx::{Connection, Row, SqliteConnection};

#[tokio::test]
async fn upgrade_preserves_native_evidence_and_accepts_objects_above_the_former_ceiling() {
    let mut connection = SqliteConnection::connect("sqlite::memory:").await.unwrap();
    sqlx::query("PRAGMA foreign_keys=ON").execute(&mut connection).await.unwrap();
    for migration in [
        include_str!("../migrations/0001_journal.sql"),
        include_str!("../migrations/0002_materializations_and_native_exits.sql"),
        include_str!("../migrations/0003_optional_deadlines.sql"),
    ] {
        sqlx::raw_sql(migration).execute(&mut connection).await.unwrap();
    }
    sqlx::raw_sql("INSERT INTO input_objects VALUES('input','1',x'6e6174697665',6,1);
        INSERT INTO runtime_jobs(external_id,run_id,attempt_no,owner_epoch,spec_json,submitted_us,deadline_us,phase,output_reservation) VALUES('live','run',1,1,'{}',1,NULL,'QUEUED',5);
        INSERT INTO runtime_jobs(external_id,run_id,attempt_no,owner_epoch,spec_json,submitted_us,deadline_us,phase,cancel_requested_us,cancel_owner_epoch,terminal_state,finished_us,output_reservation) VALUES('dead','cancelled',1,1,NULL,1,NULL,'TERMINAL',1,1,'CANCELLED',2,0);
        INSERT INTO job_outputs VALUES('live','out','{}',x'6f7574707574',6);
        INSERT INTO materialization_reservations VALUES('live',123,1);")
        .execute(&mut connection).await.unwrap();
    let mut tx = connection.begin().await.unwrap();
    sqlx::raw_sql(include_str!("../migrations/0004_unbounded_object_sizes.sql"))
        .execute(&mut *tx).await.unwrap();
    tx.commit().await.unwrap();
    assert!(sqlx::query("PRAGMA foreign_key_check").fetch_all(&mut connection).await.unwrap().is_empty());
    for (table, key, expected) in [
        ("input_objects", "id='input'", b"native".as_slice()),
        ("job_outputs", "storage_ref='out'", b"output".as_slice()),
    ] {
        let row = sqlx::query(sqlx::AssertSqlSafe(format!("SELECT bytes,byte_count FROM {table} WHERE {key}")))
            .fetch_one(&mut connection).await.unwrap();
        assert_eq!(row.get::<Vec<u8>, _>("bytes"), expected);
        assert_eq!(row.get::<i64, _>("byte_count"), expected.len() as i64);
    }
    let dead = sqlx::query("SELECT phase,terminal_state,cancel_requested_us,deadline_us FROM runtime_jobs WHERE external_id='dead'")
        .fetch_one(&mut connection).await.unwrap();
    assert_eq!(dead.get::<String, _>("phase"), "TERMINAL");
    assert_eq!(dead.get::<String, _>("terminal_state"), "CANCELLED");
    assert_eq!(dead.get::<Option<i64>, _>("deadline_us"), None);
    assert_eq!(dead.get::<i64, _>("cancel_requested_us"), 1);
    for invalid in [
        "UPDATE input_objects SET bytes=x'00' WHERE id='input'",
        "DELETE FROM input_objects WHERE id='input'",
        "UPDATE job_outputs SET metadata_json='{}' WHERE storage_ref='out'",
        "DELETE FROM job_outputs WHERE storage_ref='out'",
        "DELETE FROM runtime_jobs WHERE external_id='dead'",
        "UPDATE runtime_jobs SET finished_us=3 WHERE external_id='dead'",
        "INSERT INTO job_outputs VALUES('unknown','bad','{}',x'61',1)",
        "INSERT INTO input_objects VALUES('bad','1',x'61',2,1)",
        "DELETE FROM materialization_reservations WHERE external_id='live'",
    ] {
        assert!(sqlx::query(invalid).execute(&mut connection).await.is_err(), "{invalid}");
    }
    let size = 64 * 1024 * 1024 + 1i64;
    sqlx::query("INSERT INTO input_objects VALUES('large','1',zeroblob(?),?,1)")
        .bind(size).bind(size).execute(&mut connection).await.unwrap();
    sqlx::query("INSERT INTO job_outputs VALUES('live','large','{}',zeroblob(?),?)")
        .bind(size).bind(size).execute(&mut connection).await.unwrap();
    sqlx::query("UPDATE runtime_jobs SET output_reservation=? WHERE external_id='live'")
        .bind(size).execute(&mut connection).await.unwrap();
    for (table, key) in [("input_objects", "id='large'"), ("job_outputs", "storage_ref='large'")] {
        let observed: (i64, i64) = sqlx::query_as(sqlx::AssertSqlSafe(format!("SELECT length(bytes),byte_count FROM {table} WHERE {key}")))
            .fetch_one(&mut connection).await.unwrap();
        assert_eq!(observed, (size, size));
    }
}
