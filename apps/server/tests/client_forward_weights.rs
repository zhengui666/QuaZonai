//! Actual TCP/native CLI, authentication, PostgreSQL and object publication; PAPER only.
#[path = "support/client.rs"]
mod client;
mod support;
use axum::http::StatusCode;
use client::{browser, invoke, listen};
use contracts::{settings::*, Id, SchemaV1};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt};

#[sqlx::test(migrations = "../../migrations")]
async fn downstream_cli_publishes_original_weights_and_replays_without_replacing_the_object(
    pool: PgPool,
) {
    let f = support::fixture(pool.clone()).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (login, _) = support::confirm(&f, &enrollment, &initial, &totp, false).await;
    assert_eq!(login.status, StatusCode::OK);
    let cookie = login.cookie.unwrap_or(initial);
    let login_id: uuid::Uuid =
        sqlx::query_scalar("SELECT id FROM app.browser_logins ORDER BY created_at DESC LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let actor = store::authority::Actor::Browser {
        login_id: login_id.to_string().try_into().unwrap(),
    };
    let project = browser(&f, &cookie, "forward-project", "/api/v2/projects", json!({"schema_version":1,"name":"Paper weights","description":"Controlled integration fixture","fork_from_project_id":null})).await;
    assert_eq!(project.status, StatusCode::CREATED);
    let project = project.body["resource"]["id"].clone();
    // Registration is controlled here; no external downstream endpoint is contacted.
    let downstream = f
        .store
        .create_downstream(
            &actor,
            "forward-integration",
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: "Paper fixture".into(),
                    endpoint: "https://downstream.example".into(),
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
        .resource
        .id;
    let principal = browser(&f, &cookie, "forward-principal", "/api/v2/machine-principals", json!({"schema_version":1,"name":"Paper source","kind":"DOWNSTREAM","project_id":project,"downstream_id":downstream,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = browser(&f, &cookie, "forward-token", &format!("/api/v2/machine-principals/{}/credentials", principal.body["resource"]["id"].as_str().unwrap()), json!({"schema_version":1,"scope_codes":["FORWARD_SUBMIT"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let file = f._state.path().join("forward-token");
    fs::write(&file, token).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp_nanos_opt()
        .unwrap();
    let body = json!({"schema_version":1,"project_id":project,"environment":"PAPER","external_message_id":"paper-original","asof_ns":(now-1000000).to_string(),"available_ns":now.to_string(),"valid_until_ns":(now+60000000000_i64).to_string(),"base_currency":"USD","cash_weight":"0.25","weights":[{"instrument_id":"A.SIM","currency":"USD","weight":"0.75"}]});
    let (origin, _listener) = listen(&f).await;
    for command in ["observations", "wakes"] {
        let denied = invoke(
            &origin,
            &file,
            &["forward", command, project.as_str().unwrap()],
            Value::Null,
        )
        .await;
        assert!(!denied.status.success());
        assert!(denied.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&denied.stderr).contains(token));
    }
    let mut original = Value::Null;
    for replay in [false, true] {
        let output = invoke(
            &origin,
            &file,
            &["--idempotency-key", "transport-key", "forward-weights"],
            body.clone(),
        )
        .await;
        assert!(output.status.success(), "downstream submission failed");
        assert!(output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
        let receipt: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(receipt["replayed"], replay);
        if replay {
            assert_eq!(receipt["resource"], original);
        } else {
            original = receipt["resource"].clone();
        }
    }
    assert_eq!(
        original["content"]["source"],
        json!({"kind":"FORWARD_SNAPSHOT","downstream_id":downstream,"external_message_id":"paper-original"})
    );
    let artifact: Id = original["report_artifact_id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    let (size, provenance): (i64, String) =
        sqlx::query_as("SELECT byte_count,origin FROM app.artifacts WHERE id=$1")
            .bind(artifact.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(provenance, "SYNTHETIC");
    let objects =
        integrations::artifacts::ArtifactStore::open(&f._state.path().join("artifacts")).unwrap();
    let bytes = objects
        .read(artifact, contracts::DbCounter::new(size as u64).unwrap())
        .unwrap();
    assert_eq!(
        serde_json::from_slice::<Value>(&bytes).unwrap(),
        original["content"]
    );
    let mut changed = body;
    changed["cash_weight"] = "0.5".into();
    changed["weights"][0]["weight"] = "0.5".into();
    let rejected = invoke(
        &origin,
        &file,
        &[
            "--idempotency-key",
            "different-transport-key",
            "forward-weights",
        ],
        changed,
    )
    .await;
    assert!(!rejected.status.success());
    assert!(rejected.stdout.is_empty());
    assert_eq!(
        serde_json::from_slice::<Value>(&rejected.stderr).unwrap()["status"],
        409
    );
    assert_eq!(
        objects
            .read(artifact, contracts::DbCounter::new(size as u64).unwrap())
            .unwrap(),
        bytes
    );
}
