//! A read-only value projection, not a general restricted-artifact download.
use crate::{
    access::Authority,
    artifacts::ArtifactCapacity,
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{
        rejection::{PathRejection, QueryRejection},
        Path, Query, State,
    },
    http::StatusCode,
    Json,
};
use contracts::{
    equity_curve::{EquityCurveQuery, EquityCurveV1},
    Id,
};
use store::StoreError;

#[utoipa::path(
    get, path = "/api/v2/evaluations/{id}/equity-curve",
    operation_id = "get_evaluation_equity_curve", tag = "Evidence",
    params(("id" = Id, Path), EquityCurveQuery),
    responses(
        (status = 200, body = EquityCurveV1),
        (status = 401, body = Problem), (status = 403, body = Problem),
        (status = 404, body = Problem), (status = 422, body = Problem),
        (status = 429, body = Problem), (status = 503, body = Problem)
    )
)]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    capacity: ArtifactCapacity,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<EquityCurveQuery>, QueryRejection>,
) -> Result<Json<EquityCurveV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    let objects = state.artifact_store.clone().ok_or_else(|| {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            "ARTIFACT_STORAGE_UNAVAILABLE",
            "产物存储不可用。",
        )
    })?;
    // Retain the existing capacity permit while non-abortable native reads or
    // JSON projection finish, even when the browser disconnects mid-request.
    let result = tokio::spawn(async move {
        let _capacity = capacity;
        state
            .store
            .evaluation_equity_curve(&actor, id, &query, move |id, size| {
                let objects = objects.clone();
                async move {
                    tokio::task::spawn_blocking(move || objects.read(id, size))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                }
            })
            .await
    })
    .await
    .map_err(|_| ApiError::internal())??;
    Ok(Json(result))
}
