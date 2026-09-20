//! Real PostgreSQL profile/receipt/revision transactions. Native observations
//! below are controlled fixtures; actual App Server inspection lives in server tests.
#[path = "../../../tests/support/research.rs"]
mod research_support;
use chrono::{DateTime, Utc};
use contracts::{codex::*, control::ListQuery, Id, SchemaV1};
use serde_json::json;
use sqlx::PgPool;
use store::{
    authority::Actor,
    codex_profiles::{CodexBindingCheck, CodexProbePreparation, CodexProbeTicket},
    Store, StoreError,
};

fn request() -> CodexProfileCreateV1 {
    CodexProfileCreateV1 {
        schema_version: SchemaV1,
        name: "Native profile fixture".into(),
        home_binding: format!("native-{}", Id::new()),
        profile_origin: ProfileOrigin::OperatorMount,
        connection: CodexConnectionCreateV1::System {},
        model_settings: SavedModelSettingsV1 {
            schema_version: SchemaV1,
            use_default_model_settings: true,
            saved_model: Some("dormant".into()),
            saved_reasoning_effort: Some("retained".into()),
            saved_fast_mode: true,
        },
    }
}
async fn verified(binding: CodexBindingCheck) -> Result<(), StoreError> {
    domain::codex::settings::home_binding(&binding.home_binding)?;
    assert_eq!(binding.profile_origin, ProfileOrigin::OperatorMount);
    Ok(())
}
async fn setup(pool: &PgPool) -> (Store, Actor, CodexProfileViewV1) {
    let (store, actor) = research_support::operator(pool).await;
    let profile = store
        .create_codex_profile(&actor, "create-profile", &request(), verified)
        .await
        .unwrap()
        .resource;
    (store, actor, profile)
}
fn update(profile: &CodexProfileViewV1) -> CodexProfileUpdateV1 {
    CodexProfileUpdateV1 {
        schema_version: SchemaV1,
        expected_revision: profile.revision,
        model_settings: profile.model_settings.clone(),
    }
}
async fn prepare(
    store: &Store,
    actor: &Actor,
    profile: &CodexProfileViewV1,
    key: &str,
) -> CodexProbeTicket {
    match store
        .prepare_codex_probe(
            actor,
            key,
            &CodexProbeRequestV1 {
                schema_version: SchemaV1,
                profile_id: profile.id,
                expected_revision: profile.revision,
            },
        )
        .await
        .unwrap()
    {
        CodexProbePreparation::Execute(ticket) => *ticket,
        CodexProbePreparation::Replay(_) => panic!("fresh probe unexpectedly replayed"),
    }
}
fn available(profile: &CodexProfileViewV1, at: DateTime<Utc>) -> CodexProbeOutcomeV1 {
    CodexProbeOutcomeV1::Available {
        native_version: "0.144.4".into(),
        account: CodexAccountV1 {
            requires_openai_auth: false,
            authentication_kind: None,
            plan_type: None,
        },
        effective: CodexEffectiveSettingsV1 {
            model: "fixture-model".into(),
            provider: "fixture-provider".into(),
            reasoning_effort: Some("fixture-effort".into()),
            service_tier: None,
        },
        models: vec![CodexAdvertisedModelV1 {
            capability: ModelCapabilityV1 {
                schema_version: SchemaV1,
                id: "fixture-id".into(),
                model: "fixture-model".into(),
                display_name: "Controlled fixture model".into(),
                hidden: false,
                default_reasoning_effort: "fixture-effort".into(),
                supported_reasoning_efforts: vec![ReasoningEffortCapability {
                    reasoning_effort: "fixture-effort".into(),
                    description: String::new(),
                }],
                is_default: false,
                fetched_at: at,
                profile_revision: profile.revision,
            },
            service_tiers: vec![],
            default_service_tier: None,
        }],
    }
}
async fn observations(pool: &PgPool) -> (i64, i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM app.codex_profile_observations),(SELECT count(*) FROM app.command_receipts WHERE operation='CODEX_PROBE')").fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn account_commands_are_single_send_and_invalidate_observations(pool: PgPool) {
    use store::codex_profiles::account::{
        CodexAccountCompletion as Completion, CodexAccountNext, CodexAccountPreparation as Prepared,
    };
    let (store, actor, profile) = setup(&pool).await;
    let probe = prepare(&store, &actor, &profile, "before-login").await;
    let outcome = available(&profile, probe.started_at);
    store.complete_codex_probe(probe, outcome).await.unwrap();
    let in_flight = prepare(&store, &actor, &profile, "concurrent-probe").await;
    let in_flight_outcome = available(&profile, in_flight.started_at);
    let request = CodexAccountRequestV1 {
        schema_version: SchemaV1,
        profile_id: profile.id,
        expected_revision: profile.revision,
    };
    let Prepared::Start(ticket) = store
        .prepare_codex_account(&actor, "login", &request, CodexAccountActionV1::Login)
        .await
        .unwrap()
    else {
        panic!("new login must own its send");
    };
    let Prepared::Replay(replay) = store
        .prepare_codex_account(&actor, "login", &request, CodexAccountActionV1::Login)
        .await
        .unwrap()
    else {
        panic!("replay must not launch again");
    };
    assert_eq!(replay.resource, ticket.acceptance.resource);
    assert!(replay.replayed);
    assert!(matches!(
        store
            .update_codex_profile(
                &actor,
                "during-login",
                profile.id,
                &update(&profile),
                verified
            )
            .await,
        Err(StoreError::Conflict)
    ));
    assert!(matches!(
        store
            .complete_codex_probe(in_flight, in_flight_outcome)
            .await,
        Err(StoreError::Conflict)
    ));
    assert_eq!(
        store
            .codex_observation(&actor, profile.id)
            .await
            .unwrap()
            .state,
        CodexObservationStateV1::Stale
    );
    assert!(store.begin_codex_account(&ticket).await.unwrap());
    assert!(!store.begin_codex_account(&ticket).await.unwrap());
    let waiting = store
        .bind_codex_login(&ticket, "native-fixture-login")
        .await
        .unwrap();
    let cancel = CodexLoginCancelV1 {
        schema_version: SchemaV1,
        operation_id: waiting.operation.id,
        expected_revision: waiting.revision,
    };
    let accepted = store
        .cancel_codex_login(&actor, "cancel", &cancel)
        .await
        .unwrap();
    assert_eq!(
        accepted.resource.state,
        CodexAccountOperationStateV1::CancelRequested
    );
    assert!(
        matches!(store.next_codex_account(&ticket).await.unwrap(), CodexAccountNext::Cancel(id) if id == "native-fixture-login")
    );
    assert!(matches!(
        store.next_codex_account(&ticket).await.unwrap(),
        CodexAccountNext::Wait
    ));
    let finished = store
        .complete_codex_account(&ticket, Completion::Cancelled)
        .await
        .unwrap();
    assert_eq!(finished.state, CodexAccountOperationStateV1::Cancelled);
    assert_eq!(
        store
            .latest_codex_account_operation(&actor, profile.id)
            .await
            .unwrap(),
        Some(finished)
    );
    assert_eq!(
        store
            .cancel_codex_login(&actor, "cancel", &cancel)
            .await
            .unwrap()
            .resource,
        accepted.resource
    );
    let probe = prepare(&store, &actor, &profile, "after-login").await;
    let outcome = available(&profile, probe.started_at);
    store.complete_codex_probe(probe, outcome).await.unwrap();
    assert_eq!(
        store
            .codex_observation(&actor, profile.id)
            .await
            .unwrap()
            .state,
        CodexObservationStateV1::Available
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn unsent_account_cancellation_never_grants_send_and_success_survives_late_cancel(
    pool: PgPool,
) {
    use store::codex_profiles::account::{
        CodexAccountCompletion as Completion, CodexAccountPreparation as Prepared,
    };
    let (store, actor, profile) = setup(&pool).await;
    let request = CodexAccountRequestV1 {
        schema_version: SchemaV1,
        profile_id: profile.id,
        expected_revision: profile.revision,
    };
    let Prepared::Start(unsent) = store
        .prepare_codex_account(&actor, "unsent", &request, CodexAccountActionV1::Login)
        .await
        .unwrap()
    else {
        panic!("missing ticket");
    };
    let initial = store
        .read_codex_account_operation(&actor, unsent.acceptance.resource.id)
        .await
        .unwrap();
    let cancel = CodexLoginCancelV1 {
        schema_version: SchemaV1,
        operation_id: initial.operation.id,
        expected_revision: initial.revision,
    };
    let cancelled = store
        .cancel_codex_login(&actor, "unsent-cancel", &cancel)
        .await
        .unwrap();
    assert_eq!(
        cancelled.resource.reason,
        Some(CodexAccountReasonV1::ConfirmedNotSent)
    );
    assert!(!store.begin_codex_account(&unsent).await.unwrap());
    let Prepared::Start(ticket) = store
        .prepare_codex_account(&actor, "successful", &request, CodexAccountActionV1::Login)
        .await
        .unwrap()
    else {
        panic!("missing ticket");
    };
    assert!(store.begin_codex_account(&ticket).await.unwrap());
    let waiting = store
        .bind_codex_login(&ticket, "completed-native-login")
        .await
        .unwrap();
    let cancel = CodexLoginCancelV1 {
        schema_version: SchemaV1,
        operation_id: waiting.operation.id,
        expected_revision: waiting.revision,
    };
    store
        .cancel_codex_login(&actor, "racing-cancel", &cancel)
        .await
        .unwrap();
    let account = CodexAccountV1 {
        requires_openai_auth: true,
        authentication_kind: Some(CodexAuthenticationKind::Chatgpt),
        plan_type: Some("pro".into()),
    };
    let succeeded = store
        .complete_codex_account(&ticket, Completion::LoggedIn(account))
        .await
        .unwrap();
    assert_eq!(succeeded.state, CodexAccountOperationStateV1::Succeeded);
    assert!(matches!(
        store
            .complete_codex_account(&ticket, Completion::Cancelled)
            .await,
        Err(StoreError::Conflict)
    ));
    let document = serde_json::to_value(
        store
            .read_codex_account_operation(&actor, succeeded.operation.id)
            .await
            .unwrap(),
    )
    .unwrap();
    assert!(document.get("device_code").is_none());
    assert!(document.get("native_login_id").is_none());
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_replay_and_home_uniqueness_preserve_saved_defaults_without_claiming_native_readiness(
    pool: PgPool,
) {
    let (store, actor) = research_support::operator(&pool).await;
    let request = request();
    let (a, b) = tokio::join!(
        store.create_codex_profile(&actor, "same-create", &request, verified),
        store.create_codex_profile(&actor, "same-create", &request, verified)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(
        serde_json::to_value(&a.resource).unwrap(),
        serde_json::to_value(&b.resource).unwrap()
    );
    assert_eq!(a.resource.model_settings, request.model_settings);
    assert_eq!(a.resource.connection_mode, ConnectionMode::System);
    assert_eq!(
        store
            .codex_observation(&actor, a.resource.id)
            .await
            .unwrap()
            .state,
        CodexObservationStateV1::NeverProbed
    );
    assert!(matches!(
        store
            .create_codex_profile(&actor, "different-create", &request, verified)
            .await,
        Err(StoreError::Conflict)
    ));
    let records:(i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.codex_profiles),(SELECT count(*) FROM app.command_receipts WHERE operation='CODEX_PROFILE_CREATE')").fetch_one(&pool).await.unwrap();
    assert_eq!(records, (3, 1));
    let listed = store
        .codex_profiles(
            &actor,
            &ListQuery {
                cursor: None,
                limit: 1,
            },
        )
        .await
        .unwrap();
    let bindings = store.local_codex_bindings().await.unwrap();
    assert!(
        bindings
            .iter()
            .any(|binding| Some(binding.reference.as_str())
                == listed.items[0].home_binding.as_deref())
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_native_binding_rolls_back_creation_and_original_key_remains_available(
    pool: PgPool,
) {
    let (store, actor) = research_support::operator(&pool).await;
    let request = request();
    let failed = store
        .create_codex_profile(&actor, "retryable-create", &request, |_| async {
            Err(StoreError::IntegrationUnavailable)
        })
        .await;
    assert!(matches!(failed, Err(StoreError::IntegrationUnavailable)));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM app.codex_profiles")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 2);
    let result = store
        .create_codex_profile(&actor, "retryable-create", &request, verified)
        .await
        .unwrap();
    assert!(!result.replayed);
    let replay = store
        .create_codex_profile(&actor, "retryable-create", &request, |_| async {
            Err(StoreError::Invalid("replay_called_native_verification"))
        })
        .await
        .unwrap();
    assert!(replay.replayed);
}

#[sqlx::test(migrations = "../../migrations")]
async fn model_updates_retain_native_binding_and_reject_stale_revision(pool: PgPool) {
    let (store, actor, profile) = setup(&pool).await;
    let mut change = update(&profile);
    change.model_settings.use_default_model_settings = false;
    change.model_settings.saved_model = None;
    change.model_settings.saved_reasoning_effort = Some("native-effort".into());
    change.model_settings.saved_fast_mode = false;
    let updated = store
        .update_codex_profile(&actor, "model-only", profile.id, &change, verified)
        .await
        .unwrap()
        .resource;
    assert_eq!(updated.connection_mode, ConnectionMode::System);
    assert_eq!(updated.home_binding, profile.home_binding);
    assert_eq!(updated.name, profile.name);
    assert_eq!(updated.profile_origin, profile.profile_origin);
    assert_eq!(updated.model_settings, change.model_settings);
    let public = serde_json::to_value(&updated).unwrap();
    for key in ["credential_configured", "credential_ref", "custom_base_url"] {
        assert!(public.get(key).is_none());
    }
    assert!(matches!(
        store
            .update_codex_profile(&actor, "stale", profile.id, &update(&profile), verified)
            .await,
        Err(StoreError::RevisionConflict { .. })
    ));
    let legacy = Id::new();
    sqlx::query("INSERT INTO app.codex_profiles(id,name,connection_mode,profile_origin,codex_home_ref,use_default_model_settings,saved_fast_mode,custom_base_url,custom_api_key_ref) VALUES($1,'historical custom','CUSTOM_PROVIDER','OPERATOR_MOUNT','retired-custom',true,false,'https://provider.invalid/v1','historical-secret')")
        .bind(legacy.as_uuid()).execute(&pool).await.unwrap();
    assert!(matches!(
        store.codex_profile(&actor, legacy).await,
        Err(StoreError::NotFound)
    ));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM app.codex_profiles WHERE id=$1")
            .bind(legacy.as_uuid())
            .fetch_one(&pool)
            .await
            .unwrap(),
        1
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_home_and_profile_origin_are_permanently_bound_and_legacy_paths_are_not_public(
    pool: PgPool,
) {
    let (store, actor, profile) = setup(&pool).await;
    // The existing generic immutable-field guard and the added origin guard
    // deliberately retain their original, distinct SQLSTATEs.
    for (statement, expected_code) in [
        (
            "UPDATE app.codex_profiles SET codex_home_ref='different-home' WHERE id=$1",
            "23000",
        ),
        (
            "UPDATE app.codex_profiles SET profile_origin='MANAGED_VOLUME' WHERE id=$1",
            "23514",
        ),
    ] {
        let error = sqlx::query(statement)
            .bind(profile.id.as_uuid())
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some(expected_code)
        );
    }
    let legacy = Id::new();
    sqlx::query("INSERT INTO app.codex_profiles(id,name,connection_mode,profile_origin,codex_home_ref,use_default_model_settings,saved_fast_mode) VALUES($1,'historical reference','SYSTEM','OPERATOR_MOUNT','/private/historical/native-home',true,false)")
        .bind(legacy.as_uuid()).execute(&pool).await.unwrap();
    let view = store.codex_profile(&actor, legacy).await.unwrap();
    assert!(view.home_binding.is_none());
    assert!(!serde_json::to_string(&view).unwrap().contains("/private/"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_probe_receipt_publishes_once_and_read_only_observation_preserves_latest_failure(
    pool: PgPool,
) {
    let (store, actor, profile) = setup(&pool).await;
    let first = prepare(&store, &actor, &profile, "probe-once").await;
    let second = prepare(&store, &actor, &profile, "probe-once").await;
    let outcome = available(&profile, first.started_at);
    let (a, b) = tokio::join!(
        store.complete_codex_probe(first, outcome.clone()),
        store.complete_codex_probe(second, outcome)
    );
    let (a, b) = (a.unwrap(), b.unwrap());
    assert_ne!(a.replayed, b.replayed);
    assert_eq!(a.resource.id, b.resource.id);
    assert_eq!(observations(&pool).await, (1, 1));
    assert_eq!(
        store
            .codex_observation(&actor, profile.id)
            .await
            .unwrap()
            .state,
        CodexObservationStateV1::Available
    );
    let failed = prepare(&store, &actor, &profile, "later-failure").await;
    store
        .complete_codex_probe(
            failed,
            CodexProbeOutcomeV1::Unavailable {
                reason: CodexProbeFailureV1::NativeUnavailable,
            },
        )
        .await
        .unwrap();
    let current = store.codex_observation(&actor, profile.id).await.unwrap();
    assert_eq!(current.state, CodexObservationStateV1::Unavailable);
    assert!(matches!(
        current.observation.unwrap().outcome,
        CodexProbeOutcomeV1::Unavailable { .. }
    ));
    let before = observations(&pool).await;
    store.codex_observation(&actor, profile.id).await.unwrap();
    assert_eq!(observations(&pool).await, before);
}

#[sqlx::test(migrations = "../../migrations")]
async fn profile_update_during_probe_rejects_the_stale_observation_without_a_receipt(pool: PgPool) {
    let (store, actor, profile) = setup(&pool).await;
    let ticket = prepare(&store, &actor, &profile, "stale-probe").await;
    let outcome = available(&profile, ticket.started_at);
    let updated = store
        .update_codex_profile(
            &actor,
            "change-profile",
            profile.id,
            &update(&profile),
            verified,
        )
        .await
        .unwrap()
        .resource;
    assert!(matches!(
        store.complete_codex_probe(ticket, outcome).await,
        Err(StoreError::RevisionConflict { .. })
    ));
    assert_eq!(observations(&pool).await, (0, 0));
    let ticket = prepare(&store, &actor, &updated, "stale-probe").await;
    let outcome = available(&updated, ticket.started_at);
    assert!(
        !store
            .complete_codex_probe(ticket, outcome)
            .await
            .unwrap()
            .replayed
    );
    let newer = store
        .update_codex_profile(
            &actor,
            "newer-profile",
            profile.id,
            &update(&updated),
            verified,
        )
        .await
        .unwrap()
        .resource;
    let state = store.codex_observation(&actor, profile.id).await.unwrap();
    assert_eq!(state.state, CodexObservationStateV1::Stale);
    assert_eq!(state.profile_revision, newer.revision);
    assert_eq!(
        state.observation.unwrap().profile_revision,
        updated.revision
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_native_catalog_cannot_be_stored_and_real_epoch_revocation_is_rechecked(
    pool: PgPool,
) {
    let (store, actor, profile) = setup(&pool).await;
    let ticket = prepare(&store, &actor, &profile, "bad-catalog").await;
    let mut invalid = available(&profile, ticket.started_at);
    if let CodexProbeOutcomeV1::Available { models, .. } = &mut invalid {
        models.clear();
    }
    assert!(store.complete_codex_probe(ticket, invalid).await.is_err());
    assert_eq!(observations(&pool).await, (0, 0));
    let ticket = prepare(&store, &actor, &profile, "revoked").await;
    let outcome = available(&profile, ticket.started_at);
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=session_epoch+1")
        .execute(&pool)
        .await
        .unwrap();
    assert!(store.complete_codex_probe(ticket, outcome).await.is_err());
    assert_eq!(observations(&pool).await, (0, 0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn durable_probe_rows_are_immutable_and_unsupported_settings_never_override_real_native_output(
    pool: PgPool,
) {
    let (store, actor, profile) = setup(&pool).await;
    let mut change = update(&profile);
    change.model_settings.use_default_model_settings = false;
    change.model_settings.saved_fast_mode = false;
    let configured = store
        .update_codex_profile(&actor, "explicit", profile.id, &change, verified)
        .await
        .unwrap()
        .resource;
    let ticket = prepare(&store, &actor, &configured, "not-honored").await;
    let outcome = available(&configured, ticket.started_at);
    assert!(store.complete_codex_probe(ticket, outcome).await.is_err());
    assert_eq!(observations(&pool).await, (0, 0));
    let ticket = prepare(&store, &actor, &configured, "not-honored").await;
    let observed = store
        .complete_codex_probe(
            ticket,
            CodexProbeOutcomeV1::Unavailable {
                reason: CodexProbeFailureV1::ModelSettingsUnsupported,
            },
        )
        .await
        .unwrap()
        .resource;
    for query in [
        "UPDATE app.codex_profile_observations SET outcome=$2 WHERE id=$1",
        "DELETE FROM app.codex_profile_observations WHERE id=$1 AND outcome=$2",
    ] {
        let error = sqlx::query(query)
            .bind(observed.id.as_uuid())
            .bind(json!({"schema_version":1,"result":observed.outcome}))
            .execute(&pool)
            .await
            .unwrap_err();
        assert_eq!(
            error.as_database_error().unwrap().code().as_deref(),
            Some("23000")
        );
    }
    assert_eq!(observations(&pool).await, (1, 1));
}

#[sqlx::test(migrations = "../../migrations")]
async fn local_roles_share_account_admission_and_invalidate_both_observations(pool: PgPool) {
    use store::codex_profiles::account::{
        CodexAccountCompletion as Completion, CodexAccountPreparation as Prepared,
    };
    let (store, actor, independent) = setup(&pool).await;
    let roles = store
        .codex_profiles(
            &actor,
            &ListQuery {
                limit: 100,
                cursor: None,
            },
        )
        .await
        .unwrap()
        .items;
    assert_eq!(roles.len(), 2);
    for (index, profile) in roles.iter().enumerate() {
        let ticket = prepare(&store, &actor, profile, &format!("before-shared-{index}")).await;
        let outcome = available(profile, ticket.started_at);
        store.complete_codex_probe(ticket, outcome).await.unwrap();
    }
    let in_flight = prepare(&store, &actor, &roles[1], "shared-probe-in-flight").await;
    let in_flight_outcome = available(&roles[1], in_flight.started_at);
    let request = |profile: &CodexProfileViewV1| CodexAccountRequestV1 {
        schema_version: SchemaV1,
        profile_id: profile.id,
        expected_revision: profile.revision,
    };
    let left = request(&roles[0]);
    let right = request(&roles[1]);
    let (left_result, right_result) =
        tokio::time::timeout(std::time::Duration::from_secs(10), async {
            tokio::join!(
                store.prepare_codex_account(
                    &actor,
                    "local-account-left",
                    &left,
                    CodexAccountActionV1::Logout
                ),
                store.prepare_codex_account(
                    &actor,
                    "local-account-right",
                    &right,
                    CodexAccountActionV1::Logout
                )
            )
        })
        .await
        .expect("shared role admission must serialize without deadlocking");
    let (ticket, rejected) = match (left_result, right_result) {
        (Ok(Prepared::Start(ticket)), Err(error)) | (Err(error), Ok(Prepared::Start(ticket))) => {
            (ticket, error)
        }
        _ => panic!("exactly one role must obtain the native account send permit"),
    };
    assert!(matches!(rejected, StoreError::Conflict));
    for role in &roles {
        assert_eq!(
            store
                .codex_observation(&actor, role.id)
                .await
                .unwrap()
                .state,
            CodexObservationStateV1::Stale
        );
        assert_eq!(
            store
                .latest_codex_account_operation(&actor, role.id)
                .await
                .unwrap()
                .unwrap()
                .operation
                .id,
            ticket.acceptance.resource.id
        );
        assert!(matches!(
            store
                .update_codex_profile(
                    &actor,
                    &format!("blocked-{}", role.id),
                    role.id,
                    &update(role),
                    verified
                )
                .await,
            Err(StoreError::Conflict)
        ));
    }
    assert!(matches!(
        store
            .complete_codex_probe(in_flight, in_flight_outcome)
            .await,
        Err(StoreError::Conflict)
    ));

    // A historical/test-only independent home must not be accidentally merged
    // into the one local account group.
    let Prepared::Start(separate) = store
        .prepare_codex_account(
            &actor,
            "independent-account",
            &request(&independent),
            CodexAccountActionV1::Logout,
        )
        .await
        .unwrap()
    else {
        panic!("independent binding should keep its own lifecycle")
    };
    assert_ne!(
        separate.acceptance.resource.id,
        ticket.acceptance.resource.id
    );
    assert_eq!(
        store
            .latest_codex_account_operation(&actor, independent.id)
            .await
            .unwrap()
            .unwrap()
            .operation
            .id,
        separate.acceptance.resource.id
    );

    assert!(store.begin_codex_account(&ticket).await.unwrap());
    assert!(!store.begin_codex_account(&ticket).await.unwrap());
    store
        .complete_codex_account(
            &ticket,
            Completion::LoggedOut(CodexAccountV1 {
                requires_openai_auth: true,
                authentication_kind: None,
                plan_type: None,
            }),
        )
        .await
        .unwrap();
    for role in &roles {
        assert_eq!(
            store
                .codex_observation(&actor, role.id)
                .await
                .unwrap()
                .state,
            CodexObservationStateV1::Stale
        );
    }
    let observed = prepare(&store, &actor, &roles[0], "refresh-first-local-role").await;
    let outcome = available(&roles[0], observed.started_at);
    store.complete_codex_probe(observed, outcome).await.unwrap();
    assert_eq!(
        store
            .codex_observation(&actor, roles[0].id)
            .await
            .unwrap()
            .state,
        CodexObservationStateV1::Available
    );
    assert_eq!(
        store
            .codex_observation(&actor, roles[1].id)
            .await
            .unwrap()
            .state,
        CodexObservationStateV1::Stale
    );
    let refreshed = prepare(&store, &actor, &roles[1], "refresh-second-local-role").await;
    let outcome = available(&roles[1], refreshed.started_at);
    store
        .complete_codex_probe(refreshed, outcome)
        .await
        .unwrap();
    assert_eq!(
        store
            .codex_observation(&actor, roles[1].id)
            .await
            .unwrap()
            .state,
        CodexObservationStateV1::Available
    );
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_account_database_guard_spans_the_two_local_roles(pool: PgPool) {
    let (store, actor, _) = setup(&pool).await;
    let roles = store
        .codex_profiles(
            &actor,
            &ListQuery {
                limit: 100,
                cursor: None,
            },
        )
        .await
        .unwrap()
        .items;
    assert_eq!(roles.len(), 2);
    // Relational negative control only, not a native RPC or HTTP acceptance.
    sqlx::query("INSERT INTO app.codex_account_operations(profile_id,profile_revision,action,state,deadline_at) VALUES($1,$2,'LOGIN','REQUESTED',clock_timestamp()+interval '1 minute')")
        .bind(roles[0].id.as_uuid()).bind(roles[0].revision.get() as i64).execute(&pool).await.unwrap();
    let rejected=sqlx::query("INSERT INTO app.codex_account_operations(profile_id,profile_revision,action,state,deadline_at) VALUES($1,$2,'LOGOUT','REQUESTED',clock_timestamp()+interval '1 minute')")
        .bind(roles[1].id.as_uuid()).bind(roles[1].revision.get() as i64).execute(&pool).await.unwrap_err();
    assert_eq!(
        rejected.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
    let rejected = sqlx::query("UPDATE app.codex_profiles SET saved_model='different' WHERE id=$1")
        .bind(roles[1].id.as_uuid())
        .execute(&pool)
        .await
        .unwrap_err();
    assert_eq!(
        rejected.as_database_error().unwrap().code().as_deref(),
        Some("23514")
    );
}
