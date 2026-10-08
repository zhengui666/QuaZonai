//! Transport-only intake and owner reads for downstream native account observations.
use crate::{
    AppState,
    access::Authority,
    auth::json,
    error::{ApiError, Problem},
};
use axum::{
    Json,
    extract::{
        Path, Query, State,
        rejection::{JsonRejection, PathRejection, QueryRejection},
    },
    http::{HeaderMap, HeaderValue, StatusCode},
};
use contracts::{
    Id,
    account_observation::*,
    control::{ListQuery, Page},
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

#[utoipa::path(post,path="/api/v2/forward/client-account-observations",operation_id="submit_client_account_observation",tag="Forward",request_body=AccountObservationSubmitV2,responses((status=201,body=AccountObservationReceiptV2,headers(("x-qz-capital-exit-source"=String,description="Present only after deployment-opted-in Paper owner registration succeeds for this exact original source. Absent means no capital-exit registration acknowledgement; it is not funds readiness."))),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn submit_client_bound(
    State(state): State<AppState>,
    Authority(actor): Authority,
    body: Result<Json<AccountObservationSubmitV2>, JsonRejection>,
) -> Result<(StatusCode, HeaderMap, Json<AccountObservationReceiptV2>), ApiError> {
    let request = json(body)?;
    let store = state.store.clone();
    let owners = state.paper_capital_exit_owners.clone();
    let (result, registered_source) = crate::settings::command(&state, async move {
        let receipt = store
            .submit_client_account_observation(&actor, &request)
            .await?;
        let registered = owners
            .register_observed_source(&store, &actor, &receipt)
            .await?;
        Ok((receipt, registered))
    })
    .await?;
    let mut headers = HeaderMap::new();
    if let Some(source) = registered_source {
        headers.insert(
            crate::paper_capital_exit::REGISTERED_SOURCE_HEADER,
            HeaderValue::from_str(&source.to_string()).map_err(|_| ApiError::internal())?,
        );
    }
    Ok((StatusCode::CREATED, headers, Json(result)))
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
