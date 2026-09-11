//! Real proposal/PGMQ/native-definition transactions. No compiler or science is mocked as run.
#[path = "../../../tests/support/cycles.rs"]
mod cycle_support;
#[path = "../../../tests/support/missions.rs"]
mod mission_support;
#[path = "../../../tests/support/research.rs"]
mod research_support;
#[path = "../../../tests/support/runtime.rs"]
mod runtime_support;
use contracts::{
    execution::NativeTaskParametersV1, lifecycle::JobLimitsV1, runtime_jobs::RuntimeInputV1,
    DbCounter, Id, SchemaV1,
};
use sqlx::PgPool;
use store::{
    lifecycle::{ClaimResult, RunLease},
    Store, StoreError,
};

#[path = "../../../tests/support/experiment_tasks.rs"]
mod experiment_support;
use experiment_support::{complete_compilation, setup};

fn limits() -> JobLimitsV1 {
    JobLimitsV1 {
        schema_version: SchemaV1,
        experiments: 0,
        cpu_seconds: DbCounter::new(10).unwrap(),
        wall_seconds: 60,
        memory_mib: 1024,
        output_bytes: DbCounter::new(1024 * 1024).unwrap(),
    }
}

async fn start(
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    experiment: Id,
) -> Result<contracts::control::CommandResult<contracts::runs::RunSnapshotV1>, StoreError> {
    let objects = f.objects.clone();
    store
        .start_experiment_compilation(
            lease.run.id,
            &lease.fence,
            experiment,
            &limits(),
            move |object| async move {
                objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}

#[sqlx::test(migrations = "../../migrations")]
async fn compilation_replay_retains_one_code_producer_and_never_mounts_market_data(pool: PgPool) {
    let (store, actor, f, lease, experiment) = setup(&pool).await;
    let before: i64 =
        sqlx::query_scalar("SELECT reserved_cpu_seconds FROM app.research_cycles WHERE id=$1")
            .bind(lease.run.cycle_id.unwrap().as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let (a, b) = tokio::join!(
        start(&store, &f, &lease, experiment),
        start(&store, &f, &lease, experiment)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    assert!(a.resource.deadline_at <= lease.run.deadline_at);
    let counts: (i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.experiment_compilations WHERE experiment_id=$1),(SELECT count(*) FROM app.run_native_tasks WHERE run_id=$2),(SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$3),(SELECT reserved_cpu_seconds FROM app.research_cycles WHERE id=$4)")
        .bind(experiment.as_uuid()).bind(a.resource.id.as_uuid()).bind(a.resource.id.to_string())
        .bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1, 1, before + 10));
    let message = store
        .read_native_run_messages(60, 10)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == a.resource.id)
        .unwrap();
    let Some(ClaimResult::Leased(compiling)) = store
        .claim_native_run(&message, "native-compiler", 60)
        .await
        .unwrap()
    else {
        panic!("native compilation lease required")
    };
    let job = store
        .native_job(a.resource.id, &compiling.fence)
        .await
        .unwrap();
    assert_eq!(job.spec.inputs.len(), 2);
    assert!(job
        .spec
        .inputs
        .iter()
        .all(|input| matches!(input, RuntimeInputV1::Artifact { .. })));
    let size = job
        .spec
        .inputs
        .iter()
        .find_map(|input| match input {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &f.objects
            .read(job.spec.parameters_artifact_id, size)
            .unwrap(),
    )
    .unwrap();
    let NativeTaskParametersV1::CompileModel {
        code_artifact_id, ..
    } = task
    else {
        panic!("compile task required")
    };
    assert_eq!(
        store
            .experiment(&actor, experiment)
            .await
            .unwrap()
            .code_artifact_id,
        Some(code_artifact_id)
    );
    let changed = sqlx::query(
        "UPDATE app.experiments SET code_artifact_id=parameter_artifact_id WHERE id=$1",
    )
    .bind(experiment.as_uuid())
    .execute(&pool)
    .await
    .unwrap_err();
    assert_eq!(
        changed.as_database_error().unwrap().code().as_deref(),
        Some("23000")
    );
    assert!(
        sqlx::query("DELETE FROM app.experiment_compilations WHERE experiment_id=$1")
            .bind(experiment.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(store
        .experiment(&actor, experiment)
        .await
        .unwrap()
        .run_id
        .is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn publication_and_fence_failures_leave_no_compilation_or_resource_charge(pool: PgPool) {
    let (store, _actor, f, lease, experiment) = setup(&pool).await;
    let before: (i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.artifacts),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.run_native_tasks),(SELECT reserved_cpu_seconds FROM app.research_cycles WHERE id=$1)")
        .bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert!(matches!(
        store
            .start_experiment_compilation(
                lease.run.id,
                &lease.fence,
                experiment,
                &limits(),
                |_| async { Err(StoreError::Integrity) }
            )
            .await,
        Err(StoreError::Integrity)
    ));
    let mut stale = lease.clone();
    stale.fence.owner_epoch = stale.fence.owner_epoch.next().unwrap();
    assert!(matches!(
        start(&store, &f, &stale, experiment).await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    let mut invalid = limits();
    invalid.experiments = 1;
    assert!(matches!(
        store
            .start_experiment_compilation(
                lease.run.id,
                &lease.fence,
                experiment,
                &invalid,
                |_| async { panic!("invalid admission cannot publish") }
            )
            .await,
        Err(StoreError::Invalid("compilation_limits"))
    ));
    sqlx::raw_sql("CREATE FUNCTION public.reject_compilation_edge() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected final compilation edge failure'; END $$; CREATE TRIGGER reject_edge BEFORE INSERT ON app.experiment_compilations FOR EACH ROW EXECUTE FUNCTION public.reject_compilation_edge();")
        .execute(&pool).await.unwrap();
    let mut published = None;
    let capture = &mut published;
    let objects = f.objects.clone();
    assert!(matches!(
        store
            .start_experiment_compilation(
                lease.run.id,
                &lease.fence,
                experiment,
                &limits(),
                move |object| async move {
                    objects
                        .put(object.id, &object.bytes)
                        .map_err(|_| StoreError::Integrity)?;
                    *capture = Some((
                        object.id,
                        DbCounter::new(object.bytes.len() as u64).unwrap(),
                    ));
                    Ok(())
                }
            )
            .await,
        Err(StoreError::Database(_))
    ));
    let (id, size) =
        published.expect("native object publication preceded the injected edge failure");
    assert_eq!(f.objects.read(id, size).unwrap().len() as u64, size.get());
    sqlx::query("DROP TRIGGER reject_edge ON app.experiment_compilations")
        .execute(&pool)
        .await
        .unwrap();
    let after: (i64,i64,i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.artifacts),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.run_native_tasks),(SELECT reserved_cpu_seconds FROM app.research_cycles WHERE id=$1)")
        .bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.experiment_compilations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(
        !start(&store, &f, &lease, experiment)
            .await
            .unwrap()
            .replayed
    );
}

async fn forecast(
    store: &Store,
    f: &cycle_support::Fixture,
    lease: &RunLease,
    experiment: Id,
) -> Result<contracts::control::CommandResult<contracts::runs::RunSnapshotV1>, StoreError> {
    let reading = f.objects.clone();
    let writing = f.objects.clone();
    let mut allocation = limits();
    allocation.experiments = 1;
    store
        .start_experiment_forecast(
            lease.run.id,
            &lease.fence,
            experiment,
            &allocation,
            move |id, size| {
                let objects = reading.clone();
                async move { objects.read(id, size).map_err(|_| StoreError::Integrity) }
            },
            move |object| async move {
                writing
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity)
            },
        )
        .await
}

// Controlled result bytes prove the real publication/producer transaction, not
// execution of rustc or Wasmi. Their actual execution has separate native tests.

#[sqlx::test(migrations = "../../migrations")]
async fn forecast_requires_accepted_model_and_replay_retains_one_trial(pool: PgPool) {
    let (store, actor, f, lease, experiment) = setup(&pool).await;
    assert!(
        matches!(store.next_mission_experiment(lease.run.id, &lease.fence).await.unwrap(), Some(store::lifecycle::ExperimentWork::Compile(id)) if id == experiment)
    );
    let compilation = start(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    assert!(matches!(
        forecast(&store, &f, &lease, experiment).await,
        Err(StoreError::Invalid("accepted_compilation_required"))
    ));
    assert!(store
        .next_mission_experiment(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .is_none());
    let model = complete_compilation(&pool, &store, &f, compilation).await;
    assert!(
        matches!(store.next_mission_experiment(lease.run.id, &lease.fence).await.unwrap(), Some(store::lifecycle::ExperimentWork::Forecast(id)) if id == experiment)
    );
    let before: (i64, i64) = sqlx::query_as(
        "SELECT reserved_experiments,reserved_cpu_seconds FROM app.research_cycles WHERE id=$1",
    )
    .bind(lease.run.cycle_id.unwrap().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let (a, b) = tokio::join!(
        forecast(&store, &f, &lease, experiment),
        forecast(&store, &f, &lease, experiment)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert!(store
        .next_mission_experiment(lease.run.id, &lease.fence)
        .await
        .unwrap()
        .is_none());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    assert!(a.resource.deadline_at <= lease.run.deadline_at);
    let after: (i64, i64) = sqlx::query_as(
        "SELECT reserved_experiments,reserved_cpu_seconds FROM app.research_cycles WHERE id=$1",
    )
    .bind(lease.run.cycle_id.unwrap().as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(after, (before.0 + 1, before.1 + 10));
    let proposal = store.experiment(&actor, experiment).await.unwrap();
    assert_eq!(proposal.run_id, Some(a.resource.id));
    assert_ne!(proposal.run_id, Some(compilation));
    let counts:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.experiment_forecasts WHERE experiment_id=$1),(SELECT count(*) FROM pgmq.q_runs WHERE message->>'run_id'=$2)").bind(experiment.as_uuid()).bind(a.resource.id.to_string()).fetch_one(&pool).await.unwrap();
    assert_eq!(counts, (1, 1));
    let message = store
        .read_native_run_messages(1, 100)
        .await
        .unwrap()
        .into_iter()
        .find(|m| m.run_id == a.resource.id)
        .unwrap();
    let Some(ClaimResult::Leased(science)) = store
        .claim_native_run(&message, "forecast-native", 60)
        .await
        .unwrap()
    else {
        panic!("forecast lease required");
    };
    let job = store
        .native_job(a.resource.id, &science.fence)
        .await
        .unwrap();
    assert_eq!(job.spec.inputs.len(), 3);
    let size = job
        .spec
        .inputs
        .iter()
        .find_map(|input| match input {
            RuntimeInputV1::Artifact {
                artifact_id,
                byte_count,
                ..
            } if *artifact_id == job.spec.parameters_artifact_id => Some(*byte_count),
            _ => None,
        })
        .unwrap();
    let task: NativeTaskParametersV1 = serde_json::from_slice(
        &f.objects
            .read(job.spec.parameters_artifact_id, size)
            .unwrap(),
    )
    .unwrap();
    domain::execution::task(&job.spec, &task).unwrap();
    let NativeTaskParametersV1::EvaluateAlpha {
        dataset_revision_id,
        model_artifact_id,
        request,
        ..
    } = task
    else {
        panic!("forecast operation required");
    };
    assert_eq!(dataset_revision_id, f.data.discovery);
    assert_eq!(model_artifact_id, model);
    assert_eq!(request.parameters.label_horizon_observations, 5);
    assert!(job.spec.inputs.iter().all(|input| !matches!(
        input,
        RuntimeInputV1::Dataset {
            role: contracts::research::DataPartition::Sealed,
            ..
        }
    )));
    assert!(
        sqlx::query("UPDATE app.experiments SET run_id=$2 WHERE id=$1")
            .bind(experiment.as_uuid())
            .bind(compilation.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM app.experiment_forecasts WHERE experiment_id=$1")
            .bind(experiment.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn forecast_parameter_and_late_publication_failures_do_not_charge_a_trial(pool: PgPool) {
    let (store, _actor, f, lease, experiment) = setup(&pool).await;
    let compilation = start(&store, &f, &lease, experiment)
        .await
        .unwrap()
        .resource
        .id;
    complete_compilation(&pool, &store, &f, compilation).await;
    let before:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.artifacts),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.run_native_tasks),reserved_experiments,reserved_cpu_seconds FROM app.research_cycles WHERE id=$1").bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    let parameters: uuid::Uuid =
        sqlx::query_scalar("SELECT parameter_artifact_id FROM app.experiments WHERE id=$1")
            .bind(experiment.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    for invalid in ["dataset", "horizon", "fuel", "periods", "model", "length"] {
        let reading = f.objects.clone();
        let mut allocation = limits();
        allocation.experiments = 1;
        let result = store
            .start_experiment_forecast(
                lease.run.id,
                &lease.fence,
                experiment,
                &allocation,
                move |id, size| {
                    let objects = reading.clone();
                    async move {
                        let raw = objects.read(id, size).map_err(|_| StoreError::Integrity)?;
                        if id.as_uuid() != parameters {
                            return Ok(raw);
                        }
                        let mut value: serde_json::Value = serde_json::from_slice(&raw).unwrap();
                        match invalid {
                            "dataset" => {
                                value["dataset_revision_id"] = serde_json::json!(f.data.sealed)
                            }
                            "horizon" => {
                                value["parameters"]["label_horizon_observations"] =
                                    serde_json::json!(6)
                            }
                            "fuel" => {
                                value["parameters"]["total_fuel"] = serde_json::json!("1000000001")
                            }
                            "periods" => value["parameters"]["fast_period"] = serde_json::json!(5),
                            "model" => value["model_artifact_id"] = serde_json::json!(Id::new()),
                            _ => return Ok(Vec::new()),
                        }
                        let mut changed = serde_json::to_vec(&value).unwrap();
                        assert!(changed.len() <= raw.len());
                        changed.resize(raw.len(), b' ');
                        Ok(changed)
                    }
                },
                |_| async { panic!("invalid inputs cannot publish") },
            )
            .await;
        match invalid {
            "dataset" => assert!(matches!(
                result,
                Err(StoreError::Invalid("forecast_discovery_dataset"))
            )),
            "horizon" => assert!(matches!(
                result,
                Err(StoreError::Invalid("forecast_label_horizon"))
            )),
            "model" => assert!(matches!(
                result,
                Err(StoreError::Invalid("forecast_parameters"))
            )),
            "length" => assert!(matches!(result, Err(StoreError::Integrity))),
            _ => assert!(
                matches!(result, Err(StoreError::Domain(_))),
                "invalid {invalid} did not reach domain validation"
            ),
        }
    }
    let mut stale = lease.clone();
    stale.fence.owner_epoch = stale.fence.owner_epoch.next().unwrap();
    assert!(matches!(
        forecast(&store, &f, &stale, experiment).await,
        Err(StoreError::Domain(domain::DomainError::StaleAttempt))
    ));
    sqlx::raw_sql("CREATE FUNCTION public.reject_forecast_edge() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'injected final forecast edge failure'; END $$; CREATE TRIGGER reject_edge BEFORE INSERT ON app.experiment_forecasts FOR EACH ROW EXECUTE FUNCTION public.reject_forecast_edge();").execute(&pool).await.unwrap();
    assert!(matches!(
        forecast(&store, &f, &lease, experiment).await,
        Err(StoreError::Database(_))
    ));
    sqlx::query("DROP TRIGGER reject_edge ON app.experiment_forecasts")
        .execute(&pool)
        .await
        .unwrap();
    let after:(i64,i64,i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.artifacts),(SELECT count(*) FROM pgmq.q_runs),(SELECT count(*) FROM app.run_native_tasks),reserved_experiments,reserved_cpu_seconds FROM app.research_cycles WHERE id=$1").bind(lease.run.cycle_id.unwrap().as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    assert!(
        !forecast(&store, &f, &lease, experiment)
            .await
            .unwrap()
            .replayed
    );
}
