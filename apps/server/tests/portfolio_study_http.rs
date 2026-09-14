//! Real HTTP/PG admission from the original controlled qualification chain.
//! No SQL-authored qualification. Actual Worker terminal publication/ACK, not OCI science.
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "support/client.rs"]
#[allow(dead_code)]
mod client;
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/experiment_tasks.rs"]
mod experiment_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/experiments.rs"]
mod proposal_support;
#[path = "../../../crates/store/tests/support/qualified_portfolio.rs"]
mod qualified_portfolio;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
#[allow(dead_code)]
mod support;
#[path = "../../../crates/store/tests/support/validation_publication.rs"]
mod validation_publication;
use contracts::{
    execution::NativeTaskParametersV1, runtime_jobs::RuntimeInputV1, DbCounter, Id, SchemaV1,
};
use experiment_support::{
    complete_compilation, forecast, limits, result_turn, setup, start, trial_usage, validation,
};
use sqlx::{PgPool, Row};
use store::{
    lifecycle::{ClaimResult, RunLease},
    Store, StoreError,
};

#[sqlx::test(migrations = "../../migrations")]
async fn original_candidate_study_http_admits_replays_and_reads_cancelled_evidence(pool: PgPool) {
    let (store, actor, f, build, candidate, directory) = Box::pin(
        qualified_portfolio::qualified_chain(pool.clone(), cycle_support::Liquidity::None),
    )
    .await
    .unwrap();
    // The real Store chain has unwound before entering HTTP/crypto poll frames.
    Box::pin(http(
        &pool, &store, &actor, &f, &build, candidate, &directory,
    ))
    .await;
}

