//! Optional observation of original native account events for in-flight previews.
//! The final native result remains authoritative; preview I/O cannot alter a run.
use nautilus_common::msgbus::{self, TypedHandler};
use nautilus_model::events::PortfolioSnapshot;
use std::{
    cell::RefCell,
    fs::{File, OpenOptions},
    io::Write,
    rc::Rc,
    time::{Duration, Instant},
};

fn snapshot_record(snapshot: &PortfolioSnapshot) -> serde_json::Result<serde_json::Value> {
    let mut value = serde_json::to_value(snapshot)?;
    // Official UnixNanos serializes as u64; send its exact decimal representation
    // so JSON consumers cannot round nanoseconds through an IEEE-754 number.
    value["ts_event"] = snapshot.ts_event.to_string().into();
    value["ts_init"] = snapshot.ts_init.to_string().into();
    Ok(serde_json::json!({ "snapshot": value }))
}

struct Writer {
    file: Option<File>,
    last_write: Option<Instant>,
    pending: Option<PortfolioSnapshot>,
}

impl Writer {
    fn observe(&mut self, snapshot: &PortfolioSnapshot) {
        if self.file.is_none() {
            return;
        }
        // Throttle using wall time, never simulation time: a fast replay should
        // not spend most of its runtime serializing a duplicate result channel.
        if self
            .last_write
            .is_some_and(|last| last.elapsed() < Duration::from_millis(250))
        {
            self.pending = Some(snapshot.clone());
            return;
        }
        self.pending = None;
        self.write(snapshot);
    }

    fn write(&mut self, snapshot: &PortfolioSnapshot) {
        let Some(file) = &mut self.file else { return };
        let written = snapshot_record(snapshot)
            .and_then(|record| serde_json::to_vec(&record))
            .and_then(|mut bytes| {
                bytes.push(b'\n');
                file.write_all(&bytes).map_err(serde_json::Error::io)
            });
        if written.is_err() {
            // Preview availability must not change original scientific output.
            self.file = None;
        } else {
            self.last_write = Some(Instant::now());
        }
    }

    fn finish(&mut self) {
        if let Some(snapshot) = self.pending.take() {
            self.write(&snapshot);
        }
    }
}

pub(crate) struct LiveProgress {
    handler: TypedHandler<PortfolioSnapshot>,
    writer: Rc<RefCell<Writer>>,
}

impl LiveProgress {
    /// The execution host supplies an isolated scratch path. No path or values
    /// are written to stdout/stderr, which retain their native transport roles.
    pub(crate) fn attach() -> Option<Self> {
        let path = std::env::var_os("QZ_PROGRESS_PATH")?;
        let file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .ok()?;
        let writer = Rc::new(RefCell::new(Writer {
            file: Some(file),
            last_write: None,
            pending: None,
        }));
        let captured = writer.clone();
        let handler = TypedHandler::from(move |snapshot: &PortfolioSnapshot| {
            captured.borrow_mut().observe(snapshot);
        });
        msgbus::subscribe_portfolio_snapshot("events.portfolio.*".into(), handler.clone(), None);
        Some(Self { handler, writer })
    }
}

impl Drop for LiveProgress {
    fn drop(&mut self) {
        msgbus::unsubscribe_portfolio_snapshot("events.portfolio.*".into(), &self.handler);
        self.writer.borrow_mut().finish();
    }
}

#[cfg(test)]
#[path = "../tests/support/native_portfolio.rs"]
mod native_portfolio;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preview_preserves_original_native_snapshots_and_flushes_last_point() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("preview.ndjson");
        let mut writer = Writer {
            file: Some(File::create(&path).unwrap()),
            last_write: None,
            pending: None,
        };
        let mut account = native_portfolio::CashPortfolio::new();
        let first = account
            .portfolio
            .build_snapshot(&account.account_id)
            .unwrap();
        writer.observe(&first);
        account.advance_account();
        let last = account
            .portfolio
            .build_snapshot(&account.account_id)
            .unwrap();
        writer.observe(&last);
        assert!(writer.pending.is_some(), "fast snapshots are batched");
        writer.finish();
        let encoded = std::fs::read_to_string(path).unwrap();
        let records: Vec<serde_json::Value> = encoded
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect();
        assert_eq!(records.len(), 2);
        for (record, snapshot) in records.iter().zip([first, last]) {
            let mut original = serde_json::to_value(&snapshot).unwrap();
            original["ts_event"] = snapshot.ts_event.to_string().into();
            original["ts_init"] = snapshot.ts_init.to_string().into();
            assert_eq!(record["snapshot"], original);
        }
        assert!(writer.pending.is_none());
    }

    #[test]
    fn unavailable_preview_output_does_not_interrupt_native_observation() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("read-only-handle");
        File::create(&path).unwrap();
        let mut writer = Writer {
            file: Some(File::open(path).unwrap()),
            last_write: None,
            pending: None,
        };
        let mut account = native_portfolio::CashPortfolio::new();
        let snapshot = account
            .portfolio
            .build_snapshot(&account.account_id)
            .unwrap();
        writer.observe(&snapshot);
        assert!(writer.file.is_none());
        writer.observe(&snapshot);
        writer.finish();
    }
}
