//! Formal data administration. Native metadata arrives through the existing
//! deployment-authorized Runtime client, never as caller-asserted provenance.
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
    data::*,
    Id,
};
use store::{data_registration::RegistrationPreparation, StoreError};

fn path(value: Result<Path<Id>, PathRejection>) -> Result<Id, ApiError> {
    value.map(|Path(id)| id).map_err(|_| ApiError::validation())
}
fn query<T>(value: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    value
        .map(|Query(value)| value)
        .map_err(|_| ApiError::validation())
}

#[utoipa::path(get,path="/api/v2/data/sources",tag="Data administration",params(("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query)),responses((status=200,body=Page<DataSourceView>),(status=401,body=Problem),(status=403,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn sources(
    State(state): State<AppState>,
    Authority(actor): Authority,
    value: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<DataSourceView>>, ApiError> {
    Ok(Json(
        state
            .store
            .list_data_sources(&actor, &query(value)?)
            .await?,
    ))
}

#[utoipa::path(post,path="/api/v2/data/sources",tag="Data administration",request_body=DataSourceCreate,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<DataSourceView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn create_source(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<DataSourceCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<DataSourceView>>), ApiError> {
    let request = json(body)?;
    let result = state
        .store
        .create_data_source(&actor, idempotency_key(&headers)?, &request)
        .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/data/sources/{id}",tag="Data administration",params(("id"=Id,Path)),responses((status=200,body=DataSourceView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn source(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<DataSourceView>, ApiError> {
    Ok(Json(state.store.get_data_source(&actor, path(id)?).await?))
}

