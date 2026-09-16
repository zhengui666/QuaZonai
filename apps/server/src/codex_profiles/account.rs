//! Human-authorized native account actions. Reads never start a child or expose
//! a device code; accepted writes retain their original command receipt.
use super::*;
use store::codex_profiles::account::CodexAccountPreparation;

async fn begin(
    state: &AppState,
    actor: store::authority::Actor,
    key: String,
    request: CodexAccountRequestV1,
    action: CodexAccountActionV1,
) -> Result<(StatusCode, Json<CodexAccountStartV1>), ApiError> {
    let store = state.store.clone();
    let deployment = state.codex_deployment.clone();
    let response = crate::settings::command(state, async move {
        let acceptance = match store
            .prepare_codex_account(&actor, &key, &request, action)
            .await?
        {
            CodexAccountPreparation::Replay(result) => result,
            CodexAccountPreparation::Start(ticket) => {
                let acceptance = ticket.acceptance.clone();
                deployment.start_account(store.clone(), ticket).await;
                acceptance
            }
        };
        let current = store
            .read_codex_account_operation(&actor, acceptance.resource.id)
            .await?;
        let device_code = if current.state == CodexAccountOperationStateV1::Waiting
            && current.operation.deadline_at > chrono::Utc::now()
        {
            deployment.device_code(acceptance.resource.id).await
        } else {
            None
        };
        Ok(CodexAccountStartV1 {
            schema_version: contracts::SchemaV1,
            acceptance,
            current,
            device_code,
        })
    })
    .await?;
    Ok((StatusCode::ACCEPTED, Json(response)))
}

#[utoipa::path(post,path="/api/v2/codex/login/start",operation_id="startCodexLogin",tag="Codex settings",request_body=CodexAccountRequestV1,params(("Idempotency-Key"=String,Header)),responses((status=202,body=CodexAccountStartV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn login_start(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<CodexAccountRequestV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CodexAccountStartV1>), ApiError> {
    begin(
        &state,
        actor,
        idempotency_key(&headers)?.to_owned(),
        json(body)?,
        CodexAccountActionV1::Login,
    )
    .await
}

#[utoipa::path(post,path="/api/v2/codex/logout",operation_id="logoutCodexAccount",tag="Codex settings",request_body=CodexAccountRequestV1,params(("Idempotency-Key"=String,Header)),responses((status=202,body=CodexAccountStartV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn logout(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<CodexAccountRequestV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CodexAccountStartV1>), ApiError> {
    begin(
        &state,
        actor,
        idempotency_key(&headers)?.to_owned(),
        json(body)?,
        CodexAccountActionV1::Logout,
    )
    .await
}

#[utoipa::path(post,path="/api/v2/codex/login/cancel",operation_id="cancelCodexLogin",tag="Codex settings",request_body=CodexLoginCancelV1,params(("Idempotency-Key"=String,Header)),responses((status=202,body=CommandResult<CodexAccountOperationV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn login_cancel(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    body: Result<Json<CodexLoginCancelV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<CodexAccountOperationV1>>), ApiError> {
    let request = json(body)?;
    let key = idempotency_key(&headers)?.to_owned();
    let store = state.store.clone();
    let deployment = state.codex_deployment.clone();
    let result = crate::settings::command(&state, async move {
        let result = store.cancel_codex_login(&actor, &key, &request).await?;
        if result.resource.state == CodexAccountOperationStateV1::CancelRequested
            && !deployment
                .owns_account_operation(request.operation_id)
                .await
        {
            store
                .orphaned_codex_login(&result.resource.operation)
                .await?;
        }
        Ok(result)
    })
    .await?;
    // The cancel receipt is acceptance, never optimistic native cancellation.
    Ok((StatusCode::ACCEPTED, Json(result)))
}

#[utoipa::path(get,path="/api/v2/codex/login/{id}",operation_id="getCodexAccountOperation",tag="Codex settings",params(("id"=Id,Path)),responses((status=200,body=CodexAccountOperationV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem)))]
pub async fn login_operation(
    State(state): State<AppState>,
    Authority(actor): Authority,
    id: Result<Path<Id>, PathRejection>,
) -> Result<Json<CodexAccountOperationV1>, ApiError> {
    Ok(Json(
        state
            .store
            .read_codex_account_operation(&actor, path(id)?)
            .await?,
    ))
}

#[utoipa::path(get,path="/api/v2/codex/login",operation_id="getLatestCodexAccountOperation",tag="Codex settings",params(("profile_id"=Id,Query)),responses((status=200,body=Option<CodexAccountOperationV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=429,body=Problem)))]
pub async fn latest_operation(
    State(state): State<AppState>,
    Authority(actor): Authority,
    query: Result<Query<CodexProfileQueryV1>, QueryRejection>,
) -> Result<Json<Option<CodexAccountOperationV1>>, ApiError> {
    let Query(query) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .latest_codex_account_operation(&actor, query.profile_id)
            .await?,
    ))
}
