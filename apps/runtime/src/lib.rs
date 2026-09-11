//! Trusted native Runtime gateway. Scientific qualification remains in the control plane.
#![forbid(unsafe_code)]

pub mod config;
pub mod engine;
pub mod files;
pub mod http;
pub mod journal;
pub mod materialize;
pub mod supervisor;

use chrono::{DateTime, Utc};

pub type Result<T> = std::result::Result<T, Failure>;

#[derive(Debug, thiserror::Error)]
pub enum Failure {
    #[error("RUNTIME_INPUT_INVALID")]
    Invalid(&'static str),
    #[error("RUNTIME_IDEMPOTENCY_CONFLICT")]
    Conflict,
    #[error("RUNTIME_NOT_FOUND")]
    Missing,
    #[error("RUNTIME_RESULT_NOT_READY")]
    NotReady,
    #[error("RUNTIME_OWNER_STALE")]
    StaleOwner,
    #[error("RUNTIME_CAPACITY_EXHAUSTED")]
    Capacity,
    #[error("RUNTIME_BODY_LIMIT")]
    BodyLimit,
    #[error("RUNTIME_BUSY")]
    Busy,
    #[error("RUNTIME_STORAGE_UNAVAILABLE")]
    Database(#[from] sqlx::Error),
    #[error("RUNTIME_STORAGE_UNAVAILABLE")]
    Io(#[from] std::io::Error),
    #[error("RUNTIME_INTEGRITY_ERROR")]
    Integrity,
    #[error("RUNTIME_ENGINE_UNAVAILABLE")]
    Engine,
    #[error("RUNTIME_AUTHENTICATION_REQUIRED")]
    Authentication,
}

impl From<serde_json::Error> for Failure {
    fn from(_: serde_json::Error) -> Self {
        Self::Integrity
    }
}

/// Match the shared wire and PostgreSQL precision without inventing a logical clock.
pub fn now() -> DateTime<Utc> {
    DateTime::from_timestamp_micros(Utc::now().timestamp_micros())
        .expect("the native UTC clock must fit its own microsecond representation")
}
