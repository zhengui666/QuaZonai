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

/// A bounded routing identifier plus an opaque secret. This is not a JWT and
/// carries no roles, scopes, expiry or client-selected authority.
pub struct MachineToken<'a> {
    pub public_token_id: contracts::Id,
    pub capability: &'a str,
}

pub fn machine_token(value: &str) -> Result<MachineToken<'_>, AuthenticationError> {
    if value.len() != 84 {
        return Err(AuthenticationError::Invalid);
    }
    let mut parts = value.split('.');
    if parts.next() != Some("qz2") {
        return Err(AuthenticationError::Invalid);
    }
    let public = parts.next().ok_or(AuthenticationError::Invalid)?;
    let capability = parts.next().ok_or(AuthenticationError::Invalid)?;
    if parts.next().is_some()
        || public.len() != 36
        || capability.len() != 43
        || URL_SAFE_NO_PAD
            .decode(capability)
            .map_or(true, |bytes| bytes.len() != 32)
    {
        return Err(AuthenticationError::Invalid);
    }
    Ok(MachineToken {
        public_token_id: contracts::Id::try_from(public.to_owned())
            .map_err(|_| AuthenticationError::Invalid)?,
        capability,
    })
}

pub fn format_machine_token(
    public: contracts::Id,
    secret: &str,
) -> Result<String, AuthenticationError> {
    let value = format!("qz2.{public}.{secret}");
    machine_token(&value)?;
    Ok(value)
}

#[cfg(test)]
mod machine_token_tests {
    use super::*;
    #[test]
    fn machine_token_has_one_bounded_native_id_and_opaque_secret() {
        let public = contracts::Id::new();
        let secret = random_capability();
        let value = format_machine_token(public, &secret).unwrap();
        let parsed = machine_token(&value).unwrap();
        assert_eq!(parsed.public_token_id, public);
        assert_eq!(parsed.capability, secret);
        for bad in [
            format!(" {value}"),
            format!("{value}.extra"),
            value.replace("qz2.", "qz3."),
            format!("qz2.550e8400-e29b-41d4-a716-446655440000.{secret}"),
            format!("qz2.{public}.{}", "!".repeat(43)),
            format!("qz2.{public}.{}", "a".repeat(42)),
        ] {
            assert!(machine_token(&bad).is_err());
        }
    }
}
