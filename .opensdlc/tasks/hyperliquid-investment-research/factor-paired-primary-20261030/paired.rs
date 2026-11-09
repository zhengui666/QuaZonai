use anyhow::{Context, Result, ensure};
use contracts::{DbCounter, science::FeatureValueV1};
use job::{feature_model::FeatureModel, signals::SignalModule};
use nautilus_indicators::{
    average::ema::ExponentialMovingAverage,
    indicator::{Indicator, MovingAverage},
};
use serde_json::{Value, json};
use std::{
    fs::File,
    io::{BufReader, Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

const ROOT: &str = "/home/zzy/.local/share/quazonai-ts/data/catalogs/hyperliquid-flow-prepared-20261028/attempt-0001/prepared/research/catalog";
const START: u64 = 1_753_660_800_000_000_000;
const TRAIN_END: u64 = 1_772_323_200_000_000_000;
const END: u64 = 1_780_272_000_000_000_000;
const HORIZON: u64 = 60_000_000_000;
const BASE: &str = "/home/zzy/projects/QuaZonai/.opensdlc/tasks/hyperliquid-investment-research";

#[derive(Clone, Copy, Debug)]
struct Cohort {
    event: u64,
    price: f64,
    quantity: f64,
    count: u64,
    flow: f64,
}
impl Cohort {
    fn decode(b: [u8; 40]) -> Result<Self> {
        let n = |i: usize| u64::from_le_bytes(b[i * 8..i * 8 + 8].try_into().unwrap());
        let c = Self {
            event: n(0),
            price: f64::from_bits(n(1)),
            quantity: f64::from_bits(n(2)),
            count: n(3),
            flow: f64::from_bits(n(4)),
        };
        ensure!(
            c.price.is_finite()
                && c.price > 0.0
                && c.quantity.is_finite()
                && c.quantity > 0.0
                && c.count > 0
                && c.count <= 100000
                && c.flow.is_finite()
                && c.flow.abs() <= c.quantity,
            "invalid cohort"
        );
        Ok(c)
    }
}
struct Reader {
    input: BufReader<std::io::Take<File>>,
    left: u64,
    last: Option<u64>,
}
impl Reader {
    fn open(root: &Path, entry: &Value) -> Result<Self> {
        let rel = entry["path"].as_str().context("path missing")?;
        ensure!(
            Path::new(rel)
                .components()
                .all(|p| matches!(p, std::path::Component::Normal(_))),
            "invalid path"
        );
        let start = num(entry, "byte_start")?;
        let end = num(entry, "byte_end")?;
        ensure!(
            start <= end
                && start % 40 == 0
                && end % 40 == 0
                && (end - start) / 40 == num(entry, "cohort_count")?,
            "invalid range"
        );
        let mut file = File::open(root.join(rel))?;
        ensure!(end <= file.metadata()?.len(), "short file");
        file.seek(SeekFrom::Start(start))?;
        Ok(Self {
            input: BufReader::new(file.take(end - start)),
            left: (end - start) / 40,
            last: None,
        })
    }
    fn next(&mut self) -> Result<Option<Cohort>> {
        if self.left == 0 {
            return Ok(None);
        }
        let mut bytes = [0; 40];
        self.input.read_exact(&mut bytes)?;
        let c = Cohort::decode(bytes)?;
        ensure!(
            c.event >= START && c.event < END && self.last.is_none_or(|last| last < c.event),
            "event out of scope/order"
        );
        self.left -= 1;
        self.last = Some(c.event);
        Ok(Some(c))
    }
}
fn num(v: &Value, key: &str) -> Result<u64> {
    Ok(v[key].as_str().context("missing number")?.parse()?)
}
fn feat(value: f64, event: u64, ordinal: u32) -> Result<FeatureValueV1> {
    let t = Some(DbCounter::new(event).map_err(anyhow::Error::msg)?);
    Ok(FeatureValueV1 {
        value: Some(value),
        missing_reason: None,
        event_ns: t,
        observed_available_ns: None,
        effective_available_ns: t,
        sequence: Some(DbCounter::new(ordinal.into()).map_err(anyhow::Error::msg)?),
    })
}
#[derive(Default)]
struct Pair {
    n: u64,
    mean: [f64; 3],
    m2: [f64; 3],
    cross: [f64; 3],
}
impl Pair {
    fn push(&mut self, x: [f64; 3]) {
        self.n += 1;
        let mut d = [0.0; 3];
        for i in 0..3 {
            d[i] = x[i] - self.mean[i];
            self.mean[i] += d[i] / self.n as f64;
            self.m2[i] += d[i] * (x[i] - self.mean[i]);
        }
        for (k, (i, j)) in [(0, 1), (0, 2), (1, 2)].into_iter().enumerate() {
            self.cross[k] += d[i] * (x[j] - self.mean[j]);
        }
    }
    fn result(&self) -> Value {
        let corr = |k: usize, i: usize, j: usize| {
            if self.m2[i] > 0.0 && self.m2[j] > 0.0 {
                Some(self.cross[k] / self.m2[i].sqrt() / self.m2[j].sqrt())
            } else {
                None
            }
        };
        json!({"count":self.n,"c03_b01":corr(0,0,1),"c03_label":corr(1,0,2),"b01_label":corr(2,1,2)})
    }
}
fn self_check() -> Result<()> {
    let mut p = Pair::default();
    for x in 1..=10 {
        p.push([x as f64, 2.0 * x as f64, -(x as f64)])
    }
    let v = p.result();
    assert!((v["c03_b01"].as_f64().unwrap() - 1.0).abs() < 1e-12);
    assert!((v["c03_label"].as_f64().unwrap() + 1.0).abs() < 1e-12);
    let mut b = [0u8; 40];
    for (i, x) in [
        START,
        1.25f64.to_bits(),
        2f64.to_bits(),
        1,
        (-0.5f64).to_bits(),
    ]
    .iter()
    .enumerate()
    {
        b[i * 8..i * 8 + 8].copy_from_slice(&x.to_le_bytes())
    }
    let c = Cohort::decode(b)?;
    assert_eq!(c.event, START);
    assert_eq!(c.flow, -0.5);
    let dir = tempfile::tempdir()?;
    let path = dir.path().join("sample");
    let mut data = Vec::from(b);
    data.extend_from_slice(&[255; 40]);
    std::fs::write(path, data)?;
    let entry = json!({"path":"sample","byte_start":"0","byte_end":"40","cohort_count":"1"});
    let mut r = Reader::open(dir.path(), &entry)?;
    assert_eq!(r.next()?.unwrap().event, START);
    assert!(r.next()?.is_none());
    let features = [101.0, 100.0, 100.0, 100.0, 2.0, 1.0, 1.0, 1.0]
        .into_iter()
        .map(|x| feat(x, START, 32))
        .collect::<Result<Vec<_>>>()?;
    for (path, expected) in [
        ("mask-correction/C03-corrected.wat", -0.01),
        ("planned-flow-balance-reversion/B01.wat", -0.005),
    ] {
        let module = SignalModule::new(&wat::parse_file(Path::new(BASE).join(path))?, false)?;
        let mut model = FeatureModel::new(&module, Some(1u64), None)?;
        assert!((model.predict(START, START, 32, &features)? - expected).abs() < 1e-12);
    }
    Ok(())
}
fn main() -> Result<()> {
    self_check()?;
    if std::env::args().nth(1).as_deref() == Some("--self-check") {
        println!("self-check passed");
        return Ok(());
    }
    ensure!(
        std::env::args().nth(1).as_deref() == Some("--full"),
        "pass --self-check or --full"
    );
    let root = PathBuf::from(ROOT);
    let manifest: Value = serde_json::from_reader(File::open(root.join("cohort-cache.json"))?)?;
    ensure!(
        manifest["representation"] == "EVENT_TIME_COHORT_FLOW_CACHE_V1"
            && num(&manifest, "event_start_ns")? == START
            && num(&manifest, "event_end_ns")? == END,
        "manifest mismatch"
    );
    let module = |p: &str| -> Result<SignalModule> {
        let bytes = wat::parse_file(Path::new(BASE).join(p))?;
        SignalModule::new(&bytes, false)
    };
    let modules = [
        module("mask-correction/C03-corrected.wat")?,
        module("planned-flow-balance-reversion/B01.wat")?,
    ];
    let mut pairs: [Pair; 2] = std::array::from_fn(|_| Pair::default());
    let mut warmup = [0u64; 2];
    let mut incomplete = [0u64; 2];
    let mut crossing = [0u64; 2];
    let mut entries = manifest["files"]
        .as_array()
        .context("missing files")?
        .clone();
    entries.sort_by_key(|v| v["instrument_id"].as_str().unwrap_or("").to_owned());
    ensure!(entries.len() == 523, "asset count mismatch");
    for entry in &entries {
        let mut rows = Reader::open(&root, entry)?;
        let mut labels = Reader::open(&root, entry)?;
        let count = num(entry, "cohort_count")?;
        if count == 0 {
            ensure!(num(entry, "trade_count")? == 0, "empty asset mismatch");
            continue;
        }
        let mut models = [
            FeatureModel::new(&modules[0], Some(count), None)?,
            FeatureModel::new(&modules[1], Some(count), None)?,
        ];
        let mut fast = ExponentialMovingAverage::new(8, None);
        let mut slow = ExponentialMovingAverage::new(32, None);
        let mut previous: Option<Cohort> = None;
        let mut future: Option<Cohort> = None;
        let mut ordinal = 0u32;
        let mut trades = 0u64;
        while let Some(c) = rows.next()? {
            trades += c.count;
            let part = usize::from(c.event >= TRAIN_END);
            fast.update_raw(c.price);
            slow.update_raw(c.price);
            if let Some(prev) = previous.filter(|_| fast.initialized() && slow.initialized()) {
                let values = [
                    c.price,
                    prev.price,
                    fast.value(),
                    slow.value(),
                    c.quantity,
                    c.count as f64,
                    (c.event - prev.event) as f64,
                    c.flow,
                ];
                let features = values
                    .into_iter()
                    .map(|x| feat(x, c.event, ordinal))
                    .collect::<Result<Vec<_>>>()?;
                let scores = [
                    models[0].predict(c.event, c.event, ordinal, &features)?,
                    models[1].predict(c.event, c.event, ordinal, &features)?,
                ];
                let target = c.event.checked_add(HORIZON).context("event overflow")?;
                while future.is_none_or(|v| v.event < target) {
                    future = labels.next()?;
                    if future.is_none() {
                        break;
                    }
                }
                match future {
                    None => incomplete[part] += 1,
                    Some(v) if v.event >= if part == 0 { TRAIN_END } else { END } => {
                        crossing[part] += 1
                    }
                    Some(v) => {
                        let label = v.price / c.price - 1.0;
                        ensure!(label.is_finite(), "nonfinite label");
                        pairs[part].push([scores[0], scores[1], label])
                    }
                }
            } else {
                warmup[part] += 1
            }
            previous = Some(c);
            ordinal = ordinal.checked_add(1).context("ordinal overflow")?;
        }
        ensure!(
            trades == num(entry, "trade_count")? && ordinal as u64 == count,
            "asset counts mismatch"
        );
    }
    for (i, (count, c, b)) in [
        (311_416_587, 0.2346623154788626, 0.007005206782787033),
        (106_226_928, 0.2725295868723685, 0.008966833378257027),
    ]
    .into_iter()
    .enumerate()
    {
        let v = pairs[i].result();
        ensure!(
            pairs[i].n == count
                && (v["c03_label"].as_f64().context("C03 IC missing")? - c).abs() < 1e-9
                && (v["b01_label"].as_f64().context("B01 IC missing")? - b).abs() < 1e-9,
            "single-model agreement failed"
        );
    }
    let out = json!({"train":pairs[0].result(),"validation":pairs[1].result(),"warmup":warmup,"incomplete":incomplete,"crossing":crossing});
    println!("{}", serde_json::to_string_pretty(&out)?);
    Ok(())
}
