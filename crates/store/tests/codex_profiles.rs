//! Real PostgreSQL profile/receipt/revision transactions. Native observations
//! below are controlled fixtures; actual App Server inspection lives in server tests.
#[path = "../../../tests/support/research.rs"]
mod research_support;
use chrono::{DateTime, Utc};
use contracts::{codex::*, control::ListQuery, Id, SchemaV1};
use serde_json::json;
use sqlx::PgPool;
use store::{authority::Actor, codex_profiles::{CodexBindingCheck, CodexProbePreparation, CodexProbeTicket}, Store, StoreError};

fn request() -> CodexProfileCreateV1 {
    CodexProfileCreateV1 { schema_version:SchemaV1,name:"Native profile fixture".into(),home_binding:format!("native-{}",Id::new()),profile_origin:ProfileOrigin::OperatorMount,
        connection:CodexConnectionCreateV1::System,
        model_settings:SavedModelSettingsV1 { schema_version:SchemaV1,use_default_model_settings:true,saved_model:Some("dormant".into()),saved_reasoning_effort:Some("retained".into()),saved_fast_mode:true } }
}
async fn verified(binding: CodexBindingCheck) -> Result<(), StoreError> {
    domain::codex::settings::home_binding(&binding.home_binding)?;
    assert_eq!(binding.profile_origin, ProfileOrigin::OperatorMount);
    Ok(())
}
async fn setup(pool: &PgPool) -> (Store, Actor, CodexProfileViewV1) {
    let (store, actor) = research_support::operator(pool).await;
    let profile = store.create_codex_profile(&actor,"create-profile",&request(),verified).await.unwrap().resource;
    (store,actor,profile)
}
fn update(profile:&CodexProfileViewV1) -> CodexProfileUpdateV1 {
    CodexProfileUpdateV1 { schema_version:SchemaV1,expected_revision:profile.revision,name:profile.name.clone(),connection:CodexConnectionUpdateV1::System,model_settings:profile.model_settings.clone() }
}
async fn prepare(store:&Store,actor:&Actor,profile:&CodexProfileViewV1,key:&str) -> CodexProbeTicket {
    match store.prepare_codex_probe(actor,key,&CodexProbeRequestV1 { schema_version:SchemaV1,profile_id:profile.id,expected_revision:profile.revision }).await.unwrap() {
        CodexProbePreparation::Execute(ticket) => *ticket,
        CodexProbePreparation::Replay(_) => panic!("fresh probe unexpectedly replayed"),
    }
}
fn available(profile:&CodexProfileViewV1,at:DateTime<Utc>) -> CodexProbeOutcomeV1 {
    CodexProbeOutcomeV1::Available { native_version:"0.144.4".into(),account:CodexAccountV1 { requires_openai_auth:false,authentication_kind:None,plan_type:None },
        effective:CodexEffectiveSettingsV1 { model:"fixture-model".into(),provider:"fixture-provider".into(),reasoning_effort:Some("fixture-effort".into()),service_tier:None },
        models:vec![CodexAdvertisedModelV1 { capability:ModelCapabilityV1 { schema_version:SchemaV1,id:"fixture-id".into(),model:"fixture-model".into(),display_name:"Controlled fixture model".into(),hidden:false,
            default_reasoning_effort:"fixture-effort".into(),supported_reasoning_efforts:vec![ReasoningEffortCapability {reasoning_effort:"fixture-effort".into(),description:String::new()}],is_default:false,fetched_at:at,profile_revision:profile.revision },
            service_tiers:vec![],default_service_tier:None }] }
}
async fn observations(pool:&PgPool) -> (i64,i64) {
    sqlx::query_as("SELECT (SELECT count(*) FROM app.codex_profile_observations),(SELECT count(*) FROM app.command_receipts WHERE operation='CODEX_PROBE')").fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "../../migrations")]
