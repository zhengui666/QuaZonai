//! Operator-only projections; scientific execution and evidence disclosure stay distinct.
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
    evidence::{
        AlphaEvaluateRequestV1, AlphaVersionView, AlphaView, CalibrationView, EvaluationView,
        MetricValueV1, QualificationView,
    },
    research::ResearchListQuery,
    Id, Revision,
};
use store::StoreError;

#[utoipa::path(get,path="/api/v2/portfolio-candidates/{id}/evaluations",operation_id="list_candidate_evaluations",tag="Evidence",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<EvaluationView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn candidate_evaluations(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<EvaluationView>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .candidate_evaluations(&actor, id, &query)
            .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/alpha-versions/{id}/qualifications",operation_id="list_alpha_qualifications",tag="Evidence",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<QualificationView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn qualifications(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<QualificationView>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.alpha_qualifications(&actor, id, &query).await?,
    ))
}

#[utoipa::path(post,path="/api/v2/alpha-versions/{id}/evaluations",operation_id="start_alpha_evaluation",tag="Evidence",request_body=AlphaEvaluateRequestV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<contracts::runs::RunSnapshotV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn evaluate(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<AlphaEvaluateRequestV1>, JsonRejection>,
) -> Result<
    (
        StatusCode,
        Json<CommandResult<contracts::runs::RunSnapshotV1>>,
    ),
    ApiError,
> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        let reading = objects.clone();
        let publishing = objects.clone();
        let mut allocated = None;
        let result = store
            .start_alpha_evaluation(
                &actor,
                &key,
                id,
                &request,
                move |id, size| {
                    let objects = reading.clone();
                    async move {
                        tokio::task::spawn_blocking(move || objects.read(id, size))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    }
                },
                |object| {
                    allocated = Some(object.id);
                    async move {
                        tokio::task::spawn_blocking(move || {
                            publishing.put(object.id, &object.bytes)
                        })
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                    }
                },
            )
            .await;
        if let Some(id) = allocated.filter(|_| result.is_err()) {
            if store
                .discard_unpublished_operator_artifact(id, move |id| async move {
                    tokio::task::spawn_blocking(move || objects.discard_unpublished(id))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                })
                .await
                .is_err()
            {
                tracing::warn!(artifact_id=%id, "Sealed parameter cleanup deferred");
            }
        }
        result
    })
    .await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}

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

#[utoipa::path(get,path="/api/v2/alpha-versions/{id}/calibration",operation_id="get_alpha_calibration",tag="Evidence",params(("id"=Id,Path)),responses((status=200,body=CalibrationView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn calibration(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<CalibrationView>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.alpha_calibration(&actor, id).await?))
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
