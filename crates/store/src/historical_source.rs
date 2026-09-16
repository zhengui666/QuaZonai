//! Read-only native SQL inspection of an explicitly selected external old snapshot.
//! Counts and schema names only: never materialize credentials, payloads or chat rows.
use crate::{Store, StoreError};
use contracts::{imports::*, DbCounter, SchemaV1};
use sqlx::Row;

fn count(value: i64) -> Result<DbCounter, StoreError> {
    DbCounter::new(value.try_into().map_err(|_| StoreError::Integrity)?)
        .map_err(|_| StoreError::Integrity)
}

use sqlx::{Connection, PgConnection, Postgres, Transaction};
use std::collections::BTreeMap;

// (native PostgreSQL type, nullable, exclusion). This file is compiled into the adapter,
// audited from Git 313b0e27^'s final 0029 models; no caller supplies SQL or column policy.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProjectionRule {
    pub primary_key: Vec<String>,
    pub columns: BTreeMap<String, (String, bool, Option<HistoricalExclusionReasonV1>)>,
}
impl ProjectionRule {
    pub(crate) fn supports(&self, table: &HistoricalTableCountV1) -> bool {
        self.primary_key == table.primary_key
            && self.columns.len() == table.columns.len()
            && table.columns.iter().all(|column| {
                !column.generated
                    && self
                        .columns
                        .get(&column.name)
                        .is_some_and(|(typ, nullable, _)| {
                            *typ == column.postgres_type && *nullable == column.nullable
                        })
            })
    }
}
pub(crate) fn rules() -> Result<BTreeMap<String, ProjectionRule>, StoreError> {
    serde_json::from_str(include_str!("historical_projection_0029.json"))
        .map_err(|_| StoreError::Integrity)
}

async fn begin(connection: &mut PgConnection) -> Result<Transaction<'_, Postgres>, StoreError> {
    let mut tx = connection.begin().await?;
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
    sqlx::query("SET LOCAL timezone='UTC';")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL datestyle='ISO, YMD'")
        .execute(&mut *tx)
        .await?;
    sqlx::query("SET LOCAL extra_float_digits=3")
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

async fn inspect(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<HistoricalSourceInspectionV1, StoreError> {
    let versions: Vec<String> =
        sqlx::query_scalar("SELECT version_num FROM public.alembic_version ORDER BY version_num")
            .fetch_all(&mut **tx)
            .await?;
    if versions != ["0029_portfolio_candidate_exposure"] {
        return Err(StoreError::Invalid("historical_source_version"));
    }
    let rows = sqlx::query("SELECT c.oid::bigint AS table_oid,c.relname,format('%I.%I',n.nspname,c.relname) AS sql_name,c.relrowsecurity FROM pg_class c JOIN pg_namespace n ON n.oid=c.relnamespace WHERE n.nspname='public' AND c.relkind IN ('r','p') AND NOT c.relispartition ORDER BY c.relname")
            .fetch_all(&mut **tx).await?;
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
            .fetch_one(&mut **tx)
            .await?;
        let columns = sqlx::query("SELECT a.attname,format_type(a.atttypid,a.atttypmod) AS postgres_type,NOT a.attnotnull AS nullable,a.attidentity<>'' AS identity,a.attgenerated<>'' AS generated FROM pg_attribute a WHERE a.attrelid=$1::bigint::oid AND a.attnum>0 AND NOT a.attisdropped ORDER BY a.attnum")
                .bind(row.try_get::<i64, _>("table_oid")?)
                .fetch_all(&mut **tx).await?
                .into_iter()
                .map(|column| Ok(HistoricalColumnV1 {
                    name: column.try_get("attname")?,
                    postgres_type: column.try_get("postgres_type")?,
                    nullable: column.try_get("nullable")?,
                    identity: column.try_get("identity")?,
                    generated: column.try_get("generated")?,
                }))
                .collect::<Result<Vec<_>, sqlx::Error>>()?;
        let primary_key: Vec<String> = sqlx::query_scalar("SELECT a.attname::text FROM pg_constraint c CROSS JOIN LATERAL unnest(c.conkey) WITH ORDINALITY k(attnum,ord) JOIN pg_attribute a ON a.attrelid=c.conrelid AND a.attnum=k.attnum WHERE c.contype='p' AND c.conrelid=$1::bigint::oid ORDER BY k.ord")
            .bind(row.try_get::<i64, _>("table_oid")?).fetch_all(&mut **tx).await?;
        tables.push(HistoricalTableCountV1 {
            table: row.try_get("relname")?,
            rows: count(rows)?,
            primary_key,
            columns,
        });
    }
    let constraints = sqlx::query("SELECT c.conname,c.confmatchtype::text AS match_type,format('%I.%I',ns.nspname,s.relname) AS source_sql,format('%I.%I',nt.nspname,t.relname) AS target_sql,array_agg(sa.attname::text ORDER BY k.ord) AS source_columns,array_agg(ta.attname::text ORDER BY k.ord) AS target_columns,array_agg(quote_ident(sa.attname) ORDER BY k.ord) AS source_keys,array_agg(quote_ident(ta.attname) ORDER BY k.ord) AS target_keys FROM pg_constraint c JOIN pg_class s ON s.oid=c.conrelid JOIN pg_namespace ns ON ns.oid=s.relnamespace JOIN pg_class t ON t.oid=c.confrelid JOIN pg_namespace nt ON nt.oid=t.relnamespace CROSS JOIN LATERAL unnest(c.conkey,c.confkey) WITH ORDINALITY k(source_key,target_key,ord) JOIN pg_attribute sa ON sa.attrelid=s.oid AND sa.attnum=k.source_key JOIN pg_attribute ta ON ta.attrelid=t.oid AND ta.attnum=k.target_key WHERE c.contype='f' AND ns.nspname='public' AND c.conparentid=0 GROUP BY c.oid,c.conname,c.confmatchtype,ns.nspname,s.relname,nt.nspname,t.relname ORDER BY ns.nspname,s.relname,c.conname")
            .fetch_all(&mut **tx).await?;
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
        let match_type = match row.try_get::<String, _>("match_type")?.as_str() {
            "s" => HistoricalForeignKeyMatchV1::Simple,
            "f" => HistoricalForeignKeyMatchV1::Full,
            _ => return Err(StoreError::Invalid("historical_source_match_type")),
        };
        let predicate = match match_type {
            HistoricalForeignKeyMatchV1::Simple => missing,
            HistoricalForeignKeyMatchV1::Full => format!(
                "({missing}) OR (({}) AND ({}))",
                null.join(" OR "),
                nonnull.join(" OR ")
            ),
        };
        let orphan_rows: i64 = sqlx::query_scalar(&format!(
            "SELECT count(*) FROM {source} s WHERE {predicate}"
        ))
        .fetch_one(&mut **tx)
        .await?;
        foreign_keys.push(HistoricalForeignKeyCheckV1 {
            match_type,
            constraint: row.try_get("conname")?,
            source_table: source,
            target_table: target,
            source_columns: row.try_get("source_columns")?,
            target_columns: row.try_get("target_columns")?,
            orphan_rows: count(orphan_rows)?,
        });
    }
    let inspected_at = sqlx::query_scalar("SELECT transaction_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    Ok(HistoricalSourceInspectionV1 {
        schema_version: SchemaV1,
        source_schema_version: versions[0].clone(),
        inspected_at,
        tables,
        foreign_keys,
    })
}

