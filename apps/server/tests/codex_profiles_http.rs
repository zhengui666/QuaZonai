//! Real native authentication/Vault/HTTP/PostgreSQL profile boundaries. Native
//! probe tests additionally run the pinned official App Server, never a fake client.
#[cfg(feature = "native-codex")]
#[path = "support/codex_responses.rs"]
// This HTTP suite probes catalogs, not the shared turn-completion helpers.
#[allow(dead_code)]
mod responses;
mod support;
use axum::{
    body::Body,
    http::{header, Request, StatusCode},
};
use contracts::{codex::ProfileOrigin, SchemaV1};
use integrations::secrets::SecretVault;
use serde_json::{json, Value};
use server::{
    codex_profiles::{CodexDeployment, CodexDeploymentBinding, CodexDeploymentConfig},
    AppState, WebPolicy,
};
use sqlx::PgPool;
use std::path::{Path, PathBuf};
use support::{Fixture, Reply};
use tower_sessions::cookie::Key;

async fn configured(pool: PgPool, home: &Path, binary: PathBuf) -> (Fixture, String) {
    let mut fixture = support::fixture(pool).await;
    let deployment = CodexDeployment::new(CodexDeploymentConfig {
        schema_version: SchemaV1,
        binary,
        executable_path: std::env::var("PATH").unwrap(),
        bindings: vec![CodexDeploymentBinding {
            reference: "native-test-home".into(),
            label: "Operator native home".into(),
            profile_origin: ProfileOrigin::OperatorMount,
            home: home.to_path_buf(),
            codex_home: home.to_path_buf(),
            working_directory: home.to_path_buf(),
            environment_names: vec![],
        }],
    })
    .unwrap();
    fixture.app = server::router(
        AppState::new(
            fixture.store.clone(),
            SecretVault::open(
                &fixture._state.path().join("secrets"),
                &fixture._state.path().join("master.key"),
            )
            .unwrap(),
            WebPolicy::new(
                "https://research.example",
                "127.0.0.1:8080".parse().unwrap(),
                false,
            )
            .unwrap(),
        )
        .with_codex_deployment(deployment),
        Key::generate(),
    );
    let (enrollment, cookie, native) = support::start(&fixture).await;
    let (confirmed, _) = support::confirm(&fixture, &enrollment, &cookie, &native, false).await;
    assert_eq!(confirmed.status, StatusCode::OK);
    // confirm's second value is the one-time TOTP code, not the rotated session.
    let cookie = confirmed
        .cookie
        .expect("native authenticated session cookie");
    (fixture, cookie)
}

