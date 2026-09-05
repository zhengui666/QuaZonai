use crate::{
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        Path, Query, State,
    },
    http::StatusCode,
    Json,
};
use contracts::{auth::*, Id, SchemaV1};
use integrations::authentication::{
    accepted_step, new_totp_secret, provisioning_uri, random_capability, verify_capability,
};
use store::auth::{AuthOperation, LoginAuthority};
use tower_sessions::{Expiry, Session};

const LOGIN: &str = "operator_login";
const ENROLLMENT_BINDING: &str = "enrollment_binding";

async fn crypto<T: Send + 'static>(
    state: &AppState,
    task: impl FnOnce() -> Result<T, ApiError> + Send + 'static,
) -> Result<T, ApiError> {
    let permit = state
        .crypto_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
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
fn json<T>(body: Result<Json<T>, JsonRejection>) -> Result<T, ApiError> {
    body.map(|Json(value)| value)
        .map_err(|_| ApiError::validation())
}
async fn login_id(session: &Session) -> Result<Id, ApiError> {
    session
        .get(LOGIN)
        .await
        .map_err(|_| ApiError::internal())?
        .ok_or_else(ApiError::authentication)
}
pub async fn authority(state: &AppState, session: &Session) -> Result<LoginAuthority, ApiError> {
    Ok(state
        .store
        .browser_authority(login_id(session).await?)
        .await?)
}

async fn install_session(
    state: &AppState,
    session: &Session,
    authority: &LoginAuthority,
) -> Result<(), ApiError> {
    if let Some(old) = session
        .get::<Id>(LOGIN)
        .await
        .map_err(|_| ApiError::internal())?
    {
        state.store.logout_browser(old).await?;
    }
    session.clear().await;
    session.cycle_id().await.map_err(|_| ApiError::internal())?;
    session
        .insert(LOGIN, authority.id)
        .await
        .map_err(|_| ApiError::internal())?;
    let expiry = time::OffsetDateTime::from_unix_timestamp(authority.expires_at.timestamp())
        .map_err(|_| ApiError::internal())?;
    session.set_expiry(Some(Expiry::AtDateTime(expiry)));
    session.save().await.map_err(|_| ApiError::internal())?;
    Ok(())
}

#[utoipa::path(get,path="/api/v2/bootstrap/status",tag="Authentication",responses((status=200,body=BootstrapStatus),(status=503,body=Problem)))]
pub async fn bootstrap_status(
    State(state): State<AppState>,
) -> Result<Json<BootstrapStatus>, ApiError> {
    let snapshot = state.store.authentication_snapshot().await?;
    Ok(Json(BootstrapStatus {
        schema_version: SchemaV1,
        initialized: snapshot.initialized,
        setup_allowed: !snapshot.initialized,
    }))
}

#[utoipa::path(post,path="/api/v2/bootstrap/start",tag="Authentication",request_body=BootstrapStart,responses((status=201,body=BootstrapEnrollment),(status=401,body=Problem),(status=409,body=Problem),(status=429,body=Problem)))]
pub async fn bootstrap_start(
    State(state): State<AppState>,
    session: Session,
    body: Result<Json<BootstrapStart>, JsonRejection>,
) -> Result<(StatusCode, Json<BootstrapEnrollment>), ApiError> {
    let request = json(body)?;
    state
        .store
        .reserve_auth_attempt(AuthOperation::Bootstrap)
        .await?;
    let challenge = state
        .store
        .bootstrap_challenge(request.capability_id)
        .await?;
    let verifier = challenge.verifier.clone();
    let vault = state.vault.clone();
    let (secret_ref, uri) = crypto(&state, move || {
        if !verify_capability(&request.capability, &verifier) {
            return Err(ApiError::new(
                StatusCode::UNAUTHORIZED,
                "AUTHENTICATION_FAILED",
                "初始化凭据无效。",
            ));
        }
        let secret = new_totp_secret().map_err(|_| ApiError::internal())?;
        let uri = provisioning_uri(&secret).map_err(|_| ApiError::internal())?;
        let reference = vault
            .put("TOTP", &secret)
            .map_err(|_| ApiError::internal())?;
        Ok((reference, uri))
    })
    .await?;
    let binding = random_capability();
    // Persist the native anonymous session before consuming the capability, so
    // confirmation is inseparable from this browser's opaque private cookie.
    session
        .insert(ENROLLMENT_BINDING, &binding)
        .await
        .map_err(|_| ApiError::internal())?;
    session.save().await.map_err(|_| ApiError::internal())?;
    let enrolled = state
        .store
        .start_enrollment(challenge.id, &challenge.verifier, secret_ref, &binding)
        .await?;
    Ok((
        StatusCode::CREATED,
        Json(BootstrapEnrollment {
            schema_version: SchemaV1,
            enrollment_id: enrolled.id,
            expires_at: enrolled.expires_at,
            provisioning_uri: uri,
        }),
    ))
}

#[utoipa::path(post,path="/api/v2/bootstrap/confirm",tag="Authentication",request_body=BootstrapConfirm,responses((status=200,body=BrowserSession),(status=401,body=Problem),(status=409,body=Problem),(status=429,body=Problem)))]
pub async fn bootstrap_confirm(
    State(state): State<AppState>,
    session: Session,
    body: Result<Json<BootstrapConfirm>, JsonRejection>,
) -> Result<Json<BrowserSession>, ApiError> {
    let request = json(body)?;
    state
        .store
        .reserve_auth_attempt(AuthOperation::Bootstrap)
        .await?;
    let binding: String = session
        .get(ENROLLMENT_BINDING)
        .await
        .map_err(|_| ApiError::internal())?
        .ok_or_else(ApiError::authentication)?;
    let enrollment = state
        .store
        .enrollment_challenge(request.enrollment_id, &binding)
        .await?;
    let vault = state.vault.clone();
    let secret_ref = enrollment.secret_ref;
    let timestamp = enrollment.database_now.timestamp();
    let code = request.code;
    let step = crypto(&state, move || {
        let secret = vault
            .read(secret_ref, "TOTP")
            .map_err(|_| ApiError::internal())?;
        accepted_step(&secret, &code, timestamp)
            .map_err(|_| ApiError::internal())?
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "AUTHENTICATION_FAILED",
                    "验证码无效或过期。",
                )
            })
    })
    .await?;
    let authority = state
        .store
        .confirm_enrollment(
            enrollment.id,
            &binding,
            secret_ref,
            step,
            request.trust_device,
            request.device_label.as_deref(),
        )
        .await?;
    install_session(&state, &session, &authority).await?;
    Ok(Json(authority.public()))
}

