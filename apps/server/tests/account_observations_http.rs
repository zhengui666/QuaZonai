//! Account envelopes -> real Axum/TCP, credentials and PostgreSQL/PGMQ.
//! The explicitly invoked native pipeline test also runs Portfolio::build_snapshot,
//! the converter and portable submit/read commands. The explicitly selected capital-exit
//! bridge adds an official controlled Sandbox owner; no test connects a Live account.
#[path = "support/capital_exit_native_pipeline.rs"]
mod capital_exit_native_pipeline;
#[path = "support/client.rs"]
#[allow(dead_code)]
mod client;
#[path = "support/native_account_pipeline.rs"]
mod native_account_pipeline;
#[path = "support/portable_client.rs"]
mod portable_client;
mod support;

use axum::http::StatusCode;
use contracts::{Id, SchemaV1, settings::*};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt};
use store::authority::Actor;

const SUBMIT: &str = "/api/v2/forward/account-observations";
const SUBMIT_CLIENT: &str = "/api/v2/forward/client-account-observations";

struct AccountFixture {
    http: reqwest::Client,
    origin: String,
    _listener: client::Listener,
    f: support::Fixture,
    cookie: String,
    browser_cookie: String,
    actor: Actor,
    project: Id,
    downstream: Id,
    token: String,
}

async fn project(f: &support::Fixture, cookie: &str, key: &str) -> Id {
    let reply = client::browser(
        f,
        cookie,
        key,
        "/api/v2/projects",
        json!({"schema_version":1,"name":key,"description":"Controlled native account transport fixture","fork_from_project_id":null}),
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED, "{}", reply.body);
    serde_json::from_value(reply.body["resource"]["id"].clone()).unwrap()
}

async fn downstream(
    f: &support::Fixture,
    actor: &Actor,
    cookie: &str,
    project: Id,
    key: &str,
) -> (Id, String) {
    // Match the controlled downstream registration used by client_forward_weights.
    // No remote endpoint is contacted; bearer credentials below are issued by the API.
    let downstream = f
        .store
        .create_downstream(
            actor,
            &format!("{key}-integration"),
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: key.into(),
                    endpoint: "https://downstream.example".into(),
                    accepted_package_versions: vec![PackageSchemaVersion::V2],
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
    let principal = client::browser(
        f,
        cookie,
        &format!("{key}-principal"),
        "/api/v2/machine-principals",
        json!({"schema_version":1,"name":key,"kind":"DOWNSTREAM","project_id":project,"downstream_id":downstream,"enabled":true}),
    )
    .await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = client::browser(
        f,
        cookie,
        &format!("{key}-credential"),
        &format!(
            "/api/v2/machine-principals/{}/credentials",
            principal.body["resource"]["id"].as_str().unwrap()
        ),
        json!({"schema_version":1,"scope_codes":["FORWARD_SUBMIT"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)}),
    )
    .await;
    assert_eq!(credential.status, StatusCode::CREATED);
    (
        downstream,
        credential.body["token"].as_str().unwrap().into(),
    )
}

async fn fixture(pool: &PgPool) -> AccountFixture {
    fixture_with_paper_exit(pool, false).await
}
async fn fixture_with_paper_exit(pool: &PgPool, capital_exit: bool) -> AccountFixture {
    let f = support::fixture(pool.clone()).await;
    let cookie = support::local_session(&f).await.cookie.unwrap();
    let login: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.browser_logins")
        .fetch_one(pool)
        .await
        .unwrap();
    let actor = Actor::Browser {
        login_id: login.to_string().try_into().unwrap(),
    };
    let project = project(&f, &cookie, "native-account-project").await;
    let (downstream, token) = downstream(&f, &actor, &cookie, project, "paper-observer").await;
    let owners = if capital_exit {
        server::paper_capital_exit::PaperCapitalExitOwners::new(vec![
            server::paper_capital_exit::PaperCapitalExitOwnerConfiguration {
                schema_version: SchemaV1,
                project_id: project,
                downstream_id: downstream,
                native_trader_id: "QZEXIT-001".into(),
                native_account_id: "QZEXIT-001".into(),
                native_client_id: "QZ-EXIT-SANDBOX".into(),
                native_version: "0.63.0".into(),
                venue: "QZEXIT".into(),
                collateral_currency: "USDC".into(),
                instrument_id: "YES.QZEXIT".into(),
                controlled_strategy_ids: vec!["EXIT-FIXTURE-001".into()],
            },
        ])
        .unwrap()
    } else {
        server::paper_capital_exit::PaperCapitalExitOwners::default()
    };
    let (origin, listener) = client::listen_with_paper_owners(&f, owners).await;
    let http = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(20))
        .build()
        .unwrap();
    // The TCP listener has its own cookie key. Obtain its genuine browser session
    // through password login instead of copying the in-process router's cookie.
    let browser = http
        .post(format!("{origin}/api/v2/auth/login"))
        .header("origin", &origin)
        .json(
            &json!({"schema_version":1,"password":"native-test-password","remember_device":false}),
        )
        .send()
        .await
        .unwrap();
    assert_eq!(browser.status(), StatusCode::OK);
    let browser_cookie = browser.headers()["set-cookie"]
        .to_str()
        .unwrap()
        .split(';')
        .next()
        .unwrap()
        .to_owned();
    AccountFixture {
        http,
        origin,
        _listener: listener,
        f,
        cookie,
        browser_cookie,
        actor,
        project,
        downstream,
        token,
    }
}

