//! Real native authentication/Vault/HTTP/PostgreSQL profile boundaries. Native
//! probe tests additionally run the official App Server, never a fake client.
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
    let native_reference = fixture
        .store
        .local_codex_bindings()
        .await
        .unwrap()
        .into_iter()
        .find(|binding| binding.label == "研究员")
        .unwrap()
        .reference;
    let deployment = CodexDeployment::new(CodexDeploymentConfig {
        schema_version: SchemaV1,
        binary,
        executable_path: std::env::var("PATH").unwrap(),
        bindings: vec![CodexDeploymentBinding {
            reference: native_reference,
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
                "https://localhost",
                "127.0.0.1:8080".parse().unwrap(),
                false,
            )
            .unwrap(),
        )
        .with_codex_deployment(deployment),
        Key::generate(),
    );
    let confirmed = support::local_session(&fixture).await;
    assert_eq!(confirmed.status, StatusCode::OK);
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
            .header(header::HOST, "localhost")
            .header(header::ORIGIN, "https://localhost")
            .header(header::COOKIE, cookie)
            .header(header::CONTENT_TYPE, "application/json")
            .header("Idempotency-Key", key)
            .body(Body::from(serde_json::to_vec(&body).unwrap()))
            .unwrap(),
    )
    .await
}

async fn local_profile(f: &Fixture, cookie: &str) -> Value {
    let listed = support::call(
        f,
        "GET",
        "/api/v2/settings/codex",
        Value::Null,
        Some(cookie),
    )
    .await;
    assert_eq!(listed.status, StatusCode::OK);
    let items = listed.body["items"].as_array().unwrap();
    assert_eq!(items.len(), 2);
    items
        .iter()
        .find(|view| view["name"] == "研究员")
        .unwrap()
        .clone()
}