#[utoipa::path(post,path="/api/v2/auth/login",tag="Authentication",request_body=LoginRequest,responses((status=200,body=BrowserSession),(status=401,body=Problem),(status=409,body=Problem),(status=429,body=Problem)))]
pub async fn login(
    State(state): State<AppState>,
    session: Session,
    body: Result<Json<LoginRequest>, JsonRejection>,
) -> Result<Json<BrowserSession>, ApiError> {
    let request = json(body)?;
    state
        .store
        .reserve_auth_attempt(AuthOperation::Login)
        .await?;
    let snapshot = state.store.authentication_snapshot().await?;
    let secret_ref = snapshot.secret_ref.ok_or_else(ApiError::authentication)?;
    let vault = state.vault.clone();
    let code = request.code;
    let timestamp = snapshot.database_now.timestamp();
    let step = crypto(&state, move || {
        let secret = vault
            .read(secret_ref, "TOTP")
            .map_err(|_| ApiError::internal())?;
        accepted_step(&secret, &code, timestamp)
            .map_err(|_| ApiError::internal())?
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "AUTHENTICATION_FAILED",
                    "验证码无效或过期。",
                )
            })
    })
    .await?;
    let authority = state
        .store
        .login_with_verified_step(
            &snapshot,
            step,
            request.trust_device,
            request.device_label.as_deref(),
        )
        .await?;
    install_session(&state, &session, &authority).await?;
    Ok(Json(authority.public()))
}

#[utoipa::path(post,path="/api/v2/auth/logout",tag="Authentication",responses((status=204),(status=503,body=Problem)))]
pub async fn logout(
    State(state): State<AppState>,
    session: Session,
) -> Result<StatusCode, ApiError> {
    if let Some(id) = session
        .get::<Id>(LOGIN)
        .await
        .map_err(|_| ApiError::internal())?
    {
        state.store.logout_browser(id).await?;
    }
    session.flush().await.map_err(|_| ApiError::internal())?;
    Ok(StatusCode::NO_CONTENT)
}

#[utoipa::path(get,path="/api/v2/auth/session",tag="Authentication",responses((status=200,body=BrowserSession),(status=401,body=Problem)))]
pub async fn session_status(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<BrowserSession>, ApiError> {
    Ok(Json(authority(&state, &session).await?.public()))
}

#[utoipa::path(post,path="/api/v2/auth/verify",tag="Authentication",request_body=VerifyRequest,responses((status=200,body=BrowserSession),(status=401,body=Problem),(status=409,body=Problem),(status=429,body=Problem)))]
pub async fn verify(
    State(state): State<AppState>,
    session: Session,
    body: Result<Json<VerifyRequest>, JsonRejection>,
) -> Result<Json<BrowserSession>, ApiError> {
    let request = json(body)?;
    let login = authority(&state, &session).await?;
    state
        .store
        .reserve_auth_attempt(AuthOperation::Reauth)
        .await?;
    let snapshot = state.store.authentication_snapshot().await?;
    let reference = snapshot.secret_ref.ok_or_else(ApiError::authentication)?;
    let vault = state.vault.clone();
    let timestamp = snapshot.database_now.timestamp();
    let step = crypto(&state, move || {
        let secret = vault
            .read(reference, "TOTP")
            .map_err(|_| ApiError::internal())?;
        accepted_step(&secret, &request.code, timestamp)
            .map_err(|_| ApiError::internal())?
            .ok_or_else(|| {
                ApiError::new(
                    StatusCode::UNAUTHORIZED,
                    "AUTHENTICATION_FAILED",
                    "验证码无效或过期。",
                )
            })
    })
    .await?;
    Ok(Json(
        state
            .store
            .reauthenticate(login.id, &snapshot, step)
            .await?
            .public(),
    ))
}

#[utoipa::path(get,path="/api/v2/auth/devices",tag="Authentication",params(("cursor"=Option<Id>,Query,description="Opaque previous-page cursor")),responses((status=200,body=DeviceList),(status=401,body=Problem)))]
pub async fn devices(
    State(state): State<AppState>,
    session: Session,
    query: Result<Query<DeviceCursor>, QueryRejection>,
) -> Result<Json<DeviceList>, ApiError> {
    let Query(cursor) = query.map_err(|_| ApiError::validation())?;
    Ok(Json(
        state
            .store
            .trusted_devices(login_id(&session).await?, cursor.cursor)
            .await?,
    ))
}

#[utoipa::path(delete,path="/api/v2/auth/devices/{id}",tag="Authentication",params(("id"=Id,Path,description="Trusted device identity")),responses((status=204),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem)))]
pub async fn revoke_device(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<Id>,
) -> Result<StatusCode, ApiError> {
    state
        .store
        .revoke_trusted_device(login_id(&session).await?, id)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}