async fn create_replay_and_home_uniqueness_preserve_saved_defaults_without_claiming_native_readiness(pool:PgPool) {
    let (store,actor) = research_support::operator(&pool).await;
    let request = request();
    let (a,b) = tokio::join!(store.create_codex_profile(&actor,"same-create",&request,verified),store.create_codex_profile(&actor,"same-create",&request,verified));
    let (a,b) = (a.unwrap(),b.unwrap());
    assert_ne!(a.replayed,b.replayed);
    assert_eq!(serde_json::to_value(&a.resource).unwrap(),serde_json::to_value(&b.resource).unwrap());
    assert_eq!(a.resource.model_settings,request.model_settings);
    assert_eq!(a.resource.connection_mode,ConnectionMode::System);
    assert!(!a.resource.credential_configured);
    assert_eq!(store.codex_observation(&actor,a.resource.id).await.unwrap().state,CodexObservationStateV1::NeverProbed);
    assert!(matches!(store.create_codex_profile(&actor,"different-create",&request,verified).await,Err(StoreError::Conflict)));
    let records:(i64,i64) = sqlx::query_as("SELECT (SELECT count(*) FROM app.codex_profiles),(SELECT count(*) FROM app.command_receipts WHERE operation='CODEX_PROFILE_CREATE')").fetch_one(&pool).await.unwrap();
    assert_eq!(records,(1,1));
    let listed=store.codex_profiles(&actor,&ListQuery {cursor:None,limit:1}).await.unwrap();
    assert_eq!(listed.items[0].id,a.resource.id);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_native_binding_rolls_back_creation_and_original_key_remains_available(pool:PgPool) {
    let (store,actor)=research_support::operator(&pool).await;
    let request=request();
    let failed=store.create_codex_profile(&actor,"retryable-create",&request, |_|async {Err(StoreError::IntegrationUnavailable)}).await;
    assert!(matches!(failed,Err(StoreError::IntegrationUnavailable)));
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM app.codex_profiles").fetch_one(&pool).await.unwrap();
    assert_eq!(count,0);
    let result=store.create_codex_profile(&actor,"retryable-create",&request,verified).await.unwrap();
    assert!(!result.replayed);
    let replay=store.create_codex_profile(&actor,"retryable-create",&request, |_|async {Err(StoreError::Invalid("replay_called_native_verification"))}).await.unwrap();
    assert!(replay.replayed);
}

#[sqlx::test(migrations = "../../migrations")]
async fn custom_route_needs_its_own_credential_and_switching_system_does_not_delete_old_secret(pool:PgPool) {
    let (store,actor,profile)=setup(&pool).await;
    let mut request=update(&profile);
    request.connection=CodexConnectionUpdateV1::CustomProvider {base_url:"https://provider.invalid/v1".into(),credential_ref:None};
    assert!(matches!(store.update_codex_profile(&actor,"bad-route",profile.id,&request,verified).await,Err(StoreError::Domain(_))));
    let credential=Id::new();
    request.connection=CodexConnectionUpdateV1::CustomProvider {base_url:"https://provider.invalid/v1".into(),credential_ref:Some(credential)};
    let custom=store.update_codex_profile(&actor,"route",profile.id,&request,move|check|async move {assert_eq!(check.credential_ref,Some(credential));verified(check).await}).await.unwrap().resource;
    assert_eq!(custom.connection_mode,ConnectionMode::CustomProvider);
    assert!(custom.credential_configured);
    let public=serde_json::to_string(&custom).unwrap();
    assert!(!public.contains(&credential.to_string()));
    let mut retain=update(&custom);
    retain.connection=CodexConnectionUpdateV1::CustomProvider {base_url:"https://provider.invalid/v2".into(),credential_ref:None};
    let retained=store.update_codex_profile(&actor,"retain",profile.id,&retain,move|check|async move {assert_eq!(check.credential_ref,Some(credential));Ok(())}).await.unwrap().resource;
    let system=store.update_codex_profile(&actor,"system",profile.id,&update(&retained),move|check|async move {assert!(check.credential_ref.is_none());Ok(())}).await.unwrap().resource;
    assert_eq!(system.connection_mode,ConnectionMode::System);
    assert!(!system.credential_configured);
    assert!(system.custom_base_url.is_none());
    assert_eq!(system.home_binding,profile.home_binding);
    assert!(matches!(store.update_codex_profile(&actor,"stale",profile.id,&update(&profile),verified).await,Err(StoreError::RevisionConflict {..})));
}

#[sqlx::test(migrations = "../../migrations")]
async fn native_home_and_profile_origin_are_permanently_bound_and_legacy_paths_are_not_public(pool:PgPool) {
    let (store,actor,profile)=setup(&pool).await;
    for statement in ["UPDATE app.codex_profiles SET codex_home_ref='different-home' WHERE id=$1","UPDATE app.codex_profiles SET profile_origin='MANAGED_VOLUME' WHERE id=$1"] {
        let error=sqlx::query(statement).bind(profile.id.as_uuid()).execute(&pool).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    }
    let legacy=Id::new();
    sqlx::query("INSERT INTO app.codex_profiles(id,name,connection_mode,profile_origin,codex_home_ref,use_default_model_settings,saved_fast_mode) VALUES($1,'historical reference','SYSTEM','OPERATOR_MOUNT','/private/historical/native-home',true,false)")
        .bind(legacy.as_uuid()).execute(&pool).await.unwrap();
    let view=store.codex_profile(&actor,legacy).await.unwrap();
    assert!(view.home_binding.is_none());
    assert!(!serde_json::to_string(&view).unwrap().contains("/private/"));
}

#[sqlx::test(migrations = "../../migrations")]
async fn concurrent_probe_receipt_publishes_once_and_read_only_observation_preserves_latest_failure(pool:PgPool) {
    let (store,actor,profile)=setup(&pool).await;
    let first=prepare(&store,&actor,&profile,"probe-once").await;
    let second=prepare(&store,&actor,&profile,"probe-once").await;
    let outcome=available(&profile,first.started_at);
    let (a,b)=tokio::join!(store.complete_codex_probe(first,outcome.clone()),store.complete_codex_probe(second,outcome));
    let (a,b)=(a.unwrap(),b.unwrap());
    assert_ne!(a.replayed,b.replayed);
    assert_eq!(a.resource.id,b.resource.id);
    assert_eq!(observations(&pool).await,(1,1));
    assert_eq!(store.codex_observation(&actor,profile.id).await.unwrap().state,CodexObservationStateV1::Available);
    let failed=prepare(&store,&actor,&profile,"later-failure").await;
    store.complete_codex_probe(failed,CodexProbeOutcomeV1::Unavailable {reason:CodexProbeFailureV1::NativeUnavailable}).await.unwrap();
    let current=store.codex_observation(&actor,profile.id).await.unwrap();
    assert_eq!(current.state,CodexObservationStateV1::Unavailable);
    assert!(matches!(current.observation.unwrap().outcome,CodexProbeOutcomeV1::Unavailable {..}));
    let before=observations(&pool).await;
    store.codex_observation(&actor,profile.id).await.unwrap();
    assert_eq!(observations(&pool).await,before);
}

#[sqlx::test(migrations = "../../migrations")]
async fn profile_update_during_probe_rejects_the_stale_observation_without_a_receipt(pool:PgPool) {
    let (store,actor,profile)=setup(&pool).await;
    let ticket=prepare(&store,&actor,&profile,"stale-probe").await;
    let outcome=available(&profile,ticket.started_at);
    let updated=store.update_codex_profile(&actor,"change-profile",profile.id,&update(&profile),verified).await.unwrap().resource;
    assert!(matches!(store.complete_codex_probe(ticket,outcome).await,Err(StoreError::RevisionConflict {..})));
    assert_eq!(observations(&pool).await,(0,0));
    let ticket=prepare(&store,&actor,&updated,"stale-probe").await;
    let outcome=available(&updated,ticket.started_at);
    assert!(!store.complete_codex_probe(ticket,outcome).await.unwrap().replayed);
    let newer=store.update_codex_profile(&actor,"newer-profile",profile.id,&update(&updated),verified).await.unwrap().resource;
    let state=store.codex_observation(&actor,profile.id).await.unwrap();
    assert_eq!(state.state,CodexObservationStateV1::Stale);
    assert_eq!(state.profile_revision,newer.revision);
    assert_eq!(state.observation.unwrap().profile_revision,updated.revision);
}

#[sqlx::test(migrations = "../../migrations")]
async fn invalid_native_catalog_cannot_be_stored_and_real_epoch_revocation_is_rechecked(pool:PgPool) {
    let (store,actor,profile)=setup(&pool).await;
    let ticket=prepare(&store,&actor,&profile,"bad-catalog").await;
    let mut invalid=available(&profile,ticket.started_at);
    if let CodexProbeOutcomeV1::Available {models,..}=&mut invalid {models.clear();}
    assert!(store.complete_codex_probe(ticket,invalid).await.is_err());
    assert_eq!(observations(&pool).await,(0,0));
    let ticket=prepare(&store,&actor,&profile,"revoked").await;
    let outcome=available(&profile,ticket.started_at);
    sqlx::query("UPDATE app.operator_auth_state SET session_epoch=session_epoch+1").execute(&pool).await.unwrap();
    assert!(store.complete_codex_probe(ticket,outcome).await.is_err());
    assert_eq!(observations(&pool).await,(0,0));
}

#[sqlx::test(migrations = "../../migrations")]
async fn durable_probe_rows_are_immutable_and_unsupported_settings_never_override_real_native_output(pool:PgPool) {
    let (store,actor,profile)=setup(&pool).await;
    let mut change=update(&profile);
    change.model_settings.use_default_model_settings=false;
    change.model_settings.saved_fast_mode=false;
    let configured=store.update_codex_profile(&actor,"explicit",profile.id,&change,verified).await.unwrap().resource;
    let ticket=prepare(&store,&actor,&configured,"not-honored").await;
    let outcome=available(&configured,ticket.started_at);
    assert!(store.complete_codex_probe(ticket,outcome).await.is_err());
    assert_eq!(observations(&pool).await,(0,0));
    let ticket=prepare(&store,&actor,&configured,"not-honored").await;
    let observed=store.complete_codex_probe(ticket,CodexProbeOutcomeV1::Unavailable {reason:CodexProbeFailureV1::ModelSettingsUnsupported}).await.unwrap().resource;
    for query in ["UPDATE app.codex_profile_observations SET outcome=$2 WHERE id=$1","DELETE FROM app.codex_profile_observations WHERE id=$1 AND outcome=$2"] {
        let error=sqlx::query(query).bind(observed.id.as_uuid()).bind(json!({"schema_version":1,"result":observed.outcome})).execute(&pool).await.unwrap_err();
        assert_eq!(error.as_database_error().unwrap().code().as_deref(),Some("23514"));
    }
    assert_eq!(observations(&pool).await,(1,1));
}
