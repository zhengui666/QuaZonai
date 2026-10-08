//! Transport for the existing immutable recorded-feature Store operations.
use super::*;

#[utoipa::path(get,operation_id="list_recorded_features",path="/api/v2/data/revisions/{id}/features",tag="Data administration",params(("id"=Id,Path),("project_id"=Id,Query)),responses((status=200,body=RecordedFeatureListV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    value: Result<Query<RecordedFeatureListQuery>, QueryRejection>,
) -> Result<Json<RecordedFeatureListV1>, ApiError> {
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    Ok(Json(
        state
            .store
            .list_recorded_features(&actor, path(id)?, &query(value)?, move |id, size| {
                let objects = objects.clone();
                async move {
                    tokio::task::spawn_blocking(move || objects.read(id, size))
                    .await
                    .map_err(|_| StoreError::Integrity)?
                    .map_err(|_| StoreError::Integrity)
                }
            })
            .await?,
    ))
}

#[utoipa::path(post,operation_id="register_recorded_feature",path="/api/v2/data/revisions/{id}/features",tag="Data administration",request_body=RecordedFeatureRegisterV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<RecordedFeatureViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn register(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<RecordedFeatureRegisterV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<RecordedFeatureViewV1>>), ApiError> {
    let dataset = path(id)?;
    let request = json(body)?;
    if request.dataset_revision_id != dataset {
        return Err(ApiError::validation());
    }
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let store = state.store.clone();
    // Keep original bytes, authority and transaction alive if the HTTP
    // waiter disconnects while the non-abortable native file operation runs.
    let result = crate::settings::command(&state, async move {
        let reading = objects.clone();
        let publishing = objects.clone();
        let mut allocated = None;
        let result = store
            .register_recorded_feature(
                &actor,
                &key,
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
                        .map_err(crate::error::artifact_storage)
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
                tracing::warn!(artifact_id=%id, "recorded feature cleanup deferred");
            }
        }
        result
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}
