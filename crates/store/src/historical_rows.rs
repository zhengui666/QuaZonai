//! Native CSV decoding for historical import. Only temporary tables are written.
//! This validates one projection's keys/values, not full relations or import authority.
use crate::{historical_source, Store, StoreError};
use contracts::{imports::*, DbCounter, Id};
use sqlx::{Connection, Row};
use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom},
};

fn invalid() -> StoreError {
    StoreError::Invalid("historical_projection")
}
fn quote(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}
fn literal(name: &str) -> String {
    format!("'{}'", name.replace('\'', "''"))
}

impl Store {
    /// Internal native adapter: visits canonical old rows after validating COPY and
    /// roundtrip bytes. A visitor may see a prefix if it fails; it must not publish
    /// committed history without the outer import transaction's completion.
    pub async fn visit_historical_projection<R, V>(
        &self,
        source_installation_id: Id,
        inspection: &HistoricalTableCountV1,
        projection: &HistoricalTableExportV1,
        mut input: R,
        mut visit: V,
    ) -> Result<DbCounter, StoreError>
    where
        R: Read + Seek,
        V: FnMut(HistoricalProjectedRowV1) -> Result<(), StoreError>,
    {
        let mut connection = self.pool.acquire().await?;
        connection.close_on_drop();
        let mut tx = connection.begin().await?;
        let staged = stage(&mut tx, inspection, projection, &mut input).await?;
        let sql = staged.select.as_str();
        let mut stream = sqlx::query(sqlx::AssertSqlSafe(sql)).fetch(&mut *tx);
        let mut visited = 0u64;
        while let Some(row) = std::future::poll_fn(|cx| stream.as_mut().poll_next(cx)).await {
            let row = row?;
            visit(HistoricalProjectedRowV1 {
                key: HistoricalOriginalKeyV1 {
                    source_installation_id,
                    source_table: inspection.table.clone(),
                    values: row
                        .try_get::<sqlx::types::Json<BTreeMap<String, String>>, _>("original_key")?
                        .0,
                },
                fields: row
                    .try_get::<sqlx::types::Json<BTreeMap<String, Option<String>>>, _>("fields")?
                    .0,
            })?;
            visited += 1;
        }
        drop(stream);
        if visited != staged.rows.get() {
            return Err(invalid());
        }
        tx.rollback().await?;
        DbCounter::new(visited).map_err(|_| invalid())
    }
}

// Dynamic SQL below uses only the fixed projection rules and quoted identifiers.
// Callers retain this provenance when composing the returned SELECT and table.
pub(crate) struct StagedProjection {
    pub table: String,
    pub select: String,
    pub rows: DbCounter,
    pub columns: Vec<String>,
}

/// Caller retains one authority/commit transaction for the entire import. The
/// transfer connection must be close_on_drop before entering native COPY.
pub(crate) async fn stage<R: Read + Seek>(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    inspection: &HistoricalTableCountV1,
    projection: &HistoricalTableExportV1,
    input: &mut R,
) -> Result<StagedProjection, StoreError> {
    let rules = historical_source::rules()?;
    let rule = rules.get(&inspection.table).ok_or_else(invalid)?;
    let columns = inspection
        .columns
        .iter()
        .filter(|column| {
            rule.columns
                .get(&column.name)
                .is_some_and(|(_, _, exclusion)| exclusion.is_none())
        })
        .map(|c| c.name.clone())
        .collect::<Vec<_>>();
    let expected_bytes = projection.byte_count.ok_or_else(invalid)?.get();
    if !rule.supports(inspection)
        || rule.primary_key.is_empty()
        || projection.unsupported_schema
        || projection.table != inspection.table
        || projection.columns != columns
        || projection.source_rows != inspection.rows
        || projection.projected_rows != inspection.rows
        || projection.object_ref.is_none()
        || expected_bytes == 0
        || expected_bytes > 512 * 1024 * 1024
        || !rule.primary_key.iter().all(|key| columns.contains(key))
    {
        return Err(invalid());
    }
    for setting in [
        "SET LOCAL timezone='UTC'",
        "SET LOCAL datestyle='ISO, YMD'",
        "SET LOCAL extra_float_digits=3",
        "SET LOCAL statement_timeout='30s'",
    ] {
        sqlx::query(setting).execute(&mut **tx).await?;
    }
    let table = quote(&format!("historical_{}", Id::new()));
    let definitions = columns
        .iter()
        .map(|name| {
            let (typ, nullable, _) = &rule.columns[name];
            format!(
                "{} {typ} {}",
                quote(name),
                if *nullable { "" } else { "NOT NULL" }
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    let keys = rule
        .primary_key
        .iter()
        .map(|k| quote(k))
        .collect::<Vec<_>>()
        .join(",");
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "CREATE TEMP TABLE {table} ({definitions},PRIMARY KEY({keys})) ON COMMIT DROP"
    )))
    .execute(&mut **tx)
    .await?;
    input.seek(SeekFrom::Start(0)).map_err(|_| invalid())?;
    let mut copy = tx
        .copy_in_raw(&format!(
            "COPY pg_temp.{table} FROM STDIN WITH (FORMAT CSV, HEADER MATCH, ENCODING 'UTF8')"
        ))
        .await?;
    let mut buffer = [0u8; 64 * 1024];
    let mut bytes = 0u64;
    loop {
        let n = input.read(&mut buffer).map_err(|_| invalid())?;
        if n == 0 {
            break;
        }
        bytes = bytes.checked_add(n as u64).ok_or_else(invalid)?;
        if bytes > expected_bytes {
            return Err(invalid());
        }
        copy.send(&buffer[..n]).await?;
    }
    let rows = copy.finish().await?;
    if bytes != expected_bytes || rows != projection.projected_rows.get() {
        return Err(invalid());
    }
    // PostgreSQL typmods can silently round numbers/timestamps. Require the exact
    // native exporter representation on a second COPY before exposing any row.
    input.seek(SeekFrom::Start(0)).map_err(|_| invalid())?;
    let mut native = tx.copy_out_raw(&format!("COPY pg_temp.{table} TO STDOUT WITH (FORMAT CSV, HEADER true, FORCE_QUOTE *, ENCODING 'UTF8')")).await?;
    while let Some(chunk) = std::future::poll_fn(|cx| native.as_mut().poll_next(cx)).await {
        let chunk = chunk?;
        let mut original = vec![0; chunk.len()];
        input.read_exact(&mut original).map_err(|_| invalid())?;
        if original != chunk {
            return Err(invalid());
        }
    }
    drop(native);
    if input.read(&mut buffer[..1]).map_err(|_| invalid())? != 0 {
        return Err(invalid());
    }
    let object = |names: &[String]| {
        format!(
            "jsonb_object(ARRAY[{}],ARRAY[{}])",
            names
                .iter()
                .map(|k| literal(k))
                .collect::<Vec<_>>()
                .join(","),
            names
                .iter()
                .map(|k| format!("{}::text", quote(k)))
                .collect::<Vec<_>>()
                .join(",")
        )
    };
    let sql = format!(
        "SELECT {} AS original_key,{} AS fields FROM pg_temp.{table}",
        object(&rule.primary_key),
        object(&columns)
    );
    Ok(StagedProjection {
        table,
        select: sql,
        rows: DbCounter::new(rows).map_err(|_| invalid())?,
        columns,
    })
}
