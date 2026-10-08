//! Transport for capital intent and authenticated native evidence. No venue client.
use crate::{
    AppState,
    access::{Authority, idempotency_key},
    auth::json,
    error::{ApiError, Problem},
};
use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, StatusCode},
};
use contracts::{
    Id,
    capital_exit::*,
    control::{CommandResult, ListQuery, Page},
};
use store::StoreError;

async fn put(
    objects: std::sync::Arc<integrations::artifacts::ArtifactStore>,
    object: store::lifecycle::native::NativeObjectPublication,
) -> Result<(), StoreError> {
    tokio::task::spawn_blocking(move || objects.put(object.id, &object.bytes))
        .await
        .map_err(|_| StoreError::Integrity)?
        .map_err(crate::error::artifact_storage)
}
async fn cleanup(state: &AppState, project: Id, artifact: Option<Id>) {
    let (Some(artifact), Some(objects)) = (artifact, state.artifact_store.clone()) else {
        return;
    };
    if state
        .store
        .discard_unpublished_forward_artifact(project, artifact, |id| async move {
            tokio::task::spawn_blocking(move || objects.discard_unpublished(id))
                .await
                .map_err(|_| StoreError::Integrity)?
                .map_err(|_| StoreError::Integrity)
        })
        .await
        .is_err()
    {
        tracing::warn!(artifact_id=%artifact,"Capital control artifact cleanup deferred");
    }
}

#[utoipa::path(post,path="/api/v2/projects/{project_id}/capital-exit-previews",operation_id="preview_capital_exit",tag="CapitalExit",request_body=CapitalExitPreviewRequestV1,params(("project_id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<CapitalExitPreviewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn preview(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<CapitalExitPreviewRequestV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<CapitalExitPreviewV1>>), ApiError> {
    let Path(project) = path.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let execution = state.clone();
    let result = crate::settings::command(&state, async move {
        let mut allocated = None;
        let result = execution
            .store
            .preview_capital_exit(&actor, project, &key, &request, |object| {
                allocated = Some(object.id);
                put(objects, object)
            })
            .await;
        if result.is_err() {
            cleanup(&execution, project, allocated).await;
        }
        result
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}
#[utoipa::path(post,path="/api/v2/projects/{project_id}/capital-exits",operation_id="start_capital_exit",tag="CapitalExit",request_body=CapitalExitStartV1,params(("project_id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<CapitalExitViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn start(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<CapitalExitStartV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<CapitalExitViewV1>>), ApiError> {
    let Path(project) = path.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store
            .start_capital_exit(&actor, project, &key, &request)
            .await
    })
    .await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}
#[utoipa::path(get,path="/api/v2/projects/{project_id}/capital-exits",operation_id="list_capital_exits",tag="CapitalExit",params(("project_id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<CapitalExitViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<CapitalExitViewV1>>, ApiError> {
    let Path(project) = path.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.capital_exits(&actor, project, &query).await?,
    ))
}
#[utoipa::path(get,path="/api/v2/capital-exits/{id}",operation_id="get_capital_exit",tag="CapitalExit",params(("id"=Id,Path)),responses((status=200,body=CapitalExitViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
) -> Result<Json<CapitalExitViewV1>, ApiError> {
    let Path(id) = path.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.capital_exit(&actor, id).await?))
}

#[utoipa::path(post,path="/api/v2/capital-exits/{id}/pause",operation_id="pause_capital_exit",tag="CapitalExit",request_body=CapitalExitActionV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<CapitalExitViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn pause(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<CapitalExitActionV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<CapitalExitViewV1>>), ApiError> {
    let Path(id) = path.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    if request.action() != "PAUSE" {
        return Err(ApiError::validation());
    }
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store.act_capital_exit(&actor, id, &key, &request).await
    })
    .await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}

#[utoipa::path(post,path="/api/v2/capital-exits/{id}/cancel",operation_id="cancel_capital_exit",tag="CapitalExit",request_body=CapitalExitActionV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<CapitalExitViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn cancel(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<CapitalExitActionV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<CapitalExitViewV1>>), ApiError> {
    let Path(id) = path.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    if request.action() != "CANCEL" {
        return Err(ApiError::validation());
    }
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store.act_capital_exit(&actor, id, &key, &request).await
    })
    .await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}

