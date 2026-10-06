//! Offline handoff for qz.hf_selection/1. No HTTP, hashes or scientific admission.
use anyhow::{ensure, Context, Result};
use chrono::{DateTime, NaiveDate, Utc};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    io::{Read, Seek, SeekFrom},
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

const MAX_MANIFEST_BYTES: u64 = 32 * 1024 * 1024;

pub(super) struct Selection {
    pub manifest: Value,
    pub repository: String,
    pub revision: String,
    pub retrieved_at: DateTime<Utc>,
    pub files_root: PathBuf,
    pub files: Vec<PathBuf>,
    pub file_states: BTreeMap<PathBuf, FileState>,
}

/// Detect a changing local handoff; this is an ordinary stat check, not attestation.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct FileState {
    bytes: u64,
    modified: SystemTime,
    #[cfg(unix)]
    device: u64,
    #[cfg(unix)]
    inode: u64,
    #[cfg(unix)]
    changed_seconds: i64,
    #[cfg(unix)]
    changed_nanoseconds: i64,
}

fn no_symlinks(path: &Path) -> Result<()> {
    for ancestor in path.ancestors() {
        let metadata = fs::symlink_metadata(ancestor).context("SELECTION_FILE_MISSING")?;
        ensure!(!metadata.file_type().is_symlink(), "SELECTION_SYMLINK");
    }
    Ok(())
}

pub(super) fn file_state(path: &Path) -> Result<FileState> {
    no_symlinks(path)?;
    let metadata = fs::metadata(path).context("SELECTION_FILE_MISSING")?;
    ensure!(metadata.is_file(), "SELECTION_REGULAR_FILE_REQUIRED");
    #[cfg(unix)]
    use std::os::unix::fs::MetadataExt;
    Ok(FileState {
        bytes: metadata.len(),
        modified: metadata.modified()?,
        #[cfg(unix)]
        device: metadata.dev(),
        #[cfg(unix)]
        inode: metadata.ino(),
        #[cfg(unix)]
        changed_seconds: metadata.ctime(),
        #[cfg(unix)]
        changed_nanoseconds: metadata.ctime_nsec(),
    })
}

fn string<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("SELECTION_STRING_REQUIRED:{key}"))
}

fn integer(value: &Value, key: &str) -> Result<u64> {
    value
        .get(key)
        .and_then(Value::as_u64)
        .with_context(|| format!("SELECTION_INTEGER_REQUIRED:{key}"))
}

fn strings<'a>(value: &'a Value, key: &str) -> Result<Vec<&'a str>> {
    value
        .get(key)
        .and_then(Value::as_array)
        .context("SELECTION_REQUEST_INVALID")?
        .iter()
        .map(|v| v.as_str().context("SELECTION_REQUEST_INVALID"))
        .collect()
}

fn checked_path(path: &str) -> Result<()> {
    ensure!(
        !path.is_empty()
            && !path.contains('\\')
            && !path.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}')
            && path
                .split('/')
                .all(|p| !p.is_empty() && p != "." && p != "..")
            && Path::new(path)
                .components()
                .all(|c| matches!(c, Component::Normal(_))),
        "SELECTION_PATH_INVALID"
    );
    Ok(())
}

fn source_url(repository: &str, revision: &str, path: &str) -> String {
    // Match urllib.parse.quote(path): UTF-8, uppercase escapes, '/' stays literal.
    let mut encoded = String::new();
    for byte in path.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.~/".contains(&byte) {
            encoded.push(char::from(byte));
        } else {
            use std::fmt::Write;
            write!(encoded, "%{byte:02X}").expect("writing to String");
        }
    }
    format!("https://huggingface.co/datasets/{repository}/resolve/{revision}/{encoded}")
}

fn date(value: &str) -> Result<i64> {
    let parsed = NaiveDate::parse_from_str(value, "%Y-%m-%d").context("SELECTION_DATE_INVALID")?;
    ensure!(
        parsed.format("%Y-%m-%d").to_string() == value,
        "SELECTION_DATE_INVALID"
    );
    Ok(parsed
        .and_hms_opt(0, 0, 0)
        .context("SELECTION_DATE_INVALID")?
        .and_utc()
        .timestamp())
}

