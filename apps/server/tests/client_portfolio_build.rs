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

#[sqlx::test(migrations = "../../migrations")]
async fn automation_policy_cli_freezes_original_intent_and_revokes_history(pool: PgPool) {
    use contracts::{control::*, delivery::*, settings::*, Id, SchemaV1};
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
        .create_mandate(&actor, "automation-mandate", &source)
        .await
        .unwrap()
        .resource;
    let down = f
        .store
        .create_downstream(
            &actor,
            "automation-downstream",
            &DownstreamCreate {
                schema_version: SchemaV1,
                credential_ref: Id::new(),
                configuration: DownstreamConfigurationV1 {
                    name: "Policy protocol fixture".into(),
                    endpoint: "https://policy.example".into(),
                    accepted_package_versions: vec![PackageSchemaVersion::V1],
                    environments: DownstreamEnvironments::Both,
                    enabled: true,
                    development_http: false,
                },
            },
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    let metrics = f
        .store
        .evaluation_policy(&actor, source.content.required_evaluation_policy_id)
        .await
        .unwrap()
        .metric_requirements;
    let project = f.store.project(&actor, source.project_id).await.unwrap();
    let request = AutomationAuthorizeV1 {
        schema_version: SchemaV1,
        expected_project_revision: project.revision,
        content: AutomationPolicyContentV1 {
            mode: AutomationModeV1::AutoHandoff,
            mandate_id: mandate.id,
            downstream_id: down.id,
            required_paper_observations: 20,
            minimum_paper_elapsed_seconds: contracts::DbCounter::new(60).unwrap(),
            max_feedback_age_seconds: contracts::DbCounter::new(60).unwrap(),
            promotion_metric_requirements: metrics.clone(),
            degradation_metric_requirements: metrics,
            valid_until: f
                .store
                .authentication_snapshot()
                .await
                .unwrap()
                .database_now
                + chrono::Duration::hours(1),
            enabled_for_new_rebalances: true,
            max_rebalances_per_day: 2,
        },
    };
    let principal=browser(&f,&cookie,"automation-cli","/api/v2/machine-principals",json!({"schema_version":1,"name":"Policy CLI","kind":"CLI","project_id":source.project_id,"downstream_id":null,"enabled":true})).await;
    assert_eq!(principal.status, StatusCode::CREATED);
    let credential=browser(&f,&cookie,"automation-token",&format!("/api/v2/machine-principals/{}/credentials",principal.body["resource"]["id"].as_str().unwrap()),json!({"schema_version":1,"scope_codes":["RESEARCH_READ"],"expires_at":chrono::Utc::now()+chrono::Duration::hours(1)})).await;
    assert_eq!(credential.status, StatusCode::CREATED);
    let token = credential.body["token"].as_str().unwrap();
    let file = f._state.path().join("automation-cli-token");
    fs::write(&file, token).unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let (origin, _listener) = listen(&f).await;
    let project_id = source.project_id.to_string();
    let denied = invoke(
        &origin,
        &file,
        &[
            "--idempotency-key",
            "policy-authorize",
            "automation",
            "authorize",
            &project_id,
        ],
        json!(request),
    )
    .await;
    assert!(!denied.status.success());
    let now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let human=invoke(&origin,&file,&["--idempotency-key","policy-human","operator-grant"],json!({"schema_version":1,"command":{"operation":"POLICY_AUTHORIZE","request":request},"target_id":source.project_id,"code":totp.generate((now/30+1)*30)})).await;
    assert!(human.status.success());
    let grant: Value = serde_json::from_slice(&human.stdout).unwrap();
    let args = [
        "--idempotency-key",
        "policy-authorize",
        "--operator-grant",
        grant["resource"]["id"].as_str().unwrap(),
        "automation",
        "authorize",
        &project_id,
    ];
    let mut changed = request.clone();
    changed.content.max_rebalances_per_day = 3;
    let denied = invoke(&origin, &file, &args, json!(changed)).await;
    assert_eq!(
        serde_json::from_slice::<Value>(&denied.stderr).unwrap()["status"],
        403
    );
    let mut original = None;
    for replayed in [false, true] {
        let result = invoke(&origin, &file, &args, json!(request)).await;
        let diagnostic: Value = serde_json::from_slice(&result.stderr).unwrap_or(Value::Null);
        assert!(
            result.status.success(),
            "policy CLI failed: code={} status={}",
            diagnostic["code"],
            diagnostic["status"]
        );
        let receipt: CommandResult<AutomationPolicyViewV1> =
            serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(receipt.replayed, replayed);
        assert_eq!(receipt.resource.project_id, source.project_id);
        assert_eq!(
            serde_json::to_value(&receipt.resource.content).unwrap(),
            json!(request.content)
        );
        original = Some(receipt.resource);
    }
    let original = original.unwrap();
    let current = f.store.project(&actor, source.project_id).await.unwrap();
    assert_eq!(current.current_automation_policy_id, Some(original.id));
    assert!(current.revision > project.revision);
    assert!(matches!(
        f.store
            .authorize_automation(&actor, "stale-policy", source.project_id, &request)
            .await,
        Err(store::StoreError::RevisionConflict { .. })
    ));
    changed.expected_project_revision = current.revision;
    let second = f
        .store
        .authorize_automation(&actor, "second-policy", source.project_id, &changed)
        .await
        .unwrap()
        .resource;
    assert_eq!(
        f.store
            .automation_policy(&actor, original.id)
            .await
            .unwrap()
            .content
            .max_rebalances_per_day,
        2
    );
    assert_eq!(
        f.store
            .project(&actor, source.project_id)
            .await
            .unwrap()
            .current_automation_policy_id,
        Some(second.id)
    );
    let shown = invoke(
        &origin,
        &file,
        &["automation", "show", &original.id.to_string()],
        Value::Null,
    )
    .await;
    assert!(shown.status.success());
    let listed = invoke(
        &origin,
        &file,
        &["automation", "list", &project_id, "--limit", "1"],
        Value::Null,
    )
    .await;
    assert!(listed.status.success());
    let page: Page<AutomationPolicyViewV1> = serde_json::from_slice(&listed.stdout).unwrap();
    assert_eq!(page.items[0].id, second.id);
    assert_eq!(page.next_cursor, Some(second.id));
    let revoke = PolicyRevokeV1 {
        schema_version: SchemaV1,
        expected_latest_revocation_id: None,
        effective_at: None,
        reason: "Withdraw future policy authority".into(),
    };
    let current = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let next_window = (now / 30 + 1) * 30;
    if current < next_window {
        tokio::time::sleep(std::time::Duration::from_secs(next_window - current + 1)).await;
    }
    let revoke_now = f
        .store
        .authentication_snapshot()
        .await
        .unwrap()
        .database_now
        .timestamp() as u64;
    let human=invoke(&origin,&file,&["--idempotency-key","policy-revoke-human","operator-grant"],json!({"schema_version":1,"command":{"operation":"POLICY_REVOKE","request":revoke},"target_id":original.id,"code":totp.generate((revoke_now/30+1)*30)})).await;
    assert!(human.status.success());
    let grant: Value = serde_json::from_slice(&human.stdout).unwrap();
    for replayed in [false, true] {
        let result = invoke(
            &origin,
            &file,
            &[
                "--idempotency-key",
                "policy-revoke",
                "--operator-grant",
                grant["resource"]["id"].as_str().unwrap(),
                "automation",
                "revoke",
                &original.id.to_string(),
            ],
            json!(revoke),
        )
        .await;
        assert!(result.status.success());
        let receipt: CommandResult<PolicyRevocationViewV1> =
            serde_json::from_slice(&result.stdout).unwrap();
        assert_eq!(receipt.replayed, replayed);
        assert_eq!(receipt.resource.automation_policy_id, original.id);
    }
    let history = invoke(
        &origin,
        &file,
        &["automation", "revocations", &original.id.to_string()],
        Value::Null,
    )
    .await;
    assert!(history.status.success());
    let history: Page<PolicyRevocationViewV1> = serde_json::from_slice(&history.stdout).unwrap();
    assert_eq!(history.items.len(), 1);
    let (a, b) = tokio::join!(
        f.store
            .revoke_automation(&actor, "second-revoke", second.id, &revoke),
        f.store
            .revoke_automation(&actor, "second-revoke", second.id, &revoke)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.id, b.resource.id);
    assert!(matches!(
        f.store
            .revoke_automation(&actor, "stale-revoke", second.id, &revoke)
            .await,
        Err(store::StoreError::Conflict)
    ));
    assert!(sqlx::query(
        "UPDATE app.automation_policies SET enabled_for_new_rebalances=false WHERE id=$1"
    )
    .bind(original.id.as_uuid())
    .execute(&pool)
    .await
    .is_err());
    assert!(sqlx::query(
        "UPDATE app.policy_revocations SET reason='rewrite' WHERE automation_policy_id=$1"
    )
    .bind(original.id.as_uuid())
    .execute(&pool)
    .await
    .is_err());
    let project = f.store.project(&actor, source.project_id).await.unwrap();
    f.store
        .update_project(
            &actor,
            "archive-policy-project",
            source.project_id,
            &ProjectUpdate {
                schema_version: SchemaV1,
                expected_revision: project.revision,
                name: project.name,
                description: project.description,
                state: contracts::runs::ProjectState::Archived,
            },
        )
        .await
        .unwrap();
    let later = PolicyRevokeV1 {
        expected_latest_revocation_id: Some(a.resource.id),
        effective_at: Some(chrono::Utc::now() + chrono::Duration::hours(1)),
        ..revoke
    };
    f.store
        .revoke_automation(&actor, "archive-revoke", second.id, &later)
        .await
        .unwrap();
    assert_eq!(
        f.store
            .automation_revocations(&actor, second.id, &ListQuery::default())
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.approvals")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}
