//! Native SQLx migration execution, surrounded by one PostgreSQL write cutover.
use crate::{Store, StoreError};
use sqlx::{migrate::Migrate, Connection};

impl Store {
    /// Use the migration identity, after pausing API/worker writers. A dedicated
    /// connection is always closed, including on error/cancellation: SQLx's
    /// session advisory lock must never leak into the application pool.
    pub async fn migrate(&self) -> Result<(), StoreError> {
        self.migrate_with_application_role(None).await
    }

    /// One commit for the complete deployment command, including the pinned
    /// native session schema and optional runtime DML grants. Never execute a
    /// post-migration step on another pooled connection.
    pub async fn migrate_with_application_role(
        &self,
        application_role: Option<&str>,
    ) -> Result<(), StoreError> {
        let mut connection = self.pool.acquire().await?.detach();
        let result = async {
            sqlx::query("SET lock_timeout='5s'").execute(&mut connection).await?;
            Migrate::lock(&mut connection).await?;
            let mut tx = connection.begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL READ COMMITTED")
                .execute(&mut *tx).await?;
            let tables: Vec<String> = sqlx::query_scalar(
                "SELECT format('%I.%I',n.nspname,c.relname) FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname IN ('app','tower_sessions') AND c.relkind IN ('r','p') ORDER BY (n.nspname='app' AND c.relname='operator_auth_state') DESC,n.nspname,c.relname",
            ).fetch_all(&mut *tx).await?;
            for table in tables {
                // Identifiers come exclusively from PostgreSQL's %I formatter.
                sqlx::query(&format!("LOCK TABLE {table} IN SHARE ROW EXCLUSIVE MODE"))
                    .execute(&mut *tx).await?;
            }
            let quoted_role = if let Some(role) = application_role {
                let quoted: Option<String> = sqlx::query_scalar(
                    "SELECT pg_catalog.quote_ident(rolname) FROM pg_catalog.pg_roles WHERE rolname=$1",
                ).bind(role).fetch_optional(&mut *tx).await?;
                Some(quoted.ok_or(StoreError::Invalid("application_role_does_not_exist"))?)
            } else {
                None
            };
            let mut migrator = sqlx::migrate!("../../migrations");
            if migrator.iter().any(|migration| migration.no_tx) {
                return Err(StoreError::Invalid("nontransactional_migration_is_not_supported"));
            }
            // Native Migrate::lock above already serializes this exact database.
            // Retain it through the OUTER commit, rather than releasing it at a
            // nested savepoint. No custom version/checksum/migration runner.
            migrator.set_locking(false).run_direct(&mut *tx).await?;
            // CREATE IF NOT EXISTS alone does not validate an existing table.
            let compatible: bool = sqlx::query_scalar(include_str!("session_schema.sql"))
                .fetch_one(&mut *tx).await?;
            if !compatible {
                return Err(StoreError::Invalid("native_session_schema_incompatible"));
            }
            if let Some(role) = quoted_role {
                for statement in [
                    format!("GRANT USAGE ON SCHEMA app,tower_sessions,pgmq TO {role}"),
                    format!("GRANT SELECT,INSERT,UPDATE ON ALL TABLES IN SCHEMA app TO {role}"),
                    format!("GRANT SELECT,INSERT,UPDATE,DELETE ON ALL TABLES IN SCHEMA tower_sessions,pgmq TO {role}"),
                    format!("GRANT USAGE,SELECT ON ALL SEQUENCES IN SCHEMA pgmq TO {role}"),
                ] {
                    sqlx::query(&statement).execute(&mut *tx).await?;
                }
            }
            tx.commit().await?;
            Ok(())
        }.await;
        // The commit result, not a subsequent socket-close outcome, is decisive.
        // Await the native unlock acknowledgement instead of racing backend
        // teardown. SQLx flushes any queued transaction rollback first.
        // On transport failure, closing still disposes this dedicated session.
        let _ = Migrate::unlock(&mut connection).await;
        let _ = connection.close().await;
        result
    }
}
