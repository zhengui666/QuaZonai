//! The sole browser/machine authentication selector. Invalid machine credentials
//! never fall back to ambient browser authority; business transactions recheck.
use crate::{auth, error::ApiError, AppState};
use axum::{
    extract::FromRequestParts,
    http::{header, request::Parts, HeaderMap},
};
use contracts::Id;
use integrations::authentication::{machine_token, verify_capability};
use store::authority::Actor;
use tower_sessions::Session;

pub struct Authority(pub Actor);

pub fn one_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, ApiError> {
    let mut values = headers.get_all(name).iter();
    let value = values
        .next()
        .map(|v| v.to_str().map_err(|_| ApiError::validation()))
        .transpose()?;
    if values.next().is_some() {
        return Err(ApiError::validation());
    }
    Ok(value)
}
pub fn idempotency_key(headers: &HeaderMap) -> Result<&str, ApiError> {
    let value = one_header(headers, "idempotency-key")?.ok_or_else(ApiError::validation)?;
    if value.is_empty()
        || value.len() > 200
        || value.trim() != value
        || value.chars().any(char::is_control)
    {
        return Err(ApiError::validation());
    }
    Ok(value)
}
impl FromRequestParts<AppState> for Authority {
    type Rejection = ApiError;
    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        if let Some(authorization) = one_header(&parts.headers, "authorization")? {
            if parts.headers.contains_key(header::COOKIE) {
                return Err(ApiError::authentication());
            }
            let (scheme, value) = authorization
                .split_once(' ')
                .ok_or_else(ApiError::authentication)?;
            if !scheme.eq_ignore_ascii_case("Bearer") {
                return Err(ApiError::authentication());
            }
            let token = machine_token(value).map_err(|_| ApiError::authentication())?;
            let secret = token.capability.to_owned();
            let challenge = state.store.machine_challenge(token.public_token_id).await?;
            let grant = one_header(&parts.headers, "x-operator-grant")?
                .map(|s| Id::try_from(s.to_owned()).map_err(|_| ApiError::validation()))
                .transpose()?;
            let vault = state.vault.clone();
            let reference = challenge.verifier_ref;
            let attempt = state
                .store
                .reserve_machine_auth_attempt(challenge.credential_id)
                .await?;
            let matches = auth::crypto_with_slots(state.machine_crypto_slots.clone(), move || {
                let bytes = vault
                    .read(reference, "MACHINE_VERIFIER")
                    .map_err(|_| ApiError::internal())?;
                let verifier = std::str::from_utf8(&bytes).map_err(|_| ApiError::internal())?;
                Ok(verify_capability(&secret, verifier))
            })
            .await?;
            if !matches {
                return Err(ApiError::authentication());
            }
            state.store.machine_auth_succeeded(attempt).await?;
            Ok(Self(challenge.verified_actor(grant)))
        } else {
            if parts.headers.contains_key("x-operator-grant") {
                return Err(ApiError::authentication());
            }
            let session = Session::from_request_parts(parts, state)
                .await
                .map_err(|_| ApiError::internal())?;
            let login = auth::authority(state, &session).await?;
            Ok(Self(Actor::Browser { login_id: login.id }))
        }
    }
}
