//! Real capability probe and observation-backed readiness, not config-as-success.
use crate::{
    access::{idempotency_key, Authority},
    auth::json,
    error::{ApiError, Problem},
    runtime_transport::DownstreamTransport,
    AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection},
        Path, State,
    },
    http::HeaderMap,
    Json,
};
use contracts::{control::CommandResult, delivery::*, Id};
use store::{downstream::ProbePreparation, StoreError};

#[utoipa::path(post,path="/api/v2/integrations/downstreams/{id}/probe",operation_id="probe_downstream",tag="Downstream readiness",request_body=DownstreamProbeRequestV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<DownstreamProbeViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn probe(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<DownstreamProbeRequestV1>, JsonRejection>,
) -> Result<Json<CommandResult<DownstreamProbeViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let store = state.store.clone();
    let vault = state.vault.clone();
    let targets = state.downstream_targets.clone();
    let result = crate::settings::command(&state, async move {
        let ticket = match store
            .prepare_downstream_probe(&actor, &key, id, &request)
            .await?
        {
            ProbePreparation::Replay(result) => return Ok(*result),
            ProbePreparation::Pending(ticket) => *ticket,
        };
        let snapshot = ticket.snapshot.clone();
        let native = tokio::task::spawn_blocking(move || {
            let credential = vault
                .read(snapshot.credential_ref, "DOWNSTREAM")
                .map_err(|_| contracts::runtime::RuntimeProbeFailure::Authentication)?;
            DownstreamTransport::new(
                &targets,
                &snapshot.endpoint,
                snapshot.development_http,
                &credential,
            )
        })
        .await
        .map_err(|_| StoreError::SecretCleanup)?;
        let observed = match native {
            Ok(native) => native.capabilities().await,
            Err(reason) => Err(reason),
        };
        let outcome = match observed {
            Ok(capabilities) => DownstreamProbeOutcomeV1::Available { capabilities },
            Err(reason) => DownstreamProbeOutcomeV1::Unavailable { reason },
        };
        let mut publication = None;
        let publishing_objects = objects.clone();
        let result = store
            .complete_downstream_probe(ticket, outcome, |artifact, bytes| {
                publication = Some(artifact);
                async move {
                    tokio::task::spawn_blocking(move || publishing_objects.put(artifact, &bytes))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                }
            })
            .await;
        if let Some(artifact) = publication.filter(|_| result.is_err()) {
            let cleanup = store
                .discard_unpublished_operator_artifact(artifact, move |artifact| async move {
                    tokio::task::spawn_blocking(move || objects.discard_unpublished(artifact))
                        .await
                        .map_err(|_| StoreError::Integrity)?
                        .map_err(|_| StoreError::Integrity)
                })
                .await;
            if cleanup.is_err() {
                // The original failure remains the response. Never delete on an
                // uncertain database read or expose storage/native diagnostics.
                tracing::warn!(artifact_id = %artifact, "downstream_probe_cleanup_deferred");
            }
        }
        result
    })
    .await?;
    Ok(Json(result))
}

#[utoipa::path(get,path="/api/v2/integrations/downstreams/{id}/readiness",operation_id="downstream_readiness",tag="Downstream readiness",params(("id"=Id,Path)),responses((status=200,body=DownstreamReadinessV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn readiness(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<DownstreamReadinessV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.downstream_readiness(&actor, id).await?))
}
