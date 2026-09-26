//! Password admission, native private cookies and persistent CLI devices.
use crate::{
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{rejection::JsonRejection, Path, State},
    http::StatusCode,
    Json,
};
use contracts::{auth::*, Id, SchemaV1};
use integrations::authentication::{
    capability_verifier, format_cli_token, password_verifier, random_capability, verify_password,
};
use store::{
    auth::{AuthSnapshot, LoginAuthority},
    StoreError,
};
use tower_sessions::{Expiry, Session};

const LOGIN: &str = "operator_login";

pub(crate) async fn crypto<T: Send + 'static>(
    state: &AppState,
    task: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    crypto_with_slots(state.crypto_slots.clone(), task).await
}
pub(crate) async fn crypto_with_slots<T: Send + 'static>(
    slots: std::sync::Arc<tokio::sync::Semaphore>,
    task: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    let permit = slots.try_acquire_owned().map_err(|_| {
        ApiError::new(
            StatusCode::TOO_MANY_REQUESTS,
            "CRYPTO_BUSY",
            "验证服务繁忙，请稍后重新尝试。",
        )
    })?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        task()
    })
    .await
    .map_err(|_| ApiError::internal())?
}
pub(crate) fn json<T>(body: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    body.map(|Json(value)| value)
        .map_err(|_| ApiError::validation())
}
pub async fn authority(state: &AppState, session: &Session) -> Result<LoginAuthority, ApiError> {
    if let Some(id) = session
        .get::<Id>(LOGIN)
        .await
        .map_err(|_| ApiError::internal())?
    {
        match state.store.browser_authority(id).await {
            Ok(authority) => return Ok(authority),
            Err(StoreError::AuthenticationRequired) => {}
            Err(error) => return Err(error.into()),
        }
    }
    Err(StoreError::AuthenticationRequired.into())
}

async fn save_session(
    session: &Session,
    authority: &LoginAuthority,
    remember: bool,
) -> Result<(), ApiError> {
    session.clear().await;
    session.cycle_id().await.map_err(|_| ApiError::internal())?;
    session
        .insert(LOGIN, authority.id)
        .await
        .map_err(|_| ApiError::internal())?;
    let expiry = time::OffsetDateTime::from_unix_timestamp(authority.expires_at.timestamp())
        .map_err(|_| ApiError::internal())?;
    session.set_expiry(Some(if remember {
        Expiry::AtDateTime(expiry)
    } else {
        Expiry::OnSessionEnd
    }));
    session.save().await.map_err(|_| ApiError::internal())?;
    Ok(())
}

#[utoipa::path(get,path="/api/v2/auth/session",tag="Authentication",responses((status=200,body=BrowserSession),(status=401,body=Problem),(status=503,body=Problem)))]
pub async fn session_status(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<BrowserSession>, ApiError> {
    Ok(Json(authority(&state, &session).await?.public()))
}

#[utoipa::path(get,path="/api/v2/auth/status",tag="Authentication",responses((status=200,body=AuthStatus),(status=503,body=Problem)))]
pub async fn status(State(state): State<AppState>) -> Result<Json<AuthStatus>, ApiError> {
    let snapshot = state.store.authentication_snapshot().await?;
    Ok(Json(AuthStatus {
        schema_version: SchemaV1,
        setup_required: snapshot.password_verifier.is_none(),
    }))
}

async fn password_snapshot(state: &AppState, password: String) -> Result<AuthSnapshot, ApiError> {
    let snapshot = state.store.authentication_snapshot().await?;
    let verifier = snapshot
        .password_verifier
        .clone()
        .ok_or(StoreError::InvalidCredentials)?;
    let valid = crypto(state, move || Ok(verify_password(&password, &verifier))).await?;
    if !valid {
        return Err(StoreError::InvalidCredentials.into());
    }
    Ok(snapshot)
}

#[utoipa::path(post,path="/api/v2/auth/setup",tag="Authentication",request_body=PasswordLogin,responses((status=200,body=BrowserSession),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn setup(
    State(state): State<AppState>,
    session: Session,
    body: Result<Json<PasswordLogin>, JsonRejection>,
) -> Result<Json<BrowserSession>, ApiError> {
    let request = json(body)?;
    let verifier = crypto(&state, move || {
        password_verifier(&request.password).map_err(|_| ApiError::validation())
    })
    .await?;
    let login = state
        .store
        .setup_password(&verifier, request.remember_device)
        .await?;
    save_session(&session, &login, request.remember_device).await?;
    Ok(Json(login.public()))
}

