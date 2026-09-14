//! Native CLI/TCP/PG authentication and failed admission; not positive qualification.
#[path = "support/client.rs"]
mod client;
#[path = "../../../tests/support/mandate.rs"]
mod mandate_support;
mod support;
use axum::http::StatusCode;
use client::{browser, invoke, listen};
use serde_json::{json, Value};
use sqlx::PgPool;
use std::{fs, os::unix::fs::PermissionsExt};

#[sqlx::test(migrations = "../../migrations")]
async fn portfolio_cli_requires_exact_human_intent_and_never_admits_a_missing_cycle(pool: PgPool) {
    check_intent(pool, "build").await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn candidate_simulation_cli_requires_its_own_exact_human_intent(pool: PgPool) {
    check_intent(pool, "simulate").await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn portfolio_study_cli_requires_its_own_exact_human_intent(pool: PgPool) {
    check_intent(pool, "study").await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_cli_requires_its_own_exact_human_intent(pool: PgPool) {
    check_intent(pool, "create").await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_rejection_cli_binds_exact_intent(pool: PgPool) {
    check_intent(pool, "reject").await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_reconsideration_cli_binds_exact_intent(pool: PgPool) {
    check_intent(pool, "reconsider").await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn release_approval_cli_binds_original_release_and_exact_intent(pool: PgPool) {
    check_intent(pool, "approve").await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn handoff_offer_cli_binds_exact_original_approval(pool: PgPool) {
    check_intent(pool, "offer").await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn approval_revocation_cli_binds_exact_original_approval(pool: PgPool) {
    check_intent(pool, "revoke").await;
}

async fn check_intent(pool: PgPool, command: &str) {
    let approve = command == "approve";
    let revoke = command == "revoke";
    let offer = command == "offer";
    let decision = matches!(command, "reject" | "reconsider");
    let release = command == "create" || decision || approve;
    let group = if revoke {
        "approval"
    } else if offer {
        "handoff"
    } else if release {
        "release"
    } else {
        "portfolio"
    };
    let simulation = command != "build";
    let operation = match command {
        "create" => "RELEASE_CREATE",
        "approve" => "RELEASE_APPROVE",
        "revoke" => "APPROVAL_REVOKE",
        "offer" => "HANDOFF_OFFER",
        "reject" => "RELEASE_REJECT",
        "reconsider" => "RELEASE_REOPEN",
        "study" => "PORTFOLIO_STUDY",
        "simulate" => "PORTFOLIO_SIMULATE",
        _ => "PORTFOLIO_BUILD",
    };
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
    let source = mandate_support::request(&pool, &f.store, &actor).await;
    let mandate = f
        .store
        .create_mandate(&actor, "mandate", &source)
        .await
        .unwrap()
        .resource;
    let principal = browser(&f, &cookie, "build-cli", "/api/v2/machine-principals", json!({"schema_version":1,"name":"Portfolio CLI","kind":"CLI","project_id":source.project_id,"downstream_id":null,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential = browser(&f, &cookie, "build-token", &format!("/api/v2/machine-principals/{}/credentials", principal.body["resource"]["id"].as_str().unwrap()), json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let file = f._state.path().join("portfolio-cli-token");
    fs::write(&file, token).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let mut body = json!({"schema_version":1,"cycle_id":contracts::Id::new(),"mandate_id":mandate.id,
        "input_set_id":contracts::Id::new(),"runtime_id":source.runtime_id,"expected_runtime_revision":source.expected_runtime_revision,
        "current_weights_source":{"kind":"FORWARD_SNAPSHOT","snapshot_id":contracts::Id::new()},"environment":"PAPER",
        "members":[{"qualification_id":contracts::Id::new(),"ensemble_weight":"0.5"},{"qualification_id":contracts::Id::new(),"ensemble_weight":"0.5"}],
        "limits":{"schema_version":1,"experiments":0,"cpu_seconds":"10","wall_seconds":10,"memory_mib":64,"output_bytes":"1024"}});
    if simulation {
        let object = body.as_object_mut().unwrap();
        for key in [
            "mandate_id",
            "current_weights_source",
            "environment",
            "members",
        ] {
            object.remove(key);
        }
        object.insert("candidate_id".into(), json!(mandate.id));
        if command == "study" {
            object.remove("input_set_id");
        }
    }
    if release {
        body = json!({"schema_version":1,"candidate_id":mandate.id,"evaluation_id":contracts::Id::new()});
    }
    if decision {
        body = json!({"schema_version":1,"expected_latest_decision_id":mandate.id,"reason_code":"USER_DECISION","reason":"Controlled command intent"});
        if command == "reject" {
            body["downstream_id"] = json!(contracts::Id::new());
            body["environment"] = json!("PAPER");
            body["expected_latest_decision_id"] = Value::Null;
        }
    }
    if approve {
        body = json!({"schema_version":1,"downstream_id":contracts::Id::new(),"environment":"PAPER","expected_downstream_revision":"1","expected_latest_decision_id":null,"valid_until":chrono::Utc::now()+chrono::Duration::hours(1)});
    }
    if offer {
        body = json!({"schema_version":1,"release_id":contracts::Id::new(),"approval_id":mandate.id,"supersedes_handoff_id":null,"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)});
    }
    if revoke {
        body = json!({"schema_version":1,"expected_latest_revocation_id":null,"effective_at":null,"reason_code":"WITHDRAWN","reason":"Withdraw exact approval"});
    }
    let target = mandate.id.to_string();
    let mut denied_arguments = vec!["--idempotency-key", "build", group, command];
    if decision || approve || revoke {
        denied_arguments.push(&target);
    }
    let (origin, _listener) = listen(&f).await;
    let denied = invoke(&origin, &file, &denied_arguments, body.clone()).await;
    assert!(!denied.status.success());
    assert!(denied.stdout.is_empty());
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let human = invoke(&origin, &file, &["--idempotency-key","build-human","operator-grant"], json!({"schema_version":1,"command":{"operation":operation,"request":body},"target_id":mandate.id,"code":totp.generate((now/30+1)*30)})).await;
    let diagnostic: Value = serde_json::from_slice(&human.stderr).unwrap_or(Value::Null);
    assert!(
        human.status.success(),
        "portfolio human intent failed: status={} code={} title={}",
        diagnostic["status"],
        diagnostic["code"],
        diagnostic["title"]
    );
    let grant: Value = serde_json::from_slice(&human.stdout).unwrap();
    let mut arguments = vec![
        "--idempotency-key",
        "build",
        "--operator-grant",
        grant["resource"]["id"].as_str().unwrap(),
        group,
        command,
    ];
    if decision || approve || revoke {
        arguments.push(&target);
    }
    for _ in 0..2 {
        let rejected = invoke(&origin, &file, &arguments, body.clone()).await;
        assert!(!rejected.status.success());
        assert!(rejected.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&rejected.stderr).contains(token));
        assert_eq!(
            serde_json::from_slice::<Value>(&rejected.stderr).unwrap()["status"],
            if simulation { 404 } else { 422 }
        );
    }
    let mut changed = body;
    if revoke {
        changed["reason"] = json!("Changed revocation");
    } else if approve {
        changed["downstream_id"] = json!(contracts::Id::new());
    } else if offer {
        changed["release_id"] = json!(contracts::Id::new());
    } else if decision {
        changed["reason_code"] = json!("CHANGED_DECISION");
    } else {
        changed[if release { "evaluation_id" } else { "cycle_id" }] = json!(contracts::Id::new());
    }
    let rejected = invoke(&origin, &file, &arguments, changed).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&rejected.stderr).unwrap()["status"],
        403
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.runs WHERE kind IN ('PORTFOLIO_BUILD','PORTFOLIO_SIMULATE')",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 0);
}
