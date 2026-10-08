//! Original packages and explicit human decisions; never downstream execution.
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
    delivery::{
        ApprovalRevocationViewV1, ApprovalRevokeV1, ApprovalViewV1, HandoffAckV1, HandoffClaimV1,
        HandoffOfferV1, HandoffViewV1, PaperInitialExecutionConsumeV1, PaperInitialExecutionViewV1,
        ReleaseApproveV1, ReleaseDecisionViewV1, ReleaseRejectV1, ReleaseReopenV1,
    },
    strategy_portfolio::{HandoffClaimViewV2, ReleaseCreateEnvelopeV2, ReleaseViewEnvelopeV2},
    Id,
};
use store::StoreError;

#[utoipa::path(get,path="/api/v2/projects/{id}/releases",operation_id="list_releases",tag="Release",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<ReleaseViewEnvelopeV2>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<ReleaseViewEnvelopeV2>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.releases_envelope(&actor, id, &query).await?,
    ))
}

#[utoipa::path(post,path="/api/v2/releases",operation_id="create_release",tag="Release",request_body=ReleaseCreateEnvelopeV2,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<ReleaseViewEnvelopeV2>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn create(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<ReleaseCreateEnvelopeV2>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ReleaseViewEnvelopeV2>>), ApiError> {
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
        let mut allocated = Vec::new();
        let result = store
            .create_release_envelope(
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
                    allocated.push(object.id);
                    let objects = publishing.clone();
                    async move {
                        tokio::task::spawn_blocking(move || objects.put(object.id, &object.bytes))
                            .await
                            .map_err(|_| StoreError::Integrity)?
                            .map_err(crate::error::artifact_storage)
                    }
                },
            )
            .await;
        for id in allocated.into_iter().filter(|_| result.is_err()) {
            let objects = objects.clone();
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
                tracing::warn!(artifact_id=%id, "Release package cleanup deferred");
            }
        }
        result
    })
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/releases/{id}",operation_id="get_release",tag="Release",params(("id"=Id,Path)),responses((status=200,body=ReleaseViewEnvelopeV2),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<ReleaseViewEnvelopeV2>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.release_envelope(&actor, id).await?))
}

#[utoipa::path(post,path="/api/v2/releases/{id}/rejections",operation_id="reject_release",tag="Release",request_body=ReleaseRejectV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<ReleaseDecisionViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn reject(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<ReleaseRejectV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ReleaseDecisionViewV1>>), ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .reject_release(&actor, idempotency_key(&headers)?, id, &json(body)?)
                .await?,
        ),
    ))
}
#[utoipa::path(post,path="/api/v2/release-decisions/{id}/reopen",operation_id="reopen_release",tag="Release",request_body=ReleaseReopenV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<ReleaseDecisionViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn reopen(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    headers: HeaderMap,
    body: Result<Json<ReleaseReopenV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ReleaseDecisionViewV1>>), ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .reopen_release(&actor, idempotency_key(&headers)?, id, &json(body)?)
                .await?,
        ),
    ))
}
#[utoipa::path(get,path="/api/v2/releases/{id}/decisions",operation_id="list_release_decisions",tag="Release",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<ReleaseDecisionViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn decisions(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<ReleaseDecisionViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.release_decisions(&actor, id, &query).await?,
    ))
}

#[utoipa::path(get,path="/api/v2/releases/{id}/approvals",operation_id="list_release_approvals",tag="Release",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<ApprovalViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn approvals(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<ApprovalViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.release_approvals(&actor, id, &query).await?,
    ))
}

