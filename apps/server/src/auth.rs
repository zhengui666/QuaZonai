//! Native capability work and automatic local browser request correlation.
use crate::{
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{rejection::JsonRejection, State},
    http::StatusCode,
    Json,
};
use contracts::{auth::BrowserSession, Id};
use store::{auth::LoginAuthority, StoreError};
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
    let authority = state.store.local_browser().await?;
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
    Ok(authority)
}

#[utoipa::path(get,path="/api/v2/auth/session",tag="Local session",responses((status=200,body=BrowserSession),(status=503,body=Problem)))]
pub async fn session_status(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<BrowserSession>, ApiError> {
    Ok(Json(authority(&state, &session).await?.public()))
}
