//! Real disposable PostgreSQL binding and upgrade boundaries; no venue execution.
mod capital_exit_support;
use capital_exit_support::*;
use contracts::{
    DbCounter, Id, Revision, SchemaV1, account_observation::*, capital_exit::*, control::ListQuery,
    forward::ForwardEnvironmentV1,
};
use sqlx::{PgPool, Row};
use store::{Store, StoreError, authority::Actor, capital_exit::CapitalExitOwnerRegistration};

async fn another_source(
    pool: &PgPool,
    name: &str,
    session: &str,
) -> (
    Store,
    Actor,
    Actor,
    AccountObservationSubmitV1,
    AccountObservationReceiptV2,
) {
    let (store, operator, actor, mut observation) = account_setup_named(pool, name).await;
    observation.binding.native_session_id = session.into();
    let receipt = store
        .submit_client_account_observation(
            &actor,
            &AccountObservationSubmitV2 {
                schema_version: NativeClientObservationSchemaV2,
                native_client_id: "CAPITAL-TEST-CLIENT".into(),
                observation: observation.clone(),
            },
        )
        .await
        .unwrap();
    (store, operator, actor, observation, receipt)
}
fn configured_registration(receipt: &AccountObservationReceiptV2) -> CapitalExitOwnerRegistration {
    let b = &receipt.resource.observation.binding;
    CapitalExitOwnerRegistration {
        managed_account_key: "ignored-caller-key".into(),
        owner_binding_ref: "ignored-caller-owner".into(),
        downstream_id: receipt.resource.downstream_id,
        environment: ForwardEnvironmentV1::Paper,
        venue: "SIM".into(),
        native_account_id: b.native_account_id.clone(),
        native_trader_id: b.native_trader_id.clone(),
        native_client_id: receipt.native_client_id.clone(),
        collateral_currency: "USD".into(),
        instrument_id: "TEST.SIM".into(),
        controlled_strategy_ids: vec!["CAPITAL-001".into()],
    }
}
#[sqlx::test(migrations = "../../migrations")]
async fn paper_capital_exit_concurrent_cross_project_alias_cannot_duplicate_engine_budget(
    pool: PgPool,
) {
    let (a, _, actor_a, obs_a, receipt_a) =
        another_source(&pool, "alias-a", "controlled-engine-session-A").await;
    let (b, _, actor_b, obs_b, receipt_b) =
        another_source(&pool, "alias-b", "controlled-engine-session-A").await;
    assert_ne!(receipt_a.resource.source_id, receipt_b.resource.source_id);
    assert_ne!(obs_a.binding.project_id, obs_b.binding.project_id);
    let registration_a = configured_registration(&receipt_a);
    let registration_b = configured_registration(&receipt_b);
    let (one, two) = tokio::join!(
        a.register_paper_capital_exit_owner(
            &actor_a,
            receipt_a.resource.source_id,
            &obs_a.binding,
            &registration_a
        ),
        b.register_paper_capital_exit_owner(
            &actor_b,
            receipt_b.resource.source_id,
            &obs_b.binding,
            &registration_b
        )
    );
    assert_ne!(one.is_ok(), two.is_ok());
    let failure = if one.is_err() {
        one.unwrap_err()
    } else {
        two.unwrap_err()
    };
    assert!(matches!(
        failure,
        StoreError::Invalid("paper_capital_exit_engine_alias_conflict")
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.managed_capital_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}
#[sqlx::test(migrations = "../../migrations")]
async fn paper_capital_exit_independent_sessions_share_labels_without_sharing_capital(
    pool: PgPool,
) {
    let f = setup(&pool, true).await;
    let first = f.start().await;
    let (other, _, actor, observation, receipt) =
        another_source(&pool, "independent-engine", "controlled-engine-session-B").await;
    let configured = configured_registration(&receipt);
    let second = other
        .register_paper_capital_exit_owner(
            &actor,
            receipt.resource.source_id,
            &observation.binding,
            &configured,
        )
        .await
        .unwrap();
    assert_ne!(first.managed_account_key, second.managed_account_key);
    assert_eq!(
        second.managed_account_key,
        format!("paper-native:{}", receipt.resource.source_id)
    );
    let replay = f
        .store
        .register_paper_capital_exit_owner(
            &f.actor,
            f.source,
            &f.observation.binding,
            &f.registration,
        )
        .await
        .unwrap();
    assert_eq!(replay, f.registration);
    let reserve:String=sqlx::query_scalar("SELECT reserved_amount::text FROM app.managed_capital_reservations WHERE paper_account_source_id=$1").bind(f.source.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        reserve.parse::<contracts::DecimalValue>().unwrap(),
        money("100").amount
    );
    let epoch:i64=sqlx::query_scalar("SELECT account_control_epoch FROM app.managed_capital_reservations WHERE paper_account_source_id=$1").bind(f.source.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(epoch, first.account_control_epoch.get() as i64);
    let fresh:String=sqlx::query_scalar("SELECT reserved_amount::text FROM app.managed_capital_reservations WHERE paper_account_source_id=$1").bind(receipt.resource.source_id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(
        fresh.parse::<contracts::DecimalValue>().unwrap(),
        money("0").amount
    );
}
#[sqlx::test(migrations = "../../migrations")]
async fn paper_capital_exit_wrong_source_or_principal_cannot_reuse_an_owner_binding(pool: PgPool) {
    let f = setup(&pool, true).await;
    let (_, _, other_actor, other_binding, other_receipt) =
        another_source(&pool, "unrelated-owner", "controlled-engine-session-C").await;
    assert!(matches!(
        f.store
            .register_paper_capital_exit_owner(
                &f.actor,
                other_receipt.resource.source_id,
                &f.observation.binding,
                &f.registration
            )
            .await,
        Err(StoreError::NotFound)
    ));
    assert!(
        f.store
            .register_paper_capital_exit_owner(
                &other_actor,
                f.source,
                &f.observation.binding,
                &f.registration
            )
            .await
            .is_err()
    );
    let mut wrong = f.observation.binding.clone();
    wrong.native_session_id = other_binding.binding.native_session_id;
    assert!(matches!(
        f.store
            .register_paper_capital_exit_owner(&f.actor, f.source, &wrong, &f.registration)
            .await,
        Err(StoreError::Invalid("paper_capital_exit_source_mismatch"))
    ));
    let mut request = f.assessment();
    request.account_source_id = other_receipt.resource.source_id;
    request.request.account_source_id = other_receipt.resource.source_id;
    request.request.expected_source_observation_id = other_receipt.resource.id;
    assert!(
        f.store
            .submit_capital_exit_assessment(
                &other_actor,
                &request.external_message_id,
                &request,
                |_| async { Ok(()) }
            )
            .await
            .is_err()
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.managed_capital_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
}

#[sqlx::test(migrations = false)]
async fn paper_capital_exit_006_preserves_legacy_receipt_and_blocks_proven_engine_alias(
    pool: PgPool,
) {
    sqlx::migrate!("../../migrations")
        .run_to(202610070005, &pool)
        .await
        .unwrap();
    let f = setup(&pool, false).await;
    let artifact = Id::new();
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','qz.capital_exit_preview','1','LOCAL',$3,'1',1,'RESEARCH','SYNTHETIC','IMPORT','REFERENCED')").bind(artifact.as_uuid()).bind(f.observation.binding.project_id.as_uuid()).bind(artifact.to_string()).execute(&pool).await.unwrap();
    let mut registration = f.registration.clone();
    registration.managed_account_key = "legacy-label-only-account".into();
    registration.owner_binding_ref = "legacy-label-only-owner".into();
    let original_document = serde_json::json!({"schema_version":1,"registration":registration});
    sqlx::query("INSERT INTO app.managed_capital_reservations(managed_account_key,owner_binding_ref,downstream_id,environment,venue,native_account_id,native_trader_id,native_client_id,collateral_currency,owner_registration) VALUES($1,$2,$3,'PAPER',$4,$5,$6,$7,$8,$9)")
        .bind(&registration.managed_account_key).bind(&registration.owner_binding_ref).bind(registration.downstream_id.as_uuid()).bind(&registration.venue).bind(&registration.native_account_id).bind(&registration.native_trader_id).bind(&registration.native_client_id).bind(&registration.collateral_currency).bind(&original_document).execute(&pool).await.unwrap();
    let id = Id::new();
    let command_id = Id::new();
    let now = chrono::Utc::now();
    let view = CapitalExitViewV1 {
        schema_version: SchemaV1,
        id,
        project_id: f.observation.binding.project_id,
        account_source_id: f.source,
        environment: ForwardEnvironmentV1::Paper,
        managed_account_key: registration.managed_account_key.clone(),
        owner_binding_ref: registration.owner_binding_ref.clone(),
        account_control_epoch: DbCounter::new(1).unwrap(),
        account_control_revision: Revision::INITIAL.next().unwrap(),
        revision: Revision::INITIAL,
        state: CapitalExitStateV1::Requested,
        last_phase: CapitalExitStateV1::Requested,
        reason_codes: vec![],
        preview_id: Id::new(),
        plan_artifact_id: artifact,
        scope: f.request().scope,
        policy: CapitalExitPolicyV1::CashOnly {},
        command_id,
        owner_command: CapitalExitOwnerCommandV1 {
            schema_version: SchemaV1,
            command_id,
            account_control_epoch: DbCounter::new(1).unwrap(),
            instruction: CapitalExitOwnerInstructionV1::Start {},
        },
        external_claim_id: None,
        remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
        funds: CapitalExitFundsV1 {
            requested_amount: money("100"),
            reserved_amount: money("100"),
            released_cash_amount: None,
            verified_withdrawable_amount: None,
            reconciled_withdrawal_amount: money("0"),
            unreleased_amount: None,
            evidence_asof: None,
            evidence_valid_until: None,
            withdrawability: CapitalExitWithdrawabilityV1::Unverified,
        },
        evidence_refs: vec![],
        created_at: now,
        updated_at: now,
    };
    let start = CapitalExitStartV1 {
        schema_version: SchemaV1,
        preview_id: view.preview_id,
        expected_account_control_revision: Revision::INITIAL,
        acknowledged_plan_artifact_id: artifact,
        expected_source_observation_id: f.observation_id,
    };
    sqlx::query("INSERT INTO app.capital_exit_intents(id,project_id,account_source_id,managed_account_key,original_plan_artifact_id,original_request,state,command_id,content) VALUES($1,$2,$3,$4,$5,$6,'REQUESTED',$7,$8)").bind(id.as_uuid()).bind(view.project_id.as_uuid()).bind(f.source.as_uuid()).bind(&registration.managed_account_key).bind(artifact.as_uuid()).bind(serde_json::to_value(&start).unwrap()).bind(command_id.as_uuid()).bind(serde_json::to_value(&view).unwrap()).execute(&pool).await.unwrap();
    sqlx::query("UPDATE app.managed_capital_reservations SET reserved_amount=100,active_intent_id=$2,revision=2,account_control_epoch=1 WHERE managed_account_key=$1").bind(&registration.managed_account_key).bind(id.as_uuid()).execute(&pool).await.unwrap();
    let receipt = serde_json::json!({"schema_version":1,"replayed":false,"resource":view});
    sqlx::query("INSERT INTO app.command_receipts(principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status,response_nonsecret_body) VALUES('OPERATOR','CAPITAL_EXIT_START','legacy-start',$1,$2,202,$3)").bind(serde_json::json!({"schema_version":1,"project_id":view.project_id,"request":start})).bind(id.as_uuid()).bind(&receipt).execute(&pool).await.unwrap();
    // Preserve an original action receipt plus non-empty claim/fence/queue state.
    let historical_pause = CapitalExitActionV1::Pause {
        schema_version: SchemaV1,
        expected_revision: view.revision,
    };
    let mut paused = view.clone();
    paused.state = CapitalExitStateV1::Paused;
    paused.last_phase = CapitalExitStateV1::Requested;
    paused.revision = view.revision.next().unwrap();
    paused.account_control_revision = view.account_control_revision.next().unwrap();
    paused.account_control_epoch = DbCounter::new(2).unwrap();
    paused.command_id = Id::new();
    paused.owner_command = CapitalExitOwnerCommandV1 {
        schema_version: SchemaV1,
        command_id: paused.command_id,
        account_control_epoch: paused.account_control_epoch,
        instruction: CapitalExitOwnerInstructionV1::Pause {},
    };
    let pause_receipt = serde_json::json!({"schema_version":1,"replayed":false,"resource":paused});
    sqlx::query("INSERT INTO app.command_receipts(principal_scope,operation,idempotency_key,normalized_nonsecret_request,resource_id,response_status,response_nonsecret_body) VALUES('OPERATOR','CAPITAL_EXIT_ACTION','legacy-pause',$1,$2,202,$3)").bind(serde_json::json!({"schema_version":1,"id":id,"request":historical_pause})).bind(id.as_uuid()).bind(&pause_receipt).execute(&pool).await.unwrap();
    let mut durable = paused.clone();
    durable.revision = paused.revision.next().unwrap().next().unwrap();
    durable.external_claim_id = Some("legacy-owner-claim".into());
    let fence_id = Id::new();
    let fence_artifact = Id::new();
    durable.evidence_refs.push(fence_artifact);
    sqlx::query("INSERT INTO app.artifacts(id,project_id,kind,media_type,schema_name,schema_version,storage_backend,storage_object_ref,storage_version,byte_count,access_class,origin,created_by,retention_class) VALUES($1,$2,'REPORT','application/json','qz.capital_exit_owner_evidence','1','LOCAL',$3,'1',1,'RESEARCH','SYNTHETIC','IMPORT','REFERENCED')").bind(fence_artifact.as_uuid()).bind(view.project_id.as_uuid()).bind(fence_artifact.to_string()).execute(&pool).await.unwrap();
    let fence = f.evidence(
        &durable,
        "legacy-fence",
        1,
        CapitalExitEvidenceKindV1::FenceApplied {
            native_gate_report_ref: "controlled-historical-gate".into(),
            controlled_strategy_ids: registration.controlled_strategy_ids.clone(),
            invalidated_target_claim_refs: vec![],
            invalidated_child_timer_refs: vec![],
            resolved_inflight_report_ref: "controlled-historical-cache".into(),
            remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
            remaining_target_authority_ref: None,
        },
    );
    sqlx::query("INSERT INTO app.capital_exit_evidence(id,intent_id,managed_account_key,owner_binding_ref,account_source_id,record_kind,external_message_id,sequence,artifact_id,content) VALUES($1,$2,$3,$4,$5,'INTENT','legacy-fence',1,$6,$7)").bind(fence_id.as_uuid()).bind(id.as_uuid()).bind(&registration.managed_account_key).bind(&registration.owner_binding_ref).bind(f.source.as_uuid()).bind(fence_artifact.as_uuid()).bind(serde_json::to_value(&fence).unwrap()).execute(&pool).await.unwrap();
    let Actor::Machine { credential_id, .. } = &f.actor else {
        panic!("original downstream fixture")
    };
    sqlx::query("UPDATE app.capital_exit_intents SET state='PAUSED',revision=$2,command_id=$3,command_action='PAUSE',content=$4,claim_credential_id=$5,fence_evidence_id=$6 WHERE id=$1").bind(id.as_uuid()).bind(durable.revision.get() as i64).bind(durable.command_id.as_uuid()).bind(serde_json::to_value(&durable).unwrap()).bind(credential_id.as_uuid()).bind(fence_id.as_uuid()).execute(&pool).await.unwrap();
    sqlx::query("UPDATE app.managed_capital_reservations SET revision=$2,account_control_epoch=2 WHERE managed_account_key=$1").bind(&registration.managed_account_key).bind(durable.account_control_revision.get() as i64).execute(&pool).await.unwrap();
    sqlx::query("SELECT pgmq.send('capital_exits',$1::jsonb)").bind(serde_json::json!({"intent_id":id,"command_id":durable.command_id,"account_control_epoch":durable.account_control_epoch})).execute(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let after:serde_json::Value=sqlx::query_scalar("SELECT response_nonsecret_body FROM app.command_receipts WHERE operation='CAPITAL_EXIT_START' AND idempotency_key='legacy-start'").fetch_one(&pool).await.unwrap();
    assert_eq!(after, receipt);
    let persisted=sqlx::query("SELECT owner_registration,paper_account_source_id,paper_native_session_id,reserved_amount::text AS reserved FROM app.managed_capital_reservations WHERE managed_account_key=$1").bind(&registration.managed_account_key).fetch_one(&pool).await.unwrap();
    assert_eq!(
        persisted.get::<serde_json::Value, _>("owner_registration"),
        original_document
    );
    assert!(
        persisted
            .get::<Option<uuid::Uuid>, _>("paper_account_source_id")
            .is_none()
    );
    assert!(
        persisted
            .get::<Option<String>, _>("paper_native_session_id")
            .is_none()
    );
    assert_eq!(
        persisted
            .get::<String, _>("reserved")
            .parse::<contracts::DecimalValue>()
            .unwrap(),
        money("100").amount
    );
    let replay = f
        .store
        .start_capital_exit(&f.operator, view.project_id, "legacy-start", &start)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource, view);
    let before_rejected_actions = legacy_mutable_state(&pool, id).await;
    for (key, action) in [
        (
            "new-legacy-pause",
            CapitalExitActionV1::Pause {
                schema_version: SchemaV1,
                expected_revision: durable.revision,
            },
        ),
        (
            "new-legacy-cancel",
            CapitalExitActionV1::Cancel {
                schema_version: SchemaV1,
                expected_revision: durable.revision,
            },
        ),
        (
            "new-legacy-resume",
            CapitalExitActionV1::Resume {
                schema_version: SchemaV1,
                expected_revision: durable.revision,
                preview_id: Id::new(),
            },
        ),
        (
            "new-legacy-reconcile",
            CapitalExitActionV1::ReconcileWithdrawal {
                schema_version: SchemaV1,
                expected_revision: durable.revision,
                user_reported_amount: "30".parse().unwrap(),
                currency: "USD".into(),
                external_transfer_ref: Some("unverified-user-report".into()),
            },
        ),
    ] {
        assert!(matches!(
            f.store
                .act_capital_exit(&f.operator, id, key, &action)
                .await,
            Err(StoreError::Invalid(
                "paper_capital_exit_legacy_binding_unresolved"
            ))
        ));
        assert_eq!(
            legacy_mutable_state(&pool, id).await,
            before_rejected_actions,
            "rejected {key} changed command/epoch/revision/claim/fence/queue/receipts"
        );
    }
    let replayed_pause = f
        .store
        .act_capital_exit(&f.operator, id, "legacy-pause", &historical_pause)
        .await
        .unwrap();
    assert!(replayed_pause.replayed);
    assert_eq!(replayed_pause.resource, paused);
    assert_eq!(
        legacy_mutable_state(&pool, id).await,
        before_rejected_actions,
        "historical action replay must remain read-only"
    );
    let original_pause: serde_json::Value = sqlx::query_scalar("SELECT response_nonsecret_body FROM app.command_receipts WHERE operation='CAPITAL_EXIT_ACTION' AND idempotency_key='legacy-pause'").fetch_one(&pool).await.unwrap();
    assert_eq!(original_pause, pause_receipt);
    let current = f.store.capital_exit(&f.operator, id).await.unwrap();
    assert_eq!(current.state, CapitalExitStateV1::Blocked);
    assert!(
        current
            .reason_codes
            .contains(&"paper_capital_exit_legacy_binding_unresolved".into())
    );
    assert!(current.funds.verified_withdrawable_amount.is_none());
    assert!(matches!(
        f.store
            .register_paper_capital_exit_owner(
                &f.actor,
                f.source,
                &f.observation.binding,
                &f.registration
            )
            .await,
        Err(StoreError::Invalid(
            "paper_capital_exit_legacy_binding_unresolved"
        ))
    ));
    let claim = CapitalExitClaimV1 {
        schema_version: SchemaV1,
        expected_revision: view.revision,
        command_id: view.command_id,
        account_control_epoch: view.account_control_epoch,
        account_source_id: f.source,
        owner_binding_ref: view.owner_binding_ref.clone(),
        external_claim_id: "new-legacy-claim".into(),
    };
    assert!(matches!(
        f.store
            .claim_capital_exit(&f.actor, id, "new-legacy-claim", &claim)
            .await,
        Err(StoreError::Invalid(
            "paper_capital_exit_legacy_binding_unresolved"
        ))
    ));
    let (other, _, actor, obs, source) = another_source(
        &pool,
        "legacy-source-alias",
        &f.observation.binding.native_session_id,
    )
    .await;
    assert!(matches!(
        other
            .register_paper_capital_exit_owner(
                &actor,
                source.resource.source_id,
                &obs.binding,
                &configured_registration(&source)
            )
            .await,
        Err(StoreError::Invalid(
            "paper_capital_exit_legacy_binding_unresolved"
        ))
    ));
    let (new, _, actor, obs, source) = another_source(
        &pool,
        "new-independent-engine",
        "controlled-independent-engine-session",
    )
    .await;
    new.register_paper_capital_exit_owner(
        &actor,
        source.resource.source_id,
        &obs.binding,
        &configured_registration(&source),
    )
    .await
    .unwrap();
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.managed_capital_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        count, 2,
        "legacy labels cannot block a genuinely independent engine session"
    );
    let pending = f
        .store
        .downstream_capital_exits(
            &f.actor,
            &ListQuery {
                cursor: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert!(
        pending.items.is_empty(),
        "legacy claims must not be rediscovered as execution authority"
    );
}

async fn legacy_mutable_state(pool: &PgPool, intent: Id) -> serde_json::Value {
    sqlx::query_scalar("SELECT jsonb_build_object('intent',to_jsonb(i),'reservation',to_jsonb(r),'queue',(SELECT COALESCE(jsonb_agg(to_jsonb(q) ORDER BY q.msg_id),'[]'::jsonb) FROM pgmq.q_capital_exits q),'receipts',(SELECT COALESCE(jsonb_agg(to_jsonb(c) ORDER BY c.id),'[]'::jsonb) FROM app.command_receipts c WHERE c.resource_id=i.id),'evidence',(SELECT COALESCE(jsonb_agg(to_jsonb(e) ORDER BY e.id),'[]'::jsonb) FROM app.capital_exit_evidence e WHERE e.intent_id=i.id)) FROM app.capital_exit_intents i JOIN app.managed_capital_reservations r ON r.managed_account_key=i.managed_account_key WHERE i.id=$1")
        .bind(intent.as_uuid()).fetch_one(pool).await.unwrap()
}
