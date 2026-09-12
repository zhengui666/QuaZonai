//! Actual PG/PGMQ selection from controlled native reports, not market/T42 proof.
use super::*;
use contracts::{
    control::ListQuery,
    cycles::{SelectionStatus, TrialSelectionReason},
    experiments::ExperimentProposalV1,
    research::SelectionDirection,
};
use store::{authority::Actor, lifecycle::RunMessage};
use validation_publication::{message, prepared, publish};

async fn proposal(pool: &PgPool, experiment: Id) -> ExperimentProposalV1 {
    let body: serde_json::Value=sqlx::query_scalar("SELECT normalized_nonsecret_request FROM app.command_receipts WHERE operation='EXPERIMENT_PROPOSE' AND resource_id=$1")
        .bind(experiment.as_uuid()).fetch_one(pool).await.unwrap();
    serde_json::from_value(body).unwrap()
}

async fn cancelled_parent(
    pool: &PgPool,
    store: &Store,
    actor: &Actor,
    lease: &RunLease,
) -> RunMessage {
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    let current = store.get_run(actor, lease.run.id).await.unwrap();
    store
        .cancel_run(
            actor,
            "selection-stop",
            current.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: current.revision,
            },
        )
        .await
        .unwrap();
    assert!(store
        .complete_research_mission(current.id, &lease.fence)
        .await
        .unwrap());
    message(pool, current.id).await
}

