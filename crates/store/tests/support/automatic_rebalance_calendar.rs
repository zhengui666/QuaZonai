//! Original registered calendar, controlled source declarations, real PG clock.
use super::*;
use chrono::{DateTime, Duration, Utc};
use contracts::science::{NativeCalendarSessionV1, NativeCalendarSessionsV1};

fn nanos(time: DateTime<Utc>) -> DbCounter {
    DbCounter::new(time.timestamp_nanos_opt().unwrap().try_into().unwrap()).unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn rebalance_calendar_uses_original_sessions_offset_and_version(pool: PgPool) {
    Box::pin(scenario(pool)).await;
}

async fn scenario(pool: PgPool) {
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&pool)
        .await
        .unwrap();
    let historical = DateTime::parse_from_rfc3339("2020-01-02T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let close = now + Duration::seconds(90);
    let coverage = DateTime::parse_from_rfc3339("2010-01-01T00:00:00Z")
        .unwrap()
        .with_timezone(&Utc);
    let document = NativeCalendarSessionsV1 {
        schema_version: SchemaV1,
        calendar_ref: "fixture-calendar".into(),
        calendar_version: "1".into(),
        timezone: "UTC".into(),
        source_reference: "Controlled original session table, not exchange-calendar evidence"
            .into(),
        available_at_ns: nanos(coverage),
        coverage_start_ns: nanos(coverage),
        coverage_end_ns: nanos(now + Duration::days(366)),
        sessions: [
            historical + Duration::seconds(30),
            historical + Duration::seconds(330),
            close,
        ]
        .into_iter()
        .map(|close| NativeCalendarSessionV1 {
            open_ns: nanos(close - Duration::seconds(60)),
            close_ns: nanos(close),
        })
        .collect(),
    };
    let (store, actor, f, build, candidate, _directory) = Box::pin(qualified_chain_calendar(
        pool.clone(),
        cycle_support::Liquidity::None,
        ForwardEnvironmentV1::Live,
        DataUse::ResearchAndPaper,
        release_policy,
        None,
        Some((document.clone(), -30)),
    ))
    .await
    .unwrap();
    let (seed, _, _) = Box::pin(prepare(&pool, &store, &actor, &f, &build, candidate)).await;
    Box::pin(check(&pool, &store, &actor, &f, &seed, (&document, close))).await;
}

async fn check(
    pool: &PgPool,
    store: &Store,
    actor: &store::authority::Actor,
    f: &cycle_support::Fixture,
    seed: &ReleaseViewV1,
    calendar: (&NativeCalendarSessionsV1, DateTime<Utc>),
) {
    let (document, close) = calendar;
    let (id,size): (uuid::Uuid,i64) = sqlx::query_as("SELECT a.id,a.byte_count FROM app.universe_versions u JOIN app.artifacts a ON a.id=u.calendar_artifact_id WHERE u.id=$1")
        .bind(f.data.universe.as_uuid()).fetch_one(pool).await.unwrap();
    let id: Id = id.to_string().try_into().unwrap();
    let size = DbCounter::new(size as u64).unwrap();
    let original = f.read(id, size).await.unwrap();
    assert_eq!(
        serde_json::from_slice::<NativeCalendarSessionsV1>(&original).unwrap(),
        *document
    );
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    assert!(
        now < close - Duration::seconds(30),
        "test preparation must precede its original session offset"
    );
    assert!(Box::pin(store.automate_rebalance_build(
        seed.project_id,
        |id, size| f.read(id, size),
        |_| async { panic!("no registered session has become due") }
    ))
    .await
    .unwrap()
    .is_none());
    sqlx::query("SELECT pg_sleep(GREATEST(EXTRACT(EPOCH FROM ($1::timestamptz-clock_timestamp())),0)::double precision+0.02)")
        .bind(close-Duration::seconds(30)).execute(pool).await.unwrap();
    assert!(Box::pin(store.automate_rebalance_build(
        seed.project_id,
        |id, size| f.read(id, size),
        |_| async { panic!("wall-clock passage cannot advance the old native data cutoff") }
    ))
    .await
    .unwrap()
    .is_none());
    let dataset = inputs::forward(pool, store, actor, f).await;
    let cutoff: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let input = store
        .create_input_set(
            actor,
            "calendar-forward-after-offset",
            &InputSetCreate {
                schema_version: SchemaV1,
                project_id: seed.project_id,
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
    let changed_version = |artifact, bytes| async move {
        let original = f.read(artifact, bytes).await?;
        if artifact != id {
            return Ok(original);
        }
        let changed = String::from_utf8(original)
            .unwrap()
            .replace("\"calendar_version\":\"1\"", "\"calendar_version\":\"2\"")
            .into_bytes();
        assert_eq!(changed.len() as u64, bytes.get());
        Ok(changed)
    };
    let altered = Box::pin(store.automate_rebalance_build(
        seed.project_id,
        changed_version,
        |_| async { panic!("another calendar version cannot publish a Build") },
    ))
    .await;
    assert!(altered.is_err(), "{altered:?}");
    Box::pin(change_after_publication(
        pool,
        store,
        f,
        seed.project_id,
        id,
    ))
    .await;
    let run = Box::pin(store.automate_rebalance_build(
        seed.project_id,
        |id, size| f.read(id, size),
        |object| {
            std::future::ready(
                f.objects
                    .put(object.id, &object.bytes)
                    .map_err(|_| StoreError::Integrity),
            )
        },
    ))
    .await
    .unwrap()
    .unwrap();
    assert_eq!(run.input_set_id, input);
    let actual: DateTime<Utc> =
        sqlx::query_scalar("SELECT decision_cutoff FROM app.portfolio_rebalances WHERE run_id=$1")
            .bind(run.id.as_uuid())
            .fetch_one(pool)
            .await
            .unwrap();
    assert!(actual >= close - Duration::seconds(30));
    assert!(
        actual < close,
        "negative offset qualifies before the original session close"
    );
    assert_eq!(f.read(id, size).await.unwrap(), original);
    let candidate = Box::pin(complete_stage(pool, store, f, run.id)).await;
    Box::pin(finish_calendar_chain(pool, store, f, seed, candidate)).await;
}

async fn change_after_publication(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    project: Id,
    calendar: Id,
) {
    let before: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.portfolio_rebalances),(SELECT count(*) FROM pgmq.q_runs)").fetch_one(pool).await.unwrap();
    let allocated = std::sync::Mutex::new(None);
    let recorded = &allocated;
    let changed = |id, size| async move {
        let bytes = f.read(id, size).await?;
        if id != calendar || recorded.lock().unwrap().is_none() {
            return Ok(bytes);
        }
        let mut document: NativeCalendarSessionsV1 = serde_json::from_slice(&bytes).unwrap();
        let session = document.sessions.last_mut().unwrap();
        session.close_ns = DbCounter::new(session.close_ns.get() - 1_000_000_000).unwrap();
        let changed = serde_json::to_vec(&document).unwrap();
        assert_eq!(changed.len() as u64, size.get());
        Ok(changed)
    };
    let failed = Box::pin(
        store.automate_rebalance_build(project, changed, |object| async move {
            f.objects
                .put(object.id, &object.bytes)
                .map_err(|_| StoreError::Integrity)?;
            *recorded.lock().unwrap() = Some(object.id);
            Ok(())
        }),
    )
    .await;
    assert!(matches!(failed, Err(StoreError::Integrity)), "{failed:?}");
    let after: (i64,i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.runs),(SELECT count(*) FROM app.portfolio_rebalances),(SELECT count(*) FROM pgmq.q_runs)").fetch_one(pool).await.unwrap();
    assert_eq!(before, after);
    let id = allocated
        .into_inner()
        .unwrap()
        .expect("source changed after actual parameter publication");
    assert!(store
        .discard_unpublished_forward_artifact(project, id, |id| async move {
            f.objects
                .discard_unpublished(id)
                .map_err(|_| StoreError::Integrity)
        })
        .await
        .unwrap());
}

async fn finish_calendar_chain(
    pool: &PgPool,
    store: &Store,
    f: &cycle_support::Fixture,
    seed: &ReleaseViewV1,
    candidate: Id,
) {
    let publish = |object: store::lifecycle::native::NativeObjectPublication| {
        std::future::ready(
            f.objects
                .put(object.id, &object.bytes)
                .map_err(|_| StoreError::Integrity),
        )
    };
    let study = Box::pin(store.automate_rebalance_study(
        seed.project_id,
        |id, size| f.read(id, size),
        publish,
    ))
    .await
    .unwrap()
    .unwrap();
    let evaluation = Box::pin(complete_stage(pool, store, f, study.id)).await;
    let release = Box::pin(store.automate_rebalance_release(
        seed.project_id,
        |id, size| f.read(id, size),
        publish,
    ))
    .await
    .unwrap()
    .unwrap();
    assert_eq!(
        (release.candidate_id, release.evaluation_id),
        (candidate, evaluation)
    );
    assert_ne!(release.evaluation_id, seed.evaluation_id);
    assert_ne!(release.id, seed.id);
    assert_ne!(release.package_artifact_id, seed.package_artifact_id);
    let approvals: i64 = sqlx::query_scalar("SELECT count(*) FROM app.approvals")
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(approvals, 0);
}
