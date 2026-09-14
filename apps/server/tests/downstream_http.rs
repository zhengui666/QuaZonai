//! Actual CLI -> HTTP/grant/PG -> pinned TCP -> ArtifactStore. Remote body is a fixture.
#[path = "support/client.rs"]
mod client;
mod support;
use axum::{
    http::{header, HeaderMap, StatusCode},
    routing::get,
    Json, Router,
};
use client::{browser, invoke};
use contracts::{settings::DownstreamCreate, Id};
use integrations::{artifacts::ArtifactStore, secrets::SecretVault};
use serde_json::{json, Value};
use server::runtime_transport::{RuntimeTarget, RuntimeTargets};
use sqlx::PgPool;
use std::{
    fs,
    os::unix::fs::PermissionsExt,
    sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    },
};

#[sqlx::test(migrations = "../../migrations")]
async fn runtime_allowlist_never_authorizes_downstream_network_and_anonymous_cannot_probe(
    pool: PgPool,
) {
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: "https://downstream.example".into(),
            addresses: vec!["127.0.0.1:443".parse().unwrap()],
        }],
        false,
    )
    .unwrap();
    let f = support::fixture_with_runtime_targets(pool.clone(), Some(targets)).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (confirmed, _) = support::confirm(&f, &enrollment, &initial, &totp, false).await;
    assert_eq!(confirmed.status, StatusCode::OK);
    let cookie = confirmed.cookie.unwrap_or(initial);
    let secret=browser(&f,&cookie,"secret","/api/v2/settings/credentials",json!({"intent":{"schema_version":1,"purpose":"DOWNSTREAM","label":"Unavailable native fixture"},"value":"disposable-downstream-credential"})).await;
    assert_eq!(secret.status, StatusCode::CREATED);
    let downstream=browser(&f,&cookie,"config","/api/v2/integrations/downstreams",json!({"schema_version":1,"credential_ref":secret.body["resource"]["id"],"configuration":{"name":"Denied downstream","endpoint":"https://downstream.example","accepted_package_versions":["1"],"environments":"BOTH","enabled":true,"development_http":false}})).await;
    assert_eq!(downstream.status, StatusCode::CREATED);
    let path = format!(
        "/api/v2/integrations/downstreams/{}/probe",
        downstream.body["resource"]["id"].as_str().unwrap()
    );
    let intent =
        json!({"schema_version":1,"expected_revision":downstream.body["resource"]["revision"]});
    let denied = support::call(&f, "POST", &path, intent.clone(), None).await;
    assert_eq!(denied.status, StatusCode::UNAUTHORIZED);
    let result = browser(&f, &cookie, "probe", &path, intent).await;
    assert_eq!(result.status, StatusCode::OK);
    assert_eq!(
        result.body["resource"]["outcome"],
        json!({"status":"UNAVAILABLE","reason":"ENDPOINT_DENIED"})
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.downstream_probe_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_cli_probe_uses_exact_grant_and_rolls_back_failed_artifact_publication(
    pool: PgPool,
) {
    let socket = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = socket.local_addr().unwrap();
    let endpoint = format!("http://{address}");
    let count = Arc::new(AtomicUsize::new(0));
    let requests = count.clone();
    const SECRET: &str = "downstream-native-disposable-credential";
    let app=Router::new().route("/downstream/v1/capabilities",get(move |headers:HeaderMap| {
        count.fetch_add(1,Ordering::SeqCst);
        assert!(headers[header::AUTHORIZATION]==format!("Bearer {SECRET}"),"fixture bearer mismatch");
        async {Json(json!({"schema_version":1,"delivery_mode":"TARGET_ONLY","accepted_package_versions":["1"],"environments":["PAPER","LIVE"],"market_capability_versions":["fixture/1"],"accepting_targets":true,"checked_at":chrono::Utc::now()}))}
    }));
    let remote = tokio::spawn(async move { axum::serve(socket, app).await.unwrap() });
    let f = support::fixture(pool.clone()).await;
    let (enrollment, initial, totp) = support::start(&f).await;
    let (confirmed, _) = support::confirm(&f, &enrollment, &initial, &totp, false).await;
    assert_eq!(confirmed.status, StatusCode::OK);
    let cookie = confirmed.cookie.unwrap_or(initial);
    let secret=browser(&f,&cookie,"secret","/api/v2/settings/credentials",json!({"intent":{"schema_version":1,"purpose":"DOWNSTREAM","label":"Disposable downstream"},"value":SECRET})).await;
    assert_eq!(secret.status, StatusCode::CREATED);
    // Read the identity created by real native TOTP; do not fabricate a login row.
    let login:uuid::Uuid=sqlx::query_scalar("SELECT id FROM app.browser_logins WHERE revoked_at IS NULL ORDER BY authenticated_at DESC LIMIT 1").fetch_one(&pool).await.unwrap();
    let actor = store::authority::Actor::Browser {
        login_id: login.to_string().try_into().unwrap(),
    };
    let request:DownstreamCreate=serde_json::from_value(json!({"schema_version":1,"credential_ref":secret.body["resource"]["id"],"configuration":{"name":"Native fixture","endpoint":endpoint,"accepted_package_versions":["1"],"environments":"PAPER","enabled":true,"development_http":true}})).unwrap();
    let vault = SecretVault::open(
        &f._state.path().join("secrets"),
        &f._state.path().join("master.key"),
    )
    .unwrap();
    let downstream = f
        .store
        .create_downstream(&actor, "native-config", &request, |bindings| async move {
            for binding in bindings {
                vault
                    .read(binding.id, "DOWNSTREAM")
                    .map_err(|_| store::StoreError::Integrity)?;
            }
            Ok(())
        })
        .await
        .unwrap()
        .resource;
    let principal=browser(&f,&cookie,"cli","/api/v2/machine-principals",json!({"schema_version":1,"name":"Downstream doctor","kind":"CLI","project_id":null,"downstream_id":null,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential=browser(&f,&cookie,"token",&format!("/api/v2/machine-principals/{}/credentials",principal.body["resource"]["id"].as_str().unwrap()),json!({"schema_version":1,"scope_codes":["DOCTOR_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let file = f._state.path().join("doctor-token");
    fs::write(&file, token).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let targets = RuntimeTargets::new(
        vec![RuntimeTarget {
            origin: endpoint,
            addresses: vec![address],
        }],
        true,
    )
    .unwrap();
    let (origin, _listener) = client::listen_with_downstream_targets(&f, targets).await;
    let id = downstream.id.to_string();
    let intent = json!({"schema_version":1,"expected_revision":downstream.revision});
    let read = invoke(
        &origin,
        &file,
        &["downstream", "readiness", &id],
        Value::Null,
    )
    .await;
    assert!(read.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&read.stdout).unwrap()["state"],
        "NOT_CHECKED"
    );
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    let denied = invoke(
        &origin,
        &file,
        &["--idempotency-key", "probe", "downstream", "probe", &id],
        intent.clone(),
    )
    .await;
    assert!(!denied.status.success());
    assert_eq!(requests.load(Ordering::SeqCst), 0);
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let human=invoke(&origin,&file,&["--idempotency-key","human","operator-grant"],json!({"schema_version":1,"command":{"operation":"DOWNSTREAM_PROBE","request":intent},"target_id":downstream.id,"code":totp.generate((now/30+1)*30)})).await;
    assert!(human.status.success(), "native grant failed");
    let grant: Value = serde_json::from_slice(&human.stdout).unwrap();
    let args = [
        "--idempotency-key",
        "probe",
        "--operator-grant",
        grant["resource"]["id"].as_str().unwrap(),
        "downstream",
        "probe",
        &id,
    ];
    sqlx::raw_sql("CREATE FUNCTION app.reject_downstream_probe_fixture() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'fixture_publication_failure'; END $$; CREATE TRIGGER reject_downstream_probe_fixture BEFORE INSERT ON app.downstream_probe_observations FOR EACH ROW EXECUTE FUNCTION app.reject_downstream_probe_fixture();").execute(&pool).await.unwrap();
    let failed = invoke(&origin, &file, &args, intent.clone()).await;
    assert!(!failed.status.success());
    assert!(failed.stdout.is_empty());
    assert!(!String::from_utf8_lossy(&failed.stderr).contains(SECRET));
    assert_eq!(
        fs::read_dir(f._state.path().join("artifacts"))
            .unwrap()
            .count(),
        0
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.downstream_probe_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::raw_sql("DROP TRIGGER reject_downstream_probe_fixture ON app.downstream_probe_observations; DROP FUNCTION app.reject_downstream_probe_fixture();").execute(&pool).await.unwrap();
    let accepted = invoke(&origin, &file, &args, intent.clone()).await;
    assert!(accepted.status.success(), "native probe failed");
    assert!(!String::from_utf8_lossy(&accepted.stdout).contains(SECRET));
    let accepted: Value = serde_json::from_slice(&accepted.stdout).unwrap();
    assert_eq!(accepted["resource"]["outcome"]["status"], "AVAILABLE");
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    let replay = invoke(&origin, &file, &args, intent.clone()).await;
    assert!(replay.status.success());
    let replay: Value = serde_json::from_slice(&replay.stdout).unwrap();
    assert_eq!(replay["resource"], accepted["resource"]);
    assert_eq!(replay["replayed"], true);
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    let changed = invoke(
        &origin,
        &file,
        &args,
        json!({"schema_version":1,"expected_revision":downstream.revision.get()+1}),
    )
    .await;
    assert!(!changed.status.success());
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    let read = invoke(
        &origin,
        &file,
        &["downstream", "readiness", &id],
        Value::Null,
    )
    .await;
    assert!(read.status.success());
    let read: Value = serde_json::from_slice(&read.stdout).unwrap();
    assert_eq!(read["state"], "AVAILABLE");
    assert_eq!(read["available_environments"], json!(["PAPER"]));
    let artifact: Id = accepted["resource"]["snapshot_artifact_id"]
        .as_str()
        .unwrap()
        .to_owned()
        .try_into()
        .unwrap();
    let size: i64 = sqlx::query_scalar("SELECT byte_count FROM app.artifacts WHERE id=$1")
        .bind(artifact.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    let objects = ArtifactStore::open(&f._state.path().join("artifacts")).unwrap();
    let bytes = objects
        .read(artifact, contracts::DbCounter::new(size as u64).unwrap())
        .unwrap();
    let raw: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(raw["result"], accepted["resource"]["outcome"]);
    // A listener with no outbound allowance still serves the stored observation;
    // this read must not silently attempt a refresh or consume another grant.
    let (read_origin, _read_listener) = client::listen(&f).await;
    let read = invoke(
        &read_origin,
        &file,
        &["downstream", "readiness", &id],
        Value::Null,
    )
    .await;
    assert!(read.status.success());
    assert_eq!(
        serde_json::from_slice::<Value>(&read.stdout).unwrap()["latest_observation"]["id"],
        accepted["resource"]["id"]
    );
    assert_eq!(requests.load(Ordering::SeqCst), 2);
    remote.abort();
}