async fn http(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    build: &contracts::portfolio::PortfolioBuildRequestV1,
    candidate: Id,
    directory: &tempfile::TempDir,
) {
    use contracts::control::*;
    use integrations::authentication::{
        capability_verifier, format_machine_token, random_capability,
    };
    let web = support::fixture(pool.clone()).await;
    let vault = integrations::secrets::SecretVault::open(
        &web._state.path().join("secrets"),
        &web._state.path().join("master.key"),
    )
    .unwrap();
    let principal = store
        .create_principal(
            actor,
            "study-http-cli",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "Controlled Study HTTP".into(),
                kind: AssignablePrincipalKind::Cli,
                project_id: Some(f.data.project),
                downstream_id: None,
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let request = CredentialIssue {
        schema_version: SchemaV1,
        scope_codes: vec![
            MachineScope::ResearchRead,
            MachineScope::EvidenceRead,
            MachineScope::RunRead,
        ],
        expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
    };
    let store::control::CredentialPreparation::New(prepared) = store
        .prepare_credential_issuance(actor, "study-http-token", principal.id, &request)
        .await
        .unwrap()
    else {
        panic!("fresh test credential");
    };
    let public = Id::new();
    let secret = random_capability();
    let verifier = capability_verifier(&secret).unwrap();
    let reference = vault.put("MACHINE_VERIFIER", verifier.as_bytes()).unwrap();
    prepared.publish(public, reference).await.unwrap();
    let token = format_machine_token(public, &secret).unwrap();
    let cli = store
        .machine_challenge(public)
        .await
        .unwrap()
        .verified_actor(None);
    let intent = contracts::portfolio::PortfolioStudyRequestV1 {
        schema_version: SchemaV1,
        candidate_id: candidate,
        cycle_id: build.cycle_id,
        runtime_id: build.runtime_id,
        expected_runtime_revision: build.expected_runtime_revision,
        limits: build.limits.clone(),
    };
    let snapshot = store.authentication_snapshot().await.unwrap();
    let grant = store
        .issue_operator_grant(
            &cli,
            "study-http-grant",
            &OperatorCommand::PortfolioStudy(intent.clone()),
            Some(candidate),
            &snapshot,
            snapshot.database_now.timestamp() / 30 + 1,
        )
        .await
        .unwrap()
        .resource;
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    let origin = format!("http://{address}");
    let state = server::AppState::new(
        store.clone(),
        vault,
        server::WebPolicy::new(&origin, address, true).unwrap(),
    )
    .with_artifact_store(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let app = server::router(state, tower_sessions::cookie::Key::generate());
    let mut listener = tokio::task::JoinSet::new();
    listener.spawn(async move {
        axum::serve(socket, app).await.unwrap();
    });
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .build()
        .unwrap();
    let post = || {
        client
            .post(format!("{origin}/api/v2/portfolio-studies"))
            .bearer_auth(&token)
            .header("Idempotency-Key", "original-study-http")
            .header("X-Operator-Grant", grant.id.to_string())
            .json(&intent)
    };
    let mut run = None;
    for replayed in [false, true] {
        let response = post().send().await.unwrap();
        assert_eq!(response.status(), reqwest::StatusCode::ACCEPTED);
        let receipt: CommandResult<contracts::runs::RunSnapshotV1> = response.json().await.unwrap();
        assert_eq!(receipt.replayed, replayed);
        if let Some(id) = run {
            assert_eq!(receipt.resource.id, id);
        }
        run = Some(receipt.resource.id);
        assert_eq!(receipt.resource.cycle_id, Some(build.cycle_id));
        assert_eq!(
            receipt.resource.kind,
            contracts::runs::RunKind::PortfolioSimulate
        );
    }
    let run = store.get_run(actor, run.unwrap()).await.unwrap();
    store
        .cancel_run(
            actor,
            "study-http-cancel",
            run.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: run.revision,
            },
        )
        .await
        .unwrap();
    let message = validation_publication::message(pool, run.id).await;
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    let worker = server::worker::Worker::new(
        store.clone(),
        integrations::secrets::SecretVault::open(
            &web._state.path().join("secrets"),
            &web._state.path().join("master.key"),
        )
        .unwrap(),
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
        // A pre-dispatch cancellation must not contact any Runtime.
        server::runtime_transport::RuntimeTargets::new(Vec::new(), false).unwrap(),
        1,
    )
    .unwrap();
    let (_stop, shutdown) = tokio::sync::watch::channel(false);
    Box::pin(worker.process_message(message.clone(), "study-http-worker", shutdown.clone()))
        .await
        .unwrap();
    let published: uuid::Uuid = sqlx::query_scalar("SELECT e.id FROM app.evaluations e JOIN app.evaluation_publications p ON p.evaluation_id=e.id WHERE e.run_id=$1 AND e.evaluation_kind='PORTFOLIO'")
        .bind(run.id.as_uuid()).fetch_one(pool).await.unwrap();
    let archived: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pgmq.a_runs WHERE msg_id=$1) AND NOT EXISTS(SELECT 1 FROM pgmq.q_runs WHERE msg_id=$1)")
        .bind(message.message_id).fetch_one(pool).await.unwrap();
    assert!(archived, "Worker ACK must follow independent publication");
    // An already archived message is rejected, not treated as new work.
    assert!(
        Box::pin(worker.process_message(message, "study-http-worker-replay", shutdown))
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.evaluations WHERE run_id=$1")
        .bind(run.id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let response = client
        .get(format!("{origin}/api/v2/evaluations/{}", published))
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), reqwest::StatusCode::OK);
    let evaluation: contracts::evidence::EvaluationView = response.json().await.unwrap();
    assert_eq!(evaluation.subject_candidate_id, Some(candidate));
    assert_eq!(
        evaluation.evaluation_kind,
        contracts::evidence::EvaluationKind::Portfolio
    );
    assert_eq!(
        evaluation.decision,
        contracts::evidence::Decision::Inconclusive
    );
    listener.abort_all();
    while listener.join_next().await.is_some() {}
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_package_claim_cli_transfers_once_and_replays(pool: PgPool) {
    let (store, actor, f, build, candidate, directory) =
        Box::pin(qualified_portfolio::qualified_chain_policy(
            pool.clone(),
            cycle_support::Liquidity::None,
            contracts::forward::ForwardEnvironmentV1::Live,
            qualified_portfolio::release_policy,
        ))
        .await
        .unwrap();
    let (release, _, _) = Box::pin(qualified_portfolio::original_releases(
        &pool, &store, &actor, &f, &build, candidate,
    ))
    .await
    .unwrap();
    Box::pin(claim_http(&pool, &store, &actor, &f, &directory, &release)).await;
}

async fn claim_http(
    pool: &PgPool,
    store: &Store,
    operator: &store::authority::Actor,
    f: &cycle_support::Fixture,
    directory: &tempfile::TempDir,
    release: &contracts::delivery::ReleaseViewV1,
) {
    use contracts::{control::*, delivery::*, forward::ForwardEnvironmentV1, settings::*};
    use integrations::authentication::{
        capability_verifier, format_machine_token, random_capability,
    };
    use std::{fs, os::unix::fs::PermissionsExt};
    let down = store
        .create_downstream(
            operator,
            "claim-http-downstream",
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: "Claim HTTP protocol fixture".into(),
                    endpoint: "https://claim.example".into(),
                    accepted_package_versions: vec![PackageSchemaVersion::V1],
                    environments: DownstreamEnvironments::Paper,
                    enabled: true,
                    development_http: false,
                },
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    let store::downstream::ProbePreparation::Pending(ticket) = store
        .prepare_downstream_probe(
            operator,
            "claim-http-probe",
            down.id,
            &DownstreamProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: down.revision,
            },
        )
        .await
        .unwrap()
    else {
        panic!("new probe")
    };
    store
        .complete_downstream_probe(
            *ticket,
            DownstreamProbeOutcomeV1::Available {
                capabilities: DownstreamCapabilitiesV1 {
                    schema_version: SchemaV1,
                    delivery_mode: DownstreamDeliveryModeV1::TargetOnly,
                    accepted_package_versions: vec![PackageSchemaVersion::V1],
                    environments: vec![ForwardEnvironmentV1::Paper],
                    market_capability_versions: vec![release.market_capability_version.clone()],
                    accepting_targets: true,
                    checked_at: chrono::Utc::now(),
                },
            },
            |id, bytes| async move { f.objects.put(id, &bytes).map_err(|_| StoreError::Integrity) },
        )
        .await
        .unwrap();
    let approval = Box::pin(store.approve_release(
        operator,
        "claim-http-approval",
        release.id,
        &ReleaseApproveV1 {
            schema_version: SchemaV1,
            downstream_id: down.id,
            environment: ForwardEnvironmentV1::Paper,
            expected_downstream_revision: down.revision,
            expected_latest_decision_id: None,
            valid_until: release.valid_until,
        },
        |id, size| f.read(id, size),
    ))
    .await
    .unwrap()
    .resource;
    let offer = Box::pin(store.offer_handoff(
        operator,
        "claim-http-offer",
        &HandoffOfferV1 {
            schema_version: SchemaV1,
            release_id: release.id,
            approval_id: approval.id,
            supersedes_handoff_id: None,
            expires_at: release.valid_until,
        },
        |id, size| f.read(id, size),
    ))
    .await
    .unwrap()
    .resource;
    let web = support::fixture(pool.clone()).await;
    let vault = integrations::secrets::SecretVault::open(
        &web._state.path().join("secrets"),
        &web._state.path().join("master.key"),
    )
    .unwrap();
    let principal = store
        .create_principal(
            operator,
            "claim-http-principal",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "HTTP claim consumer".into(),
                kind: AssignablePrincipalKind::Downstream,
                project_id: Some(release.project_id),
                downstream_id: Some(down.id),
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let store::control::CredentialPreparation::New(prepared) = store
        .prepare_credential_issuance(
            operator,
            "claim-http-credential",
            principal.id,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![MachineScope::DownstreamClaim, MachineScope::DownstreamAck],
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new credential")
    };
    let public = Id::new();
    let secret = random_capability();
    let verifier = capability_verifier(&secret).unwrap();
    let reference = vault.put("MACHINE_VERIFIER", verifier.as_bytes()).unwrap();
    prepared.publish(public, reference).await.unwrap();
    let token = format_machine_token(public, &secret).unwrap();
    let credential = directory.path().join("claim-credential");
    fs::write(&credential, &token).unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    let origin = format!("http://{address}");
    let state = server::AppState::new(
        store.clone(),
        vault,
        server::WebPolicy::new(&origin, address, true).unwrap(),
    )
    .with_artifact_store(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let app = server::router(state, tower_sessions::cookie::Key::generate());
    let mut listener = tokio::task::JoinSet::new();
    listener.spawn(async move {
        axum::serve(socket, app).await.unwrap();
    });
    let id = offer.id.to_string();
    let body = serde_json::json!({"schema_version":1,"external_claim_id":"http-original-claim","package_schema_version":"1"});
    for replayed in [false, true] {
        let response = client::invoke(
            &origin,
            &credential,
            &[
                "--idempotency-key",
                "http-original-claim",
                "handoff",
                "claim",
                &id,
            ],
            body.clone(),
        )
        .await;
        let diagnostic: serde_json::Value =
            serde_json::from_slice(&response.stderr).unwrap_or(serde_json::Value::Null);
        assert!(
            response.status.success(),
            "claim HTTP failed: code={} status={}",
            diagnostic["code"],
            diagnostic["status"]
        );
        let result: CommandResult<HandoffClaimViewV1> =
            serde_json::from_slice(&response.stdout).unwrap();
        assert_eq!(result.replayed, replayed);
        assert_eq!(result.resource.handoff.state, HandoffStateV1::Claimed);
        assert_eq!(result.resource.package.release_id, release.id);
    }
    let changed = serde_json::json!({"schema_version":1,"external_claim_id":"second-http-claim","package_schema_version":"1"});
    let denied = client::invoke(
        &origin,
        &credential,
        &[
            "--idempotency-key",
            "second-http-claim",
            "handoff",
            "claim",
            &id,
        ],
        changed,
    )
    .await;
    assert!(!denied.status.success());
    assert!(!String::from_utf8_lossy(&denied.stderr).contains(&token));
    assert_eq!(
        serde_json::from_slice::<serde_json::Value>(&denied.stderr).unwrap()["status"],
        409
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.handoff_transfers WHERE handoff_id=$1"
        )
        .bind(offer.id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap(),
        1
    );
    let shown = client::invoke(
        &origin,
        &credential,
        &["handoff", "show", &id],
        serde_json::Value::Null,
    )
    .await;
    assert!(shown.status.success());
    assert_eq!(
        serde_json::from_slice::<HandoffViewV1>(&shown.stdout)
            .unwrap()
            .state,
        HandoffStateV1::Claimed
    );
    let ack = serde_json::json!({"schema_version":1,"external_ack_id":"http-ack","external_claim_id":"http-original-claim","outcome":"ACKNOWLEDGED","reason_code":"ACCEPTED","reason":"Original package accepted"});
    for replayed in [false, true] {
        let response = client::invoke(
            &origin,
            &credential,
            &["--idempotency-key", "http-ack", "handoff", "ack", &id],
            ack.clone(),
        )
        .await;
        let diagnostic: serde_json::Value =
            serde_json::from_slice(&response.stderr).unwrap_or(serde_json::Value::Null);
        assert!(
            response.status.success(),
            "ACK HTTP failed: code={} status={}",
            diagnostic["code"],
            diagnostic["status"]
        );
        let result: CommandResult<HandoffViewV1> =
            serde_json::from_slice(&response.stdout).unwrap();
        assert_eq!(result.replayed, replayed);
        assert_eq!(result.resource.state, HandoffStateV1::Acknowledged);
        assert!(result.resource.acknowledged_at.is_some());
    }
    listener.abort_all();
}
