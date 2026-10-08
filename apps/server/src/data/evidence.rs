//! Bounded owner-only projection; native locators and documents stay server-side.
use super::*;

#[utoipa::path(get,operation_id="dataset_evidence",path="/api/v2/data/revisions/{id}/evidence",tag="Data administration",params(("id"=Id,Path)),responses((status=200,body=DatasetEvidenceViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<DatasetEvidenceViewV1>, ApiError> {
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    Ok(Json(state.store.get_dataset_evidence(&actor, path(id)?, move |id, size| {
        let objects = objects.clone();
        async move {
            tokio::task::spawn_blocking(move || objects.read(id, size)).await.map_err(|_| StoreError::Integrity)?
                .map_err(|_| StoreError::Integrity)
        }
    }).await.map_err(|error| match error {
        StoreError::Invalid("dataset_evidence_sealed") => ApiError::new(
            StatusCode::UNPROCESSABLE_ENTITY,
            "DATASET_EVIDENCE_SEALED_UNSUPPORTED",
            "此证据摘要只支持非 Sealed 数据集；请使用 data revision show 查看既有登记信息。",
        ),
        other => other.into(),
    })?))
}
