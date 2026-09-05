//! PostgreSQL/PGMQ transactions. No HTTP/authentication, queue implementation,
//! model loop, or scientific estimator lives here. Callers are trusted services;
//! untrusted agents and workers executing research code never get this pool.
#![forbid(unsafe_code)]

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

    /// Invoke with the separate migration role against a new database. The
    /// application role must not own the schema or hold DDL/TRUNCATE privilege.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        sqlx::migrate!("../../migrations").run(&self.pool).await?;
        Ok(())
    }
}