#[utoipa::path(post,path="/api/v2/releases/{id}/approvals",operation_id="approve_release",tag="Release",request_body=ReleaseApproveV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<ApprovalViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn approve(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<ReleaseApproveV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ApprovalViewV1>>), ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store
            .approve_release(&actor, &key, id, &request, move |id, size| {
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
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/approvals/{id}",operation_id="get_approval",tag="Release",params(("id"=Id,Path)),responses((status=200,body=ApprovalViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn approval(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<ApprovalViewV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.approval(&actor, id).await?))
}

#[utoipa::path(post,path="/api/v2/handoffs",operation_id="offer_handoff",tag="Release",request_body=HandoffOfferV1,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<HandoffViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn offer(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<HandoffOfferV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<HandoffViewV1>>), ApiError> {
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store
            .offer_handoff(&actor, &key, &request, move |id, size| {
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
    .await?;
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/projects/{id}/handoffs",operation_id="list_handoffs",tag="Release",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<HandoffViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn handoffs(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<HandoffViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.handoffs(&actor, id, &query).await?))
}

#[utoipa::path(get,path="/api/v2/handoffs/{id}",operation_id="get_handoff",tag="Release",params(("id"=Id,Path)),responses((status=200,body=HandoffViewV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn handoff(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<HandoffViewV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.handoff(&actor, id).await?))
}

#[utoipa::path(post,path="/api/v2/handoffs/{id}/claim",operation_id="claim_handoff",tag="Release",request_body=HandoffClaimV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<HandoffClaimViewV2>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn claim(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<HandoffClaimV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<HandoffClaimViewV2>>), ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::Invalid("artifact_store_unavailable"))?;
    let store = state.store.clone();
    let result = crate::settings::command(&state, async move {
        store
            .claim_handoff_envelope(&actor, &key, id, &request, move |id, size| {
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
    .await?;
    Ok((StatusCode::OK, Json(result)))
}

/// Atomically consumes one explicit Paper model initialization. It never runs a host.
#[utoipa::path(post,path="/api/v2/handoffs/{id}/paper-initial-execution/consume",operation_id="consume_paper_initial_execution",tag="Release",request_body=PaperInitialExecutionConsumeV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<PaperInitialExecutionViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn consume_paper_initial_execution(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<PaperInitialExecutionConsumeV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<PaperInitialExecutionViewV1>>), ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let result = state
        .store
        .consume_paper_initial_execution(&actor, idempotency_key(&headers)?, id, &request)
        .await?;
    Ok((StatusCode::OK, Json(result)))
}

#[utoipa::path(post,path="/api/v2/handoffs/{id}/ack",operation_id="acknowledge_handoff",tag="Release",request_body=HandoffAckV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<HandoffViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn ack(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<HandoffAckV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<HandoffViewV1>>), ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let request = json(body)?;
    let result = state
        .store
        .acknowledge_handoff(&actor, idempotency_key(&headers)?, id, &request)
        .await?;
    Ok((StatusCode::OK, Json(result)))
}

#[utoipa::path(post,path="/api/v2/approvals/{id}/revoke",operation_id="revoke_approval",tag="Release",request_body=ApprovalRevokeV1,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<ApprovalRevocationViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn revoke_approval(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<ApprovalRevokeV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ApprovalRevocationViewV1>>), ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .revoke_approval(&actor, idempotency_key(&headers)?, id, &json(body)?)
                .await?,
        ),
    ))
}
#[utoipa::path(get,path="/api/v2/approvals/{id}/revocations",operation_id="list_approval_revocations",tag="Release",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<ApprovalRevocationViewV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn revocations(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<ApprovalRevocationViewV1>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.approval_revocations(&actor, id, &query).await?,
    ))
}

#[cfg(test)]
mod target_delivery_v2_tests {
    use super::*;
    use axum::{body::Body, extract::FromRequest, http::Request, response::IntoResponse};
    use serde_json::{json, Value};