#[utoipa::path(post,path="/api/v2/capital-exits/{id}/resume",operation_id="resume_capital_exit",tag="CapitalExit",request_body=CapitalExitActionV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<CapitalExitViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn resume(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<CapitalExitActionV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<CapitalExitViewV1>>), ApiError> {
    let Path(id) = path.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    if request.action() != "RESUME" {
        return Err(ApiError::validation());
    }
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store.act_capital_exit(&actor, id, &key, &request).await
    })
    .await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}

#[utoipa::path(post,path="/api/v2/capital-exits/{id}/reconcile-withdrawal",operation_id="reconcile_capital_exit",tag="CapitalExit",request_body=CapitalExitActionV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<CapitalExitViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn reconcile(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<CapitalExitActionV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<CapitalExitViewV1>>), ApiError> {
    let Path(id) = path.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    if request.action() != "RECONCILE_WITHDRAWAL" {
        return Err(ApiError::validation());
    }
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store.act_capital_exit(&actor, id, &key, &request).await
    })
    .await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/downstream/capital-exits",operation_id="list_downstream_capital_exits",tag="CapitalExit",params(("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<CapitalExitViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=422,body=Problem)))]
pub async fn downstream(
    State(state): State<AppState>,
    Authority(actor): Authority,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<CapitalExitViewV1>>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.downstream_capital_exits(&actor, &query).await?,
    ))
}
#[utoipa::path(post,path="/api/v2/capital-exits/{id}/claim",operation_id="claim_capital_exit",tag="CapitalExit",request_body=CapitalExitClaimV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<CapitalExitViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn claim(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<CapitalExitClaimV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<CapitalExitViewV1>>), ApiError> {
    let Path(id) = path.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store.claim_capital_exit(&actor, id, &key, &request).await
    })
    .await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}
#[utoipa::path(post,path="/api/v2/capital-exits/{id}/evidence",operation_id="submit_capital_exit_evidence",tag="CapitalExit",request_body=CapitalExitOwnerEvidenceV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<CapitalExitViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn evidence(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<CapitalExitOwnerEvidenceV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<CapitalExitViewV1>>), ApiError> {
    let Path(id) = path.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let project = state
        .store
        .machine_session(&actor)
        .await?
        .project_id
        .ok_or(StoreError::Forbidden)?;
    let execution = state.clone();
    let result = crate::settings::command(&state, async move {
        let mut allocated = None;
        let result = execution
            .store
            .submit_capital_exit_evidence(&actor, id, &key, &request, |object| {
                allocated = Some(object.id);
                put(objects, object)
            })
            .await;
        if result.is_err() {
            cleanup(&execution, project, allocated).await;
        }
        result
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}
#[utoipa::path(post,path="/api/v2/downstream/capital-exit-assessments",operation_id="submit_capital_exit_assessment",tag="CapitalExit",request_body=CapitalExitOwnerAssessmentV1,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<CapitalExitAssessmentReceiptV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn assessment(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<CapitalExitOwnerAssessmentV1>, JsonRejection>,
) -> Result<
    (
        StatusCode,
        Json<CommandResult<CapitalExitAssessmentReceiptV1>>,
    ),
    ApiError,
> {
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let project = state
        .store
        .machine_session(&actor)
        .await?
        .project_id
        .ok_or(StoreError::Forbidden)?;
    let execution = state.clone();
    let result = crate::settings::command(&state, async move {
        let mut allocated = None;
        let result = execution
            .store
            .submit_capital_exit_assessment(&actor, &key, &request, |object| {
                allocated = Some(object.id);
                put(objects, object)
            })
            .await;
        if result.is_err() {
            cleanup(&execution, project, allocated).await;
        }
        result
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/downstream/capital-exit-assessments",operation_id="list_capital_exit_assessment_requests",tag="CapitalExit",params(("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<CapitalExitPreviewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=422,body=Problem)))]
pub async fn assessment_requests(
    State(state): State<AppState>,
    Authority(actor): Authority,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<CapitalExitPreviewV1>>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .downstream_capital_exit_assessments(&actor, &query)
            .await?,
    ))
}
