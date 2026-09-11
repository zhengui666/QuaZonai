//! Actual MCP SDK -> TCP HTTP -> native crypto -> PostgreSQL -> ArtifactStore.
//! Parent Cycle/data are explicit relational fixtures, not T42 fresh-user acceptance.
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "../../../tests/support/experiments.rs"]
mod experiment_support;
#[path = "support/mcp_authoring.rs"]
mod native;
#[path = "../../../tests/support/research.rs"]
mod research_support;
use contracts::{
    control::CommandResult,
    experiments::{ExperimentOutcome, ExperimentSource, ExperimentView},
    Id,
};
use integrations::artifacts::ArtifactStore;
use native::{body, call, client, fixture, upload};
use serde_json::json;
use sqlx::PgPool;
use std::{fs, os::unix::fs::symlink};

#[sqlx::test(migrations = "../../migrations")]
async fn sdk_publishes_original_artifacts_and_proposal(pool: PgPool) {
    let f = fixture(
        &pool,
        &[
            "RUN_READ",
            "RESEARCH_READ",
            "ARTIFACT_SUBMIT",
            "EXPERIMENT_SUBMIT",
        ],
    )
    .await;
    let (client, task) = client(&f).await;
    let mut proposal = f.research.request.clone();
    for (kind, path) in [
        ("CODE", "alpha.rs"),
        ("PARAMETERS", "parameters.json"),
        ("REPORT", "proposal.json"),
    ] {
        let response = upload(&client, kind, path, path).await;
        assert_ne!(response["isError"], true, "{response}");
        let original = body(&response);
        assert_eq!(original["resource"]["created_by"], "AGENT");
        assert_eq!(original["resource"]["origin"], "SYNTHETIC");
        assert_eq!(
            original["resource"]["producer_attempt_id"],
            json!(f.binding.attempt_id)
        );
        let id: Id = original["resource"]["id"]
            .as_str()
            .unwrap()
            .to_owned()
            .try_into()
            .unwrap();
        let locator = f.store.artifact_content(&f.operator, id).await.unwrap();
        let objects = ArtifactStore::open(&f.private.path().join("artifacts")).unwrap();
        assert_eq!(
            objects.read(id, locator.metadata.byte_count).unwrap(),
            fs::read(f.work.join(path)).unwrap()
        );
        match kind {
            "CODE" => proposal.code_artifact_id = Some(id),
            "PARAMETERS" => proposal.parameter_artifact_id = id,
            _ => proposal.proposal_artifact_id = id,
        }
        let retry = body(&upload(&client, kind, path, path).await);
        assert_eq!(retry["replayed"], true);
        assert_eq!(retry["resource"], original["resource"]);
    }
    let intent = json!({"idempotency_key":"experiment","proposal":proposal});
    let response = call(&client, "experiment.propose", intent.clone()).await;
    assert_ne!(response["isError"], true, "{response}");
    let initial: CommandResult<ExperimentView> = serde_json::from_value(body(&response)).unwrap();
    assert_eq!(initial.resource.author_run_id, Some(f.binding.run_id));
    assert_eq!(initial.resource.trial_source, ExperimentSource::Codex);
    assert_eq!(initial.resource.outcome, Some(ExperimentOutcome::Pending));
    assert!(initial.resource.run_id.is_none());
    let retry = body(&call(&client, "experiment.propose", intent.clone()).await);
    assert_eq!(retry["replayed"], true);
    assert_eq!(retry["resource"], body(&response)["resource"]);
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.artifacts WHERE producer_run_id=$1),(SELECT count(*) FROM app.experiment_authorship WHERE author_run_id=$1)")
        .bind(f.binding.run_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (3, 1));
    let mut changed = intent;
    changed["proposal"]["hypothesis"] = json!("different premise");
    let rejected = call(&client, "experiment.propose", changed).await;
    assert_eq!(rejected["isError"], true);
    assert_eq!(body(&rejected)["http_status"], 409);
    fs::write(
        f.work.join("alpha.rs"),
        "pub fn forecast() -> f64 { 0.2 }\n",
    )
    .unwrap();
    let rejected = upload(&client, "CODE", "alpha.rs", "alpha.rs").await;
    assert_eq!(rejected["isError"], true);
    assert_eq!(body(&rejected)["http_status"], 409);
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn file_scope_and_revocation_rejections_leave_no_artifact(pool: PgPool) {
    let f = fixture(&pool, &["RUN_READ", "RESEARCH_READ", "ARTIFACT_SUBMIT"]).await;
    let (client, task) = client(&f).await;
    symlink(
        f.private.path().join("master.key"),
        f.work.join("linked.rs"),
    )
    .unwrap();
    for path in ["../master.key", "linked.rs", "/etc/passwd", ".git/config"] {
        let rejected = upload(&client, "CODE", path, "bad-path").await;
        assert_eq!(rejected["isError"], true);
        assert_eq!(body(&rejected)["code"], "MCP_WORKSPACE_FILE_REJECTED");
        assert!(!rejected
            .to_string()
            .contains(f.private.path().to_str().unwrap()));
    }
    let denied = call(
        &client,
        "experiment.propose",
        json!({"idempotency_key":"no-scope","proposal":f.research.request}),
    )
    .await;
    assert_eq!(denied["isError"], true);
    assert_eq!(body(&denied)["code"], "MCP_AUTHORITY_REJECTED");
    sqlx::query("INSERT INTO app.machine_credential_revocations(credential_id,effective_at,reason) VALUES($1,clock_timestamp(),'test revoke')")
        .bind(f.credential.as_uuid()).execute(&pool).await.unwrap();
    let rejected = upload(&client, "CODE", "alpha.rs", "revoked").await;
    assert_eq!(rejected["isError"], true);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.artifacts WHERE producer_run_id=$1")
            .bind(f.binding.run_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}

#[sqlx::test(migrations = "../../migrations")]
async fn old_mcp_cannot_publish_after_attempt_replacement(pool: PgPool) {
    let f = fixture(
        &pool,
        &[
            "RUN_READ",
            "RESEARCH_READ",
            "ARTIFACT_SUBMIT",
            "EXPERIMENT_SUBMIT",
        ],
    )
    .await;
    let (client, task) = client(&f).await;
    let attempt = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,dispatch_state,runtime_state) VALUES($1,$2,2,'replacement',1,clock_timestamp()+interval '1 hour','NOT_SENT','UNKNOWN')")
        .bind(attempt.as_uuid()).bind(f.binding.run_id.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE app.runs SET active_attempt_id=$1,current_attempt_no=2 WHERE id=$2")
        .bind(attempt.as_uuid())
        .bind(f.binding.run_id.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    let rejected = upload(&client, "CODE", "alpha.rs", "old-attempt").await;
    assert_eq!(rejected["isError"], true);
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.artifacts WHERE producer_run_id=$1),(SELECT count(*) FROM app.experiment_authorship WHERE author_run_id=$1)")
        .bind(f.binding.run_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (0, 0));
    client.cancel().await.unwrap();
    assert!(task.await.unwrap().is_ok());
}