#[utoipa::path(patch,path="/api/v2/data/sources/{id}",tag="Data administration",request_body=DataSourceUpdate,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<DataSourceView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn update_source(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<DataSourceUpdate>, JsonRejection>,
) -> Result<Json<CommandResult<DataSourceView>>, ApiError> {
    Ok(Json(
        state
            .store
            .update_data_source(&actor, idempotency_key(&headers)?, path(id)?, &json(body)?)
            .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/data/sources/{id}/grants",tag="Data administration",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query)),responses((status=200,body=Page<DataGrantView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn grants(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    value: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<DataGrantView>>, ApiError> {
    Ok(Json(
        state
            .store
            .list_data_grants(&actor, path(id)?, &query(value)?)
            .await?,
    ))
}

#[utoipa::path(post,path="/api/v2/data/sources/{id}/grants",tag="Data administration",request_body=DataGrantCreate,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<DataGrantView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn create_grant(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<DataGrantCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<DataGrantView>>), ApiError> {
    let source = path(id)?;
    let request = json(body)?;
    if source != request.source_id {
        return Err(ApiError::validation());
    }
    let result = state
        .store
        .create_data_grant(&actor, idempotency_key(&headers)?, &request)
        .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(post,path="/api/v2/data/grants/{id}/revoke",tag="Data administration",request_body=DataGrantRevoke,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<DataGrantRevocationView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn revoke_grant(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<DataGrantRevoke>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<DataGrantRevocationView>>), ApiError> {
    let result = state
        .store
        .revoke_data_grant(&actor, idempotency_key(&headers)?, path(id)?, &json(body)?)
        .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/data/grants/{id}/revocations",tag="Data administration",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query)),responses((status=200,body=Page<DataGrantRevocationView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn revocations(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    value: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<DataGrantRevocationView>>, ApiError> {
    Ok(Json(
        state
            .store
            .list_data_revocations(&actor, path(id)?, &query(value)?)
            .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/data/revisions",tag="Data administration",params(("source_id"=Option<Id>,Query),("partition"=Option<contracts::research::DataPartition>,Query),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query)),responses((status=200,body=Page<DatasetView>),(status=401,body=Problem),(status=403,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn revisions(
    State(state): State<AppState>,
    Authority(actor): Authority,
    value: Result<Query<DataListQuery>, QueryRejection>,
) -> Result<Json<Page<DatasetView>>, ApiError> {
    Ok(Json(
        state
            .store
            .list_dataset_revisions(&actor, &query(value)?)
            .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/data/revisions/{id}",tag="Data administration",params(("id"=Id,Path)),responses((status=200,body=DatasetView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn revision(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<DatasetView>, ApiError> {
    Ok(Json(
        state.store.get_dataset_revision(&actor, path(id)?).await?,
    ))
}

#[utoipa::path(get,path="/api/v2/data/universes",tag="Data administration",params(("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query)),responses((status=200,body=Page<UniverseView>),(status=401,body=Problem),(status=403,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn universes(
    State(state): State<AppState>,
    Authority(actor): Authority,
    value: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<UniverseView>>, ApiError> {
    Ok(Json(
        state
            .store
            .list_universe_versions(&actor, &query(value)?)
            .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/data/universes/{id}",tag="Data administration",params(("id"=Id,Path)),responses((status=200,body=UniverseView),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn universe(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<UniverseView>, ApiError> {
    Ok(Json(
        state.store.get_universe_version(&actor, path(id)?).await?,
    ))
}

#[utoipa::path(post,path="/api/v2/data/revisions",tag="Data administration",request_body=DatasetRegister,params(("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<DatasetView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn register(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<DatasetRegister>, JsonRejection>,
) -> Result<Json<CommandResult<DatasetView>>, ApiError> {
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let store = state.store.clone();
    let vault = state.vault.clone();
    let targets = state.runtime_targets.clone();
    let result = crate::settings::command(&state, async move {
        let ticket = match store
            .prepare_dataset_registration(&actor, &key, &request)
            .await?
        {
            RegistrationPreparation::Replay(result) => return Ok(*result),
            RegistrationPreparation::Execute(ticket) => *ticket,
        };
        let snapshot = ticket.runtime_snapshot.clone();
        let native = tokio::task::spawn_blocking(move || {
            crate::runtime::native_transport(&vault, &targets, &snapshot)
        })
        .await
        .map_err(|_| StoreError::SecretCleanup)?
        .map_err(|_| StoreError::IntegrationUnavailable)?;
        let observed = native
            .catalog_metadata(
                &ticket.source.native_catalog_ref,
                &ticket.request.native_storage_version,
            )
            .await
            .map_err(|error| match error {
                crate::runtime_transport::RuntimeRequestError::Authentication
                | crate::runtime_transport::RuntimeRequestError::Unavailable => {
                    StoreError::IntegrationUnavailable
                }
                crate::runtime_transport::RuntimeRequestError::Missing => StoreError::NotFound,
                _ => StoreError::Domain(domain::research::invalid(
                    "native_storage_version",
                    "NATIVE_CATALOG_CONTRACT_INVALID",
                )),
            })?;
        let reading = objects.clone();
        let publishing = objects.clone();
        let mut allocated = Vec::new();
        let result = store
            .complete_dataset_registration(
                ticket,
                observed.raw_document,
                move |id, bytes| {
                    let objects = reading.clone();
                    async move {
                        tokio::task::spawn_blocking(move || objects.read(id, bytes))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    }
                },
                |publications| {
                    allocated.extend(publications.iter().map(|object| object.id));
                    async move {
                        tokio::task::spawn_blocking(move || {
                            for object in publications {
                                publishing
                                    .put(object.id, &object.bytes)
                                    .map_err(|_| StoreError::Integrity)?;
                            }
                            Ok(())
                        })
                        .await
                        .map_err(|_| StoreError::Integrity)?
                    }
                },
            )
            .await;
        if result.is_err() {
            for id in allocated {
                let objects = objects.clone();
                let cleanup = store
                    .discard_unpublished_operator_artifact(id, move |id| async move {
                        tokio::task::spawn_blocking(move || objects.discard_unpublished(id))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(|_| StoreError::Integrity)
                    })
                    .await;
                if cleanup.is_err() {
                    tracing::warn!(artifact_id = %id, "native_dataset_cleanup_deferred");
                }
            }
        }
        result
    })
    .await?;
    Ok(Json(result))
}
