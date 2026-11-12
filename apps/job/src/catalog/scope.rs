//! Native BAR catalog identities only; no factor calculation.
use anyhow::{ensure, Result};
use nautilus_model::data::BarType;
use parquet::arrow::arrow_reader::ParquetRecordBatchReaderBuilder;
use std::{
    collections::HashSet,
    fs::{self, File},
    path::{Path, PathBuf},
};

fn series_paths(directory: &Path) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "parquet")
        {
            paths.push(path);
        }
    }
    paths.sort();
    ensure!(!paths.is_empty(), "CATALOG_EMPTY_SERIES");
    Ok(paths)
}

fn series_directories(root: &Path) -> Result<Vec<PathBuf>> {
    let root = crate::catalog::catalog_root(root)?;
    let mut paths = Vec::new();
    for entry in fs::read_dir(root.join("data/bars"))? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            paths.push(entry.path());
        }
    }
    paths.sort();
    ensure!(!paths.is_empty(), "CATALOG_EMPTY_SELECTION");
    Ok(paths)
}

fn metadata_kind(metadata: &std::collections::HashMap<String, String>) -> Result<BarType> {
    let name = metadata
        .get("bar_type")
        .ok_or_else(|| anyhow::anyhow!("CATALOG_BAR_TYPE_MISSING"))?;
    Ok(crate::catalog::bar_types(std::slice::from_ref(name))?[0])
}

/// Machine-private identities for scientific exposure; no BAR values or labels.
pub fn catalog_factor_scope(root: &Path) -> Result<Vec<String>> {
    let mut identities = HashSet::new();
    let mut types = Vec::new();
    for directory in series_directories(root)? {
        let mut identity = None;
        for path in series_paths(&directory)? {
            let builder = ParquetRecordBatchReaderBuilder::try_new(File::open(path)?)?;
            let kind = metadata_kind(builder.schema().metadata())?;
            ensure!(
                identity.is_none_or(|original| original == kind),
                "CATALOG_SERIES_IDENTITY_MISMATCH"
            );
            identity = Some(kind);
        }
        let kind = identity.expect("nonempty native file list");
        ensure!(identities.insert(kind), "DUPLICATE_CATALOG_BAR_TYPE");
        types.push(kind.to_string());
    }
    Ok(types)
}