#[sqlx::test(migrations = "../../migrations")]
async fn failed_selection_keeps_original_queue_and_replay_seals_members_not_other_cycles(
    pool: PgPool,
) {
    let (store, actor, f, lease, experiment, validation) = prepared(&pool).await;
    experiment_support::complete_validation(&pool, &store, &f, validation, 1000, 0.8).await;
    let evaluation = publish(&store, &f, validation).await.unwrap().resource;
    let request = proposal(&pool, experiment).await;
    let verifier = Id::new();
    let credential = store
        .issue_mission_credential(lease.run.id, &lease.fence, Id::new(), verifier)
        .await
        .unwrap();
    let agent = Actor::Machine {
        credential_id: credential,
        verifier_ref: verifier,
        operator_grant: None,
    };
    assert!(matches!(
        store.cycle_selection(&agent, request.cycle_id).await,
        Err(StoreError::Forbidden)
    ));
    assert!(matches!(
        store
            .cycle_selection_trials(&agent, request.cycle_id, &Default::default())
            .await,
        Err(StoreError::Forbidden)
    ));
    let other = Id::new();
    // Explicit relational second Cycle: it is not claimed to have run a Mission.
    sqlx::query("INSERT INTO app.research_cycles(id,project_id,brief_id,ordinal,trigger,state,budget_snapshot) SELECT $1,project_id,brief_id,ordinal+1,'OPERATOR','RUNNING',budget_snapshot FROM app.research_cycles WHERE id=$2")
        .bind(other.as_uuid()).bind(request.cycle_id.as_uuid()).execute(&pool).await.unwrap();
    let mut next = request.clone();
    next.cycle_id = other;
    let pending = store
        .propose_experiment(&actor, "another-cycle", &next)
        .await
        .unwrap()
        .resource
        .id;
    let queued = cancelled_parent(&pool, &store, &actor, &lease).await;
    sqlx::raw_sql("CREATE FUNCTION public.reject_selection() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled selection failure'; END $$; CREATE TRIGGER reject_selection BEFORE INSERT ON app.cycle_selections FOR EACH ROW EXECUTE FUNCTION public.reject_selection();")
        .execute(&pool).await.unwrap();
    assert!(store.acknowledge_run(&queued).await.is_err());
    for table in ["cycle_selections", "cycle_selection_trials"] {
        let count: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM app.{table}"))
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(count, 0, "whole snapshot rolled back");
    }
    assert_eq!(
        store.get_run(&actor, lease.run.id).await.unwrap().state,
        contracts::runs::RunState::Cancelled
    );
    assert_eq!(
        message(&pool, lease.run.id).await.message_id,
        queued.message_id
    );
    sqlx::query("DROP TRIGGER reject_selection ON app.cycle_selections")
        .execute(&pool)
        .await
        .unwrap();
    let (a, b) = tokio::join!(
        store.acknowledge_run(&queued),
        store.acknowledge_run(&queued)
    );
    a.unwrap();
    b.unwrap();
    let snapshot = store
        .cycle_selection(&actor, request.cycle_id)
        .await
        .unwrap();
    let cli =
        proposal_support::machine(&pool, f.data.project, None, "CLI", &["RESEARCH_READ"]).await;
    assert_eq!(
        store
            .cycle_selection(&cli, request.cycle_id)
            .await
            .unwrap()
            .cycle_id,
        request.cycle_id
    );
    assert_eq!(
        store
            .cycle_selection_trials(&cli, request.cycle_id, &Default::default())
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    for (kind, scopes) in [
        ("CLI", vec!["RUN_READ"]),
        ("AUTOMATION", vec!["RESEARCH_READ"]),
    ] {
        let reader = proposal_support::machine(&pool, f.data.project, None, kind, &scopes).await;
        assert!(matches!(
            store.cycle_selection(&reader, request.cycle_id).await,
            Err(StoreError::Forbidden)
        ));
        assert!(matches!(
            store
                .cycle_selection_trials(&reader, request.cycle_id, &Default::default())
                .await,
            Err(StoreError::Forbidden)
        ));
    }
    let other_project = research_support::setup(&pool, &store, &actor).await;
    let wrong = proposal_support::machine(
        &pool,
        other_project.project,
        None,
        "CLI",
        &["RESEARCH_READ"],
    )
    .await;
    assert!(matches!(
        store.cycle_selection(&wrong, request.cycle_id).await,
        Err(StoreError::NotFound)
    ));
    assert!(store
        .cycle_selection_trials(
            &cli,
            request.cycle_id,
            &ListQuery {
                limit: 0,
                cursor: None
            }
        )
        .await
        .is_err());
    assert_eq!(snapshot.status, SelectionStatus::Inconclusive);
    assert_eq!(
        (
            snapshot.trial_count.get(),
            snapshot.eligible_count.get(),
            snapshot.selected_count.get(),
            snapshot.unfinished_count.get()
        ),
        (2, 1, 1, 1)
    );
    let first = store
        .cycle_selection_trials(
            &actor,
            request.cycle_id,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(first.items[0].experiment_id, experiment);
    assert_eq!(first.items[0].evaluation_id, Some(evaluation));
    assert_eq!(
        first.items[0].selection_metric.as_ref().unwrap().value,
        Some(0.8)
    );
    let second = store
        .cycle_selection_trials(
            &actor,
            request.cycle_id,
            &ListQuery {
                cursor: first.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(second.items[0].experiment_id, pending);
    assert_eq!(second.items[0].reason, TrialSelectionReason::Unfinished);
    assert!(second.items[0].selection_metric.is_none());
    assert!(second.next_cursor.is_none());
    // Recording the other Cycle's pending state must not consume/freeze it.
    sqlx::query("UPDATE app.experiments SET outcome='INCONCLUSIVE',outcome_reason='CONTROLLED_LATER_END' WHERE id=$1")
        .bind(pending.as_uuid()).execute(&pool).await.unwrap();
    store
        .propose_experiment(&actor, "later-trial", &next)
        .await
        .unwrap();
    store.acknowledge_run(&queued).await.unwrap();
    assert_eq!(
        serde_json::to_value(
            store
                .cycle_selection(&actor, request.cycle_id)
                .await
                .unwrap()
        )
        .unwrap(),
        serde_json::to_value(&snapshot).unwrap()
    );
    let unchanged = store
        .cycle_selection_trials(
            &actor,
            request.cycle_id,
            &ListQuery {
                cursor: first.next_cursor,
                limit: 1,
            },
        )
        .await
        .unwrap();
    assert_eq!(
        serde_json::to_value(unchanged).unwrap(),
        serde_json::to_value(second).unwrap()
    );
    assert!(store
        .propose_experiment(&actor, "after-selection", &request)
        .await
        .is_err());
    let replay = store
        .propose_experiment(&actor, "native-compilation", &request)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, experiment);
    for statement in [
        "UPDATE app.cycle_selections SET trial_count=trial_count WHERE cycle_id=$1",
        "DELETE FROM app.cycle_selections WHERE cycle_id=$1",
        "UPDATE app.cycle_selection_trials SET selected=selected WHERE cycle_id=$1",
        "DELETE FROM app.cycle_selection_trials WHERE cycle_id=$1",
        "INSERT INTO app.cycle_selection_trials SELECT * FROM app.cycle_selection_trials WHERE cycle_id=$1",
        "INSERT INTO app.experiments(project_id,cycle_id,family_id,ordinal,hypothesis,expected_failure_modes,proposal_artifact_id,trial_source,outcome) SELECT project_id,cycle_id,family_id,99,hypothesis,expected_failure_modes,proposal_artifact_id,trial_source,'PENDING' FROM app.experiments WHERE cycle_id=$1",
    ] { assert!(sqlx::query(statement).bind(request.cycle_id.as_uuid()).execute(&pool).await.is_err()); }
    assert!(store
        .cycle(&actor, request.cycle_id)
        .await
        .unwrap()
        .available_actions
        .contains(&contracts::cycles::CycleReadAction::ViewSelection));
}

#[sqlx::test(migrations = "../../migrations")]
async fn unbound_historical_runs_retain_original_states_without_formal_metrics(pool: PgPool) {
    let (store, actor, f, lease, experiment, validation) = prepared(&pool).await;
    experiment_support::complete_validation(&pool, &store, &f, validation, 1000, 0.8).await;
    publish(&store, &f, validation).await.unwrap();
    let request = proposal(&pool, experiment).await;
    let mut originals = Vec::new();
    for (state, reason) in [
        ("QUEUED", TrialSelectionReason::Unfinished),
        ("SUCCEEDED", TrialSelectionReason::NoFormalEvaluation),
        ("FAILED", TrialSelectionReason::ExecutionFailed),
        ("CANCELLED", TrialSelectionReason::ExecutionCancelled),
    ] {
        let trial = store
            .propose_experiment(&actor, state, &request)
            .await
            .unwrap()
            .resource
            .id;
        let run = Id::new();
        // Historical relational metadata, deliberately NOT native execution or
        // an accepted receipt. Such rows cannot become formal scientific evidence.
        sqlx::query("INSERT INTO app.runs(id,project_id,cycle_id,kind,input_set_id,state,queued_at,deadline_at,finished_at) VALUES($1,$2,$3,'ALPHA_EVALUATE',$4,$5,clock_timestamp(),clock_timestamp()+interval '1 hour',CASE WHEN $5='QUEUED' THEN NULL ELSE clock_timestamp() END)")
            .bind(run.as_uuid()).bind(f.data.project.as_uuid()).bind(request.cycle_id.as_uuid())
            .bind(f.freeze.execution_context.validation_input_set_id.as_uuid()).bind(state).execute(&pool).await.unwrap();
        sqlx::query("UPDATE app.experiments SET run_id=$1 WHERE id=$2")
            .bind(run.as_uuid())
            .bind(trial.as_uuid())
            .execute(&pool)
            .await
            .unwrap();
        originals.push((trial, run, state, reason));
    }
    let queued = cancelled_parent(&pool, &store, &actor, &lease).await;
    store.acknowledge_run(&queued).await.unwrap();
    let snapshot = store
        .cycle_selection(&actor, request.cycle_id)
        .await
        .unwrap();
    assert_eq!(snapshot.trial_count.get(), 5);
    assert_eq!(snapshot.eligible_count.get(), 1);
    assert_eq!(snapshot.unfinished_count.get(), 1);
    assert_eq!(snapshot.status, SelectionStatus::Inconclusive);
    let rows = store
        .cycle_selection_trials(&actor, request.cycle_id, &Default::default())
        .await
        .unwrap()
        .items;
    for (trial, run, state, reason) in originals {
        let row = rows.iter().find(|r| r.experiment_id == trial).unwrap();
        assert_eq!(row.execution_run_id, Some(run));
        assert_eq!(serde_json::to_value(row.execution_state).unwrap(), state);
        assert_eq!(row.reason, reason);
        assert!(
            row.evaluation_id.is_none()
                && row.selection_metric.is_none()
                && row.rank.is_none()
                && !row.selected
        );
    }
}

async fn ranked(pool: PgPool, direction: SelectionDirection) {
    let (store, actor) = research_support::operator(&pool).await;
    let directory = tempfile::tempdir().unwrap();
    let objects = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let f = cycle_support::setup_with_policy(&pool, &store, &actor, objects, |policy| {
        policy.selection.direction = direction;
        policy.selection.candidate_count = 2;
    })
    .await;
    let (store, actor, f, cycle, preparation) =
        mission_support::start(store, actor, f, false).await;
    mission_support::complete(&pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let message = store.read_mission_messages(60, 1).await.unwrap().remove(0);
    let Some(ClaimResult::Leased(lease)) = store
        .claim_mission(&message, "selection-fixture", 120)
        .await
        .unwrap()
    else {
        panic!("lease")
    };
    let original = experiment_support::propose(&pool, &store, &actor, &f, cycle).await;
    let request = proposal(&pool, original).await;
    let mut ids = Vec::new();
    for (i, value) in [0.0, -0.0, 0.8].into_iter().enumerate() {
        let experiment = if i == 0 {
            original
        } else {
            store
                .propose_experiment(&actor, &format!("rank-{i}"), &request)
                .await
                .unwrap()
                .resource
                .id
        };
        ids.push(experiment);
        let compiled = start(&store, &f, &lease, experiment)
            .await
            .unwrap()
            .resource
            .id;
        complete_compilation(&pool, &store, &f, compiled).await;
        let predicted = forecast(&store, &f, &lease, experiment)
            .await
            .unwrap()
            .resource
            .id;
        experiment_support::complete_forecast(&pool, &store, &f, predicted).await;
        store
            .prepare_research_alpha(lease.run.id, &lease.fence, experiment)
            .await
            .unwrap();
        let validation = validation(&store, &f, &lease, experiment)
            .await
            .unwrap()
            .resource
            .id;
        experiment_support::complete_validation(&pool, &store, &f, validation, 1000, value).await;
        publish(&store, &f, validation).await.unwrap();
    }
    // An unexecuted original proposal is retained but never gets a zero metric.
    let unexecuted = store
        .propose_experiment(&actor, "unexecuted", &request)
        .await
        .unwrap()
        .resource
        .id;
    let queued = cancelled_parent(&pool, &store, &actor, &lease).await;
    store.acknowledge_run(&queued).await.unwrap();
    let snapshot = store.cycle_selection(&actor, cycle).await.unwrap();
    assert_eq!(snapshot.status, SelectionStatus::Complete);
    assert_eq!(
        (
            snapshot.trial_count.get(),
            snapshot.eligible_count.get(),
            snapshot.selected_count.get()
        ),
        (4, 3, 2)
    );
    let trials = store
        .cycle_selection_trials(&actor, cycle, &Default::default())
        .await
        .unwrap()
        .items;
    assert_eq!(
        trials.iter().map(|v| v.experiment_id).collect::<Vec<_>>(),
        [ids[0], ids[1], ids[2], unexecuted]
    );
    let expected = if direction == SelectionDirection::Maximize {
        [2, 3, 1]
    } else {
        [1, 2, 3]
    };
    for (row, rank) in trials.iter().zip(expected) {
        assert_eq!(row.rank.unwrap().get(), rank);
        assert_eq!(row.selected, rank <= 2);
        assert_eq!(row.reason, TrialSelectionReason::Eligible);
    }
    // Scientific REJECT is still VALID comparative evidence, not an approval.
    let decision: String = sqlx::query_scalar("SELECT decision FROM app.evaluations WHERE id=$1")
        .bind(trials[0].evaluation_id.unwrap().as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(decision, "REJECT");
    assert_eq!(trials[3].reason, TrialSelectionReason::NotExecuted);
    assert!(trials[3].selection_metric.is_none() && trials[3].rank.is_none());
    let qualification: i64 = sqlx::query_scalar("SELECT count(*) FROM app.qualifications")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(qualification, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn maximize_preserves_original_finite_values_and_native_uuid_zero_ties(pool: PgPool) {
    ranked(pool, SelectionDirection::Maximize).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn minimize_preserves_rejected_trials_and_does_not_treat_minus_zero_as_better(pool: PgPool) {
    ranked(pool, SelectionDirection::Minimize).await;
}
