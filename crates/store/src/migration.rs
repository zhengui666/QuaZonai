//! Native SQLx migration execution, surrounded by one PostgreSQL write cutover.
use crate::{Store, StoreError};
use sqlx::{migrate::Migrate, Connection};

impl Store {
    /// Use the migration identity, after pausing API/worker writers. A dedicated
    /// connection is always closed, including on error/cancellation: SQLx's
    /// session advisory lock must never leak into the application pool.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        let mut connection = self.pool.acquire().await?.detach();
        let result = async {
            sqlx::query("SET lock_timeout='5s'").execute(&mut connection).await?;
            Migrate::lock(&mut connection).await?;
            let mut tx = connection.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
                .execute(&mut *tx).await?;
            let tables: Vec<String> = sqlx::query_scalar(
                "SELECT format('%I.%I',n.nspname,c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='app' AND c.relkind IN ('r','p') ORDER BY (c.relname='operator_auth_state') DESC,c.relname",
            ).fetch_all(&mut *tx).await?;
            for table in tables {
                // Identifiers come exclusively from PostgreSQL's %I formatter.
                sqlx::query(&format!("LOCK TABLE {table} IN SHARE ROW EXCLUSIVE MODE"))
                    .execute(&mut *tx).await?;
            }
            let mut migrator = sqlx::migrate!("../../migrations");
            if migrator.iter().any(|migration| migration.no_tx) {
                return Err(StoreError::Invalid("nontransactional_migration_is_not_supported"));
            }
            // Native Migrate::lock above already serializes this exact database.
            // Retain it through the OUTER commit, rather than releasing it at a
            // nested savepoint. No custom version/checksum/migration runner.
            migrator.set_locking(false).run_direct(&mut *tx).await?;
            tx.commit().await?;
            Ok(())
        }.await;
        // The commit result, not a subsequent socket-close outcome, is decisive.
        // Closing also releases SQLx's native session advisory lock on failure.
        let _ = connection.close().await;
        result
    }
}