    #[tokio::test]
    async fn target_v2_http_claim_extractor_rejects_v1_without_dispatch() {
        for version in [json!("1"), json!(1), json!(2), Value::Null] {
            let request = Request::builder().method("POST").header("content-type", "application/json")
                .body(Body::from(serde_json::to_vec(&json!({
                    "schema_version":1,"external_claim_id":"original-claim","package_schema_version":version
                })).unwrap())).unwrap();
            let parsed = Json::<HandoffClaimV1>::from_request(request, &()).await;
            let error = crate::auth::json(parsed).unwrap_err();
            assert_eq!(
                error.into_response().status(),
                StatusCode::UNPROCESSABLE_ENTITY
            );
        }
        let request = Request::builder().method("POST").header("content-type", "application/json")
            .body(Body::from(r#"{"schema_version":1,"external_claim_id":"original-claim","package_schema_version":"2"}"#)).unwrap();
        let parsed =
            crate::auth::json(Json::<HandoffClaimV1>::from_request(request, &()).await).unwrap();
        assert!(parsed.package_schema_version.is_deliverable());
        assert_eq!(parsed.external_claim_id, "original-claim");
        // Parsing does not mint authority or replace the existing idempotency gate.
        assert!(idempotency_key(&HeaderMap::new()).is_err());
    }

    #[test]
    fn target_v2_http_schema_exposes_only_active_bodies_and_keeps_claim_guards() {
        let api: Value = serde_json::from_str(&crate::openapi_json().unwrap()).unwrap();
        let schemas = &api["components"]["schemas"];
        assert!(schemas.get("TargetPackageV1").is_none());
        assert!(schemas.get("ForecastTargetPackageV2").is_some());
        assert!(schemas.get("TargetPackageV2").is_some());
        let envelope = schemas["TargetPackageEnvelopeV2"].to_string();
        assert!(envelope.contains("ForecastTargetPackageV2"));
        assert!(envelope.contains("TargetPackageV2"));
        assert!(!envelope.contains("TargetPackageV1"));
        let claim = &api["paths"]["/api/v2/handoffs/{id}/claim"]["post"];
        assert!(claim["parameters"]
            .as_array()
            .unwrap()
            .iter()
            .any(|parameter| {
                parameter["name"] == "Idempotency-Key" && parameter["required"] == true
            }));
        assert!(claim["security"]
            .as_array()
            .is_some_and(|rules| !rules.is_empty()));
        for status in ["401", "403", "409", "422"] {
            assert!(claim["responses"].get(status).is_some());
        }
        let response = claim["responses"]["200"].to_string();
        assert!(response.contains("HandoffClaimViewV2"));
    }

    #[test]
    fn target_v2_http_schema_reference_closure_preserves_active_and_historical_versions() {
        fn assert_refs(value: &Value, document: &Value) {
            match value {
                Value::Object(fields) => {
                    if let Some(reference) = fields.get("$ref") {
                        let reference = reference.as_str().expect("schema reference is a string");
                        let pointer = reference
                            .strip_prefix('#')
                            .expect("HTTP schema is self-contained");
                        assert!(
                            document.pointer(pointer).is_some(),
                            "unresolved schema reference: {reference}"
                        );
                    }
                    for field in fields.values() {
                        assert_refs(field, document);
                    }
                }
                Value::Array(items) => {
                    for item in items {
                        assert_refs(item, document);
                    }
                }
                _ => {}
            }
        }
        let api: Value = serde_json::from_str(&crate::openapi_json().unwrap()).unwrap();
        assert_refs(&api, &api);
        let schemas = &api["components"]["schemas"];
        assert_eq!(
            schemas["DownstreamDeliveryModeV1"]["enum"],
            json!(["TARGET_ONLY"])
        );
        let available = schemas["DownstreamProbeOutcomeV1"]["oneOf"]
            .as_array()
            .unwrap()
            .iter()
            .find(|variant| variant["properties"]["status"]["enum"] == json!(["AVAILABLE"]))
            .expect("historical available observation");
        assert_eq!(
            available["properties"]["capabilities"]["properties"]["accepted_package_versions"]
                ["items"]["enum"],
            json!(["1", "2"])
        );
        for name in ["DownstreamCreate", "DownstreamUpdate"] {
            assert_eq!(
                schemas[name]["properties"]["configuration"]["properties"]
                    ["accepted_package_versions"]["items"]["enum"],
                json!(["2"])
            );
        }
        assert_eq!(schemas["TargetPackageVersionV2"]["enum"], json!(["2"]));
        assert!(schemas.get("TargetPackageV1").is_none());
    }
}