fn parquet(path: &Path, size: u64) -> Result<FileState> {
    let before = file_state(path)?;
    ensure!(before.bytes == size, "SELECTION_BYTE_SIZE_MISMATCH");
    ensure!(size >= 12, "SELECTION_PARQUET_ENVELOPE");
    let mut input = fs::File::open(path)?;
    let mut magic = [0; 4];
    input.read_exact(&mut magic)?;
    ensure!(&magic == b"PAR1", "SELECTION_PARQUET_ENVELOPE");
    input.seek(SeekFrom::End(-4))?;
    input.read_exact(&mut magic)?;
    ensure!(&magic == b"PAR1", "SELECTION_PARQUET_ENVELOPE");
    ensure!(before == file_state(path)?, "SELECTION_FILE_CHANGED");
    Ok(before)
}

pub(super) fn load(path: &Path, start: u64, end: u64) -> Result<Selection> {
    ensure!(start < end, "INVALID_HISTORY_WINDOW");
    let (start, end) = (i64::try_from(start)?, i64::try_from(end)?);
    let path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()?.join(path)
    };
    ensure!(
        !path.components().any(|c| matches!(c, Component::ParentDir)),
        "SELECTION_PATH_INVALID"
    );
    let manifest_state = file_state(&path)?;
    ensure!(
        manifest_state.bytes <= MAX_MANIFEST_BYTES,
        "SELECTION_MANIFEST_LIMIT"
    );
    let manifest: Value = super::read_json(&path)?;
    ensure!(
        manifest_state == file_state(&path)?,
        "SELECTION_FILE_CHANGED"
    );
    ensure!(
        string(&manifest, "schema")? == "qz.hf_selection/1",
        "SELECTION_SCHEMA_INVALID"
    );
    let plan = manifest.get("plan").context("SELECTION_PLAN_REQUIRED")?;
    ensure!(
        string(plan, "schema")? == "qz.hf_dataset_plan/1",
        "SELECTION_PLAN_SCHEMA_INVALID"
    );
    let repository = string(plan, "repository")?;
    checked_path(repository)?;
    ensure!(
        repository.split('/').count() <= 2
            && repository
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"-_./".contains(&c)),
        "SELECTION_REPOSITORY_INVALID"
    );
    let revision = string(plan, "revision")?;
    ensure!(
        revision.len() == 40
            && revision
                .bytes()
                .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c)),
        "SELECTION_REVISION_REQUIRED"
    );
    ensure!(
        !string(plan, "requested_revision")?.trim().is_empty(),
        "SELECTION_REQUEST_INVALID"
    );
    ensure!(
        string(plan, "coverage")? == "NOT_ASSERTED"
            && string(plan, "selection_bounds")? == "[start_date,end_date)"
            && string(plan, "download_granularity")? == "FILE_PARTITION",
        "SELECTION_SCOPE_INVALID"
    );
    let retrieved_at: DateTime<Utc> = serde_json::from_value(
        manifest
            .get("retrieved_at")
            .context("SELECTION_CLOCK_REQUIRED")?
            .clone(),
    )?;
    ensure!(retrieved_at <= Utc::now(), "SELECTION_FUTURE_CLOCK");
    let budget = integer(plan, "max_bytes")?;
    ensure!(
        budget > 0 && budget <= i64::MAX as u64,
        "SELECTION_BYTE_BUDGET_INVALID"
    );
    let request = plan.get("request").context("SELECTION_REQUEST_INVALID")?;
    let includes = strings(request, "includes")?;
    for pattern in &includes {
        checked_path(pattern)?;
    }
    let markets = strings(request, "markets")?;
    ensure!(
        markets.iter().all(|m| !m.trim().is_empty()),
        "SELECTION_REQUEST_INVALID"
    );
    let dates = match (request.get("start_date"), request.get("end_date")) {
        (Some(Value::Null), Some(Value::Null)) => None,
        (Some(Value::String(a)), Some(Value::String(b))) => {
            let (a, b) = (date(a)?, date(b)?);
            ensure!(
                a < b && start >= a && end <= b,
                "SELECTION_WINDOW_OUTSIDE_REQUEST"
            );
            Some((a, b))
        }
        _ => anyhow::bail!("SELECTION_REQUEST_INVALID"),
    };
    ensure!(
        !includes.is_empty() || !markets.is_empty() || dates.is_some(),
        "SELECTION_EXPLICIT_REQUEST_REQUIRED"
    );
    if let Some(index) = plan.get("partition_index").filter(|v| !v.is_null()) {
        let path = string(index, "path")?;
        checked_path(path)?;
        ensure!(
            integer(index, "size")? <= 8 * 1024 * 1024
                && string(index, "url")? == source_url(repository, revision, path),
            "SELECTION_INDEX_INVALID"
        );
    } else {
        ensure!(
            markets.is_empty() && dates.is_none(),
            "SELECTION_INDEX_REQUIRED"
        );
    }
    let cache_root = PathBuf::from(string(&manifest, "cache_root")?);
    ensure!(
        cache_root.is_absolute()
            && cache_root.components().all(|c| matches!(
                c,
                Component::Prefix(_) | Component::RootDir | Component::Normal(_)
            )),
        "SELECTION_CACHE_ROOT_INVALID"
    );
    no_symlinks(&cache_root)?;
    ensure!(
        fs::metadata(&cache_root)?.is_dir(),
        "SELECTION_CACHE_ROOT_INVALID"
    );
    let files_root = cache_root.join("files");
    let planned = plan
        .get("files")
        .and_then(Value::as_array)
        .context("SELECTION_FILES_REQUIRED")?;
    let recorded = manifest
        .get("files")
        .and_then(Value::as_array)
        .context("SELECTION_FILES_REQUIRED")?;
    ensure!(
        !planned.is_empty() && planned.len() <= 100_000 && planned.len() == recorded.len(),
        "SELECTION_FILE_LIMIT"
    );
    let mut seen = BTreeSet::new();
    let mut total = 0_u64;
    let mut downloaded = 0_u64;
    let mut cached_count = 0_u64;
    let mut files = Vec::new();
    let mut file_states = BTreeMap::new();
    for (item, record) in planned.iter().zip(recorded) {
        let source_path = string(item, "path")?;
        checked_path(source_path)?;
        ensure!(seen.insert(source_path), "SELECTION_DUPLICATE_FILE");
        ensure!(
            string(item, "url")? == source_url(repository, revision, source_path),
            "SELECTION_SOURCE_URL_INVALID"
        );
        ensure!(
            string(item, "format")? == "parquet",
            "SELECTION_NATIVE_PARQUET_REQUIRED"
        );
        let size = integer(item, "size")?;
        total = total
            .checked_add(size)
            .context("SELECTION_BYTE_BUDGET_INVALID")?;
        ensure!(
            size <= budget && total <= budget,
            "SELECTION_BYTE_BUDGET_INVALID"
        );
        if let Some(partition) = item.get("partition") {
            let source_markets = strings(partition, "markets")?;
            let unique = source_markets.iter().copied().collect::<BTreeSet<_>>();
            let valid_mapping = match partition.get("market_mapping") {
                None => !source_markets.is_empty(),
                Some(Value::String(value)) if value == "UNKNOWN" => {
                    source_markets.is_empty() && markets.is_empty()
                }
                _ => false,
            };
            let a = date(string(partition, "start_date")?)?;
            let b = date(string(partition, "end_date")?)?;
            ensure!(
                valid_mapping
                    && unique.len() == source_markets.len()
                    && source_markets.iter().all(|m| !m.trim().is_empty())
                    && a < b
                    && string(partition, "format")? == "parquet"
                    && (markets.is_empty() || markets.iter().any(|m| unique.contains(m)))
                    && dates.is_none_or(|(start, end)| b > start && a < end),
                "SELECTION_PARTITION_INVALID"
            );
        } else {
            ensure!(
                markets.is_empty() && dates.is_none(),
                "SELECTION_PARTITION_REQUIRED"
            );
        }
        let mut original = record
            .as_object()
            .context("SELECTION_RECORD_INVALID")?
            .clone();
        for key in ["local_path", "cached", "resumed_bytes", "validation"] {
            original.remove(key);
        }
        ensure!(
            Value::Object(original) == *item,
            "SELECTION_RECORD_DIFFERS_FROM_PLAN"
        );
        let target = files_root.join(source_path);
        ensure!(
            string(record, "local_path")? == target.to_str().context("SELECTION_PATH_INVALID")?,
            "SELECTION_LOCAL_PATH_INVALID"
        );
        let cached = record
            .get("cached")
            .and_then(Value::as_bool)
            .context("SELECTION_RECORD_INVALID")?;
        let resumed = integer(record, "resumed_bytes")?;
        ensure!(
            resumed <= size && string(record, "validation")? == "PARQUET_ENVELOPE",
            "SELECTION_RECORD_INVALID"
        );
        if cached {
            cached_count += 1;
        } else {
            downloaded = downloaded
                .checked_add(size - resumed)
                .context("SELECTION_BYTE_BUDGET_INVALID")?;
        }
        let state = parquet(&target, size)?;
        file_states.insert(target.clone(), state);
        files.push(target);
    }
    ensure!(
        total == integer(plan, "total_bytes")?
            && downloaded == integer(&manifest, "downloaded_bytes")?
            && cached_count == integer(&manifest, "cached_files")?,
        "SELECTION_TOTALS_INVALID"
    );
    Ok(Selection {
        repository: repository.to_owned(),
        revision: revision.to_owned(),
        retrieved_at,
        manifest,
        files_root,
        files,
        file_states,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn handoff(directory: &Path, partition: Value, markets: Value) -> PathBuf {
        // Envelope-only handoff fixture, not source data or native conversion.
        let root = directory.canonicalize().unwrap();
        let cached = root.join("cache/files/data.parquet");
        fs::create_dir_all(cached.parent().unwrap()).unwrap();
        fs::write(&cached, b"PAR1xxxxPAR1").unwrap();
        let revision = "a".repeat(40);
        let item = json!({
            "path": "data.parquet", "size": 12, "format": "parquet",
            "url": source_url("fixture/history", &revision, "data.parquet"),
            "partition": partition
        });
        let mut record = item.clone();
        record["local_path"] = json!(cached);
        record["cached"] = true.into();
        record["resumed_bytes"] = 0.into();
        record["validation"] = "PARQUET_ENVELOPE".into();
        let manifest = json!({
            "schema": "qz.hf_selection/1", "cache_root": root.join("cache"),
            "retrieved_at": "2026-01-01T00:00:00Z", "downloaded_bytes": 0,
            "cached_files": 1, "files": [record],
            "plan": {
                "schema": "qz.hf_dataset_plan/1", "repository": "fixture/history",
                "revision": revision, "requested_revision": "main",
                "coverage": "NOT_ASSERTED", "selection_bounds": "[start_date,end_date)",
                "download_granularity": "FILE_PARTITION", "max_bytes": 12, "total_bytes": 12,
                "partition_index": {"path": "partitions.json", "size": 200,
                    "url": source_url("fixture/history", &revision, "partitions.json")},
                "request": {"includes": [], "markets": markets,
                    "start_date": "1970-01-01", "end_date": "1970-01-02"},
                "files": [item]
            }
        });
        let path = root.join("selection.json");
        fs::write(&path, serde_json::to_vec(&manifest).unwrap()).unwrap();
        path
    }

    fn partition() -> Value {
        json!({"markets": [], "market_mapping": "UNKNOWN", "format": "parquet",
               "start_date": "1970-01-01", "end_date": "1970-01-02"})
    }

    #[test]
    fn unknown_mapping_allows_date_and_explicit_file_handoffs() {
        for dates in [true, false] {
            let directory = tempfile::tempdir().unwrap();
            let path = handoff(directory.path(), partition(), json!([]));
            if !dates {
                let mut value: Value = super::super::read_json(&path).unwrap();
                value["plan"]["request"] = json!({"includes": ["data.parquet"], "markets": [],
                    "start_date": null, "end_date": null});
                fs::write(&path, serde_json::to_vec(&value).unwrap()).unwrap();
            }
            let selection = load(&path, 0, 60).unwrap();
            assert_eq!(selection.files.len(), 1);
            assert_eq!(
                selection.manifest["files"][0]["partition"]["market_mapping"],
                "UNKNOWN"
            );
        }
    }

    #[test]
    fn unknown_mapping_cannot_satisfy_market_filters_or_pseudo_all() {
        for market in ["market-a", "ALL", "*", "UNKNOWN"] {
            let directory = tempfile::tempdir().unwrap();
            let path = handoff(directory.path(), partition(), json!([market]));
            assert!(load(&path, 0, 60)
                .err()
                .unwrap()
                .to_string()
                .contains("SELECTION_PARTITION_INVALID"));
        }
    }

    #[test]
    fn unknown_marker_is_explicit_and_legacy_known_mapping_stays_valid() {
        for mapping in [json!(null), json!("unknown"), json!("ALL")] {
            let directory = tempfile::tempdir().unwrap();
            let mut item = partition();
            item["market_mapping"] = mapping;
            let path = handoff(directory.path(), item, json!([]));
            assert!(load(&path, 0, 60).is_err());
        }
        let directory = tempfile::tempdir().unwrap();
        let mut item = partition();
        item["markets"] = json!(["ALL"]);
        let path = handoff(directory.path(), item.clone(), json!([]));
        assert!(load(&path, 0, 60).is_err());
        item.as_object_mut().unwrap().remove("market_mapping");
        item["markets"] = json!([]);
        let path = handoff(directory.path(), item.clone(), json!([]));
        assert!(load(&path, 0, 60).is_err());
        item["markets"] = json!(["market-a"]);
        let path = handoff(directory.path(), item, json!(["market-a"]));
        assert!(load(&path, 0, 60).is_ok());
    }
}
