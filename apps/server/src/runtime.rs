//! Real capability probe and observation-backed readiness, not config-as-success.
use crate::{
    access::{idempotency_key, Authority},
    auth::json,
    error::{ApiError, Problem},
    runtime_transport::RuntimeTransport,
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
use contracts::{control::CommandResult, runtime::*, Id};
use store::{runtime::ProbePreparation, StoreError};

#[utoipa::path(post,path="/api/v2/integrations/runtimes/{id}/probe",tag="Runtime readiness",request_body=RuntimeProbeRequestV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<RuntimeProbeViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn probe(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<RuntimeProbeRequestV1>, JsonRejection>,
) -> Result<Json<CommandResult<RuntimeProbeViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
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
            .prepare_runtime_probe(&actor, &key, id, &request)
            .await?
        {
            ProbePreparation::Replay(result) => return Ok(*result),
            ProbePreparation::Pending(ticket) => *ticket,
        };
        let snapshot = ticket.snapshot.clone();
        let native = tokio::task::spawn_blocking(move || {
            let credential_id: Id = snapshot
                .credential_ref
                .clone()
                .try_into()
                .map_err(|_| RuntimeProbeFailure::NotConfigured)?;
            let credential = vault
                .read(credential_id, "RUNTIME")
                .map_err(|_| RuntimeProbeFailure::Authentication)?;
            let ca = snapshot
                .ca_certificate_ref
                .as_ref()
                .map(|reference| {
                    let id: Id = reference
                        .clone()
                        .try_into()
                        .map_err(|_| RuntimeProbeFailure::TlsConfiguration)?;
                    vault
                        .read(id, "TLS_CA")
                        .map_err(|_| RuntimeProbeFailure::TlsConfiguration)
                })
                .transpose()?;
            RuntimeTransport::new(&targets, &snapshot, &credential, ca.as_deref())
        })
        .await
        .map_err(|_| StoreError::SecretCleanup)?;
        let observed = match native {
            Ok(native) => native.capabilities().await,
            Err(reason) => Err(reason),
        };
        let outcome = match observed {
            Ok(capabilities) => RuntimeProbeOutcomeV1::Available {
                capabilities: Box::new(capabilities),
            },
            Err(reason) => RuntimeProbeOutcomeV1::Unavailable { reason },
        };
        store
            .complete_runtime_probe(ticket, outcome, move |artifact, bytes| async move {
                tokio::task::spawn_blocking(move || objects.put(artifact, &bytes))
                    .await
                    .map_err(|_| StoreError::Integrity)?
                    .map_err(|_| StoreError::Integrity)
            })
            .await
    })
    .await?;
    Ok(Json(result))
}

#[utoipa::path(get,path="/api/v2/integrations/runtimes/{id}/readiness",tag="Runtime readiness",params(("id"=Id,Path)),responses((status=200,body=RuntimeReadinessV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn readiness(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<RuntimeReadinessV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.runtime_readiness(&actor, id).await?))
}
