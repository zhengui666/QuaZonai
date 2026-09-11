//! Actual operator commands and project-scoped cycle reads; no fixture SQL API.
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
    cycles::*,
    Id, SchemaV1,
};

fn path(value: Result<Path<Id>, PathRejection>) -> Result<Id, ApiError> {
    value.map(|Path(id)| id).map_err(|_| ApiError::validation())
}

#[utoipa::path(post,path="/api/v2/briefs/{id}/freeze",operation_id="freezeResearchBrief",tag="Research startup",request_body=BriefFreezeV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<FrozenBriefV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn freeze(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<BriefFreezeV1>, JsonRejection>,
) -> Result<Json<CommandResult<FrozenBriefV1>>, ApiError> {
    Ok(Json(
        state
            .store
            .freeze_brief(&actor, idempotency_key(&headers)?, path(id)?, &json(body)?)
            .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/briefs/{id}/execution-context",operation_id="getResearchBriefExecutionContext",tag="Research startup",params(("id"=Id,Path)),responses((status=200,body=FrozenBriefV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn frozen(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<FrozenBriefV1>, ApiError> {
    Ok(Json(state.store.frozen_brief(&actor, path(id)?).await?))
}

#[utoipa::path(post,path="/api/v2/projects/{id}/cycles",operation_id="startResearchCycle",tag="Research startup",request_body=CycleStartV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<CycleStartedV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn start(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<CycleStartV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<CycleStartedV1>>), ApiError> {
    let intent = CycleStartIntent {
        schema_version: SchemaV1,
        project_id: path(id)?,
        request: json(body)?,
    };
    Ok((
        StatusCode::ACCEPTED,
        Json(
            state
                .store
                .start_cycle(&actor, idempotency_key(&headers)?, &intent)
                .await?,
        ),
    ))
}

#[utoipa::path(get,path="/api/v2/projects/{id}/cycles",operation_id="listProjectResearchCycles",tag="Research startup",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<CycleViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<CycleViewV1>>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.cycles(&actor, path(id)?, &query).await?))
}

#[utoipa::path(get,path="/api/v2/cycles/{id}",operation_id="getResearchCycle",tag="Research startup",params(("id"=Id,Path)),responses((status=200,body=CycleViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<CycleViewV1>, ApiError> {
    Ok(Json(state.store.cycle(&actor, path(id)?).await?))
}
