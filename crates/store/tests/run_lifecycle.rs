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
    // Expire a real short lease after adoption, never mutate terminal history.
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()+interval '1 second' WHERE id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
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
    sqlx::query("SELECT pg_sleep(GREATEST(0,EXTRACT(EPOCH FROM lease_expires_at-clock_timestamp()))+0.02) FROM app.run_attempts WHERE id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).execute(&pool).await.unwrap();
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
        loop {let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT id,enabled FROM app.runtime_integrations%')").fetch_one(&pool).await.unwrap();if waiting{break}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
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

#[sqlx::test(migrations = "../../migrations")]
async fn review_expired_unsent_attempt_is_finalized_without_releasing_another_lease(pool: PgPool) {
    let (store, f, mut request, actor) = setup(&pool).await;
    request.limits.wall_seconds = 1;
    let run = store
        .enqueue_run("overdue", &request)
        .await
        .unwrap()
        .resource;
    let m = message(&store, run.id).await;
    let ClaimResult::Leased(lease) = store.claim_run(&m, "original", 1).await.unwrap() else {
        panic!("lease")
    };
    sqlx::query(
        "SELECT pg_sleep(GREATEST(0,EXTRACT(EPOCH FROM $1::timestamptz-clock_timestamp()))+0.02)",
    )
    .bind(lease.lease_expires_at.max(run.deadline_at))
    .execute(&pool)
    .await
    .unwrap();
    let ClaimResult::Terminal(ended) = store.claim_run(&m, "recovery", 60).await.unwrap() else {
        panic!("an expired NOT_SENT attempt must terminate instead of receiving a fresh lease")
    };
    assert_eq!(ended.state, RunState::Failed);
    assert_eq!(
        ended.terminal_reason_code.as_deref(),
        Some("DEADLINE_EXCEEDED")
    );
    let attempt: (String, i64, String, String) = sqlx::query_as("SELECT worker_owner_id,owner_epoch::bigint,dispatch_state,runtime_state FROM app.run_attempts WHERE id=$1")
        .bind(lease.fence.attempt_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        attempt,
        ("original".into(), 1, "TERMINAL".into(), "UNKNOWN".into())
    );
    assert_eq!(usage(&pool, f.cycle).await, (0, 1, 100));
    assert!(matches!(
        store.claim_run(&m, "again", 60).await.unwrap(),
        ClaimResult::Terminal(_)
    ));
    store.acknowledge_run(&m).await.unwrap();
    store.acknowledge_run(&m).await.unwrap();
    assert_eq!(usage(&pool, f.cycle).await, (0, 1, 100));
    assert_eq!(store.get_run(&actor, run.id).await.unwrap(), ended);
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_terminal_attempt_without_manifest_cannot_be_rewritten(pool: PgPool) {
    let (store, _, request, _) = setup(&pool).await;
    let run = store
        .enqueue_run("failed", &request)
        .await
        .unwrap()
        .resource;
    let m = message(&store, run.id).await;
    let lease = leased(&store, &m, "worker").await;
    store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap();
    let obs = observed(&pool, &lease, NativeOutcome::Failed, None).await;
    store
        .accept_run_terminal(run.id, &lease.fence, &obs)
        .await
        .unwrap();
    for change in [
        "worker_owner_id='different'",
        "owner_epoch=owner_epoch+1",
        "lease_expires_at=clock_timestamp()+interval '1 hour'",
        "external_job_id='different'",
        "dispatch_state='SENT_UNKNOWN'",
        "runtime_state='RUNNING'",
        "error_code='DIFFERENT'",
    ] {
        let mut tx = pool.begin().await.unwrap();
        let result = sqlx::query(&format!("UPDATE app.run_attempts SET {change} WHERE id=$1"))
            .bind(lease.fence.attempt_id.as_uuid())
            .execute(&mut *tx)
            .await;
        tx.rollback().await.unwrap();
        assert!(result.is_err(), "terminal attempt accepted: {change}");
    }
    let replay = store
        .accept_run_terminal(run.id, &lease.fence, &obs)
        .await
        .unwrap();
    assert!(replay.replayed);
    store.acknowledge_run(&m).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_success_is_not_an_attempt_error(pool: PgPool) {
    let (store, _, request, _) = setup(&pool).await;
    let run = store
        .enqueue_run("success", &request)
        .await
        .unwrap()
        .resource;
    let m = message(&store, run.id).await;
    let lease = leased(&store, &m, "worker").await;
    store
        .begin_run_dispatch(run.id, &lease.fence)
        .await
        .unwrap();
    let report = manifest(&pool, &lease).await;
    let obs = observed(&pool, &lease, NativeOutcome::Succeeded, Some(report)).await;
    let ended = store
        .accept_run_terminal(run.id, &lease.fence, &obs)
        .await
        .unwrap();
    assert_eq!(ended.resource.state, RunState::Succeeded);
    let error: (Option<String>, Option<String>) =
        sqlx::query_as("SELECT error_class,error_code FROM app.run_attempts WHERE id=$1")
            .bind(lease.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(error, (None, None));
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_compatible_unknown_event_retains_its_envelope_and_cursor(pool: PgPool) {
    let (store, _, request, actor) = setup(&pool).await;
    let run = store
        .enqueue_run("events-extension", &request)
        .await
        .unwrap()
        .resource;
    let payload = serde_json::json!({"schema_version":1,"completed":"2","unit":"observations"});
    sqlx::query("INSERT INTO app.run_events(run_id,seq,event_type,schema_version,payload,occurred_at) VALUES($1,2,'run.observations_processed',1,$2,clock_timestamp())")
        .bind(run.id.as_uuid()).bind(&payload).execute(&pool).await.unwrap();
    let current = store.get_run(&actor, run.id).await.unwrap();
    store
        .cancel_run(
            &actor,
            "cancel-extension",
            run.id,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: current.revision,
            },
        )
        .await
        .unwrap();
    let batch = store
        .run_events(&actor, run.id, DbCounter::ZERO, 100)
        .await
        .unwrap();
    let wire = serde_json::to_value(&batch).unwrap();
    assert_eq!(batch.events.len(), 3);
    assert_eq!(
        wire["events"][1]["event_type"],
        "run.observations_processed"
    );
    assert_eq!(wire["events"][1]["payload"], payload);
    assert_eq!(batch.events[2].seq.get(), 3);
    let resumed = store
        .run_events(&actor, run.id, DbCounter::new(2).unwrap(), 100)
        .await
        .unwrap();
    assert_eq!(resumed.events.len(), 1);
    assert_eq!(resumed.events[0].seq.get(), 3);
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_browser_cancel_requires_recent_authentication_but_reads_do_not(pool: PgPool) {
    let (store, _, request, _) = setup(&pool).await;
    let id = Id::new();
    sqlx::query("INSERT INTO app.browser_logins(id,auth_epoch,authenticated_at,expires_at) SELECT $1,session_epoch,clock_timestamp()-interval '10 minutes',clock_timestamp()+interval '1 hour' FROM app.operator_auth_state")
        .bind(id.as_uuid()).execute(&pool).await.unwrap();
    let stale = Actor::Browser { login_id: id };
    let run = store
        .enqueue_run("recent", &request)
        .await
        .unwrap()
        .resource;
    assert!(store.get_run(&stale, run.id).await.is_ok());
    assert!(matches!(
        store
            .cancel_run(
                &stale,
                "recent",
                run.id,
                &RunCancelV1 {
                    schema_version: SchemaV1,
                    expected_revision: run.revision
                }
            )
            .await,
        Err(StoreError::RecentAuthenticationRequired)
    ));
    assert_eq!(
        store.get_run(&stale, run.id).await.unwrap().state,
        RunState::Queued
    );
}

async fn licensed_inputs(
    pool: &PgPool,
    store: &Store,
    f: &support::Fixture,
    actor: &Actor,
    runtime: Id,
    until: Option<chrono::DateTime<Utc>>,
) -> (Id, Id, Id) {
    use contracts::research::*;
    let source = Id::new();
    let grant = Id::new();
    let dataset = Id::new();
    sqlx::query("INSERT INTO app.data_sources(id,name,runtime_id,native_catalog_ref,provider_kind,enabled) VALUES($1,'grant fixture',$2,$3,'fixture',true)")
        .bind(source.as_uuid()).bind(runtime.as_uuid()).bind(format!("fixture/{source}")).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.data_use_grants(id,source_id,version,license_reference,evidence_artifact_id,allowed_uses,valid_from,valid_until,authorized_by) VALUES($1,$2,1,'test-only reference',$3,'RESEARCH',clock_timestamp()-interval '1 day',$4,'OPERATOR')")
        .bind(grant.as_uuid()).bind(source.as_uuid()).bind(f.artifact.as_uuid()).bind(until).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.dataset_revisions(id,source_id,data_use_grant_id,native_snapshot_ref,native_storage_version,universe_version_id,schema_version,data_kind,partition_role,event_start,event_end,available_through,row_count,timezone,quality_artifact_id,pit_status,revision_policy,origin) SELECT $1,$2,$3,$4,'1',b.universe_version_id,'1','BAR','DISCOVERY','2010-01-01','2020-01-01','2020-01-02',1000,'UTC',$5,'UNVERIFIED','AS_KNOWN_THEN','FIXTURE' FROM app.research_briefs b WHERE b.project_id=$6 LIMIT 1")
        .bind(dataset.as_uuid()).bind(source.as_uuid()).bind(grant.as_uuid()).bind(format!("fixture/{dataset}"))
        .bind(f.artifact.as_uuid()).bind(f.project.as_uuid()).execute(pool).await.unwrap();
    let cutoff: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let input = store
        .create_input_set(
            actor,
            &Id::new().to_string(),
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.project,
                purpose: InputPurpose::Discovery,
                decision_cutoff: cutoff,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset,
                    role: DataPartition::Discovery,
                }],
            },
        )
        .await
        .unwrap()
        .resource
        .header
        .id;
    (input, grant, source)
}
async fn revoke_data(pool: &PgPool, grant: Id) {
    sqlx::query("INSERT INTO app.data_use_revocations(grant_id,reason_code,reason,effective_at) VALUES($1,'TEST_WITHDRAWAL','test withdrawal',clock_timestamp())")
        .bind(grant.as_uuid()).execute(pool).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_frozen_data_does_not_authorize_new_admission_after_revocation(pool: PgPool) {
    let (store, f, mut request, actor) = setup(&pool).await;
    let (input, grant, _) =
        licensed_inputs(&pool, &store, &f, &actor, request.runtime_id, None).await;
    request.input_set_id = input;
    let original = store.enqueue_run("before", &request).await.unwrap();
    revoke_data(&pool, grant).await;
    assert!(store.enqueue_run("after", &request).await.is_err());
    assert_eq!(usage(&pool, f.cycle).await, (1, 0, 100));
    let counts: (i64, i64) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM app.run_admissions),(SELECT count(*) FROM pgmq.q_runs)",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(counts, (1, 1));
    let replay = store.enqueue_run("before", &request).await.unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, original.resource);
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_first_dispatch_rechecks_grants_but_unknown_outcomes_remain_reconcilable(
    pool: PgPool,
) {
    let (store, f, mut request, actor) = setup(&pool).await;
    let (input, grant, _) =
        licensed_inputs(&pool, &store, &f, &actor, request.runtime_id, None).await;
    request.input_set_id = input;
    let sent = store.enqueue_run("sent", &request).await.unwrap().resource;
    let unsent = store
        .enqueue_run("unsent", &request)
        .await
        .unwrap()
        .resource;
    // One PGMQ read leases the complete batch. Do not read again for the second
    // message while the first read's visibility timeout still owns it.
    let messages = store.read_run_messages(30, 100).await.unwrap();
    let sent_message = messages.iter().find(|m| m.run_id == sent.id).unwrap();
    let unsent_message = messages.iter().find(|m| m.run_id == unsent.id).unwrap();
    let sent_lease = leased(&store, sent_message, "sent").await;
    let unsent_lease = leased(&store, unsent_message, "unsent").await;
    assert!(store
        .begin_run_dispatch(sent.id, &sent_lease.fence)
        .await
        .unwrap());
    revoke_data(&pool, grant).await;
    assert!(store
        .begin_run_dispatch(unsent.id, &unsent_lease.fence)
        .await
        .is_err());
    let state: String =
        sqlx::query_scalar("SELECT dispatch_state FROM app.run_attempts WHERE id=$1")
            .bind(unsent_lease.fence.attempt_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(state, "NOT_SENT");
    assert!(!store
        .begin_run_dispatch(sent.id, &sent_lease.fence)
        .await
        .unwrap());
    let failed = observed(&pool, &sent_lease, NativeOutcome::Failed, None).await;
    assert_eq!(
        store
            .accept_run_terminal(sent.id, &sent_lease.fence, &failed)
            .await
            .unwrap()
            .resource
            .state,
        RunState::Failed
    );
    assert_eq!(usage(&pool, f.cycle).await, (1, 1, 200));
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_grant_revocation_wait_is_rechecked_before_queue_commit(pool: PgPool) {
    let (store, f, mut request, actor) = setup(&pool).await;
    let (input, grant, _) =
        licensed_inputs(&pool, &store, &f, &actor, request.runtime_id, None).await;
    request.input_set_id = input;
    let mut revoke = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.data_use_revocations(grant_id,reason_code,reason,effective_at) VALUES($1,'TEST_WITHDRAWAL','concurrent withdrawal',clock_timestamp())")
        .bind(grant.as_uuid()).execute(&mut *revoke).await.unwrap();
    let task = {
        let store = store.clone();
        tokio::spawn(async move { store.enqueue_run("wait-revoke", &request).await })
    };
    tokio::time::timeout(std::time::Duration::from_secs(5),async{
        loop{let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'SELECT id,source_id,valid_from,valid_until,allowed_uses%')").fetch_one(&pool).await.unwrap();
            if waiting{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
    }).await.expect("must observe real grant lock wait");
    revoke.commit().await.unwrap();
    assert!(task.await.unwrap().is_err());
    assert_eq!(usage(&pool, f.cycle).await, (0, 0, 0));
    let queued: i64 = sqlx::query_scalar("SELECT count(*) FROM pgmq.q_runs")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(queued, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn review_standalone_work_uses_no_cycle_and_cannot_hide_research_trials(pool: PgPool) {
    let (store, f, request, actor) = setup(&pool).await;
    let mut limits = request.limits.clone();
    limits.experiments = 0;
    let admin = StandaloneRunSubmission {
        project_id: f.project,
        input_set_id: request.input_set_id,
        runtime_id: request.runtime_id,
        runtime_revision: request.runtime_revision,
        kind: RunKind::DataValidate,
        limits,
        max_parallel_runs: 1,
    };
    let (a, b) = tokio::join!(
        store.enqueue_standalone_run("admin", &admin),
        store.enqueue_standalone_run("admin", &admin)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource, b.resource);
    assert_eq!(a.resource.cycle_id, None);
    assert!(store.enqueue_standalone_run("full", &admin).await.is_err());
    let mut invalid = admin.clone();
    invalid.kind = RunKind::AgentResearch;
    assert!(store
        .enqueue_standalone_run("research", &invalid)
        .await
        .is_err());
    invalid = admin.clone();
    invalid.limits.experiments = 1;
    assert!(store
        .enqueue_standalone_run("hidden", &invalid)
        .await
        .is_err());
    let msg = message(&store, a.resource.id).await;
    let lease = leased(&store, &msg, "management").await;
    assert!(store
        .begin_run_dispatch(a.resource.id, &lease.fence)
        .await
        .unwrap());
    let report = manifest(&pool, &lease).await;
    let obs = observed(&pool, &lease, NativeOutcome::Succeeded, Some(report)).await;
    let terminal = store
        .accept_run_terminal(a.resource.id, &lease.fence, &obs)
        .await
        .unwrap();
    assert_eq!(terminal.resource.state, RunState::Succeeded);
    store.acknowledge_run(&msg).await.unwrap();
    store.acknowledge_run(&msg).await.unwrap();
    assert_eq!(usage(&pool, f.cycle).await, (0, 0, 0));
    assert!(store
        .get_run(&actor, a.resource.id)
        .await
        .unwrap()
        .cycle_id
        .is_none());
    let next = store
        .enqueue_standalone_run("next", &admin)
        .await
        .unwrap()
        .resource;
    store
        .cancel_run(
            &actor,
            "cancel-admin",
            next.id,
            &RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: next.revision,
            },
        )
        .await
        .unwrap();
    assert_eq!(usage(&pool, f.cycle).await, (0, 0, 0));
}