impl Store {
    pub async fn inspect_historical_source(
        &self,
    ) -> Result<HistoricalSourceInspectionV1, StoreError> {
        let mut connection = self.pool.acquire().await?;
        let mut tx = begin(&mut connection).await?;
        let report = inspect(&mut tx).await?;
        tx.rollback().await?;
        Ok(report)
    }

    /// Deployment-only native CSV stream; the callback writes private files, never logs bytes.
    /// A report is valid only after all streams and the callback's durable file writes succeed.
    pub async fn export_historical_rows<F>(
        &self,
        source_installation_id: contracts::Id,
        mut write: F,
    ) -> Result<HistoricalRowExportV1, StoreError>
    where
        F: FnMut(contracts::Id, &[u8]) -> Result<(), StoreError>,
    {
        let rules = rules()?;
        let mut connection = self.pool.acquire().await?;
        // An interrupted COPY stream must never return unread frames to the pool.
        connection.close_on_drop();
        let mut tx = begin(&mut connection).await?;
        let inspection = inspect(&mut tx).await?;
        let missing_tables = rules
            .keys()
            .filter(|name| !inspection.tables.iter().any(|table| &table.table == *name))
            .cloned()
            .collect();
        let mut tables = Vec::new();
        for table in &inspection.tables {
            let rule = rules.get(&table.table);
            let expected = rule.map(|rule| &rule.columns);
            let supported = rule.is_some_and(|rule| rule.supports(table));
            let mut result = HistoricalTableExportV1 {
                table: table.table.clone(),
                source_rows: table.rows,
                projected_rows: DbCounter::ZERO,
                object_ref: None,
                byte_count: None,
                columns: Vec::new(),
                excluded_columns: Vec::new(),
                unsupported_schema: !supported,
            };
            for column in &table.columns {
                let exclusion = if supported {
                    expected
                        .and_then(|columns| columns.get(&column.name))
                        .ok_or(StoreError::Integrity)?
                        .2
                } else {
                    Some(HistoricalExclusionReasonV1::UnsupportedSchema)
                };
                match exclusion {
                    Some(reason) => result.excluded_columns.push(HistoricalColumnExclusionV1 {
                        column: column.name.clone(),
                        reason,
                    }),
                    None => result.columns.push(column.name.clone()),
                }
            }
            if !result.columns.is_empty() {
                // All identifiers here matched the compiled allowlist. Double quotes are still
                // escaped correctly; there is no request-provided SQL or arbitrary expression.
                let quote = |name: &str| format!("\"{}\"", name.replace('"', "\"\""));
                let columns = result
                    .columns
                    .iter()
                    .map(|name| quote(name))
                    .collect::<Vec<_>>()
                    .join(",");
                let sql = format!("COPY (SELECT {columns} FROM public.{}) TO STDOUT WITH (FORMAT CSV, HEADER true, FORCE_QUOTE *, ENCODING 'UTF8')", quote(&table.table));
                let object = contracts::Id::new();
                let mut byte_count = 0u64;
                let mut stream = tx.copy_out_raw(&sql).await?;
                while let Some(chunk) =
                    std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await
                {
                    let chunk = chunk?;
                    byte_count = byte_count
                        .checked_add(chunk.len() as u64)
                        .ok_or(StoreError::Integrity)?;
                    if byte_count > 512 * 1024 * 1024 {
                        return Err(StoreError::Invalid("historical_projection_size"));
                    }
                    write(object, &chunk)?;
                }
                result.object_ref = Some(object);
                result.byte_count =
                    Some(DbCounter::new(byte_count).map_err(|_| StoreError::Integrity)?);
                result.projected_rows = table.rows;
            }
            tables.push(result);
        }
        tx.rollback().await?;
        Ok(HistoricalRowExportV1 {
            schema_version: SchemaV1,
            source_installation_id,
            inspection,
            missing_tables,
            tables,
        })
    }
}
