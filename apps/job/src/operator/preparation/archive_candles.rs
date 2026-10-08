//! Independently reproduce one frozen Binance spot archive before native publication.
use super::bars::{
    bar_type, candle, definitions, digest, exact, no_symlinks, publish, read, write_catalog,
};
use anyhow::{Context, Result, ensure};
use bigdecimal::BigDecimal;
use chrono::{DateTime, NaiveDate, Utc};
use clap::Parser;
use nautilus_model::instruments::Instrument;
use rust_decimal::Decimal;
use serde::{
    Deserialize, Deserializer, Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Component, Path, PathBuf},
    str::FromStr,
};

const MIB: u64 = 1024 * 1024;
const SCHEMA: &str = "qz.public_archive_acquisition/1";
const PROVIDER: &str = "binance-vision-spot-klines";
const PROFILE: &str = "CLASSIC_SINGLE_MEMBER_STORED_OR_DEFLATE_V1";
const SPEC_REVISION: &str = "bd110bb04caad6ad964a0098809f18343b1e104b";
const EVIDENCE: [&str; 4] = [
    "vision_terms",
    "incorporated_terms",
    "license",
    "parser_spec",
];

#[derive(Parser)]
#[command(
    about = "Import an original offline spot archive using declared receipt clocks; no PIT attestation",
    after_help = "Only classic single-member stored/deflate ZIP is supported. ts_init is exactly the declared archive completion; UNKNOWN or missing retrieval clocks are refused. OPERATOR_DECLARED stays unverified; SYNTHETIC stays a fixture."
)]
pub struct Arguments {
    /// Original frozen archive.json and its unchanged local bundle.
    #[arg(long)]
    pub acquisition: PathBuf,
    /// Original native InstrumentAny JSON array, including historical fees and ticks.
    #[arg(long)]
    pub instruments: PathBuf,
    /// New output directory; import-report.json is published only after native readback.
    #[arg(long)]
    pub output: PathBuf,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Selection {
    symbol: String,
    base_asset: String,
    quote_asset: String,
    day: String,
    interval: String,
}

struct Spec {
    selection: Selection,
    start: u64,
    end: u64,
    interval: u64,
    scale: u64,
    plan: Value,
}
impl Spec {
    fn new(selection: Selection) -> Result<Self> {
        for part in [
            &selection.symbol,
            &selection.base_asset,
            &selection.quote_asset,
        ] {
            ensure!(
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit()),
                "SOURCE_SYMBOL"
            );
        }
        ensure!(
            selection.base_asset != selection.quote_asset
                && selection.symbol == format!("{}{}", selection.base_asset, selection.quote_asset),
            "SOURCE_SYMBOL"
        );
        let interval = match selection.interval.as_str() {
            "1m" => 60,
            "3m" => 180,
            "5m" => 300,
            "15m" => 900,
            "30m" => 1800,
            "1h" => 3600,
            "2h" => 7200,
            "4h" => 14400,
            "6h" => 21600,
            "8h" => 28800,
            "12h" => 43200,
            "1d" => 86400,
            _ => anyhow::bail!("SOURCE_INTERVAL"),
        };
        ensure!(
            selection.day.len() == 10
                && selection
                    .day
                    .bytes()
                    .enumerate()
                    .all(|(i, b)| if i == 4 || i == 7 {
                        b == b'-'
                    } else {
                        b.is_ascii_digit()
                    })
                && !selection.day.starts_with("0000"),
            "SOURCE_DAY"
        );
        let date = NaiveDate::parse_from_str(&selection.day, "%Y-%m-%d")?;
        let start = u64::try_from(
            date.and_hms_opt(0, 0, 0)
                .context("SOURCE_DAY")?
                .and_utc()
                .timestamp(),
        )?
        .checked_mul(1_000_000_000)
        .context("TIMESTAMP_RANGE")?;
        let end = start
            .checked_add(86_400_000_000_000)
            .context("TIMESTAMP_RANGE")?;
        ensure!(end <= i64::MAX as u64, "TIMESTAMP_RANGE");
        let unit = if selection.day.as_str() >= "2025-01-01" {
            "us"
        } else {
            "ms"
        };
        let stem = format!(
            "{}-{}-{}",
            selection.symbol, selection.interval, selection.day
        );
        let archive = format!("{stem}.zip");
        let url = format!(
            "https://data.binance.vision/data/spot/daily/klines/{}/{}/{archive}",
            selection.symbol, selection.interval
        );
        let plan = json!({"schema":SCHEMA,"provider":{"id":PROVIDER,"version":1,"venue":"BINANCE","market":"SPOT","record_kind":"OHLCV_CANDLE"},
            "parser":"binance-vision-spot-csv/1","selection":selection,"identity_status":"OPERATOR_DECLARED_UNVERIFIED",
            "start_ns":start.to_string(),"end_ns":end.to_string(),"interval_seconds":interval,"source_timestamp_unit":unit,
            "archive_name":archive,"member_name":format!("{stem}.csv"),
            "source_references":[{"role":"CHECKSUM","url":format!("{url}.CHECKSUM")},{"role":"ARCHIVE","url":url}],
            "source_reference_status":"DOCUMENTATION_DERIVED_NOT_REQUESTED","artifact_scope":"OFFLINE_LOCAL_IMPORT",
            "parser_spec_reference":{"url":format!("https://github.com/binance/binance-public-data/blob/{SPEC_REVISION}/README.md"),"documentation_revision":SPEC_REVISION},
            "container_validation":"STDLIB_MEMBER_VALIDATION_NOT_CANONICAL_ZIP",
            "limits":{"archive_bytes":MIB,"checksum_bytes":4096,"csv_bytes":4*MIB,"row_bytes":4096,"field_bytes":128,"decimal_digits":100,"rows":1440,"provenance_bytes":4096,"evidence_bytes_each":2*MIB,"manifest_bytes":256*1024,"records_bytes":4*MIB},
            "admission":{"coverage":"UNPROVEN","historical_availability":"UNVERIFIED","research_qualified":false,"registered_in_quazonai":false,"permission_status":"REQUIRES_INDEPENDENT_REVIEW"}});
        Ok(Self {
            selection,
            start,
            end,
            interval,
            scale: if unit == "us" { 1000 } else { 1_000_000 },
            plan,
        })
    }
}

