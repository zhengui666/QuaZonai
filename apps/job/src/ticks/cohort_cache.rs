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
    io::{BufRead, BufReader, BufWriter, Read, Seek, SeekFrom, Write},
    path::{Component, Path, PathBuf},
};

pub const REPRESENTATION: &str = "EVENT_TIME_COHORT_CACHE_V1";
pub const FLOW_REPRESENTATION: &str = "EVENT_TIME_COHORT_FLOW_CACHE_V1";
pub const RECEIVED_REPRESENTATION: &str = "RECEIVED_TIME_COHORT_CACHE_V1";
pub const RECEIVED_FLOW_REPRESENTATION: &str = "RECEIVED_TIME_COHORT_FLOW_CACHE_V1";
pub const FLOW_FRAME_BYTES: u64 = 40;
pub const RECEIVED_FRAME_BYTES: u64 = 48;
pub const RECEIVED_FLOW_FRAME_BYTES: u64 = 56;
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_start_ns: Option<DbCounter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_end_ns: Option<DbCounter>,
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maximum_receive_delay_ns: Option<DbCounter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_start_ns: Option<DbCounter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_event_end_ns: Option<DbCounter>,
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
        RECEIVED_REPRESENTATION => Ok(RECEIVED_FRAME_BYTES),
        RECEIVED_FLOW_REPRESENTATION => Ok(RECEIVED_FLOW_FRAME_BYTES),
        _ => anyhow::bail!("TICK_CACHE_REPRESENTATION_UNSUPPORTED"),
    }
}

