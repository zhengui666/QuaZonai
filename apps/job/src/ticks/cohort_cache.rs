//! Derived exact-cohort marks, with bounded range reads; never a native TradeTick catalog.
use super::{Cohort, NativeTradeTickCoverageV1, NativeTradeTickFactorEvaluationRequestV1};
use anyhow::{ensure, Context, Result};
use contracts::{ticks::*, DbCounter, SchemaV1};
use nautilus_model::{data::TradeTick, identifiers::TradeId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs::{self, File, OpenOptions},
    io::{BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

pub const REPRESENTATION: &str = "EVENT_TIME_COHORT_CACHE_V1";
pub const FLOW_REPRESENTATION: &str = "EVENT_TIME_COHORT_FLOW_CACHE_V1";
pub const FLOW_FRAME_BYTES: u64 = 40;
pub const MANIFEST_FILE: &str = "cohort-cache.json";
pub const FRAME_BYTES: u64 = 32;
const WRITE_BATCH: usize = 4096 * FRAME_BYTES as usize;
const BUFFERED_BYTES: usize = 8 * 1024 * 1024;

fn counter(value: u64) -> Result<DbCounter> {
    DbCounter::new(value).map_err(anyhow::Error::msg)
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CacheFile {
    pub instrument_id: String,
    pub path: String,
    pub bytes: DbCounter,
    pub byte_start: DbCounter,
    pub byte_end: DbCounter,
    pub trade_count: DbCounter,
    pub cohort_count: DbCounter,
    pub first_event_ns: Option<DbCounter>,
    pub last_event_ns: Option<DbCounter>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub schema_version: SchemaV1,
    pub representation: String,
    pub source_revision: String,
    pub availability: String,
    pub event_start_ns: DbCounter,
    pub event_end_ns: DbCounter,
    pub trade_count: DbCounter,
    pub cohort_count: DbCounter,
    pub files: Vec<CacheFile>,
    pub source_report: Value,
}

pub fn exact_json(value: &mut Value) {
    match value {
        Value::Number(number) if number.as_u64().is_some_and(|n| n > 9_007_199_254_740_991) => {
            *value = Value::String(number.to_string());
        }
        Value::Array(values) => values.iter_mut().for_each(exact_json),
        Value::Object(values) => values.values_mut().for_each(exact_json),
        _ => {}
    }
}

fn publish(path: &Path, value: &impl Serialize) -> Result<()> {
    let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
    serde_json::to_writer(&mut file, value)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    Ok(())
}

fn cache_path(root: &Path, name: &str) -> Result<PathBuf> {
    let relative = Path::new(name);
    ensure!(
        !relative.as_os_str().is_empty()
            && relative
                .components()
                .all(|part| matches!(part, Component::Normal(_)))
            && !name.contains('\\'),
        "TICK_CACHE_PATH_INVALID"
    );
    let path = root.join(relative);
    for ancestor in path.ancestors().take_while(|path| path.starts_with(root)) {
        ensure!(
            !fs::symlink_metadata(ancestor)?.file_type().is_symlink(),
            "TICK_CACHE_SYMLINK"
        );
    }
    Ok(path)
}

fn frame_bytes(representation: &str) -> Result<u64> {
    match representation {
        REPRESENTATION => Ok(FRAME_BYTES),
        FLOW_REPRESENTATION => Ok(FLOW_FRAME_BYTES),
        _ => anyhow::bail!("TICK_CACHE_REPRESENTATION_UNSUPPORTED"),
    }
}

fn frame(cohort: &Cohort, flow: bool) -> Result<[u8; FLOW_FRAME_BYTES as usize]> {
    let mut encoded = [0; FLOW_FRAME_BYTES as usize];
    for (slot, value) in [
        cohort.event,
        cohort.vwap.to_bits(),
        cohort.quantity.to_bits(),
        u64::try_from(cohort.count)?,
    ]
    .into_iter()
    .enumerate()
    {
        encoded[slot * 8..slot * 8 + 8].copy_from_slice(&value.to_le_bytes());
    }
    if flow {
        encoded[32..40].copy_from_slice(
            &cohort
                .signed_quantity
                .context("TICK_FLOW_REQUIRED")?
                .to_bits()
                .to_le_bytes(),
        );
    }
    Ok(encoded)
}

fn decode(bytes: &[u8]) -> Result<Cohort> {
    let value = |slot: usize| u64::from_le_bytes(bytes[slot * 8..slot * 8 + 8].try_into().unwrap());
    Ok(Cohort {
        event: value(0),
        vwap: f64::from_bits(value(1)),
        quantity: f64::from_bits(value(2)),
        count: usize::try_from(value(3))?,
        signed_quantity: (bytes.len() == FLOW_FRAME_BYTES as usize)
            .then(|| f64::from_bits(value(4))),
    })
}

struct Output {
    flow: bool,
    path: PathBuf,
    expected: Vec<u8>,
    verified: u64,
    record: CacheFile,
    pending: Vec<TradeTick>,
    identities: BTreeMap<TradeId, TradeTick>,
}

impl Output {
    fn verify(&mut self) -> Result<()> {
        if !self.expected.is_empty() {
            let mut file = OpenOptions::new()
                .read(true)
                .append(true)
                .open(&self.path)?;
            ensure!(
                file.metadata()?.len() == self.verified,
                "TICK_CACHE_READBACK_MISMATCH"
            );
            file.write_all(&self.expected)?;
            file.seek(SeekFrom::Start(self.verified))?;
            let mut actual = vec![0; self.expected.len()];
            file.read_exact(&mut actual)?;
            ensure!(actual == self.expected, "TICK_CACHE_READBACK_MISMATCH");
            self.verified += u64::try_from(actual.len())?;
            self.expected = Vec::new();
        }
        Ok(())
    }

    fn flush_cohort(&mut self, maximum: u32) -> Result<usize> {
        if self.pending.is_empty() {
            return Ok(0);
        }
        let cohort = super::cohorts_with_flow(&self.pending, maximum, self.flow)?
            .pop()
            .context("TICK_CACHE_EMPTY_COHORT")?;
        let encoded = frame(&cohort, self.flow)?;
        let width = if self.flow {
            FLOW_FRAME_BYTES
        } else {
            FRAME_BYTES
        };
        self.expected.extend_from_slice(&encoded[..width as usize]);
        self.record.cohort_count = counter(self.record.cohort_count.get() + 1)?;
        self.record.trade_count =
            counter(self.record.trade_count.get() + u64::try_from(cohort.count)?)?;
        self.record
            .first_event_ns
            .get_or_insert(counter(cohort.event)?);
        self.record.last_event_ns = Some(counter(cohort.event)?);
        self.record.bytes = counter(self.record.bytes.get() + width)?;
        self.record.byte_end = self.record.bytes;
        let count = self.pending.len();
        self.pending = Vec::new();
        self.identities.clear();
        if self.expected.len() >= WRITE_BATCH {
            self.verify()?;
        }
        Ok(count)
    }
}

/// Source conversion retains only unfinished cohorts and bounded buffered readback.
pub struct Builder {
    flow: bool,
    root: PathBuf,
    outputs: BTreeMap<String, Output>,
    pending_rows: usize,
    buffered_bytes: usize,
    maximum_working_rows: usize,
    maximum_cohort_rows: u32,
    maximum_output_bytes: u64,
    written_bytes: u64,
    sealed: bool,
}

impl Builder {
    pub fn new(
        output: &Path,
        maximum_cohort_rows: u32,
        maximum_working_rows: usize,
        maximum_output_bytes: u64,
    ) -> Result<Self> {
        Self::create(
            output,
            maximum_cohort_rows,
            maximum_working_rows,
            maximum_output_bytes,
            false,
        )
    }

    pub fn new_flow(
        output: &Path,
        maximum_cohort_rows: u32,
        maximum_working_rows: usize,
        maximum_output_bytes: u64,
    ) -> Result<Self> {
        Self::create(
            output,
            maximum_cohort_rows,
            maximum_working_rows,
            maximum_output_bytes,
            true,
        )
    }

    fn create(
        output: &Path,
        maximum_cohort_rows: u32,
        maximum_working_rows: usize,
        maximum_output_bytes: u64,
        flow: bool,
    ) -> Result<Self> {
        ensure!(
            maximum_cohort_rows > 0
                && maximum_cohort_rows <= domain::ticks::MAXIMUM_COHORT_ROWS
                && maximum_working_rows >= maximum_cohort_rows as usize
                && maximum_output_bytes > 0,
            "TICK_CACHE_RESOURCE_LIMIT"
        );
        fs::create_dir(output).context("OUTPUT_MUST_BE_NEW")?;
        let root = output.join("catalog");
        fs::create_dir_all(root.join("data/cohorts"))?;
        Ok(Self {
            flow,
            root,
            outputs: BTreeMap::new(),
            pending_rows: 0,
            buffered_bytes: 0,
            maximum_working_rows,
            maximum_cohort_rows,
            maximum_output_bytes,
            written_bytes: 0,
            sealed: false,
        })
    }

    pub fn push(&mut self, trade: TradeTick) -> Result<()> {
        ensure!(
            !self.sealed && trade.ts_event == trade.ts_init,
            "TICK_CACHE_SOURCE_CLOCK_OR_STATE"
        );
        let id = trade.instrument_id.to_string();
        if !self.outputs.contains_key(&id) {
            let path = format!(
                "data/cohorts/{}.bin",
                nautilus_persistence::backend::catalog::urisafe_instrument_id(&id)
            );
            let file_path = self.root.join(&path);
            OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&file_path)?;
            self.outputs.insert(
                id.clone(),
                Output {
                    flow: self.flow,
                    path: file_path,
                    expected: Vec::new(),
                    verified: 0,
                    pending: Vec::new(),
                    identities: BTreeMap::new(),
                    record: CacheFile {
                        instrument_id: id.clone(),
                        path,
                        bytes: counter(0)?,
                        byte_start: counter(0)?,
                        byte_end: counter(0)?,
                        trade_count: counter(0)?,
                        cohort_count: counter(0)?,
                        first_event_ns: None,
                        last_event_ns: None,
                    },
                },
            );
        }
        let output = self.outputs.get_mut(&id).unwrap();
        if let Some(previous) = output.pending.first() {
            ensure!(
                trade.ts_event >= previous.ts_event,
                "TICK_CACHE_SOURCE_TIME_REVERSAL"
            );
            if trade.ts_event != previous.ts_event {
                let buffered = output.expected.len();
                let flushed = output.flush_cohort(self.maximum_cohort_rows)?;
                self.buffered_bytes = self.buffered_bytes - buffered + output.expected.len();
                self.pending_rows -= flushed;
                self.written_bytes += if self.flow {
                    FLOW_FRAME_BYTES
                } else {
                    FRAME_BYTES
                };
                ensure!(
                    self.written_bytes <= self.maximum_output_bytes,
                    "TICK_CACHE_OUTPUT_LIMIT"
                );
            }
        }
        if let Some(previous) = output.identities.insert(trade.trade_id, trade) {
            ensure!(previous == trade, "TICK_CACHE_SOURCE_ID_CONFLICT");
            anyhow::bail!("TICK_CACHE_SOURCE_ID_DUPLICATE");
        }
        ensure!(
            output.pending.len() < self.maximum_cohort_rows as usize,
            "TICK_COHORT_ROW_LIMIT"
        );
        ensure!(
            self.pending_rows < self.maximum_working_rows,
            "TICK_CACHE_WORKING_ROW_LIMIT"
        );
        output.pending.push(trade);
        self.pending_rows += 1;
        if self.buffered_bytes >= BUFFERED_BYTES {
            for output in self.outputs.values_mut() {
                output.verify()?;
            }
            self.buffered_bytes = 0;
        }
        Ok(())
    }

    pub fn seal(&mut self, source_revision: &str, start: u64, end: u64) -> Result<Manifest> {
        ensure!(!self.sealed, "TICK_CACHE_ALREADY_SEALED");
        for output in self.outputs.values_mut() {
            if output.flush_cohort(self.maximum_cohort_rows)? > 0 {
                self.written_bytes += if self.flow {
                    FLOW_FRAME_BYTES
                } else {
                    FRAME_BYTES
                };
            }
            ensure!(
                self.written_bytes <= self.maximum_output_bytes,
                "TICK_CACHE_OUTPUT_LIMIT"
            );
            output.verify()?;
            File::open(&output.path)?.sync_all()?;
        }
        self.sealed = true;
        let files = self
            .outputs
            .values()
            .map(|output| output.record.clone())
            .collect::<Vec<_>>();
        ensure!(!files.is_empty(), "TICK_CACHE_EMPTY_SELECTION");
        Ok(Manifest {
            schema_version: SchemaV1,
            representation: if self.flow {
                FLOW_REPRESENTATION
            } else {
                REPRESENTATION
            }
            .into(),
            source_revision: source_revision.into(),
            availability: "EVENT_TIME_PROXY_UNVERIFIED".into(),
            event_start_ns: counter(start)?,
            event_end_ns: counter(end)?,
            trade_count: counter(files.iter().map(|file| file.trade_count.get()).sum())?,
            cohort_count: counter(files.iter().map(|file| file.cohort_count.get()).sum())?,
            files,
            source_report: Value::Null,
        })
    }

    pub fn publish(self, mut manifest: Manifest, mut source_report: Value) -> Result<()> {
        ensure!(self.sealed, "TICK_CACHE_NOT_SEALED");
        exact_json(&mut source_report);
        manifest.source_report = source_report;
        let report = self.root.parent().unwrap().join("import-report.json");
        let report_bytes = match fs::metadata(report) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        let bytes = self
            .written_bytes
            .checked_add(u64::try_from(serde_json::to_vec(&manifest)?.len())? + 1)
            .and_then(|bytes| bytes.checked_add(report_bytes))
            .context("TICK_CACHE_OUTPUT_LIMIT")?;
        ensure!(
            bytes <= self.maximum_output_bytes,
            "TICK_CACHE_OUTPUT_LIMIT"
        );
        publish(&self.root.join(MANIFEST_FILE), &manifest)
    }
}

pub(super) struct CacheAsset {
    pub instrument_id: String,
    pub cohort_count: u64,
    file: CacheFile,
}

pub(super) struct CacheInput {
    frame_bytes: u64,
    pub coverage: NativeTradeTickCoverageV1,
    pub assets: Vec<CacheAsset>,
    root: PathBuf,
    start: u64,
    end: u64,
    maximum_cohort_rows: u32,
}

pub(super) struct CacheReader {
    frame_bytes: u64,
    input: BufReader<std::io::Take<File>>,
    file: CacheFile,
    start: u64,
    end: u64,
    maximum_cohort_rows: u32,
    seen: u64,
    trades: u64,
    first: Option<u64>,
    last: Option<u64>,
    finished: bool,
}

impl Iterator for CacheReader {
    type Item = Result<Cohort>;
    fn next(&mut self) -> Option<Self::Item> {
        if self.finished {
            return None;
        }
        let result = (|| -> Result<Option<Cohort>> {
            if self.seen == self.file.cohort_count.get() {
                ensure!(
                    self.trades == self.file.trade_count.get()
                        && self.first == self.file.first_event_ns.map(|value| value.get())
                        && self.last == self.file.last_event_ns.map(|value| value.get()),
                    "TICK_CACHE_COUNTS_MISMATCH"
                );
                return Ok(None);
            }
            let mut encoded = [0; FLOW_FRAME_BYTES as usize];
            self.input
                .read_exact(&mut encoded[..self.frame_bytes as usize])
                .context("TICK_CACHE_FRAME_TRUNCATED")?;
            let cohort = decode(&encoded[..self.frame_bytes as usize])?;
            ensure!(
                cohort.event >= self.start
                    && cohort.event < self.end
                    && self.last.is_none_or(|last| last < cohort.event),
                "TICK_CACHE_EVENT_ORDER_OR_SCOPE"
            );
            ensure!(
                cohort.vwap.is_finite()
                    && cohort.vwap > 0.0
                    && cohort.quantity.is_finite()
                    && cohort.quantity > 0.0
                    && cohort.count > 0
                    && cohort.count <= self.maximum_cohort_rows as usize,
                "TICK_CACHE_FRAME_INVALID"
            );
            ensure!(
                cohort
                    .signed_quantity
                    .is_none_or(|flow| flow.is_finite() && flow.abs() <= cohort.quantity),
                "TICK_FLOW_INVALID"
            );
            self.seen += 1;
            self.trades = self
                .trades
                .checked_add(u64::try_from(cohort.count)?)
                .context("TICK_CACHE_COUNT_RANGE")?;
            self.first.get_or_insert(cohort.event);
            self.last = Some(cohort.event);
            Ok(Some(cohort))
        })();
        match result {
            Ok(Some(cohort)) => Some(Ok(cohort)),
            Ok(None) => {
                self.finished = true;
                None
            }
            Err(error) => {
                self.finished = true;
                Some(Err(error))
            }
        }
    }
}

impl CacheInput {
    pub fn cohorts(&self, asset: &CacheAsset) -> Result<CacheReader> {
        let mut file = File::open(cache_path(&self.root, &asset.file.path)?)?;
        file.seek(SeekFrom::Start(asset.file.byte_start.get()))?;
        Ok(CacheReader {
            frame_bytes: self.frame_bytes,
            input: BufReader::new(
                file.take(asset.file.byte_end.get() - asset.file.byte_start.get()),
            ),
            file: asset.file.clone(),
            start: self.start,
            end: self.end,
            maximum_cohort_rows: self.maximum_cohort_rows,
            seen: 0,
            trades: 0,
            first: None,
            last: None,
            finished: false,
        })
    }
}

fn number(value: &Value, field: &str) -> Result<u64> {
    let value = value
        .get(field)
        .with_context(|| format!("TICK_CACHE_SOURCE_FIELD: {field}"))?;
    value
        .as_u64()
        .or_else(|| value.as_str().and_then(|text| text.parse().ok()))
        .with_context(|| format!("TICK_CACHE_SOURCE_FIELD: {field}"))
}

pub fn manifest(root: &Path) -> Result<Manifest> {
    let file = File::open(cache_path(root, MANIFEST_FILE)?)?;
    ensure!(
        file.metadata()?.len() <= 16 * 1024 * 1024,
        "TICK_CACHE_MANIFEST_LIMIT"
    );
    serde_json::from_reader(file).context("TICK_CACHE_MANIFEST_INVALID")
}

pub(super) fn load(
    root: &Path,
    request: &NativeTradeTickFactorEvaluationRequestV1,
) -> Result<CacheInput> {
    domain::ticks::evaluation_request(request)?;
    ensure!(
        request.input.is_cohort_cache(),
        "TICK_CACHE_REPRESENTATION_REQUIRED"
    );
    let root = crate::catalog::catalog_root(root)?;
    let manifest = manifest(&root)?;
    let frame_bytes = frame_bytes(&manifest.representation)?;
    ensure!(
        Some(manifest.representation.as_str()) == request.input.cache_representation()
            && manifest.source_revision == request.input.source_revision
            && manifest.availability == "EVENT_TIME_PROXY_UNVERIFIED"
            && manifest.event_start_ns == request.input.event_start_ns
            && manifest.event_end_ns == request.input.event_end_ns,
        "TICK_CACHE_VIEW_MISMATCH"
    );
    let report = &manifest.source_report;
    ensure!(
        report["status"] == "IMPORTED"
            && report["availability"] == "EVENT_TIME_PROXY_UNVERIFIED"
            && report["source_revision"] == manifest.source_revision,
        "TICK_CACHE_SOURCE_INCOMPLETE"
    );
    if let Some(revision) = report.get("revision") {
        ensure!(
            revision.as_str() == Some(&manifest.source_revision),
            "TICK_CACHE_SOURCE_REVISION_CONFLICT"
        );
    }
    for key in ["missing_symbols", "rejected_rows"] {
        ensure!(
            report[key]
                .as_object()
                .is_some_and(|values| values.is_empty()),
            "TICK_CACHE_SOURCE_INCOMPLETE"
        );
    }
    ensure!(
        report["missing_partitions"]
            .as_array()
            .is_some_and(|values| values.is_empty())
            && number(report, "duplicate_trades")? == 0
            && manifest.event_start_ns.get() >= number(report, "event_start_ns")?
            && manifest.event_end_ns.get() <= number(report, "event_end_ns")?,
        "TICK_CACHE_SOURCE_INCOMPLETE"
    );
    let wanted = request
        .input
        .instrument_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    let mut ids = BTreeSet::new();
    let mut paths = BTreeSet::new();
    let mut assets = Vec::new();
    let mut total_trades = 0_u64;
    let mut total_cohorts = 0_u64;
    for file in &manifest.files {
        ensure!(
            ids.insert(file.instrument_id.clone()) && paths.insert(file.path.clone()),
            "TICK_CACHE_IDENTITY_DUPLICATE"
        );
        ensure!(
            file.bytes.get().is_multiple_of(frame_bytes)
                && file.byte_start.get().is_multiple_of(frame_bytes)
                && file.byte_end.get().is_multiple_of(frame_bytes)
                && file.byte_start <= file.byte_end
                && file.byte_end <= file.bytes
                && (file.byte_end.get() - file.byte_start.get()) / frame_bytes
                    == file.cohort_count.get(),
            "TICK_CACHE_RANGE_INVALID"
        );
        let path = cache_path(&root, &file.path)?;
        let state = fs::metadata(&path)?;
        ensure!(
            state.is_file() && state.len() == file.bytes.get(),
            "TICK_CACHE_FILE_INCOMPLETE"
        );
        let mut input = File::open(path)?;
        for (position, before) in [
            (file.byte_start.get().checked_sub(frame_bytes), true),
            (
                (file.byte_end < file.bytes).then_some(file.byte_end.get()),
                false,
            ),
        ] {
            if let Some(position) = position {
                input.seek(SeekFrom::Start(position))?;
                let mut event = [0; 8];
                input.read_exact(&mut event)?;
                let event = u64::from_le_bytes(event);
                ensure!(
                    if before {
                        event < manifest.event_start_ns.get()
                    } else {
                        event >= manifest.event_end_ns.get()
                    },
                    "TICK_CACHE_VIEW_OMITS_IN_RANGE_COHORTS"
                );
            }
        }
        ensure!(
            (file.cohort_count.get() == 0
                && file.trade_count.get() == 0
                && file.first_event_ns.is_none()
                && file.last_event_ns.is_none())
                || (file.cohort_count.get() > 0
                    && file.trade_count.get() >= file.cohort_count.get()
                    && file
                        .first_event_ns
                        .is_some_and(|event| event >= manifest.event_start_ns
                            && event < manifest.event_end_ns)
                    && file
                        .last_event_ns
                        .is_some_and(|event| event >= file.first_event_ns.unwrap()
                            && event < manifest.event_end_ns)),
            "TICK_CACHE_COVERAGE_INVALID"
        );
        total_trades = total_trades
            .checked_add(file.trade_count.get())
            .context("TICK_CACHE_COUNT_RANGE")?;
        total_cohorts = total_cohorts
            .checked_add(file.cohort_count.get())
            .context("TICK_CACHE_COUNT_RANGE")?;
        if wanted.contains(&file.instrument_id) {
            assets.push(CacheAsset {
                instrument_id: file.instrument_id.clone(),
                cohort_count: file.cohort_count.get(),
                file: file.clone(),
            });
        }
    }
    ensure!(
        total_trades == manifest.trade_count.get()
            && total_cohorts == manifest.cohort_count.get()
            && assets.len() == wanted.len(),
        "TICK_CACHE_COVERAGE_INVALID"
    );
    assets.sort_by(|left, right| left.instrument_id.cmp(&right.instrument_id));
    let selected_trades = assets
        .iter()
        .map(|asset| asset.file.trade_count.get())
        .sum::<u64>();
    let selected_cohorts = assets.iter().map(|asset| asset.cohort_count).sum::<u64>();
    ensure!(
        selected_trades > 0
            && selected_trades
                <= request
                    .input
                    .source_row_limit()
                    .context("TICK_CACHE_BUDGET_REQUIRED")?,
        "TICK_CACHE_SOURCE_ROW_LIMIT"
    );
    let instrument_coverage = request
        .input
        .instrument_ids
        .iter()
        .map(|id| {
            let file = &assets
                .iter()
                .find(|asset| asset.instrument_id == *id)
                .unwrap()
                .file;
            NativeTradeTickInstrumentCoverageV1 {
                instrument_id: id.clone(),
                trade_count: file.trade_count,
                cohort_count: file.cohort_count,
                first_event_ns: file.first_event_ns,
                last_event_ns: file.last_event_ns,
            }
        })
        .collect();
    let first = assets
        .iter()
        .filter_map(|asset| asset.file.first_event_ns)
        .min()
        .context("TICK_CACHE_EMPTY_SELECTION")?;
    let last = assets
        .iter()
        .filter_map(|asset| asset.file.last_event_ns)
        .max()
        .context("TICK_CACHE_EMPTY_SELECTION")?;
    let coverage = NativeTradeTickCoverageV1 {
        modality: NativeTickModalityV1::TradeTick,
        source_revision: manifest.source_revision,
        instrument_ids: request.input.instrument_ids.clone(),
        instrument_coverage,
        trade_count: counter(selected_trades)?,
        cohort_count: counter(selected_cohorts)?,
        first_event_ns: first,
        last_event_ns: last,
        availability: NativeTickAvailabilityV1::EventTimeProxyUnverified,
        limitations: vec![
            "UNQUALIFIED_EXPLORATORY".into(),
            "EVENT_TIME_PROXY_UNVERIFIED".into(),
            "DERIVED_COHORT_CACHE_NOT_NATIVE_TRADE_TICKS".into(),
            "VWAP_IS_NOT_EXECUTABLE_PRICE".into(),
            "CURRENT_DEFINITIONS_ARE_REPRESENTATION_METADATA_NOT_HISTORICAL_CONTRACT_OR_FEES"
                .into(),
        ],
        source_evidence: BTreeMap::from([
            ("representation".into(), json!(manifest.representation)),
            ("source_report".into(), manifest.source_report),
        ]),
    };
    Ok(CacheInput {
        frame_bytes,
        coverage,
        assets,
        root,
        start: request.input.event_start_ns.get(),
        end: request.input.event_end_ns.get(),
        maximum_cohort_rows: request.sampling.maximum_cohort_rows,
    })
}

/// Concatenate complete chronological cache units without retaining their reports or frames.
pub fn concatenate(
    units: &[PathBuf],
    output: &Path,
    mut source_report: Value,
    maximum_output_bytes: u64,
) -> Result<Manifest> {
    ensure!(
        !units.is_empty() && maximum_output_bytes > 0,
        "TICK_CACHE_RESOURCE_LIMIT"
    );
    let output = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?
        .join(
            output
                .file_name()
                .context("TICK_OUTPUT_DIRECTORY_REQUIRED")?,
        );
    for unit in units {
        let unit = unit.canonicalize()?;
        ensure!(!output.starts_with(&unit), "OUTPUT_MUST_NOT_OVERLAP_SOURCE");
    }
    fs::create_dir(&output).context("OUTPUT_MUST_BE_NEW")?;
    let root = output.join("catalog");
    fs::create_dir_all(root.join("data/cohorts"))?;
    let mut combined: Option<Manifest> = None;
    let mut outputs: BTreeMap<String, CacheFile> = BTreeMap::new();
    let mut written = 0_u64;
    for unit in units {
        let unit = crate::catalog::catalog_root(unit)?;
        let current = manifest(&unit)?;
        let width = frame_bytes(&current.representation)?;
        ensure!(
            current.event_start_ns < current.event_end_ns
                && current.availability == "EVENT_TIME_PROXY_UNVERIFIED",
            "TICK_CACHE_VIEW_MISMATCH"
        );
        if let Some(previous) = &mut combined {
            ensure!(
                previous.representation == current.representation
                    && previous.source_revision == current.source_revision
                    && previous.availability == current.availability
                    && previous.event_end_ns == current.event_start_ns,
                "TICK_CACHE_UNIT_MISMATCH"
            );
            previous.event_end_ns = current.event_end_ns;
        } else {
            combined = Some(Manifest {
                schema_version: SchemaV1,
                representation: current.representation.clone(),
                source_revision: current.source_revision.clone(),
                availability: current.availability.clone(),
                event_start_ns: current.event_start_ns,
                event_end_ns: current.event_end_ns,
                trade_count: counter(0)?,
                cohort_count: counter(0)?,
                files: Vec::new(),
                source_report: Value::Null,
            });
        }
        let mut ids = BTreeSet::new();
        let mut paths = BTreeSet::new();
        let mut trades = 0_u64;
        let mut cohorts = 0_u64;
        for record in current.files {
            ensure!(
                ids.insert(record.instrument_id.clone()) && paths.insert(record.path.clone()),
                "TICK_CACHE_IDENTITY_DUPLICATE"
            );
            ensure!(
                record.byte_start.get() == 0
                    && record.byte_end == record.bytes
                    && record.bytes.get().is_multiple_of(width)
                    && record.bytes.get() / width == record.cohort_count.get(),
                "TICK_CACHE_FULL_UNIT_REQUIRED"
            );
            let path = cache_path(&unit, &record.path)?;
            let source = File::open(path)?;
            ensure!(
                source.metadata()?.is_file() && source.metadata()?.len() == record.bytes.get(),
                "TICK_CACHE_FILE_INCOMPLETE"
            );
            if !outputs.contains_key(&record.instrument_id) {
                let path = format!(
                    "data/cohorts/{}.bin",
                    nautilus_persistence::backend::catalog::urisafe_instrument_id(
                        &record.instrument_id
                    )
                );
                OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(root.join(&path))?;
                outputs.insert(
                    record.instrument_id.clone(),
                    CacheFile {
                        instrument_id: record.instrument_id.clone(),
                        path,
                        bytes: counter(0)?,
                        byte_start: counter(0)?,
                        byte_end: counter(0)?,
                        trade_count: counter(0)?,
                        cohort_count: counter(0)?,
                        first_event_ns: None,
                        last_event_ns: None,
                    },
                );
            }
            let reader = CacheReader {
                frame_bytes: width,
                input: BufReader::new(source.take(record.bytes.get())),
                file: record.clone(),
                start: current.event_start_ns.get(),
                end: current.event_end_ns.get(),
                maximum_cohort_rows: domain::ticks::MAXIMUM_COHORT_ROWS,
                seen: 0,
                trades: 0,
                first: None,
                last: None,
                finished: false,
            };
            let target = outputs.get_mut(&record.instrument_id).unwrap();
            let mut writer = BufWriter::with_capacity(
                WRITE_BATCH,
                OpenOptions::new()
                    .append(true)
                    .open(root.join(&target.path))?,
            );
            for cohort in reader {
                let cohort = cohort?;
                ensure!(
                    target
                        .last_event_ns
                        .is_none_or(|last| last.get() < cohort.event),
                    "TICK_CACHE_EVENT_ORDER_OR_SCOPE"
                );
                written = written
                    .checked_add(width)
                    .context("TICK_CACHE_OUTPUT_LIMIT")?;
                ensure!(written <= maximum_output_bytes, "TICK_CACHE_OUTPUT_LIMIT");
                writer.write_all(&frame(&cohort, width == FLOW_FRAME_BYTES)?[..width as usize])?;
                target.first_event_ns.get_or_insert(counter(cohort.event)?);
                target.last_event_ns = Some(counter(cohort.event)?);
            }
            writer.flush()?;
            drop(writer);
            target.bytes = counter(
                target
                    .bytes
                    .get()
                    .checked_add(record.bytes.get())
                    .context("TICK_CACHE_COUNT_RANGE")?,
            )?;
            target.byte_end = target.bytes;
            target.trade_count = counter(
                target
                    .trade_count
                    .get()
                    .checked_add(record.trade_count.get())
                    .context("TICK_CACHE_COUNT_RANGE")?,
            )?;
            target.cohort_count = counter(
                target
                    .cohort_count
                    .get()
                    .checked_add(record.cohort_count.get())
                    .context("TICK_CACHE_COUNT_RANGE")?,
            )?;
            trades = trades
                .checked_add(record.trade_count.get())
                .context("TICK_CACHE_COUNT_RANGE")?;
            cohorts = cohorts
                .checked_add(record.cohort_count.get())
                .context("TICK_CACHE_COUNT_RANGE")?;
        }
        ensure!(
            trades == current.trade_count.get() && cohorts == current.cohort_count.get(),
            "TICK_CACHE_COUNTS_MISMATCH"
        );
    }
    let mut result = combined.unwrap();
    for (_, record) in outputs {
        File::open(root.join(&record.path))?.sync_all()?;
        result.trade_count = counter(
            result
                .trade_count
                .get()
                .checked_add(record.trade_count.get())
                .context("TICK_CACHE_COUNT_RANGE")?,
        )?;
        result.cohort_count = counter(
            result
                .cohort_count
                .get()
                .checked_add(record.cohort_count.get())
                .context("TICK_CACHE_COUNT_RANGE")?,
        )?;
        result.files.push(record);
    }
    ensure!(result.cohort_count.get() > 0, "TICK_CACHE_EMPTY_SELECTION");
    source_report["native_files"] = json!(result
        .files
        .iter()
        .map(|file| json!({"path": file.path, "size": file.bytes.get()}))
        .collect::<Vec<_>>());
    source_report["native_records"] = json!({"cohorts": result.files.iter().map(|file|
        (file.instrument_id.clone(), file.cohort_count.get())).collect::<BTreeMap<_, _>>()});
    source_report["readback_records"] = json!(result.cohort_count.get());
    source_report["representation"] = json!(result.representation);
    source_report["event_start_ns"] = json!(result.event_start_ns.get());
    source_report["event_end_ns"] = json!(result.event_end_ns.get());
    result.source_report = source_report.clone();
    exact_json(&mut result.source_report);
    let metadata_bytes = u64::try_from(serde_json::to_vec(&result)?.len())?
        .checked_add(u64::try_from(serde_json::to_vec(&source_report)?.len())?)
        .and_then(|bytes| bytes.checked_add(2))
        .context("TICK_CACHE_OUTPUT_LIMIT")?;
    ensure!(
        written
            .checked_add(metadata_bytes)
            .is_some_and(|bytes| bytes <= maximum_output_bytes),
        "TICK_CACHE_OUTPUT_LIMIT"
    );
    publish(&output.join("import-report.json"), &source_report)?;
    publish(&root.join(MANIFEST_FILE), &result)?;
    Ok(result)
}

pub fn prepare_views(
    source: &Path,
    request: &NativeTradeTickFactorEvaluationRequestV1,
    output: &Path,
    maximum_output_bytes: u64,
) -> Result<Value> {
    let input = load(source, request)?;
    let source = source.canonicalize()?;
    let economic_sources = super::economic::funding_sources(&source, request)?;
    let parent = output
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .canonicalize()?;
    let output = parent.join(
        output
            .file_name()
            .context("TICK_OUTPUT_DIRECTORY_REQUIRED")?,
    );
    ensure!(
        !output.starts_with(&source),
        "OUTPUT_MUST_NOT_OVERLAP_SOURCE"
    );
    fs::create_dir(&output).context("OUTPUT_MUST_BE_NEW")?;
    let original = manifest(&source)?;
    let mut full = original.clone();
    full.files = input
        .assets
        .iter()
        .map(|asset| asset.file.clone())
        .collect();
    full.trade_count = input.coverage.trade_count;
    full.cohort_count = input.coverage.cohort_count;
    let mut primary = full.clone();
    primary.event_end_ns = request.validation_end_ns;
    for (asset, file) in input.assets.iter().zip(&mut primary.files) {
        let mut trades = 0;
        let mut count = 0;
        let mut first = None;
        let mut last = None;
        for cohort in input.cohorts(asset)? {
            let cohort = cohort?;
            if cohort.event < request.validation_end_ns.get() {
                trades += u64::try_from(cohort.count)?;
                count += 1;
                first.get_or_insert(counter(cohort.event)?);
                last = Some(counter(cohort.event)?);
            }
        }
        file.byte_end = counter(file.byte_start.get() + count * input.frame_bytes)?;
        file.trade_count = counter(trades)?;
        file.cohort_count = counter(count)?;
        file.first_event_ns = first;
        file.last_event_ns = last;
    }
    primary.trade_count = counter(
        primary
            .files
            .iter()
            .map(|file| file.trade_count.get())
            .sum(),
    )?;
    primary.cohort_count = counter(
        primary
            .files
            .iter()
            .map(|file| file.cohort_count.get())
            .sum(),
    )?;
    let logical_bytes = full
        .files
        .iter()
        .map(|file| file.bytes.get())
        .sum::<u64>()
        .checked_mul(2)
        .and_then(|bytes| {
            economic_sources.iter().try_fold(bytes, |total, (_, size)| {
                total.checked_add(size.checked_mul(2)?)
            })
        })
        .context("TICK_PREPARATION_OUTPUT_LIMIT")?;
    let mut research_request = request.clone();
    research_request.clip_end(request.validation_end_ns);
    let mut metadata = json!({"schema_version":1,"modality":"TRADE_TICK","representation":full.representation,
        "operational_state":"NATIVE_PREPARED","scientific_qualification":"UNQUALIFIED_EXPLORATORY",
        "catalog_format":format!("QZ_{}", full.representation),"request":request,"research_request":research_request,
        "research_catalog":"research/catalog","independent_catalog":"independent/catalog","coverage":input.coverage,
        "maximum_output_bytes":maximum_output_bytes.to_string(),"output_bytes":"0"});
    if !economic_sources.is_empty() {
        metadata["economic_sources"] = json!(economic_sources
            .iter()
            .map(|(path, bytes)| json!({"path":path,"bytes":bytes.to_string()}))
            .collect::<Vec<_>>());
    }
    let views_bytes =
        u64::try_from(serde_json::to_vec(&full)?.len() + serde_json::to_vec(&primary)?.len() + 2)?;
    loop {
        let metadata_bytes = u64::try_from(serde_json::to_vec(&metadata)?.len())? + 1;
        let bytes = logical_bytes
            .checked_add(views_bytes)
            .and_then(|bytes| bytes.checked_add(metadata_bytes))
            .context("TICK_PREPARATION_OUTPUT_LIMIT")?;
        ensure!(
            bytes <= maximum_output_bytes,
            "TICK_PREPARATION_OUTPUT_LIMIT"
        );
        if metadata["output_bytes"].as_str() == Some(bytes.to_string().as_str()) {
            break;
        }
        metadata["output_bytes"] = json!(bytes.to_string());
    }
    for (directory, manifest) in [
        ("research/catalog", &primary),
        ("independent/catalog", &full),
    ] {
        let root = output.join(directory);
        fs::create_dir_all(&root)?;
        for file in &manifest.files {
            let target = root.join(&file.path);
            fs::create_dir_all(target.parent().unwrap())?;
            fs::hard_link(cache_path(&source, &file.path)?, target)?;
        }
        publish(&root.join(MANIFEST_FILE), manifest)?;
        for (name, _) in &economic_sources {
            fs::hard_link(cache_path(&source, name)?, root.join(name))?;
        }
    }
    load(&output.join("research/catalog"), &research_request)?;
    load(&output.join("independent/catalog"), request)?;
    publish(&output.join("catalog-metadata.json"), &metadata)?;
    Ok(metadata)
}

#[cfg(test)]
mod concatenate_tests {
    use super::*;

    #[test]
    fn concatenate_preserves_both_frame_layouts_and_rejects_partial_units() {
        for flow in [false, true] {
            let temp = tempfile::tempdir().unwrap();
            let mut units = Vec::new();
            let mut expected = Vec::new();
            for index in 0..2_u64 {
                let root = temp.path().join(index.to_string());
                fs::create_dir_all(root.join("data/cohorts")).unwrap();
                let cohort = Cohort {
                    event: index * 10 + 1,
                    vwap: 1.2345678901234567,
                    quantity: 2.0,
                    count: 2,
                    signed_quantity: flow.then_some(-0.0),
                };
                let width = if flow { FLOW_FRAME_BYTES } else { FRAME_BYTES };
                let encoded = frame(&cohort, flow).unwrap();
                let bytes = &encoded[..width as usize];
                expected.extend_from_slice(bytes);
                fs::write(root.join("data/cohorts/input.bin"), bytes).unwrap();
                publish(
                    &root.join(MANIFEST_FILE),
                    &Manifest {
                        schema_version: SchemaV1,
                        representation: if flow {
                            FLOW_REPRESENTATION
                        } else {
                            REPRESENTATION
                        }
                        .into(),
                        source_revision: "a".repeat(40),
                        availability: "EVENT_TIME_PROXY_UNVERIFIED".into(),
                        event_start_ns: counter(index * 10).unwrap(),
                        event_end_ns: counter(index * 10 + 10).unwrap(),
                        trade_count: counter(2).unwrap(),
                        cohort_count: counter(1).unwrap(),
                        files: vec![CacheFile {
                            instrument_id: "BTC.HYPERLIQUID".into(),
                            path: "data/cohorts/input.bin".into(),
                            bytes: counter(width).unwrap(),
                            byte_start: counter(0).unwrap(),
                            byte_end: counter(width).unwrap(),
                            trade_count: counter(2).unwrap(),
                            cohort_count: counter(1).unwrap(),
                            first_event_ns: Some(counter(cohort.event).unwrap()),
                            last_event_ns: Some(counter(cohort.event).unwrap()),
                        }],
                        source_report: Value::Null,
                    },
                )
                .unwrap();
                units.push(root);
            }
            let output = temp.path().join("combined");
            let result =
                concatenate(&units, &output, json!({"event_end_ns": 20}), 100_000).unwrap();
            assert_eq!(result.trade_count.get(), 4);
            assert_eq!(result.cohort_count.get(), 2);
            assert_eq!(
                fs::read(output.join("catalog").join(&result.files[0].path)).unwrap(),
                expected
            );
            let report: Value =
                serde_json::from_reader(File::open(output.join("import-report.json")).unwrap())
                    .unwrap();
            assert_eq!(report["readback_records"], 2);
            assert_eq!(report["native_files"][0]["size"], expected.len());
            let mut partial = manifest(&units[0]).unwrap();
            partial.files[0].byte_end = counter(0).unwrap();
            fs::remove_file(units[0].join(MANIFEST_FILE)).unwrap();
            publish(&units[0].join(MANIFEST_FILE), &partial).unwrap();
            assert!(concatenate(&units, &temp.path().join("partial"), json!({}), 100_000).is_err());
        }
    }
}

#[cfg(test)]
mod resource_tests {
    use super::*;
    use nautilus_model::{
        enums::AggressorSide,
        identifiers::InstrumentId,
        types::{Price, Quantity},
    };

    #[test]
    fn cache_releases_flushed_trade_storage_and_idle_descriptors() {
        let temp = tempfile::tempdir().unwrap();
        let mut builder = Builder::new(&temp.path().join("cache"), 100, 2000, 10_000_000).unwrap();
        for asset in 0..1200 {
            let id = InstrumentId::from(format!("ASSET{asset}.SIM").as_str());
            for (event, count) in [(1_u64, 100), (2, 1)] {
                for number in 0..count {
                    builder
                        .push(TradeTick::new(
                            id,
                            Price::from("10"),
                            Quantity::from("1"),
                            AggressorSide::Buy,
                            TradeId::from(number.to_string().as_str()),
                            event.into(),
                            event.into(),
                        ))
                        .unwrap();
                }
            }
            let output = &builder.outputs[&id.to_string()];
            assert!(
                output.pending.capacity() <= 4,
                "flushed peak allocation retained"
            );
            assert!(builder.buffered_bytes < BUFFERED_BYTES);
        }
        for event in 3_u64..260 {
            for asset in 0..1200 {
                let id = InstrumentId::from(format!("ASSET{asset}.SIM").as_str());
                builder
                    .push(TradeTick::new(
                        id,
                        Price::from("10"),
                        Quantity::from("1"),
                        AggressorSide::Buy,
                        TradeId::from("0"),
                        event.into(),
                        event.into(),
                    ))
                    .unwrap();
                assert!(builder.buffered_bytes < BUFFERED_BYTES);
            }
        }
        assert!(builder
            .outputs
            .values()
            .any(|output| fs::metadata(&output.path).unwrap().len() > 0));
        #[cfg(target_os = "linux")]
        for entry in fs::read_dir("/proc/self/fd").unwrap() {
            if let Ok(path) = fs::read_link(entry.unwrap().path()) {
                assert!(
                    !path.starts_with(temp.path()),
                    "idle cache file remains open"
                );
            }
        }
        let manifest = builder.seal("fixed-source", 0, 260).unwrap();
        assert_eq!(manifest.trade_count.get(), 1200 * (101 + 257));
        assert_eq!(manifest.cohort_count.get(), 1200 * 259);
        assert!(builder
            .outputs
            .values()
            .all(|output| output.pending.capacity() == 0 && output.expected.capacity() == 0));
    }
}
