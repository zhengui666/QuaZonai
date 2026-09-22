//! Native CLI -> actual HTTP/Bearer/PostgreSQL identity, with disposable credentials only.
#[path = "support/client.rs"]
mod client;
mod support;
use axum::http::StatusCode;
use client::{browser, invoke, listen};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt};

#[sqlx::test(migrations = "../../migrations")]
async fn identity_reports_the_current_public_machine_binding_without_returning_its_token(pool: PgPool) {
    let f = support::fixture(pool).await;
    let confirmed = support::local_session(&f).await;
    assert_eq!(confirmed.status, StatusCode::OK);
    let cookie = confirmed.cookie.unwrap();
    let principal = browser(&f, &cookie, "identity-principal", "/api/v2/machine-principals", json!({
        "schema_version":1,"name":"Identity regression","kind":"CLI","project_id":null,"downstream_id":null,"enabled":true
    })).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = browser(&f, &cookie, "identity-credential", &format!("/api/v2/machine-principals/{}/credentials", principal.body["resource"]["id"].as_str().unwrap()), json!({
        "schema_version":1,"scope_codes":["DOCTOR_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)
    })).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let file = f._state.path().join("identity-token");
    fs::write(&file, token).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let (origin, _listener) = listen(&f).await;
    let output = invoke(&origin, &file, &["identity"], Value::Null).await;
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let session: contracts::control::MachineSessionView = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(session.kind, contracts::control::PrincipalKind::Cli);
    assert_eq!(session.scope_codes, vec![contracts::control::MachineScope::DoctorRead]);
    assert!(session.project_id.is_none());
    assert!(session.run_id.is_none());
    assert!(session.expires_at > chrono::Utc::now());
    assert!(!String::from_utf8_lossy(&output.stdout).contains(token));
    assert!(output.stderr.is_empty());
}
