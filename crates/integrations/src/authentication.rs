//! Native TOTP/opaque-capability primitives. Database state owns replay, expiry
//! and authority; a successful cryptographic check alone grants no permission.
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use chacha20poly1305::aead::{rand_core::RngCore, OsRng};
use thiserror::Error;
use totp_rs::{Algorithm, Secret, TOTP};

#[derive(Debug, Error)]
pub enum AuthenticationError {
    #[error("invalid authentication material")]
    Invalid,
    #[error("authentication primitive failed")]
    Primitive,
}

pub fn random_capability() -> String {
    let mut bytes = [0_u8; 32];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Mature salted Argon2id verifier; never use this value as a business identity.
pub fn capability_verifier(secret: &str) -> Result<String, AuthenticationError> {
    if secret.len() != 43
        || URL_SAFE_NO_PAD
            .decode(secret)
            .map_or(true, |v| v.len() != 32)
    {
        return Err(AuthenticationError::Invalid);
    }
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(secret.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(|_| AuthenticationError::Primitive)
}

pub fn verify_capability(secret: &str, verifier: &str) -> bool {
    // Bound attacker-controlled parsing/hashing work before the upstream call.
    if secret.len() != 43 || verifier.len() > 256 {
        return false;
    }
    PasswordHash::new(verifier).is_ok_and(|hash| {
        Argon2::default()
            .verify_password(secret.as_bytes(), &hash)
            .is_ok()
    })
}

pub fn new_totp_secret() -> Result<Vec<u8>, AuthenticationError> {
    Secret::generate_secret()
        .to_bytes()
        .map_err(|_| AuthenticationError::Primitive)
}

fn native_totp(secret: &[u8]) -> Result<TOTP, AuthenticationError> {
    // skew=0: identify the exact accepted step using the upstream check; the
    // caller's durable state, not this object, applies the ±1 policy and replay.
    TOTP::new(
        Algorithm::SHA1,
        6,
        0,
        30,
        secret.to_vec(),
        Some("QuaZonai".into()),
        "operator".into(),
    )
    .map_err(|_| AuthenticationError::Invalid)
}

pub fn provisioning_uri(secret: &[u8]) -> Result<String, AuthenticationError> {
    Ok(native_totp(secret)?.get_url())
}

pub fn accepted_step(
    secret: &[u8],
    code: &str,
    unix_seconds: i64,
) -> Result<Option<i64>, AuthenticationError> {
    if code.len() != 6 || !code.bytes().all(|b| b.is_ascii_digit()) || unix_seconds < 30 {
        return Ok(None);
    }
    let totp = native_totp(secret)?;
    let step = unix_seconds / 30;
    // Prefer the newest matching step in the rare event of a collision so an
    // accepted code cannot be replayed at a second step inside the same window.
    for candidate in (step - 1..=step + 1).rev() {
        if totp.check(code, candidate as u64 * 30) {
            return Ok(Some(candidate));
        }
    }
    Ok(None)
}
