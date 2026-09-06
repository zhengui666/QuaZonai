//! PostgreSQL/PGMQ transactions. No HTTP/authentication, queue implementation,
//! model loop, or scientific estimator lives here. Callers are trusted services;
//! untrusted agents and workers executing research code never get this pool.
#![forbid(unsafe_code)]

pub mod auth;
pub mod authority;
mod commands;
pub mod control;
mod db;
pub mod lifecycle;
pub mod machine_auth;
mod migration;
pub mod research;
pub mod turns;

use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;
use thiserror::Error;

#[derive(Clone)]
pub struct Store {
    pool: PgPool,
}

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("authentication required")]
    AuthenticationRequired,
    #[error("authentication attempt rejected")]
    InvalidCredentials,
    #[error("initial setup has already completed")]
    SetupCompleted,
    #[error("authentication code has already been consumed")]
    TotpReplay,
    #[error("recent authentication required")]
    RecentAuthenticationRequired,
    #[error("authentication rate limit exceeded")]
    AuthRateLimited { retry_after_seconds: u32 },
    #[error("operation is not permitted for this identity")]
    Forbidden,
    #[error("object revision changed")]
    RevisionConflict { current: contracts::Revision },
    #[error("idempotency key was already used with different command content")]
    IdempotencyConflict,
    #[error("stored contract integrity check failed")]
    Integrity,
    #[error("secret reconciliation could not be completed")]
    SecretCleanup,
    #[error("event cursor no longer matches the durable stream")]
    EventCursorExpired,
    #[error("event contract is not supported by this reader")]
    EventContractUnsupported,
    #[error("record not found")]
    NotFound,
    #[error("conflicting immutable command or native identity")]
    Conflict,
    #[error("mission already has an unresolved model turn")]
    TurnPending,
    #[error("invalid store contract: {0}")]
    Invalid(&'static str),
    #[error("domain command rejected: {0}")]
    Domain(#[from] domain::DomainError),
    // Never send PostgreSQL details or a connection URL to a browser/agent.
    #[error("database operation failed")]
    Database(#[from] sqlx::Error),
    #[error("database migration failed")]
    Migration(#[from] sqlx::migrate::MigrateError),
}

impl Store {
    /// For native adapters in trusted entrypoints (for example tower-sessions).
    /// This is never exposed through an Agent tool or an HTTP/CLI data endpoint.
    pub fn native_pool(&self) -> PgPool {
        self.pool.clone()
    }

    pub async fn verify_runtime_role(&self) -> Result<(), StoreError> {
        // Inspect native ownership/ACLs across the entire application schema,
        // including authority reachable through inherited or SET ROLE grants.
        // Checking one authentication table would miss destructive access to
        // unrelated immutable research/evidence records.
        let elevated: bool = sqlx::query_scalar(include_str!("runtime_role.sql"))
            .fetch_one(&self.pool)
            .await?;
        if elevated {
            return Err(StoreError::Invalid(
                "runtime_role_must_be_non_owner_and_unprivileged",
            ));
        }
        Ok(())
    }

    pub fn from_pool(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(url: &str) -> Result<Self, StoreError> {
        let pool = PgPoolOptions::new()
            .max_connections(16)
            .acquire_timeout(Duration::from_secs(10))
            .after_connect(|connection, _| {
                Box::pin(async move {
                    sqlx::query("SELECT set_config('timezone','UTC',false), set_config('statement_timeout','15s',false), set_config('lock_timeout','5s',false)")
                    .execute(connection)
                    .await?;
                    Ok(())
                })
            })
            .connect(url)
            .await?;
        Ok(Self { pool })
    }
}
