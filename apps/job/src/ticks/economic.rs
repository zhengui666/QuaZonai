//! Original funding, context and published-book data adapters, without economic strategy execution.
use anyhow::{ensure, Context, Result};
use contracts::{DbCounter, SchemaV1};
use rust_decimal::Decimal;
use serde::Deserialize;
use std::{fs::File, io::BufReader, path::Path};

pub(super) mod books;
pub mod closes;
mod source_books;
pub use source_books::{
    export_catalog as export_published_books,
    export_catalog_selected as export_published_books_selected,
};

#[derive(Deserialize)]
struct FundingManifest {
    source_revision: String,
    start_ns: DbCounter,
    end_ns: DbCounter,
    #[serde(rename = "instruments")]
    _instruments: Vec<String>,
    #[serde(rename = "rows")]
    _rows: u64,
    #[serde(rename = "missing_instruments")]
    _missing_instruments: Vec<String>,
}
const CONTEXT_MANIFEST: &str = "context-prices.manifest.json";
const EVENT_TIME_CONTEXT_MANIFEST: &str = "event-time-context-prices.manifest.json";
#[derive(Deserialize)]
struct ContextFile {
    instrument_id: String,
    path: String,
    bytes: DbCounter,
    rows: DbCounter,
    first_event_ns: Option<DbCounter>,
    last_event_ns: Option<DbCounter>,
}
#[derive(Deserialize)]
struct ContextManifest {
    schema_version: SchemaV1,
    format: String,
    clock: String,
    source_revision: String,
    research_source_revision: String,
    start_ns: DbCounter,
    end_ns: DbCounter,
    rows: DbCounter,
    instrument_ids: Vec<String>,
    files: Vec<ContextFile>,
}
fn context_manifest(
    root: &Path,
    request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
) -> Result<ContextManifest> {
    context_manifest_at(root, request, CONTEXT_MANIFEST, "context-prices-", false)
}
fn event_time_context_manifest(
    root: &Path,
    request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
) -> Result<ContextManifest> {
    context_manifest_at(
        root,
        request,
        EVENT_TIME_CONTEXT_MANIFEST,
        "event-time-context-prices-",
        true,
    )
}
fn context_manifest_at(
    root: &Path,
    request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
    manifest_name: &str,
    file_prefix: &str,
    event_time_only: bool,
) -> Result<ContextManifest> {
    let state = std::fs::symlink_metadata(root.join(manifest_name))?;
    ensure!(
        state.is_file() && !state.file_type().is_symlink(),
        "ECONOMIC_CONTEXT_MANIFEST_INVALID"
    );
    let m: ContextManifest =
        serde_json::from_reader(BufReader::new(File::open(root.join(manifest_name))?))?;
    let _ = m.schema_version;
    let clock_matches = if event_time_only {
        m.format == "EVENT_TIME_CONTEXT_PRICE_CSV_V1" && m.clock == "EVENT_TIME_PROXY_UNVERIFIED"
    } else {
        ((m.format == "EVENT_TIME_CONTEXT_PRICE_CSV_V1"
            && m.clock == "EVENT_TIME_PROXY_UNVERIFIED")
            || (m.format == "RECEIVED_TIME_CONTEXT_PRICE_CSV_V1"
                && m.clock == "RECEIVED_TIME_OBSERVED"))
            && (!request.input.uses_received_time() || m.clock == "RECEIVED_TIME_OBSERVED")
    };
    ensure!(
        clock_matches
            && !m.source_revision.trim().is_empty()
            && m.research_source_revision == request.input.source_revision
            && m.start_ns <= request.input.event_start_ns
            && m.end_ns >= request.input.event_end_ns
            && m.start_ns < m.end_ns
            && !m.instrument_ids.is_empty()
            && m.instrument_ids.windows(2).all(|p| p[0] < p[1])
            && m.files.len() == m.instrument_ids.len(),
        "ECONOMIC_CONTEXT_MANIFEST_INVALID"
    );
    let mut rows = 0u64;
    for (index, f) in m.files.iter().enumerate() {
        ensure!(
            f.instrument_id == m.instrument_ids[index]
                && !f.instrument_id.is_empty()
                && f.path == format!("{file_prefix}{index:03}.csv"),
            "ECONOMIC_CONTEXT_FILE_INVALID"
        );
        let state = std::fs::symlink_metadata(root.join(&f.path))?;
        ensure!(
            state.is_file()
                && !state.file_type().is_symlink()
                && state.len() == f.bytes.get()
                && state.len() >= "event_ns,oracle_px,mark_px\n".len() as u64,
            "ECONOMIC_CONTEXT_FILE_INVALID"
        );
        ensure!(
            if f.rows.get() == 0 {
                f.first_event_ns.is_none() && f.last_event_ns.is_none()
            } else {
                matches!((f.first_event_ns, f.last_event_ns), (Some(first), Some(last))
                if first <= last && first >= m.start_ns && last < m.end_ns)
            },
            "ECONOMIC_CONTEXT_RANGE_INVALID"
        );
        rows = rows
            .checked_add(f.rows.get())
            .context("ECONOMIC_CONTEXT_ROW_OVERFLOW")?;
    }
    ensure!(rows == m.rows.get(), "ECONOMIC_CONTEXT_ROW_COUNT_MISMATCH");
    Ok(m)
}
pub fn context_decimal(text: &str) -> Result<Option<Decimal>> {
    if text.is_empty() {
        return Ok(None);
    }
    let value = if text.contains(['e', 'E']) {
        Decimal::from_scientific(text)?
    } else {
        Decimal::from_str_exact(text)?
    };
    ensure!(value > Decimal::ZERO, "ECONOMIC_CONTEXT_PRICE_INVALID");
    Ok(Some(value))
}
pub(crate) fn funding_sources(
    root: &Path,
    request: &contracts::ticks::NativeTradeTickFactorEvaluationRequestV1,
) -> Result<Vec<(String, u64)>> {
    let names = ["funding-rates.jsonl", "funding-rates.manifest.json"];
    let mut files = Vec::new();
    if names.iter().any(|name| root.join(name).exists()) {
        for name in names {
            let state = std::fs::symlink_metadata(root.join(name))?;
            ensure!(
                state.is_file() && !state.file_type().is_symlink(),
                "ECONOMIC_FUNDING_REGULAR_SOURCE_REQUIRED"
            );
            files.push((name.into(), state.len()));
        }
        let manifest: FundingManifest =
            serde_json::from_reader(BufReader::new(File::open(root.join(names[1]))?))?;
        ensure!(
            manifest.source_revision == request.input.source_revision
                && manifest.start_ns <= request.input.event_start_ns
                && manifest.end_ns >= request.input.event_end_ns,
            "ECONOMIC_FUNDING_COVERAGE_INVALID"
        );
    }
    if let Some(manifest) = closes::manifest(root, request)? {
        files.push((
            closes::MANIFEST.into(),
            std::fs::metadata(root.join(closes::MANIFEST))?.len(),
        ));
        files.extend(
            manifest
                .files
                .into_iter()
                .map(|file| (file.path, file.bytes.get())),
        );
    }
    if root.join(CONTEXT_MANIFEST).exists() {
        let context = context_manifest(root, request)?;
        files.push((
            CONTEXT_MANIFEST.into(),
            std::fs::metadata(root.join(CONTEXT_MANIFEST))?.len(),
        ));
        files.extend(
            context
                .files
                .into_iter()
                .map(|file| (file.path, file.bytes.get())),
        );
    }
    if root.join(EVENT_TIME_CONTEXT_MANIFEST).exists() {
        let context = event_time_context_manifest(root, request)?;
        files.push((
            EVENT_TIME_CONTEXT_MANIFEST.into(),
            std::fs::metadata(root.join(EVENT_TIME_CONTEXT_MANIFEST))?.len(),
        ));
        files.extend(
            context
                .files
                .into_iter()
                .map(|file| (file.path, file.bytes.get())),
        );
    }
    for (name, issue) in [
        (
            "spot-valuation.jsonl",
            "ECONOMIC_SPOT_QUOTE_REGULAR_SOURCE_REQUIRED",
        ),
        (
            "book-snapshots.source.json",
            "ECONOMIC_BOOK_SOURCE_REGULAR_FILE_REQUIRED",
        ),
    ] {
        match std::fs::symlink_metadata(root.join(name)) {
            Ok(state) => {
                ensure!(
                    state.is_file() && !state.file_type().is_symlink(),
                    "{issue}"
                );
                files.push((name.into(), state.len()));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(files)
}

#[cfg(test)]
mod tests;
