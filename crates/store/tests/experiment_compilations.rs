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
    artifacts::{ArtifactCreate, ResearchArtifactKind},
    execution::NativeTaskParametersV1,
    experiments::ExperimentProposalV1,
    lifecycle::JobLimitsV1,
    runtime::{RuntimeArtifactSchemaV1, RuntimeProbeOutcomeV1, RuntimeProbeRequestV1},
    runtime_jobs::RuntimeInputV1,
    DbCounter, Id, SchemaV1,
};
use sqlx::PgPool;
use store::{
    authority::Actor,
    lifecycle::{ClaimResult, RunLease},
    runtime::ProbePreparation,
    Store, StoreError,
};

async fn upload(
    store: &Store,
    actor: &Actor,
    f: &cycle_support::Fixture,
    kind: ResearchArtifactKind,
) -> Id {
    let content = if kind == ResearchArtifactKind::Code {
        "#![no_std]\n#[unsafe(no_mangle)] pub extern \"C\" fn predict(c:f64,p:f64,_f:f64,_s:f64,_v:f64,_o:f64,_h:f64,_l:f64)->f64 { c-p }"
    } else {
        "{\"schema_version\":1,\"summary\":\"Controlled compilation input\"}"
    };
    let upload = store
        .prepare_artifact_upload(
            actor,
            &Id::new().to_string(),
            &ArtifactCreate {
                schema_version: SchemaV1,
                project_id: f.data.project,
                kind,
                content: content.into(),
            },
        )
        .await
        .unwrap();
    f.objects.put(upload.id(), content.as_bytes()).unwrap();
    upload.publish().await.unwrap().resource.id
}

async fn setup(pool: &PgPool) -> (Store, Actor, cycle_support::Fixture, RunLease, Id) {
    let (store, actor, f, cycle, preparation) = mission_support::setup(pool).await;
    mission_support::complete(pool, &store, &f, preparation, false).await;
    assert!(store.advance_initial_cycle(preparation).await.unwrap());
    let message = store.read_mission_messages(60, 1).await.unwrap().remove(0);
    let Some(ClaimResult::Leased(lease)) = store
        .claim_mission(&message, "compiler-parent", 120)
        .await
        .unwrap()
    else {
        panic!("Mission lease required")
    };
    let revision = f.freeze.execution_context.runtime_revision;
    let ProbePreparation::Pending(ticket) = store
        .prepare_runtime_probe(
            &actor,
            "compiler-capability",
            f.data.runtime,
            &RuntimeProbeRequestV1 {
                schema_version: SchemaV1,
                expected_revision: revision,
            },
        )
        .await
        .unwrap()
    else {
        panic!("probe required")
    };
    let mut capabilities = runtime_support::capabilities(chrono::Utc::now());
    for name in ["qz.data_quality", "qz.wasm_model", "qz.model_compilation"] {
        capabilities.artifact_schemas.push(RuntimeArtifactSchemaV1 {
            name: name.into(),
            version: "1".into(),
        });
    }
    let objects = f.objects.clone();
    store.complete_runtime_probe(*ticket, RuntimeProbeOutcomeV1::Available { capabilities: Box::new(capabilities) },
        move |id, bytes| async move { objects.put(id, &bytes).map_err(|_| StoreError::Integrity) }).await.unwrap();
    let family: uuid::Uuid =
        sqlx::query_scalar("SELECT family_id FROM app.evaluation_policies WHERE id=$1")
            .bind(f.brief.content.evaluation_policy_id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    let experiment = store
        .propose_experiment(
            &actor,
            "native-compilation",
            &ExperimentProposalV1 {
                schema_version: SchemaV1,
                cycle_id: cycle,
                family_id: Id::try_from(family.to_string()).unwrap(),
                parent_experiment_id: None,
                hypothesis: "Compile the exact proposed code".into(),
                expected_failure_modes: "ABI or compiler rejection is not a scientific result"
                    .into(),
                proposal_artifact_id: upload(&store, &actor, &f, ResearchArtifactKind::Report)
                    .await,
                parameter_artifact_id: upload(&store, &actor, &f, ResearchArtifactKind::Parameters)
                    .await,
                code_artifact_id: Some(
                    upload(&store, &actor, &f, ResearchArtifactKind::Code).await,
                ),
            },
        )
        .await
        .unwrap()
        .resource
        .id;
    (store, actor, f, *lease, experiment)
}

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
