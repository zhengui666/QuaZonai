//! Read-only native SQL inspection of an explicitly selected external old snapshot.
//! Counts and schema names only: never materialize credentials, payloads or chat rows.
use crate::{Store, StoreError};
use contracts::{imports::*, DbCounter, SchemaV1};
use sqlx::Row;

fn count(value: i64) -> Result<DbCounter, StoreError> {
    DbCounter::new(value.try_into().map_err(|_| StoreError::Integrity)?)
        .map_err(|_| StoreError::Integrity)
}

impl Store {
    pub async fn inspect_historical_source(
        &self,
    ) -> Result<HistoricalSourceInspectionV1, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        sqlx::query("SET LOCAL statement_timeout='30s'")
            .execute(&mut *tx)
            .await?;
        // Fail instead of silently counting a policy-filtered subset, including FK targets
        // outside public. This does not grant any additional row access.
        sqlx::query("SET LOCAL row_security=off")
            .execute(&mut *tx)
            .await?;
        let versions: Vec<String> = sqlx::query_scalar(
            "SELECT version_num FROM public.alembic_version ORDER BY version_num",
        )
        .fetch_all(&mut *tx)
        .await?;
        if versions != ["0029_portfolio_candidate_exposure"] {
            return Err(StoreError::Invalid("historical_source_version"));
        }
        let rows = sqlx::query("SELECT c.oid::bigint AS table_oid,c.relname,format('%I.%I',n.nspname,c.relname) AS sql_name,c.relrowsecurity FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind IN ('r','p') AND NOT c.relispartition ORDER BY c.relname")
            .fetch_all(&mut *tx).await?;
        if rows.len() > 256 {
            return Err(StoreError::Invalid("historical_source_tables"));
        }
        let mut tables = Vec::new();
        for row in rows {
            if row.try_get::<bool, _>("relrowsecurity")? {
                return Err(StoreError::Invalid("historical_source_row_visibility"));
            }
            let name: String = row.try_get("sql_name")?;
            // Identifiers are quoted by PostgreSQL from its own catalog, never caller SQL.
            let rows: i64 = sqlx::query_scalar(&format!("SELECT count(*) FROM {name}"))
                .fetch_one(&mut *tx)
                .await?;
            let columns = sqlx::query("SELECT a.attname,format_type(a.atttypid,a.atttypmod) AS postgres_type,NOT a.attnotnull AS nullable,a.attidentity<>'' AS identity,a.attgenerated<>'' AS generated FROM pg_attribute a WHERE a.attrelid=$1::bigint::oid AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum")
                .bind(row.try_get::<i64, _>("table_oid")?)
                .fetch_all(&mut *tx).await?
                .into_iter()
                .map(|column| Ok(HistoricalColumnV1 {
                    name: column.try_get("attname")?,
                    postgres_type: column.try_get("postgres_type")?,
                    nullable: column.try_get("nullable")?,
                    identity: column.try_get("identity")?,
                    generated: column.try_get("generated")?,
                }))
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
            tables.push(HistoricalTableCountV1 {
                table: row.try_get("relname")?,
                rows: count(rows)?,
                columns,
            });
        }
        let constraints = sqlx::query("SELECT c.conname,c.confmatchtype::text AS match_type,format('%I.%I',ns.nspname,s.relname) AS source_sql,format('%I.%I',nt.nspname,t.relname) AS target_sql,array_agg(sa.attname::text ORDER BY k.ord) AS source_columns,array_agg(ta.attname::text ORDER BY k.ord) AS target_columns,array_agg(quote_ident(sa.attname) ORDER BY k.ord) AS source_keys,array_agg(quote_ident(ta.attname) ORDER BY k.ord) AS target_keys FROM pg_constraint c JOIN pg_class s ON s.oid=c.conrelid JOIN pg_namespace ns ON ns.oid=s.relnamespace JOIN pg_class t ON t.oid=c.confrelid JOIN pg_namespace nt ON nt.oid=t.relnamespace CROSS JOIN LATERAL unnest(c.conkey,c.confkey) WITH ORDINALITY k(source_key,target_key,ord) JOIN pg_attribute sa ON sa.attrelid=s.oid AND sa.attnum=k.source_key JOIN pg_attribute ta ON ta.attrelid=t.oid AND ta.attnum=k.target_key WHERE c.contype='f' AND ns.nspname='public' AND c.conparentid=0 GROUP BY c.oid,c.conname,c.confmatchtype,ns.nspname,s.relname,nt.nspname,t.relname ORDER BY ns.nspname,s.relname,c.conname")
            .fetch_all(&mut *tx).await?;
        if constraints.len() > 1024 {
            return Err(StoreError::Invalid("historical_source_constraints"));
        }
        let mut foreign_keys = Vec::new();
        for row in constraints {
            let source: String = row.try_get("source_sql")?;
            let target: String = row.try_get("target_sql")?;
            let keys: Vec<String> = row.try_get("source_keys")?;
            let targets: Vec<String> = row.try_get("target_keys")?;
            let nonnull = keys
                .iter()
                .map(|k| format!("s.{k} IS NOT NULL"))
                .collect::<Vec<_>>();
            let null = keys
                .iter()
                .map(|k| format!("s.{k} IS NULL"))
                .collect::<Vec<_>>();
            let equal = keys
                .iter()
                .zip(&targets)
                .map(|(s, t)| format!("s.{s}=t.{t}"))
                .collect::<Vec<_>>()
                .join(" AND ");
            let missing = format!(
                "({}) AND NOT EXISTS(SELECT 1 FROM {target} t WHERE {equal})",
                nonnull.join(" AND ")
            );
            let predicate = match row.try_get::<String, _>("match_type")?.as_str() {
                "s" => missing,
                "f" => format!(
                    "({missing}) OR (({}) AND ({}))",
                    null.join(" OR "),
                    nonnull.join(" OR ")
                ),
                _ => return Err(StoreError::Invalid("historical_source_match_type")),
            };
            let orphan_rows: i64 = sqlx::query_scalar(&format!(
                "SELECT count(*) FROM {source} s WHERE {predicate}"
            ))
            .fetch_one(&mut *tx)
            .await?;
            foreign_keys.push(HistoricalForeignKeyCheckV1 {
                constraint: row.try_get("conname")?,
                source_table: source,
                target_table: target,
                source_columns: row.try_get("source_columns")?,
                target_columns: row.try_get("target_columns")?,
                orphan_rows: count(orphan_rows)?,
            });
        }
        let inspected_at = sqlx::query_scalar("SELECT transaction_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        tx.rollback().await?;
        Ok(HistoricalSourceInspectionV1 {
            schema_version: SchemaV1,
            source_schema_version: versions[0].clone(),
            inspected_at,
            tables,
            foreign_keys,
        })
    }
}
