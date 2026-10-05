//! Thin authenticated facade; immutable version ownership stays in Store.
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
    portfolio::*,
    strategy_portfolio::*,
    Id,
};
use store::StoreError;

#[utoipa::path(get,path="/api/v2/projects/{id}/portfolio-candidates",operation_id="list_candidates",tag="Portfolio candidates",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<PortfolioCandidateListEnvelopeV2>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn candidates(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<PortfolioCandidateListEnvelopeV2>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.candidates_envelope(&actor, id, &query).await?,
    ))
}

#[utoipa::path(get,path="/api/v2/portfolio-candidates/{id}",operation_id="get_candidate",tag="Portfolio candidates",params(("id"=Id,Path)),responses((status=200,body=PortfolioCandidateEnvelopeV2),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn candidate(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<PortfolioCandidateEnvelopeV2>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.candidate_envelope(&actor, id).await?))
}

/// The published strategy candidate's original native result, never a new replay.
#[utoipa::path(get,path="/api/v2/portfolio-candidates/{id}/summary",operation_id="get_strategy_portfolio_summary",tag="Portfolio candidates",params(("id"=Id,Path)),responses((status=200,body=StrategyPortfolioSummaryV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn summary(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<StrategyPortfolioSummaryV1>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let objects = state
        .artifact_store
        .clone()
        .ok_or(StoreError::IntegrationUnavailable)?;
    Ok(Json(
        state
            .store
            .strategy_portfolio_summary(&actor, id, move |id, size| {
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

#[utoipa::path(post,path="/api/v2/portfolio-builds",operation_id="start_portfolio_build",tag="Portfolio",request_body=PortfolioBuildEnvelopeV2,params(("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<contracts::runs::RunSnapshotV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn build(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<PortfolioBuildEnvelopeV2>, JsonRejection>,
) -> Result<
    (
        StatusCode,
        Json<CommandResult<contracts::runs::RunSnapshotV1>>,
    ),
    ApiError,
> {
    run_portfolio(
        state,
        actor,
        headers,
        contracts::control::OperatorCommand::PortfolioBuild(Box::new(json(body)?)),
    )
    .await
}

#[utoipa::path(post,path="/api/v2/candidate-simulations",operation_id="start_candidate_simulation",tag="Portfolio",request_body=CandidateSimulationRequestV1,params(("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<contracts::runs::RunSnapshotV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn simulate(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<CandidateSimulationRequestV1>, JsonRejection>,
) -> Result<
    (
        StatusCode,
        Json<CommandResult<contracts::runs::RunSnapshotV1>>,
    ),
    ApiError,
> {
    run_portfolio(
        state,
        actor,
        headers,
        contracts::control::OperatorCommand::PortfolioSimulate(json(body)?),
    )
    .await
}

#[utoipa::path(post,path="/api/v2/portfolio-studies",operation_id="start_portfolio_study",tag="Portfolio",request_body=PortfolioStudyRequestV1,params(("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<contracts::runs::RunSnapshotV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn study(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<PortfolioStudyRequestV1>, JsonRejection>,
) -> Result<
    (
        StatusCode,
        Json<CommandResult<contracts::runs::RunSnapshotV1>>,
    ),
    ApiError,
> {
    run_portfolio(
        state,
        actor,
        headers,
        contracts::control::OperatorCommand::PortfolioStudy(json(body)?),
    )
    .await
}

async fn run_portfolio(
    state: AppState,
    actor: store::authority::Actor,
    headers: HeaderMap,
    command: contracts::control::OperatorCommand,
) -> Result<
    (
        StatusCode,
        Json<CommandResult<contracts::runs::RunSnapshotV1>>,
    ),
    ApiError,
> {
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
        let read = move |id, size| {
            let objects = reading.clone();
            async move {
                tokio::task::spawn_blocking(move || objects.read(id, size))
                    .await
                    .map_err(|_| StoreError::Integrity)?
                    .map_err(|_| StoreError::Integrity)
            }
        };
        let publish = |object: store::lifecycle::native::NativeObjectPublication| {
            allocated.push(object.id);
            let publishing = publishing.clone();
            async move {
                tokio::task::spawn_blocking(move || publishing.put(object.id, &object.bytes))
                    .await
                    .map_err(|_| StoreError::Integrity)?
                    .map_err(crate::error::artifact_storage)
            }
        };
        let result = match command {
            contracts::control::OperatorCommand::PortfolioBuild(request) => match *request {
                PortfolioBuildEnvelopeV2::Forecast(request) => {
                    store
                        .start_portfolio_build(&actor, &key, &request, read, publish)
                        .await
                }
                PortfolioBuildEnvelopeV2::Strategy(request) => {
                    store
                        .start_strategy_portfolio_build(&actor, &key, &request, read, publish)
                        .await
                }
            },
            contracts::control::OperatorCommand::PortfolioSimulate(request) => {
                store
                    .start_candidate_simulation(&actor, &key, &request, read, publish)
                    .await
            }
            contracts::control::OperatorCommand::PortfolioStudy(request) => {
                store
                    .start_portfolio_study(&actor, &key, &request, read, publish)
                    .await
            }
            _ => Err(StoreError::Invalid("portfolio_operation")),
        };
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
                tracing::warn!(artifact_id=%id, "Portfolio parameter cleanup deferred");
            }
        }
        result
    })
    .await?;
    Ok((StatusCode::ACCEPTED, Json(result)))
}

#[utoipa::path(post,path="/api/v2/portfolio-mandates",operation_id="create_mandate",tag="Portfolio mandates",params(("Idempotency-Key"=String,Header)),request_body=MandateCreateEnvelopeV2,responses((status=201,body=CommandResult<MandateViewEnvelopeV2>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn create(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<MandateCreateEnvelopeV2>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<MandateViewEnvelopeV2>>), ApiError> {
    let key = idempotency_key(&headers)?;
    let result = match json(body)? {
        MandateCreateEnvelopeV2::Forecast(request) => {
            let result = state.store.create_mandate(&actor, key, &request).await?;
            CommandResult {
                schema_version: result.schema_version,
                replayed: result.replayed,
                resource: MandateViewEnvelopeV2::Forecast(Box::new(result.resource)),
            }
        }
        MandateCreateEnvelopeV2::Strategy(request) => {
            let result = state
                .store
                .create_strategy_mandate(&actor, key, &request)
                .await?;
            CommandResult {
                schema_version: result.schema_version,
                replayed: result.replayed,
                resource: MandateViewEnvelopeV2::Strategy(Box::new(result.resource)),
            }
        }
    };
    Ok((StatusCode::CREATED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/projects/{id}/portfolio-mandates",operation_id="list_mandates",tag="Portfolio mandates",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<MandateViewEnvelopeV2>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    query: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<MandateViewEnvelopeV2>>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state.store.mandates_envelope(&actor, id, &query).await?,
    ))
}

#[utoipa::path(get,path="/api/v2/portfolio-mandates/{id}",operation_id="get_mandate",tag="Portfolio mandates",params(("id"=Id,Path)),responses((status=200,body=MandateViewEnvelopeV2),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<MandateViewEnvelopeV2>, ApiError> {
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.mandate_envelope(&actor, id).await?))
}
