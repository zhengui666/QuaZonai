//! Native transaction semantics beneath the cryptographically verified HTTP
//! boundary. Internal Actor facts here are fixture inputs, not an auth bypass.
mod support;
use chrono::{Duration, Utc};
use contracts::{control::*, runs::ProjectState, Id, Revision, SchemaV1};
use sqlx::PgPool;
use store::{authority::Actor, Store, StoreError};

async fn operator(pool: &PgPool) -> (Store, Actor) {
    let store = Store::from_pool(pool.clone());
    let cap = store
        .issue_bootstrap_capability("$argon2id$fixture-verified-by-native-adapter")
        .await
        .unwrap();
    let binding = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQ";
    let e = store
        .start_enrollment(cap.id, &cap.verifier, Id::new(), binding)
        .await
        .unwrap();
    let login = store
        .confirm_enrollment(
            e.id,
            binding,
            e.secret_ref,
            e.database_now.timestamp() / 30,
            false,
            None,
        )
        .await
        .unwrap();
    (store, Actor::Browser { login_id: login.id })
}
fn create(name: &str) -> ProjectCreate {
    ProjectCreate {
        schema_version: SchemaV1,
        name: name.into(),
        description: String::new(),
        fork_from_project_id: None,
    }
}
async fn machine(
    store: &Store,
    actor: &Actor,
    project: Option<Id>,
    kind: AssignablePrincipalKind,
) -> (PrincipalView, CredentialView, Actor) {
    let p = store
        .create_principal(
            actor,
            &Id::new().to_string(),
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "fixture".into(),
                kind,
                project_id: project,
                downstream_id: None,
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let c = store
        .issue_credential(
            actor,
            &Id::new().to_string(),
            p.id,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![if project.is_some() {
                    MachineScope::ResearchRead
                } else {
                    MachineScope::DoctorRead
                }],
                expires_at: Utc::now() + Duration::hours(1),
            },
            Id::new(),
            Id::new(),
        )
        .await
        .unwrap()
        .resource;
    let challenge = store.machine_challenge(c.public_token_id).await.unwrap();
    let machine = challenge.verified_actor(None);
    (p, c, machine)
}

