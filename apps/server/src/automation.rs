//! Explicit frozen policy management, never an Agent-controlled switch.
use crate::{
    access::{idempotency_key, Authority},
    auth::json,
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    Json,
};
use contracts::{
    control::{CommandResult, ListQuery, Page},
    delivery::*,
    Id,
};

#[utoipa::path(post,path="/api/v2/projects/{id}/automation-policies",operation_id="authorize_automation",tag="Automation",request_body=AutomationAuthorizeV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<AutomationPolicyViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn authorize_automation(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<AutomationAuthorizeV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<AutomationPolicyViewV1>>), ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .authorize_automation(&actor, idempotency_key(&headers)?, id, &json(body)?)
                .await?,
        ),
    ))
}

#[utoipa::path(post,path="/api/v2/automation-policies/{id}/revoke",operation_id="revoke_automation",tag="Automation",request_body=PolicyRevokeV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<PolicyRevocationViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn revoke_automation(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<PolicyRevokeV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<PolicyRevocationViewV1>>), ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .revoke_automation(&actor, idempotency_key(&headers)?, id, &json(body)?)
                .await?,
        ),
    ))
}

#[utoipa::path(get,path="/api/v2/projects/{id}/automation-policies",operation_id="automation_policies",tag="Automation",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<AutomationPolicyViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn automation_policies(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<AutomationPolicyViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.automation_policies(&actor, id, &query).await?,
    ))
}

#[utoipa::path(get,path="/api/v2/automation-policies/{id}",operation_id="automation_policy",tag="Automation",params(("id"=Id,Path)),responses((status=200,body=AutomationPolicyViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn automation_policy(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<AutomationPolicyViewV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;

    Ok(Json(state.store.automation_policy(&actor, id).await?))
}

#[utoipa::path(get,path="/api/v2/automation-policies/{id}/revocations",operation_id="automation_revocations",tag="Automation",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<PolicyRevocationViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn automation_revocations(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<PolicyRevocationViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .automation_revocations(&actor, id, &query)
            .await?,
    ))
}
