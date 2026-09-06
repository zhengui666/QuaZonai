//! Authenticated preparation endpoints; all authority and publication live in Store transactions.
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
    control::{CommandResult, Page},
    research::*,
    Id,
};

fn path(value: Result<Path<Id>, PathRejection>) -> Result<Id, ApiError> {
    value.map(|Path(id)| id).map_err(|_| ApiError::validation())
}
fn query(
    value: Result<Query<ResearchListQuery>, QueryRejection>,
) -> Result<ResearchListQuery, ApiError> {
    value.map(|Query(q)| q).map_err(|_| ApiError::validation())
}
#[utoipa::path(get,path="/api/v2/input-sets",tag="Research preparation",params(("project_id"=Id,Query),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<InputSetSummary>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn input_sets(
    State(state): State<AppState>,
    Authority(actor): Authority,
    q: Result<Query<ResearchListQuery>, QueryRejection>,
) -> Result<Json<Page<InputSetSummary>>, ApiError> {
    Ok(Json(state.store.input_sets(&actor, &query(q)?).await?))
}
#[utoipa::path(get,path="/api/v2/input-sets/{id}",tag="Research preparation",params(("id"=Id,Path)),responses((status=200,body=InputSetView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn input_set(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<InputSetView>, ApiError> {
    Ok(Json(state.store.input_set(&actor, path(id)?).await?))
}
#[utoipa::path(post,path="/api/v2/input-sets",tag="Research preparation",request_body=InputSetCreate,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<InputSetView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn create_input_set(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<InputSetCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<InputSetView>>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_input_set(&actor, idempotency_key(&headers)?, &json(body)?)
                .await?,
        ),
    ))
}
#[utoipa::path(get,path="/api/v2/evaluation-policies",tag="Research preparation",params(("project_id"=Id,Query),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<EvaluationPolicyView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn evaluation_policies(
    State(state): State<AppState>,
    Authority(actor): Authority,
    q: Result<Query<ResearchListQuery>, QueryRejection>,
) -> Result<Json<Page<EvaluationPolicyView>>, ApiError> {
    Ok(Json(
        state.store.evaluation_policies(&actor, &query(q)?).await?,
    ))
}
#[utoipa::path(get,path="/api/v2/evaluation-policies/{id}",tag="Research preparation",params(("id"=Id,Path)),responses((status=200,body=EvaluationPolicyView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn evaluation_policy(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<EvaluationPolicyView>, ApiError> {
    Ok(Json(
        state.store.evaluation_policy(&actor, path(id)?).await?,
    ))
}
#[utoipa::path(post,path="/api/v2/evaluation-policies",tag="Research preparation",request_body=EvaluationPolicyCreate,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<EvaluationPolicyView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn create_evaluation_policy(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<EvaluationPolicyCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<EvaluationPolicyView>>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_evaluation_policy(&actor, idempotency_key(&headers)?, &json(body)?)
                .await?,
        ),
    ))
}