async fn owner_cli_session(a: &AccountFixture) -> String {
    let response = a.http
        .post(format!("{}/api/v2/auth/cli/login", a.origin))
        .header("origin", &a.origin)
        .header("x-quazonai-cli", "1")
        .json(&json!({"schema_version":1,"password":"native-test-password","name":"Native account readback"}))
        .send().await.unwrap();
    assert_eq!(response.status(), StatusCode::CREATED);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let response: Value = response.json().await.unwrap();
    response["token"].as_str().unwrap().to_owned()
}

fn observation(project: Id) -> Value {
    let mut value: Value = serde_json::from_str(include_str!(
        "../../../tests/contracts/native-account-paper-snapshot.json"
    ))
    .unwrap();
    value["binding"]["project_id"] = json!(project);
    value
}

async fn post(a: &AccountFixture, token: &str, body: &Value) -> (StatusCode, Value) {
    let response = a
        .http
        .post(format!("{}{SUBMIT}", a.origin))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    let body = response.json().await.unwrap();
    (status, body)
}

async fn post_client(a: &AccountFixture, token: &str, body: &Value) -> (StatusCode, Value) {
    let response = a
        .http
        .post(format!("{}{SUBMIT_CLIENT}", a.origin))
        .bearer_auth(token)
        .json(body)
        .send()
        .await
        .unwrap();
    let status = response.status();
    (status, response.json().await.unwrap())
}

async fn submit(a: &AccountFixture, body: &Value) -> Value {
    let (status, result) = post(a, &a.token, body).await;
    assert_eq!(status, StatusCode::CREATED, "{result}");
    assert_eq!(result["replayed"], false);
    assert_eq!(result["resource"]["observation"], *body);
    result["resource"].clone()
}

