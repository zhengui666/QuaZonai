//! Brief authoring is an authenticated facade over the shared command transaction.
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
    brief::*,
    control::{CommandResult, ListQuery, Page},
    Id, SchemaV1,
};
fn path(value: Result<Path<Id>, PathRejection>) -> Result<Id, ApiError> {
    value.map(|Path(id)| id).map_err(|_| ApiError::validation())
}
#[utoipa::path(get,path="/api/v2/projects/{id}/briefs",tag="Research briefs",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<BriefView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    q: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<BriefView>>, ApiError> {
    let q = q.map(|Query(q)| q).map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.briefs(&actor, path(id)?, &q).await?))
}
#[utoipa::path(get,path="/api/v2/briefs/{id}",tag="Research briefs",params(("id"=Id,Path)),responses((status=200,body=BriefView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<BriefView>, ApiError> {
    Ok(Json(state.store.brief(&actor, path(id)?).await?))
}
#[utoipa::path(post,path="/api/v2/projects/{id}/briefs",tag="Research briefs",params(("id"=Id,Path),("Idempotency-Key"=String,Header)),request_body=BriefCreate,responses((status=201,body=CommandResult<BriefView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn create(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<BriefCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<BriefView>>), ApiError> {
    let intent = BriefCreateIntent {
        schema_version: SchemaV1,
        project_id: path(id)?,
        request: json(body)?,
    };
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_brief(&actor, idempotency_key(&headers)?, &intent)
                .await?,
        ),
    ))
}
#[utoipa::path(patch,path="/api/v2/briefs/{id}",tag="Research briefs",params(("id"=Id,Path),("Idempotency-Key"=String,Header)),request_body=BriefUpdate,responses((status=200,body=CommandResult<BriefView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn update(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<BriefUpdate>, JsonRejection>,
) -> Result<Json<CommandResult<BriefView>>, ApiError> {
    Ok(Json(
        state
            .store
            .update_brief(&actor, idempotency_key(&headers)?, path(id)?, &json(body)?)
            .await?,
    ))
}