// Value's ordinary map deserializer overwrites duplicate fields. All source JSON,
// including arbitrary nested file/provenance maps and original definitions, is
// read through this recursive visitor before semantic reproduction.
struct Unique(Value);
impl<'de> Deserialize<'de> for Unique {
    fn deserialize<D: Deserializer<'de>>(d: D) -> std::result::Result<Self, D::Error> {
        struct Strict;
        impl<'de> Visitor<'de> for Strict {
            type Value = Unique;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("JSON without duplicate object fields")
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> std::result::Result<Unique, E> {
                serde_json::Number::from_f64(v)
                    .map(|n| Unique(n.into()))
                    .ok_or_else(|| E::custom("nonfinite JSON"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_string<E: de::Error>(self, v: String) -> std::result::Result<Unique, E> {
                Ok(Unique(v.into()))
            }
            fn visit_unit<E: de::Error>(self) -> std::result::Result<Unique, E> {
                Ok(Unique(Value::Null))
            }
            fn visit_seq<A: SeqAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut values = Vec::new();
                while let Some(Unique(v)) = a.next_element()? {
                    values.push(v);
                }
                Ok(Unique(values.into()))
            }
            fn visit_map<A: MapAccess<'de>>(
                self,
                mut a: A,
            ) -> std::result::Result<Unique, A::Error> {
                let mut values = Map::new();
                while let Some((k, Unique(v))) = a.next_entry::<String, Unique>()? {
                    if values.insert(k, v).is_some() {
                        return Err(de::Error::custom("duplicate JSON field"));
                    }
                }
                Ok(Unique(values.into()))
            }
        }
        d.deserialize_any(Strict)
    }
}
fn strict_json(bytes: &[u8]) -> Result<Value> {
    Ok(serde_json::from_slice::<Unique>(bytes)?.0)
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    value
        .get(key)
        .and_then(Value::as_str)
        .context("SOURCE_FIELD_TYPE")
}
fn unsigned(s: &str) -> Result<u64> {
    ensure!(
        !s.is_empty()
            && s.len() <= 20
            && s.bytes().all(|b| b.is_ascii_digit())
            && (s.len() == 1 || !s.starts_with('0')),
        "SOURCE_UNSIGNED"
    );
    Ok(s.parse()?)
}
fn clock(s: &str) -> Result<u64> {
    let core = s
        .strip_suffix('Z')
        .context("UTC_NANOSECOND_CLOCK_REQUIRED")?;
    let (whole, fraction) = core
        .split_once('.')
        .map_or((core, None), |(a, b)| (a, Some(b)));
    ensure!(
        whole.len() == 19
            && whole.bytes().enumerate().all(|(i, b)| match i {
                4 | 7 => b == b'-',
                10 => b == b'T',
                13 | 16 => b == b':',
                _ => b.is_ascii_digit(),
            })
            && fraction
                .is_none_or(|f| (1..=9).contains(&f.len()) && f.bytes().all(|b| b.is_ascii_digit()))
            && &whole[17..19] < "60" && !whole.starts_with("0000"),
        "UTC_NANOSECOND_CLOCK_REQUIRED"
    );
    Ok(u64::try_from(
        DateTime::parse_from_rfc3339(s)?
            .timestamp_nanos_opt()
            .context("TIMESTAMP_RANGE")?,
    )?)
}
fn amount(s: &str, positive: bool) -> Result<BigDecimal> {
    let (integer, fraction) = s.split_once('.').map_or((s, None), |(a, b)| (a, Some(b)));
    ensure!(
        !integer.is_empty()
            && integer.bytes().all(|b| b.is_ascii_digit())
            && (integer.len() == 1 || !integer.starts_with('0'))
            && fraction.is_none_or(|f| !f.is_empty() && f.bytes().all(|b| b.is_ascii_digit())),
        "SOURCE_DECIMAL_LEXEME"
    );
    let v = BigDecimal::from_str(s)?;
    ensure!(
        !positive || v > BigDecimal::from(0),
        "SOURCE_NONPOSITIVE_PRICE"
    );
    Ok(v)
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Provenance {
    kind: String,
    retrieval: Option<Retrieval>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Retrieval {
    checksum: RetrievalClock,
    archive: RetrievalClock,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct RetrievalClock {
    started_at: String,
    completed_at: String,
}
fn provenance(bytes: Option<&[u8]>, spec: &Spec, imported: u64) -> Result<(Value, u64, Value)> {
    let value = bytes
        .map(strict_json)
        .transpose()?
        .unwrap_or_else(|| json!({"kind":"UNKNOWN","retrieval":null}));
    let declared: Provenance = serde_json::from_value(value.clone())?;
    ensure!(
        matches!(
            declared.kind.as_str(),
            "UNKNOWN" | "OPERATOR_DECLARED" | "SYNTHETIC"
        ) && imported >= spec.end,
        "SOURCE_PROVENANCE"
    );
    ensure!(
        serde_json::to_value(&declared)? == value,
        "SOURCE_PROVENANCE"
    );
    ensure!(declared.kind != "UNKNOWN", "SOURCE_RECEIPT_REQUIRED");
    let retrieval = declared.retrieval.context("SOURCE_RECEIPT_REQUIRED")?;
    let mut previous = spec.end;
    for entry in [&retrieval.checksum, &retrieval.archive] {
        let start = clock(&entry.started_at)?;
        let finish = clock(&entry.completed_at)?;
        ensure!(
            previous <= start && start <= finish && finish <= imported,
            "SOURCE_RETRIEVAL_ORDER"
        );
        previous = finish;
    }
    let basis = json!({"kind":if declared.kind == "SYNTHETIC" { "SYNTHETIC" } else { "OPERATOR_DECLARED_UNVERIFIED" },
        "source_clock":"provenance.retrieval.archive.completed_at","declared_observed_at":retrieval.archive.completed_at,"ts_init_ns":previous.to_string()});
    Ok((value, previous, basis))
}

struct Candle {
    values: [Decimal; 5],
    event: u64,
}
fn records(
    body: &[u8],
    spec: &Spec,
    declaration: &Value,
    basis: &Value,
) -> Result<(Vec<u8>, Vec<Candle>, Value)> {
    ensure!(
        body.is_ascii()
            && body
                .iter()
                .all(|b| !matches!(*b,0..=9|11..=12|14..=31|127|b'"')),
        "SOURCE_CSV_BYTES"
    );
    let csv = std::str::from_utf8(body)?;
    ensure!(
        !csv.replace("\r\n", "").contains('\r'),
        "SOURCE_CSV_NEWLINES"
    );
    let physical: Vec<_> = csv
        .strip_suffix('\n')
        .unwrap_or(csv)
        .split('\n')
        .filter(|_| !csv.is_empty())
        .collect();
    let possible = 86400 / spec.interval;
    ensure!(
        physical.len() <= possible as usize
            && physical
                .iter()
                .all(|l| !l.trim_end_matches('\r').is_empty()),
        "SOURCE_CSV_ROW_LIMIT"
    );
    let mut reader = csv::ReaderBuilder::new()
        .has_headers(false)
        .quoting(false)
        .flexible(false)
        .from_reader(body);
    let mut reproduced = Vec::new();
    let mut candles = Vec::new();
    let mut present = BTreeSet::new();
    let mut previous = None;
    let width = spec.interval * 1_000_000_000;
    for (index, row) in reader.records().enumerate() {
        let row = row?;
        ensure!(
            index < physical.len() && row.len() == 12,
            "SOURCE_CSV_FIELDS"
        );
        let opened = unsigned(&row[0])?
            .checked_mul(spec.scale)
            .context("TIMESTAMP_RANGE")?;
        let closed = unsigned(&row[6])?
            .checked_mul(spec.scale)
            .context("TIMESTAMP_RANGE")?;
        ensure!(
            spec.start <= opened
                && opened < spec.end
                && (opened - spec.start).is_multiple_of(width)
                && closed == opened + width - spec.scale
                && previous.is_none_or(|p| p < opened),
            "SOURCE_CSV_CLOCK"
        );
        previous = Some(opened);
        present.insert(opened);
        let amounts = [
            amount(&row[1], true)?,
            amount(&row[2], true)?,
            amount(&row[3], true)?,
            amount(&row[4], true)?,
            amount(&row[5], false)?,
            amount(&row[7], false)?,
            amount(&row[9], false)?,
            amount(&row[10], false)?,
        ];
        ensure!(
            amounts[2] <= amounts[0]
                && amounts[0] <= amounts[1]
                && amounts[2] <= amounts[3]
                && amounts[3] <= amounts[1]
                && amounts[6] <= amounts[4]
                && amounts[7] <= amounts[5],
            "SOURCE_CSV_VALUES"
        );
        unsigned(&row[8])?;
        ensure!(
            !row[11].is_empty() && row[11].bytes().all(|b| (32..=126).contains(&b)),
            "SOURCE_CSV_IGNORED"
        );
        let record = json!({"kind":"OHLCV_CANDLE","symbol":spec.selection.symbol,"bucket_open_ns":opened.to_string(),"event_end_ns":(opened+width).to_string(),
            "source_open":row[0],"source_close_inclusive":row[6],"source_close_inclusive_ns":closed.to_string(),"source_timestamp_unit":spec.plan["source_timestamp_unit"],
            "open":row[1],"high":row[2],"low":row[3],"close":row[4],"base_volume":row[5],"quote_volume":row[7],"trade_count":row[8],
            "taker_base_volume":row[9],"taker_quote_volume":row[10],"source_ignored":row[11],"source_archive":"raw/archive.zip","source_member":spec.plan["member_name"],
            "source_row_index":index.to_string(),"observed_at":null,"historical_available_at":null,"provenance_kind":declaration["kind"],"declared_observed_at":basis["declared_observed_at"]});
        // The workspace enables serde_json/preserve_order. Canonical source
        // JSONL requires sorted keys regardless of Cargo feature unification.
        reproduced.extend(serde_json::to_vec(
            &record
                .as_object()
                .context("SOURCE_RECORD")?
                .iter()
                .collect::<BTreeMap<_, _>>(),
        )?);
        reproduced.push(b'\n');
        candles.push(Candle {
            values: [
                exact(&row[1])?,
                exact(&row[2])?,
                exact(&row[3])?,
                exact(&row[4])?,
                exact(&row[5])?,
            ],
            event: opened + width,
        });
    }
    ensure!(candles.len() == physical.len(), "SOURCE_CSV_ROW_COUNT");
    let missing: Vec<_> = (spec.start..spec.end)
        .step_by(width as usize)
        .filter(|t| !present.contains(t))
        .map(|t| t.to_string())
        .collect();
    let counts = json!({"rows":candles.len().to_string(),"possible_buckets":possible.to_string(),"missing_buckets":missing.len().to_string(),"missing_bucket_open_ns":missing});
    Ok((reproduced, candles, counts))
}

struct OriginalFile {
    path: PathBuf,
    bytes: Vec<u8>,
}
fn source_file(
    root: &Path,
    name: &str,
    files: &mut Map<String, Value>,
    originals: &mut Vec<OriginalFile>,
) -> Result<Vec<u8>> {
    let path = root.join(name);
    let bytes = read(&path)?;
    files.insert(
        name.into(),
        json!({"path":name,"size":bytes.len(),"sha256":digest(&bytes)}),
    );
    originals.push(OriginalFile {
        path,
        bytes: bytes.clone(),
    });
    Ok(bytes)
}
fn checksum(bytes: &[u8], archive: &[u8], name: &str) -> Result<String> {
    let s = std::str::from_utf8(bytes)?;
    let s = s
        .strip_suffix("\r\n")
        .or_else(|| s.strip_suffix('\n'))
        .unwrap_or(s);
    ensure!(
        s.len() == 66 + name.len()
            && s.as_bytes()[..64].iter().all(u8::is_ascii_hexdigit)
            && &s[64..65] == " "
            && matches!(s.as_bytes()[65], b' ' | b'*')
            && &s[66..] == name,
        "SOURCE_CHECKSUM_FORMAT"
    );
    let hash = s[..64].to_ascii_lowercase();
    ensure!(hash == digest(archive), "SOURCE_CHECKSUM_MISMATCH");
    Ok(hash)
}

pub fn run(args: &Arguments) -> Result<Value> {
    let bytes = read(&args.acquisition)?;
    let manifest = strict_json(&bytes)?;
    let spec = Spec::new(serde_json::from_value(
        manifest
            .get("selection")
            .context("SOURCE_SELECTION")?
            .clone(),
    )?)?;
    let source = args
        .acquisition
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut original_files = vec![OriginalFile {
        path: args.acquisition.clone(),
        bytes: bytes.clone(),
    }];
    let imported = clock(text(&manifest, "imported_at")?)?;
    let published = clock(text(&manifest, "published_at")?)?;
    ensure!(
        imported <= published
            && published
                <= u64::try_from(
                    Utc::now()
                        .timestamp_nanos_opt()
                        .context("TIMESTAMP_RANGE")?
                )?,
        "SOURCE_PUBLICATION_ORDER"
    );
    let original_map = manifest
        .get("files")
        .and_then(Value::as_object)
        .context("SOURCE_FILES")?;
    let mut files = Map::new();
    let archive = source_file(source, "raw/archive.zip", &mut files, &mut original_files)?;
    let checksum_bytes = source_file(
        source,
        "raw/archive.CHECKSUM",
        &mut files,
        &mut original_files,
    )?;
    let record_bytes = source_file(source, "records.jsonl", &mut files, &mut original_files)?;
    let provenance_bytes = original_map
        .contains_key("provenance.json")
        .then(|| source_file(source, "provenance.json", &mut files, &mut original_files))
        .transpose()?;
    let (declaration, received, basis) = provenance(provenance_bytes.as_deref(), &spec, imported)?;
    let mut evidence = Map::new();
    for role in EVIDENCE {
        let path = format!("evidence/{role}.bin");
        let supplied = original_map.contains_key(&path);
        if supplied {
            source_file(source, &path, &mut files, &mut original_files)?;
        }
        evidence.insert(role.into(),json!({"status":if supplied {"OPERATOR_SUPPLIED_UNVERIFIED"} else {"NOT_SUPPLIED"},"source_url":null,"revision":null}));
    }
    let checksum_hash = checksum(&checksum_bytes, &archive, text(&spec.plan, "archive_name")?)?;
    let decoded =
        super::classic_zip::decode_zip(&archive, text(&spec.plan, "member_name")?.as_bytes())
            .map_err(anyhow::Error::msg)?;
    let (reproduced, candles, counts) = records(&decoded, &spec, &declaration, &basis)?;
    ensure!(reproduced == record_bytes, "SOURCE_RECORD_REPRODUCTION");
    let implementation = text(&manifest, "implementation_sha256")?;
    ensure!(
        implementation.len() == 64
            && implementation
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)),
        "SOURCE_IMPLEMENTATION_HASH"
    );
    let mut expected = spec.plan.clone();
    let recorded_limits = manifest
        .get("limits")
        .context("SOURCE_ACQUISITION_LIMITS")?;
    let mut uncapped_limits = expected["limits"].clone();
    for value in uncapped_limits
        .as_object_mut()
        .context("SOURCE_PLAN")?
        .values_mut()
    {
        *value = Value::Null;
    }
    ensure!(
        *recorded_limits == expected["limits"] || *recorded_limits == uncapped_limits,
        "SOURCE_ACQUISITION_LIMITS"
    );
    expected["limits"] = recorded_limits.clone();
    expected.as_object_mut().context("SOURCE_PLAN")?.extend(json!({
        "files":files,"decoded_member":{"name":spec.plan["member_name"],"size":decoded.len(),"sha256":digest(&decoded)},
        "checksum_sha256":checksum_hash,"archive_identity":format!("sha256:{}",digest(&archive)),"upstream_revision":null,
        "implementation_sha256":implementation,"implementation_revision":null,"implementation_status":"RECORDED_LOCAL_HASH_NOT_ATTESTED",
        "provenance":declaration,"provenance_kind":declaration["kind"],"provenance_status":if declaration["kind"] == "SYNTHETIC" {"SYNTHETIC"} else {"DECLARED_UNVERIFIED"},
        "evidence":evidence,"imported_at":manifest["imported_at"],"published_at":manifest["published_at"],"counts":counts,
        "status":if candles.is_empty() {"NO_OBSERVATIONS"} else {"OBSERVATIONS"}}).as_object().context("SOURCE_MANIFEST")?.clone());
    ensure!(manifest == expected, "SOURCE_MANIFEST_REPRODUCTION");
    ensure!(!candles.is_empty(), "SOURCE_EMPTY");
    let definition_bytes = read(&args.instruments)?;
    strict_json(&definition_bytes)?;
    let instruments = definitions(
        &definition_bytes,
        "BINANCE",
        &spec.selection.symbol,
        &spec.selection.base_asset,
        &spec.selection.quote_asset,
    )?;
    ensure!(
        instruments.iter().all(|i| i.ts_init().as_u64() <= received),
        "UNOBSERVED_FUTURE_DEFINITION"
    );
    let kind = bar_type(&instruments, spec.interval)?;
    let bars = candles
        .iter()
        .map(|c| candle(kind, c.values, c.event, received, &instruments))
        .collect::<Result<Vec<_>>>()?;
    let counter = |v| contracts::DbCounter::new(v).map_err(anyhow::Error::msg);
    let selection = contracts::science::NativeBarSelectionV1 {
        schema_version: contracts::SchemaV1,
        bar_types: vec![kind.to_string()],
        event_start_ns: counter(bars[0].ts_event.as_u64())?,
        event_end_ns: counter(
            bars.last()
                .context("SOURCE_EMPTY")?
                .ts_event
                .as_u64()
                .checked_add(spec.interval * 1_000_000_000)
                .context("TIMESTAMP_RANGE")?,
        )?,
        decision_cutoff_ns: counter(received)?,
        maximum_rows: u32::try_from(bars.len())?,
    };
    original_files.push(OriginalFile {
        path: args.instruments.clone(),
        bytes: definition_bytes.clone(),
    });
    let parent = args
        .output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    no_symlinks(parent)?;
    let output = parent.canonicalize()?.join(
        args.output
            .file_name()
            .context("OUTPUT_DIRECTORY_REQUIRED")?,
    );
    ensure!(
        !output.starts_with(source.canonicalize()?)
            && !args
                .output
                .components()
                .any(|p| matches!(p, Component::ParentDir)),
        "OUTPUT_OVERLAPS_SOURCE"
    );
    fs::create_dir(&output).context("OUTPUT_MUST_BE_NEW")?;
    let detached = json!({"schema_version":1,"source_acquisition_path":args.acquisition.canonicalize()?,"acquisition_sha256":digest(&bytes),"acquisition":manifest,
        "instrument_definitions_sha256":digest(&definition_bytes),"instrument_definitions":strict_json(&definition_bytes)?,"receipt_basis":basis,"supported_zip_profile":PROFILE});
    publish(
        &output.join("source-evidence.json"),
        &serde_json::to_vec_pretty(&detached)?,
    )?;
    let root = output.join("catalog");
    fs::create_dir(&root)?;
    write_catalog(&root, &instruments, &bars)?;
    for original in original_files {
        ensure!(
            read(&original.path)? == original.bytes,
            "SOURCE_INPUT_CHANGED_DURING_IMPORT"
        );
    }
    let report = json!({"schema_version":1,"native_version":"0.63.0","source_provider":PROVIDER,"source_schema":SCHEMA,"source_record_kind":"OHLCV_CANDLE",
        "native_selection":selection,"acquisition_sha256":digest(&bytes),"instrument_definitions_sha256":digest(&definition_bytes),"instruments":1,"instrument_versions":instruments.len(),"bars":bars.len(),
        "catalog_relative_path":"catalog","source_evidence_relative_path":"source-evidence.json","native_readback_verified":true,
        "source_provenance_kind":declaration["kind"],"receipt_basis":basis,"supported_zip_profile":PROFILE,
        "coverage":"UNPROVEN","historical_availability":"UNVERIFIED","registered_in_quazonai":false,"research_qualified":false,
        "limitations":["Declared archive completion is an unverified simulation receipt, not attested historical availability.",
        "Original observed_at and historical_available_at remain null; synthetic provenance remains a fixture.",
        "Only the explicit classic single-member stored/deflate ZIP profile is supported; ZIP64 and other containers are refused.",
        "No source authentication, data-use grant, PIT verification, catalog registration or scientific qualification is created.",
        "Retain original archive/checksum/provenance/evidence files; detached evidence stays outside research mounts."]});
    publish(
        &output.join("import-report.json"),
        &serde_json::to_vec_pretty(&report)?,
    )?;
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn spec(day: &str, interval: &str) -> Spec {
        Spec::new(Selection {
            symbol: "BTCUSDT".into(),
            base_asset: "BTC".into(),
            quote_asset: "USDT".into(),
            day: day.into(),
            interval: interval.into(),
        })
        .unwrap()
    }
    fn row(spec: &Spec, offset: u64) -> String {
        let opened = spec.start + offset * spec.interval * 1_000_000_000;
        format!(
            "{},1.00,2.00,0.50,1.50,0.10,{},10000000000000000000000000000000000000000000000000000000000.00000000000001,1,0.01,0.00000000000001,0",
            opened / spec.scale,
            (opened + spec.interval * 1_000_000_000 - spec.scale) / spec.scale
        )
    }
    fn parse(bytes: &[u8], spec: &Spec) -> Result<(Vec<u8>, Vec<Candle>, Value)> {
        records(
            bytes,
            spec,
            &json!({"kind":"SYNTHETIC"}),
            &json!({"declared_observed_at":"2025-01-02T00:00:00.123456789Z"}),
        )
    }
    #[test]
    fn clocks_are_exact_and_reject_loss_or_non_utc_syntax() {
        assert_eq!(
            clock("2025-01-02T00:00:00.123456789Z").unwrap(),
            1_735_776_000_123_456_789
        );
        for value in [
            "2025-01-02T00:00:00.1234567890Z",
            "2025-01-02T00:00:00+00:00",
            "2025-01-02T00:00:60Z",
            "0000-01-01T00:00:00Z",
            "1969-12-31T23:59:59Z",
            "2263-01-01T00:00:00Z",
        ] {
            assert!(clock(value).is_err(), "{value}");
        }
    }
    #[test]
    fn source_decimal_lexemes_and_native_precision_are_separate() {
        assert!(amount("123456789012345678901234567890.00000000000001", false).is_ok());
        assert!(exact("123456789012345678901234567890.00000000000001").is_err());
        for value in ["", "00", "01", "1.", ".1", "-1", "+1", "1e2", "1 0", "1\t0"] {
            assert!(amount(value, false).is_err(), "{value}");
        }
        assert!(amount(&"1".repeat(101), false).is_ok());
        assert!(amount("0", true).is_err());
        assert!(amount("0", false).is_ok());
    }
    #[test]
    fn csv_reproduces_exact_flat_sorted_json_and_sparse_buckets() {
        let s = spec("2025-01-01", "1m");
        let csv = format!("{}\r\n{}", row(&s, 0), row(&s, 2));
        let (bytes, candles, counts) = parse(csv.as_bytes(), &s).unwrap();
        assert_eq!(candles.len(), 2);
        assert_eq!(counts["missing_buckets"], "1438");
        assert_eq!(
            counts["missing_bucket_open_ns"][0],
            (s.start + 60_000_000_000).to_string()
        );
        let first = bytes.split(|b| *b == b'\n').next().unwrap();
        assert!(first.starts_with(b"{\"base_volume\":\"0.10\",\"bucket_open_ns\":"));
        let record: Value = serde_json::from_slice(first).unwrap();
        assert_eq!(record["open"], "1.00");
        assert!(record["observed_at"].is_null());
        assert!(record["historical_available_at"].is_null());
    }
    #[test]
    fn csv_refuses_field_line_ascii_decimal_and_ancillary_forgery() {
        let s = spec("2025-01-01", "1m");
        let valid = row(&s, 0);
        let cases = [
            format!("{valid},extra"),
            format!("{valid}\n\n"),
            format!("{valid}\r"),
            format!("{valid}\n{valid}"),
            valid.replacen("1.00", "\"1.00\"", 1),
            valid.replacen("1.00", "01.00", 1),
            valid.replacen("1.00", "3.00", 1),
            valid.replacen("0.01,0.00000000000001", "0.11,0.00000000000001", 1),
            valid.replacen(
                "0.00000000000001,0",
                "999999999999999999999999999999999999999999999999999999999999999999999999999999,0",
                1,
            ),
            valid.replacen(",1,0.01", ",01,0.01", 1),
            format!("{valid}{}", "0".repeat(129)),
            format!("{valid}\t"),
            format!("{valid}é"),
        ];
        for csv in cases {
            assert!(parse(csv.as_bytes(), &s).is_err(), "accepted {csv:?}");
        }
    }
    #[test]
    fn csv_timestamp_unit_and_end_are_selected_by_day() {
        for day in ["2024-12-31", "2025-01-01"] {
            let s = spec(day, "1d");
            let valid = row(&s, 0);
            let (_, candles, counts) = parse(valid.as_bytes(), &s).unwrap();
            assert_eq!(candles[0].event, s.end);
            assert_eq!(counts["missing_buckets"], "0");
            let mut fields: Vec<_> = valid.split(',').map(str::to_owned).collect();
            fields[6] = (s.end / s.scale).to_string();
            assert!(parse(fields.join(",").as_bytes(), &s).is_err());
            fields = valid.split(',').map(str::to_owned).collect();
            fields[0] = (s.start / (if s.scale == 1000 { 1_000_000 } else { 1000 })).to_string();
            assert!(parse(fields.join(",").as_bytes(), &s).is_err());
        }
    }
    #[test]
    fn strict_json_rejects_duplicate_fields_at_any_depth() {
        for bytes in [
            br#"{"x":1,"x":2}"#.as_slice(),
            br#"{"files":{"x":{},"x":{}}}"#,
            br#"[{"fee":"0","fee":"1"}]"#,
        ] {
            assert!(strict_json(bytes).is_err());
        }
    }
}
