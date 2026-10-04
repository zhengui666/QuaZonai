//! Convert official native PortfolioSnapshot JSON into Q intake envelopes.
//! No credentials, network, account connection or execution engine is started.
use crate::account_observer::NativeAccountObserver;
use anyhow::{anyhow, bail, Result};
use clap::Parser;
use contracts::{account_observation::*, DbCounter};
use nautilus_model::events::PortfolioSnapshot;
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, Read, Write},
};

pub(crate) const MAX_RECORD_BYTES: usize = 2 * 1024 * 1024;

#[derive(Parser)]
pub struct Args {
    /// Trusted downstream-owned binding JSON; contains no exchange credentials.
    #[arg(long)]
    binding: std::path::PathBuf,
    /// Last persisted source sequence; zero only for a new observer session.
    #[arg(long)]
    last_sequence: String,
    #[arg(long)]
    dropped_events: String,
    /// Read newline-complete native snapshot NDJSON until EOF. Keep the native input for recovery.
    /// Runs outside the native event thread; output is synced before accepting the next snapshot.
    #[arg(long, requires = "output")]
    stream: bool,
    /// Create a NEW retained envelope NDJSON segment; never overwrites or appends an existing file.
    /// Reuse the exact existing session/cursor only if known; lost cursor requires a new binding session.
    #[arg(long, requires = "stream")]
    output: Option<std::path::PathBuf>,
}

pub(crate) fn now() -> Result<DbCounter> {
    let nanos = chrono::Utc::now()
        .timestamp_nanos_opt()
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| anyhow!("unsupported wall clock"))?;
    DbCounter::new(nanos).map_err(|e| anyhow!(e))
}

fn envelope(
    observer: &NativeAccountObserver,
    bytes: &[u8],
    observed_at: DbCounter,
) -> Result<AccountObservationSubmitV1> {
    let snapshot: PortfolioSnapshot = serde_json::from_slice(bytes)?;
    let (sender, receiver) = std::sync::mpsc::sync_channel(1);
    observer.try_emit(
        Some(&snapshot),
        observed_at,
        AccountConnectionV1::Unknown,
        &sender,
    )?;
    Ok(receiver.try_recv()?)
}

pub(crate) fn new_segment(path: &std::path::Path) -> Result<File> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let file = options.open(path)?;
    file.sync_all()?;
    #[cfg(unix)]
    {
        // Persist the new directory entry as well as subsequent file contents.
        let parent = path
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty())
            .unwrap_or_else(|| std::path::Path::new("."));
        File::open(parent)?.sync_all()?;
    }
    Ok(file)
}

/// The single retained-envelope format shared by stdin conversion and typed node events.
pub(crate) fn retain_envelope(
    output: &mut File,
    envelope: &AccountObservationSubmitV1,
) -> Result<()> {
    let mut bytes = serde_json::to_vec(envelope)?;
    bytes.push(b'\n');
    if bytes.len() > MAX_RECORD_BYTES {
        bail!("projected envelope exceeds relay limit; retain original input and segment");
    }
    output.write_all(&bytes)?;
    output.sync_data()?;
    Ok(())
}