async fn browser_get(a: &AccountFixture, path: &str) -> Value {
    let reply = a
        .http
        .get(format!("{}{path}", a.origin))
        .header("cookie", &a.browser_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(reply.status(), StatusCode::OK);
    reply.json().await.unwrap()
}

fn source_path(a: &AccountFixture, resource: &Value) -> String {
    format!(
        "/api/v2/projects/{}/account-sources/{}",
        a.project,
        resource["source_id"].as_str().unwrap()
    )
}

// Compare every persisted field, including cursor IDs/clocks, not only row counts.
async fn ledger(pool: &PgPool) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
            'sources', (SELECT coalesce(jsonb_agg(to_jsonb(s) ORDER BY id), '[]'::jsonb) FROM app.native_account_sources s),
            'observations', (SELECT coalesce(jsonb_agg(to_jsonb(o) ORDER BY id), '[]'::jsonb) FROM app.native_account_observations o),
            'cursors', (SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY source_id), '[]'::jsonb) FROM app.native_account_cursors c))",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_snapshot_replay_and_owner_reads_preserve_the_exact_native_envelope(pool: PgPool) {
    let a = fixture(&pool).await;
    let original = observation(a.project);
    let snapshot = submit(&a, &original).await;
    assert_eq!(snapshot["downstream_id"], json!(a.downstream));
    assert_eq!(snapshot["gap_before"], false);
    let before = ledger(&pool).await;
    assert_eq!(before["observations"].as_array().unwrap().len(), 1);
    let (status, replay) = post(&a, &a.token, &original).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["resource"], snapshot);
    assert_eq!(ledger(&pool).await, before);

    let mut heartbeat = original.clone();
    heartbeat["sequence"] = json!("3");
    heartbeat["dropped_events"] = json!("1");
    heartbeat["connection"] = json!("DISCONNECTED");
    heartbeat["snapshot"] = Value::Null;
    let heartbeat = submit(&a, &heartbeat).await;
    assert_eq!(heartbeat["gap_before"], true);
    let before = ledger(&pool).await;
    let (status, replay) = post(&a, &a.token, &original).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(replay["resource"], snapshot);
    assert_eq!(replay["replayed"], true);
    assert_eq!(
        ledger(&pool).await,
        before,
        "old retries cannot roll back the cursor"
    );
    let mut conflict = original.clone();
    conflict["snapshot"]["total_equity"][0]["amount"] = json!("999999");
    let (status, _) = post(&a, &a.token, &conflict).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(ledger(&pool).await, before);

    let base = source_path(&a, &snapshot);
    let current = browser_get(&a, &format!("{base}/current")).await;
    assert_eq!(current["latest_snapshot"], snapshot);
    assert_eq!(current["valuation"], "PRICED");
    assert_eq!(current["source"]["last_sequence"], "3");
    assert_eq!(current["source"]["last_observation_id"], heartbeat["id"]);
    assert_eq!(current["source"]["latest_snapshot_id"], snapshot["id"]);
    assert_eq!(current["source"]["has_gap"], true);
    assert_eq!(current["source"]["connection"], "DISCONNECTED");
    let sources = browser_get(
        &a,
        &format!("/api/v2/projects/{}/account-sources", a.project),
    )
    .await;
    assert_eq!(sources["items"].as_array().unwrap().len(), 1);
    assert_eq!(sources["items"][0]["binding"], original["binding"]);
    assert_eq!(sources["items"][0]["downstream_id"], json!(a.downstream));
    let history = browser_get(&a, &format!("{base}/observations?limit=1")).await;
    assert_eq!(history["items"], json!([heartbeat]));
    assert_eq!(history["next_cursor"], heartbeat["id"]);
    let tail = browser_get(
        &a,
        &format!(
            "{base}/observations?limit=1&cursor={}",
            heartbeat["id"].as_str().unwrap()
        ),
    )
    .await;
    assert_eq!(tail["items"], json!([snapshot]));
    assert!(tail["next_cursor"].is_null());

    // Genuine public password login -> isolated saved profile -> portable CLI.
    // Interactive terminal login is tested separately in client_login.
    let directory = tempfile::tempdir().unwrap();
    let config = directory.path().join("quazonai");
    fs::create_dir(&config).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).unwrap();
    let owner_token = owner_cli_session(&a).await;
    let token = owner_token.as_str();
    let profile = config.join("client.json");
    let profile_bytes = serde_json::to_vec(&json!({
        "schema_version":1,"origin":a.origin,"token":token,"development_http":true,"ca_certificate":null
    })).unwrap();
    fs::write(&profile, profile_bytes).unwrap();
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o600)).unwrap();
    let project = a.project.to_string();
    let source = snapshot["source_id"].as_str().unwrap();
    for (command, arguments) in [
        (
            "current",
            vec!["forward", "accounts", "current", &project, source],
        ),
        ("sources", vec!["forward", "accounts", "sources", &project]),
        (
            "history",
            vec!["forward", "accounts", "history", &project, source],
        ),
    ] {
        let output = portable_client::saved(directory.path(), &arguments, Value::Null).await;
        assert!(output.status.success(), "portable account {command} failed");
        assert!(output.stderr.is_empty());
        assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        match command {
            "current" => {
                assert_eq!(value["latest_snapshot"], snapshot);
                assert_eq!(value["source"]["last_observation_id"], heartbeat["id"]);
                assert_eq!(value["valuation"], current["valuation"]);
            }
            "sources" => {
                assert_eq!(value["items"].as_array().unwrap().len(), 1);
                assert_eq!(value["items"][0]["binding"], original["binding"]);
                assert_eq!(value["items"][0]["latest_snapshot_id"], snapshot["id"]);
            }
            "history" => assert_eq!(value["items"], json!([heartbeat, snapshot])),
            _ => unreachable!(),
        }
    }
    assert_eq!(ledger(&pool).await, before, "all owner reads are read-only");
}

