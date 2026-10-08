//! Actual SQLx upgrade and Store receipt fences using controlled historical
//! relational fixtures. This does not assert model execution or market evidence.
#[path = "../../../tests/support/portfolio.rs"]
mod portfolio_config;
mod support;

use chrono::{DateTime, Duration, Utc};
use contracts::{
    control::*, delivery::*, forward::ForwardEnvironmentV1, settings::PackageSchemaVersion, Id,
    Revision, SchemaV1,
};
use serde_json::{json, Value};
use sqlx::PgPool;
use store::{authority::Actor, Store, StoreError};

const CUTOVER: i64 = 202610070002;

async fn upgrade_after_shutdown(pool: PgPool) -> PgPool {
    // Deployment stops API/Worker before the standalone migration command, then
    // starts fresh processes. Old SELECT r.*/h.* prepared plans cannot survive
    // additive columns in later migrations. Preserve the same test database and
    // SQLx pool options, including its shared connection limit and normal cache.
    let options = pool.options().clone();
    let connection_options = pool.connect_options().as_ref().clone();
    pool.close().await;
    let migrations = options
        .clone()
        .connect_with(connection_options.clone())
        .await
        .unwrap();
    Store::from_pool(migrations.clone()).migrate().await.unwrap();
    migrations.close().await;
    options.connect_with(connection_options).await.unwrap()
}

async fn historical_release(
    pool: &PgPool,
    f: &support::Fixture,
    mandate: Id,
    candidate: Id,
    evaluation: Id,
) -> (Id, Id, Vec<u8>) {
    let release = Id::new();
    let artifact = Id::new();
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(pool)
        .await
        .unwrap();
    let input: contracts::portfolio::AllocationInputV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/allocation-input.json"
    ))
    .unwrap();
    let config = portfolio_config::request(&input).mandate;
    let package = TargetPackageV1 {
        release_id: release,
        package_schema_version: PackageSchemaVersion::V1,
        environment_origin: PackageOriginV1::Real,
        project_id: f.project,
        candidate_id: candidate,
        mandate_id: mandate,
        qualification_refs: vec![Id::new(), Id::new()],
        evaluation_refs: vec![evaluation],
        input_revision_refs: vec![Id::new()],
        engine_versions: [("controlled-history".into(), "1".into())].into(),
        asof: now,
        valid_from: now,
        valid_until: now + Duration::hours(1),
        base_currency: input.base_currency.clone(),
        capital_assumption: input.capital_assumption.clone(),
        current_weights_source: contracts::portfolio::CandidateWeightsSourceV1::LastTarget,
        targets: input
            .assets
            .iter()
            .map(|asset| PackageTargetV1 {
                instrument_id: asset.instrument_id.clone(),
                target_weight: asset.current_weight.clone(),
                currency: asset.currency.clone(),
            })
            .collect(),
        cash_weight: "0".parse().unwrap(),
        constraints_summary: config.constraints,
        exposure_tolerance: input.exposure_tolerance,
        cost_assumption_ref: config.execution_assumptions_id,
        compatible_market_capabilities: vec!["fixture".into()],
        limitations: vec![
            "Controlled historical metadata; not executable scientific evidence".into(),
        ],
        provenance_artifact_refs: vec![f.report],
    };
    let bytes = serde_json::to_vec(&package).unwrap();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'PACKAGE','application/json','qz.target_package','1','LOCAL',$3,'1',$4,'DELIVERY','REAL','OPERATOR','AUDIT')")
        .bind(artifact.as_uuid()).bind(f.project.as_uuid()).bind(artifact.to_string()).bind(bytes.len() as i64).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.releases(id,candidate_id,package_artifact_id,package_schema_version,mandate_id,evaluation_id,market_capability_version,asof,valid_from,valid_until,environment) VALUES($1,$2,$3,'1',$4,$5,'fixture',$6,$6,$7,'REAL')")
        .bind(release.as_uuid()).bind(candidate.as_uuid()).bind(artifact.as_uuid()).bind(mandate.as_uuid()).bind(evaluation.as_uuid()).bind(now).bind(package.valid_until).execute(pool).await.unwrap();
    (release, artifact, bytes)
}