async fn command(
    f: &Fixture,
    cookie: &str,
    key: &str,
    method: &str,
    path: &str,
    body: Value,
) -> Reply {
    support::exchange(
        &f.app,
        Request::builder()
            .method(method)
            .uri(path)
            .header(header::HOST, "research.example")
            .header(header::ORIGIN, "https://research.example")
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", key)
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
}

fn profile() -> Value {
    json!({"schema_version":1,"name":"System Codex","home_binding":"native-test-home","profile_origin":"OPERATOR_MOUNT","connection":{"mode":"SYSTEM"},
        "model_settings":{"schema_version":1,"use_default_model_settings":true,"saved_model":"dormant-value","saved_reasoning_effort":"dormant-effort","saved_fast_mode":false}})
}

#[sqlx::test(migrations = "../../migrations")]
async fn profiles_use_the_real_settings_boundary_and_public_views_expose_no_paths(pool: PgPool) {
    let home = tempfile::tempdir().unwrap();
    // Existence verifies deployment binding only; this test performs no native probe.
    let (f, cookie) = configured(pool.clone(), home.path(), std::env::current_exe().unwrap()).await;
    let homes = support::call(&f, "GET", "/api/v2/codex/homes", Value::Null, Some(&cookie)).await;
    assert_eq!(homes.status, StatusCode::OK);
    assert_eq!(homes.body[0]["reference"], "native-test-home");
    assert!(!homes
        .body
        .to_string()
        .contains(home.path().to_str().unwrap()));
    let anonymous = support::call(&f, "GET", "/api/v2/settings/codex", Value::Null, None).await;
    assert_eq!(anonymous.status, StatusCode::UNAUTHORIZED);
    let created = command(
        &f,
        &cookie,
        "new-profile",
        "POST",
        "/api/v2/settings/codex",
        profile(),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let view = &created.body["resource"];
    assert_eq!(view["connection_mode"], "SYSTEM");
    assert_eq!(view["credential_configured"], false);
    assert!(view.get("custom_api_key_ref").is_none());
    assert_eq!(view["model_settings"], profile()["model_settings"]);
    let id = view["id"].as_str().unwrap();
    for path in [
        format!("/api/v2/codex/models?profile_id={id}"),
        format!("/api/v2/codex/account?profile_id={id}"),
    ] {
        let current = support::call(&f, "GET", &path, Value::Null, Some(&cookie)).await;
        assert_eq!(current.status, StatusCode::OK);
        assert_eq!(current.body["state"], "NEVER_PROBED");
        assert_eq!(current.body["observation"], Value::Null);
        assert_eq!(current.headers[header::CACHE_CONTROL], "no-store");
    }
    let replay = command(
        &f,
        &cookie,
        "new-profile",
        "POST",
        "/api/v2/settings/codex",
        profile(),
    )
    .await;
    assert_eq!(replay.status, StatusCode::CREATED);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(replay.body["resource"], created.body["resource"]);
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM app.codex_profile_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(rows, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_home_provider_secret_and_stale_cas_cannot_mutate_the_current_profile(
    pool: PgPool,
) {
    let home = tempfile::tempdir().unwrap();
    let (f, cookie) = configured(pool.clone(), home.path(), std::env::current_exe().unwrap()).await;
    let mut unknown = profile();
    unknown["home_binding"] = json!("unregistered-home");
    let denied = command(
        &f,
        &cookie,
        "bad-home",
        "POST",
        "/api/v2/settings/codex",
        unknown,
    )
    .await;
    assert_eq!(denied.status, StatusCode::SERVICE_UNAVAILABLE);
    let mut injected = profile();
    injected["connection"]["base_url"] = json!("https://unused.invalid");
    assert_eq!(
        command(
            &f,
            &cookie,
            "injected",
            "POST",
            "/api/v2/settings/codex",
            injected,
        )
        .await
        .status,
        StatusCode::UNPROCESSABLE_ENTITY
    );
    let created = command(
        &f,
        &cookie,
        "correct",
        "POST",
        "/api/v2/settings/codex",
        profile(),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let id = created.body["resource"]["id"].as_str().unwrap();
    let mut update = json!({"schema_version":1,"expected_revision":created.body["resource"]["revision"],"name":"Native updated","connection":{"mode":"SYSTEM"},"model_settings":profile()["model_settings"]});
    let changed = command(
        &f,
        &cookie,
        "update",
        "PATCH",
        "/api/v2/settings/codex",
        json!({"schema_version":1,"profile_id":id,"request":update}),
    )
    .await;
    assert_eq!(changed.status, StatusCode::OK);
    assert_eq!(
        command(
            &f,
            &cookie,
            "stale",
            "PATCH",
            &format!("/api/v2/settings/codex/{id}"),
            update.clone(),
        )
        .await
        .status,
        StatusCode::CONFLICT
    );
    update["expected_revision"] = changed.body["resource"]["revision"].clone();
    update["connection"] = json!({"mode":"CUSTOM_PROVIDER","base_url":"https://provider.invalid/v1","credential_ref":contracts::Id::new()});
    let unknown_secret = command(
        &f,
        &cookie,
        "unknown-secret",
        "PATCH",
        &format!("/api/v2/settings/codex/{id}"),
        update,
    )
    .await;
    assert_eq!(unknown_secret.status, StatusCode::UNPROCESSABLE_ENTITY);
    let current = support::call(
        &f,
        "GET",
        &format!("/api/v2/settings/codex/{id}"),
        Value::Null,
        Some(&cookie),
    )
    .await;
    assert_eq!(current.body, changed.body["resource"]);
    assert!(!unknown_secret
        .body
        .to_string()
        .contains(home.path().to_str().unwrap()));
}

#[cfg(feature = "native-codex")]
#[sqlx::test(migrations = "../../migrations")]
async fn explicit_probe_observes_actual_system_configuration_without_any_model_request(
    pool: PgPool,
) {
    let home = tempfile::tempdir().unwrap();
    let provider = responses::Provider::start(home.path()).await;
    let binary =
        PathBuf::from(std::env::var_os("CODEX_NATIVE_BIN").expect("pinned native binary required"));
    let (f, cookie) = configured(pool.clone(), home.path(), binary).await;
    let created = command(
        &f,
        &cookie,
        "native-profile",
        "POST",
        "/api/v2/settings/codex",
        profile(),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let id = created.body["resource"]["id"].as_str().unwrap();
    let request = json!({"schema_version":1,"profile_id":id,"expected_revision":created.body["resource"]["revision"]});
    let checked = command(
        &f,
        &cookie,
        "native-probe",
        "POST",
        "/api/v2/codex/probe",
        request.clone(),
    )
    .await;
    assert_eq!(checked.status, StatusCode::OK);
    assert_eq!(checked.body["resource"]["outcome"]["status"], "AVAILABLE");
    assert_eq!(
        checked.body["resource"]["outcome"]["native_version"],
        "0.144.4"
    );
    assert_eq!(
        checked.body["resource"]["outcome"]["effective"]["provider"],
        "local_fixture"
    );
    assert_eq!(
        checked.body["resource"]["outcome"]["effective"]["model"],
        "gpt-5.4"
    );
    assert!(
        checked.body["resource"]["outcome"]["models"]
            .as_array()
            .unwrap()
            .len()
            > 1
    );
    assert_eq!(provider.request_count(), 0, "probe must not call Responses");
    assert!(!checked
        .body
        .to_string()
        .contains(home.path().to_str().unwrap()));
    let replay = command(
        &f,
        &cookie,
        "native-probe",
        "POST",
        "/api/v2/codex/probe",
        request,
    )
    .await;
    assert_eq!(replay.body["resource"], checked.body["resource"]);
    assert_eq!(replay.body["replayed"], true);
    assert!(!home.path().join("auth.json").exists());
    let counts: (i64, i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.codex_profile_observations),(SELECT count(*) FROM app.command_receipts WHERE operation='CODEX_PROBE')")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1));
}

#[cfg(feature = "native-codex")]
#[sqlx::test(migrations = "../../migrations")]
async fn account_routes_preserve_acceptance_and_observe_native_logout(pool: PgPool) {
    use std::time::Duration;
    let home = tempfile::tempdir().unwrap();
    // The official release forbids ChatGPT login under this native setting; no
    // debug issuer override, live account, device code or network login is used.
    std::fs::write(
        home.path().join("config.toml"),
        "forced_login_method = \"api\"\ncli_auth_credentials_store = \"file\"\n",
    )
    .unwrap();
    let binary =
        PathBuf::from(std::env::var_os("CODEX_NATIVE_BIN").expect("pinned native binary required"));
    let (f, cookie) = configured(pool.clone(), home.path(), binary).await;
    let created = command(
        &f,
        &cookie,
        "account-profile",
        "POST",
        "/api/v2/settings/codex",
        profile(),
    )
    .await;
    assert_eq!(created.status, StatusCode::CREATED);
    let profile_id = created.body["resource"]["id"].as_str().unwrap();
    let latest = format!("/api/v2/codex/login?profile_id={profile_id}");
    let empty = support::call(&f, "GET", &latest, Value::Null, Some(&cookie)).await;
    assert_eq!(empty.status, StatusCode::OK);
    assert_eq!(empty.body, Value::Null);
    let request = json!({"schema_version":1,"profile_id":profile_id,"expected_revision":created.body["resource"]["revision"]});
    for (path, key, expected_state, expected_reason) in [
        (
            "/api/v2/codex/login/start",
            "account-login",
            "UNKNOWN",
            "NATIVE_RESPONSE_UNKNOWN",
        ),
        (
            "/api/v2/codex/logout",
            "account-logout",
            "SUCCEEDED",
            "NATIVE_LOGOUT_COMPLETED",
        ),
    ] {
        let initial = command(&f, &cookie, key, "POST", path, request.clone()).await;
        assert_eq!(initial.status, StatusCode::ACCEPTED);
        assert_eq!(initial.body["device_code"], Value::Null);
        let id = initial.body["acceptance"]["resource"]["id"]
            .as_str()
            .unwrap();
        let operation_path = format!("/api/v2/codex/login/{id}");
        let current = tokio::time::timeout(Duration::from_secs(15), async {
            loop {
                let current =
                    support::call(&f, "GET", &operation_path, Value::Null, Some(&cookie)).await;
                assert_eq!(current.status, StatusCode::OK);
                if current.body["finished_at"] != Value::Null {
                    break current;
                }
                tokio::time::sleep(Duration::from_millis(25)).await;
            }
        })
        .await
        .expect("bounded native account operation did not finish");
        assert_eq!(current.body["state"], expected_state);
        assert_eq!(current.body["reason"], expected_reason);
        assert!(current.body.get("device_code").is_none());
        assert!(current.body.get("native_login_id").is_none());
        assert_eq!(current.headers[header::CACHE_CONTROL], "no-store");
        let replay = command(&f, &cookie, key, "POST", path, request.clone()).await;
        assert_eq!(replay.status, StatusCode::ACCEPTED);
        assert_eq!(
            replay.body["acceptance"]["resource"],
            initial.body["acceptance"]["resource"]
        );
        assert_eq!(replay.body["acceptance"]["replayed"], true);
        assert_eq!(replay.body["current"], current.body);
        assert_eq!(
            support::call(&f, "GET", &latest, Value::Null, Some(&cookie))
                .await
                .body,
            current.body
        );
        if expected_state == "SUCCEEDED" {
            assert_eq!(current.body["account"]["authentication_kind"], Value::Null);
        }
    }
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.codex_account_operations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "replaying acceptance must not start a new operation"
    );
    assert!(!home.path().join("auth.json").exists());
}
