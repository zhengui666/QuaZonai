//! PostgreSQL evidence/authority tests. These synthetic envelopes are not native execution acceptance.
mod capital_exit_support;
use capital_exit_support::*;
use contracts::{
    DbCounter, Id, SchemaV1, account_observation::*, capital_exit::*, control::ListQuery,
};
use sqlx::{PgPool, Row};
use store::StoreError;

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_missing_owner_is_blocked_and_preview_cannot_reserve(pool: PgPool) {
    let f = setup(&pool, false).await;
    let preview = f.preview("unbound-preview").await;
    assert_eq!(preview.capability, CapitalExitCapabilityV1::Blocked);
    assert_eq!(preview.managed_account_key, None);
    assert_eq!(preview.funds.verified_idle_cash, None);
    assert!(
        preview
            .reason_codes
            .contains(&"account_owner_binding_unavailable".into())
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.capital_exit_intents")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.managed_capital_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
    assert!(matches!(
        f.store
            .preview_capital_exit(
                &f.actor,
                f.observation.binding.project_id,
                "machine-preview",
                &f.request(),
                |_| async { Ok(()) }
            )
            .await,
        Err(StoreError::Forbidden)
    ));
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_start_replay_preserves_one_account_reserve_and_physical_alias_is_rejected(
    pool: PgPool,
) {
    let f = setup(&pool, true).await;
    let view = f.start().await;
    let receipt:serde_json::Value=sqlx::query_scalar("SELECT normalized_nonsecret_request FROM app.command_receipts WHERE operation='CAPITAL_EXIT_START'").fetch_one(&pool).await.unwrap();
    let request: CapitalExitStartV1 = serde_json::from_value(receipt["request"].clone()).unwrap();
    let replay = f
        .store
        .start_capital_exit(
            &f.operator,
            f.observation.binding.project_id,
            "start",
            &request,
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, view.id);
    assert!(matches!(
        f.store
            .start_capital_exit(
                &f.operator,
                f.observation.binding.project_id,
                "different-start",
                &request
            )
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    let mut alias = f.registration.clone();
    alias.managed_account_key = "different-project-key".into();
    alias.owner_binding_ref = "different-owner-alias".into();
    assert!(f.store.register_capital_exit_owner(&alias).await.is_err());
    let row=sqlx::query("SELECT reserved_amount::text AS amount,account_control_epoch FROM app.managed_capital_reservations").fetch_one(&pool).await.unwrap();
    let amount: contracts::DecimalValue = row.get::<String, _>("amount").parse().unwrap();
    assert_eq!(amount, money("100").amount);
    assert_eq!(row.get::<i64, _>("account_control_epoch"), 1);
    assert!(
        sqlx::query("UPDATE app.capital_exit_intents SET original_request='{}'::jsonb WHERE id=$1")
            .bind(view.id.as_uuid())
            .execute(&pool)
            .await
            .is_err()
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_old_epoch_cannot_report_ready_and_pause_keeps_reserve(pool: PgPool) {
    let f = setup(&pool, true).await;
    let view = f.start().await;
    let claimed = f.claim(&view, "claim-1").await;
    let unfenced = f.evidence(
        &claimed,
        "unfenced",
        1,
        CapitalExitEvidenceKindV1::WithdrawabilityObserved {
            available_cash: money("1000"),
            basis: CapitalExitAvailabilityBasisV1::ControlledSandbox,
            venue: "SIM".into(),
            native_availability_report_ref: "fixture-availability".into(),
            settlement_report_ref: "fixture-settlement".into(),
            margin_report_ref: "cash-no-margin".into(),
            open_orders_report_ref: "fixture-no-openers".into(),
        },
    );
    assert!(matches!(
        f.store
            .submit_capital_exit_evidence(&f.actor, view.id, "unfenced", &unfenced, |_| async {
                Ok(())
            })
            .await,
        Err(StoreError::Invalid(
            "capital_exit_owner_fence_unacknowledged"
        ))
    ));
    let fenced = f.fence(&claimed, 1).await;
    let mut available = unfenced;
    available.sequence = DbCounter::new(2).unwrap();
    available.external_message_id = "available".into();
    let ready = f
        .store
        .submit_capital_exit_evidence(&f.actor, view.id, "available", &available, |_| async {
            Ok(())
        })
        .await
        .unwrap()
        .resource;
    assert_eq!(
        ready.funds.withdrawability,
        CapitalExitWithdrawabilityV1::Simulated
    );
    assert_eq!(ready.funds.verified_withdrawable_amount, Some(money("100")));
    let repeated = f
        .store
        .submit_capital_exit_evidence(&f.actor, view.id, "available", &available, |_| async {
            panic!("replay must not publish")
        })
        .await
        .unwrap();
    assert!(repeated.replayed);
    let mut changed = available.clone();
    changed.native_evidence = serde_json::json!({"changed":true});
    assert!(matches!(
        f.store
            .submit_capital_exit_evidence(&f.actor, view.id, "available", &changed, |_| async {
                Ok(())
            })
            .await,
        Err(StoreError::IdempotencyConflict)
    ));
    let paused = f
        .store
        .act_capital_exit(
            &f.operator,
            view.id,
            "pause",
            &CapitalExitActionV1::Pause {
                schema_version: SchemaV1,
                expected_revision: ready.revision,
            },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(paused.funds.reserved_amount, money("100"));
    assert_eq!(paused.funds.verified_withdrawable_amount, None);
    assert!(paused.account_control_epoch > fenced.account_control_epoch);
    available.external_message_id = "old-epoch-new-message".into();
    available.sequence = DbCounter::new(3).unwrap();
    assert!(
        f.store
            .submit_capital_exit_evidence(
                &f.actor,
                view.id,
                &available.external_message_id,
                &available,
                |_| async { Ok(()) }
            )
            .await
            .is_err()
    );
    let claimed = f.claim(&paused, "claim-pause").await;
    let fenced = f.fence(&claimed, 3).await;
    let progress = f.evidence(
        &fenced,
        "pause-wait",
        4,
        CapitalExitEvidenceKindV1::NativeProgress {
            phase: CapitalExitStateV1::WaitingEvidence,
            released_cash_amount: Some(money("50")),
            native_order_refs: vec![],
            native_position_refs: vec![],
            native_report_ref: "fixture-native-pause-pending".into(),
            reason_codes: vec![],
        },
    );
    let paused = f
        .store
        .submit_capital_exit_evidence(&f.actor, view.id, "pause-wait", &progress, |_| async {
            Ok(())
        })
        .await
        .unwrap()
        .resource;
    assert_eq!(paused.state, CapitalExitStateV1::Paused);
    assert_eq!(paused.funds.reserved_amount, money("100"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_user_report_and_balance_drop_do_not_reconcile_without_cash_movement(
    pool: PgPool,
) {
    let mut f = setup(&pool, true).await;
    let view = f.start().await;
    let reported = f
        .store
        .act_capital_exit(
            &f.operator,
            view.id,
            "reported-withdrawal",
            &CapitalExitActionV1::ReconcileWithdrawal {
                schema_version: SchemaV1,
                expected_revision: view.revision,
                user_reported_amount: "30".parse().unwrap(),
                currency: "USD".into(),
                external_transfer_ref: Some("transfer-30".into()),
            },
        )
        .await
        .unwrap()
        .resource;
    let before = f.observation_id;
    let mut lower = f.observation.clone();
    lower.sequence = DbCounter::new(2).unwrap();
    let ns = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
    lower.observed_at_ns = DbCounter::new(ns).unwrap();
    let snapshot = lower.snapshot.as_mut().unwrap();
    snapshot.event_id = "60a29b32-9e54-4f36-9304-0f0a8b16ce92".into();
    snapshot.ts_event = DbCounter::new(ns - 2).unwrap();
    snapshot.ts_init = DbCounter::new(ns - 1).unwrap();
    snapshot.balances[0].total = money("970.25");
    snapshot.balances[0].free = money("970.25");
    snapshot.total_equity = vec![money("970.25")];
    let receipt = f
        .store
        .submit_client_account_observation(
            &f.actor,
            &AccountObservationSubmitV2 {
                schema_version: NativeClientObservationSchemaV2,
                native_client_id: f.registration.native_client_id.clone(),
                observation: lower,
            },
        )
        .await
        .unwrap();
    f.observation_id = receipt.resource.id;
    let read = f.store.capital_exit(&f.operator, view.id).await.unwrap();
    assert_eq!(read.state, CapitalExitStateV1::ReconcilingWithdrawal);
    assert_eq!(read.funds.reserved_amount, money("100"));
    assert_eq!(read.funds.reconciled_withdrawal_amount, money("0"));
    let claimed = f.claim(&reported, "reconcile-claim").await;
    let fenced = f.fence(&claimed, 1).await;
    let evidence = f.evidence(
        &fenced,
        "native-transfer-30",
        2,
        CapitalExitEvidenceKindV1::WithdrawalReconciled {
            amount: money("30"),
            native_cash_movement_ref: "native-movement-30".into(),
            external_transfer_ref: "transfer-30".into(),
            before_observation_id: before,
            after_observation_id: f.observation_id,
            observed_managed_capital_after: money("970.25"),
        },
    );
    let reconciled = f
        .store
        .submit_capital_exit_evidence(
            &f.actor,
            view.id,
            &evidence.external_message_id,
            &evidence,
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(reconciled.funds.reserved_amount, money("70"));
    assert_eq!(reconciled.funds.reconciled_withdrawal_amount, money("30"));
    let replay = f
        .store
        .submit_capital_exit_evidence(
            &f.actor,
            view.id,
            &evidence.external_message_id,
            &evidence,
            |_| async { panic!("no second publication") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.funds.reserved_amount, money("70"));
    let mut second_movement = evidence.clone();
    second_movement.external_message_id = "same-report-different-movement".into();
    second_movement.sequence = DbCounter::new(3).unwrap();
    if let CapitalExitEvidenceKindV1::WithdrawalReconciled {
        native_cash_movement_ref,
        ..
    } = &mut second_movement.evidence
    {
        *native_cash_movement_ref = "different-native-movement".into();
    }
    assert!(matches!(
        f.store
            .submit_capital_exit_evidence(
                &f.actor,
                view.id,
                &second_movement.external_message_id,
                &second_movement,
                |_| async { panic!("consumed report must not publish") }
            )
            .await,
        Err(StoreError::Invalid(
            "capital_exit_withdrawal_report_consumed"
        ))
    ));
    let basis: serde_json::Value = sqlx::query_scalar(
        "SELECT observed_managed_capital_after FROM app.managed_capital_reservations",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(
        basis,
        serde_json::json!({"schema_version":1,"money":money("970.25"),"source_observation_id":f.observation_id})
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_assessment_is_read_only_and_unregistered_session_cannot_borrow_binding(
    pool: PgPool,
) {
    let f = setup(&pool, true).await;
    let blocked = f.preview("pending-preview").await;
    assert_eq!(blocked.capability, CapitalExitCapabilityV1::Blocked);
    let pending = f
        .store
        .downstream_capital_exit_assessments(
            &f.actor,
            &ListQuery {
                cursor: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(pending.items[0].id, blocked.id);
    let view = f.start().await;
    let mut restart = f.observation.clone();
    restart.binding.native_session_id = "new-native-session".into();
    let new = f
        .store
        .submit_client_account_observation(
            &f.actor,
            &AccountObservationSubmitV2 {
                schema_version: NativeClientObservationSchemaV2,
                native_client_id: f.registration.native_client_id.clone(),
                observation: restart,
            },
        )
        .await
        .unwrap();
    let mut assessment = f.assessment();
    assessment.external_message_id = "restart-assessment".into();
    assessment.expected_account_control_revision = view.account_control_revision;
    assessment.account_source_id = new.resource.source_id;
    assessment.request.account_source_id = new.resource.source_id;
    assessment.request.expected_source_observation_id = new.resource.id;
    assert!(matches!(
        f.store
            .submit_capital_exit_assessment(
                &f.actor,
                &assessment.external_message_id,
                &assessment,
                |_| async { Ok(()) }
            )
            .await,
        Err(StoreError::Invalid("account_owner_binding_unavailable"))
    ));
    let amount: String =
        sqlx::query_scalar("SELECT reserved_amount::text FROM app.managed_capital_reservations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(
        amount.parse::<contracts::DecimalValue>().unwrap(),
        money("100").amount
    );
    assert!(matches!(
        f.store
            .downstream_capital_exits(
                &f.operator,
                &ListQuery {
                    cursor: None,
                    limit: 10
                }
            )
            .await,
        Err(StoreError::Forbidden)
    ));
    let _ = Id::new();
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_concurrent_start_claims_one_reservation_and_evidence_stays_on_claim_credential(
    pool: PgPool,
) {
    let f = setup(&pool, true).await;
    let assessment = f.assessment();
    f.store
        .submit_capital_exit_assessment(
            &f.actor,
            &assessment.external_message_id,
            &assessment,
            |_| async { Ok(()) },
        )
        .await
        .unwrap();
    let preview = f.preview("concurrent-preview").await;
    let request = CapitalExitStartV1 {
        schema_version: SchemaV1,
        preview_id: preview.id,
        expected_account_control_revision: preview.expected_account_control_revision.unwrap(),
        acknowledged_plan_artifact_id: preview.plan_artifact_id,
        expected_source_observation_id: f.observation_id,
    };
    let (a, b) = tokio::join!(
        f.store.start_capital_exit(
            &f.operator,
            f.observation.binding.project_id,
            "concurrent-start",
            &request
        ),
        f.store.start_capital_exit(
            &f.operator,
            f.observation.binding.project_id,
            "concurrent-start",
            &request
        )
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_eq!(a.resource.id, b.resource.id);
    assert_ne!(a.replayed, b.replayed);
    let claimed = f.claim(&a.resource, "original-claim").await;
    let store::authority::Actor::Machine { credential_id, .. } = &f.actor else {
        panic!("machine fixture")
    };
    let principal: uuid::Uuid =
        sqlx::query_scalar("SELECT principal_id FROM app.machine_credentials WHERE id=$1")
            .bind(credential_id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    let principal = Id::try_from(principal.to_string()).unwrap();
    let verifier = Id::new();
    let store::control::CredentialPreparation::New(prepared) = f
        .store
        .prepare_credential_issuance(
            &f.operator,
            "second-owner-credential",
            principal,
            &contracts::control::CredentialIssue {
                schema_version: SchemaV1,
                scope_codes: vec![contracts::control::MachineScope::ForwardSubmit],
                expires_at: chrono::Utc::now() + chrono::Duration::hours(1),
            },
        )
        .await
        .unwrap()
    else {
        panic!("new credential")
    };
    let credential = prepared
        .publish(Id::new(), verifier)
        .await
        .unwrap()
        .resource
        .id;
    let other = store::authority::Actor::Machine {
        credential_id: credential,
        verifier_ref: verifier,
        operator_grant: None,
    };
    let evidence = f.evidence(
        &claimed,
        "different-credential-event",
        1,
        CapitalExitEvidenceKindV1::FenceApplied {
            native_gate_report_ref: "fixture-native-gate".into(),
            controlled_strategy_ids: f.registration.controlled_strategy_ids.clone(),
            invalidated_target_claim_refs: vec![],
            invalidated_child_timer_refs: vec![],
            resolved_inflight_report_ref: "fixture-resolved-native".into(),
            remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
            remaining_target_authority_ref: None,
        },
    );
    assert!(matches!(
        f.store
            .submit_capital_exit_evidence(
                &other,
                claimed.id,
                &evidence.external_message_id,
                &evidence,
                |_| async { Ok(()) }
            )
            .await,
        Err(StoreError::Forbidden)
    ));
}

#[sqlx::test(migrations = false)]
async fn capital_exit_additive_migration_preserves_preexisting_original_receipt(pool: PgPool) {
    sqlx::migrate!("../../migrations")
        .run_to(202610070002, &pool)
        .await
        .unwrap();
    let store = store::Store::from_pool(pool.clone());
    let login = store.local_browser().await.unwrap();
    let actor = store::authority::Actor::Browser { login_id: login.id };
    let request = contracts::control::ProjectCreate {
        schema_version: SchemaV1,
        name: "Before capital control".into(),
        description: "Original immutable receipt migration fixture".into(),
        fork_from_project_id: None,
    };
    let original = store
        .create_project(&actor, "pre-capital-exit-project", &request)
        .await
        .unwrap();
    let before:serde_json::Value=sqlx::query_scalar("SELECT response_nonsecret_body FROM app.command_receipts WHERE operation='PROJECT_CREATE' AND resource_id=$1").bind(original.resource.id.as_uuid()).fetch_one(&pool).await.unwrap();
    sqlx::migrate!("../../migrations").run(&pool).await.unwrap();
    let after:serde_json::Value=sqlx::query_scalar("SELECT response_nonsecret_body FROM app.command_receipts WHERE operation='PROJECT_CREATE' AND resource_id=$1").bind(original.resource.id.as_uuid()).fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    let replay = store
        .create_project(&actor, "pre-capital-exit-project", &request)
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(replay.resource.id, original.resource.id);
    let capital_tables:i64=sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema='app' AND table_name IN ('capital_exit_intents','managed_capital_reservations','capital_exit_evidence')").fetch_one(&pool).await.unwrap();
    assert_eq!(capital_tables, 3);
}

fn availability() -> CapitalExitEvidenceKindV1 {
    CapitalExitEvidenceKindV1::WithdrawabilityObserved {
        available_cash: money("1000"),
        basis: CapitalExitAvailabilityBasisV1::ControlledSandbox,
        venue: "SIM".into(),
        native_availability_report_ref: "controlled-native-availability".into(),
        settlement_report_ref: "controlled-native-settlement".into(),
        margin_report_ref: "controlled-native-cash-account".into(),
        open_orders_report_ref: "controlled-native-open-orders".into(),
    }
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_versioned_documents_and_owner_command_reconcile_are_discoverable(
    pool: PgPool,
) {
    let f = setup(&pool, true).await;
    let owner_document: serde_json::Value =
        sqlx::query_scalar("SELECT owner_registration FROM app.managed_capital_reservations")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(owner_document["schema_version"], 1);
    assert_eq!(
        owner_document["registration"],
        serde_json::to_value(&f.registration).unwrap()
    );
    let initial = f.start().await;
    let reported = f
        .store
        .act_capital_exit(
            &f.operator,
            initial.id,
            "typed-reconciliation",
            &CapitalExitActionV1::ReconcileWithdrawal {
                schema_version: SchemaV1,
                expected_revision: initial.revision,
                user_reported_amount: "30".parse().unwrap(),
                currency: "USD".into(),
                external_transfer_ref: Some("original-transfer-30".into()),
            },
        )
        .await
        .unwrap()
        .resource;
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
    let command = &pending.items[0].owner_command;
    assert_eq!(command.command_id, reported.command_id);
    assert_eq!(
        command.account_control_epoch,
        reported.account_control_epoch
    );
    assert_eq!(
        command.instruction,
        CapitalExitOwnerInstructionV1::ReconcileWithdrawal {
            user_reported_amount: "30".parse().unwrap(),
            currency: "USD".into(),
            external_transfer_ref: Some("original-transfer-30".into())
        }
    );
    let claim = f.claim(&reported, "typed-report-claim").await;
    assert_eq!(claim.owner_command, *command);
    let missing_versions:i64=sqlx::query_scalar("SELECT count(*) FROM app.command_receipts WHERE operation LIKE 'CAPITAL_EXIT_%' AND normalized_nonsecret_request->'schema_version' IS DISTINCT FROM '1'::jsonb").fetch_one(&pool).await.unwrap();
    assert_eq!(missing_versions, 0);
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_live_environment_revocation_invalidates_readiness_before_expiry(
    pool: PgPool,
) {
    let f = setup_environment(&pool, true, contracts::forward::ForwardEnvironmentV1::Live).await;
    let initial = f.start().await;
    let claimed = f.claim(&initial, "live-claim").await;
    let fenced = f.fence(&claimed, 1).await;
    let mut kind = availability();
    if let CapitalExitEvidenceKindV1::WithdrawabilityObserved { basis, .. } = &mut kind {
        *basis = CapitalExitAvailabilityBasisV1::NativeVenueAvailable;
    }
    let observed = f.evidence(&fenced, "live-availability", 2, kind);
    let ready = f
        .store
        .submit_capital_exit_evidence(
            &f.actor,
            initial.id,
            &observed.external_message_id,
            &observed,
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(
        ready.funds.withdrawability,
        CapitalExitWithdrawabilityV1::Verified
    );
    assert_eq!(ready.funds.verified_withdrawable_amount, Some(money("100")));
    sqlx::query("UPDATE app.downstream_integrations SET environments='PAPER' WHERE id=$1")
        .bind(f.registration.downstream_id.as_uuid())
        .execute(&pool)
        .await
        .unwrap();
    let current = f.store.capital_exit(&f.operator, initial.id).await.unwrap();
    assert!(current.funds.evidence_valid_until.unwrap() > chrono::Utc::now());
    assert_eq!(current.state, CapitalExitStateV1::Blocked);
    assert_eq!(current.funds.verified_withdrawable_amount, None);
    assert_eq!(
        current.funds.withdrawability,
        CapitalExitWithdrawabilityV1::Stale
    );
    let page = f
        .store
        .capital_exits(
            &f.operator,
            initial.project_id,
            &ListQuery {
                cursor: None,
                limit: 10,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items[0].funds.verified_withdrawable_amount, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_fresh_fence_response_cannot_republish_old_snapshot_availability(
    pool: PgPool,
) {
    let mut f = setup(&pool, true).await;
    let initial = f.start().await;
    let claim = f.claim(&initial, "source-claim").await;
    let fenced = f.fence(&claim, 1).await;
    let evidence = f.evidence(&fenced, "available-A", 2, availability());
    let ready = f
        .store
        .submit_capital_exit_evidence(
            &f.actor,
            initial.id,
            &evidence.external_message_id,
            &evidence,
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    assert!(ready.funds.verified_withdrawable_amount.is_some());
    let mut newer = f.observation.clone();
    newer.sequence = DbCounter::new(2).unwrap();
    let ns = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
    newer.observed_at_ns = DbCounter::new(ns).unwrap();
    let snapshot = newer.snapshot.as_mut().unwrap();
    snapshot.event_id = "60a29b32-9e54-4f36-9304-0f0a8b16ce93".into();
    snapshot.ts_event = DbCounter::new(ns - 2).unwrap();
    snapshot.ts_init = DbCounter::new(ns - 1).unwrap();
    let source = f
        .store
        .submit_client_account_observation(
            &f.actor,
            &AccountObservationSubmitV2 {
                schema_version: NativeClientObservationSchemaV2,
                native_client_id: f.registration.native_client_id.clone(),
                observation: newer,
            },
        )
        .await
        .unwrap();
    f.observation_id = source.resource.id;
    let response = f.fence(&ready, 3).await;
    assert_eq!(response.funds.verified_withdrawable_amount, None);
    assert_eq!(
        response.funds.withdrawability,
        CapitalExitWithdrawabilityV1::Stale
    );
    let replay = f
        .store
        .submit_capital_exit_evidence(
            &f.actor,
            initial.id,
            &evidence.external_message_id,
            &evidence,
            |_| async { panic!("historical replay must not republish") },
        )
        .await
        .unwrap();
    assert!(replay.replayed);
    assert_eq!(
        replay.resource.funds.verified_withdrawable_amount,
        Some(money("100"))
    );
    let current = f.store.capital_exit(&f.operator, initial.id).await.unwrap();
    assert_eq!(current.funds.verified_withdrawable_amount, None);
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_availability_requires_the_trusted_venue_binding(pool: PgPool) {
    let f = setup(&pool, true).await;
    let initial = f.start().await;
    let claim = f.claim(&initial, "venue-claim").await;
    let fenced = f.fence(&claim, 1).await;
    let mut kind = availability();
    if let CapitalExitEvidenceKindV1::WithdrawabilityObserved { venue, .. } = &mut kind {
        *venue = "OTHER-VENUE".into();
    }
    let evidence = f.evidence(&fenced, "wrong-venue", 2, kind);
    assert!(matches!(
        f.store
            .submit_capital_exit_evidence(
                &f.actor,
                initial.id,
                &evidence.external_message_id,
                &evidence,
                |_| async { panic!("wrong venue must not publish") }
            )
            .await,
        Err(StoreError::Invalid(
            "capital_exit_availability_venue_mismatch"
        ))
    ));
    let current = f.store.capital_exit(&f.operator, initial.id).await.unwrap();
    assert_eq!(current.funds.verified_withdrawable_amount, None);
}

/// Synthetic original source snapshots for transaction recovery tests only.
async fn next_snapshot(f: &mut Fixture, sequence: u64, stale: bool, balance: Option<&str>) {
    let mut next = f.observation.clone();
    next.sequence = DbCounter::new(sequence).unwrap();
    let ns = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
    next.observed_at_ns = DbCounter::new(ns).unwrap();
    let snapshot = next.snapshot.as_mut().unwrap();
    snapshot.event_id = format!("60a29b32-9e54-4f36-9304-{sequence:012x}");
    snapshot.ts_event = DbCounter::new(ns - 2).unwrap();
    snapshot.ts_init = DbCounter::new(ns - 1).unwrap();
    snapshot.is_stale = stale;
    if let Some(balance) = balance {
        snapshot.balances[0].total = money(balance);
        snapshot.balances[0].free = money(balance);
        snapshot.total_equity = vec![money(balance)];
    }
    let retained = f
        .store
        .submit_client_account_observation(
            &f.actor,
            &AccountObservationSubmitV2 {
                schema_version: NativeClientObservationSchemaV2,
                native_client_id: f.registration.native_client_id.clone(),
                observation: next.clone(),
            },
        )
        .await
        .unwrap();
    f.observation = next;
    f.observation_id = retained.resource.id;
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_stale_withdrawal_report_recovers_on_original_command_and_epoch(pool: PgPool) {
    let mut f = setup(&pool, true).await;
    let initial = f.start().await;
    let original_observation = f.observation_id;
    next_snapshot(&mut f, 2, true, None).await;
    let reported = f
        .store
        .act_capital_exit(
            &f.operator,
            initial.id,
            "one-user-withdrawal-report",
            &CapitalExitActionV1::ReconcileWithdrawal {
                schema_version: SchemaV1,
                expected_revision: initial.revision,
                user_reported_amount: "30".parse().unwrap(),
                currency: "USD".into(),
                external_transfer_ref: Some("original-transfer-30".into()),
            },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(reported.state, CapitalExitStateV1::Blocked);
    assert_eq!(
        reported.last_phase,
        CapitalExitStateV1::ReconcilingWithdrawal
    );
    assert_eq!(reported.funds.verified_withdrawable_amount, None);
    let stored = sqlx::query("SELECT state,content FROM app.capital_exit_intents WHERE id=$1")
        .bind(initial.id.as_uuid())
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored.get::<String, _>("state"), "RECONCILING_WITHDRAWAL");
    assert_eq!(
        stored.get::<serde_json::Value, _>("content")["state"],
        "RECONCILING_WITHDRAWAL"
    );
    next_snapshot(&mut f, 3, false, Some("970.25")).await;
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
    assert_eq!(
        pending.items[0].state,
        CapitalExitStateV1::ReconcilingWithdrawal
    );
    assert_eq!(pending.items[0].command_id, reported.command_id);
    assert_eq!(
        pending.items[0].account_control_epoch,
        reported.account_control_epoch
    );
    assert_eq!(pending.items[0].owner_command, reported.owner_command);
    let claimed = f.claim(&reported, "original-reconciliation-claim").await;
    let fenced = f.fence(&claimed, 1).await;
    assert_eq!(fenced.state, CapitalExitStateV1::ReconcilingWithdrawal);
    let evidence = f.evidence(
        &fenced,
        "recovered-original-transfer",
        2,
        CapitalExitEvidenceKindV1::WithdrawalReconciled {
            amount: money("30"),
            native_cash_movement_ref: "native-original-transfer-30".into(),
            external_transfer_ref: "original-transfer-30".into(),
            before_observation_id: original_observation,
            after_observation_id: f.observation_id,
            observed_managed_capital_after: money("970.25"),
        },
    );
    let reconciled = f
        .store
        .submit_capital_exit_evidence(
            &f.actor,
            initial.id,
            &evidence.external_message_id,
            &evidence,
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(reconciled.command_id, reported.command_id);
    assert_eq!(
        reconciled.account_control_epoch,
        reported.account_control_epoch
    );
    assert_eq!(reconciled.funds.reserved_amount, money("70"));
    assert_eq!(reconciled.funds.reconciled_withdrawal_amount, money("30"));
    let reports: i64 = sqlx::query_scalar("SELECT count(*) FROM app.command_receipts WHERE operation='CAPITAL_EXIT_ACTION' AND normalized_nonsecret_request->'request'->>'action'='RECONCILE_WITHDRAWAL'").fetch_one(&pool).await.unwrap();
    assert_eq!(
        reports, 1,
        "freshness recovery must not require another user report"
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn capital_exit_stale_cancel_accepts_fresh_terminal_evidence_on_original_command(
    pool: PgPool,
) {
    let mut f = setup(&pool, true).await;
    let initial = f.start().await;
    next_snapshot(&mut f, 2, true, None).await;
    let cancelled = f
        .store
        .act_capital_exit(
            &f.operator,
            initial.id,
            "one-user-cancel",
            &CapitalExitActionV1::Cancel {
                schema_version: SchemaV1,
                expected_revision: initial.revision,
            },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(cancelled.state, CapitalExitStateV1::Blocked);
    assert_eq!(cancelled.last_phase, CapitalExitStateV1::CancellingExit);
    let stored: String =
        sqlx::query_scalar("SELECT state FROM app.capital_exit_intents WHERE id=$1")
            .bind(initial.id.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(stored, "CANCELLING_EXIT");
    next_snapshot(&mut f, 3, false, None).await;
    let claimed = f.claim(&cancelled, "original-cancel-claim").await;
    assert_eq!(claimed.state, CapitalExitStateV1::CancellingExit);
    assert_eq!(claimed.command_id, cancelled.command_id);
    assert_eq!(
        claimed.account_control_epoch,
        cancelled.account_control_epoch
    );
    let fenced = f.fence(&claimed, 1).await;
    let evidence = f.evidence(
        &fenced,
        "original-cancel-native-terminal",
        2,
        CapitalExitEvidenceKindV1::NativeProgress {
            phase: CapitalExitStateV1::CancelledReserved,
            released_cash_amount: None,
            native_order_refs: vec![CapitalExitNativeOrderRefV1 {
                native_client_order_id: "original-exit-order".into(),
                native_strategy_id: "CAPITAL-001".into(),
                native_instrument_id: "TEST.SIM".into(),
                original_event_refs: vec!["original-native-cancelled-event".into()],
            }],
            native_position_refs: vec![],
            native_report_ref: "original-native-cancel-report".into(),
            reason_codes: vec![],
        },
    );
    let completed = f
        .store
        .submit_capital_exit_evidence(
            &f.actor,
            initial.id,
            &evidence.external_message_id,
            &evidence,
            |_| async { Ok(()) },
        )
        .await
        .unwrap()
        .resource;
    assert_eq!(completed.state, CapitalExitStateV1::CancelledReserved);
    assert_eq!(completed.command_id, cancelled.command_id);
    assert_eq!(
        completed.account_control_epoch,
        cancelled.account_control_epoch
    );
    assert_eq!(completed.funds.reserved_amount, money("100"));
    assert_eq!(completed.funds.verified_withdrawable_amount, None);
    let cancels: i64 = sqlx::query_scalar("SELECT count(*) FROM app.command_receipts WHERE operation='CAPITAL_EXIT_ACTION' AND normalized_nonsecret_request->'request'->>'action'='CANCEL'").fetch_one(&pool).await.unwrap();
    assert_eq!(
        cancels, 1,
        "native terminal evidence must finish the original cancellation"
    );
}