async fn approval_offer(
    pool: &PgPool,
    release: Id,
    downstream: Id,
    inputs: Id,
    sequence: i64,
) -> (Id, Id) {
    let approval = Id::new();
    let handoff = Id::new();
    sqlx::query("INSERT INTO app.approvals(id,release_id,environment,downstream_id,authority_kind,evidence_set_id,granted_at,valid_until) VALUES($1,$2,'PAPER',$3,'OPERATOR',$4,clock_timestamp(),clock_timestamp()+interval '30 minutes')")
        .bind(approval.as_uuid()).bind(release.as_uuid()).bind(downstream.as_uuid()).bind(inputs.as_uuid()).execute(pool).await.unwrap();
    sqlx::query("INSERT INTO app.handoff_offers(id,release_id,approval_id,downstream_id,environment,delivery_sequence,state,offered_at,expires_at) VALUES($1,$2,$3,$4,'PAPER',$5,'OFFERED',clock_timestamp(),clock_timestamp()+interval '20 minutes')")
        .bind(handoff.as_uuid()).bind(release.as_uuid()).bind(approval.as_uuid()).bind(downstream.as_uuid()).bind(sequence).execute(pool).await.unwrap();
    (approval, handoff)
}

async fn receipt(
    pool: &PgPool,
    scope: &str,
    operation: &str,
    key: &str,
    target: Id,
    request: Value,
    resource: Value,
) {
    sqlx::query("INSERT INTO app.command_receipts(principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status,response_nonsecret_body) VALUES($1,$2,$3,$4,$5,200,$6)")
        .bind(scope).bind(operation).bind(key).bind(request).bind(target.as_uuid())
        .bind(json!({"schema_version":1,"replayed":false,"resource":resource})).execute(pool).await.unwrap();
}

async fn snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar(
        "SELECT jsonb_build_object(
      'artifacts',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM app.artifacts a),
      'releases',(SELECT jsonb_agg(to_jsonb(r)-'paper_initial_weights_artifact_id' ORDER BY id) FROM app.releases r),
      'approvals',(SELECT jsonb_agg(to_jsonb(a) ORDER BY id) FROM app.approvals a),
      'handoffs',(SELECT jsonb_agg(to_jsonb(h)-'paper_claim_credential_id' ORDER BY id) FROM app.handoff_offers h),
      'transfers',(SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM app.handoff_transfers t),
      'receipts',(SELECT jsonb_agg(to_jsonb(r) ORDER BY id) FROM app.command_receipts r))",
    )
    .fetch_one(pool)
    .await
    .unwrap()
}

fn retired<T>(result: Result<T, StoreError>) {
    assert!(matches!(
        result,
        Err(StoreError::Domain(
            domain::DomainError::CapabilityUnavailable("target_package_version")
        ))
    ));
}

