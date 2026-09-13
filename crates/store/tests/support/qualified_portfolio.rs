//! Original Store admissions and files with controlled runtime/model declarations.
//! No SQL-authored qualifications, actual model inference or REAL market claim.
use super::*;
use contracts::{experiments::ExperimentProposalV1, research::DataOrigin};
use store::turns::{NativePublicSummary, TurnOutcome, UsageReceipt};

#[path = "portfolio_inputs.rs"]
mod inputs;
#[path = "portfolio_result.rs"]
mod result;

async fn begin(store: &Store, lease: &RunLease) {
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_mission_session(
            lease.run.id,
            &lease.fence,
            &store::lifecycle::mission::NativeSessionReceipt {
                thread_id: Id::new().to_string(),
                codex_version: "0.144.4".into(),
                protocol_schema_version: "v2".into(),
                requested_service_tier: None,
                effective: contracts::codex::CodexEffectiveSettingsV1 {
                    model: "controlled-native-model".into(),
                    provider: "controlled-native-provider".into(),
                    reasoning_effort: Some("medium".into()),
                    service_tier: None,
                },
            },
        )
        .await
        .unwrap();
}

async fn answer(store: &Store, f: &cycle_support::Fixture, lease: &RunLease, text: String) {
    let turn = store
        .mission_turn_checkpoint(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .latest
        .unwrap()
        .reservation;
    let native_turn = Id::new().to_string();
    store
        .claim_turn_dispatch(turn.id, &lease.fence)
        .await
        .unwrap();
    store
        .bind_native_turn(turn.id, &lease.fence, &native_turn)
        .await
        .unwrap();
    store
        .observe_mission_turn_terminal(
            turn.id,
            &lease.fence,
            TurnOutcome::Succeeded,
            "NATIVE_TURN_COMPLETED",
        )
        .await
        .unwrap();
    store
        .settle_turn(
            turn.id,
            &lease.fence,
            &UsageReceipt {
                outcome: TurnOutcome::Succeeded,
                actual_tokens: DbCounter::new(12).unwrap(),
                actual_cost: None,
                currency: None,
                reason_code: "NATIVE_TURN_COMPLETED".into(),
            },
        )
        .await
        .unwrap();
    store
        .record_mission_summary(
            turn.id,
            &lease.fence,
            &NativePublicSummary {
                schema_version: SchemaV1,
                native_turn_id: native_turn,
                native_item_id: Id::new().to_string(),
                phase: Some("final_answer".into()),
                text,
            },
            |id, size| f.read(id, size),
            |object| async move {
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn original_reviewed_alphas_publish_candidates_and_retry_last_target(pool: PgPool) {
    let directory = tempfile::tempdir().unwrap();
    let objects = std::sync::Arc::new(
        integrations::artifacts::ArtifactStore::open(&directory.path().join("objects")).unwrap(),
    );
    let (store, actor) = research_support::operator(&pool).await;
    let f = cycle_support::setup_with_policy(
        &pool,
        &store,
        &actor,
        objects,
        DataOrigin::Real,
        |policy| {
            policy.selection.candidate_count = 2;
            policy.maximum_sealed_uses_per_lineage = 2;
            policy.sealed_metric_requirements[0].threshold_low = Some("0.1".parse().unwrap());
        },
    )
    .await;
    let (store, actor, f, cycle, preparation) =
        mission_support::start(store, actor, f, false).await;
    mission_support::complete(&pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let message = store.read_mission_messages(60, 1).await.unwrap().remove(0);
    let Some(ClaimResult::Leased(parent)) = store
        .claim_mission(&message, "two-source-research", 120)
        .await
        .unwrap()
    else {
        panic!("original Research admission");
    };
    begin(&store, &parent).await;
    store
        .prepare_initial_mission_turn(
            parent.run.id,
            &parent.fence,
            |id, size| f.read(id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap();
    answer(
        &store,
        &f,
        &parent,
        "Controlled research plan, not native science.".into(),
    )
    .await;
    let first = experiment_support::propose(&pool, &store, &actor, &f, cycle).await;
    let body: serde_json::Value = sqlx::query_scalar("SELECT normalized_nonsecret_request FROM app.command_receipts WHERE operation='EXPERIMENT_PROPOSE' AND resource_id=$1")
        .bind(first.as_uuid()).fetch_one(&pool).await.unwrap();
    let mut proposal: ExperimentProposalV1 = serde_json::from_value(body).unwrap();
    proposal.hypothesis =
        "Second preregistered controlled trial with the same original input contract".into();
    let second = store
        .propose_experiment(&actor, "second-original-trial", &proposal)
        .await
        .unwrap()
        .resource
        .id;
    let experiments = [first, second];
    for experiment in experiments {
        let compiler = start(&store, &f, &parent, experiment)
            .await
            .unwrap()
            .resource
            .id;
        complete_compilation(&pool, &store, &f, compiler).await;
        let predicted = forecast(&store, &f, &parent, experiment)
            .await
            .unwrap()
            .resource
            .id;
        experiment_support::complete_forecast(&pool, &store, &f, predicted).await;
        store
            .prepare_research_alpha(parent.run.id, &parent.fence, experiment)
            .await
            .unwrap();
        let validated = validation(&store, &f, &parent, experiment)
            .await
            .unwrap()
            .resource
            .id;
        experiment_support::complete_validation(&pool, &store, &f, validated, 1000, 0.8).await;
        validation_publication::publish(&store, &f, validated)
            .await
            .unwrap();
        assert!(result_turn(&store, &f, &parent).await.unwrap());
        answer(
            &store,
            &f,
            &parent,
            "Controlled acknowledgement of original formal Validation.".into(),
        )
        .await;
    }
    assert!(store
        .complete_mission(parent.run.id, &parent.fence)
        .await
        .unwrap());
    store.acknowledge_run(&message).await.unwrap();
    let message = store.read_mission_messages(60, 1).await.unwrap().remove(0);
    let Some(ClaimResult::Leased(review)) = store
        .claim_mission(&message, "two-source-review", 120)
        .await
        .unwrap()
    else {
        panic!("original independent Reviewer admission");
    };
    begin(&store, &review).await;
    for _ in experiments {
        let work = store
            .mission_review_work(review.run.id, &review.fence)
            .await
            .unwrap()
            .unwrap();
        store
            .prepare_mission_review_turn(
                review.run.id,
                &review.fence,
                work.experiment_id,
                |id, size| f.read(id, size),
                |object| {
                    std::future::ready(
                        f.objects
                            .put(object.id, &object.bytes)
                            .map_err(|_| StoreError::Integrity),
                    )
                },
            )
            .await
            .unwrap();
        answer(
            &store,
            &f,
            &review,
            serde_json::json!({"schema_version":1,
            "alpha_version_id":work.alpha_version_id,"decision":"PASS",
            "reasons":["Controlled independent protocol assessment, not actual scientific review"]})
            .to_string(),
        )
        .await;
    }
    assert!(store
        .mission_review_work(review.run.id, &review.fence)
        .await
        .unwrap()
        .is_none());
    for _ in experiments {
        store
            .prepare_review_sealed(
                review.run.id,
                &review.fence,
                |id, size| f.read(id, size),
                |object| {
                    std::future::ready(
                        f.objects
                            .put(object.id, &object.bytes)
                            .map_err(|_| StoreError::Integrity),
                    )
                },
            )
            .await
            .unwrap();
    }
    // Match the Worker: admit every reviewed target before ending the Reviewer,
    // with both native jobs queued under the frozen parallel-run limit of two.
    assert!(store
        .complete_mission(review.run.id, &review.fence)
        .await
        .unwrap());
    store.acknowledge_run(&message).await.unwrap();
    let sealed_runs: Vec<uuid::Uuid> = sqlx::query_scalar("SELECT held.run_id FROM app.mission_sealed_evaluations held JOIN app.mission_review_turns t ON t.reservation_id=held.review_reservation_id JOIN app.runs r ON r.id=held.run_id WHERE t.run_id=$1 AND r.state='QUEUED' ORDER BY held.run_id")
        .bind(review.run.id.as_uuid()).fetch_all(&pool).await.unwrap();
    assert_eq!(sealed_runs.len(), 2);
    for sealed in sealed_runs {
        let sealed: Id = sealed.to_string().try_into().unwrap();
        let message = validation_publication::message(&pool, sealed).await;
        let Some(ClaimResult::Leased(lease)) = store
            .claim_native_run(&message, "two-source-sealed", 60)
            .await
            .unwrap()
        else {
            panic!("original Sealed admission");
        };
        experiment_support::complete_sealed(&pool, &store, &f, *lease).await;
        validation_publication::publish(&store, &f, sealed)
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            store.acknowledge_run(&message),
            store.acknowledge_run(&message)
        );
        a.unwrap();
        b.unwrap();
    }
    let counts: (i64,i64,i64) = sqlx::query_as("SELECT count(*),count(DISTINCT q.alpha_version_id),count(DISTINCT q.qualifying_evaluation_id) FROM app.qualifications q JOIN app.alpha_versions v ON v.id=q.alpha_version_id JOIN app.alphas a ON a.id=v.alpha_id AND a.active_version_id=v.id AND a.lifecycle='QUALIFIED' WHERE v.project_id=$1")
        .bind(f.data.project.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (2, 2, 2));
    let request = inputs::request(&pool, &store, &actor, &f, cycle).await;
    let origin:String=sqlx::query_scalar("SELECT a.origin FROM app.execution_assumptions e JOIN app.artifacts a ON a.id=e.fee_schedule_artifact_id WHERE e.id=$1")
        .bind(f.data.assumptions.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        origin, "SYNTHETIC",
        "declared parameters must not masquerade as market data"
    );
    let admitted = store
        .start_portfolio_build(
            &actor,
            "original-qualified-build",
            &request,
            |id, size| f.read(id, size),
            |object| {
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    let replay = store
        .start_portfolio_build(
            &actor,
            "original-qualified-build",
            &request,
            |_, _| async { panic!("replay reads no source") },
            |_| async { panic!("replay publishes no object") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, admitted.id);
    let message = validation_publication::message(&pool, admitted.id).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "original-qualified-build", 60)
        .await
        .unwrap()
    else {
        panic!("original Build admission");
    };
    let job = store.native_job(admitted.id, &lease.fence).await.unwrap();
    assert_eq!(job.spec.job_kind, contracts::runs::RunKind::PortfolioBuild);
    for input in &job.spec.inputs {
        if let RuntimeInputV1::Artifact {
            artifact_id,
            byte_count,
            ..
        } = input
        {
            assert_eq!(
                f.read(*artifact_id, *byte_count).await.unwrap().len() as u64,
                byte_count.get()
            );
        }
    }
    result::complete(&pool, &store, &f, &lease, &job).await;
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    let candidate = validation_publication::publish(&store, &f, admitted.id)
        .await
        .unwrap()
        .resource;
    let facts:(String,String,String,i64,i64)=sqlx::query_as("SELECT c.solver_status,c.evidence_status,a.origin,(SELECT count(*) FROM app.candidate_alphas WHERE candidate_id=c.id),(SELECT count(*) FROM app.candidate_targets WHERE candidate_id=c.id) FROM app.portfolio_candidates c JOIN app.candidate_publications p ON p.candidate_id=c.id JOIN app.artifacts a ON a.id=c.target_artifact_id WHERE c.id=$1")
        .bind(candidate.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        facts,
        ("OPTIMAL".into(), "VALID".into(), "SYNTHETIC".into(), 2, 1)
    );
    let replay = store
        .publish_scientific_result(
            admitted.id,
            |_, _| async { panic!("Candidate replay reads nothing") },
            |_| async { panic!("Candidate replay publishes nothing") },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, candidate);
    store.acknowledge_run(&message).await.unwrap();

    // A later decision consumes the original published target, not an account
    // snapshot and not a copied qualification. It needs its own frozen cutoff.
    use contracts::{
        portfolio::PortfolioBuildWeightsV1,
        research::{DataPartition, InputItemV1, InputPurpose, InputSetCreate},
    };
    let dataset: uuid::Uuid = sqlx::query_scalar(
        "SELECT dataset_revision_id FROM app.input_set_items WHERE input_set_id=$1",
    )
    .bind(request.input_set_id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let cutoff = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let input = store
        .create_input_set(
            &actor,
            "last-target-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.data.project,
                purpose: InputPurpose::Forward,
                decision_cutoff: cutoff,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset.to_string().try_into().unwrap(),
                    role: DataPartition::Forward,
                }],
            },
        )
        .await
        .unwrap()
        .resource
        .header
        .id;
    let mut next = request;
    next.input_set_id = input;
    next.current_weights_source = PortfolioBuildWeightsV1::LastTarget {
        candidate_id: candidate,
    };
    let stale = store
        .start_portfolio_build(
            &actor,
            "stale-last-target",
            &next,
            |id, size| f.read(id, size),
            |_| async { panic!("old catalog cutoff cannot admit a newer target") },
        )
        .await;
    assert!(
        matches!(stale,Err(StoreError::Domain(domain::DomainError::Fields(ref fields))) if fields.iter().any(|issue|issue.field=="portfolio.current_weights")),
        "{stale:?}"
    );
    let dataset = inputs::forward(&pool, &store, &actor, &f).await;
    let cutoff = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    next.input_set_id = store
        .create_input_set(
            &actor,
            "current-last-target-input",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.data.project,
                purpose: InputPurpose::Forward,
                decision_cutoff: cutoff,
                items: vec![InputItemV1::Dataset {
                    dataset_revision_id: dataset,
                    role: DataPartition::Forward,
                }],
            },
        )
        .await
        .unwrap()
        .resource
        .header
        .id;
    let snapshot="SELECT (SELECT count(*) FROM app.portfolio_build_tasks),(SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.artifacts),reserved_cpu_seconds FROM app.research_cycles WHERE id=$1";
    let before: (i64, i64, i64, i64) = sqlx::query_as(snapshot)
        .bind(cycle.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut allocated = Vec::new();
    let failed = store
        .start_portfolio_build(
            &actor,
            "last-target-build",
            &next,
            |id, size| f.read(id, size),
            |object| {
                allocated.push((
                    object.id,
                    DbCounter::new(object.bytes.len() as u64).unwrap(),
                ));
                std::future::ready(if allocated.len() == 2 {
                    Err(StoreError::Integrity)
                } else {
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                })
            },
        )
        .await;
    assert!(matches!(failed, Err(StoreError::Integrity)), "{failed:?}");
    assert_eq!(allocated.len(), 2, "derived weights then native parameters");
    let after: (i64, i64, i64, i64) = sqlx::query_as(snapshot)
        .bind(cycle.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        before, after,
        "no artifact rows, Run or CPU charge after the second publication fails"
    );
    for (id, size) in allocated {
        store
            .discard_unpublished_operator_artifact(id, |id| {
                std::future::ready(
                    f.objects
                        .discard_unpublished(id)
                        .map_err(|_| StoreError::Integrity),
                )
            })
            .await
            .unwrap();
        assert!(f.read(id, size).await.is_err());
    }
    let mut published = 0;
    let next_run = store
        .start_portfolio_build(
            &actor,
            "last-target-build",
            &next,
            |id, size| f.read(id, size),
            |object| {
                published += 1;
                std::future::ready(
                    f.objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity),
                )
            },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(published, 2);
    let message = validation_publication::message(&pool, next_run.id).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "last-target-build", 60)
        .await
        .unwrap()
    else {
        panic!("original LastTarget admission");
    };
    let job = store.native_job(next_run.id, &lease.fence).await.unwrap();
    result::complete(&pool, &store, &f, &lease, &job).await;
    let next_candidate = validation_publication::publish(&store, &f, next_run.id)
        .await
        .unwrap()
        .resource;
    assert_ne!(next_candidate, candidate);
    let facts:(String,String,String,uuid::Uuid,Option<uuid::Uuid>)=sqlx::query_as("SELECT c.evidence_status,c.current_weights_source,a.origin,t.last_target_candidate_id,t.snapshot_id FROM app.portfolio_candidates c JOIN app.artifacts a ON a.id=c.target_artifact_id JOIN app.portfolio_build_tasks t ON t.run_id=c.run_id WHERE c.id=$1")
        .bind(next_candidate.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        facts,
        (
            "VALID".into(),
            "LAST_TARGET".into(),
            "SYNTHETIC".into(),
            candidate.as_uuid(),
            None
        )
    );
    store.acknowledge_run(&message).await.unwrap();
}
