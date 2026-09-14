//! Real PostgreSQL admission/publication over explicit historical relation fixtures.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/forward_result.rs"]
mod forward_result;
#[path = "../../../tests/support/forward.rs"]
mod forward_support;
use chrono::{Duration, Utc};
use contracts::{delivery::*, research::*, DbCounter, Id, SchemaV1};
use forward_support::count;
use forward_support::{
    research as research_support, runtime_observation::protocol_fixture as runtime_support,
};
use sqlx::PgPool;
use store::StoreError;
#[sqlx::test(migrations = "../../migrations")]
async fn original_forward_inputs_queue_once_and_revalidate_before_dispatch(pool: PgPool) {
    exercise(pool, Case::Relational).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn native_wake_consumes_original_human_context_once(pool: PgPool) {
    exercise(pool, Case::Native).await;
}
#[sqlx::test(migrations = "../../migrations")]
async fn native_wake_honors_daily_cycle_quota(pool: PgPool) {
    exercise(pool, Case::DailyQuota).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_wake_rechecks_expiry_after_parameter_publication(pool: PgPool) {
    exercise(pool, Case::Expiry).await;
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Case {
    Relational,
    Native,
    DailyQuota,
    Expiry,
}

async fn exercise(pool: PgPool, case: Case) {
    let native = match case {
        Case::Relational => None,
        Case::DailyQuota => Some(true),
        Case::Native | Case::Expiry => Some(false),
    };
    let human = if let Some(quota) = native {
        Some(human_source(&pool, quota).await)
    } else {
        None
    };
    let setup = if let Some((f, store, actor)) = &human {
        let context = &f.freeze.execution_context;
        let started = f
            .start(
                store,
                actor,
                "original-human-cycle",
                &cycle_support::start_request(store, actor, f).await,
            )
            .await
            .unwrap()
            .resource;
        let (run, session, fence, deadline) = forward_support::support::mission(
            &pool,
            f.data.project,
            started.cycle.id,
            context.discovery_input_set_id,
            f.researcher_profile.profile_id,
        )
        .await;
        let report =
            forward_support::support::report_artifact(&pool, f.data.project, run, fence.attempt_id)
                .await;
        let relational = forward_support::support::Fixture {
            project: f.data.project,
            cycle: started.cycle.id,
            run,
            session,
            profile: f.researcher_profile.profile_id,
            input_set: context.discovery_input_set_id,
            artifact: f.data.artifact,
            report,
            budget: f.brief.content.budget.clone(),
            fence,
            deadline,
        };
        forward_support::setup_with_source(&pool, relational, store.clone(), actor.clone()).await
    } else {
        forward_support::setup(&pool).await
    };
    let forward_support::ForwardFixture {
        f,
        store,
        operator,
        actor,
        runtime,
        caps,
        handoff,
        objects,
        end,
        mut message,
        original,
        policy,
    } = setup;
    let read = |id: Id, size: DbCounter| {
        let result = objects.lock().unwrap().get(&id).cloned().or_else(|| {
            human
                .as_ref()
                .and_then(|(f, _, _)| f.objects.read(id, size).ok())
        });
        async move {
            let bytes = result.ok_or(StoreError::NotFound)?;
            assert_eq!(bytes.len() as u64, size.get());
            Ok(bytes)
        }
    };
    let publish = |v: store::lifecycle::native::NativeObjectPublication| {
        objects.lock().unwrap().insert(v.id, v.bytes);
        async { Ok(()) }
    };
    let (a, b) = tokio::join!(
        store.prepare_forward_evaluation(f.project),
        store.prepare_forward_evaluation(f.project)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.is_some(), b.is_some());
    assert_eq!(a.or(b), Some((handoff, "daily".into())));
    let (left, right) = tokio::join!(
        store.enqueue_forward_evaluation(handoff, "daily", read, publish),
        store.enqueue_forward_evaluation(handoff, "daily", read, publish)
    );
    let left = left.unwrap();
    let right = right.unwrap();
    assert_eq!(left.resource.id, right.resource.id);
    assert_ne!(left.replayed, right.replayed);
    assert!(left.resource.cycle_id.is_none());
    assert!(store
        .prepare_forward_evaluation(f.project)
        .await
        .unwrap()
        .is_none());
    let original_input = left.resource.input_set_id;
    let queued: i64 =
        sqlx::query_scalar("SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$1")
            .bind(left.resource.id.to_string())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(queued, 1);
    let params: uuid::Uuid = sqlx::query_scalar(
        "SELECT parameters_artifact_id FROM app.run_native_tasks WHERE run_id=$1",
    )
    .bind(left.resource.id.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let parameter: Id = params.to_string().try_into().unwrap();
    let definition: contracts::execution::NativeTaskParametersV1 =
        serde_json::from_slice(&objects.lock().unwrap()[&parameter]).unwrap();
    assert_eq!(
        definition.job_kind(),
        contracts::runs::RunKind::ForwardEvaluate
    );
    assert!(store
        .create_input_set(
            &operator,
            "forbidden-copy",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: f.project,
                purpose: InputPurpose::Forward,
                decision_cutoff: end,
                items: vec![InputItemV1::Artifact {
                    artifact_id: original.report_artifact_id,
                    role: ArtifactInputRole::Report
                }]
            }
        )
        .await
        .is_err());
    let messages = store.read_run_messages(60, 100).await.unwrap();
    let queued = messages
        .iter()
        .find(|m| m.run_id == left.resource.id)
        .unwrap();
    let store::lifecycle::ClaimResult::Leased(lease) = store
        .claim_run(queued, "forward-fixture", 60)
        .await
        .unwrap()
    else {
        panic!("original native lease")
    };
    let job = store
        .native_job(left.resource.id, &lease.fence)
        .await
        .unwrap();
    assert_eq!(job.spec.input_set_id, original_input);
    domain::execution::task(&job.spec, &definition).unwrap();
    let future_revocation;
    {
        // A separate original stream preserves the unsent-correction assertion below.
        let mut blocked = message.clone();
        blocked.external_message_id = "blocked-original".into();
        blocked.report.stream_id = "blocked".into();
        blocked.report.complete = false;
        let blocked_original = store
            .submit_forward_message(&actor, &blocked, read, publish)
            .await
            .unwrap()
            .resource;
        let mut measurement_message = message.clone();
        measurement_message.external_message_id = "measurement-original".into();
        measurement_message.report.stream_id = "measurement".into();
        store
            .submit_forward_message(&actor, &measurement_message, read, publish)
            .await
            .unwrap();
        assert_eq!(
            store.prepare_forward_evaluation(f.project).await.unwrap(),
            Some((handoff, "blocked".into()))
        );
        assert!(store
            .enqueue_forward_evaluation(handoff, "blocked", read, |_| async {
                panic!("partial source cannot publish parameters")
            })
            .await
            .is_err());
        assert_eq!(
            store.prepare_forward_evaluation(f.project).await.unwrap(),
            Some((handoff, "measurement".into()))
        );
        assert!(store
            .prepare_forward_evaluation(f.project)
            .await
            .unwrap()
            .is_none());
        // Correction is new original data and can bypass the reserved retry delay.
        blocked.external_message_id = "blocked-correction".into();
        blocked.report.message_revision = 2;
        blocked.report.supersedes_message_id = Some(blocked_original.id);
        blocked.report.complete = true;
        store
            .submit_forward_message(&actor, &blocked, read, publish)
            .await
            .unwrap();
        assert_eq!(
            store.prepare_forward_evaluation(f.project).await.unwrap(),
            Some((handoff, "blocked".into()))
        );
        assert!(store
            .prepare_forward_evaluation(f.project)
            .await
            .unwrap()
            .is_none());
        let measured = store
            .enqueue_forward_evaluation(handoff, "measurement", read, publish)
            .await
            .unwrap()
            .resource;
        let (_, job, request, finished) =
            forward_result::complete(&pool, &store, measured.id, &caps, &objects).await;
        let scheduled_at = finished
            + if case == Case::Expiry {
                Duration::seconds(20)
            } else {
                Duration::minutes(30)
            };
        future_revocation = store
            .revoke_automation(
                &operator,
                "future-forward-revoke",
                policy.id,
                &PolicyRevokeV1 {
                    schema_version: SchemaV1,
                    expected_latest_revocation_id: None,
                    effective_at: Some(scheduled_at),
                    reason: "controlled future revoke".into(),
                },
            )
            .await
            .unwrap()
            .resource
            .id;
        // Inject the final aggregate write failure, then retry the same terminal Run.
        sqlx::query("CREATE FUNCTION app.test_forward_window_failure() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled forward window failure'; END $$").execute(&pool).await.unwrap();
        sqlx::query("CREATE TRIGGER test_forward_window_failure BEFORE INSERT ON app.forward_evidence_windows FOR EACH ROW EXECUTE FUNCTION app.test_forward_window_failure()").execute(&pool).await.unwrap();
        let mut allocated = Vec::new();
        assert!(store
            .publish_scientific_result(job.run.id, read, |value| {
                allocated.push(value.id);
                publish(value)
            })
            .await
            .is_err());
        let count_rows: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app.evaluations WHERE run_id=$1")
                .bind(job.run.id.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(count_rows, 0);
        for id in allocated {
            store
                .discard_unpublished_native_object(job.run.id, id, |id| {
                    objects.lock().unwrap().remove(&id);
                    async { Ok(()) }
                })
                .await
                .unwrap();
        }
        sqlx::query("DROP TRIGGER test_forward_window_failure ON app.forward_evidence_windows")
            .execute(&pool)
            .await
            .unwrap();
        let (a, b) = tokio::join!(
            store.publish_scientific_result(job.run.id, read, publish),
            store.publish_scientific_result(job.run.id, read, publish)
        );
        let a = a.unwrap().unwrap();
        let b = b.unwrap().unwrap();
        assert_eq!(a.resource, b.resource);
        assert_ne!(a.replayed, b.replayed);
        let actual: (String,String,i64,bool,i64) = sqlx::query_as("SELECT e.evidence_status,e.decision,w.complete_observations,w.is_contiguous,(SELECT count(*) FROM app.metric_values WHERE evaluation_id=e.id) FROM app.evaluations e JOIN app.forward_evidence_windows w ON w.evaluation_id=e.id WHERE e.id=$1").bind(a.resource.as_uuid()).fetch_one(&pool).await.unwrap();
        assert_eq!(actual, ("VALID".into(), "INCONCLUSIVE".into(), 2, true, 3));
        let replay = store
            .publish_scientific_result(
                job.run.id,
                |_, _| async { panic!("published replay must not read") },
                |_| async { panic!("published replay must not write") },
            )
            .await
            .unwrap()
            .unwrap();
        assert_eq!(replay.resource, a.resource);
        assert!(replay.replayed);
        let until: chrono::DateTime<Utc> =
            sqlx::query_scalar("SELECT valid_until FROM app.evaluations WHERE id=$1")
                .bind(a.resource.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(until, scheduled_at);
        // Measurement remains VALID/INCONCLUSIVE; only its frozen maintenance policy classifies degradation.
        sqlx::query("CREATE FUNCTION app.fail_forward_wake() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled Wake failure'; END $$").execute(&pool).await.unwrap();
        sqlx::query("CREATE TRIGGER fail_forward_wake BEFORE INSERT ON app.wake_events FOR EACH ROW EXECUTE FUNCTION app.fail_forward_wake()").execute(&pool).await.unwrap();
        assert!(store.observe_forward(job.run.id).await.is_err());
        let rolled_back:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.degradation_observations WHERE evaluation_id=$1),(SELECT count(*) FROM app.forward_observation_publications WHERE run_id=$2),(SELECT count(*) FROM app.evaluations WHERE id=$1)").bind(a.resource.as_uuid()).bind(job.run.id.as_uuid()).fetch_one(&pool).await.unwrap();
        assert_eq!(rolled_back, (0, 0, 1));
        sqlx::query("DROP TRIGGER fail_forward_wake ON app.wake_events")
            .execute(&pool)
            .await
            .unwrap();
        let (observed, repeated) = tokio::join!(
            store.observe_forward(job.run.id),
            store.observe_forward(job.run.id)
        );
        let observed = observed.unwrap().unwrap();
        let repeated = repeated.unwrap().unwrap();
        assert_eq!(observed.resource, repeated.resource);
        assert_ne!(observed.replayed, repeated.replayed);
        let wake:(String,String,String,Vec<String>)=sqlx::query_as("SELECT o.classification,w.trigger,w.state,o.reason_codes FROM app.degradation_observations o JOIN app.wake_events w ON w.observation_id=o.id WHERE o.id=$1").bind(observed.resource.as_uuid()).fetch_one(&pool).await.unwrap();
        assert_eq!(
            (wake.0.as_str(), wake.1.as_str(), wake.2.as_str()),
            ("DEGRADED", "DEGRADATION", "PENDING")
        );
        assert!(wake
            .3
            .iter()
            .any(|r| r.starts_with("MAINTENANCE:") && r.ends_with("THRESHOLD_NOT_MET")));
        let wake_count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM app.wake_events WHERE observation_id=$1")
                .bind(observed.resource.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(wake_count, 1);
        let wake_id: uuid::Uuid =
            sqlx::query_scalar("SELECT id FROM app.wake_events WHERE observation_id=$1")
                .bind(observed.resource.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
        let wake_id: Id = wake_id.to_string().try_into().unwrap();
        if let Some(quota) = native {
            sqlx::query("UPDATE app.projects SET state='PAUSED' WHERE id=$1")
                .bind(f.project.as_uuid())
                .execute(&pool)
                .await
                .unwrap();
            assert!(store
                .prepare_degradation_wake(f.project)
                .await
                .unwrap()
                .is_none());
            assert!(store
                .consume_degradation_wake(wake_id, read, publish)
                .await
                .unwrap()
                .is_none());
            sqlx::query("UPDATE app.projects SET state='ACTIVE' WHERE id=$1")
                .bind(f.project.as_uuid())
                .execute(&pool)
                .await
                .unwrap();
            let (a, b) = tokio::join!(
                store.prepare_degradation_wake(f.project),
                store.prepare_degradation_wake(f.project)
            );
            assert_ne!(a.unwrap().is_some(), b.unwrap().is_some());
            assert!(store
                .consume_degradation_wake(wake_id, read, publish)
                .await
                .unwrap()
                .is_none());
            let reason: String =
                sqlx::query_scalar("SELECT reason FROM app.wake_events WHERE id=$1")
                    .bind(wake_id.as_uuid())
                    .fetch_one(&pool)
                    .await
                    .unwrap();
            assert_eq!(
                reason,
                if quota {
                    "CYCLES_PER_DAY"
                } else {
                    "CYCLE_COOLDOWN"
                }
            );
            if !quota {
                let wait: f64 = sqlx::query_scalar("SELECT greatest(0,extract(epoch FROM (not_before-clock_timestamp())))::double precision FROM app.wake_events WHERE id=$1").bind(wake_id.as_uuid()).fetch_one(&pool).await.unwrap();
                assert!(wait <= 15.0);
                tokio::time::sleep(std::time::Duration::from_secs_f64(wait + 0.02)).await;
                let before: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.research_cycles),(SELECT count(*) FROM app.run_admissions),(SELECT count(*) FROM pgmq.q_runs)").fetch_one(&pool).await.unwrap();
                if case == Case::Expiry {
                    let mut allocated = Vec::new();
                    let error = store.consume_degradation_wake(wake_id, read, |object| {
                        allocated.push(object.id);
                        let writing = publish(object);
                        let pool = &pool;
                        async move {
                            writing.await?;
                            let remaining: f64 = sqlx::query_scalar("SELECT greatest(0,extract(epoch FROM ($1::timestamptz-clock_timestamp())))::double precision")
                                .bind(scheduled_at).fetch_one(pool).await?;
                            assert!(remaining > 0.0 && remaining <= 20.0);
                            tokio::time::sleep(std::time::Duration::from_secs_f64(remaining + 0.03)).await;
                            Ok(())
                        }
                    }).await.unwrap_err();
                    assert!(
                        matches!(error, StoreError::Invalid("wake_source_not_current")),
                        "{error:?}"
                    );
                    assert_eq!(
                        allocated.len(),
                        1,
                        "must reach actual parameter publication"
                    );
                    let after: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.research_cycles),(SELECT count(*) FROM app.run_admissions),(SELECT count(*) FROM pgmq.q_runs)").fetch_one(&pool).await.unwrap();
                    assert_eq!(before, after);
                    for id in allocated {
                        assert!(store
                            .discard_unpublished_forward_artifact(f.project, id, |id| {
                                assert!(objects.lock().unwrap().remove(&id).is_some());
                                async { Ok(()) }
                            })
                            .await
                            .unwrap());
                    }
                    assert!(store
                        .consume_degradation_wake(
                            wake_id,
                            |_, _| async { panic!("expired source read") },
                            |_| async { panic!("expired source publish") }
                        )
                        .await
                        .unwrap()
                        .is_none());
                    let historical: (String,String,Option<uuid::Uuid>) = sqlx::query_as("SELECT o.classification,w.state,w.consumed_cycle_id FROM app.wake_events w JOIN app.degradation_observations o ON o.id=w.observation_id WHERE w.id=$1").bind(wake_id.as_uuid()).fetch_one(&pool).await.unwrap();
                    assert_eq!(historical, ("DEGRADED".into(), "CANCELLED".into(), None));
                    return;
                }
                sqlx::query("CREATE FUNCTION app.fail_wake_consume() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.state='CONSUMED' THEN RAISE EXCEPTION 'controlled consumption failure'; END IF; RETURN NEW; END $$").execute(&pool).await.unwrap();
                sqlx::query("CREATE TRIGGER fail_wake_consume BEFORE UPDATE ON app.wake_events FOR EACH ROW EXECUTE FUNCTION app.fail_wake_consume()").execute(&pool).await.unwrap();
                assert!(store
                    .consume_degradation_wake(wake_id, read, publish)
                    .await
                    .is_err());
                let after: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.research_cycles),(SELECT count(*) FROM app.run_admissions),(SELECT count(*) FROM pgmq.q_runs)").fetch_one(&pool).await.unwrap();
                assert_eq!(before, after);
                sqlx::query("DROP TRIGGER fail_wake_consume ON app.wake_events")
                    .execute(&pool)
                    .await
                    .unwrap();
                let (a, b) = tokio::join!(
                    store.consume_degradation_wake(wake_id, read, publish),
                    store.consume_degradation_wake(wake_id, read, publish)
                );
                let (a, b) = (a.unwrap().unwrap(), b.unwrap().unwrap());
                assert_eq!(a.resource, b.resource);
                assert_ne!(a.replayed, b.replayed);
                let actual: (String,uuid::Uuid,String,i64) = sqlx::query_as("SELECT c.trigger,c.wake_id,r.kind,(SELECT count(*) FROM app.command_receipts WHERE operation='CYCLE_START') FROM app.research_cycles c JOIN app.cycle_startups s ON s.cycle_id=c.id JOIN app.runs r ON r.id=s.initial_run_id WHERE c.id=$1").bind(a.resource.as_uuid()).fetch_one(&pool).await.unwrap();
                assert_eq!(
                    actual,
                    (
                        "DEGRADATION".into(),
                        wake_id.as_uuid(),
                        "DATA_VALIDATE".into(),
                        1
                    )
                );
                assert!(sqlx::query(
                    "UPDATE app.wake_events SET state='PENDING',consumed_cycle_id=NULL WHERE id=$1"
                )
                .bind(wake_id.as_uuid())
                .execute(&pool)
                .await
                .is_err());
                let replay = store
                    .consume_degradation_wake(
                        wake_id,
                        |_, _| async { panic!("replay read") },
                        |_| async { panic!("replay publish") },
                    )
                    .await
                    .unwrap()
                    .unwrap();
                assert_eq!(replay.resource, a.resource);
            }
        } else {
            assert!(matches!(
                store.consume_degradation_wake(wake_id, read, publish).await,
                Err(StoreError::Invalid("wake_human_context_required"))
            ));
            let mut correction = measurement_message.clone();
            correction.external_message_id = "measurement-correction".into();
            correction.report.message_revision = 2;
            correction.report.supersedes_message_id = Some(request.window.latest_message_ids[0]);
            store
                .submit_forward_message(&actor, &correction, read, publish)
                .await
                .unwrap();
            assert!(store
                .consume_degradation_wake(wake_id, read, publish)
                .await
                .unwrap()
                .is_none());
            let state: String = sqlx::query_scalar("SELECT state FROM app.wake_events WHERE id=$1")
                .bind(wake_id.as_uuid())
                .fetch_one(&pool)
                .await
                .unwrap();
            assert_eq!(state, "CANCELLED");
        }

        assert!(
            sqlx::query("DELETE FROM app.forward_observation_publications WHERE run_id=$1")
                .bind(job.run.id.as_uuid())
                .execute(&pool)
                .await
                .is_err()
        );
        assert!(sqlx::query("UPDATE app.forward_evidence_windows SET complete_observations=3 WHERE evaluation_id=$1").bind(a.resource.as_uuid()).execute(&pool).await.is_err());
        assert!(
            sqlx::query("DELETE FROM app.metric_values WHERE evaluation_id=$1")
                .bind(a.resource.as_uuid())
                .execute(&pool)
                .await
                .is_err()
        );
    }
    message.external_message_id = "daily-correction".into();
    message.report.message_revision = 2;
    message.report.supersedes_message_id = Some(original.id);
    message.report.returns[0].value = Some(0.02);
    store
        .submit_forward_message(&actor, &message, read, publish)
        .await
        .unwrap();
    assert!(store
        .native_job(left.resource.id, &lease.fence)
        .await
        .is_err());
    // Inject a failure after input/Run/queue creation to check the whole transaction.
    sqlx::query("CREATE FUNCTION app.fail_forward_bind() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'controlled binding failure'; END $$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER fail_forward_bind BEFORE INSERT ON app.run_native_tasks FOR EACH ROW EXECUTE FUNCTION app.fail_forward_bind()").execute(&pool).await.unwrap();
    let before: Vec<_> = objects.lock().unwrap().keys().copied().collect();
    assert!(store
        .enqueue_forward_evaluation(handoff, "daily", read, publish)
        .await
        .is_err());
    let allocated: Vec<_> = objects
        .lock()
        .unwrap()
        .keys()
        .filter(|id| !before.contains(id))
        .copied()
        .collect();
    assert_eq!(allocated.len(), 1);
    for id in allocated {
        assert!(store
            .discard_unpublished_forward_artifact(f.project, id, |id| {
                objects.lock().unwrap().remove(&id);
                async { Ok(()) }
            })
            .await
            .unwrap());
    }
    assert_eq!(objects.lock().unwrap().len(), before.len());
    let frozen: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM app.forward_evaluation_inputs WHERE project_id=$1 AND request->'request'->'window'->>'stream_id'='daily'",
    )
    .bind(f.project.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(frozen, 1);
    sqlx::query("DROP TRIGGER fail_forward_bind ON app.run_native_tasks")
        .execute(&pool)
        .await
        .unwrap();
    let second = store
        .enqueue_forward_evaluation(handoff, "daily", read, publish)
        .await
        .unwrap();
    assert_ne!(second.resource.id, left.resource.id);
    store
        .revoke_automation(
            &operator,
            "revoke",
            policy.id,
            &PolicyRevokeV1 {
                schema_version: SchemaV1,
                expected_latest_revocation_id: Some(future_revocation),
                effective_at: None,
                reason: "controlled revoke".into(),
            },
        )
        .await
        .unwrap();
    assert!(store
        .prepare_forward_evaluation(f.project)
        .await
        .unwrap()
        .is_none());
    // Historical replay keeps the original queued identity, never a fresh authorization.
    assert_eq!(
        store
            .enqueue_forward_evaluation(
                handoff,
                "daily",
                |_, _| async { panic!("replay must not reread private reports") },
                |_| async { panic!("replay must not republish parameters") }
            )
            .await
            .unwrap()
            .resource
            .id,
        second.resource.id
    );
    let messages = store.read_run_messages(60, 100).await.unwrap();
    let message = messages
        .iter()
        .find(|m| m.run_id == second.resource.id)
        .unwrap();
    let store::lifecycle::ClaimResult::Leased(lease) = store
        .claim_run(message, "revoked-forward", 60)
        .await
        .unwrap()
    else {
        panic!("durable existing Run")
    };
    assert!(store
        .native_job(second.resource.id, &lease.fence)
        .await
        .is_err());
    let snapshot = store.get_run(&operator, second.resource.id).await.unwrap();
    store
        .cancel_run(
            &operator,
            "cancel-revoked-forward",
            second.resource.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: snapshot.revision,
            },
        )
        .await
        .unwrap();
    let at: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    store
        .accept_run_terminal(
            second.resource.id,
            &lease.fence,
            &store::lifecycle::TerminalObservation {
                schema_version: SchemaV1,
                external_job_id: lease.external_job_id.clone(),
                outcome: store::lifecycle::NativeOutcome::ConfirmedAbsent,
                manifest_artifact_id: None,
                failure_class: None,
                failure_code: None,
                observed_at: at,
            },
        )
        .await
        .unwrap();
    let cancelled = store
        .publish_scientific_result(
            second.resource.id,
            |_, _| async { panic!("cancel has no result bytes") },
            publish,
        )
        .await
        .unwrap()
        .unwrap();
    let cancelled_facts:(String,String,Option<chrono::DateTime<Utc>>,i64,bool,i64)=sqlx::query_as("SELECT e.execution_status,e.evidence_status,e.valid_until,w.complete_observations,w.is_contiguous,(SELECT count(*) FROM app.metric_values WHERE evaluation_id=e.id) FROM app.evaluations e JOIN app.forward_evidence_windows w ON w.evaluation_id=e.id WHERE e.id=$1").bind(cancelled.resource.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        cancelled_facts,
        ("CANCELLED".into(), "INCOMPLETE".into(), None, 0, false, 0)
    );
    let cancelled_observation = store
        .observe_forward(second.resource.id)
        .await
        .unwrap()
        .unwrap();
    let insufficient:(String,i64)=sqlx::query_as("SELECT classification,(SELECT count(*) FROM app.wake_events WHERE observation_id=o.id) FROM app.degradation_observations o WHERE id=$1").bind(cancelled_observation.resource.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(insufficient, ("INSUFFICIENT_DATA".into(), 0));
    let generic = store::lifecycle::StandaloneRunSubmission {
        project_id: f.project,
        input_set_id: f.input_set,
        runtime_id: runtime,
        runtime_revision: contracts::Revision::INITIAL,
        kind: contracts::runs::RunKind::ForwardEvaluate,
        max_parallel_runs: 2,
        limits: contracts::lifecycle::JobLimitsV1 {
            schema_version: SchemaV1,
            experiments: 0,
            cpu_seconds: count(30),
            wall_seconds: 60,
            memory_mib: 512,
            output_bytes: count(1048576),
        },
    };
    assert!(matches!(
        store
            .enqueue_standalone_run("unregistered-forward", &generic)
            .await,
        Err(StoreError::Invalid("forward_native_input_required"))
    ));
}

// Only the human freeze/start is native here. Candidate qualification, historical
// Claim and scientific response remain explicit controlled relationship/protocol fixtures.
async fn human_source(
    pool: &PgPool,
    quota: bool,
) -> (
    cycle_support::Fixture,
    store::Store,
    store::authority::Actor,
) {
    let (store, actor) = research_support::operator(pool).await;
    let mut f = cycle_support::setup(pool, &store, &actor).await;
    let mut content = f.brief.content.clone();
    content.budget.min_cycle_interval_seconds = 15;
    content.budget.max_cycles_per_day = if quota { 1 } else { 3 };
    f.brief = store
        .update_brief(
            &actor,
            "wake-budget",
            f.brief.id,
            &contracts::brief::BriefUpdate {
                schema_version: SchemaV1,
                expected_revision: f.brief.revision,
                content,
                bindings: f.brief.bindings.clone(),
            },
        )
        .await
        .unwrap()
        .resource;
    f.freeze.expected_revision = f.brief.revision;
    store
        .freeze_brief(&actor, "wake-freeze", f.brief.id, &f.freeze, |id, size| {
            f.read(id, size)
        })
        .await
        .unwrap();
    (f, store, actor)
}
