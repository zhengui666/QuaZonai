//! Thin authenticated facade; immutable version ownership stays in Store.
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
    portfolio::*,
    Id,
};

#[utoipa::path(post,path="/api/v2/portfolio-mandates",operation_id="create_mandate",tag="Portfolio mandates",params(("Idempotency-Key"=String,Header)),request_body=MandateCreateV1,responses((status=201,body=CommandResult<MandateViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn create(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<MandateCreateV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<MandateViewV1>>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_mandate(&actor, idempotency_key(&headers)?, &json(body)?)
                .await?,
        ),
    ))
}

#[utoipa::path(get,path="/api/v2/projects/{id}/portfolio-mandates",operation_id="list_mandates",tag="Portfolio mandates",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<MandateViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<MandateViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.mandates(&actor, id, &query).await?))
}

#[utoipa::path(get,path="/api/v2/portfolio-mandates/{id}",operation_id="get_mandate",tag="Portfolio mandates",params(("id"=Id,Path)),responses((status=200,body=MandateViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<MandateViewV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.mandate(&actor, id).await?))
}