#[sqlx::test(migrations = false)]
async fn real_upgrade_preserves_v1_history_and_retires_active_receipt_replays(pool: PgPool) {
    sqlx::migrate!("../../migrations")
        .run_to(202610070001, &pool)
        .await
        .unwrap();
    assert!(!sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM _sqlx_migrations WHERE version=$1)"
    )
    .bind(CUTOVER)
    .fetch_one(&pool)
    .await
    .unwrap());
    let store = Store::from_pool(pool.clone());
    let actor = Actor::Browser {
        login_id: store.local_browser().await.unwrap().id,
    };
    let f = support::fixture(&pool, support::budget()).await;
    let (mandate, candidate, evaluation) = support::portfolio(&pool, &f).await;
    let (release, artifact, bytes) =
        historical_release(&pool, &f, mandate, candidate, evaluation).await;
    let (claimed_release, claimed_artifact, claimed_bytes) =
        historical_release(&pool, &f, mandate, candidate, evaluation).await;
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join(artifact.to_string());
    let claimed_path = directory.path().join(claimed_artifact.to_string());
    std::fs::write(&path, &bytes).unwrap();
    std::fs::write(&claimed_path, &claimed_bytes).unwrap();
    let downstream = Id::new();
    sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'historical','https://example.invalid','controlled-reference','{1}','PAPER',true)")
        .bind(downstream.as_uuid()).execute(&pool).await.unwrap();
    let inputs = support::approval_inputs(&pool, &f, evaluation).await;
    let (approval, offered) = approval_offer(&pool, release, downstream, inputs, 1).await;
    let (_, claimed) = approval_offer(&pool, claimed_release, downstream, inputs, 2).await;
    sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED',external_claim_id='historical-claim',claimed_at=clock_timestamp() WHERE id=$1")
        .bind(claimed.as_uuid()).execute(&pool).await.unwrap();

    let principal = store
        .create_principal(
            &actor,
            "legacy-machine",
            &PrincipalCreate {
                schema_version: SchemaV1,
                name: "Controlled historical consumer".into(),
                kind: AssignablePrincipalKind::Downstream,
                project_id: Some(f.project),
                downstream_id: Some(downstream),
                enabled: true,
            },
        )
        .await
        .unwrap()
        .resource;
    let store::control::CredentialPreparation::New(ticket) = store
        .prepare_credential_issuance(
            &actor,
            "legacy-credential",
            principal.id,
            &CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![MachineScope::DownstreamClaim, MachineScope::DownstreamAck],
                expires_at: Utc::now() + Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new fixture credential")
    };
    let verifier = Id::new();
    let credential = ticket.publish(Id::new(), verifier).await.unwrap().resource;
    let machine = Actor::Machine {
        credential_id: credential.id,
        verifier_ref: verifier,
        operator_grant: None,
    };
    let release_view = store.release(&actor, release).await.unwrap();
    let create = ReleaseCreateV1 {
        schema_version: SchemaV1,
        candidate_id: candidate,
        evaluation_id: evaluation,
    };
    let approve = ReleaseApproveV1 {
        schema_version: SchemaV1,
        downstream_id: downstream,
        environment: ForwardEnvironmentV1::Paper,
        expected_downstream_revision: Revision::INITIAL,
        expected_latest_decision_id: None,
        valid_until: release_view.valid_until,
    };
    let offer = HandoffOfferV1 {
        schema_version: SchemaV1,
        release_id: release,
        approval_id: approval,
        supersedes_handoff_id: None,
        expires_at: release_view.valid_until,
    };
    let claim = HandoffClaimV1 {
        schema_version: SchemaV1,
        external_claim_id: "historical-claim".into(),
        package_schema_version: PackageSchemaVersion::V1,
    };
    receipt(
        &pool,
        "OPERATOR",
        "RELEASE_CREATE",
        "historical-create",
        candidate,
        json!(create),
        json!(release_view),
    )
    .await;
    receipt(
        &pool,
        "OPERATOR",
        "RELEASE_APPROVE",
        "historical-approve",
        release,
        json!(approve),
        json!(store.approval(&actor, approval).await.unwrap()),
    )
    .await;
    receipt(
        &pool,
        "OPERATOR",
        "HANDOFF_OFFER",
        "historical-offer",
        approval,
        json!(offer),
        json!(store.handoff(&actor, offered).await.unwrap()),
    )
    .await;
    receipt(&pool, &format!("DOWNSTREAM:{downstream}"), "HANDOFF_CLAIM", "historical-claim", claimed,
        // Serialize the historical wire shape explicitly: the new active DTO
        // refuses to emit version 1 even in a migration fixture.
        json!({"schema_version":1,"handoff_id":claimed,"request":{"schema_version":1,"external_claim_id":"historical-claim","package_schema_version":"1"}}),
        json!({"handoff":store.handoff(&actor, claimed).await.unwrap(),"package":serde_json::from_slice::<Value>(&claimed_bytes).unwrap()})).await;
    let before = snapshot(&pool).await;
    drop(store);
    let pool = upgrade_after_shutdown(pool).await;
    let store = Store::from_pool(pool.clone());
    assert!(
        sqlx::query_scalar::<_, bool>("SELECT success FROM _sqlx_migrations WHERE version=$1")
            .bind(CUTOVER)
            .fetch_one(&pool)
            .await
            .unwrap()
    );
    assert_eq!(snapshot(&pool).await, before);
    assert!(sqlx::query_scalar::<_, bool>(
        "SELECT bool_and(paper_claim_credential_id IS NULL) FROM app.handoff_offers"
    )
    .fetch_one(&pool)
    .await
    .unwrap());
    assert!(sqlx::query_scalar::<_, bool>("SELECT bool_and(to_jsonb(r)->>'paper_initial_weights_artifact_id' IS NULL) FROM app.releases r")
        .fetch_one(&pool).await.unwrap(), "an additive Paper source column cannot adopt historical releases");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read(&claimed_path).unwrap(), claimed_bytes);
    assert_eq!(
        store
            .release(&actor, release)
            .await
            .unwrap()
            .package_schema_version,
        PackageSchemaVersion::V1
    );
    assert_eq!(
        store
            .releases_envelope(&actor, f.project, &Default::default())
            .await
            .unwrap()
            .items
            .len(),
        2
    );
    assert_eq!(
        store.handoff(&actor, claimed).await.unwrap().state,
        HandoffStateV1::Claimed
    );

    retired(
        Box::pin(store.create_release(
            &actor,
            "historical-create",
            &create,
            |_, _| async { panic!("retired replay reads nothing") },
            |_| async { panic!("retired replay publishes nothing") },
        ))
        .await,
    );
    retired(
        Box::pin(store.approve_release(
            &actor,
            "historical-approve",
            release,
            &approve,
            |_, _| async { panic!("retired approval reads nothing") },
        ))
        .await,
    );
    retired(
        Box::pin(
            store.offer_handoff(&actor, "historical-offer", &offer, |_, _| async {
                panic!("retired offer reads nothing")
            }),
        )
        .await,
    );
    retired(
        Box::pin(store.claim_handoff_envelope(
            &machine,
            "historical-claim",
            claimed,
            &claim,
            |_, _| async { panic!("retired claim receipt cannot be returned") },
        ))
        .await,
    );
    let fresh_claim = HandoffClaimV1 {
        external_claim_id: "new-v2-negotiation".into(),
        package_schema_version: PackageSchemaVersion::V2,
        ..claim.clone()
    };
    retired(
        Box::pin(store.claim_handoff_envelope(
            &machine,
            "new-v2-negotiation",
            offered,
            &fresh_claim,
            |_, _| async { panic!("V2 negotiation cannot upgrade V1 bytes") },
        ))
        .await,
    );
    assert_eq!(
        snapshot(&pool).await,
        before,
        "all refused commands leave original receipts intact"
    );

    // Rolled-back old binaries cannot write V1 even when they bypass Store.
    for (table, original) in [
        (
            "artifacts",
            before["artifacts"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == json!(artifact))
                .unwrap(),
        ),
        (
            "releases",
            before["releases"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == json!(release))
                .unwrap(),
        ),
        (
            "approvals",
            before["approvals"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == json!(approval))
                .unwrap(),
        ),
        (
            "handoff_offers",
            before["handoffs"]
                .as_array()
                .unwrap()
                .iter()
                .find(|v| v["id"] == json!(offered))
                .unwrap(),
        ),
    ] {
        let mut attempted = original.clone();
        attempted["id"] = json!(Id::new());
        if table == "handoff_offers" {
            attempted["delivery_sequence"] = json!(3);
        }
        let sql = format!(
            "INSERT INTO app.{table} SELECT (jsonb_populate_record(NULL::app.{table},$1)).*"
        );
        let error = sqlx::query(sqlx::AssertSqlSafe(sql))
            .bind(attempted)
            .execute(&pool)
            .await
            .unwrap_err();
        support::sqlstate(error, "23514");
    }
    support::sqlstate(sqlx::query("UPDATE app.handoff_offers SET state='CLAIMED',external_claim_id='old-writer-new-claim',claimed_at=clock_timestamp() WHERE id=$1")
        .bind(offered.as_uuid()).execute(&pool).await.unwrap_err(), "23514");
    assert_eq!(snapshot(&pool).await, before);

    // Safety retirement is still possible after cutover; it is not a new claim.
    store
        .revoke_approval(
            &actor,
            "legacy-revoke",
            approval,
            &ApprovalRevokeV1 {
                schema_version: SchemaV1,
                expected_latest_revocation_id: None,
                effective_at: None,
                reason_code: "V1_RETIRED".into(),
                reason: "Retire original unclaimed authority".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store.handoff(&actor, offered).await.unwrap().state,
        HandoffStateV1::Revoked
    );
    store
        .acknowledge_handoff(
            &machine,
            "legacy-ack",
            claimed,
            &HandoffAckV1 {
                schema_version: SchemaV1,
                external_ack_id: "legacy-ack".into(),
                external_claim_id: Some("historical-claim".into()),
                outcome: HandoffAckOutcomeV1::Acknowledged,
                reason_code: "ORIGINAL_RECEIPT".into(),
                reason: "Historical transfer acknowledgement".into(),
            },
        )
        .await
        .unwrap();
    assert_eq!(
        store.handoff(&actor, claimed).await.unwrap().state,
        HandoffStateV1::Acknowledged
    );
    assert_eq!(std::fs::read(path).unwrap(), bytes);
    assert_eq!(std::fs::read(claimed_path).unwrap(), claimed_bytes);
    pool.close().await;
}