#[sqlx::test(migrations = "../../migrations")]
async fn local_roles_exist_without_registration_or_credentials_and_reads_do_not_probe(
    pool: PgPool,
) {
    let home = tempfile::tempdir().unwrap();
    let (f, cookie) = configured(pool.clone(), home.path(), std::env::current_exe().unwrap()).await;
    let view = local_profile(&f, &cookie).await;
    assert_eq!(view["connection_mode"], "SYSTEM");
    assert_eq!(view["model_settings"]["use_default_model_settings"], true);
    for field in ["credential_configured", "credential_ref", "custom_base_url"] {
        assert!(view.get(field).is_none());
    }
    assert!(!view.to_string().contains(home.path().to_str().unwrap()));
    let anonymous = support::call(&f, "GET", "/api/v2/settings/codex", Value::Null, None).await;
    assert_eq!(anonymous.status, StatusCode::OK);
    assert!(anonymous.cookie.is_some());
    let homes = support::call(&f, "GET", "/api/v2/codex/homes", Value::Null, Some(&cookie)).await;
    assert_eq!(homes.status, StatusCode::NOT_FOUND);
    let registration = command(&f, &cookie, "no-registration", "POST", "/api/v2/settings/codex",
        json!({"schema_version":1,"home_binding":"elsewhere","connection":{"mode":"CUSTOM_PROVIDER"}})).await;
    assert_eq!(registration.status, StatusCode::METHOD_NOT_ALLOWED);
    let id = view["id"].as_str().unwrap();
    for path in [
        format!("/api/v2/codex/models?profile_id={id}"),
        format!("/api/v2/codex/account?profile_id={id}"),
    ] {
        let current = support::call(&f, "GET", &path, Value::Null, Some(&cookie)).await;
        assert_eq!(current.status, StatusCode::OK);
        assert_eq!(current.body["state"], "NEVER_PROBED");
        assert!(current.body["observation"].is_null());
        assert_eq!(current.headers[header::CACHE_CONTROL], "no-store");
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.codex_profile_observations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn only_model_preferences_change_with_original_receipts_and_revision_checks(pool: PgPool) {
    let home = tempfile::tempdir().unwrap();
    let (f, cookie) = configured(pool.clone(), home.path(), std::env::current_exe().unwrap()).await;
    let view = local_profile(&f, &cookie).await;
    let id = view["id"].as_str().unwrap();
    let path = format!("/api/v2/settings/codex/{id}");
    let mut settings = view["model_settings"].clone();
    settings["saved_model"] = json!("dormant-native-model");
    settings["saved_reasoning_effort"] = json!("dormant-native-effort");
    let update =
        json!({"schema_version":1,"expected_revision":view["revision"],"model_settings":settings});
    let changed = command(&f, &cookie, "model-update", "PATCH", &path, update.clone()).await;
    assert_eq!(changed.status, StatusCode::OK);
    assert_eq!(changed.body["resource"]["name"], view["name"]);
    assert_eq!(
        changed.body["resource"]["home_binding"],
        view["home_binding"]
    );
    assert_eq!(changed.body["resource"]["model_settings"], settings);
    let replay = command(&f, &cookie, "model-update", "PATCH", &path, update.clone()).await;
    assert_eq!(replay.status, StatusCode::OK);
    assert_eq!(replay.body["resource"], changed.body["resource"]);
    assert_eq!(replay.body["replayed"], true);
    assert_eq!(
        command(&f, &cookie, "stale", "PATCH", &path, update.clone())
            .await
            .status,
        StatusCode::CONFLICT
    );
    for (field, value) in [
        (
            "connection",
            json!({"mode":"CUSTOM_PROVIDER","base_url":"https://provider.invalid/v1","credential_ref":contracts::Id::new()}),
        ),
        ("home_binding", json!("other")),
        ("name", json!("replacement")),
    ] {
        let mut injected = update.clone();
        injected["expected_revision"] = changed.body["resource"]["revision"].clone();
        injected[field] = value;
        assert_eq!(
            command(&f, &cookie, field, "PATCH", &path, injected)
                .await
                .status,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
    let current = support::call(&f, "GET", &path, Value::Null, Some(&cookie)).await;
    assert_eq!(current.body, changed.body["resource"]);
}

#[cfg(feature = "native-codex")]
#[sqlx::test(migrations = "../../migrations")]
async fn explicit_probe_observes_actual_system_configuration_without_any_model_request(
    pool: PgPool,
) {
    let home = tempfile::tempdir().unwrap();
    let provider = responses::Provider::start(home.path()).await;
    let binary =
        PathBuf::from(std::env::var_os("CODEX_NATIVE_BIN").expect("native binary required"));
    let (f, cookie) = configured(pool.clone(), home.path(), binary).await;
    let view = local_profile(&f, &cookie).await;
    let id = view["id"].as_str().unwrap();
    let request = json!({"schema_version":1,"profile_id":id,"expected_revision":view["revision"]});
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
    assert!(domain::codex::valid_codex_version(
        checked.body["resource"]["outcome"]["native_version"]
            .as_str()
            .unwrap()
    ));
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
    let view = local_profile(&f, &cookie).await;
    let profile_id = view["id"].as_str().unwrap();
    let latest = format!("/api/v2/codex/login?profile_id={profile_id}");
    let empty = support::call(&f, "GET", &latest, Value::Null, Some(&cookie)).await;
    assert_eq!(empty.status, StatusCode::OK);
    assert_eq!(empty.body, Value::Null);
    let request =
        json!({"schema_version":1,"profile_id":profile_id,"expected_revision":view["revision"]});
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

#[sqlx::test(migrations = "../../migrations")]
async fn raw_http_cannot_activate_unobserved_model_settings(pool: PgPool) {
    let home = tempfile::tempdir().unwrap();
    let (f, cookie) = configured(pool.clone(), home.path(), std::env::current_exe().unwrap()).await;
    let profile = local_profile(&f, &cookie).await;
    let id = profile["id"].as_str().unwrap();
    let path = format!("/api/v2/settings/codex/{id}");
    let mut settings = profile["model_settings"].clone();
    settings["use_default_model_settings"] = json!(false);
    settings["saved_model"] = json!("unobserved-model");
    let request = json!({
        "schema_version":1, "expected_revision":profile["revision"], "model_settings":settings
    });
    let denied = command(&f, &cookie, "unobserved", "PATCH", &path, request).await;
    assert_eq!(denied.status, StatusCode::CONFLICT, "{}", denied.body);
    assert_eq!(denied.body["code"], "DOMAIN_CONFLICT");
    let current = support::call(&f, "GET", &path, Value::Null, Some(&cookie)).await;
    assert_eq!(current.body, profile);
    let observations: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.codex_profile_observations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(observations, 0, "saving must not execute a probe");
}

#[cfg(feature = "native-codex")]
#[sqlx::test(migrations = "../../migrations")]
async fn native_probe_preserves_the_unoverridden_model_across_model_clear(pool: PgPool) {
    let home = tempfile::tempdir().unwrap();
    let provider = responses::Provider::start(home.path()).await;
    let original_config = std::fs::read(home.path().join("config.toml")).unwrap();
    let binary =
        PathBuf::from(std::env::var_os("CODEX_NATIVE_BIN").expect("pinned native binary required"));
    let (f, cookie) = configured(pool.clone(), home.path(), binary).await;
    let mut profile = local_profile(&f, &cookie).await;
    let id = profile["id"].as_str().unwrap().to_owned();
    let path = format!("/api/v2/settings/codex/{id}");
    let mut alternative = String::new();
    let mut native_effort = String::new();
    for stage in 0..3 {
        let observation = command(
            &f,
            &cookie,
            &format!("native-transition-probe-{stage}"),
            "POST",
            "/api/v2/codex/probe",
            json!({"schema_version":1,"profile_id":id,"expected_revision":profile["revision"]}),
        )
        .await;
        assert_eq!(observation.status, StatusCode::OK, "{}", observation.body);
        let outcome = &observation.body["resource"]["outcome"];
        assert_eq!(outcome["native_default_model"], "gpt-5.4");
        if stage == 1 {
            assert_eq!(outcome["effective"]["model"], alternative);
        } else {
            assert_eq!(outcome["effective"]["model"], "gpt-5.4");
        }
        if stage == 0 {
            let models = outcome["models"].as_array().unwrap();
            alternative = models
                .iter()
                .find_map(|model| {
                    model["capability"]["model"]
                        .as_str()
                        .filter(|model| *model != "gpt-5.4")
                })
                .expect("the pinned native catalog includes another model")
                .to_owned();
            native_effort = models
                .iter()
                .find(|model| model["capability"]["model"] == "gpt-5.4")
                .unwrap()["capability"]["default_reasoning_effort"]
                .as_str()
                .unwrap()
                .to_owned();
        }
        if stage == 2 {
            assert_eq!(outcome["effective"]["reasoning_effort"], native_effort);
            break;
        }
        let settings = json!({
            "schema_version":1,"use_default_model_settings":false,
            "saved_model":if stage == 0 { json!(alternative) } else { Value::Null },
            "saved_reasoning_effort":if stage == 0 { Value::Null } else { json!(native_effort) },
            "saved_fast_mode":false
        });
        let updated = command(
            &f, &cookie, &format!("native-transition-update-{stage}"), "PATCH", &path,
            json!({"schema_version":1,"expected_revision":profile["revision"],"model_settings":settings})
        ).await;
        assert_eq!(updated.status, StatusCode::OK, "{}", updated.body);
        profile = updated.body["resource"].clone();
    }
    assert_eq!(
        provider.request_count(),
        0,
        "model discovery does not call Responses"
    );
    assert_eq!(
        std::fs::read(home.path().join("config.toml")).unwrap(),
        original_config
    );
    assert!(!home.path().join("auth.json").exists());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.codex_profile_observations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn collection_patch_is_unavailable_without_changing_the_native_profile(pool: PgPool) {
    let home = tempfile::tempdir().unwrap();
    let (f, cookie) = configured(pool, home.path(), std::env::current_exe().unwrap()).await;
    let original = local_profile(&f, &cookie).await;
    let path = format!(
        "/api/v2/settings/codex/{}",
        original["id"].as_str().unwrap()
    );
    let mut settings = original["model_settings"].clone();
    settings["saved_model"] = json!("dormant-local-model");
    let update = json!({
        "schema_version":1, "expected_revision":original["revision"], "model_settings":settings
    });
    let legacy = json!({"schema_version":1,"profile_id":original["id"],"request":update});
    let rejected = command(
        &f,
        &cookie,
        "removed-collection-update",
        "PATCH",
        "/api/v2/settings/codex",
        legacy,
    )
    .await;
    assert_eq!(
        rejected.status,
        StatusCode::METHOD_NOT_ALLOWED,
        "legacy collection PATCH must be unavailable; response={}",
        rejected.body
    );
    let retained = support::call(&f, "GET", &path, Value::Null, Some(&cookie)).await;
    assert_eq!(retained.status, StatusCode::OK);
    assert_eq!(
        retained.body, original,
        "the rejected route cannot mutate profile state"
    );
    let changed = command(
        &f,
        &cookie,
        "canonical-model-update",
        "PATCH",
        &path,
        update.clone(),
    )
    .await;
    assert_eq!(changed.status, StatusCode::OK, "{}", changed.body);
    assert_eq!(changed.body["resource"]["model_settings"], settings);
    let replay = command(
        &f,
        &cookie,
        "canonical-model-update",
        "PATCH",
        &path,
        update,
    )
    .await;
    assert_eq!(replay.status, StatusCode::OK);
    assert_eq!(replay.body["resource"], changed.body["resource"]);
    assert_eq!(replay.body["replayed"], true);
}

#[test]
fn codex_openapi_exposes_only_collection_reads_and_item_model_updates() {
    let api: Value = serde_json::from_str(&server::openapi_json().unwrap()).unwrap();
    let collection = &api["paths"]["/api/v2/settings/codex"];
    assert!(collection["get"].is_object());
    assert!(collection.get("post").is_none());
    assert!(collection.get("patch").is_none());
    let item = &api["paths"]["/api/v2/settings/codex/{id}"];
    assert!(item["get"].is_object());
    assert_eq!(item["patch"]["operationId"], "updateCodexProfile");
    assert!(api["components"]["schemas"]
        .get("CodexSettingsUpdateV1")
        .is_none());
    assert!(api["components"]["schemas"]["CodexProfileUpdateV1"].is_object());
}
