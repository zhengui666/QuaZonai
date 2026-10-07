//! Transport-only intake and owner reads for downstream native account observations.
use crate::{
    access::Authority,
    auth::json,
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        Path, Query, State,
    },
    http::StatusCode,
    Json,
};
use contracts::{
    account_observation::*,
    control::{ListQuery, Page},
    Id,
};

#[utoipa::path(post,path="/api/v2/forward/account-observations",operation_id="submit_account_observation",tag="Forward",request_body=AccountObservationSubmitV1,responses((status=201,body=AccountObservationReceiptV1),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn submit(
    State(state): State<AppState>,
    Authority(actor): Authority,
    body: Result<Json<AccountObservationSubmitV1>, JsonRejection>,
) -> Result<(StatusCode, Json<AccountObservationReceiptV1>), ApiError> {
    let request = json(body)?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store.submit_account_observation(&actor, &request).await
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(post,path="/api/v2/forward/client-account-observations",operation_id="submit_client_account_observation",tag="Forward",request_body=AccountObservationSubmitV2,responses((status=201,body=AccountObservationReceiptV2),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn submit_client_bound(
    State(state): State<AppState>,
    Authority(actor): Authority,
    body: Result<Json<AccountObservationSubmitV2>, JsonRejection>,
) -> Result<(StatusCode, Json<AccountObservationReceiptV2>), ApiError> {
    let request = json(body)?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store
            .submit_client_account_observation(&actor, &request)
            .await
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/projects/{project_id}/account-sources/{source_id}/client-binding",operation_id="get_account_client_binding",tag="Forward",params(("project_id"=Id,Path),("source_id"=Id,Path)),responses((status=200,body=AccountClientBindingV2),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn client_binding(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<(Id, Id)>, PathRejection>,
) -> Result<Json<AccountClientBindingV2>, ApiError> {
    let Path((project, source)) = path.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .account_client_binding(&actor, project, source)
            .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/projects/{project_id}/account-sources",operation_id="list_account_sources",tag="Forward",params(("project_id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<AccountSourceV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn sources(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<AccountSourceV1>>, ApiError> {
    let Path(project) = path.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.account_sources(&actor, project, &query).await?,
    ))
}

#[utoipa::path(get,path="/api/v2/projects/{project_id}/account-sources/{source_id}/current",operation_id="get_account_current",tag="Forward",params(("project_id"=Id,Path),("source_id"=Id,Path)),responses((status=200,body=AccountCurrentV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem)))]
pub async fn current(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<(Id, Id)>, PathRejection>,
) -> Result<Json<AccountCurrentV1>, ApiError> {
    let Path((project, source)) = path.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.account_current(&actor, project, source).await?,
    ))
}

#[utoipa::path(get,path="/api/v2/projects/{project_id}/account-sources/{source_id}/observations",operation_id="list_account_observations",tag="Forward",params(("project_id"=Id,Path),("source_id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<AccountObservationV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=410,body=Problem),(status=422,body=Problem)))]
pub async fn observations(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<(Id, Id)>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<AccountObservationV1>>, ApiError> {
    let Path((project, source)) = path.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .account_observations(&actor, project, source, &query)
            .await?,
    ))
}
