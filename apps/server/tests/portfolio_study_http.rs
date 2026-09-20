//! Real HTTP/PG admission from the original controlled qualification chain.
//! No SQL-authored qualification. Actual Worker terminal publication/ACK, not OCI science.
#[path = "support/automatic_live.rs"]
mod automatic_live;
#[path = "support/automatic_paper.rs"]
mod automatic_paper;
#[path = "support/automatic_rebalance.rs"]
mod automatic_rebalance;
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "support/client.rs"]
#[allow(dead_code)]
mod client;
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/experiment_tasks.rs"]
mod experiment_support;
#[path = "support/forward_messages.rs"]
mod forward_messages;
#[path = "../../../tests/support/forward_result.rs"]
mod forward_result;
#[path = "../../../tests/support/forward.rs"]
#[allow(dead_code)]
mod forward_support;
#[path = "support/graph_recovery.rs"]
mod graph_recovery;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "support/postgres.rs"]
mod postgres;
#[path = "../../../tests/support/experiments.rs"]
mod proposal_support;
#[path = "../../../crates/store/tests/support/qualified_portfolio.rs"]
mod qualified_portfolio;
use forward_support::{
    research as research_support, runtime_observation::protocol_fixture as runtime_support,
};
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
    let foreign = store
        .create_project(
            actor,
            "release-list-other-project",
            &ProjectCreate {
                schema_version: SchemaV1,
                name: "Release list isolation".into(),
                description: "Native read boundary".into(),
                fork_from_project_id: None,
            },
        )
        .await
        .unwrap()
        .resource;
    let release_url = format!("{origin}/api/v2/projects/{}/releases", f.data.project);
    assert_eq!(
        client.get(&release_url).send().await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let empty = client
        .get(&release_url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(empty.status(), reqwest::StatusCode::OK);
    let empty: contracts::control::Page<contracts::delivery::ReleaseViewV1> =
        empty.json().await.unwrap();
    assert!(empty.items.is_empty());
    assert!(empty.next_cursor.is_none());
    assert_eq!(
        client
            .get(format!("{release_url}?limit=0"))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        client
            .get(format!("{origin}/api/v2/projects/{}/releases", foreign.id))
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::NOT_FOUND
    );
    use std::os::unix::fs::PermissionsExt;
    let release_credential = directory.path().join("release-read-credential");
    std::fs::write(&release_credential, &token).unwrap();
    std::fs::set_permissions(&release_credential, std::fs::Permissions::from_mode(0o600)).unwrap();
    let listed = client::invoke(
        &origin,
        &release_credential,
        &[
            "release",
            "list",
            &f.data.project.to_string(),
            "--limit",
            "1",
        ],
        serde_json::Value::Null,
    )
    .await;
    assert!(listed.status.success());
    let listed: contracts::control::Page<contracts::delivery::ReleaseViewV1> =
        serde_json::from_slice(&listed.stdout).unwrap();
    assert!(listed.items.is_empty());
    assert!(listed.next_cursor.is_none());
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
    let provenance_url = format!("{origin}/api/v2/runs/{}/rebalance", run.id);
    assert_eq!(
        client.get(&provenance_url).send().await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    let provenance = client
        .get(&provenance_url)
        .bearer_auth(&token)
        .send()
        .await
        .unwrap();
    assert_eq!(provenance.status(), reqwest::StatusCode::OK);
    let provenance: contracts::runs::RunRebalanceViewV1 = provenance.json().await.unwrap();
    assert!(
        provenance.rebalance.is_none(),
        "manual Study has no automatic provenance"
    );
    let read = client::invoke(
        &origin,
        &release_credential,
        &["run", "rebalance", &run.id.to_string()],
        serde_json::Value::Null,
    )
    .await;
    assert!(read.status.success());
    let read: contracts::runs::RunRebalanceViewV1 = serde_json::from_slice(&read.stdout).unwrap();
    assert!(read.rebalance.is_none());

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
            contracts::research::DataUse::ResearchAndPaper,
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
    let web = support::fixture(pool.clone()).await;
    let vault = integrations::secrets::SecretVault::open(
        &web._state.path().join("secrets"),
        &web._state.path().join("master.key"),
    )
    .unwrap();
    const PROBE_SECRET: &str = "disposable-worker-probe-credential";
    let probe_credential = vault.put("DOWNSTREAM", PROBE_SECRET.as_bytes()).unwrap();
    let probe_socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let probe_address = probe_socket.local_addr().unwrap();
    let probe_endpoint = format!("http://{probe_address}");
    let calls = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = calls.clone();
    let market = release.market_capability_version.clone();
    let probe_app=axum::Router::new().route("/downstream/v1/capabilities",axum::routing::get(move |headers:axum::http::HeaderMap| {
        assert!(headers[axum::http::header::AUTHORIZATION]==format!("Bearer {PROBE_SECRET}"),"probe fixture bearer mismatch");
        counted.fetch_add(1,std::sync::atomic::Ordering::SeqCst);
        let market=market.clone();
        async move {axum::Json(serde_json::json!({"schema_version":1,"delivery_mode":"TARGET_ONLY","accepted_package_versions":["1"],"environments":["PAPER"],"market_capability_versions":[market],"accepting_targets":true,"checked_at":chrono::Utc::now()}))}
    }));
    let probe_server =
        tokio::spawn(async move { axum::serve(probe_socket, probe_app).await.unwrap() });
    let down = store
        .create_downstream(
            operator,
            "claim-http-downstream",
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: probe_credential,
                configuration: DownstreamConfigurationV1 {
                    name: "Claim HTTP protocol fixture".into(),
                    endpoint: probe_endpoint.clone(),
                    accepted_package_versions: vec![PackageSchemaVersion::V1],
                    environments: DownstreamEnvironments::Paper,
                    enabled: true,
                    development_http: true,
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
    // Wait for the original immutable observation's refresh window; never edit its TTL.
    assert!(store.prepare_downstream_refresh().await.unwrap().is_none());
    tokio::time::sleep(std::time::Duration::from_secs(46)).await;
    let (a, b) = tokio::join!(
        store.prepare_downstream_refresh(),
        store.prepare_downstream_refresh()
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.is_some(), b.is_some());
    let ticket = a.or(b).unwrap();
    let failed = store
        .complete_downstream_refresh(
            ticket,
            DownstreamProbeOutcomeV1::Unavailable {
                reason: contracts::runtime::RuntimeProbeFailure::EndpointDenied,
            },
            |_, _| async { Err(StoreError::Integrity) },
        )
        .await;
    assert!(matches!(failed, Err(StoreError::Integrity)));
    assert!(store.prepare_downstream_refresh().await.unwrap().is_none());
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.downstream_probe_observations WHERE downstream_id=$1"
        )
        .bind(down.id.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap(),
        1
    );
    // Fault injection changes only the mutable retry reservation, never observed evidence.
    sqlx::query("UPDATE app.downstream_probe_refresh SET lease_until=clock_timestamp()-interval '1 second',next_attempt_at=clock_timestamp() WHERE downstream_id=$1").bind(down.id.as_uuid()).execute(pool).await.unwrap();
    let stale = store.prepare_downstream_refresh().await.unwrap().unwrap();
    sqlx::query("UPDATE app.downstream_probe_refresh SET lease_id=$2,lease_until=clock_timestamp()-interval '1 second',next_attempt_at=clock_timestamp() WHERE downstream_id=$1").bind(down.id.as_uuid()).bind(Id::new().as_uuid()).execute(pool).await.unwrap();
    assert!(matches!(
        store
            .complete_downstream_refresh(
                stale,
                DownstreamProbeOutcomeV1::Unavailable {
                    reason: contracts::runtime::RuntimeProbeFailure::EndpointDenied
                },
                |_, _| async { panic!("lost reservation must not publish") }
            )
            .await,
        Err(StoreError::Conflict)
    ));
    let worker_vault = integrations::secrets::SecretVault::open(
        &web._state.path().join("secrets"),
        &web._state.path().join("master.key"),
    )
    .unwrap();
    let worker = server::worker::Worker::new(
        store.clone(),
        worker_vault,
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
        server::runtime_transport::RuntimeTargets::default(),
        1,
    )
    .unwrap()
    .with_downstream_targets(
        server::runtime_transport::RuntimeTargets::new(
            vec![server::runtime_transport::RuntimeTarget {
                origin: probe_endpoint,
                addresses: vec![probe_address],
            }],
            true,
        )
        .unwrap(),
    );
    let (shutdown, observed) = tokio::sync::watch::channel(false);
    let first = tokio::spawn(worker.clone().run(observed.clone()));
    let second = tokio::spawn(worker.run(observed));
    tokio::time::timeout(std::time::Duration::from_secs(15), async {
        loop {
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM app.downstream_probe_observations WHERE downstream_id=$1",
            )
            .bind(down.id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
            if count == 2 {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
        }
    })
    .await
    .expect("native Worker probe must publish within its deadline");
    shutdown.send(true).unwrap();
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    assert_eq!(calls.load(std::sync::atomic::Ordering::SeqCst), 1);
    let readiness = store.downstream_readiness(operator, down.id).await.unwrap();
    assert_eq!(readiness.state, DownstreamReadinessState::Available);
    let latest = readiness.latest_observation.unwrap();
    assert_eq!(
        latest.valid_until - latest.started_at,
        chrono::Duration::seconds(60)
    );
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT created_by FROM app.artifacts WHERE id=$1")
            .bind(latest.snapshot_artifact_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap(),
        "RUNTIME"
    );
    assert!(store.prepare_downstream_refresh().await.unwrap().is_none());
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
                scope_codes: vec![
                    MachineScope::DownstreamClaim,
                    MachineScope::DownstreamAck,
                    MachineScope::ForwardSubmit,
                ],
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
    let foreign = store
        .create_project(
            operator,
            "approval-list-foreign",
            &ProjectCreate {
                schema_version: SchemaV1,
                name: "Other approval reader".into(),
                description: "Native project boundary".into(),
                fork_from_project_id: None,
            },
        )
        .await
        .unwrap()
        .resource;
    let mut readers = Vec::new();
    for (name, project, scope, status) in [
        (
            "own",
            release.project_id,
            MachineScope::ResearchRead,
            reqwest::StatusCode::OK,
        ),
        (
            "foreign",
            foreign.id,
            MachineScope::ResearchRead,
            reqwest::StatusCode::NOT_FOUND,
        ),
        (
            "scope",
            release.project_id,
            MachineScope::EvidenceRead,
            reqwest::StatusCode::FORBIDDEN,
        ),
    ] {
        let principal = store
            .create_principal(
                operator,
                &format!("approval-list-{name}"),
                &PrincipalCreate {
                    schema_version: SchemaV1,
                    name: format!("Approval reader {name}"),
                    kind: AssignablePrincipalKind::Cli,
                    project_id: Some(project),
                    downstream_id: None,
                    enabled: true,
                },
            )
            .await
            .unwrap()
            .resource;
        let store::control::CredentialPreparation::New(prepared) = store
            .prepare_credential_issuance(
                operator,
                &format!("approval-list-key-{name}"),
                principal.id,
                &CredentialIssue {
                    schema_version: SchemaV1,
                    scope_codes: vec![scope],
                    expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
                },
            )
            .await
            .unwrap()
        else {
            panic!("new read credential")
        };
        let public = Id::new();
        let secret = random_capability();
        let verifier = capability_verifier(&secret).unwrap();
        let reference = vault.put("MACHINE_VERIFIER", verifier.as_bytes()).unwrap();
        prepared.publish(public, reference).await.unwrap();
        readers.push((name, format_machine_token(public, &secret).unwrap(), status));
    }
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
    let http = reqwest::Client::new();
    let approvals_url = format!("{origin}/api/v2/releases/{}/approvals", release.id);
    assert_eq!(
        http.get(&approvals_url).send().await.unwrap().status(),
        reqwest::StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        http.get(&approvals_url)
            .bearer_auth(&token)
            .send()
            .await
            .unwrap()
            .status(),
        reqwest::StatusCode::FORBIDDEN
    );
    for (name, reader, expected) in readers {
        let response = http
            .get(&approvals_url)
            .bearer_auth(&reader)
            .send()
            .await
            .unwrap();
        assert_eq!(response.status(), expected, "approval reader {name}");
        if expected != reqwest::StatusCode::OK {
            continue;
        }
        let page: Page<ApprovalViewV1> = response.json().await.unwrap();
        assert!(
            page.items
                .iter()
                .any(|item| item.id == approval.id
                    && item.evidence_set_id == approval.evidence_set_id)
        );
        assert!(page
            .items
            .iter()
            .all(|item| item.release_id == release.id && item.project_id == release.project_id));
        assert_eq!(
            http.get(format!("{approvals_url}?limit=0"))
                .bearer_auth(&reader)
                .send()
                .await
                .unwrap()
                .status(),
            reqwest::StatusCode::UNPROCESSABLE_ENTITY
        );
        let path = directory.path().join("approval-read-credential");
        fs::write(&path, &reader).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let output = client::invoke(
            &origin,
            &path,
            &[
                "approval",
                "list",
                &release.id.to_string(),
                "--limit",
                "100",
            ],
            serde_json::Value::Null,
        )
        .await;
        assert!(output.status.success(), "native approval list failed");
        let page: Page<ApprovalViewV1> = serde_json::from_slice(&output.stdout).unwrap();
        assert!(page.items.iter().any(|item| item.id == approval.id));
    }
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
    let listed = client::invoke(
        &origin,
        &credential,
        &[
            "handoff",
            "list",
            &offer.project_id.to_string(),
            "--limit",
            "1",
        ],
        serde_json::Value::Null,
    )
    .await;
    assert!(listed.status.success());
    let page: contracts::control::Page<HandoffViewV1> =
        serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(page.items.len(), 1);
    assert_eq!(page.items[0].id, offer.id);
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
    Box::pin(forward_messages::check(
        pool,
        store,
        operator,
        f,
        (&origin, &credential),
        offer.id,
    ))
    .await;
    listener.abort_all();
    probe_server.abort();
    Box::pin(graph_recovery::check(
        pool,
        &directory.path().join("objects"),
        operator,
    ))
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_frozen_policy_automates_paper_without_live_promotion(pool: PgPool) {
    let (store, actor, f, build, candidate, directory) =
        Box::pin(qualified_portfolio::qualified_chain_policy(
            pool.clone(),
            cycle_support::Liquidity::None,
            contracts::forward::ForwardEnvironmentV1::Live,
            contracts::research::DataUse::ResearchAndPaper,
            qualified_portfolio::release_policy,
        ))
        .await
        .unwrap();
    let (_, release, _) = Box::pin(qualified_portfolio::original_releases(
        &pool, &store, &actor, &f, &build, candidate,
    ))
    .await
    .unwrap();
    Box::pin(automatic_paper::check(
        &pool, &store, &actor, &f, &directory, &release,
    ))
    .await;
    Box::pin(automatic_paper::quota(
        &pool, &store, &actor, &f, &build, &release,
    ))
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_healthy_paper_promotes_live_with_frozen_complete_evidence(pool: PgPool) {
    Box::pin(healthy_paper_live_scenario(pool)).await;
}

async fn healthy_paper_live_scenario(pool: PgPool) {
    let (store, actor, f, build, candidate, directory) =
        Box::pin(qualified_portfolio::qualified_chain_policy(
            pool.clone(),
            cycle_support::Liquidity::None,
            contracts::forward::ForwardEnvironmentV1::Live,
            contracts::research::DataUse::ResearchPaperLive,
            qualified_portfolio::release_policy,
        ))
        .await
        .unwrap();
    let (_, release, _) = Box::pin(qualified_portfolio::original_releases(
        &pool, &store, &actor, &f, &build, candidate,
    ))
    .await
    .unwrap();
    Box::pin(automatic_live::check(
        &pool, &store, &actor, &f, &release, &directory,
    ))
    .await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn frozen_policy_worker_rebalance_advances_original_study_release_and_paper(pool: PgPool) {
    Box::pin(automatic_rebalance::scenario(pool)).await;
}