#[sqlx::test(migrations = false)]
async fn historical_probe_versions_stay_readable_but_only_v2_is_currently_deliverable(
    pool: PgPool,
) {
    sqlx::migrate!("../../migrations")
        .run_to(202610070001, &pool)
        .await
        .unwrap();
    let store = Store::from_pool(pool.clone());
    let actor = Actor::Browser {
        login_id: store.local_browser().await.unwrap().id,
    };
    let mut examples = Vec::new();
    for versions in [vec!["1"], vec!["1", "2"]] {
        let downstream = Id::new();
        let artifact = Id::new();
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&pool)
            .await
            .unwrap();
        let outcome = json!({"schema_version":1,"result":{"status":"AVAILABLE","capabilities":{
            "schema_version":1,"delivery_mode":"TARGET_ONLY","accepted_package_versions":versions,
            "environments":["PAPER"],"market_capability_versions":["fixture"],"accepting_targets":true,"checked_at":now}}});
        sqlx::query("INSERT INTO app.downstream_integrations(id,name,endpoint,credential_ref,accepted_package_versions,environments,enabled) VALUES($1,'original probe','https://example.invalid',$2,$3,'PAPER',true)")
            .bind(downstream.as_uuid()).bind(Id::new().to_string()).bind(&versions).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO app.artifacts(id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,'REPORT','application/json','qz.downstream_probe','1','LOCAL',$2,'1',$3,'OPERATOR','REAL','OPERATOR','AUDIT')")
            .bind(artifact.as_uuid()).bind(artifact.to_string()).bind(serde_json::to_vec(&outcome).unwrap().len() as i64).execute(&pool).await.unwrap();
        sqlx::query("INSERT INTO app.downstream_probe_observations(downstream_id,integration_revision,snapshot_artifact_id,started_at,observed_at,valid_until,outcome) VALUES($1,1,$2,$3,$3,$3+interval '60 seconds',$4)")
            .bind(downstream.as_uuid()).bind(artifact.as_uuid()).bind(now).bind(&outcome).execute(&pool).await.unwrap();
        examples.push((downstream, versions, outcome));
    }
    drop(store);
    let pool = upgrade_after_shutdown(pool).await;
    let store = Store::from_pool(pool.clone());
    for (downstream, versions, original) in examples {
        let readiness = store
            .downstream_readiness(&actor, downstream)
            .await
            .unwrap();
        let serialized = serde_json::to_value(&readiness).unwrap();
        assert_eq!(
            serialized["latest_observation"]["outcome"],
            original["result"]
        );
        assert_eq!(
            serde_json::to_value(store.downstream(&actor, downstream).await.unwrap()).unwrap()
                ["configuration"]["accepted_package_versions"],
            json!(versions)
        );
        if versions == ["1"] {
            assert_eq!(readiness.state, DownstreamReadinessState::Unavailable);
            assert!(readiness.available_package_versions.is_empty());
            assert!(readiness.available_environments.is_empty());
        } else {
            assert_eq!(readiness.state, DownstreamReadinessState::Available);
            assert_eq!(
                readiness.available_package_versions,
                vec![PackageSchemaVersion::V2]
            );
        }
    }
    pool.close().await;
}