fn frame(
    cohort: &Cohort,
    flow: bool,
    received: bool,
) -> Result<[u8; RECEIVED_FLOW_FRAME_BYTES as usize]> {
    let mut encoded = [0; RECEIVED_FLOW_FRAME_BYTES as usize];
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
    if received {
        encoded[32..40].copy_from_slice(&cohort.source_event_start.to_le_bytes());
        encoded[40..48].copy_from_slice(&cohort.source_event_end.to_le_bytes());
    }
    if flow {
        let offset = if received { 48 } else { 32 };
        encoded[offset..offset + 8].copy_from_slice(
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
    let received = bytes.len() >= RECEIVED_FRAME_BYTES as usize;
    let flow = bytes.len() == FLOW_FRAME_BYTES as usize
        || bytes.len() == RECEIVED_FLOW_FRAME_BYTES as usize;
    Ok(Cohort {
        event: value(0),
        source_event_start: if received { value(4) } else { value(0) },
        source_event_end: if received {
            value(5)
        } else {
            value(0).checked_add(1).context("TICK_EVENT_TIME_RANGE")?
        },
        vwap: f64::from_bits(value(1)),
        quantity: f64::from_bits(value(2)),
        count: usize::try_from(value(3))?,
        signed_quantity: flow.then(|| f64::from_bits(value(if received { 6 } else { 4 }))),
    })
}

struct Output {
    flow: bool,
    received: bool,
    path: PathBuf,
    expected: Vec<u8>,
    verified: u64,
    record: CacheFile,
    pending: Vec<TradeTick>,
    identities: BTreeMap<(u64, TradeId), TradeTick>,
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
        let cohort = super::sampled_cohorts(&self.pending, maximum, self.flow, self.received)?
            .pop()
            .context("TICK_CACHE_EMPTY_COHORT")?;
        let encoded = frame(&cohort, self.flow, self.received)?;
        let width = match (self.received, self.flow) {
            (true, true) => RECEIVED_FLOW_FRAME_BYTES,
            (true, false) => RECEIVED_FRAME_BYTES,
            (false, true) => FLOW_FRAME_BYTES,
            (false, false) => FRAME_BYTES,
        };
        self.expected.extend_from_slice(&encoded[..width as usize]);
        self.record.cohort_count = counter(self.record.cohort_count.get() + 1)?;
        self.record.trade_count =
            counter(self.record.trade_count.get() + u64::try_from(cohort.count)?)?;
        self.record
            .first_event_ns
            .get_or_insert(counter(cohort.event)?);
        self.record.last_event_ns = Some(counter(cohort.event)?);
        if self.received {
            self.record.source_event_start_ns = Some(counter(
                self.record
                    .source_event_start_ns
                    .map_or(cohort.source_event_start, |old| {
                        old.get().min(cohort.source_event_start)
                    }),
            )?);
            self.record.source_event_end_ns = Some(counter(
                self.record
                    .source_event_end_ns
                    .map_or(cohort.source_event_end, |old| {
                        old.get().max(cohort.source_event_end)
                    }),
            )?);
        }
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
    maximum_receive_delay_ns: Option<u64>,
    root: PathBuf,
    outputs: BTreeMap<String, Output>,
    pending_rows: usize,
    buffered_bytes: usize,
    maximum_cohort_rows: u32,
    written_bytes: u64,
    sealed: bool,
}

impl Builder {
    fn representation(&self) -> &'static str {
        match (self.maximum_receive_delay_ns.is_some(), self.flow) {
            (true, true) => RECEIVED_FLOW_REPRESENTATION,
            (true, false) => RECEIVED_REPRESENTATION,
            (false, true) => FLOW_REPRESENTATION,
            (false, false) => REPRESENTATION,
        }
    }

    pub fn new(
        output: &Path,
        maximum_cohort_rows: u32,
        _maximum_working_rows: usize,
        _maximum_output_bytes: u64,
    ) -> Result<Self> {
        Self::create(
            output,
            maximum_cohort_rows,
            _maximum_working_rows,
            _maximum_output_bytes,
            false,
            None,
        )
    }

    pub fn new_flow(
        output: &Path,
        maximum_cohort_rows: u32,
        _maximum_working_rows: usize,
        _maximum_output_bytes: u64,
    ) -> Result<Self> {
        Self::create(
            output,
            maximum_cohort_rows,
            _maximum_working_rows,
            _maximum_output_bytes,
            true,
            None,
        )
    }

    pub fn new_received(
        output: &Path,
        maximum_cohort_rows: u32,
        maximum_working_rows: usize,
        maximum_output_bytes: u64,
        flow: bool,
        maximum_receive_delay_ns: u64,
    ) -> Result<Self> {
        counter(maximum_receive_delay_ns)?;
        Self::create(
            output,
            maximum_cohort_rows,
            maximum_working_rows,
            maximum_output_bytes,
            flow,
            Some(maximum_receive_delay_ns),
        )
    }

    fn create(
        output: &Path,
        maximum_cohort_rows: u32,
        _maximum_working_rows: usize,
        _maximum_output_bytes: u64,
        flow: bool,
        maximum_receive_delay_ns: Option<u64>,
    ) -> Result<Self> {
        ensure!(
            maximum_cohort_rows > 0 && maximum_cohort_rows <= domain::ticks::MAXIMUM_COHORT_ROWS,
            "TICK_CACHE_RESOURCE_LIMIT"
        );
        fs::create_dir(output).context("OUTPUT_MUST_BE_NEW")?;
        let root = output.join("catalog");
        fs::create_dir_all(root.join("data/cohorts"))?;
        Ok(Self {
            flow,
            maximum_receive_delay_ns,
            root,
            outputs: BTreeMap::new(),
            pending_rows: 0,
            buffered_bytes: 0,
            maximum_cohort_rows,
            written_bytes: 0,
            sealed: false,
        })
    }

    pub fn push(&mut self, trade: TradeTick) -> Result<()> {
        let width = frame_bytes(self.representation())?;
        ensure!(!self.sealed, "TICK_CACHE_SOURCE_CLOCK_OR_STATE");
        if let Some(maximum) = self.maximum_receive_delay_ns {
            ensure!(
                trade
                    .ts_init
                    .as_u64()
                    .checked_sub(trade.ts_event.as_u64())
                    .is_some_and(|delay| delay <= maximum),
                "TICK_RECEIVE_DELAY_OUTSIDE_BOUND"
            );
        } else {
            ensure!(
                trade.ts_event == trade.ts_init,
                "TICK_CACHE_SOURCE_CLOCK_OR_STATE"
            );
        }
        let id = trade.instrument_id.to_string();
        if !self.outputs.contains_key(&id) {
            let path = format!(
                "data/cohorts/{}.bin",
                nautilus_persistence::backend::parquet::catalog::urisafe_instrument_id(&id)
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
                    received: self.maximum_receive_delay_ns.is_some(),
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
                        source_event_start_ns: None,
                        source_event_end_ns: None,
                    },
                },
            );
        }
        let output = self.outputs.get_mut(&id).unwrap();
        if let Some(previous) = output.pending.first() {
            ensure!(
                trade.ts_init >= previous.ts_init,
                "TICK_CACHE_SOURCE_TIME_REVERSAL"
            );
            if trade.ts_init != previous.ts_init {
                let buffered = output.expected.len();
                let flushed = output.flush_cohort(self.maximum_cohort_rows)?;
                self.buffered_bytes = self.buffered_bytes - buffered + output.expected.len();
                self.pending_rows -= flushed;
                self.written_bytes += width;
            }
        }
        if let Some(previous) = output
            .identities
            .insert((trade.ts_event.as_u64(), trade.trade_id), trade)
        {
            ensure!(previous == trade, "TICK_CACHE_SOURCE_ID_CONFLICT");
            anyhow::bail!("TICK_CACHE_SOURCE_ID_DUPLICATE");
        }
        ensure!(
            output.pending.len() < self.maximum_cohort_rows as usize,
            "TICK_COHORT_ROW_LIMIT"
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
        let width = frame_bytes(self.representation())?;
        for output in self.outputs.values_mut() {
            if output.flush_cohort(self.maximum_cohort_rows)? > 0 {
                self.written_bytes += width;
            }

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
            representation: self.representation().into(),
            source_revision: source_revision.into(),
            availability: if self.maximum_receive_delay_ns.is_some() {
                "RECEIVED_TIME_OBSERVED"
            } else {
                "EVENT_TIME_PROXY_UNVERIFIED"
            }
            .into(),
            event_start_ns: counter(start)?,
            event_end_ns: counter(end)?,
            maximum_receive_delay_ns: self.maximum_receive_delay_ns.map(counter).transpose()?,
            source_event_start_ns: files
                .iter()
                .filter_map(|file| file.source_event_start_ns)
                .min(),
            source_event_end_ns: files
                .iter()
                .filter_map(|file| file.source_event_end_ns)
                .max(),
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
        let _bytes = self
            .written_bytes
            .checked_add(u64::try_from(serde_json::to_vec(&manifest)?.len())? + 1)
            .and_then(|bytes| bytes.checked_add(report_bytes))
            .context("TICK_CACHE_OUTPUT_LIMIT")?;

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
    maximum_receive_delay_ns: Option<u64>,
    pub coverage: NativeTradeTickCoverageV1,
    pub assets: Vec<CacheAsset>,
    root: PathBuf,
    start: u64,
    end: u64,
    maximum_cohort_rows: u32,
}

pub(super) struct CacheReader {
    frame_bytes: u64,
    maximum_receive_delay_ns: Option<u64>,
    input: BufReader<std::io::Take<File>>,
    file: CacheFile,
    start: u64,
    end: u64,
    maximum_cohort_rows: u32,
    seen: u64,
    trades: u64,
    first: Option<u64>,
    last: Option<u64>,
    source_first: Option<u64>,
    source_last: Option<u64>,
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
                        && self.last == self.file.last_event_ns.map(|value| value.get())
                        && self.source_first
                            == self.file.source_event_start_ns.map(|value| value.get())
                        && self.source_last
                            == self.file.source_event_end_ns.map(|value| value.get()),
                    "TICK_CACHE_COUNTS_MISMATCH"
                );
                return Ok(None);
            }
            let mut encoded = [0; RECEIVED_FLOW_FRAME_BYTES as usize];
            self.input
                .read_exact(&mut encoded[..self.frame_bytes as usize])
                .context("TICK_CACHE_FRAME_TRUNCATED")?;
            let cohort = decode(&encoded[..self.frame_bytes as usize])?;
            if let Some(maximum) = self.maximum_receive_delay_ns {
                ensure!(
                    cohort.source_event_start < cohort.source_event_end
                        && cohort.source_event_end - 1 <= cohort.event
                        && cohort
                            .event
                            .checked_sub(cohort.source_event_start)
                            .is_some_and(|delay| delay <= maximum),
                    "TICK_RECEIVE_DELAY_OUTSIDE_BOUND"
                );
                self.source_first =
                    Some(self.source_first.map_or(cohort.source_event_start, |old| {
                        old.min(cohort.source_event_start)
                    }));
                self.source_last = Some(self.source_last.map_or(cohort.source_event_end, |old| {
                    old.max(cohort.source_event_end)
                }));
            }
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
            maximum_receive_delay_ns: self.maximum_receive_delay_ns,
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
            source_first: None,
            source_last: None,
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
    serde_json::from_reader(BufReader::new(file)).context("TICK_CACHE_MANIFEST_INVALID")
}