#[utoipa::path(post,path="/api/v2/auth/login",tag="Authentication",request_body=PasswordLogin,responses((status=200,body=BrowserSession),(status=401,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn login(
    State(state): State<AppState>,
    session: Session,
    body: Result<Json<PasswordLogin>, JsonRejection>,
) -> Result<Json<BrowserSession>, ApiError> {
    let request = json(body)?;
    let snapshot = password_snapshot(&state, request.password).await?;
    let login = state
        .store
        .password_login(&snapshot, request.remember_device)
        .await?;
    save_session(&session, &login, request.remember_device).await?;
    Ok(Json(login.public()))
}

#[utoipa::path(post,path="/api/v2/auth/logout",tag="Authentication",responses((status=204),(status=401,body=Problem),(status=503,body=Problem)))]
pub async fn logout(
    State(state): State<AppState>,
    session: Session,
) -> Result<StatusCode, ApiError> {
    let login = authority(&state, &session).await?;
    state.store.logout_browser(login.id).await?;
    session.flush().await.map_err(|_| ApiError::internal())?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post,path="/api/v2/auth/password",tag="Authentication",request_body=PasswordChange,responses((status=204),(status=401,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn change_password(
    State(state): State<AppState>,
    session: Session,
    body: Result<Json<PasswordChange>, JsonRejection>,
) -> Result<StatusCode, ApiError> {
    let login = authority(&state, &session).await?;
    let request = json(body)?;
    let snapshot = password_snapshot(&state, request.current_password).await?;
    let verifier = crypto(&state, move || {
        password_verifier(&request.new_password).map_err(|_| ApiError::validation())
    })
    .await?;
    state
        .store
        .change_password(login.id, &snapshot, &verifier)
        .await?;
    session.flush().await.map_err(|_| ApiError::internal())?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(post,path="/api/v2/auth/cli/login",tag="Authentication",request_body=CliLogin,responses((status=201,body=CliLoginResult),(status=401,body=Problem),(status=422,body=Problem),(status=429,body=Problem),(status=503,body=Problem)))]
pub async fn cli_login(
    State(state): State<AppState>,
    body: Result<Json<CliLogin>, JsonRejection>,
) -> Result<(StatusCode, Json<CliLoginResult>), ApiError> {
    let request = json(body)?;
    let snapshot = password_snapshot(&state, request.password).await?;
    let (secret, verifier) = crypto(&state, || {
        let secret = random_capability();
        let verifier = capability_verifier(&secret).map_err(|_| ApiError::internal())?;
        Ok((secret, verifier))
    })
    .await?;
    let device = state
        .store
        .register_cli_device(&snapshot, &request.name, &verifier)
        .await?;
    let token = format_cli_token(device.id, &secret).map_err(|_| ApiError::internal())?;
    Ok((StatusCode::CREATED, Json(CliLoginResult { device, token })))
}

#[utoipa::path(get,path="/api/v2/auth/cli/session",tag="Authentication",responses((status=200,body=CliDevice),(status=401,body=Problem),(status=403,body=Problem),(status=503,body=Problem)))]
pub async fn cli_session(
    State(state): State<AppState>,
    crate::access::Authority(actor): crate::access::Authority,
) -> Result<Json<CliDevice>, ApiError> {
    Ok(Json(state.store.cli_device_session(&actor).await?))
}

#[utoipa::path(get,path="/api/v2/auth/cli/devices",tag="Authentication",responses((status=200,body=Vec<CliDevice>),(status=401,body=Problem),(status=503,body=Problem)))]
pub async fn cli_devices(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<Vec<CliDevice>>, ApiError> {
    let login = authority(&state, &session).await?;
    Ok(Json(state.store.cli_devices(login.id).await?))
}

#[utoipa::path(delete,path="/api/v2/auth/cli/devices/{id}",tag="Authentication",params(("id"=Id,Path)),responses((status=204),(status=401,body=Problem),(status=404,body=Problem),(status=422,body=Problem),(status=503,body=Problem)))]
pub async fn revoke_cli_device(
    State(state): State<AppState>,
    session: Session,
    id: Result<Path<Id>, axum::extract::rejection::PathRejection>,
) -> Result<StatusCode, ApiError> {
    let login = authority(&state, &session).await?;
    let Path(id) = id.map_err(|_| ApiError::validation())?;
    state.store.revoke_cli_device(login.id, id).await?;
    Ok(StatusCode::NO_CONTENT)
}
