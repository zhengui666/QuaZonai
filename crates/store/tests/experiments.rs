//! Real PostgreSQL proposal transactions; relational fixtures are not T42 acceptance.
#[path = "../../../tests/support/brief.rs"]
mod brief_support;
#[path = "../../../tests/support/experiments.rs"]
mod experiment_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
use contracts::{
    artifacts::ResearchArtifactKind,
    experiments::{ExperimentOutcome, ExperimentResultVisibility, ExperimentSource},
    research::ResearchListQuery,
    Id,
};
use experiment_support::{artifact, machine, mission, setup};
use sqlx::{PgPool, Row};
use store::{authority::Actor, StoreError};

async fn counts(pool: &PgPool, cycle: Id) -> (i64, i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM app.experiments WHERE cycle_id=$1),(SELECT count(*) FROM app.experiment_authorship a JOIN app.experiments e ON e.id=a.experiment_id WHERE e.cycle_id=$1),(SELECT count(*) FROM app.command_receipts c JOIN app.experiments e ON e.id=c.resource_id WHERE c.operation='EXPERIMENT_PROPOSE' AND e.cycle_id=$1)")
        .bind(cycle.as_uuid()).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn exact_replay_keeps_one_original_proposal_author_and_receipt(pool: PgPool) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 3).await;
    let (a, b) = tokio::join!(
        store.propose_experiment(&operator, "same", &f.request),
        store.propose_experiment(&operator, "same", &f.request),
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.ordinal, 1);
    assert_eq!(a.resource.trial_source, ExperimentSource::Operator);
    assert_eq!(a.resource.outcome, Some(ExperimentOutcome::Pending));
    assert_eq!(
        a.resource.result_visibility,
        ExperimentResultVisibility::Pending
    );
    assert!(a.resource.run_id.is_none() && a.resource.author_run_id.is_none());
    assert_eq!(counts(&pool, f.cycle).await, (1, 1, 1));
    let mut different = f.request.clone();
    different.hypothesis.push('!');
    assert!(matches!(
        store
            .propose_experiment(&operator, "same", &different)
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    sqlx::query("UPDATE app.experiments SET outcome='REJECTED',outcome_reason='UNKNOWN_INTERNAL_SOURCE' WHERE id=$1")
        .bind(a.resource.id.as_uuid()).execute(&pool).await.unwrap();
    let replay = store
        .propose_experiment(&operator, "same", &f.request)
        .await
        .unwrap();
    assert_eq!(replay.resource.outcome, Some(ExperimentOutcome::Pending));
    assert_eq!(
        store
            .experiment(&operator, a.resource.id)
            .await
            .unwrap()
            .result_visibility,
        ExperimentResultVisibility::Restricted
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_credentials_cannot_exceed_frozen_proposal_count_or_double_charge_runs(
    pool: PgPool,
) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 3).await;
    let mut jobs = Vec::new();
    for i in 0..8 {
        let actor = machine(
            &pool,
            f.data.project,
            None,
            "CLI",
            &["EXPERIMENT_SUBMIT", "RESEARCH_READ"],
        )
        .await;
        let request = f.request.clone();
        let store = store.clone();
        jobs.push(tokio::spawn(async move {
            store
                .propose_experiment(&actor, &format!("proposal-{i}"), &request)
                .await
        }));
    }
    let mut ordinals = Vec::new();
    let mut exhausted = 0;
    for job in jobs {
        match job.await.unwrap() {
            Ok(result) => ordinals.push(result.resource.ordinal),
            Err(StoreError::Domain(domain::DomainError::BudgetExhausted("experiments"))) => {
                exhausted += 1
            }
            other => panic!("unexpected proposal result: {other:?}"),
        }
    }
    ordinals.sort_unstable();
    assert_eq!(ordinals, vec![1, 2, 3]);
    assert_eq!(exhausted, 5);
    assert_eq!(counts(&pool, f.cycle).await, (3, 3, 3));
    let counters: (i64, i64, i64) = sqlx::query_as("SELECT reserved_experiments::bigint,used_experiments::bigint,reserved_cpu_seconds::bigint FROM app.research_cycles WHERE id=$1")
        .bind(f.cycle.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counters, (0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn artifacts_families_and_parents_are_exact_and_rejection_leaves_no_ledger_row(pool: PgPool) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 8).await;
    let other = setup(&pool, &store, &operator, 8).await;
    let hidden = artifact(
        &pool,
        f.data.project,
        ResearchArtifactKind::Report,
        "EVALUATOR_ONLY",
    )
    .await;
    let mut bad = f.request.clone();
    bad.family_id = other.family;
    assert!(store
        .propose_experiment(&operator, "bad-family", &bad)
        .await
        .is_err());
    bad = f.request.clone();
    bad.proposal_artifact_id = hidden;
    assert!(store
        .propose_experiment(&operator, "hidden", &bad)
        .await
        .is_err());
    bad = f.request.clone();
    bad.code_artifact_id = Some(other.request.code_artifact_id.unwrap());
    assert!(store
        .propose_experiment(&operator, "other-code", &bad)
        .await
        .is_err());
    bad = f.request.clone();
    std::mem::swap(
        &mut bad.proposal_artifact_id,
        &mut bad.parameter_artifact_id,
    );
    assert!(store
        .propose_experiment(&operator, "wrong-roles", &bad)
        .await
        .is_err());
    bad = f.request.clone();
    bad.parent_experiment_id = Some(Id::new());
    assert!(store
        .propose_experiment(&operator, "unknown-parent", &bad)
        .await
        .is_err());
    let parent = store
        .propose_experiment(&operator, "parent", &other.request)
        .await
        .unwrap()
        .resource;
    bad = f.request.clone();
    bad.parent_experiment_id = Some(parent.id);
    assert!(store
        .propose_experiment(&operator, "wrong-parent", &bad)
        .await
        .is_err());
    assert_eq!(counts(&pool, f.cycle).await, (0, 0, 0));
    let first = store
        .propose_experiment(&operator, "valid", &f.request)
        .await
        .unwrap()
        .resource;
    let mut child = f.request.clone();
    child.parent_experiment_id = Some(first.id);
    let child = store
        .propose_experiment(&operator, "child", &child)
        .await
        .unwrap()
        .resource;
    assert_eq!(child.parent_experiment_id, Some(first.id));
    assert_eq!(child.ordinal, 2);
}

#[sqlx::test(migrations = "../../migrations")]
async fn machine_permission_is_not_operator_or_automation_authority(pool: PgPool) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 3).await;
    for (kind, scopes) in [
        ("CLI", vec!["RESEARCH_READ"]),
        ("AUTOMATION", vec!["RESEARCH_READ", "EXPERIMENT_SUBMIT"]),
    ] {
        let actor = machine(&pool, f.data.project, None, kind, &scopes).await;
        assert!(matches!(
            store.propose_experiment(&actor, "denied", &f.request).await,
            Err(StoreError::Forbidden)
        ));
    }
    let other = setup(&pool, &store, &operator, 3).await;
    let outsider = machine(
        &pool,
        other.data.project,
        None,
        "CLI",
        &["EXPERIMENT_SUBMIT", "RESEARCH_READ"],
    )
    .await;
    assert!(matches!(
        store
            .propose_experiment(&outsider, "other", &f.request)
            .await,
        Err(StoreError::NotFound)
    ));
    let created = store
        .propose_experiment(&operator, "created", &f.request)
        .await
        .unwrap()
        .resource;
    assert!(matches!(
        store.experiment(&outsider, created.id).await,
        Err(StoreError::NotFound)
    ));
    assert!(matches!(
        store
            .experiments(
                &outsider,
                &ResearchListQuery {
                    project_id: f.data.project,
                    cursor: None,
                    limit: 50
                }
            )
            .await,
        Err(StoreError::NotFound)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn mission_proposals_bind_the_real_author_attempt_not_the_science_run(pool: PgPool) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 6).await;
    let (run, attempt) = mission(&pool, &store, &operator, &f).await;
    let actor = machine(
        &pool,
        f.data.project,
        Some(run),
        "MISSION",
        &["RESEARCH_READ", "EXPERIMENT_SUBMIT"],
    )
    .await;
    let result = store
        .propose_experiment(&actor, "mission", &f.request)
        .await
        .unwrap()
        .resource;
    assert_eq!(result.author_run_id, Some(run));
    assert_eq!(result.author_attempt_id, Some(attempt));
    assert_eq!(result.trial_source, ExperimentSource::Codex);
    assert!(result.run_id.is_none());
    let row = sqlx::query("SELECT actor_kind,credential_id,author_attempt_id FROM app.experiment_authorship WHERE experiment_id=$1")
        .bind(result.id.as_uuid()).fetch_one(&pool).await.unwrap();
    let Actor::Machine { credential_id, .. } = actor else {
        unreachable!()
    };
    assert_eq!(row.get::<String, _>("actor_kind"), "MISSION");
    assert_eq!(
        row.get::<uuid::Uuid, _>("credential_id"),
        credential_id.as_uuid()
    );
    let new_attempt = Id::new();
    let mut tx = pool.begin().await.unwrap();
    sqlx::query("INSERT INTO app.run_attempts(id,run_id,attempt_no,worker_owner_id,owner_epoch,lease_expires_at,dispatch_state,runtime_state) VALUES($1,$2,2,'replacement',1,clock_timestamp()+interval '1 hour','NOT_SENT','UNKNOWN')")
        .bind(new_attempt.as_uuid()).bind(run.as_uuid()).execute(&mut *tx).await.unwrap();
    sqlx::query("UPDATE app.runs SET active_attempt_id=$1,current_attempt_no=2 WHERE id=$2")
        .bind(new_attempt.as_uuid())
        .bind(run.as_uuid())
        .execute(&mut *tx)
        .await
        .unwrap();
    tx.commit().await.unwrap();
    assert!(matches!(
        store
            .propose_experiment(&actor, "mission-again", &f.request)
            .await,
        Err(StoreError::InvalidCredentials)
    ));
    // Even replay requires current possession/authority; the old author row stays immutable.
    assert!(matches!(
        store
            .propose_experiment(&actor, "mission", &f.request)
            .await,
        Err(StoreError::InvalidCredentials)
    ));
    assert_eq!(
        store
            .experiment(&operator, result.id)
            .await
            .unwrap()
            .author_attempt_id,
        Some(attempt)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn mission_write_requires_an_unexpired_attempt_lease(pool: PgPool) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 3).await;
    let (run, attempt) = mission(&pool, &store, &operator, &f).await;
    let actor = machine(
        &pool,
        f.data.project,
        Some(run),
        "MISSION",
        &["RESEARCH_READ", "EXPERIMENT_SUBMIT"],
    )
    .await;
    sqlx::query("UPDATE app.run_attempts SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(attempt.as_uuid()).execute(&pool).await.unwrap();
    // Lease expiry now invalidates the Mission identity for every scope, before
    // experiment-specific authorization. Keep the precise authentication error.
    let result = store
        .propose_experiment(&actor, "expired-lease", &f.request)
        .await;
    assert!(
        matches!(result, Err(StoreError::InvalidCredentials)),
        "expired Mission must fail at the shared identity boundary: {result:?}"
    );
    assert_eq!(counts(&pool, f.cycle).await, (0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn paused_cycle_denies_new_proposals_but_preserves_exact_human_receipt(pool: PgPool) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 3).await;
    let initial = store
        .propose_experiment(&operator, "prior", &f.request)
        .await
        .unwrap()
        .resource;
    sqlx::query("UPDATE app.research_cycles SET state='PAUSED' WHERE id=$1")
        .bind(f.cycle.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        store.propose_experiment(&operator, "new", &f.request).await,
        Err(StoreError::Domain(domain::DomainError::AdmissionClosed))
    ));
    let replay = store
        .propose_experiment(&operator, "prior", &f.request)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, initial.id);
    assert_eq!(counts(&pool, f.cycle).await, (1, 1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn revocation_is_observed_after_the_native_credential_lock_wait(pool: PgPool) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 3).await;
    let actor = machine(
        &pool,
        f.data.project,
        None,
        "CLI",
        &["RESEARCH_READ", "EXPERIMENT_SUBMIT"],
    )
    .await;
    let Actor::Machine { credential_id, .. } = actor else {
        unreachable!()
    };
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("SELECT id FROM app.machine_credentials WHERE id=$1 FOR UPDATE")
        .bind(credential_id.as_uuid())
        .execute(&mut *lock)
        .await
        .unwrap();
    let work = {
        let store = store.clone();
        let actor = actor.clone();
        let request = f.request.clone();
        tokio::spawn(async move {
            store
                .propose_experiment(&actor, "revocation-race", &request)
                .await
        })
    };
    research_support::wait_for_query_lock(&pool, "SELECT principal_epoch,scope_codes").await;
    sqlx::query("INSERT INTO app.machine_credential_revocations(credential_id,effective_at,reason) VALUES($1,clock_timestamp(),'test revocation')")
        .bind(credential_id.as_uuid()).execute(&mut *lock).await.unwrap();
    lock.commit().await.unwrap();
    assert!(matches!(
        work.await.unwrap(),
        Err(StoreError::InvalidCredentials)
    ));
    assert_eq!(counts(&pool, f.cycle).await, (0, 0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn evaluator_or_unknown_conclusions_never_leak_through_research_metadata(pool: PgPool) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 4).await;
    let reader = machine(&pool, f.data.project, None, "CLI", &["RESEARCH_READ"]).await;
    let created = store
        .propose_experiment(&operator, "created", &f.request)
        .await
        .unwrap()
        .resource;
    let hidden = artifact(
        &pool,
        f.data.project,
        ResearchArtifactKind::Report,
        "EVALUATOR_ONLY",
    )
    .await;
    sqlx::query("UPDATE app.experiments SET outcome='SUPPORTED',outcome_reason='SEALED_SECRET_SENTINEL',conclusion_artifact_id=$1 WHERE id=$2")
        .bind(hidden.as_uuid()).bind(created.id.as_uuid()).execute(&pool).await.unwrap();
    for actor in [&operator, &reader] {
        let view = store.experiment(actor, created.id).await.unwrap();
        assert_eq!(
            view.result_visibility,
            ExperimentResultVisibility::Restricted
        );
        assert!(
            view.outcome.is_none()
                && view.outcome_reason.is_none()
                && view.conclusion_artifact_id.is_none()
        );
        let body = serde_json::to_string(&view).unwrap();
        assert!(!body.contains("SEALED_SECRET_SENTINEL") && !body.contains(&hidden.to_string()));
        let page = store
            .experiments(
                actor,
                &ResearchListQuery {
                    project_id: f.data.project,
                    cursor: None,
                    limit: 1,
                },
            )
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        assert_eq!(
            page.items[0].result_visibility,
            ExperimentResultVisibility::Restricted
        );
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn publication_failure_rolls_back_proposal_author_and_original_response(pool: PgPool) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 3).await;
    sqlx::query("CREATE FUNCTION app.test_reject_proposal() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.operation='EXPERIMENT_PROPOSE' THEN RAISE EXCEPTION USING ERRCODE='23514',MESSAGE='injected publication failure'; END IF; RETURN NEW; END $$")
        .execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER test_reject_proposal BEFORE INSERT ON app.command_receipts FOR EACH ROW EXECUTE FUNCTION app.test_reject_proposal()")
        .execute(&pool).await.unwrap();
    assert!(matches!(
        store
            .propose_experiment(&operator, "fail", &f.request)
            .await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(counts(&pool, f.cycle).await, (0, 0, 0));
    sqlx::query("DROP TRIGGER test_reject_proposal ON app.command_receipts")
        .execute(&pool)
        .await
        .unwrap();
    let result = store
        .propose_experiment(&operator, "fail", &f.request)
        .await
        .unwrap();
    assert!(!result.replayed);
    assert_eq!(result.resource.ordinal, 1);
}

#[sqlx::test(migrations = "../../migrations")]
async fn authorship_is_immutable_and_pagination_preserves_each_trial(pool: PgPool) {
    let (store, operator) = research_support::operator(&pool).await;
    let f = setup(&pool, &store, &operator, 3).await;
    let a = store
        .propose_experiment(&operator, "a", &f.request)
        .await
        .unwrap()
        .resource;
    let b = store
        .propose_experiment(&operator, "b", &f.request)
        .await
        .unwrap()
        .resource;
    let first = store
        .experiments(
            &operator,
            &ResearchListQuery {
                project_id: f.data.project,
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.items[0].id, b.id);
    let second = store
        .experiments(
            &operator,
            &ResearchListQuery {
                project_id: f.data.project,
                cursor: first.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(second.items[0].id, a.id);
    assert!(second.next_cursor.is_none());
    for sql in [
        "DELETE FROM app.experiment_authorship WHERE experiment_id=$1",
        "UPDATE app.experiment_authorship SET actor_kind=actor_kind WHERE experiment_id=$1",
    ] {
        let error = sqlx::query(sql)
            .bind(a.id.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23000")
        );
    }
    assert!(store
        .experiments(
            &operator,
            &ResearchListQuery {
                project_id: f.data.project,
                cursor: None,
                limit: 0
            }
        )
        .await
        .is_err());
    assert_eq!(counts(&pool, f.cycle).await, (2, 2, 2));
}
