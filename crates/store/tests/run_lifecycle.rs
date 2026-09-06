//! Real PostgreSQL and PGMQ; remote outcomes here are trusted adapter fixtures,
//! not evidence that an isolated runtime or model tool loop has been delivered.
mod support;
use chrono::{Duration, Utc};
use contracts::{
    control::*,
    lifecycle::*,
    runs::{RunKind, RunState},
    DbCounter, Id, Revision, SchemaV1,
};
use domain::DomainError;
use sqlx::PgPool;
use store::{authority::Actor, lifecycle::*, Store, StoreError};

async fn setup(pool: &PgPool) -> (Store, support::Fixture, RunSubmission, Actor) {
    let f = support::fixture(pool, support::budget()).await;
    // End the unrelated model-ledger fixture before testing admission slots.
    sqlx::query("UPDATE app.runs SET state='FAILED',finished_at=clock_timestamp() WHERE id=$1")
        .bind(f.run.as_uuid())
        .execute(pool)
        .await
        .unwrap();
    let runtime = Id::new();
    sqlx::query("INSERT INTO app.runtime_integrations(id,name,endpoint,tls_policy,credential_ref,allowed_capabilities,protocol_version,enabled) VALUES($1,'runtime fixture','https://runtime.example','SYSTEM_CA','fixture-credential',ARRAY['DATA_VALIDATE'],'1',true)").bind(runtime.as_uuid()).execute(pool).await.unwrap();
    let store = Store::from_pool(pool.clone());
    let cap = store
        .issue_bootstrap_capability("$argon2id$fixture-native-verification")
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
    let actor = Actor::Browser { login_id: login.id };
    let request = RunSubmission {
        cycle_id: f.cycle,
        input_set_id: f.input_set,
        runtime_id: runtime,
        runtime_revision: Revision::INITIAL,
        kind: RunKind::DataValidate,
        limits: JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 1,
            cpu_seconds: DbCounter::new(100).unwrap(),
            wall_seconds: 3600,
            memory_mib: 1024,
            output_bytes: DbCounter::new(4096).unwrap(),
        },
    };
    (store, f, request, actor)
}
async fn message(store: &Store, id: Id) -> RunMessage {
    store
        .read_run_messages(30, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == id)
        .unwrap()
}
async fn leased(store: &Store, m: &RunMessage, owner: &str) -> RunLease {
    match store.claim_run(m, owner, 60).await.unwrap() {
        ClaimResult::Leased(lease) => *lease,
        _ => panic!("expected native lease"),
    }
}
async fn manifest(pool: &PgPool, lease: &RunLease) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,producer_run_id,producer_attempt_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,$3,$4,'REPORT','application/json','qz.job_result','1','LOCAL',$5,'1',12,'EVALUATOR_ONLY','FIXTURE','RUNTIME','AUDIT')")
        .bind(id.as_uuid()).bind(lease.run.project_id.as_uuid()).bind(lease.run.id.as_uuid()).bind(lease.fence.attempt_id.as_uuid()).bind(format!("fixture/{id}")).execute(pool).await.unwrap();
    id
}
async fn observed(
    pool: &PgPool,
    lease: &RunLease,
    outcome: NativeOutcome,
    manifest: Option<Id>,
) -> TerminalObservation {
    TerminalObservation {
        schema_version: SchemaV1,
        external_job_id: lease.external_job_id.clone(),
        outcome,
        manifest_artifact_id: manifest,
        failure_class: if outcome == NativeOutcome::Failed {
            Some(FailureClass::RetryableInfra)
        } else {
            None
        },
        failure_code: (outcome == NativeOutcome::Failed).then(|| "UPSTREAM_FAILURE".into()),
        observed_at: sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(pool)
            .await
            .unwrap(),
    }
}
async fn usage(pool: &PgPool, cycle: Id) -> (i64, i64, i64) {
    sqlx::query_as("SELECT reserved_experiments::bigint,used_experiments::bigint,reserved_cpu_seconds::bigint FROM app.research_cycles WHERE id=$1").bind(cycle.as_uuid()).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_admission_is_atomic_bounded_and_idempotent(pool: PgPool) {
    let (store, f, request, actor) = setup(&pool).await;
    let (a, b) = tokio::join!(
        store.enqueue_run("same", &request),
        store.enqueue_run("same", &request)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource, b.resource);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(usage(&pool, f.cycle).await, (1, 0, 100));
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.run_admissions),(SELECT count(*) FROM pgmq.q_runs)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1));
    let mut different = request.clone();
    different.limits.experiments = 2;
    assert!(matches!(
        store.enqueue_run("same", &different).await,
        Err(StoreError::IdempotencyConflict)
    ));
    let (second, third) = tokio::join!(
        store.enqueue_run("second", &request),
        store.enqueue_run("third", &request)
    );
    assert_ne!(second.is_ok(), third.is_ok());
    assert!(matches!(
        second.err().or_else(|| third.err()),
        Some(StoreError::Domain(DomainError::BudgetExhausted(
            "parallel_runs"
        )))
    ));
    assert_eq!(usage(&pool, f.cycle).await, (2, 0, 200));
    let batch = store
        .run_events(&actor, a.resource.id, DbCounter::ZERO, 50)
        .await
        .unwrap();
    assert_eq!(batch.events.len(), 1);
    assert_eq!(batch.events[0].seq.get(), 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn queue_failure_rolls_back_every_admission_fact(pool: PgPool) {
    let (store, f, request, _) = setup(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION public.reject_enqueue() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected queue failure'; END $$; CREATE TRIGGER fail_enqueue BEFORE INSERT ON pgmq.q_runs FOR EACH ROW EXECUTE FUNCTION public.reject_enqueue();").execute(&pool).await.unwrap();
    assert!(matches!(
        store.enqueue_run("fail", &request).await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(usage(&pool, f.cycle).await, (0, 0, 0));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.run_admissions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.runs WHERE id<>$1")
        .bind(f.run.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER fail_enqueue ON pgmq.q_runs")
        .execute(&pool)
        .await
        .unwrap();
    assert!(!store.enqueue_run("fail", &request).await.unwrap().replayed);
}

#[sqlx::test(migrations = "../../migrations")]
async fn reject_mutable_inputs_wrong_capabilities_and_budget_excess(pool: PgPool) {
    let (store, f, request, _) = setup(&pool).await;
    let input = Id::new();
    sqlx::query("INSERT INTO app.input_sets(id,project_id,purpose,decision_cutoff) VALUES($1,$2,'DISCOVERY',clock_timestamp())").bind(input.as_uuid()).bind(f.project.as_uuid()).execute(&pool).await.unwrap();
    let mut invalid = request.clone();
    invalid.input_set_id = input;
    assert!(matches!(
        store.enqueue_run("draft", &invalid).await,
        Err(StoreError::Invalid("frozen_inputs_required"))
    ));
    invalid = request.clone();
    invalid.kind = RunKind::PortfolioBuild;
    assert!(matches!(
        store.enqueue_run("kind", &invalid).await,
        Err(StoreError::Domain(DomainError::CapabilityUnavailable(_)))
    ));
    invalid = request.clone();
    invalid.runtime_revision = Revision::INITIAL.next().unwrap();
    assert!(store.enqueue_run("stale", &invalid).await.is_err());
    invalid = request.clone();
    invalid.limits.experiments = 21;
    assert!(matches!(
        store.enqueue_run("budget", &invalid).await,
        Err(StoreError::Domain(DomainError::BudgetExhausted(
            "experiments"
        )))
    ));
    assert_eq!(usage(&pool, f.cycle).await, (0, 0, 0));
    sqlx::query("UPDATE app.projects SET state='PAUSED' WHERE id=$1")
        .bind(f.project.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.enqueue_run("paused", &request).await,
        Err(StoreError::Domain(DomainError::AdmissionClosed))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn redelivery_never_steals_live_lease_or_grants_second_dispatch(pool: PgPool) {
    let (store, _, request, _) = setup(&pool).await;
    let run = store.enqueue_run("run", &request).await.unwrap().resource;
    let m = message(&store, run.id).await;
    let one = leased(&store, &m, "one").await;
    assert!(matches!(
        store.claim_run(&m, "two", 60).await.unwrap(),
        ClaimResult::Busy
    ));
    assert!(store.begin_run_dispatch(run.id, &one.fence).await.unwrap());
    assert!(!store.begin_run_dispatch(run.id, &one.fence).await.unwrap());
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(one.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    let two = leased(&store, &m, "two").await;
    assert_eq!(two.fence.attempt_id, one.fence.attempt_id);
    assert_eq!(two.attempt_no, 1);
    assert_eq!(two.external_job_id, one.external_job_id);
    assert_eq!(two.fence.owner_epoch.get(), 2);
    assert_eq!(two.action, NextRuntimeAction::Reconcile);
    assert!(matches!(
        store.begin_run_dispatch(run.id, &one.fence).await,
        Err(StoreError::Domain(DomainError::StaleAttempt))
    ));
    assert!(!store.begin_run_dispatch(run.id, &two.fence).await.unwrap());
    assert_eq!(
        store
            .observe_run_running(run.id, &two.fence, &two.external_job_id)
            .await
            .unwrap()
            .state,
        RunState::Running
    );
    assert!(matches!(
        store.renew_run_lease(run.id, &one.fence, 30).await,
        Err(StoreError::Domain(DomainError::StaleAttempt))
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn terminal_receipt_survives_lost_ack_without_double_consumption(pool: PgPool) {
    let (store, f, request, actor) = setup(&pool).await;
    let run = store.enqueue_run("run", &request).await.unwrap().resource;
    let m = message(&store, run.id).await;
    let lease = leased(&store, &m, "worker").await;
    assert!(store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap());
    let report = manifest(&pool, &lease).await;
    let obs = observed(&pool, &lease, NativeOutcome::Succeeded, Some(report)).await;
    let (one, two) = tokio::join!(
        store.accept_run_terminal(run.id, &lease.fence, &obs),
        store.accept_run_terminal(run.id, &lease.fence, &obs)
    );
    let (one, two) = (one.unwrap(), two.unwrap());
    assert_eq!(one.resource, two.resource);
    assert_ne!(one.replayed, two.replayed);
    assert_eq!(one.resource.state, RunState::Succeeded);
    assert_eq!(usage(&pool, f.cycle).await, (0, 1, 100));
    // A committed result remains identifiable after a worker lease ends.
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    assert!(
        store
            .accept_run_terminal(run.id, &lease.fence, &obs)
            .await
            .unwrap()
            .replayed
    );
    let mut conflict = obs.clone();
    conflict.observed_at -= Duration::microseconds(1);
    assert!(matches!(
        store
            .accept_run_terminal(run.id, &lease.fence, &conflict)
            .await,
        Err(StoreError::Conflict)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.q_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    store.acknowledge_run(&m).await.unwrap();
    store.acknowledge_run(&m).await.unwrap();
    assert_eq!(usage(&pool, f.cycle).await, (0, 1, 100));
    assert_eq!(
        store.get_run(&actor, run.id).await.unwrap().state,
        RunState::Succeeded
    );
    let err = sqlx::query("UPDATE app.runs SET state='RUNNING',finished_at=NULL WHERE id=$1")
        .bind(run.id.as_uuid())
        .execute(&pool)
        .await
        .unwrap_err();
    support::sqlstate(err, "23000");
}

#[sqlx::test(migrations = "../../migrations")]
async fn unknown_job_cannot_ack_or_borrow_manifest_from_other_attempt(pool: PgPool) {
    let (store, f, request, _) = setup(&pool).await;
    let run = store.enqueue_run("one", &request).await.unwrap().resource;
    let m = message(&store, run.id).await;
    let one = leased(&store, &m, "worker").await;
    assert!(matches!(
        store.acknowledge_run(&m).await,
        Err(StoreError::Conflict)
    ));
    assert!(store.begin_run_dispatch(run.id, &one.fence).await.unwrap());
    let invalid = observed(&pool, &one, NativeOutcome::Succeeded, Some(f.report)).await;
    assert!(matches!(
        store
            .accept_run_terminal(run.id, &one.fence, &invalid)
            .await,
        Err(StoreError::Invalid("manifest_exact_producer"))
    ));
    let missing = observed(&pool, &one, NativeOutcome::Succeeded, None).await;
    assert!(matches!(
        store
            .accept_run_terminal(run.id, &one.fence, &missing)
            .await,
        Err(StoreError::Invalid("manifest_required"))
    ));
    let unknown = observed(&pool, &one, NativeOutcome::ConfirmedAbsent, None).await;
    assert!(matches!(
        store
            .accept_run_terminal(run.id, &one.fence, &unknown)
            .await,
        Err(StoreError::Domain(DomainError::InvalidTransition))
    ));
    assert_eq!(usage(&pool, f.cycle).await, (1, 0, 100));
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancellation_before_dispatch_is_atomic_idempotent_and_auditable(pool: PgPool) {
    let (store, f, request, actor) = setup(&pool).await;
    let run = store.enqueue_run("one", &request).await.unwrap().resource;
    let m = message(&store, run.id).await;
    let cancel = RunCancelV1 {
        schema_version: SchemaV1,
        expected_revision: run.revision,
    };
    let (a, b) = tokio::join!(
        store.cancel_run(&actor, "cancel", run.id, &cancel),
        store.cancel_run(&actor, "cancel", run.id, &cancel)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.state, RunState::Cancelled);
    assert_eq!(a.resource, b.resource);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(usage(&pool, f.cycle).await, (0, 1, 100));
    assert!(
        matches!(store.claim_run(&m,"worker",60).await.unwrap(),ClaimResult::Terminal(r) if r.state==RunState::Cancelled)
    );
    assert!(store.enqueue_run("one", &request).await.unwrap().replayed);
    store.acknowledge_run(&m).await.unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.run_attempts WHERE run_id=$1")
        .bind(run.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let events = store
        .run_events(&actor, run.id, DbCounter::ZERO, 100)
        .await
        .unwrap();
    assert_eq!(events.events.len(), 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_intent_requires_real_termination_and_preserves_native_failure(pool: PgPool) {
    let (store, f, request, actor) = setup(&pool).await;
    for (key, outcome, expected) in [
        ("failure", NativeOutcome::Failed, RunState::Failed),
        ("success", NativeOutcome::Succeeded, RunState::Cancelled),
    ] {
        let run = store.enqueue_run(key, &request).await.unwrap().resource;
        let m = message(&store, run.id).await;
        let lease = leased(&store, &m, "worker").await;
        store
            .begin_run_dispatch(run.id, &lease.fence)
            .await
            .unwrap();
        let current = store.get_run(&actor, run.id).await.unwrap();
        let cancel = store
            .cancel_run(
                &actor,
                key,
                run.id,
                &RunCancelV1 {
                    schema_version: SchemaV1,
                    expected_revision: current.revision,
                },
            )
            .await
            .unwrap();
        assert_eq!(cancel.resource.state, RunState::CancelRequested);
        assert!(cancel.resource.finished_at.is_none());
        assert!(store.acknowledge_run(&m).await.is_err());
        let report = if outcome == NativeOutcome::Succeeded {
            Some(manifest(&pool, &lease).await)
        } else {
            None
        };
        let obs = observed(&pool, &lease, outcome, report).await;
        assert_eq!(
            store
                .accept_run_terminal(run.id, &lease.fence, &obs)
                .await
                .unwrap()
                .resource
                .state,
            expected
        );
    }
    assert_eq!(usage(&pool, f.cycle).await, (0, 2, 200));
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancel_completion_race_has_one_terminal_and_one_trial_charge(pool: PgPool) {
    let (store, f, request, actor) = setup(&pool).await;
    let run = store.enqueue_run("race", &request).await.unwrap().resource;
    let m = message(&store, run.id).await;
    let lease = leased(&store, &m, "worker").await;
    store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap();
    let current = store.get_run(&actor, run.id).await.unwrap();
    let report = manifest(&pool, &lease).await;
    let obs = observed(&pool, &lease, NativeOutcome::Succeeded, Some(report)).await;
    let request = RunCancelV1 {
        schema_version: SchemaV1,
        expected_revision: current.revision,
    };
    let (cancel, complete) = tokio::join!(
        store.cancel_run(&actor, "race", run.id, &request),
        store.accept_run_terminal(run.id, &lease.fence, &obs)
    );
    let complete = complete.unwrap();
    if cancel.is_ok() {
        assert_eq!(complete.resource.state, RunState::Cancelled);
    } else {
        assert_eq!(complete.resource.state, RunState::Succeeded);
    }
    assert_eq!(usage(&pool, f.cycle).await, (0, 1, 100));
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM app.run_terminal_receipts WHERE run_id=$1")
            .bind(run.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn event_pages_resume_without_gaps_and_reject_foreign_machine_scope(pool: PgPool) {
    let (store, f, request, actor) = setup(&pool).await;
    let run = store
        .enqueue_run("events", &request)
        .await
        .unwrap()
        .resource;
    let m = message(&store, run.id).await;
    let lease = leased(&store, &m, "worker").await;
    store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap();
    store
        .observe_run_running(run.id, &lease.fence, &lease.external_job_id)
        .await
        .unwrap();
    let mut after = DbCounter::ZERO;
    let mut found = Vec::new();
    loop {
        let batch = store.run_events(&actor, run.id, after, 1).await.unwrap();
        if batch.events.is_empty() {
            break;
        }
        after = batch.events[0].seq;
        found.push(after.get());
    }
    assert_eq!(found, vec![1, 2, 3]);
    assert!(matches!(
        store
            .run_events(&actor, run.id, DbCounter::new(4).unwrap(), 10)
            .await,
        Err(StoreError::EventCursorExpired)
    ));
    let other = support::fixture(&pool, support::budget()).await;
    let p = store
        .create_principal(
            &actor,
            "principal",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "scoped CLI".into(),
                kind: AssignablePrincipalKind::Cli,
                project_id: Some(other.project),
                downstream_id: None,
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let c = fixture_credential(
        &store,
        &actor,
        "credential",
        p.id,
        &CredentialIssue {
            schema_version: SchemaV1,
            scope_codes: vec![MachineScope::RunRead, MachineScope::RunCancel],
            expires_at: Utc::now() + Duration::hours(1),
        },
        Id::new(),
        Id::new(),
    )
    .await
    .unwrap()
    .resource;
    let machine = store
        .machine_challenge(c.public_token_id)
        .await
        .unwrap()
        .verified_actor(None);
    assert!(matches!(
        store.get_run(&machine, run.id).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store
            .run_events(&machine, run.id, DbCounter::ZERO, 10)
            .await,
        Err(StoreError::NotFound)
    ));
    let list = store
        .list_runs(&machine, &RunListQuery::default())
        .await
        .unwrap();
    assert!(list.items.iter().all(|r| r.project_id == other.project));
    assert!(matches!(
        store
            .cancel_run(
                &machine,
                "forbidden",
                run.id,
                &RunCancelV1 {
                    schema_version: SchemaV1,
                    expected_revision: store.get_run(&actor, run.id).await.unwrap().revision
                }
            )
            .await,
        Err(StoreError::NotFound)
    ));
    assert_eq!(usage(&pool, f.cycle).await, (1, 0, 100));
}

#[sqlx::test(migrations = "../../migrations")]
async fn changed_runtime_does_not_redirect_an_already_dispatched_job(pool: PgPool) {
    let (store, _, request, _) = setup(&pool).await;
    let run = store.enqueue_run("one", &request).await.unwrap().resource;
    let m = message(&store, run.id).await;
    let one = leased(&store, &m, "one").await;
    store.begin_run_dispatch(run.id, &one.fence).await.unwrap();
    sqlx::query("UPDATE app.runtime_integrations SET endpoint='https://another.example',credential_ref='other-secret' WHERE id=$1").bind(request.runtime_id.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1").bind(one.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
    let two = leased(&store, &m, "two").await;
    assert_eq!(two.runtime.endpoint, "https://runtime.example");
    assert_eq!(two.runtime.credential_ref, "fixture-credential");
    assert_eq!(two.action, NextRuntimeAction::Reconcile);
    assert!(!store.begin_run_dispatch(run.id, &two.fence).await.unwrap());
}

async fn fixture_credential(
    store: &Store,
    actor: &Actor,
    key: &str,
    principal: Id,
    request: &CredentialIssue,
    public: Id,
    verifier: Id,
) -> Result<CommandResult<CredentialView>, StoreError> {
    match store
        .prepare_credential_issuance(actor, key, principal, request)
        .await?
    {
        store::control::CredentialPreparation::Replay(result) => Ok(result),
        store::control::CredentialPreparation::New(prepared) => {
            prepared.publish(public, verifier).await
        }
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn first_dispatch_rechecks_lease_after_real_runtime_lock_wait(pool: PgPool) {
    let (store, _, request, _) = setup(&pool).await;
    let run = store.enqueue_run("wait", &request).await.unwrap().resource;
    let msg = message(&store, run.id).await;
    let ClaimResult::Leased(lease) = store.claim_run(&msg, "short-lived", 1).await.unwrap() else {
        panic!("lease")
    };
    let mut blocker = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM app.runtime_integrations WHERE id=$1 FOR UPDATE")
        .bind(request.runtime_id.as_uuid())
        .fetch_one(&mut *blocker)
        .await
        .unwrap();
    let task = {
        let store = store.clone();
        let fence = lease.fence.clone();
        tokio::spawn(async move { store.begin_run_dispatch(run.id, &fence).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(3),async {
        loop {let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT enabled,revision%')").fetch_one(&pool).await.unwrap();if waiting{break}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
    }).await.unwrap();
    sqlx::query(
        "SELECT pg_sleep(GREATEST(0,EXTRACT(EPOCH FROM $1::timestamptz-clock_timestamp()))+0.05)",
    )
    .bind(lease.lease_expires_at)
    .execute(&mut *blocker)
    .await
    .unwrap();
    blocker.commit().await.unwrap();
    assert!(matches!(
        task.await.unwrap(),
        Err(StoreError::Domain(DomainError::StaleAttempt))
    ));
    let state: String =
        sqlx::query_scalar("SELECT dispatch_state FROM app.run_attempts WHERE id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "NOT_SENT");
}

#[sqlx::test(migrations = "../../migrations")]
async fn renewal_returns_actual_nonshrinking_expiry_and_event_failure_is_atomic(pool: PgPool) {
    let (store, f, request, _) = setup(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION app.inject_event_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'event unavailable'; END $$; CREATE TRIGGER inject_event_failure BEFORE INSERT ON app.run_events FOR EACH ROW EXECUTE FUNCTION app.inject_event_failure();").execute(&pool).await.unwrap();
    assert!(store.enqueue_run("event-failed", &request).await.is_err());
    assert_eq!(usage(&pool, f.cycle).await, (0, 0, 0));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.run_admissions")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    sqlx::query("DROP TRIGGER inject_event_failure ON app.run_events")
        .execute(&pool)
        .await
        .unwrap();
    let run = store
        .enqueue_run("event-failed", &request)
        .await
        .unwrap()
        .resource;
    let msg = message(&store, run.id).await;
    let lease = leased(&store, &msg, "worker").await;
    let renewed = store
        .renew_run_lease(run.id, &lease.fence, 1)
        .await
        .unwrap();
    assert_eq!(renewed, lease.lease_expires_at);
    let persisted: chrono::DateTime<Utc> = sqlx::query_scalar(
        "SELECT lease_expires_at::timestamptz FROM app.run_attempts WHERE id=$1",
    )
    .bind(lease.fence.attempt_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(renewed, persisted);
}

#[sqlx::test(migrations = "../../migrations")]
async fn false_terminal_receipts_and_unstructured_failure_reasons_are_rejected(pool: PgPool) {
    let (store, f, request, actor) = setup(&pool).await;
    let run = store
        .enqueue_run("false-receipt", &request)
        .await
        .unwrap()
        .resource;
    let error=sqlx::query("INSERT INTO app.run_terminal_receipts(run_id,observation,terminal_state,result_snapshot) VALUES($1,'{\"schema_version\":1}','SUCCEEDED',$2)").bind(run.id.as_uuid()).bind(serde_json::to_value(&run).unwrap()).execute(&pool).await.unwrap_err();
    support::sqlstate(error, "23514");
    let msg = message(&store, run.id).await;
    assert!(store.acknowledge_run(&msg).await.is_err());
    let lease = leased(&store, &msg, "worker").await;
    store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap();
    let valid = observed(&pool, &lease, NativeOutcome::Failed, None).await;
    for code in [
        None,
        Some(String::new()),
        Some("unsafe\nsecret".into()),
        Some("A".repeat(65)),
    ] {
        let mut observation = valid.clone();
        observation.failure_code = code;
        assert!(store
            .accept_run_terminal(run.id, &lease.fence, &observation)
            .await
            .is_err());
    }
    assert_eq!(usage(&pool, f.cycle).await, (1, 0, 100));
    let result = store
        .accept_run_terminal(run.id, &lease.fence, &valid)
        .await
        .unwrap();
    assert_eq!(result.resource.state, RunState::Failed);
    let error_code: String =
        sqlx::query_scalar("SELECT error_code FROM app.run_attempts WHERE id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(error_code, "UPSTREAM_FAILURE");
    let current = store.get_run(&actor, run.id).await.unwrap();
    assert_eq!(current.state, RunState::Failed);
}