fn stream(
    input: &mut impl BufRead,
    output: &mut File,
    observer: &NativeAccountObserver,
    now: impl Fn() -> Result<DbCounter>,
) -> Result<()> {
    loop {
        let mut line = Vec::new();
        Read::by_ref(input)
            .take(MAX_RECORD_BYTES as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if line.is_empty() {
            return Ok(());
        }
        if line.len() > MAX_RECORD_BYTES {
            bail!("native snapshot record exceeds limit; retain original input and segment");
        }
        if !line.ends_with(b"\n") {
            bail!("incomplete native snapshot record; retain original input and segment");
        }
        retain_envelope(output, &envelope(observer, &line, now()?)?)?;
    }
}

pub fn run(args: Args) -> Result<()> {
    let binding: NativeAccountBindingV1 = serde_json::from_slice(&std::fs::read(args.binding)?)?;
    let observer = NativeAccountObserver::new(
        binding,
        DbCounter::try_from(args.last_sequence).map_err(|e| anyhow!(e))?,
        DbCounter::try_from(args.dropped_events).map_err(|e| anyhow!(e))?,
    )?;
    if args.stream {
        let mut output = new_segment(&args.output.ok_or_else(|| anyhow!("output required"))?)?;
        return stream(&mut std::io::stdin().lock(), &mut output, &observer, now);
    }
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(MAX_RECORD_BYTES as u64 + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() > MAX_RECORD_BYTES {
        bail!("native snapshot input exceeds limit");
    }
    let mut bytes = serde_json::to_vec(&envelope(&observer, &bytes, now()?)?)?;
    bytes.push(b'\n');
    let mut stdout = std::io::stdout().lock();
    stdout.write_all(&bytes)?;
    stdout.flush()?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_core::{UnixNanos, UUID4};
    use nautilus_model::{enums::AccountType, identifiers::AccountId, types::Money};

    fn observer() -> NativeAccountObserver {
        let fixture: AccountObservationSubmitV1 = serde_json::from_str(include_str!(
            "../../../tests/fixtures/account-observations/paper-snapshot.json"
        ))
        .unwrap();
        NativeAccountObserver::new(fixture.binding, DbCounter::ZERO, DbCounter::ZERO).unwrap()
    }

    fn snapshot() -> PortfolioSnapshot {
        PortfolioSnapshot::new(
            AccountId::new("SIM-001"),
            AccountType::Cash,
            None,
            vec![],
            vec![],
            vec![],
            vec![],
            vec![Money::from("0.12345678 USDT")],
            None,
            false,
            vec![],
            vec![],
            vec![],
            UUID4::new(),
            UnixNanos::from(1),
            UnixNanos::from(2),
        )
    }

    #[test]
    fn stream_persists_native_records_without_connection_or_clock_invention() {
        let first = snapshot();
        let second = snapshot();
        let raw = format!(
            "{}\n{}\n",
            serde_json::to_string(&first).unwrap(),
            serde_json::to_string(&second).unwrap()
        );
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("observations.ndjson");
        let mut output = new_segment(&path).unwrap();
        let observer = observer();
        stream(&mut raw.as_bytes(), &mut output, &observer, || {
            Ok(DbCounter::new(3).unwrap())
        })
        .unwrap();
        let stored = std::fs::read_to_string(&path).unwrap();
        let records: Vec<AccountObservationSubmitV1> = stored
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records.len(), 2);
        for (index, (record, native)) in records.iter().zip([first, second]).enumerate() {
            assert_eq!(record.sequence.get(), index as u64 + 1);
            assert_eq!(record.connection, AccountConnectionV1::Unknown);
            assert_eq!(record.observed_at_ns.get(), 3);
            assert_eq!(
                record.snapshot.as_ref().unwrap(),
                &crate::account_observer::project_snapshot(&native).unwrap()
            );
            assert!(record.snapshot.as_ref().unwrap().realized_pnls.is_empty());
        }
        assert!(
            new_segment(&path).is_err(),
            "never overwrite a recovery source"
        );
        assert_eq!(std::fs::read_to_string(path).unwrap(), stored);
    }

    #[test]
    fn incomplete_or_oversize_native_record_stops_without_allocating_a_sequence() {
        let directory = tempfile::tempdir().unwrap();
        for (index, raw) in [
            serde_json::to_vec(&snapshot()).unwrap(),
            vec![b' '; MAX_RECORD_BYTES + 1],
        ]
        .into_iter()
        .enumerate()
        {
            let observer = observer();
            let path = directory.path().join(index.to_string());
            let mut output = new_segment(&path).unwrap();
            assert!(stream(&mut raw.as_slice(), &mut output, &observer, || {
                Ok(DbCounter::new(3).unwrap())
            })
            .is_err());
            assert_eq!(observer.cursor().0, DbCounter::ZERO);
            assert!(std::fs::read(path).unwrap().is_empty());
        }
        let observer = observer();
        let path = directory.path().join("complete-prefix.ndjson");
        let mut output = new_segment(&path).unwrap();
        let raw = format!("{}\n{{", serde_json::to_string(&snapshot()).unwrap());
        assert!(stream(&mut raw.as_bytes(), &mut output, &observer, || {
            Ok(DbCounter::new(3).unwrap())
        })
        .is_err());
        assert_eq!(observer.cursor().0.get(), 1);
        let retained = std::fs::read_to_string(path).unwrap();
        assert!(retained.ends_with('\n'));
        let retained: AccountObservationSubmitV1 = serde_json::from_str(&retained).unwrap();
        assert_eq!(retained.sequence.get(), 1);
    }
}
