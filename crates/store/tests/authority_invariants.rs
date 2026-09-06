//! Actual PostgreSQL authority/identity failures, not a public authentication API.
mod support;
use contracts::Id;
use sqlx::PgPool;
use support::*;

fn sqlstate(error: sqlx::Error, expected: &str) {
    assert_eq!(
        error.as_database_error().and_then(|e| e.code()).as_deref(),
        Some(expected),
        "{error:?}"
    );
}

async fn downstream(pool: &PgPool) -> Id {
    let id = Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'fixture','https://example.invalid','fixture','{fixture}','BOTH',true)")
        .bind(id.as_uuid()).execute(pool).await.unwrap();
    id
}

async fn principal(
    pool: &PgPool,
    kind: &str,
    project: Option<Id>,
    run: Option<Id>,
    downstream: Option<Id>,
) -> Result<Id, sqlx::Error> {
    let id = Id::new();
    sqlx::query("INSERT INTO app.machine_principals(id,name,kind,project_id,run_id,downstream_id,enabled,credential_epoch) VALUES($1,'fixture',$2,$3,$4,$5,true,1)")
        .bind(id.as_uuid()).bind(kind).bind(project.map(Id::as_uuid)).bind(run.map(Id::as_uuid))
        .bind(downstream.map(Id::as_uuid)).execute(pool).await?;
    Ok(id)
}

async fn credential(
    pool: &PgPool,
    principal: Id,
    scopes: &str,
    issuer: &str,
    epoch: i64,
    seconds: i32,
) -> Result<(), sqlx::Error> {
    sqlx::query("INSERT INTO app.machine_credentials(principal_id,public_token_id,verifier_ref,principal_epoch,scope_codes,issued_at,expires_at,issued_by) VALUES($1,$2,'test-only-verifier',$3,$4::text::text[],statement_timestamp(),statement_timestamp()+$5*interval '1 second',$6)")
        .bind(principal.as_uuid()).bind(Id::new().to_string()).bind(epoch).bind(scopes)
        .bind(seconds).bind(issuer).execute(pool).await?;
    Ok(())
}