#[sqlx::test(migrations = "../../migrations")]
async fn missing_zero_stale_unpriced_and_currency_evidence_survive_http_and_heartbeats(
    pool: PgPool,
) {
    let a = fixture(&pool).await;
    let original = observation(a.project);
    let mut frame = original.clone();
    frame["snapshot"] = Value::Null;
    let first = submit(&a, &frame).await;
    let base = source_path(&a, &first);
    let current = browser_get(&a, &format!("{base}/current")).await;
    assert_eq!(current["valuation"], "UNAVAILABLE");
    assert!(current["latest_snapshot"].is_null());
    assert_eq!(current["source"]["connection"], "STALE");
    let mut receipts = vec![first];

    for (sequence, valuation) in [
        (2, "UNAVAILABLE"),
        (3, "PRICED"),
        (4, "STALE"),
        (6, "UNPRICED"),
    ] {
        let mut frame = original.clone();
        frame["sequence"] = json!(sequence.to_string());
        frame["snapshot"]["event_id"] =
            json!(format!("60a29b32-9e54-4f36-9304-0f0a8b16ce9{sequence}"));
        match sequence {
            2 => {
                frame["snapshot"]["total_equity"] = json!([]);
                frame["snapshot"]["realized_pnls"] = json!([]);
                frame["snapshot"]["unrealized_pnls"] = json!([]);
            }
            3 => frame["snapshot"]["total_equity"] = json!([{"amount":"0","currency":"USD"}]),
            4 | 6 => {
                frame["snapshot"]["is_stale"] = json!(true);
                frame["snapshot"]["stale_instruments"] = json!(["BTCUSDT.SIM"]);
                frame["snapshot"]["stale_currencies"] = json!(["USDT"]);
                if sequence == 6 {
                    frame["dropped_events"] = json!("1");
                    frame["snapshot"]["unpriced_instruments"] = json!(["BTCUSDT.SIM"]);
                }
            }
            _ => unreachable!(),
        }
        let receipt = submit(&a, &frame).await;
        assert_eq!(receipt["gap_before"], sequence == 6);
        let current = browser_get(&a, &format!("{base}/current")).await;
        assert_eq!(current["valuation"], valuation);
        assert_eq!(current["latest_snapshot"], receipt);
        assert!(
            current["latest_snapshot"]["observation"]["snapshot"]["base_currency_equity"].is_null()
        );
        receipts.push(receipt);
    }
    let unpriced = receipts.last().unwrap().clone();
    let native = &unpriced["observation"]["snapshot"];
    assert_eq!(native["total_equity"], original["snapshot"]["total_equity"]);
    assert_eq!(
        native["total_equity"][1],
        json!({"amount":"12.3456789","currency":"USDT"})
    );
    assert_eq!(
        native["realized_pnls"][0],
        json!({"amount":"-2.51","currency":"USD"})
    );
    assert_eq!(native["balances"][0]["locked"]["amount"], "0");

    let now =
        a.f.store
            .authentication_snapshot()
            .await
            .unwrap()
            .database_now
            .timestamp_nanos_opt()
            .unwrap();
    let mut heartbeat = original.clone();
    heartbeat["sequence"] = json!("7");
    heartbeat["dropped_events"] = json!("1");
    heartbeat["observed_at_ns"] = json!(now.to_string());
    heartbeat["snapshot"] = Value::Null;
    let heartbeat = submit(&a, &heartbeat).await;
    assert_eq!(heartbeat["gap_before"], false);
    let current = browser_get(&a, &format!("{base}/current")).await;
    assert_eq!(current["source"]["connection"], "CONNECTED");
    assert_eq!(current["source"]["has_gap"], true);
    assert_eq!(current["source"]["dropped_events"], "1");
    assert_eq!(current["source"]["last_observation_id"], heartbeat["id"]);
    assert_eq!(
        current["latest_snapshot"], unpriced,
        "fresh transport cannot refresh native valuation"
    );
    assert_eq!(current["valuation"], "UNPRICED");
    receipts.push(heartbeat);
    receipts.reverse();
    let history = browser_get(&a, &format!("{base}/observations")).await;
    assert_eq!(history["items"], json!(receipts));
    assert!(history["next_cursor"].is_null());
}

#[sqlx::test(migrations = "../../migrations")]
async fn downstream_namespaces_and_project_environment_scopes_cannot_mutate_another_source(
    pool: PgPool,
) {
    let a = fixture(&pool).await;
    let original = observation(a.project);
    let first = submit(&a, &original).await;
    let (other_downstream, other_token) =
        downstream(&a.f, &a.actor, &a.cookie, a.project, "other-observer").await;
    // Downstream identity comes from the verified credential, never the body.
    // The same native binding is a separate source for another allowed downstream.
    let (status, second) = post(&a, &other_token, &original).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(second["replayed"], false);
    let second = &second["resource"];
    assert_ne!(second["source_id"], first["source_id"]);
    assert_eq!(second["downstream_id"], json!(other_downstream));
    assert_eq!(second["observation"], original);
    let other_project = project(&a.f, &a.cookie, "other-account-project").await;
    let before = ledger(&pool).await;
    assert_eq!(before["sources"].as_array().unwrap().len(), 2);
    assert_eq!(before["observations"].as_array().unwrap().len(), 2);
    assert_eq!(before["cursors"].as_array().unwrap().len(), 2);
    // Existing machine project scoping resolves a foreign project as NotFound;
    // the configured downstream's unsupported environment is Forbidden.
    for (field, value, expected_status, expected_code) in [
        (
            "project_id",
            json!(other_project),
            StatusCode::NOT_FOUND,
            "NOT_FOUND",
        ),
        (
            "environment",
            json!("LIVE"),
            StatusCode::FORBIDDEN,
            "FORBIDDEN",
        ),
    ] {
        let mut wrong = original.clone();
        wrong["binding"][field] = value;
        let (status, problem) = post(&a, &a.token, &wrong).await;
        assert_eq!(status, expected_status, "wrong {field}: {problem}");
        assert_eq!(problem["code"], expected_code);
        assert_eq!(ledger(&pool).await, before);
    }
    let base = source_path(&a, &first);
    let denied = a
        .http
        .get(format!("{}{base}/current", a.origin))
        .bearer_auth(&other_token)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    assert_eq!(denied.json::<Value>().await.unwrap()["code"], "FORBIDDEN");
    let wrong_project = a
        .http
        .get(format!(
            "{}/api/v2/projects/{other_project}/account-sources/{}/current",
            a.origin,
            first["source_id"].as_str().unwrap()
        ))
        .header("cookie", &a.browser_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_project.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        wrong_project.json::<Value>().await.unwrap()["code"],
        "NOT_FOUND"
    );
    let current = browser_get(&a, &format!("{base}/current")).await;
    assert_eq!(current["latest_snapshot"], first);
    assert_eq!(current["source"]["downstream_id"], json!(a.downstream));
    assert_eq!(ledger(&pool).await, before);
}

