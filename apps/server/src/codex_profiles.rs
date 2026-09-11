//! Native Codex configuration endpoints. Read-only views never launch a process;
//! an explicit probe is human-authorized and remains distinct from paid inference.
mod deployment;
pub use deployment::{CodexDeployment, CodexDeploymentBinding, CodexDeploymentConfig};
use crate::{access::{idempotency_key, Authority}, auth::json, error::{ApiError, Problem}, AppState};
use axum::{extract::{rejection::{JsonRejection, PathRejection, QueryRejection}, Path, Query, State}, http::{HeaderMap, StatusCode}, Json};
use contracts::{codex::*, control::{CommandResult, ListQuery, Page}, Id};
use store::codex_profiles::CodexProbePreparation;

fn path(value: Result<Path<Id>, PathRejection>) -> Result<Id, ApiError> {
    value.map(|Path(id)| id).map_err(|_| ApiError::validation())
}

#[utoipa::path(get,path="/api/v2/settings/codex",operation_id="listCodexProfiles",tag="Codex settings",params(("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<CodexProfileViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=422,body=Problem),(status=429,body=Problem)))]
pub async fn profiles(State(state): State<AppState>, Authority(actor): Authority, query: Result<Query<ListQuery>, QueryRejection>) -> Result<Json<Page<CodexProfileViewV1>>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.codex_profiles(&actor, &query).await?))
}

#[utoipa::path(get,path="/api/v2/settings/codex/{id}",operation_id="getCodexProfile",tag="Codex settings",params(("id"=Id,Path)),responses((status=200,body=CodexProfileViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem)))]
pub async fn profile(State(state): State<AppState>, Authority(actor): Authority, id: Result<Path<Id>, PathRejection>) -> Result<Json<CodexProfileViewV1>, ApiError> {
    Ok(Json(state.store.codex_profile(&actor, path(id)?).await?))
}

#[utoipa::path(get,path="/api/v2/codex/homes",operation_id="listCodexHomeBindings",tag="Codex settings",responses((status=200,body=Vec<CodexHomeBindingV1>),(status=401,body=Problem),(status=403,body=Problem),(status=429,body=Problem)))]
pub async fn homes(State(state): State<AppState>, Authority(actor): Authority) -> Result<Json<Vec<CodexHomeBindingV1>>, ApiError> {
    state.store.authorize_codex_settings_read(&actor).await?;
    Ok(Json(state.codex_deployment.public_bindings()))
}

#[utoipa::path(post,path="/api/v2/settings/codex",operation_id="createCodexProfile",tag="Codex settings",request_body=CodexProfileCreateV1,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<CodexProfileViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn create(State(state): State<AppState>, Authority(actor): Authority, headers: HeaderMap, body: Result<Json<CodexProfileCreateV1>, JsonRejection>) -> Result<(StatusCode, Json<CommandResult<CodexProfileViewV1>>), ApiError> {
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let store = state.store.clone();
    let deployment = state.codex_deployment.clone();
    let vault = state.vault.clone();
    let result = crate::settings::command(&state, async move {
        store.create_codex_profile(&actor, &key, &request, move |binding| async move {
            deployment.verify(binding, vault).await
        }).await
    }).await?;
    Ok((StatusCode::CREATED, Json(result)))
}

async fn update_profile(state: &AppState, actor: store::authority::Actor, key: String, id: Id, request: CodexProfileUpdateV1) -> Result<Json<CommandResult<CodexProfileViewV1>>, ApiError> {
    let store = state.store.clone();
    let deployment = state.codex_deployment.clone();
    let vault = state.vault.clone();
    let result = crate::settings::command(state, async move {
        store.update_codex_profile(&actor, &key, id, &request, move |binding| async move {
            deployment.verify(binding, vault).await
        }).await
    }).await?;
    Ok(Json(result))
}

#[utoipa::path(patch,path="/api/v2/settings/codex/{id}",operation_id="updateCodexProfile",tag="Codex settings",request_body=CodexProfileUpdateV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<CodexProfileViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn update(State(state): State<AppState>, Authority(actor): Authority, headers: HeaderMap, id: Result<Path<Id>, PathRejection>, body: Result<Json<CodexProfileUpdateV1>, JsonRejection>) -> Result<Json<CommandResult<CodexProfileViewV1>>, ApiError> {
    update_profile(&state, actor, idempotency_key(&headers)?.to_owned(), path(id)?, json(body)?).await
}

#[utoipa::path(patch,path="/api/v2/settings/codex",operation_id="updateCodexSettings",tag="Codex settings",request_body=CodexSettingsUpdateV1,params(("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<CodexProfileViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn update_selected(State(state): State<AppState>, Authority(actor): Authority, headers: HeaderMap, body: Result<Json<CodexSettingsUpdateV1>, JsonRejection>) -> Result<Json<CommandResult<CodexProfileViewV1>>, ApiError> {
    let request = json(body)?;
    update_profile(&state, actor, idempotency_key(&headers)?.to_owned(), request.profile_id, request.request).await
}

#[utoipa::path(post,path="/api/v2/codex/probe",operation_id="probeCodexProfile",tag="Codex settings",request_body=CodexProbeRequestV1,params(("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<CodexProbeViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn probe(State(state): State<AppState>, Authority(actor): Authority, headers: HeaderMap, body: Result<Json<CodexProbeRequestV1>, JsonRejection>) -> Result<Json<CommandResult<CodexProbeViewV1>>, ApiError> {
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let store = state.store.clone();
    let deployment = state.codex_deployment.clone();
    let vault = state.vault.clone();
    let result = crate::settings::command(&state, async move {
        match store.prepare_codex_probe(&actor, &key, &request).await? {
            CodexProbePreparation::Replay(result) => Ok(*result),
            CodexProbePreparation::Execute(ticket) => {
                let outcome = deployment.probe(&ticket.snapshot, vault).await;
                store.complete_codex_probe(*ticket, outcome).await
            }
        }
    }).await?;
    Ok(Json(result))
}

#[utoipa::path(get,path="/api/v2/codex/models",operation_id="getCodexModelObservation",tag="Codex settings",params(("profile_id"=Id,Query)),responses((status=200,body=CodexObservationV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem)))]
pub async fn models(State(state): State<AppState>, Authority(actor): Authority, query: Result<Query<CodexProfileQueryV1>, QueryRejection>) -> Result<Json<CodexObservationV1>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.codex_observation(&actor, query.profile_id).await?))
}

#[utoipa::path(get,path="/api/v2/codex/account",operation_id="getCodexAccountObservation",tag="Codex settings",params(("profile_id"=Id,Query)),responses((status=200,body=CodexObservationV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem)))]
pub async fn account(State(state): State<AppState>, Authority(actor): Authority, query: Result<Query<CodexProfileQueryV1>, QueryRejection>) -> Result<Json<CodexObservationV1>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.codex_observation(&actor, query.profile_id).await?))
}