#[sqlx::test(migrations = "../../migrations")]
async fn two_exact_command_retries_commit_one_project_lineage_and_original_receipt(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let request = create("one");
    let (a, b) = tokio::join!(
        store.create_project(&actor, "same", &request),
        store.create_project(&actor, "same", &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.projects),(SELECT count(*) FROM app.research_lineages),(SELECT count(*) FROM app.command_receipts WHERE operation='PROJECT_CREATE')").fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1));
    assert!(matches!(
        store.create_project(&actor, "same", &create("other")).await,
        Err(StoreError::IdempotencyConflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn parallel_cas_reports_real_current_revision_and_does_not_leave_receipt(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let p = store
        .create_project(&actor, "create", &create("p"))
        .await
        .unwrap()
        .resource;
    let a = ProjectUpdate {
        schema_version: SchemaV1,
        expected_revision: p.revision,
        name: "a".into(),
        description: String::new(),
        state: ProjectState::Paused,
    };
    let mut b = a.clone();
    b.name = "b".into();
    let (left, right) = tokio::join!(
        store.update_project(&actor, "a", p.id, &a),
        store.update_project(&actor, "b", p.id, &b)
    );
    assert_ne!(left.is_ok(), right.is_ok());
    let err = left.err().or_else(|| right.err()).unwrap();
    assert!(matches!(err,StoreError::RevisionConflict{current} if current.get()==2));
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE operation='PROJECT_UPDATE'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(count, 1);
    let active = ProjectUpdate {
        schema_version: SchemaV1,
        expected_revision: Revision::try_from("2".to_string()).unwrap(),
        name: "a".into(),
        description: String::new(),
        state: ProjectState::Active,
    };
    assert!(matches!(
        store
            .update_project(&actor, "invalid-active", p.id, &active)
            .await,
        Err(StoreError::Domain(domain::DomainError::AdmissionClosed))
    ));
    assert!(sqlx::query_scalar::<_,bool>("SELECT NOT EXISTS(SELECT 1 FROM app.command_receipts WHERE idempotency_key='invalid-active')").fetch_one(&pool).await.unwrap());
}

#[sqlx::test(migrations = "../../migrations")]
async fn credential_revocation_wins_before_locked_read_even_after_crypto_challenge(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let p = store
        .create_project(&actor, "project", &create("p"))
        .await
        .unwrap()
        .resource;
    let (_, c, machine) = machine(
        &store,
        &actor,
        Some(p.id),
        AssignablePrincipalKind::Automation,
    )
    .await;
    store.machine_session(&machine).await.unwrap();
    let mut revoker = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.machine_credential_revocations(credential_id,effective_at,reason) VALUES($1,clock_timestamp(),'revoked after possession check')").bind(c.id.as_uuid()).execute(&mut *revoker).await.unwrap();
    let options = pool
        .connect_options()
        .as_ref()
        .clone()
        .application_name(&format!("scope-{}", Id::new()));
    let p2 = sqlx::postgres::PgPoolOptions::new()
        .max_connections(1)
        .connect_with(options)
        .await
        .unwrap();
    let mut c2 = p2.acquire().await.unwrap();
    let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *c2)
        .await
        .unwrap();
    drop(c2);
    let read_store = Store::from_pool(p2.clone());
    let read_actor = machine.clone();
    let reading = tokio::spawn(async move { read_store.project(&read_actor, p.id).await });
    support::wait_for_database_lock(&pool, pid).await;
    revoker.commit().await.unwrap();
    assert!(matches!(
        reading.await.unwrap(),
        Err(StoreError::InvalidCredentials)
    ));
    p2.close().await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn epochs_scopes_and_mission_deadlines_are_rechecked_not_claimed_by_actor(pool: PgPool) {
    let (store, actor) = operator(&pool).await;
    let p = store
        .create_project(&actor, "p", &create("p"))
        .await
        .unwrap()
        .resource;
    let (principal, credential, authority) =
        machine(&store, &actor, Some(p.id), AssignablePrincipalKind::Cli).await;
    support::sqlstate(
        sqlx::query("UPDATE app.machine_principals SET enabled=false WHERE id=$1")
            .bind(principal.id.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23514",
    );
    sqlx::query("UPDATE app.machine_principals SET enabled=false,credential_epoch=credential_epoch+1 WHERE id=$1").bind(principal.id.as_uuid()).execute(&pool).await.unwrap();
    support::sqlstate(
        sqlx::query("UPDATE app.machine_principals SET credential_epoch=1 WHERE id=$1")
            .bind(principal.id.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23514",
    );
    sqlx::query("UPDATE app.machine_principals SET enabled=true,credential_epoch=credential_epoch+1 WHERE id=$1").bind(principal.id.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store.machine_session(&authority).await,
        Err(StoreError::InvalidCredentials)
    ));
    assert!(matches!(
        store.machine_challenge(credential.public_token_id).await,
        Err(StoreError::InvalidCredentials)
    ));
    let f = support::fixture(&pool, support::budget()).await;
    let principal = Id::new();
    let cred = Id::new();
    let public = Id::new();
    let verifier = Id::new();
    sqlx::query("INSERT INTO app.machine_principals(id,name,kind,project_id,run_id,enabled,credential_epoch) VALUES($1,'fixture mission','MISSION',$2,$3,true,1)").bind(principal.as_uuid()).bind(f.project.as_uuid()).bind(f.run.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO app.machine_credentials(id,principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by) VALUES($1,$2,$3,$4,1,ARRAY['RESEARCH_READ'],clock_timestamp(),$5,'MISSION_SERVICE')").bind(cred.as_uuid()).bind(principal.as_uuid()).bind(public.to_string()).bind(verifier.to_string()).bind(f.deadline).execute(&pool).await.unwrap();
    let mission = store
        .machine_challenge(public)
        .await
        .unwrap()
        .verified_actor(None);
    let session = store.machine_session(&mission).await.unwrap();
    assert_eq!(session.run_id, Some(f.run));
    assert_eq!(session.kind, PrincipalKind::Mission);
    sqlx::query("UPDATE app.projects SET state='PAUSED' WHERE id=$1")
        .bind(f.project.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.project(&mission, f.project).await,
        Err(StoreError::InvalidCredentials)
    ));
    assert!(matches!(
        store
            .issue_credential(
                &actor,
                "no-manual-mission",
                principal,
                &CredentialIssue {
                    schema_version: SchemaV1,
                    scope_codes: vec![MachineScope::ResearchRead],
                    expires_at: f.deadline
                },
                Id::new(),
                Id::new()
            )
            .await,
        Err(StoreError::Forbidden)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn grant_binds_parent_resource_and_is_not_reusable_by_another_credential(pool: PgPool) {
    let (store, operator) = operator(&pool).await;
    let (p1, _, cli) = machine(&store, &operator, None, AssignablePrincipalKind::Cli).await;
    let (p2, _, other_cli) = machine(&store, &operator, None, AssignablePrincipalKind::Cli).await;
    let issue = CredentialIssue {
        schema_version: SchemaV1,
        scope_codes: vec![MachineScope::DoctorRead],
        expires_at: Utc::now() + Duration::minutes(10),
    };
    let command = OperatorCommand::CredentialIssue(CredentialIssueIntent {
        schema_version: SchemaV1,
        principal_id: p1.id,
        request: issue.clone(),
    });
    let snapshot = store.authentication_snapshot().await.unwrap();
    let step = snapshot.database_now.timestamp() / 30 + 1;
    let granted = store
        .issue_operator_grant(&cli, "grant", &command, None, &snapshot, step)
        .await
        .unwrap()
        .resource;
    let with_grant = |mut actor: Actor| {
        if let Actor::Machine { operator_grant, .. } = &mut actor {
            *operator_grant = Some(granted.id)
        }
        actor
    };
    assert!(matches!(
        store
            .issue_credential(
                &with_grant(cli.clone()),
                "execute",
                p2.id,
                &issue,
                Id::new(),
                Id::new()
            )
            .await,
        Err(StoreError::Forbidden)
    ));
    assert!(matches!(
        store
            .issue_credential(
                &with_grant(other_cli),
                "execute",
                p1.id,
                &issue,
                Id::new(),
                Id::new()
            )
            .await,
        Err(StoreError::Forbidden)
    ));
    let a = with_grant(cli);
    let result = store
        .issue_credential(&a, "execute", p1.id, &issue, Id::new(), Id::new())
        .await
        .unwrap();
    assert_eq!(result.resource.id, granted.target_id);
    assert!(
        store
            .issue_credential(&a, "execute", p1.id, &issue, Id::new(), Id::new())
            .await
            .unwrap()
            .replayed
    );
    assert!(matches!(
        store
            .issue_credential(&a, "another", p1.id, &issue, Id::new(), Id::new())
            .await,
        Err(StoreError::Conflict)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn issued_credential_retry_never_rebinds_its_native_verifier_and_rollback_is_atomic(
    pool: PgPool,
) {
    let (store, actor) = operator(&pool).await;
    let p = store
        .create_principal(
            &actor,
            "principal",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "doctor".into(),
                kind: AssignablePrincipalKind::Cli,
                project_id: None,
                downstream_id: None,
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let issue = CredentialIssue {
        schema_version: SchemaV1,
        scope_codes: vec![MachineScope::DoctorRead],
        expires_at: Utc::now() + Duration::hours(1),
    };
    let (a, b) = tokio::join!(
        store.issue_credential(&actor, "issue", p.id, &issue, Id::new(), Id::new()),
        store.issue_credential(&actor, "issue", p.id, &issue, Id::new(), Id::new())
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_eq!(a.resource.public_token_id, b.resource.public_token_id);
    assert_ne!(a.replayed, b.replayed);
    let mut invalid = issue.clone();
    invalid.scope_codes = vec![MachineScope::DoctorRead, MachineScope::DoctorRead];
    assert!(matches!(
        store
            .issue_credential(&actor, "bad", p.id, &invalid, Id::new(), Id::new())
            .await,
        Err(StoreError::Domain(domain::DomainError::Invalid(
            "scope_codes"
        )))
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.machine_credentials")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.command_receipts WHERE operation='CREDENTIAL_ISSUE'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receipts, 1);
}