fn missing_hours(entries: &[Value]) -> Result<u64> {
    entries.iter().try_fold(0_u64, |total, entry| {
        let count = if let Some(hours) = entry.get("hours") {
            u64::try_from(
                hours
                    .as_array()
                    .context("TICK_CACHE_SOURCE_FIELD: hours")?
                    .len(),
            )?
        } else {
            1
        };
        total.checked_add(count).context("TICK_CACHE_COUNT_RANGE")
    })
}

// The catalog keeps the full report; trial evidence carries its facts and reference.
fn compact_published_report(report: &mut Value) -> Result<()> {
    if !super::catalog::observed_published_files(report) {
        return Ok(());
    }
    let source = report["source_manifest"]
        .as_object_mut()
        .context("TICK_CACHE_SOURCE_FIELD: source_manifest")?;
    if let Some(files) = source.remove("files") {
        let entries = files.as_array().context("TICK_CACHE_SOURCE_FIELD: files")?;
        let bytes = entries.iter().try_fold(0_u64, |total, file| {
            total
                .checked_add(number(file, "size")?)
                .context("TICK_CACHE_COUNT_RANGE")
        })?;
        source.insert("file_count".into(), json!(entries.len()));
        source.insert("total_bytes".into(), json!(bytes));
    }
    if let Some(missing) = source.remove("missing") {
        let entries = missing
            .as_array()
            .context("TICK_CACHE_SOURCE_FIELD: missing")?;
        source.insert("missing_entry_count".into(), json!(entries.len()));
        if !source.contains_key("missing_hour_count") {
            source.insert("missing_hour_count".into(), json!(missing_hours(entries)?));
        }
    }
    let fields = report
        .as_object_mut()
        .context("TICK_CACHE_SOURCE_FIELD: source_report")?;
    if let Some(missing) = fields.remove("missing_partitions") {
        let entries = missing
            .as_array()
            .context("TICK_CACHE_SOURCE_FIELD: missing_partitions")?;
        fields.insert("missing_partition_count".into(), json!(entries.len()));
        fields.insert("missing_hour_count".into(), json!(missing_hours(entries)?));
    }
    fields.insert(
        "full_report_reference".into(),
        json!(format!("{MANIFEST_FILE}#source_report")),
    );
    exact_json(report);
    Ok(())
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
    let mut manifest = manifest(&root)?;
    let frame_bytes = frame_bytes(&manifest.representation)?;
    ensure!(
        Some(manifest.representation.as_str()) == request.input.cache_representation()
            && manifest.source_revision == request.input.source_revision
            && manifest.availability == request.input.availability_name()
            && manifest.maximum_receive_delay_ns == request.input.maximum_receive_delay_ns
            && manifest.event_start_ns == request.input.event_start_ns
            && manifest.event_end_ns == request.input.event_end_ns,
        "TICK_CACHE_VIEW_MISMATCH"
    );
    let report = &manifest.source_report;
    ensure!(
        report["status"] == "IMPORTED"
            && report["availability"] == request.input.availability_name()
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
    super::catalog::validate_source_observations(report).context("TICK_CACHE_SOURCE_INCOMPLETE")?;
    ensure!(
        manifest.event_start_ns.get() >= number(report, "event_start_ns")?
            && manifest.event_end_ns.get() <= number(report, "event_end_ns")?,
        "TICK_CACHE_SOURCE_INCOMPLETE"
    );
    let wanted = request
        .input
        .instrument_ids
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();
    ensure!(
        manifest.source_event_start_ns
            == manifest
                .files
                .iter()
                .filter_map(|file| file.source_event_start_ns)
                .min()
            && manifest.source_event_end_ns
                == manifest
                    .files
                    .iter()
                    .filter_map(|file| file.source_event_end_ns)
                    .max(),
        "TICK_CACHE_SOURCE_EVENT_COVERAGE_INVALID"
    );
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
        ensure!(
            if request.input.uses_received_time() && file.cohort_count.get() > 0 {
                matches!((file.source_event_start_ns, file.source_event_end_ns), (Some(start), Some(end)) if start < end)
            } else {
                file.source_event_start_ns.is_none() && file.source_event_end_ns.is_none()
            },
            "TICK_CACHE_SOURCE_EVENT_COVERAGE_INVALID"
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
    ensure!(selected_trades > 0, "TICK_CACHE_SOURCE_EMPTY");
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
    compact_published_report(&mut manifest.source_report)?;
    let coverage = NativeTradeTickCoverageV1 {
        modality: NativeTickModalityV1::TradeTick,
        source_revision: manifest.source_revision,
        instrument_ids: request.input.instrument_ids.clone(),
        instrument_coverage,
        trade_count: counter(selected_trades)?,
        cohort_count: counter(selected_cohorts)?,
        first_event_ns: first,
        last_event_ns: last,
        source_event_start_ns: assets
            .iter()
            .filter_map(|asset| asset.file.source_event_start_ns)
            .min(),
        source_event_end_ns: assets
            .iter()
            .filter_map(|asset| asset.file.source_event_end_ns)
            .max(),
        availability: request.input.availability,
        limitations: vec![
            "UNQUALIFIED_EXPLORATORY".into(),
            request.input.availability_name().into(),
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
        maximum_receive_delay_ns: request
            .input
            .maximum_receive_delay_ns
            .map(|value| value.get()),
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
    _maximum_output_bytes: u64,
) -> Result<Manifest> {
    ensure!(!units.is_empty(), "TICK_CACHE_INPUT_EMPTY");
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
        let received = current.maximum_receive_delay_ns.is_some();
        ensure!(
            current.event_start_ns < current.event_end_ns
                && received == (width >= RECEIVED_FRAME_BYTES)
                && current.availability
                    == if received {
                        "RECEIVED_TIME_OBSERVED"
                    } else {
                        "EVENT_TIME_PROXY_UNVERIFIED"
                    },
            "TICK_CACHE_VIEW_MISMATCH"
        );
        if let Some(previous) = &mut combined {
            ensure!(
                previous.representation == current.representation
                    && previous.source_revision == current.source_revision
                    && previous.availability == current.availability
                    && previous.maximum_receive_delay_ns == current.maximum_receive_delay_ns
                    && previous.event_end_ns == current.event_start_ns,
                "TICK_CACHE_UNIT_MISMATCH"
            );
            previous.event_end_ns = current.event_end_ns;
            previous.source_event_start_ns = previous
                .source_event_start_ns
                .into_iter()
                .chain(current.source_event_start_ns)
                .min();
            previous.source_event_end_ns = previous
                .source_event_end_ns
                .into_iter()
                .chain(current.source_event_end_ns)
                .max();
        } else {
            combined = Some(Manifest {
                schema_version: SchemaV1,
                representation: current.representation.clone(),
                source_revision: current.source_revision.clone(),
                availability: current.availability.clone(),
                event_start_ns: current.event_start_ns,
                event_end_ns: current.event_end_ns,
                maximum_receive_delay_ns: current.maximum_receive_delay_ns,
                source_event_start_ns: current.source_event_start_ns,
                source_event_end_ns: current.source_event_end_ns,
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
                    nautilus_persistence::backend::parquet::catalog::urisafe_instrument_id(
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
                        source_event_start_ns: None,
                        source_event_end_ns: None,
                    },
                );
            }
            let reader = CacheReader {
                frame_bytes: width,
                maximum_receive_delay_ns: current.maximum_receive_delay_ns.map(|value| value.get()),
                input: BufReader::new(source.take(record.bytes.get())),
                file: record.clone(),
                start: current.event_start_ns.get(),
                end: current.event_end_ns.get(),
                maximum_cohort_rows: domain::ticks::MAXIMUM_COHORT_ROWS,
                seen: 0,
                trades: 0,
                first: None,
                last: None,
                source_first: None,
                source_last: None,
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

                writer.write_all(
                    &frame(
                        &cohort,
                        width == FLOW_FRAME_BYTES || width == RECEIVED_FLOW_FRAME_BYTES,
                        received,
                    )?[..width as usize],
                )?;
                target.first_event_ns.get_or_insert(counter(cohort.event)?);
                target.last_event_ns = Some(counter(cohort.event)?);
                if received {
                    target.source_event_start_ns = Some(counter(
                        target
                            .source_event_start_ns
                            .map_or(cohort.source_event_start, |old| {
                                old.get().min(cohort.source_event_start)
                            }),
                    )?);
                    target.source_event_end_ns = Some(counter(
                        target
                            .source_event_end_ns
                            .map_or(cohort.source_event_end, |old| {
                                old.get().max(cohort.source_event_end)
                            }),
                    )?);
                }
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
    source_report["availability"] = json!(result.availability);
    if let Some(delay) = result.maximum_receive_delay_ns {
        source_report["maximum_receive_delay_ns"] = json!(delay);
        source_report["source_event_start_ns"] = json!(result.source_event_start_ns);
        source_report["source_event_end_ns"] = json!(result.source_event_end_ns);
    }
    source_report["event_start_ns"] = json!(result.event_start_ns.get());
    source_report["event_end_ns"] = json!(result.event_end_ns.get());
    result.source_report = source_report.clone();
    exact_json(&mut result.source_report);
    let metadata_bytes = u64::try_from(serde_json::to_vec(&result)?.len())?
        .checked_add(u64::try_from(serde_json::to_vec(&source_report)?.len())?)
        .and_then(|bytes| bytes.checked_add(2))
        .context("TICK_CACHE_OUTPUT_SIZE_OVERFLOW")?;
    let _total_output_bytes = written
        .checked_add(metadata_bytes)
        .context("TICK_CACHE_OUTPUT_SIZE_OVERFLOW")?;

    publish(&output.join("import-report.json"), &source_report)?;
    publish(&root.join(MANIFEST_FILE), &result)?;
    Ok(result)
}

#[derive(Deserialize)]
struct SpotQuoteClock {
    instrument_id: String,
    received_ns: DbCounter,
}

fn prepare_spot_views(
    source: &Path,
    output: &Path,
    instruments: &BTreeSet<&str>,
    primary_end: u64,
    full_end: u64,
) -> Result<(u64, u64)> {
    let mut primary = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("research/catalog/spot-valuation.jsonl"))?,
    );
    let mut full = BufWriter::new(
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(output.join("independent/catalog/spot-valuation.jsonl"))?,
    );
    let mut input = BufReader::new(File::open(source)?);
    let mut line = Vec::new();
    let (mut primary_bytes, mut full_bytes) = (0_u64, 0_u64);
    loop {
        line.clear();
        if input.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        let clock: SpotQuoteClock = serde_json::from_slice(&line)?;
        if !instruments.contains(clock.instrument_id.as_str()) {
            continue;
        }
        let size = u64::try_from(line.len())?;
        if clock.received_ns.get() < full_end {
            full_bytes = full_bytes
                .checked_add(size)
                .context("TICK_PREPARATION_OUTPUT_LIMIT")?;
            if clock.received_ns.get() < primary_end {
                primary_bytes = primary_bytes
                    .checked_add(size)
                    .context("TICK_PREPARATION_OUTPUT_LIMIT")?;
                primary.write_all(&line)?;
            }
            full.write_all(&line)?;
        }
    }
    primary.flush()?;
    full.flush()?;
    primary.get_ref().sync_all()?;
    full.get_ref().sync_all()?;
    Ok((primary_bytes, full_bytes))
}

pub fn prepare_views(
    source: &Path,
    request: &NativeTradeTickFactorEvaluationRequestV1,
    output: &Path,
    _maximum_output_bytes: u64,
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
    full.source_event_start_ns = input.coverage.source_event_start_ns;
    full.source_event_end_ns = input.coverage.source_event_end_ns;
    let mut primary = full.clone();
    primary.event_end_ns = request.validation_end_ns;
    for (asset, file) in input.assets.iter().zip(&mut primary.files) {
        let mut trades = 0;
        let mut count = 0;
        let mut first = None;
        let mut last = None;
        let mut source_first: Option<u64> = None;
        let mut source_last: Option<u64> = None;
        for cohort in input.cohorts(asset)? {
            let cohort = cohort?;
            if cohort.event < request.validation_end_ns.get() {
                trades += u64::try_from(cohort.count)?;
                count += 1;
                first.get_or_insert(counter(cohort.event)?);
                last = Some(counter(cohort.event)?);
                if request.input.uses_received_time() {
                    source_first = Some(source_first.map_or(cohort.source_event_start, |old| {
                        old.min(cohort.source_event_start)
                    }));
                    source_last = Some(source_last.map_or(cohort.source_event_end, |old| {
                        old.max(cohort.source_event_end)
                    }));
                }
            }
        }
        file.byte_end = counter(file.byte_start.get() + count * input.frame_bytes)?;
        file.trade_count = counter(trades)?;
        file.cohort_count = counter(count)?;
        file.first_event_ns = first;
        file.last_event_ns = last;
        file.source_event_start_ns = source_first.map(counter).transpose()?;
        file.source_event_end_ns = source_last.map(counter).transpose()?;
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
    primary.source_event_start_ns = primary
        .files
        .iter()
        .filter_map(|file| file.source_event_start_ns)
        .min();
    primary.source_event_end_ns = primary
        .files
        .iter()
        .filter_map(|file| file.source_event_end_ns)
        .max();
    let spot_sizes = if economic_sources
        .iter()
        .any(|(name, _)| name == "spot-valuation.jsonl")
    {
        for view in ["research/catalog", "independent/catalog"] {
            fs::create_dir_all(output.join(view))?;
        }
        Some(prepare_spot_views(
            &cache_path(&source, "spot-valuation.jsonl")?,
            &output,
            &input
                .assets
                .iter()
                .map(|asset| asset.instrument_id.as_str())
                .collect(),
            request.validation_end_ns.get(),
            request.input.event_end_ns.get(),
        )?)
    } else {
        None
    };
    let logical_bytes = full
        .files
        .iter()
        .map(|file| file.bytes.get())
        .sum::<u64>()
        .checked_mul(2)
        .and_then(|bytes| {
            economic_sources
                .iter()
                .try_fold(bytes, |total, (name, size)| {
                    if name == "spot-valuation.jsonl" {
                        let (primary, full) = spot_sizes?;
                        total.checked_add(primary.checked_add(full)?)
                    } else {
                        total.checked_add(size.checked_mul(2)?)
                    }
                })
        })
        .context("TICK_PREPARATION_OUTPUT_LIMIT")?;
    let mut research_request = request.clone();
    research_request.clip_end(request.validation_end_ns);
    let mut metadata = json!({"schema_version":1,"modality":"TRADE_TICK","representation":full.representation,
        "operational_state":"NATIVE_PREPARED","scientific_qualification":"UNQUALIFIED_EXPLORATORY",
        "catalog_format":format!("QZ_{}", full.representation),"request":request,"research_request":research_request,
        "research_catalog":"research/catalog","independent_catalog":"independent/catalog","coverage":input.coverage,
        "maximum_output_bytes":_maximum_output_bytes.to_string(),"output_bytes":"0"});
    if !economic_sources.is_empty() {
        metadata["economic_sources"] =
            json!(economic_sources
            .iter()
            .map(|(path, bytes)| if path == "spot-valuation.jsonl" {
                let (primary, full) = spot_sizes.unwrap();
                json!({"path":path,"bytes":full.to_string(),"research_bytes":primary.to_string()})
            } else { json!({"path":path,"bytes":bytes.to_string()}) })
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
            if name != "spot-valuation.jsonl" {
                fs::hard_link(cache_path(&source, name)?, root.join(name))?;
            }
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
    fn spot_views_keep_only_selected_instruments_at_each_time_boundary() {
        let temp = tempfile::tempdir().unwrap();
        let source = temp.path().join("source.jsonl");
        let early = "{\"instrument_id\":\"BTC.SIM\",\"received_ns\":\"1\"}\n";
        let excluded = "{\"instrument_id\":\"ETH.SIM\",\"received_ns\":\"2\"}\n";
        let heldout = "{\"instrument_id\":\"BTC.SIM\",\"received_ns\":\"10\"}\n";
        let beyond = "{\"instrument_id\":\"BTC.SIM\",\"received_ns\":\"20\"}\n";
        fs::write(&source, format!("{early}{excluded}{heldout}{beyond}")).unwrap();
        let output = temp.path().join("prepared");
        for view in ["research/catalog", "independent/catalog"] {
            fs::create_dir_all(output.join(view)).unwrap();
        }
        let sizes =
            prepare_spot_views(&source, &output, &BTreeSet::from(["BTC.SIM"]), 10, 20).unwrap();
        assert_eq!(
            fs::read_to_string(output.join("research/catalog/spot-valuation.jsonl")).unwrap(),
            early
        );
        assert_eq!(
            fs::read_to_string(output.join("independent/catalog/spot-valuation.jsonl")).unwrap(),
            format!("{early}{heldout}")
        );
        assert_eq!(
            sizes,
            (early.len() as u64, (early.len() + heldout.len()) as u64)
        );
    }

    #[test]
    fn concatenate_preserves_all_frame_layouts_and_rejects_partial_units() {
        for (flow, received) in [(false, false), (true, false), (false, true), (true, true)] {
            let temp = tempfile::tempdir().unwrap();
            let mut units = Vec::new();
            let mut expected = Vec::new();
            for index in 0..2_u64 {
                let root = temp.path().join(index.to_string());
                fs::create_dir_all(root.join("data/cohorts")).unwrap();
                let cohort = Cohort {
                    event: index * 10 + 1,
                    source_event_start: index * 10,
                    source_event_end: index * 10 + 1,
                    vwap: 1.2345678901234567,
                    quantity: 2.0,
                    count: 2,
                    signed_quantity: flow.then_some(-0.0),
                };
                let representation = match (received, flow) {
                    (true, true) => RECEIVED_FLOW_REPRESENTATION,
                    (true, false) => RECEIVED_REPRESENTATION,
                    (false, true) => FLOW_REPRESENTATION,
                    (false, false) => REPRESENTATION,
                };
                let width = frame_bytes(representation).unwrap();
                let encoded = frame(&cohort, flow, received).unwrap();
                let bytes = &encoded[..width as usize];
                expected.extend_from_slice(bytes);
                fs::write(root.join("data/cohorts/input.bin"), bytes).unwrap();
                publish(
                    &root.join(MANIFEST_FILE),
                    &Manifest {
                        schema_version: SchemaV1,
                        representation: representation.into(),
                        source_revision: "a".repeat(40),
                        availability: if received {
                            "RECEIVED_TIME_OBSERVED"
                        } else {
                            "EVENT_TIME_PROXY_UNVERIFIED"
                        }
                        .into(),
                        event_start_ns: counter(index * 10).unwrap(),
                        event_end_ns: counter(index * 10 + 10).unwrap(),
                        maximum_receive_delay_ns: received.then(|| counter(2).unwrap()),
                        source_event_start_ns: received
                            .then(|| counter(cohort.source_event_start).unwrap()),
                        source_event_end_ns: received
                            .then(|| counter(cohort.source_event_end).unwrap()),
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
                            source_event_start_ns: received
                                .then(|| counter(cohort.source_event_start).unwrap()),
                            source_event_end_ns: received
                                .then(|| counter(cohort.source_event_end).unwrap()),
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
    fn published_wire_report_retains_facts_counts_and_reference_without_source_arrays() {
        let temp = tempfile::tempdir().unwrap();
        let reference = "https://www.cryptohftdata.com/datasets/hyperliquid-orderbook-data";
        let mut report = json!({
            "source_reference":reference,"coverage":"OBSERVED_PUBLISHED_FILES",
            "source_revision":"published-files-2026-01-01-2026-01-02",
            "source_manifest":{"repository":"CryptoHFTData","source_reference":reference,
                "coverage":"OBSERVED_PUBLISHED_FILES","start_date":"2026-01-01","end_date":"2026-01-03",
                "profile":{"streams":[{"name":"trades"}]},
                "files":[{"path":"first.parquet","size":10,"file_identity":"1:2:10:3:4"},
                    {"path":"second.parquet","size":20,"file_identity":"1:3:20:3:4"}],
                "missing":[{"hours":[1,2,3]},{"hour":7}],"missing_hour_count":4,
                "unobserved_symbols":["UNLISTED"]},
            "instrument_source":{"instruments":[{"id":"BTC.SIM"}]},
            "source_coverage":{"trades":{"source_rows":8,"selected_rows":7}},
            "missing_partitions":[{"hour":7}],"missing_hour_count":4,
            "duplicate_trades":1,"deduplicated_trades":1,"deduplication_policy":"EARLIEST_RECEIPT_IDENTICAL_TRADE",
            "source_rows":8,"selected_rows":7,"readback_records":6,"historical_availability":"UNVERIFIED"
        });
        publish(
            &temp.path().join(MANIFEST_FILE),
            &json!({"source_report":report}),
        )
        .unwrap();
        let original = fs::read(temp.path().join(MANIFEST_FILE)).unwrap();
        compact_published_report(&mut report).unwrap();
        assert_eq!(
            report["full_report_reference"],
            "cohort-cache.json#source_report"
        );
        let source = &report["source_manifest"];
        assert!(source.get("files").is_none() && source.get("missing").is_none());
        assert_eq!(source["file_count"], 2);
        assert_eq!(source["total_bytes"], 30);
        assert_eq!(source["missing_entry_count"], 2);
        assert_eq!(source["missing_hour_count"], 4);
        assert_eq!(source["start_date"], "2026-01-01");
        assert_eq!(source["unobserved_symbols"], json!(["UNLISTED"]));
        assert!(report.get("missing_partitions").is_none());
        assert_eq!(report["missing_partition_count"], 1);
        assert_eq!(report["missing_hour_count"], 1);
        assert_eq!(
            report["instrument_source"]["instruments"][0]["id"],
            "BTC.SIM"
        );
        assert_eq!(report["source_coverage"]["trades"]["source_rows"], 8);
        assert_eq!(report["source_rows"], 8);
        assert_eq!(report["selected_rows"], 7);
        assert_eq!(report["readback_records"], 6);
        assert_eq!(report["deduplicated_trades"], 1);
        assert_eq!(report["historical_availability"], "UNVERIFIED");
        let full: Value = serde_json::from_slice(&original).unwrap();
        assert_eq!(
            full["source_report"]["source_manifest"]["files"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
        assert_eq!(
            full["source_report"]["missing_partitions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(fs::read(temp.path().join(MANIFEST_FILE)).unwrap(), original);
    }

    #[test]
    fn legacy_wire_report_keeps_original_source_arrays() {
        let mut report = json!({"source_manifest":{"repository":"legacy/HF",
            "files":[{"path":"fills.parquet","size":12}],"missing":[]},
            "missing_partitions":[],"source_coverage":{"fills":{"source_rows":3}}});
        let original = serde_json::to_vec(&report).unwrap();
        compact_published_report(&mut report).unwrap();
        assert_eq!(serde_json::to_vec(&report).unwrap(), original);
    }

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