#[sqlx::test(migrations = "../../migrations")]
async fn principal_kinds_are_disjoint_and_missions_remain_project_owned(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let d = downstream(&pool).await;
    for (kind, project, run, delivery) in [
        ("MISSION", Some(f.project), None, None),
        ("MISSION", Some(f.project), Some(f.run), Some(d)),
        ("MISSION", None, Some(f.run), None),
        ("CLI", Some(f.project), Some(f.run), None),
        ("CLI", Some(f.project), None, Some(d)),
        ("AUTOMATION", Some(f.project), Some(f.run), None),
        ("DOWNSTREAM", Some(f.project), None, None),
        ("DOWNSTREAM", Some(f.project), Some(f.run), Some(d)),
    ] {
        sqlstate(
            principal(&pool, kind, project, run, delivery)
                .await
                .unwrap_err(),
            "23514",
        );
    }
    principal(&pool, "MISSION", Some(f.project), Some(f.run), None)
        .await
        .unwrap();
    principal(&pool, "DOWNSTREAM", Some(f.project), None, Some(d))
        .await
        .unwrap();
    let other = fixture(&pool, budget()).await;
    sqlstate(
        principal(&pool, "MISSION", Some(other.project), Some(f.run), None)
            .await
            .unwrap_err(),
        "23503",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn credentials_reject_unknown_null_duplicate_and_cross_kind_scopes(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let cli = principal(&pool, "CLI", Some(f.project), None, None)
        .await
        .unwrap();
    for scopes in [
        "{}",
        "{NULL}",
        "{RESEARCH_READ,NULL}",
        "{RESEARCH_READ,RESEARCH_READ}",
        "{*}",
        "{OPERATOR}",
        "{DOWNSTREAM_CLAIM}",
        "{{RESEARCH_READ,RUN_READ},{RUN_CANCEL,ARTIFACT_SUBMIT}}",
        "[0:0]={RESEARCH_READ}",
    ] {
        sqlstate(
            credential(&pool, cli, scopes, "OPERATOR", 1, 600)
                .await
                .unwrap_err(),
            "23514",
        );
    }
    credential(&pool, cli, "{RESEARCH_READ,RUN_READ}", "OPERATOR", 1, 600)
        .await
        .unwrap();
    for (issuer, epoch) in [("MISSION_SERVICE", 1), ("OPERATOR", 2)] {
        sqlstate(
            credential(&pool, cli, "{RESEARCH_READ}", issuer, epoch, 600)
                .await
                .unwrap_err(),
            "23514",
        );
    }
    let delivery = principal(
        &pool,
        "DOWNSTREAM",
        Some(f.project),
        None,
        Some(downstream(&pool).await),
    )
    .await
    .unwrap();
    sqlstate(
        credential(&pool, delivery, "{RESEARCH_READ}", "OPERATOR", 1, 600)
            .await
            .unwrap_err(),
        "23514",
    );
    credential(
        &pool,
        delivery,
        "{DOWNSTREAM_CLAIM,DOWNSTREAM_ACK,FORWARD_SUBMIT}",
        "OPERATOR",
        1,
        600,
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn global_doctor_and_mission_lifetimes_cannot_expand_authority(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let doctor = principal(&pool, "CLI", None, None, None).await.unwrap();
    credential(&pool, doctor, "{DOCTOR_READ}", "OPERATOR", 1, 600)
        .await
        .unwrap();
    sqlstate(
        credential(&pool, doctor, "{DOCTOR_READ,RUN_READ}", "OPERATOR", 1, 600)
            .await
            .unwrap_err(),
        "23514",
    );
    let mission = principal(&pool, "MISSION", Some(f.project), Some(f.run), None)
        .await
        .unwrap();
    credential(&pool, mission, "{RUN_READ}", "MISSION_SERVICE", 1, 600)
        .await
        .unwrap();
    sqlstate(
        credential(&pool, mission, "{RUN_READ}", "MISSION_SERVICE", 1, 3600)
            .await
            .unwrap_err(),
        "23514",
    );
    sqlx::query("UPDATE app.runs SET state='CANCEL_REQUESTED',cancellation_requested_at=clock_timestamp() WHERE id=$1")
        .bind(f.run.as_uuid()).execute(&pool).await.unwrap();
    sqlstate(
        credential(&pool, mission, "{RUN_READ}", "MISSION_SERVICE", 1, 600)
            .await
            .unwrap_err(),
        "23514",
    );
    sqlx::query("UPDATE app.machine_principals SET enabled=false,credential_epoch=credential_epoch+1 WHERE id=$1")
        .bind(doctor.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    // The current epoch is supplied: rejection must be caused by disabled state,
    // not by accidentally testing only a stale credential epoch.
    sqlstate(
        credential(&pool, doctor, "{DOCTOR_READ}", "OPERATOR", 2, 600)
            .await
            .unwrap_err(),
        "23514",
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn issuer_rechecks_epoch_after_a_real_principal_lock_wait(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let cli = principal(&pool, "CLI", Some(f.project), None, None)
        .await
        .unwrap();
    let mut revoke = pool.begin().await.unwrap();
    sqlx::query(
        "UPDATE app.machine_principals SET credential_epoch=credential_epoch+1 WHERE id=$1",
    )
    .bind(cli.as_uuid())
    .execute(&mut *revoke)
    .await
    .unwrap();
    let issue = credential(&pool, cli, "{RESEARCH_READ}", "OPERATOR", 1, 600);
    let release = async {
        let mut waiting = false;
        for _ in 0..500 {
            waiting = sqlx::query_scalar::<_, bool>("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE 'INSERT INTO app.machine_credentials%')")
                .fetch_one(&pool).await.unwrap();
            if waiting {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(10)).await;
        }
        assert!(waiting, "issuance must wait for the revocation transaction");
        revoke.commit().await.unwrap();
    };
    let (result, ()) = tokio::join!(issue, release);
    sqlstate(result.unwrap_err(), "23514");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.machine_credentials")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}

async fn offer(pool: &PgPool, f: &Fixture) -> (Id, Id) {
    let (mandate, candidate, evaluation) = portfolio(pool, f).await;
    let release = support::delivery_release_metadata(pool, f, mandate, candidate, evaluation)
        .await
        .unwrap();
    let delivery = downstream(pool).await;
    let evidence = support::approval_inputs(pool, f, evaluation).await;
    let approval = Id::new();
    sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) VALUES($1,$2,'PAPER',$3,'OPERATOR',$4,statement_timestamp(),statement_timestamp()+interval '1 hour')")
        .bind(approval.as_uuid()).bind(release.as_uuid()).bind(delivery.as_uuid())
        .bind(evidence.as_uuid()).execute(pool).await.unwrap();
    let offer = Id::new();
    sqlx::query("INSERT INTO app.handoff_offers(id,release_id,approval_id,downstream_id,environment,delivery_sequence,state,offered_at,expires_at) VALUES($1,$2,$3,$4,'PAPER',1,'OFFERED',statement_timestamp(),statement_timestamp()+interval '1 hour')")
        .bind(offer.as_uuid()).bind(release.as_uuid()).bind(approval.as_uuid()).bind(delivery.as_uuid())
        .execute(pool).await.unwrap();
    (offer, delivery)
}

#[sqlx::test(migrations = "../../migrations")]
async fn handoff_requires_claim_identity_and_cannot_revoke_transferred_execution(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let (offer, _) = offer(&pool, &f).await;
    sqlstate(
        sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED' WHERE id=$1")
            .bind(offer.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23514",
    );
    sqlstate(
        sqlx::query("UPDATE app.handoff_offers SET external_claim_id='external',claimed_at=clock_timestamp() WHERE id=$1")
            .bind(offer.as_uuid()).execute(&pool).await.unwrap_err(),
        "23000",
    );
    sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED',external_claim_id='external',claimed_at=clock_timestamp() WHERE id=$1")
        .bind(offer.as_uuid()).execute(&pool).await.unwrap();
    for state in ["REVOKED", "EXPIRED", "OFFERED"] {
        sqlstate(
            sqlx::query("UPDATE app.handoff_offers SET state=$2 WHERE id=$1")
                .bind(offer.as_uuid())
                .bind(state)
                .execute(&pool)
                .await
                .unwrap_err(),
            "23000",
        );
    }
    sqlstate(
        sqlx::query("UPDATE app.handoff_offers SET state='ACKNOWLEDGED',external_claim_id='other',acknowledged_at=clock_timestamp() WHERE id=$1")
            .bind(offer.as_uuid()).execute(&pool).await.unwrap_err(),
        "23000",
    );
    sqlx::query("UPDATE app.handoff_offers SET state='ACKNOWLEDGED',acknowledged_at=clock_timestamp() WHERE id=$1")
        .bind(offer.as_uuid()).execute(&pool).await.unwrap();
    sqlstate(
        sqlx::query("UPDATE app.handoff_offers SET state='REJECTED' WHERE id=$1")
            .bind(offer.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err(),
        "23000",
    );
}

async fn feedback(
    pool: &PgPool,
    offer: Id,
    downstream: Id,
    report: Id,
    revision: i32,
    predecessor: Option<Id>,
    stream: &str,
) -> Result<Id, sqlx::Error> {
    let id = Id::new();
    sqlx::query("INSERT INTO app.forward_messages(id,downstream_id,external_message_id,handoff_id,stream_id,sequence,message_revision,supersedes_message_id,window_start,window_end,coverage_status,observation_count,report_artifact_id,issued_at,received_at) VALUES($1,$2,$3,$4,$5,1,$6,$7,statement_timestamp()-interval '1 hour',statement_timestamp(),$8,10,$9,statement_timestamp(),statement_timestamp())")
        .bind(id.as_uuid()).bind(downstream.as_uuid()).bind(id.to_string()).bind(offer.as_uuid())
        .bind(stream).bind(revision).bind(predecessor.map(Id::as_uuid))
        .bind(if predecessor.is_some() { "CORRECTION" } else { "COMPLETE" })
        .bind(report.as_uuid()).execute(pool).await?;
    Ok(id)
}

#[sqlx::test(migrations = "../../migrations")]
async fn feedback_revisions_are_contiguous_unforked_and_bound_to_the_logical_stream(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let report = support::forward_report_metadata(&pool, f.project).await;
    let (offer, delivery) = offer(&pool, &f).await;
    sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED',external_claim_id='positive-control',claimed_at=clock_timestamp() WHERE id=$1")
        .bind(offer.as_uuid()).execute(&pool).await.unwrap();
    sqlstate(
        feedback(&pool, offer, delivery, report, 2, None, "nav")
            .await
            .unwrap_err(),
        "23514",
    );
    let first = feedback(&pool, offer, delivery, report, 1, None, "nav")
        .await
        .unwrap();
    sqlstate(
        feedback(&pool, offer, delivery, report, 3, Some(first), "nav")
            .await
            .unwrap_err(),
        "23514",
    );
    let second = feedback(&pool, offer, delivery, report, 2, Some(first), "nav")
        .await
        .unwrap();
    sqlstate(
        feedback(&pool, offer, delivery, report, 2, Some(first), "nav")
            .await
            .unwrap_err(),
        "23505",
    );
    sqlstate(
        feedback(&pool, offer, delivery, report, 3, Some(second), "other")
            .await
            .unwrap_err(),
        "23503",
    );
    feedback(&pool, offer, delivery, report, 3, Some(second), "nav")
        .await
        .unwrap();
}

#[sqlx::test(migrations = "../../migrations")]
async fn declared_hot_path_indexes_exist_with_the_required_key_order(pool: PgPool) {
    for (name, keys) in [
        ("projects_by_state", "(state, updated_at)"),
        ("experiments_by_family", "(family_id, ordinal)"),
        ("artifacts_by_producer_run", "(producer_run_id)"),
        (
            "evaluations_by_alpha_conclusion",
            "(subject_alpha_version_id, concluded_at DESC)",
        ),
        ("evaluations_by_candidate", "(subject_candidate_id)"),
        ("releases_by_candidate", "(candidate_id)"),
        (
            "handoffs_by_delivery_state",
            "(downstream_id, environment, state, delivery_sequence)",
        ),
    ] {
        let definition: String = sqlx::query_scalar(
            "SELECT pg_get_indexdef(i.indexrelid) FROM pg_index i JOIN pg_class c ON c.oid=i.indexrelid JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='app' AND c.relname=$1 AND i.indisvalid",
        )
        .bind(name)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert!(definition.contains(keys), "{name}: {definition}");
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn doctor_is_read_only_cli_or_automation_not_downstream_or_agent_authority(pool: PgPool) {
    let f = fixture(&pool, budget()).await;
    let d = downstream(&pool).await;
    for project in [None, Some(f.project)] {
        let delivery = principal(&pool, "DOWNSTREAM", project, None, Some(d))
            .await
            .unwrap();
        for scopes in ["{DOCTOR_READ}", "{DOWNSTREAM_CLAIM,DOCTOR_READ}"] {
            sqlstate(
                credential(&pool, delivery, scopes, "OPERATOR", 1, 600)
                    .await
                    .unwrap_err(),
                "23514",
            );
        }
        for kind in ["CLI", "AUTOMATION"] {
            let doctor = principal(&pool, kind, project, None, None).await.unwrap();
            credential(&pool, doctor, "{DOCTOR_READ}", "OPERATOR", 1, 600)
                .await
                .unwrap();
            sqlstate(
                credential(&pool, doctor, "{DOCTOR_READ,RUN_READ}", "OPERATOR", 1, 600)
                    .await
                    .unwrap_err(),
                "23514",
            );
        }
    }
    let agent = principal(&pool, "MISSION", Some(f.project), Some(f.run), None)
        .await
        .unwrap();
    for scopes in ["{DOCTOR_READ}", "{RUN_READ,DOCTOR_READ}"] {
        sqlstate(
            credential(&pool, agent, scopes, "MISSION_SERVICE", 1, 600)
                .await
                .unwrap_err(),
            "23514",
        );
    }
    // Existing valid research and delivery authority is not disabled by the fix.
    credential(&pool, agent, "{RUN_READ}", "MISSION_SERVICE", 1, 600)
        .await
        .unwrap();
    let delivery = principal(&pool, "DOWNSTREAM", Some(f.project), None, Some(d))
        .await
        .unwrap();
    credential(
        &pool,
        delivery,
        "{DOWNSTREAM_CLAIM,DOWNSTREAM_ACK,FORWARD_SUBMIT}",
        "OPERATOR",
        1,
        600,
    )
    .await
    .unwrap();
}

#[sqlx::test(migrations = false)]
async fn doctor_upgrade_requires_explicit_revocation_and_preserves_original_issuance(pool: PgPool) {
    // Native SQLx still resolves, checksums, locks and applies every migration.
    // Only the test's input directory selects the historical pre-fix version.
    let directory = std::env::temp_dir().join(format!("quazonai-migration-test-{}", Id::new()));
    std::fs::create_dir(&directory).unwrap();
    let source = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let name = entry.file_name();
        let name = name.to_str().unwrap();
        let version: i64 = name.split('_').next().unwrap().parse().unwrap();
        if version < 202609060004 {
            std::fs::copy(entry.path(), directory.join(name)).unwrap();
        }
    }
    let old = sqlx::migrate::Migrator::new(directory.as_path())
        .await
        .unwrap();
    std::fs::remove_dir_all(&directory).unwrap();
    old.run(&pool).await.unwrap();
    let delivery = principal(
        &pool,
        "DOWNSTREAM",
        None,
        None,
        Some(downstream(&pool).await),
    )
    .await
    .unwrap();
    credential(&pool, delivery, "{DOCTOR_READ}", "OPERATOR", 1, 600)
        .await
        .unwrap();
    let before: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(c) FROM app.machine_credentials c WHERE principal_id=$1",
    )
    .bind(delivery.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    let latest = sqlx::migrate!("../../migrations");
    let mut failed = pool.acquire().await.unwrap();
    match latest.run(&mut *failed).await.unwrap_err() {
        sqlx::migrate::MigrateError::ExecuteMigration(error, 202609060004) => {
            sqlstate(error, "23514");
        }
        other => panic!("unexpected migration failure: {other:?}"),
    }
    // Closing the native session also releases the migrator's advisory lock
    // after this intentionally failed migration. Do not leak it into the pool.
    failed.close().await.unwrap();
    let applied: i64 = sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(applied < 202609060004);
    sqlx::query("INSERT INTO app.machine_credential_revocations(credential_id,effective_at,reason) SELECT id,clock_timestamp(),'operator revocation for migration test' FROM app.machine_credentials WHERE principal_id=$1")
        .bind(delivery.as_uuid()).execute(&pool).await.unwrap();
    latest.run(&pool).await.unwrap();
    let after: serde_json::Value = sqlx::query_scalar(
        "SELECT to_jsonb(c) FROM app.machine_credentials c WHERE principal_id=$1",
    )
    .bind(delivery.as_uuid())
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        before, after,
        "historical issuance must remain byte-for-field equivalent"
    );
    sqlstate(
        credential(&pool, delivery, "{DOCTOR_READ}", "OPERATOR", 1, 600)
            .await
            .unwrap_err(),
        "23514",
    );
}
