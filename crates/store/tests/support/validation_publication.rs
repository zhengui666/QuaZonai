//! Real PG publication and ACK boundaries with explicitly controlled scientific bytes.
use super::*;
use contracts::{control::CommandResult, runs::RunState};
use store::{authority::Actor, lifecycle::RunMessage};

pub(super) async fn prepared(
    pool: &PgPool,
) -> (Store, Actor, cycle_support::Fixture, RunLease, Id, Id) {
    let (store, actor, f, lease, experiment) = setup(pool).await;
    let compiler = start(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    complete_compilation(pool, &store, &f, compiler).await;
    let predicted = forecast(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    experiment_support::complete_forecast(pool, &store, &f, predicted).await;
    store
        .prepare_research_alpha(lease.run.id, &lease.fence, experiment)
        .await
        .unwrap();
    let run = validation(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    (store, actor, f, lease, experiment, run)
}

pub(super) async fn publish(
    store: &Store,
    f: &cycle_support::Fixture,
    run: Id,
) -> Result<CommandResult<Id>, StoreError> {
    store
        .publish_alpha_validation(
            run,
            |id, size| f.read(id, size),
            |object| {
                let objects = f.objects.clone();
                async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }
            },
        )
        .await?
        .ok_or(StoreError::Integrity)
}
pub(super) async fn message(pool: &PgPool, run: Id) -> RunMessage {
    let id: i64 = sqlx::query_scalar("SELECT msg_id FROM pgmq.q_runs WHERE message->>'run_id'=$1")
        .bind(run.to_string())
        .fetch_one(pool)
        .await
        .unwrap();
    RunMessage {
        message_id: id,
        run_id: run,
        read_count: 1,
    }
}
async fn empty(pool: &PgPool, experiment: Id) {
    let row=sqlx::query("SELECT outcome,(SELECT count(*) FROM app.evaluations) AS evaluations,(SELECT count(*) FROM app.metric_values) AS metrics,(SELECT count(*) FROM app.artifacts WHERE schema_name IN ('qz.alpha_evaluation','qz.alpha_calibration')) AS reports,(SELECT count(*) FROM app.calibrations) AS calibrations FROM app.experiments WHERE id=$1")
        .bind(experiment.as_uuid()).fetch_one(pool).await.unwrap();
    assert_eq!(row.get::<String, _>("outcome"), "PENDING");
    for name in ["evaluations", "metrics", "reports", "calibrations"] {
        assert_eq!(row.get::<i64, _>(name), 0);
    }
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.alpha_versions WHERE experiment_id=$1"
        )
        .bind(experiment.as_uuid())
        .fetch_one(pool)
        .await
        .unwrap(),
        1
    );
}
async fn decision(pool: &PgPool, evaluation: Id) -> (String, String, String) {
    sqlx::query_as(
        "SELECT execution_status,evidence_status,decision FROM app.evaluations WHERE id=$1",
    )
    .bind(evaluation.as_uuid())
    .fetch_one(pool)
    .await
    .unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn mission_cancellation_preserves_validation_publication_and_original_trial(pool: PgPool) {
    let (store, actor, f, lease, experiment, validation) = prepared(&pool).await;
    store
        .begin_run_dispatch(lease.run.id, &lease.fence)
        .await
        .unwrap();
    let parent = store.get_run(&actor, lease.run.id).await.unwrap();
    store
        .cancel_run(
            &actor,
            "cancel-parent",
            parent.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: parent.revision,
            },
        )
        .await
        .unwrap();
    assert!(!store
        .complete_research_mission(parent.id, &lease.fence)
        .await
        .unwrap());
    let child = store.get_run(&actor, validation).await.unwrap();
    assert_eq!(
        child.state,
        RunState::Queued,
        "parent cancellation never cancels another Run"
    );
    store
        .cancel_run(
            &actor,
            "explicitly-cancel-validation",
            child.id,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: child.revision,
            },
        )
        .await
        .unwrap();
    let receipt: (String, Option<uuid::Uuid>) = sqlx::query_as(
        "SELECT terminal_state,attempt_id FROM app.run_terminal_receipts WHERE run_id=$1",
    )
    .bind(validation.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(receipt, ("CANCELLED".into(), None));
    assert!(store
        .complete_research_mission(parent.id, &lease.fence)
        .await
        .unwrap());
    let observation: serde_json::Value =
        sqlx::query_scalar("SELECT observation FROM app.run_terminal_receipts WHERE run_id=$1")
            .bind(parent.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(observation["formal_evaluation_count"], "0");
    assert_ne!(observation["formal_evaluation"], "PUBLISHED");
    empty(&pool, experiment).await;
    let parent_message = message(&pool, parent.id).await;
    assert!(matches!(
        store.acknowledge_run(&parent_message).await,
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        store
            .cycle_selection(&actor, lease.run.cycle_id.unwrap())
            .await,
        Err(StoreError::NotFound)
    ));
    let pending = message(&pool, validation).await;
    assert!(matches!(
        store.acknowledge_run(&pending).await,
        Err(StoreError::Conflict)
    ));
    let evaluated = publish(&store, &f, validation).await.unwrap().resource;
    assert_eq!(
        decision(&pool, evaluated).await,
        (
            "CANCELLED".into(),
            "INCOMPLETE".into(),
            "INCONCLUSIVE".into()
        )
    );
    store.acknowledge_run(&pending).await.unwrap();
    store.acknowledge_run(&parent_message).await.unwrap();
    let selection = store
        .cycle_selection(&actor, lease.run.cycle_id.unwrap())
        .await
        .unwrap();
    assert_eq!(selection.trial_count.get(), 1);
    assert_eq!(selection.eligible_count.get(), 0);
    let rows = store
        .cycle_selection_trials(&actor, selection.cycle_id, &Default::default())
        .await
        .unwrap();
    assert_eq!(rows.items[0].evaluation_id, Some(evaluated));
    assert_eq!(
        rows.items[0].reason,
        contracts::cycles::TrialSelectionReason::ExecutionCancelled
    );
    assert_eq!(
        store.get_run(&actor, parent.id).await.unwrap().state,
        RunState::Cancelled
    );
    assert_eq!(trial_usage(&pool, &lease).await, (0, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn complete_validation_publication_is_atomic_unique_producer_bound_and_precedes_ack(
    pool: PgPool,
) {
    let (store, actor, f, lease, experiment, run) = prepared(&pool).await;
    assert!(publish(&store, &f, run).await.is_err());
    let raw = experiment_support::complete_validation(&pool, &store, &f, run, 1000, 0.8).await;
    let fixture = &f;
    let message = message(&pool, run).await;
    assert!(matches!(
        store.acknowledge_run(&message).await,
        Err(StoreError::Conflict)
    ));
    empty(&pool, experiment).await;
    assert!(store
        .publish_alpha_validation(
            run,
            |id, size| f.read(id, size),
            |_| async { Err(StoreError::Integrity) }
        )
        .await
        .is_err());
    empty(&pool, experiment).await;
    assert!(store
        .publish_alpha_validation(
            run,
            |id, size| async move {
                if id == raw {
                    Ok(Vec::new())
                } else {
                    fixture.read(id, size).await
                }
            },
            |_| async { panic!("corrupt input cannot publish") }
        )
        .await
        .is_err());
    empty(&pool, experiment).await;
    sqlx::raw_sql("CREATE FUNCTION public.reject_evaluation_metric() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected metric publication failure'; END $$; CREATE TRIGGER reject_metric BEFORE INSERT ON app.metric_values FOR EACH ROW EXECUTE FUNCTION public.reject_evaluation_metric();").execute(&pool).await.unwrap();
    let mut written = None;
    assert!(store
        .publish_alpha_validation(
            run,
            |id, size| f.read(id, size),
            |object| {
                written = Some(object.id);
                async move {
                    fixture
                        .objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }
            }
        )
        .await
        .is_err());
    empty(&pool, experiment).await;
    store
        .discard_unpublished_native_object(run, written.unwrap(), |id| async move {
            fixture
                .objects
                .discard_unpublished(id)
                .map_err(|_| StoreError::Integrity)
        })
        .await
        .unwrap();
    sqlx::raw_sql("DROP TRIGGER reject_metric ON app.metric_values; DROP FUNCTION public.reject_evaluation_metric();").execute(&pool).await.unwrap();
    // The second native object and the calibration row are part of this same
    // original publication. Neither failure can leave a successful evaluation.
    for failure in 0..4 {
        if failure == 1 {
            sqlx::raw_sql("CREATE FUNCTION public.reject_calibration() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected calibration publication failure'; END $$; CREATE TRIGGER reject_calibration BEFORE INSERT ON app.calibrations FOR EACH ROW EXECUTE FUNCTION public.reject_calibration();").execute(&pool).await.unwrap();
        }
        if failure == 2 {
            sqlx::raw_sql("CREATE FUNCTION public.change_training_time() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.fit_end_available_at := (SELECT decision_cutoff FROM app.input_sets WHERE id=NEW.train_input_set_id) + interval '1 microsecond'; RETURN NEW; END $$; CREATE TRIGGER altered_training_time BEFORE INSERT ON app.calibrations FOR EACH ROW EXECUTE FUNCTION public.change_training_time();").execute(&pool).await.unwrap();
        }
        if failure == 3 {
            sqlx::raw_sql("CREATE FUNCTION public.change_calibrated_version() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.runtime_image_ref := 'changed-after-validation'; RETURN NEW; END $$; CREATE TRIGGER altered_calibrated_version BEFORE INSERT ON app.alpha_versions FOR EACH ROW EXECUTE FUNCTION public.change_calibrated_version();").execute(&pool).await.unwrap();
        }
        let mut allocated = Vec::new();
        assert!(store
            .publish_alpha_validation(
                run,
                |id, size| f.read(id, size),
                |object| {
                    allocated.push(object.id);
                    let fail = failure == 0 && allocated.len() == 2;
                    async move {
                        if fail {
                            return Err(StoreError::Integrity);
                        }
                        fixture
                            .objects
                            .put(object.id, &object.bytes)
                            .map_err(|_| StoreError::Integrity)
                    }
                }
            )
            .await
            .is_err());
        assert_eq!(allocated.len(), 2);
        empty(&pool, experiment).await;
        for id in allocated {
            store
                .discard_unpublished_native_object(run, id, |id| async move {
                    fixture
                        .objects
                        .discard_unpublished(id)
                        .map_err(|_| StoreError::Integrity)
                })
                .await
                .unwrap();
        }
        assert!(store.acknowledge_run(&message).await.is_err());
        if failure == 1 {
            sqlx::raw_sql("DROP TRIGGER reject_calibration ON app.calibrations; DROP FUNCTION public.reject_calibration();").execute(&pool).await.unwrap();
        }
        if failure == 2 {
            sqlx::raw_sql("DROP TRIGGER altered_training_time ON app.calibrations; DROP FUNCTION public.change_training_time();").execute(&pool).await.unwrap();
        }
        if failure == 3 {
            sqlx::raw_sql("DROP TRIGGER altered_calibrated_version ON app.alpha_versions; DROP FUNCTION public.change_calibrated_version();").execute(&pool).await.unwrap();
        }
    }
    let (a, b) = tokio::join!(publish(&store, &f, run), publish(&store, &f, run));
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource, b.resource);
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(
        decision(&pool, a.resource).await,
        ("SUCCEEDED".into(), "VALID".into(), "PASS".into())
    );
    let row=sqlx::query("SELECT e.*,a.byte_count,a.access_class,a.producer_run_id,a.producer_attempt_id,r.active_attempt_id,x.outcome,x.run_id AS discovery_run,x.conclusion_artifact_id FROM app.evaluations e JOIN app.artifacts a ON a.id=e.report_artifact_id JOIN app.runs r ON r.id=e.run_id JOIN app.alpha_versions v ON v.id=e.subject_alpha_version_id JOIN app.experiments x ON x.id=v.experiment_id WHERE e.id=$1")
        .bind(a.resource.as_uuid()).fetch_one(&pool).await.unwrap();
    let report: Id = row
        .get::<uuid::Uuid, _>("report_artifact_id")
        .to_string()
        .try_into()
        .unwrap();
    let raw_report: serde_json::Value = serde_json::from_slice(
        &f.read(
            report,
            DbCounter::new(row.get::<i64, _>("byte_count") as u64).unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(row.get::<String, _>("outcome"), "SUPPORTED");
    assert_eq!(row.get::<String, _>("access_class"), "EVALUATOR_ONLY");
    assert_eq!(row.get::<uuid::Uuid, _>("producer_run_id"), run.as_uuid());
    assert_eq!(
        row.get::<uuid::Uuid, _>("producer_attempt_id"),
        row.get::<uuid::Uuid, _>("active_attempt_id")
    );
    assert_ne!(row.get::<uuid::Uuid, _>("discovery_run"), run.as_uuid());
    assert_eq!(
        row.get::<uuid::Uuid, _>("conclusion_artifact_id"),
        report.as_uuid()
    );
    assert_eq!(
        row.get::<uuid::Uuid, _>("method_versions_artifact_id"),
        report.as_uuid()
    );
    assert_eq!(raw_report["native_report_artifact_id"], raw.to_string());
    assert_eq!(raw_report["source_observations"], "1000");
    assert_eq!(raw_report["native_versions"]["solow-cv"], "0.7.3");
    let fitted = sqlx::query("SELECT c.*,a.byte_count,a.producer_run_id,a.producer_attempt_id,a.kind,a.access_class,a.origin FROM app.calibrations c JOIN app.artifacts a ON a.id=c.model_artifact_id WHERE c.validation_evaluation_id=$1")
        .bind(a.resource.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        fitted.get::<String, _>("estimator_kind"),
        "linregress.affine_ols"
    );
    assert_eq!(fitted.get::<String, _>("estimator_version"), "0.5.4");
    assert_eq!(fitted.get::<String, _>("output_unit"), "RETURN_PER_HORIZON");
    assert_eq!(fitted.get::<String, _>("kind"), "MODEL");
    assert_eq!(fitted.get::<String, _>("access_class"), "EVALUATOR_ONLY");
    assert_eq!(fitted.get::<String, _>("origin"), "FIXTURE");
    assert_eq!(
        fitted.get::<uuid::Uuid, _>("producer_run_id"),
        run.as_uuid()
    );
    assert_eq!(
        fitted.get::<uuid::Uuid, _>("producer_attempt_id"),
        row.get::<uuid::Uuid, _>("active_attempt_id")
    );
    assert_eq!(
        fitted.get::<uuid::Uuid, _>("train_input_set_id"),
        row.get::<uuid::Uuid, _>("input_set_id")
    );
    let model_artifact: Id = fitted
        .get::<uuid::Uuid, _>("model_artifact_id")
        .to_string()
        .try_into()
        .unwrap();
    let model: contracts::science::NativeFrozenCalibrationV1 = serde_json::from_slice(
        &f.read(
            model_artifact,
            DbCounter::new(fitted.get::<i64, _>("byte_count") as u64).unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    assert_eq!(model.source_report_artifact_id, raw);
    domain::execution::check_alpha_calibration(&model).unwrap();
    let fit_time = fitted.get::<chrono::DateTime<chrono::Utc>, _>("fit_end_available_at");
    assert_eq!(
        fit_time.timestamp_micros() as u64 * 1000,
        model.fit_end_available_ns.get() + 999
    );
    let native: contracts::science::NativeAlphaValidationResultV1 = serde_json::from_slice(
        &f.read(
            raw,
            DbCounter::new(
                sqlx::query_scalar::<_, i64>("SELECT byte_count FROM app.artifacts WHERE id=$1")
                    .bind(raw.as_uuid())
                    .fetch_one(&pool)
                    .await
                    .unwrap() as u64,
            )
            .unwrap(),
        )
        .await
        .unwrap(),
    )
    .unwrap();
    for asset in &model.assets {
        let last = native
            .folds
            .iter()
            .rfind(|fold| fold.instrument_id == asset.instrument_id)
            .unwrap();
        assert_eq!(asset.fold_index, last.fold_index);
        assert_eq!(asset.training_ordinals, last.training_ordinals);
        assert_eq!(
            asset.training_end_available_ns,
            last.training_end_available_ns
        );
        assert_eq!(
            asset.calibration.intercept,
            last.calibration.as_ref().unwrap().intercept
        );
        assert_eq!(
            asset.calibration.slope,
            last.calibration.as_ref().unwrap().slope
        );
    }
    assert!(store.artifact(&actor, model_artifact).await.is_err());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.calibrations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
    for query in ["UPDATE app.calibrations SET estimator_version='changed'", "DELETE FROM app.calibrations",
        "INSERT INTO app.calibrations(estimator_kind,estimator_version,model_artifact_id,train_input_set_id,fit_end_available_at,output_unit,horizon_kind,horizon_value,validation_evaluation_id) SELECT estimator_kind,estimator_version,model_artifact_id,train_input_set_id,fit_end_available_at,output_unit,horizon_kind,horizon_value,validation_evaluation_id FROM app.calibrations"] {
        assert!(sqlx::query(query).execute(&pool).await.is_err());
    }
    assert!(raw_report.get("folds").is_none());
    assert!(store.artifact(&actor, report).await.is_err());
    let public = store.experiment(&actor, experiment).await.unwrap();
    assert_eq!(
        public.result_visibility,
        contracts::experiments::ExperimentResultVisibility::Research
    );
    assert_eq!(
        public.outcome,
        Some(contracts::experiments::ExperimentOutcome::Supported)
    );
    assert_eq!(public.conclusion_artifact_id, Some(report));
    let counts:(i64,i64,i64)=sqlx::query_as("SELECT count(*),count(*) FILTER(WHERE source_artifact_id=$2),count(DISTINCT scope) FROM app.metric_values WHERE evaluation_id=$1")
        .bind(a.resource.as_uuid()).bind(raw.as_uuid()).fetch_one(&pool).await.unwrap();
    assert!(counts.0 > 2);
    assert_eq!(counts.0, counts.1);
    assert_eq!(counts.0, counts.2 * 2);
    // Public reads project the committed original aggregate without opening a
    // byte reader, publishing another evaluation, or consuming another trial.
    let list = contracts::control::ListQuery {
        limit: 1,
        cursor: None,
    };
    let header = store.evaluation(&actor, a.resource).await.unwrap();
    assert_eq!(header.run_id, run);
    assert_eq!(header.report_artifact_id, report);
    assert_eq!(header.origin, contracts::research::DataOrigin::Fixture);
    assert!(header.unexpired_at_read);
    assert_eq!(
        header.valid_until,
        row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("valid_until")
    );
    assert_eq!(
        header.concluded_at,
        row.get::<chrono::DateTime<chrono::Utc>, _>("concluded_at")
    );
    let alphas = store
        .alphas(
            &actor,
            &contracts::research::ResearchListQuery {
                project_id: f.data.project,
                limit: 1,
                cursor: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(alphas.items.len(), 1);
    let alpha = &alphas.items[0];
    let versions = store.alpha_versions(&actor, alpha.id, &list).await.unwrap();
    assert_eq!(versions.items.len(), 1);
    assert!(versions.next_cursor.is_some());
    let derived = &versions.items[0];
    assert_eq!(
        derived.version,
        contracts::Revision::INITIAL.next().unwrap()
    );
    assert_eq!(alpha.active_version_id, Some(derived.id));
    let version = store
        .alpha_version(&actor, alpha.id, contracts::Revision::INITIAL)
        .await
        .unwrap();
    assert_eq!(Some(version.id), header.subject_alpha_version_id);
    assert_eq!(version.experiment_id, experiment);
    assert_eq!(
        version.origin,
        Some(contracts::research::DataOrigin::Fixture)
    );
    assert_eq!(version.calibration_id, None);
    let mut original_fields = serde_json::to_value(&version).unwrap();
    let mut derived_fields = serde_json::to_value(derived).unwrap();
    for field in ["id", "version", "calibration_id", "created_at"] {
        original_fields.as_object_mut().unwrap().remove(field);
        derived_fields.as_object_mut().unwrap().remove(field);
    }
    assert_eq!(original_fields, derived_fields);
    assert!(matches!(
        store.alpha_calibration(&actor, version.id).await,
        Err(StoreError::NotFound)
    ));
    let calibration = store.alpha_calibration(&actor, derived.id).await.unwrap();
    assert_eq!(Some(calibration.id), derived.calibration_id);
    assert_eq!(calibration.alpha_version_id, derived.id);
    assert_eq!(calibration.model_artifact_id, model_artifact);
    assert_eq!(calibration.train_input_set_id, header.input_set_id);
    assert_eq!(calibration.fit_end_available_at, fit_time);
    assert_eq!(calibration.horizon_value, derived.horizon_value.unwrap());
    assert_eq!(calibration.validation.id, header.id);
    assert_eq!(
        calibration.validation.subject_alpha_version_id,
        Some(version.id)
    );
    assert_eq!(calibration.validation.valid_until, header.valid_until);
    assert_eq!(calibration.validation.concluded_at, header.concluded_at);
    assert!(store
        .alpha_evaluations(&actor, derived.id, &list)
        .await
        .unwrap()
        .items
        .is_empty());
    let cli =
        proposal_support::machine(&pool, f.data.project, None, "CLI", &["RESEARCH_READ"]).await;
    assert_eq!(
        store.alpha_calibration(&cli, derived.id).await.unwrap().id,
        calibration.id
    );
    let verifier = Id::new();
    let credential = store
        .issue_mission_credential(lease.run.id, &lease.fence, Id::new(), verifier)
        .await
        .unwrap();
    let mission = Actor::Machine {
        credential_id: credential,
        verifier_ref: verifier,
        operator_grant: None,
    };
    assert!(matches!(
        store.alpha_calibration(&mission, derived.id).await,
        Err(StoreError::Forbidden)
    ));
    for (kind, scopes) in [
        ("CLI", vec!["EXPERIMENT_SUBMIT"]),
        ("AUTOMATION", vec!["RESEARCH_READ"]),
    ] {
        let reader = proposal_support::machine(&pool, f.data.project, None, kind, &scopes).await;
        assert!(matches!(
            store.alpha_calibration(&reader, derived.id).await,
            Err(StoreError::Forbidden)
        ));
    }
    let other = research_support::setup(&pool, &store, &actor).await;
    let foreign =
        proposal_support::machine(&pool, other.project, None, "CLI", &["RESEARCH_READ"]).await;
    assert!(matches!(
        store.alpha_calibration(&foreign, derived.id).await,
        Err(StoreError::NotFound)
    ));
    for query in [
        "UPDATE app.alpha_versions SET calibration_id=NULL WHERE calibration_id IS NOT NULL",
        "DELETE FROM app.alpha_versions WHERE calibration_id IS NOT NULL",
        "INSERT INTO app.alpha_versions(project_id,alpha_id,version,experiment_id,root_lineage_id,code_artifact_id,model_artifact_id,signal_contract_version,signal_kind,horizon_kind,horizon_value,forecast_unit,calibration_id,runtime_image_ref) SELECT project_id,alpha_id,version+1,experiment_id,root_lineage_id,code_artifact_id,model_artifact_id,signal_contract_version,signal_kind,horizon_kind,horizon_value,forecast_unit,calibration_id,runtime_image_ref FROM app.alpha_versions WHERE calibration_id IS NOT NULL",
    ] {
        assert!(sqlx::query(query).execute(&pool).await.is_err());
    }
    let evaluations = store
        .alpha_evaluations(&actor, version.id, &list)
        .await
        .unwrap();
    assert_eq!(evaluations.items.len(), 1);
    assert_eq!(evaluations.items[0].id, header.id);
    assert!(evaluations.next_cursor.is_none());
    let mut query = list.clone();
    let mut seen = std::collections::BTreeSet::new();
    loop {
        let page = store
            .evaluation_metrics(&actor, header.id, &query)
            .await
            .unwrap();
        assert_eq!(page.items.len(), 1);
        let metric = &page.items[0];
        assert_eq!(metric.evaluation_id, header.id);
        assert_eq!(metric.source_artifact_id, raw);
        assert!(seen.insert((metric.metric_code.clone(), metric.scope.clone())));
        let original = sqlx::query("SELECT * FROM app.metric_values WHERE evaluation_id=$1 AND metric_code=$2 AND scope=$3")
            .bind(header.id.as_uuid()).bind(&metric.metric_code).bind(&metric.scope).fetch_one(&pool).await.unwrap();
        assert_eq!(metric.value, original.get::<Option<f64>, _>("value"));
        assert_eq!(
            metric.observation_count.get() as i64,
            original.get::<i64, _>("observation_count")
        );
        assert_eq!(
            metric.method_version,
            original.get::<String, _>("method_version")
        );
        assert_eq!(
            metric.period_start,
            original.get::<chrono::DateTime<chrono::Utc>, _>("period_start")
        );
        assert_eq!(
            metric.period_end,
            original.get::<chrono::DateTime<chrono::Utc>, _>("period_end")
        );
        assert_eq!(metric.unit, original.get::<String, _>("unit"));
        if let Some(cursor) = page.next_cursor {
            assert_eq!(cursor.as_uuid(), original.get::<uuid::Uuid, _>("id"));
            query.cursor = Some(cursor);
        } else {
            break;
        }
    }
    assert_eq!(seen.len() as i64, counts.0);
    assert_eq!(trial_usage(&pool, &lease).await, (0, 1));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.qualifications")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    let replay = store
        .publish_alpha_validation(
            run,
            |_, _| async { panic!("replay cannot reread") },
            |_| async { panic!("replay cannot publish") },
        )
        .await
        .unwrap()
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, a.resource);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.alpha_versions WHERE experiment_id=$1"
        )
        .bind(experiment.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    let duplicate = sqlx::query("INSERT INTO app.evaluations(project_id,subject_alpha_version_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until) SELECT project_id,subject_alpha_version_id,input_set_id,policy_id,run_id,evaluation_kind,execution_status,evidence_status,decision,report_artifact_id,method_versions_artifact_id,concluded_at,valid_until FROM app.evaluations WHERE id=$1")
        .bind(a.resource.as_uuid()).execute(&pool).await.unwrap_err();
    assert_eq!(
        duplicate.as_database_error().unwrap().code().as_deref(),
        Some("23505")
    );
    assert!(sqlx::query(
        "UPDATE app.experiments SET outcome='REJECTED',revision=revision+1 WHERE id=$1"
    )
    .bind(experiment.as_uuid())
    .execute(&pool)
    .await
    .is_err());
    assert!(
        sqlx::query("UPDATE app.metric_values SET value=0 WHERE evaluation_id=$1")
            .bind(a.resource.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    store.acknowledge_run(&message).await.unwrap();
    store.acknowledge_run(&message).await.unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn successful_validation_with_missing_registered_rows_is_not_a_pass(pool: PgPool) {
    let (store, _, f, _, _, run) = prepared(&pool).await;
    experiment_support::complete_validation(&pool, &store, &f, run, 900, 0.8).await;
    let result = publish(&store, &f, run).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.calibrations")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        decision(&pool, result.resource).await,
        (
            "SUCCEEDED".into(),
            "INCOMPLETE".into(),
            "INCONCLUSIVE".into()
        )
    );
}

async fn keeps_alpha_state(pool: &PgPool, lifecycle: &str, clear_active: bool) {
    let (store, actor, f, _, experiment, run) = prepared(pool).await;
    let (alpha, source): (uuid::Uuid, uuid::Uuid) =
        sqlx::query_as("SELECT alpha_id,id FROM app.alpha_versions WHERE experiment_id=$1")
            .bind(experiment.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    sqlx::query("UPDATE app.alphas SET lifecycle=$2,active_version_id=CASE WHEN $3 THEN NULL ELSE active_version_id END,revision=revision+1 WHERE id=$1")
        .bind(alpha).bind(lifecycle).bind(clear_active).execute(pool).await.unwrap();
    let before: (String, Option<uuid::Uuid>, i64) =
        sqlx::query_as("SELECT lifecycle,active_version_id,revision FROM app.alphas WHERE id=$1")
            .bind(alpha)
            .fetch_one(pool)
            .await
            .unwrap();
    experiment_support::complete_validation(pool, &store, &f, run, 1000, 0.8).await;
    publish(&store, &f, run).await.unwrap();
    let after: (String, Option<uuid::Uuid>, i64) =
        sqlx::query_as("SELECT lifecycle,active_version_id,revision FROM app.alphas WHERE id=$1")
            .bind(alpha)
            .fetch_one(pool)
            .await
            .unwrap();
    assert_eq!(
        before, after,
        "publication cannot undo a later Operator choice"
    );
    let derived: uuid::Uuid = sqlx::query_scalar("SELECT id FROM app.alpha_versions WHERE alpha_id=$1 AND version=2 AND calibration_id IS NOT NULL")
        .bind(alpha).fetch_one(pool).await.unwrap();
    assert_eq!(
        store
            .alpha_calibration(&actor, derived.to_string().try_into().unwrap())
            .await
            .unwrap()
            .validation
            .subject_alpha_version_id
            .map(Id::as_uuid),
        Some(source)
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn calibrated_version_preserves_a_cleared_active_pointer(pool: PgPool) {
    keeps_alpha_state(&pool, "RESEARCH", true).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn calibrated_version_does_not_unsuspend_alpha(pool: PgPool) {
    keeps_alpha_state(&pool, "SUSPENDED", false).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn calibrated_version_does_not_unretire_alpha(pool: PgPool) {
    keeps_alpha_state(&pool, "RETIRED", false).await;
}

#[sqlx::test(migrations = "../../migrations")]
async fn registered_missing_fraction_boundary_is_exact_and_a_failed_metric_rejects(pool: PgPool) {
    let (store, actor, f, _, experiment, run) = prepared(&pool).await;
    experiment_support::complete_validation(&pool, &store, &f, run, 950, 0.05).await;
    let result = publish(&store, &f, run).await.unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM app.calibrations WHERE validation_evaluation_id=$1"
        )
        .bind(result.resource.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap(),
        1
    );
    assert_eq!(
        decision(&pool, result.resource).await,
        ("SUCCEEDED".into(), "VALID".into(), "REJECT".into())
    );
    let derived: uuid::Uuid = sqlx::query_scalar(
        "SELECT id FROM app.alpha_versions WHERE experiment_id=$1 AND version=2",
    )
    .bind(experiment.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let calibrated = store
        .alpha_calibration(&actor, derived.to_string().try_into().unwrap())
        .await
        .unwrap();
    assert_eq!(
        calibrated.validation.decision,
        contracts::evidence::Decision::Reject
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.qualifications")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn cancelled_unsubmitted_validation_publishes_without_inventing_a_native_report(
    pool: PgPool,
) {
    let (store, actor, f, _, _, run) = prepared(&pool).await;
    let cancelled = store
        .cancel_run(
            &actor,
            "cancel-validation",
            run,
            &contracts::lifecycle::RunCancelV1 {
                schema_version: SchemaV1,
                expected_revision: store.get_run(&actor, run).await.unwrap().revision,
            },
        )
        .await
        .unwrap();
    assert_eq!(cancelled.resource.state, RunState::Cancelled);
    let result = store
        .publish_alpha_validation(
            run,
            |_, _| async { panic!("no native report exists") },
            |object| {
                let objects = f.objects.clone();
                async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        decision(&pool, result.resource).await,
        (
            "CANCELLED".into(),
            "INCOMPLETE".into(),
            "INCONCLUSIVE".into()
        )
    );
    let header = store.evaluation(&actor, result.resource).await.unwrap();
    assert_eq!(
        header.execution_status,
        contracts::runtime_jobs::RuntimeResultState::Cancelled
    );
    assert_eq!(
        header.evidence_status,
        contracts::evidence::EvidenceStatus::Incomplete
    );
    assert_eq!(header.decision, contracts::evidence::Decision::Inconclusive);
    assert_eq!(header.valid_until, None);
    assert!(!header.unexpired_at_read);
    assert!(store
        .evaluation_metrics(
            &actor,
            header.id,
            &contracts::control::ListQuery {
                limit: 1,
                cursor: None
            }
        )
        .await
        .unwrap()
        .items
        .is_empty());
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.metric_values")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    store
        .acknowledge_run(&message(&pool, run).await)
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn rejected_native_manifest_still_has_one_inconclusive_evaluation(pool: PgPool) {
    let (store, _, f, _, _, run) = prepared(&pool).await;
    let message = message(&pool, run).await;
    let Some(ClaimResult::Leased(lease)) = store
        .claim_native_run(&message, "invalid-manifest", 60)
        .await
        .unwrap()
    else {
        panic!("lease required")
    };
    let job = store.native_job(run, &lease.fence).await.unwrap();
    store.begin_run_dispatch(run, &lease.fence).await.unwrap();
    let now = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let status = contracts::runtime_jobs::RuntimeJobStatusV1 {
        schema_version: SchemaV1,
        run_id: run,
        attempt_no: 1,
        external_job_id: job.spec.external_job_id,
        state: contracts::runtime_jobs::RuntimeJobState::Failed,
        submitted_at: job.submitted_not_before,
        started_at: Some(job.submitted_not_before),
        finished_at: Some(now),
        has_result: true,
    };
    store
        .reject_native_manifest(
            run,
            &lease.fence,
            &status,
            store::lifecycle::native::NativeManifestFailure::Contract,
        )
        .await
        .unwrap();
    let result = store
        .publish_alpha_validation(
            run,
            |_, _| async { panic!("unusable manifest is not evidence") },
            |object| {
                let objects = f.objects.clone();
                async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)
                }
            },
        )
        .await
        .unwrap()
        .unwrap();
    assert_eq!(
        decision(&pool, result.resource).await,
        ("FAILED".into(), "INCOMPLETE".into(), "INCONCLUSIVE".into())
    );
    store.acknowledge_run(&message).await.unwrap();
}