#[sqlx::test(migrations = "../../migrations")]
async fn client_bound_http_retains_original_values_rejects_protocol_changes_and_keeps_read_scope(
    pool: PgPool,
) {
    let a = fixture(&pool).await;
    let original = observation(a.project);
    let wrapped = json!({"schema_version":2,"native_client_id":"CONTROLLED-NATIVE-CLIENT","observation":original});
    let (status, first) = post_client(&a, &a.token, &wrapped).await;
    assert_eq!(status, StatusCode::CREATED, "{first}");
    assert_eq!(first["schema_version"], 2);
    assert_eq!(first["resource"]["observation"], original);
    let before = ledger(&pool).await;
    let (status, replay) = post_client(&a, &a.token, &wrapped).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["resource"], first["resource"]);
    assert_eq!(ledger(&pool).await, before);
    let base = source_path(&a, &first["resource"]);
    let metadata = browser_get(&a, &format!("{base}/client-binding")).await;
    assert_eq!(metadata["native_client_id"], wrapped["native_client_id"]);
    assert_eq!(metadata["source_id"], first["resource"]["source_id"]);
    let current = browser_get(&a, &format!("{base}/current")).await;
    assert_eq!(current["latest_snapshot"], first["resource"]);
    assert!(
        current["source"]["binding"]
            .get("native_client_id")
            .is_none(),
        "the old V1 read shape remains unchanged"
    );

    let mut wrong = wrapped.clone();
    wrong["native_client_id"] = json!("OTHER-CLIENT");
    assert_eq!(
        post_client(&a, &a.token, &wrong).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(post(&a, &a.token, &original).await.0, StatusCode::CONFLICT);
    assert_eq!(ledger(&pool).await, before);
    let denied = a
        .http
        .get(format!("{}{base}/client-binding", a.origin))
        .bearer_auth(&a.token)
        .send()
        .await
        .unwrap();
    assert_eq!(denied.status(), StatusCode::FORBIDDEN);
    wrong = wrapped.clone();
    wrong["observation"]["binding"]["environment"] = json!("LIVE");
    assert_eq!(
        post_client(&a, &a.token, &wrong).await.0,
        StatusCode::FORBIDDEN
    );
    let foreign = project(&a.f, &a.cookie, "foreign-client-observation").await;
    wrong = wrapped.clone();
    wrong["observation"]["binding"]["project_id"] = json!(foreign);
    assert_eq!(
        post_client(&a, &a.token, &wrong).await.0,
        StatusCode::NOT_FOUND
    );
    let wrong_project = a
        .http
        .get(format!(
            "{}/api/v2/projects/{foreign}/account-sources/{}/client-binding",
            a.origin,
            first["resource"]["source_id"].as_str().unwrap()
        ))
        .header("cookie", &a.browser_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(wrong_project.status(), StatusCode::NOT_FOUND);
}

#[sqlx::test(migrations = "../../migrations")]
async fn legacy_http_source_cannot_gain_client_evidence_from_a_new_wrapper(pool: PgPool) {
    let a = fixture(&pool).await;
    let original = observation(a.project);
    let legacy = submit(&a, &original).await;
    let before = ledger(&pool).await;
    let wrapped =
        json!({"schema_version":2,"native_client_id":"CLAIMED-CLIENT","observation":original});
    assert_eq!(
        post_client(&a, &a.token, &wrapped).await.0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        post_client(&a, &a.token, &original).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    assert_eq!(
        post(&a, &a.token, &wrapped).await.0,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let base = source_path(&a, &legacy);
    let absent = a
        .http
        .get(format!("{}{base}/client-binding", a.origin))
        .header("cookie", &a.browser_cookie)
        .send()
        .await
        .unwrap();
    assert_eq!(absent.status(), StatusCode::UNPROCESSABLE_ENTITY);
    assert_eq!(ledger(&pool).await, before);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires QUAZONAI_NATIVE_CLIENT_ACCOUNT_FIXTURE_BIN built with native-sandbox-test"]
async fn actual_native_client_observations_reach_http_sql_and_original_readback(pool: PgPool) {
    let a = fixture(&pool).await;
    let fixture_bin = std::env::var("QUAZONAI_NATIVE_CLIENT_ACCOUNT_FIXTURE_BIN")
        .expect("build the official no-order Sandbox fixture first");
    let directory = tempfile::tempdir().unwrap();
    let output = directory.path().join("native-client.ndjson");
    let mut command = tokio::process::Command::new(fixture_bin);
    command
        .env_clear()
        .current_dir(directory.path())
        .kill_on_drop(true)
        .arg(a.project.to_string())
        .arg(&output);
    let run = tokio::time::timeout(std::time::Duration::from_secs(20), command.output())
        .await
        .expect("bounded no-order Sandbox fixture must exit")
        .unwrap();
    assert!(
        run.status.success(),
        "{}",
        String::from_utf8_lossy(&run.stderr)
    );
    let rows: Vec<Value> = fs::read_to_string(&output)
        .unwrap()
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert!(!rows.is_empty());
    assert!(
        rows.iter()
            .any(|row| !row["observation"]["snapshot"].is_null())
    );
    let mut receipts = Vec::new();
    let mut last_snapshot = None;
    for original in &rows {
        let (status, receipt) = post_client(&a, &a.token, original).await;
        assert_eq!(status, StatusCode::CREATED, "{receipt}");
        assert_eq!(receipt["native_client_id"], original["native_client_id"]);
        assert_eq!(receipt["resource"]["observation"], original["observation"]);
        assert_eq!(
            receipt["resource"]["observation"]["binding"]["environment"],
            "PAPER"
        );
        if !original["observation"]["snapshot"].is_null() {
            last_snapshot = Some(receipt["resource"].clone());
        }
        receipts.push(receipt);
    }
    let base = source_path(&a, &receipts[0]["resource"]);
    let current = browser_get(&a, &format!("{base}/current")).await;
    assert_eq!(current["latest_snapshot"], last_snapshot.unwrap());
    let metadata = browser_get(&a, &format!("{base}/client-binding")).await;
    assert_eq!(metadata["native_client_id"], rows[0]["native_client_id"]);
    let before = ledger(&pool).await;
    for (original, receipt) in rows.iter().zip(&receipts) {
        let (status, repeated) = post_client(&a, &a.token, original).await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(repeated["replayed"], true);
        assert_eq!(repeated["resource"], receipt["resource"]);
    }
    assert_eq!(ledger(&pool).await, before);
}

#[sqlx::test(migrations = "../../migrations")]
#[ignore = "requires this candidate's native exporter, converter and portable CLI binaries; run explicitly on the native PostgreSQL host"]
async fn native_portfolio_submit_preserves_values_and_original_receipts(pool: PgPool) {
    use native_account_pipeline::{assert_clean_exit, assert_snapshot, records, run};

    let a = fixture(&pool).await;
    let directory = tempfile::tempdir().unwrap();
    // Only the trusted binding is reused; no hand-written monetary snapshot is
    // fed into this pipeline. The exporter uses official Portfolio::build_snapshot.
    let binding = observation(a.project)["binding"].clone();
    let binding_path = directory.path().join("binding.json");
    fs::write(&binding_path, serde_json::to_vec(&binding).unwrap()).unwrap();
    let exported = run(
        "QUAZONAI_NATIVE_ACCOUNT_FIXTURE_BIN",
        directory.path(),
        &[],
        None,
    )
    .await;
    let native = records(&exported.stdout);
    assert_eq!(native.len(), 2);
    let native_path = directory.path().join("native-snapshots.ndjson");
    fs::write(&native_path, &exported.stdout).unwrap();
    let segment_path = directory.path().join("retained-envelopes.ndjson");
    let before_conversion = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
    let converted = run(
        "QUAZONAI_NATIVE_JOB_BIN",
        directory.path(),
        &[
            "native-account-observation",
            "--binding",
            binding_path.to_str().unwrap(),
            "--last-sequence",
            "0",
            "--dropped-events",
            "0",
            "--stream",
            "--output",
            segment_path.to_str().unwrap(),
        ],
        Some(&native_path),
    )
    .await;
    let after_conversion = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
    assert!(
        converted.stdout.is_empty(),
        "stream mode writes its retained segment"
    );
    let segment = fs::read(&segment_path).unwrap();
    let envelopes = records(&segment);
    assert_eq!(envelopes.len(), 2);
    for (index, (envelope, snapshot)) in envelopes.iter().zip(&native).enumerate() {
        assert_eq!(envelope["binding"], binding);
        assert_eq!(envelope["sequence"], (index + 1).to_string());
        assert_eq!(envelope["dropped_events"], "0");
        assert_eq!(envelope["connection"], "UNKNOWN");
        let observed: u64 = envelope["observed_at_ns"]
            .as_str()
            .unwrap()
            .parse()
            .unwrap();
        assert!((before_conversion..=after_conversion).contains(&observed));
        assert_snapshot(&envelope["snapshot"], snapshot);
        assert_eq!(
            snapshot["ts_event"],
            json!(1700000000000000001_u64 + index as u64)
        );
        assert_eq!(snapshot["ts_init"], snapshot["ts_event"]);
        assert_eq!(snapshot["is_stale"], false);
        for field in [
            "stale_instruments",
            "stale_currencies",
            "unpriced_instruments",
        ] {
            assert_eq!(snapshot[field], json!([]));
        }
    }
    assert_ne!(native[0]["event_id"], native[1]["event_id"]);
    // Test AccountState inputs change balances. Their difference is not a
    // strategy return, realized PnL, execution fill or Sandbox trading result.
    for (index, usd, usdt) in [(0, "1000.25", "0.12345678"), (1, "1001.75", "0.22345678")] {
        let equity = envelopes[index]["snapshot"]["total_equity"]
            .as_array()
            .unwrap();
        assert_eq!(equity.len(), 2);
        assert!(equity.contains(&json!({"amount": usd, "currency": "USD"})));
        assert!(equity.contains(&json!({"amount": usdt, "currency": "USDT"})));
    }

    let credential = directory.path().join("downstream-token");
    fs::write(&credential, &a.token).unwrap();
    fs::set_permissions(&credential, fs::Permissions::from_mode(0o600)).unwrap();
    let arguments = [
        "--origin",
        &a.origin,
        "--credential-file",
        credential.to_str().unwrap(),
        "--development-http",
        "forward",
        "accounts",
        "submit",
    ];
    let mut receipts = Vec::new();
    for envelope in &envelopes {
        let key = format!("native-account-{}", envelope["sequence"].as_str().unwrap());
        let mut submit_arguments = arguments.to_vec();
        submit_arguments.extend(["--idempotency-key", key.as_str()]);
        let sent =
            portable_client::saved(directory.path(), &submit_arguments, envelope.clone()).await;
        assert_clean_exit(&sent, "native account submit", Some(&a.token));
        assert!(!String::from_utf8_lossy(&sent.stdout).contains(&a.token));
        receipts.push(serde_json::from_slice::<Value>(&sent.stdout).unwrap());
    }
    assert_eq!(receipts.len(), envelopes.len());
    for (receipt, envelope) in receipts.iter().zip(&envelopes) {
        assert_eq!(receipt["replayed"], false);
        assert_eq!(receipt["resource"]["observation"], *envelope);
        assert_eq!(receipt["resource"]["downstream_id"], json!(a.downstream));
        assert_eq!(receipt["resource"]["gap_before"], false);
        assert!(
            chrono::DateTime::parse_from_rfc3339(
                receipt["resource"]["received_at"].as_str().unwrap()
            )
            .is_ok()
        );
    }
    let first = &receipts[0]["resource"];
    let latest = &receipts[1]["resource"];
    assert_eq!(latest["source_id"], first["source_id"]);
    assert_ne!(latest["id"], first["id"]);
    let base = source_path(&a, latest);
    let current = browser_get(&a, &format!("{base}/current")).await;
    assert_eq!(current["latest_snapshot"], *latest);
    assert_eq!(current["valuation"], "PRICED");
    assert_eq!(current["source"]["binding"], binding);
    assert_eq!(current["source"]["downstream_id"], json!(a.downstream));
    assert_eq!(current["source"]["connection"], "UNKNOWN");
    assert_eq!(current["source"]["last_sequence"], "2");
    assert_eq!(current["source"]["dropped_events"], "0");
    assert_eq!(current["source"]["has_gap"], false);
    assert_eq!(current["source"]["last_observation_id"], latest["id"]);
    assert_eq!(current["source"]["latest_snapshot_id"], latest["id"]);
    assert_eq!(
        current["source"]["last_observed_at_ns"],
        envelopes[1]["observed_at_ns"]
    );
    assert_eq!(current["source"]["last_received_at"], latest["received_at"]);
    let history = browser_get(&a, &format!("{base}/observations")).await;
    assert_eq!(history["items"], json!([latest, first]));
    assert!(history["next_cursor"].is_null());
    let config = directory.path().join("quazonai");
    fs::create_dir(&config).unwrap();
    fs::set_permissions(&config, fs::Permissions::from_mode(0o700)).unwrap();
    let owner_token = owner_cli_session(&a).await;
    let token = owner_token.as_str();
    let profile = config.join("client.json");
    let profile_bytes = serde_json::to_vec(&json!({
        "schema_version":1,"origin":a.origin,"token":token,"development_http":true,"ca_certificate":null
    })).unwrap();
    fs::write(&profile, profile_bytes).unwrap();
    fs::set_permissions(&profile, fs::Permissions::from_mode(0o600)).unwrap();
    let project = a.project.to_string();
    let source = first["source_id"].as_str().unwrap();
    for (command, arguments) in [
        (
            "current",
            vec!["forward", "accounts", "current", &project, source],
        ),
        ("sources", vec!["forward", "accounts", "sources", &project]),
        (
            "history",
            vec!["forward", "accounts", "history", &project, source],
        ),
    ] {
        let output = portable_client::saved(directory.path(), &arguments, Value::Null).await;
        assert_clean_exit(&output, "native account readback", Some(token));
        assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        match command {
            "current" => {
                assert_eq!(value["latest_snapshot"], *latest);
                assert_eq!(value["source"]["last_observation_id"], latest["id"]);
                assert_eq!(value["valuation"], current["valuation"]);
            }
            "sources" => {
                assert_eq!(value["items"].as_array().unwrap().len(), 1);
                assert_eq!(value["items"][0]["binding"], binding);
                assert_eq!(value["items"][0]["latest_snapshot_id"], latest["id"]);
            }
            "history" => assert_eq!(value["items"], json!([latest, first])),
            _ => unreachable!(),
        }
    }
    let before = ledger(&pool).await;
    assert_eq!(before["sources"].as_array().unwrap().len(), 1);
    assert_eq!(before["observations"].as_array().unwrap().len(), 2);
    assert_eq!(before["cursors"].as_array().unwrap().len(), 1);

    // Restart the real CLI and submit the same retained envelopes. IDs, receipt
    // clocks and every persisted cursor/source field must be unchanged.
    let mut replayed = Vec::new();
    for envelope in &envelopes {
        // Reuse the original operation key and complete envelope after restart.
        let key = format!("native-account-{}", envelope["sequence"].as_str().unwrap());
        let mut submit_arguments = arguments.to_vec();
        submit_arguments.extend(["--idempotency-key", key.as_str()]);
        let replay =
            portable_client::saved(directory.path(), &submit_arguments, envelope.clone()).await;
        assert_clean_exit(&replay, "native account replay", Some(&a.token));
        assert!(!String::from_utf8_lossy(&replay.stdout).contains(&a.token));
        replayed.push(serde_json::from_slice::<Value>(&replay.stdout).unwrap());
    }
    assert_eq!(replayed.len(), receipts.len());
    for (replayed, original) in replayed.iter().zip(&receipts) {
        assert_eq!(replayed["replayed"], true);
        assert_eq!(replayed["resource"], original["resource"]);
    }
    assert_eq!(
        ledger(&pool).await,
        before,
        "replay cannot refresh or duplicate native evidence"
    );
    let after = browser_get(&a, &format!("{base}/current")).await;
    assert_eq!(after["latest_snapshot"], current["latest_snapshot"]);
    let mut after_source = after["source"].clone();
    let mut before_source = current["source"].clone();
    // checked_at is the read's clock, not refreshed source or valuation evidence.
    after_source.as_object_mut().unwrap().remove("checked_at");
    before_source.as_object_mut().unwrap().remove("checked_at");
    assert_eq!(after_source, before_source);
    assert_eq!(after["valuation"], current["valuation"]);
    assert_eq!(
        browser_get(&a, &format!("{base}/observations")).await,
        history
    );
    assert_eq!(fs::read(&segment_path).unwrap(), segment);
    assert_eq!(fs::read(&native_path).unwrap(), exported.stdout);
    eprintln!("NATIVE_ACCOUNT_SUBMIT_ACCEPTANCE_OK native_records=2 receipts=2 replayed=2");
}

#[path = "support/paper_capital_exit_registration.rs"]
mod paper_capital_exit_registration;

#[path = "support/paper_service_acceptance.rs"]
mod paper_service_acceptance;

#[path = "support/capital_exit_bridge_diagnostics.rs"]
mod capital_exit_bridge_diagnostics;
