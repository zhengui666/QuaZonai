//! Database identity/transfer regressions, not real market or delivery evidence.
mod support;
use contracts::Id;
use sqlx::{PgConnection, PgPool};
use support::{budget, delivery_release_metadata, fixture, portfolio, release, sqlstate};

async fn package(pool: &PgPool, project: Id, kind: &str, origin: &str, access: &str) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,'application/json','qz.target_package','1','LOCAL',$4,'1',32,$5,$6,'OPERATOR','AUDIT')")
        .bind(id.as_uuid()).bind(project.as_uuid()).bind(kind).bind(id.to_string()).bind(access).bind(origin).execute(pool).await.unwrap();
    id
}
async fn copy_release(
    pool: &PgPool,
    base: Id,
    artifact: Id,
    version: &str,
    environment: &str,
) -> Result<Id, sqlx::Error> {
    let id = Id::new();
    sqlx::query("INSERT INTO app.releases(id,candidate_id,package_artifact_id,package_schema_version,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,environment) SELECT $1,candidate_id,$2,$3,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,$4 FROM app.releases WHERE id=$5")
        .bind(id.as_uuid()).bind(artifact.as_uuid()).bind(version).bind(environment).bind(base.as_uuid()).execute(pool).await?;
    Ok(id)
}
async fn downstream(pool: &PgPool) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'relational test','https://example.invalid','not-a-secret','{1}','BOTH',true)")
        .bind(id.as_uuid()).execute(pool).await.unwrap();
    id
}
async fn approve(
    pool: &PgPool,
    release: Id,
    d: Id,
    input: Id,
    env: &str,
) -> Result<Id, sqlx::Error> {
    let id = Id::new();
    sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) VALUES($1,$2,$3,$4,'OPERATOR',$5,clock_timestamp(),clock_timestamp()+interval '1 hour')")
        .bind(id.as_uuid()).bind(release.as_uuid()).bind(env).bind(d.as_uuid()).bind(input.as_uuid()).execute(pool).await?;
    Ok(id)
}
async fn offer(pool: &PgPool, release: Id, approval: Id, d: Id, ttl: f64) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.handoff_offers(id,release_id,approval_id,downstream_id,environment,delivery_sequence,state,offered_at,expires_at) VALUES($1,$2,$3,$4,'PAPER',1,'OFFERED',clock_timestamp()-interval '1 hour',clock_timestamp()+make_interval(secs=>$5))")
        .bind(id.as_uuid()).bind(release.as_uuid()).bind(approval.as_uuid()).bind(d.as_uuid()).bind(ttl).execute(pool).await.unwrap();
    id
}
async fn delivery(pool: &PgPool, ttl: f64) -> (Id, Id, Id) {
    let f = fixture(pool, budget()).await;
    let (m, c, e) = portfolio(pool, &f).await;
    let r = delivery_release_metadata(pool, &f, m, c, e).await.unwrap();
    let d = downstream(pool).await;
    let i = support::approval_inputs(pool, &f, e).await;
    let a = approve(pool, r, d, i, "PAPER").await.unwrap();
    let o = offer(pool, r, a, d, ttl).await;
    (
        o,
        d,
        support::forward_report_metadata(pool, f.project).await,
    )
}
async fn claim(c: &mut PgConnection, o: Id) -> Result<(), sqlx::Error> {
    // Deliberately untrusted/backdated proposed time. The database must not use
    // it as the operation clock and must record the actual acceptance time.
    sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED',external_claim_id='claim-1',claimed_at=offered_at+interval '1 second' WHERE id=$1")
        .bind(o.as_uuid()).execute(c).await?;
    Ok(())
}
async fn feedback(c: &mut PgConnection, o: Id, d: Id, report: Id) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.forward_messages(downstream_id,external_message_id,handoff_id,stream_id,sequence,message_revision,window_start,window_end,coverage_status,observation_count,report_artifact_id,issued_at,received_at) VALUES($1,$2,$3,'test-stream',1,1,clock_timestamp()-interval '1 hour',clock_timestamp(),'COMPLETE',10,$4,clock_timestamp(),clock_timestamp())")
        .bind(d.as_uuid()).bind(Id::new().to_string()).bind(o.as_uuid()).bind(report.as_uuid()).execute(c).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn packages_require_exact_project_kind_schema_and_real_origin(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let other = fixture(&pool, budget()).await;
    let (m, c, e) = portfolio(&pool, &f).await;
    let r = release(&pool, &f, m, c, e).await.unwrap();
    for p in [
        f.artifact,
        package(&pool, other.project, "PACKAGE", "REAL", "DELIVERY").await,
        package(&pool, f.project, "REPORT", "REAL", "DELIVERY").await,
        package(&pool, f.project, "PACKAGE", "FIXTURE", "DELIVERY").await,
        package(&pool, f.project, "PACKAGE", "SYNTHETIC", "DELIVERY").await,
        package(&pool, f.project, "PACKAGE", "REAL", "RESEARCH").await,
    ] {
        sqlstate(
            copy_release(&pool, r, p, "1", "REAL").await.unwrap_err(),
            "23503",
        );
    }
    let p = package(&pool, f.project, "PACKAGE", "REAL", "DELIVERY").await;
    sqlstate(
        copy_release(&pool, r, p, "2", "REAL").await.unwrap_err(),
        "23503",
    );
    copy_release(&pool, r, p, "1", "REAL").await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.releases")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn demo_cannot_receive_paper_or_live_authority(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (m, c, e) = portfolio(&pool, &f).await;
    let r = release(&pool, &f, m, c, e).await.unwrap();
    let d = downstream(&pool).await;
    let i = support::approval_inputs(&pool, &f, e).await;
    for env in ["PAPER", "LIVE"] {
        sqlstate(approve(&pool, r, d, i, env).await.unwrap_err(), "23514");
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.approvals")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.handoff_offers")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let real = delivery_release_metadata(&pool, &f, m, c, e).await.unwrap();
    approve(&pool, real, d, i, "PAPER").await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn research_lineages_reject_self_and_multirow_cycles_preserving_real_ancestry(pool: PgPool) {
    let a = Id::new();
    let b = Id::new();
    sqlstate(sqlx::query("INSERT INTO app.research_lineages(id,origin,parent_lineage_id,reason) VALUES($1,'FORK',$1,'self cycle')")
        .bind(a.as_uuid()).execute(&pool).await.unwrap_err(),"23514");
    sqlstate(sqlx::query("INSERT INTO app.research_lineages(id,origin,parent_lineage_id,reason) VALUES($1,'FORK',$2,'cycle A'),($2,'FORK',$1,'cycle B')")
        .bind(a.as_uuid()).bind(b.as_uuid()).execute(&pool).await.unwrap_err(),"23514");
    sqlx::query(
        "INSERT INTO app.research_lineages(id,origin,reason) VALUES($1,'NEW','original lineage')",
    )
    .bind(a.as_uuid())
    .execute(&pool)
    .await
    .unwrap();
    sqlx::query("INSERT INTO app.research_lineages(id,origin,parent_lineage_id,reason) VALUES($1,'FORK',$2,'valid inherited exposure')")
        .bind(b.as_uuid()).bind(a.as_uuid()).execute(&pool).await.unwrap();
    sqlstate(
        sqlx::query("UPDATE app.research_lineages SET parent_lineage_id=$1 WHERE id=$2")
            .bind(b.as_uuid())
            .bind(a.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23000",
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.research_lineages")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn a_backdated_claim_cannot_resurrect_an_expired_offer(pool: PgPool) {
    let (o, _, _) = delivery(&pool, -1.0).await;
    let mut c = pool.acquire().await.unwrap();
    sqlstate(claim(&mut c, o).await.unwrap_err(), "23514");
    assert!(sqlx::query_scalar::<_,bool>("SELECT state='OFFERED' AND claimed_at IS NULL AND external_claim_id IS NULL FROM app.handoff_offers WHERE id=$1")
        .bind(o.as_uuid()).fetch_one(&pool).await.unwrap());
    let (valid, _, _) = delivery(&pool, 60.0).await;
    let before: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    claim(&mut c, valid).await.unwrap();
    let actual: chrono::DateTime<chrono::Utc> =
        sqlx::query_scalar("SELECT claimed_at FROM app.handoff_offers WHERE id=$1")
            .bind(valid.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(
        actual >= before,
        "must retain actual DB acceptance, not the proposed old timestamp"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn claim_rechecks_clock_after_waiting_for_the_offer_lock(pool: PgPool) {
    let (o, _, _) = delivery(&pool, 1.0).await;
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM app.handoff_offers WHERE id=$1 FOR UPDATE")
        .bind(o.as_uuid())
        .fetch_one(&mut *lock)
        .await
        .unwrap();
    let mut c = pool.acquire().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *c)
        .await
        .unwrap();
    let task = tokio::spawn(async move { claim(&mut c, o).await });
    support::wait_for_database_lock(&pool, pid).await;
    // Real DB clock controls the test; observing the lock wait avoids relying
    // on a sleep to arrange concurrency. Do not mutate immutable expiry fields.
    sqlx::query("SELECT pg_sleep(GREATEST(0,EXTRACT(epoch FROM expires_at-clock_timestamp()))+0.02) FROM app.handoff_offers WHERE id=$1")
        .bind(o.as_uuid()).execute(&mut *lock).await.unwrap();
    lock.commit().await.unwrap();
    sqlstate(task.await.unwrap().unwrap_err(), "23514");
}

#[sqlx::test(migrations = "../../migrations")]
async fn feedback_requires_a_live_transferred_state(pool: PgPool) {
    for state in ["OFFERED", "REVOKED", "EXPIRED", "REJECTED"] {
        let (o, d, r) = delivery(&pool, 60.0).await;
        if state != "OFFERED" {
            sqlx::query("UPDATE app.handoff_offers SET state=$2 WHERE id=$1")
                .bind(o.as_uuid())
                .bind(state)
                .execute(&pool)
                .await
                .unwrap();
        }
        let mut c = pool.acquire().await.unwrap();
        sqlstate(feedback(&mut c, o, d, r).await.unwrap_err(), "23514");
    }
    let (o, d, r) = delivery(&pool, 60.0).await;
    let mut c = pool.acquire().await.unwrap();
    claim(&mut c, o).await.unwrap();
    feedback(&mut c, o, d, r).await.unwrap();
    sqlx::query("UPDATE app.handoff_offers SET state='ACKNOWLEDGED',acknowledged_at=clock_timestamp() WHERE id=$1")
        .bind(o.as_uuid()).execute(&pool).await.unwrap();
    // Different logical sequence for another valid post-transfer report.
    sqlx::query("INSERT INTO app.forward_messages(downstream_id,external_message_id,handoff_id,stream_id,sequence,message_revision,window_start,window_end,coverage_status,observation_count,report_artifact_id,issued_at,received_at) VALUES($1,$2,$3,'test-stream',2,1,clock_timestamp()-interval '1 hour',clock_timestamp(),'PARTIAL',1,$4,clock_timestamp(),clock_timestamp())")
        .bind(d.as_uuid()).bind(Id::new().to_string()).bind(o.as_uuid()).bind(r.as_uuid()).execute(&pool).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.forward_messages")
            .fetch_one(&pool)
            .await
            .unwrap(),
        2
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn feedback_waiting_behind_rejection_does_not_publish_an_ineligible_row(pool: PgPool) {
    let (o, d, r) = delivery(&pool, 60.0).await;
    let mut c = pool.acquire().await.unwrap();
    claim(&mut c, o).await.unwrap();
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("UPDATE app.handoff_offers SET state='REJECTED' WHERE id=$1")
        .bind(o.as_uuid())
        .execute(&mut *lock)
        .await
        .unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *c)
        .await
        .unwrap();
    let task = tokio::spawn(async move { feedback(&mut c, o, d, r).await });
    support::wait_for_database_lock(&pool, pid).await;
    lock.commit().await.unwrap();
    sqlstate(task.await.unwrap().unwrap_err(), "23514");
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.forward_messages")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = false)]
async fn upgrade_rejects_cyclic_history_without_rewriting_the_original_rows(pool: PgPool) {
    support::migrate_before(&pool, 202609060013).await;
    let previous_version: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    let a = Id::new();
    let b = Id::new();
    sqlx::query("INSERT INTO app.research_lineages(id,origin,parent_lineage_id,reason) VALUES($1,'FORK',$2,'bad history A'),($2,'FORK',$1,'bad history B')")
        .bind(a.as_uuid()).bind(b.as_uuid()).execute(&pool).await.unwrap();
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM app.research_lineages l",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(store::Store::from_pool(pool.clone())
        .migrate()
        .await
        .is_err());
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT jsonb_agg(to_jsonb(l) ORDER BY id) FROM app.research_lineages l",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT max(version) FROM _sqlx_migrations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        previous_version
    );
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT to_regprocedure('app.release_package_valid(uuid,uuid,text,text)') IS NULL"
    )
    .fetch_one(&pool)
    .await
    .unwrap());
}

#[sqlx::test(migrations = false)]
async fn upgrade_rejects_historical_demo_authority_without_erasing_it(pool: PgPool) {
    support::migrate_before(&pool, 202609060013).await;
    let f = fixture(&pool, budget()).await;
    let (m, c, e) = portfolio(&pool, &f).await;
    let r = release(&pool, &f, m, c, e).await.unwrap();
    let d = downstream(&pool).await;
    let i = support::approval_inputs(&pool, &f, e).await;
    let a = approve(&pool, r, d, i, "PAPER").await.unwrap();
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(a) FROM app.approvals a WHERE id=$1")
            .bind(a.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(store::Store::from_pool(pool.clone())
        .migrate()
        .await
        .is_err());
    let after: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(a) FROM app.approvals a WHERE id=$1")
            .bind(a.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=202609060013)"
    )
    .fetch_one(&pool)
    .await
    .unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_forward_reports_require_exact_project_kind_origin_and_contract(pool: PgPool) {
    use serde_json::json;
    let (o, d, report) = delivery(&pool, 60.0).await;
    let mut c = pool.acquire().await.unwrap();
    claim(&mut c, o).await.unwrap();
    let other = fixture(&pool, budget()).await;
    let base: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(a) FROM app.artifacts a WHERE id=$1")
            .bind(report.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    for (field, value) in [
        ("project_id", json!(other.project)),
        ("kind", json!("PARAMETERS")),
        ("media_type", json!("text/plain")),
        ("schema_name", json!("qz.unrelated")),
        ("schema_version", json!("2")),
        ("origin", json!("FIXTURE")),
        ("access_class", json!("RESEARCH")),
        ("byte_count", json!(0)),
    ] {
        let id = Id::new();
        let mut row = base.clone();
        row["id"] = json!(id);
        row["storage_object_ref"] = json!(format!("relational-negative/{id}"));
        row[field] = value;
        sqlx::query(
            "INSERT INTO app.artifacts SELECT (jsonb_populate_record(NULL::app.artifacts,$1)).*",
        )
        .bind(row)
        .execute(&pool)
        .await
        .unwrap();
        sqlstate(feedback(&mut c, o, d, id).await.unwrap_err(), "23503");
    }
    feedback(&mut c, o, d, report).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.forward_messages")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_forward_rejection_cannot_fabricate_a_transfer(pool: PgPool) {
    let (o, _, _) = delivery(&pool, 60.0).await;
    sqlstate(sqlx::query("UPDATE app.handoff_offers SET state='REJECTED',external_claim_id='invented',claimed_at=clock_timestamp() WHERE id=$1")
        .bind(o.as_uuid()).execute(&pool).await.unwrap_err(), "23514");
    sqlx::query("UPDATE app.handoff_offers SET state='REJECTED' WHERE id=$1")
        .bind(o.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.handoff_transfers WHERE handoff_id=$1")
            .bind(o.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_forward_valid_history_survives_post_claim_rejection(pool: PgPool) {
    let (o, d, r) = delivery(&pool, 60.0).await;
    let mut c = pool.acquire().await.unwrap();
    claim(&mut c, o).await.unwrap();
    feedback(&mut c, o, d, r).await.unwrap();
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(t) FROM app.handoff_transfers t WHERE handoff_id=$1")
            .bind(o.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before["provenance"], "RECORDED_TRANSITION");
    sqlx::query("UPDATE app.handoff_offers SET state='REJECTED' WHERE id=$1")
        .bind(o.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    sqlstate(feedback(&mut c, o, d, r).await.unwrap_err(), "23514");
    for sql in [
        "UPDATE app.handoff_transfers SET provenance='LEGACY_CLAIMED_STATE' WHERE handoff_id=$1",
        "DELETE FROM app.handoff_transfers WHERE handoff_id=$1",
    ] {
        sqlstate(
            sqlx::query(sql)
                .bind(o.as_uuid())
                .execute(&pool)
                .await
                .unwrap_err(),
            "23000",
        );
    }
    let after: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(t) FROM app.handoff_transfers t WHERE handoff_id=$1")
            .bind(o.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.forward_messages")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    store::Store::from_pool(pool.clone())
        .migrate()
        .await
        .unwrap();
}

async fn assert_forward_upgrade_preserves_failure(pool: &PgPool) {
    let before:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('offers',(SELECT jsonb_agg(to_jsonb(h) ORDER BY id) FROM app.handoff_offers h),'feedback',(SELECT jsonb_agg(to_jsonb(m) ORDER BY id) FROM app.forward_messages m),'versions',(SELECT jsonb_agg(version ORDER BY version) FROM _sqlx_migrations))")
        .fetch_one(pool).await.unwrap();
    assert!(store::Store::from_pool(pool.clone())
        .migrate()
        .await
        .is_err());
    let after:serde_json::Value=sqlx::query_scalar("SELECT jsonb_build_object('offers',(SELECT jsonb_agg(to_jsonb(h) ORDER BY id) FROM app.handoff_offers h),'feedback',(SELECT jsonb_agg(to_jsonb(m) ORDER BY id) FROM app.forward_messages m),'versions',(SELECT jsonb_agg(version ORDER BY version) FROM _sqlx_migrations))")
        .fetch_one(pool).await.unwrap();
    assert_eq!(before, after);
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT to_regclass('app.handoff_transfers') IS NULL")
            .fetch_one(pool)
            .await
            .unwrap()
    );
}

#[sqlx::test(migrations = false)]
async fn review_forward_upgrade_rejects_feedback_created_before_actual_claim(pool: PgPool) {
    support::migrate_before(&pool, 202609060013).await;
    let (o, d, r) = delivery(&pool, 60.0).await;
    let mut c = pool.acquire().await.unwrap();
    feedback(&mut c, o, d, r).await.unwrap();
    sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED',external_claim_id='later-native-claim',claimed_at=clock_timestamp() WHERE id=$1")
        .bind(o.as_uuid()).execute(&pool).await.unwrap();
    let out_of_order:bool=sqlx::query_scalar("SELECT m.created_at<h.claimed_at AND m.received_at<h.claimed_at FROM app.forward_messages m JOIN app.handoff_offers h ON h.id=m.handoff_id WHERE h.id=$1")
        .bind(o.as_uuid()).fetch_one(&pool).await.unwrap();
    assert!(
        out_of_order,
        "must reproduce the actual historical chronology"
    );
    assert_forward_upgrade_preserves_failure(&pool).await;
}

#[sqlx::test(migrations = false)]
async fn review_forward_upgrade_rejects_ambiguous_rejected_claim_fields(pool: PgPool) {
    support::migrate_before(&pool, 202609060013).await;
    let (o, d, r) = delivery(&pool, 60.0).await;
    sqlx::query("UPDATE app.handoff_offers SET state='REJECTED',external_claim_id='never-claimed',claimed_at=clock_timestamp() WHERE id=$1")
        .bind(o.as_uuid()).execute(&pool).await.unwrap();
    feedback(&mut pool.acquire().await.unwrap(), o, d, r)
        .await
        .unwrap();
    assert_forward_upgrade_preserves_failure(&pool).await;
}

#[sqlx::test(migrations = false)]
async fn review_forward_upgrade_rejects_unrelated_report_preserving_history(pool: PgPool) {
    support::migrate_before(&pool, 202609060017).await;
    let (o, d, _) = delivery(&pool, 60.0).await;
    let other = fixture(&pool, budget()).await;
    let mut c = pool.acquire().await.unwrap();
    claim(&mut c, o).await.unwrap();
    feedback(&mut c, o, d, other.artifact).await.unwrap();
    assert_forward_upgrade_preserves_failure(&pool).await;
}

#[sqlx::test(migrations = false)]
async fn review_forward_upgrade_records_valid_legacy_state_without_rewriting_feedback(
    pool: PgPool,
) {
    support::migrate_before(&pool, 202609060017).await;
    let (o, d, r) = delivery(&pool, 60.0).await;
    let mut c = pool.acquire().await.unwrap();
    claim(&mut c, o).await.unwrap();
    feedback(&mut c, o, d, r).await.unwrap();
    let before: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(m) FROM app.forward_messages m WHERE handoff_id=$1")
            .bind(o.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let store = store::Store::from_pool(pool.clone());
    store.migrate().await.unwrap();
    store.migrate().await.unwrap();
    let after: serde_json::Value =
        sqlx::query_scalar("SELECT to_jsonb(m) FROM app.forward_messages m WHERE handoff_id=$1")
            .bind(o.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(before, after);
    let provenance: String =
        sqlx::query_scalar("SELECT provenance FROM app.handoff_transfers WHERE handoff_id=$1")
            .bind(o.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(provenance, "LEGACY_CLAIMED_STATE");
    sqlx::query("UPDATE app.handoff_offers SET state='REJECTED' WHERE id=$1")
        .bind(o.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    store.migrate().await.unwrap();
}
