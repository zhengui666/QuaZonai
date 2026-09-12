//! Operator-only projections; scientific execution and evidence disclosure stay distinct.
use crate::{
    access::Authority,
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{
        rejection::{PathRejection, QueryRejection},
        Path, Query, State,
    },
    Json,
};
use contracts::{
    control::{ListQuery, Page},
    evidence::{AlphaVersionView, AlphaView, EvaluationView, MetricValueV1},
    research::ResearchListQuery,
    Id, Revision,
};

#[utoipa::path(get,path="/api/v2/alphas",operation_id="list_alphas",tag="Evidence",params(("project_id"=Id,Query),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<AlphaView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn alphas(
    State(state): State<AppState>,
    Authority(actor): Authority,
    query: Result<Query<ResearchListQuery>, QueryRejection>,
) -> Result<Json<Page<AlphaView>>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.alphas(&actor, &query).await?))
}

#[utoipa::path(get,path="/api/v2/alphas/{id}/versions",operation_id="list_alpha_versions",tag="Evidence",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<AlphaVersionView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn versions(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<AlphaVersionView>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.alpha_versions(&actor, id, &query).await?))
}

#[utoipa::path(get,path="/api/v2/alphas/{id}/versions/{version}",operation_id="get_alpha_version",tag="Evidence",params(("id"=Id,Path),("version"=Revision,Path)),responses((status=200,body=AlphaVersionView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn version(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<(Id, Revision)>, PathRejection>,
) -> Result<Json<AlphaVersionView>, ApiError> {
    let Path((id, version)) = path.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.alpha_version(&actor, id, version).await?))
}

#[utoipa::path(get,path="/api/v2/alpha-versions/{id}/evaluations",operation_id="list_alpha_evaluations",tag="Evidence",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<EvaluationView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn evaluations(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<EvaluationView>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.alpha_evaluations(&actor, id, &query).await?,
    ))
}

#[utoipa::path(get,path="/api/v2/evaluations/{id}",operation_id="get_evaluation",tag="Evidence",params(("id"=Id,Path)),responses((status=200,body=EvaluationView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn evaluation(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<EvaluationView>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.evaluation(&actor, id).await?))
}

#[utoipa::path(get,path="/api/v2/evaluations/{id}/metrics",operation_id="list_evaluation_metrics",tag="Evidence",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<MetricValueV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn metrics(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<MetricValueV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.evaluation_metrics(&actor, id, &query).await?,
    ))
}
