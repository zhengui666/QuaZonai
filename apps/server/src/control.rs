//! Typed entrypoints only: possession in the extractor, transaction authority
//! and idempotency in Store. Secret generation never enters command receipts.
use crate::{
    access::{idempotency_key, Authority},
    auth::{crypto, json},
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
use contracts::{control::*, Id, SchemaV1};
use integrations::authentication::{
    accepted_step, capability_verifier, format_machine_token, random_capability,
};
use store::{auth::AuthOperation, authority::Actor};

fn path(value: Result<Path<Id>, PathRejection>) -> Result<Id, ApiError> {
    value.map(|Path(v)| v).map_err(|_| ApiError::validation())
}
fn query(value: Result<Query<ListQuery>, QueryRejection>) -> Result<ListQuery, ApiError> {
    value.map(|Query(v)| v).map_err(|_| ApiError::validation())
}

#[utoipa::path(get,path="/api/v2/projects",tag="Projects",params(("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<ProjectView>),(status=401,body=Problem),(status=403,body=Problem),(status=422,body=Problem)))]
pub async fn projects(
    State(state): State<AppState>,
    Authority(actor): Authority,
    q: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<ProjectView>>, ApiError> {
    Ok(Json(state.store.projects(&actor, &query(q)?).await?))
}
#[utoipa::path(get,path="/api/v2/projects/{id}",tag="Projects",params(("id"=Id,Path)),responses((status=200,body=ProjectView),(status=401,body=Problem),(status=404,body=Problem)))]
pub async fn project(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<ProjectView>, ApiError> {
    Ok(Json(state.store.project(&actor, path(id)?).await?))
}
#[utoipa::path(post,path="/api/v2/projects",tag="Projects",request_body=ProjectCreate,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<ProjectView>),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn create_project(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<ProjectCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<ProjectView>>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_project(&actor, idempotency_key(&headers)?, &json(body)?)
                .await?,
        ),
    ))
}
#[utoipa::path(patch,path="/api/v2/projects/{id}",tag="Projects",request_body=ProjectUpdate,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<ProjectView>),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn update_project(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<ProjectUpdate>, JsonRejection>,
) -> Result<Json<CommandResult<ProjectView>>, ApiError> {
    Ok(Json(
        state
            .store
            .update_project(&actor, idempotency_key(&headers)?, path(id)?, &json(body)?)
            .await?,
    ))
}
#[utoipa::path(get,path="/api/v2/machine-principals",tag="Machine identity",params(("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<PrincipalView>),(status=401,body=Problem),(status=403,body=Problem)))]
pub async fn principals(
    State(state): State<AppState>,
    Authority(actor): Authority,
    q: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<PrincipalView>>, ApiError> {
    Ok(Json(state.store.principals(&actor, &query(q)?).await?))
}
#[utoipa::path(post,path="/api/v2/machine-principals",tag="Machine identity",request_body=PrincipalCreate,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<PrincipalView>),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn create_principal(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<PrincipalCreate>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<PrincipalView>>), ApiError> {
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .create_principal(&actor, idempotency_key(&headers)?, &json(body)?)
                .await?,
        ),
    ))
}
#[utoipa::path(patch,path="/api/v2/machine-principals/{id}",tag="Machine identity",request_body=PrincipalUpdate,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<PrincipalView>),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn update_principal(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<PrincipalUpdate>, JsonRejection>,
) -> Result<Json<CommandResult<PrincipalView>>, ApiError> {
    Ok(Json(
        state
            .store
            .update_principal(&actor, idempotency_key(&headers)?, path(id)?, &json(body)?)
            .await?,
    ))
}
#[utoipa::path(get,path="/api/v2/machine-principals/{id}/credentials",tag="Machine identity",params(("id"=Id,Path),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<CredentialView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem)))]
pub async fn credentials(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
    q: Result<Query<ListQuery>, QueryRejection>,
) -> Result<Json<Page<CredentialView>>, ApiError> {
    Ok(Json(
        state
            .store
            .credentials(&actor, path(id)?, &query(q)?)
            .await?,
    ))
}
#[utoipa::path(post,path="/api/v2/machine-principals/{id}/credentials",tag="Machine identity",request_body=CredentialIssue,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=201,body=CredentialCreated),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem)))]
pub async fn issue_credential(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<CredentialIssue>, JsonRejection>,
) -> Result<(StatusCode, Json<CredentialCreated>), ApiError> {
    let principal = path(id)?;
    let request = json(body)?;
    let key = idempotency_key(&headers)?;
    let prepared = match state
        .store
        .prepare_credential_issuance(&actor, key, principal, &request)
        .await?
    {
        store::control::CredentialPreparation::Replay(replay) => {
            return Ok((
                StatusCode::CREATED,
                Json(CredentialCreated {
                    schema_version: SchemaV1,
                    replayed: true,
                    resource: replay.resource,
                    token: None,
                }),
            ));
        }
        store::control::CredentialPreparation::New(prepared) => prepared,
    };
    let public = Id::new();
    let vault = state.vault.clone();
    let (token, verifier_ref) = crypto(&state, move || {
        let secret = random_capability();
        let verifier = capability_verifier(&secret).map_err(|_| ApiError::internal())?;
        let reference = vault
            .put("MACHINE_VERIFIER", verifier.as_bytes())
            .map_err(|_| ApiError::internal())?;
        let token = format_machine_token(public, &secret).map_err(|_| ApiError::internal())?;
        Ok((token, reference))
    })
    .await?;
    let result = match prepared.publish(public, verifier_ref).await {
        Ok(result) => result,
        Err(error) => {
            if crate::secrets::reconcile_verifier(&state.store, state.vault.clone(), verifier_ref)
                .await
                .is_err()
            {
                // Preserve on uncertainty. The local reconciliation command can
                // retry after DB recovery; never delete a potentially committed key.
                tracing::warn!(
                    code = "VERIFIER_RECONCILIATION_REQUIRED",
                    "unpublished verifier requires reconciliation"
                );
            }
            return Err(error.into());
        }
    };
    Ok((
        StatusCode::CREATED,
        Json(CredentialCreated {
            schema_version: SchemaV1,
            token: if result.replayed { None } else { Some(token) },
            replayed: result.replayed,
            resource: result.resource,
        }),
    ))
}
#[utoipa::path(post,path="/api/v2/machine-credentials/{id}/revoke",tag="Machine identity",request_body=CredentialRevoke,params(("id"=Id,Path),("Idempotency-Key"=String,Header)),responses((status=200,body=CommandResult<CredentialView>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem)))]
pub async fn revoke_credential(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    id: Result<Path<Id>, PathRejection>,
    body: Result<Json<CredentialRevoke>, JsonRejection>,
) -> Result<Json<CommandResult<CredentialView>>, ApiError> {
    Ok(Json(
        state
            .store
            .revoke_credential(&actor, idempotency_key(&headers)?, path(id)?, &json(body)?)
            .await?,
    ))
}
#[utoipa::path(get,path="/api/v2/auth/machine",tag="Machine identity",responses((status=200,body=MachineSessionView),(status=401,body=Problem),(status=403,body=Problem)))]
pub async fn machine_session(
    State(state): State<AppState>,
    Authority(actor): Authority,
) -> Result<Json<MachineSessionView>, ApiError> {
    Ok(Json(state.store.machine_session(&actor).await?))
}
#[utoipa::path(post,path="/api/v2/auth/operator-command-grants",tag="Machine identity",request_body=OperatorGrantRequest,params(("Idempotency-Key"=String,Header)),responses((status=201,body=CommandResult<OperatorGrantView>),(status=401,body=Problem),(status=403,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem)))]
pub async fn issue_grant(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<OperatorGrantRequest>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<OperatorGrantView>>), ApiError> {
    let request = json(body)?;
    let key = idempotency_key(&headers)?;
    if !matches!(actor, Actor::Machine { .. }) {
        return Err(store::StoreError::Forbidden.into());
    }
    let session = state.store.machine_session(&actor).await?;
    if session.kind != PrincipalKind::Cli {
        return Err(store::StoreError::Forbidden.into());
    }
    domain::control::command(&request.command).map_err(store::StoreError::Domain)?;
    if let Some(replay) = state
        .store
        .operator_grant_replay(&actor, key, &request.command, request.target_id)
        .await?
    {
        return Ok((StatusCode::CREATED, Json(replay)));
    }
    state
        .store
        .reserve_auth_attempt(AuthOperation::Reauth)
        .await?;
    let snapshot = state.store.authentication_snapshot().await?;
    let reference = snapshot.secret_ref.ok_or_else(ApiError::authentication)?;
    let now = snapshot.database_now.timestamp();
    let vault = state.vault.clone();
    let code = request.code;
    let step = crypto(&state, move || {
        let secret = vault
            .read(reference, "TOTP")
            .map_err(|_| ApiError::internal())?;
        accepted_step(&secret, &code, now)
            .map_err(|_| ApiError::internal())?
            .ok_or_else(ApiError::authentication)
    })
    .await?;
    Ok((
        StatusCode::CREATED,
        Json(
            state
                .store
                .issue_operator_grant(
                    &actor,
                    key,
                    &request.command,
                    request.target_id,
                    &snapshot,
                    step,
                )
                .await?,
        ),
    ))
}
