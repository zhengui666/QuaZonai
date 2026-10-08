//! Ordered, replayable transport for already-produced account envelopes.
//! The retained append-only file is the recovery source; there is no second
//! cursor, accounting engine, native event thread, or broker connection here.
use super::{
    commands::Request, preview, service_http, session, write_json, Connection, Failure, Result,
};
use clap::Args;
use contracts::account_observation::{AccountObservationReceiptV1, AccountObservationSubmitV1};
use std::{
    fs::{File, OpenOptions},
    io::{BufRead, BufReader},
    path::PathBuf,
    time::Duration,
};

#[derive(Args)]
pub struct Arguments {
    /// Retained append-only envelope NDJSON, never a pipe. Every record ends with a newline.
    #[arg(long)]
    pub input: PathBuf,
    /// Wait for appended complete records. Never rotates, truncates, or removes input.
    #[arg(long)]
    pub follow: bool,
    /// Attempts per record for unavailable transport or retryable server Problems.
    /// Omitted means retry until receipt, a nonretryable failure, or client stop.
    /// A configured exhaustion stops before the next record; restart with the same file.
    #[arg(long, value_parser = clap::value_parser!(u64).range(1..))]
    pub max_attempts: Option<u64>,
}

struct RetainedInput {
    reader: BufReader<File>,
    identity: same_file::Handle,
    path: PathBuf,
    high_water: u64,
    pending: Vec<u8>,
}

fn regular_file(path: &std::path::Path) -> std::io::Result<File> {
    if !std::fs::metadata(path)?.is_file() {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    open_regular_file(path)
}

/// The path may change after the caller's metadata check. On Unix, a FIFO with
/// no writer must not block this open; recheck the actual opened handle, too.
fn open_regular_file(path: &std::path::Path) -> std::io::Result<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path)?;
    if !file.metadata()?.is_file() {
        return Err(std::io::ErrorKind::InvalidInput.into());
    }
    Ok(file)
}

impl RetainedInput {
    fn open(path: &std::path::Path) -> Result<Self> {
        let file = regular_file(path).map_err(|_| Failure::Input)?;
        let metadata = file.metadata().map_err(|_| Failure::Input)?;
        let identity = same_file::Handle::from_file(file.try_clone().map_err(|_| Failure::Input)?)
            .map_err(|_| Failure::Input)?;
        Ok(Self {
            reader: BufReader::new(file),
            identity,
            path: path.to_owned(),
            high_water: metadata.len(),
            pending: Vec::new(),
        })
    }

    fn unchanged(&mut self) -> Result<()> {
        let file = regular_file(&self.path).map_err(|_| Failure::AccountStreamChanged)?;
        let current =
            same_file::Handle::from_file(file).map_err(|_| Failure::AccountStreamChanged)?;
        let length = self
            .reader
            .get_ref()
            .metadata()
            .map_err(|_| Failure::AccountStreamChanged)?
            .len();
        if current != self.identity || length < self.high_water {
            return Err(Failure::AccountStreamChanged);
        }
        self.high_water = length;
        Ok(())
    }

    /// None is the current EOF, including an unfinished line in follow mode.
    /// Keep the partial bytes for the next append; never submit a partial JSON.
    fn next(&mut self, follow: bool) -> Result<Option<Vec<u8>>> {
        self.unchanged()?;
        self.reader
            .read_until(b'\n', &mut self.pending)
            .map_err(|_| Failure::Input)?;
        if self.pending.ends_with(b"\n") {
            return Ok(Some(std::mem::take(&mut self.pending)));
        }
        if !follow && !self.pending.is_empty() {
            return Err(Failure::AccountStreamIncomplete);
        }
        Ok(None)
    }
}

fn prepare(line: &[u8]) -> Result<(Request, AccountObservationSubmitV1)> {
    // The native binding, sequence/event and complete envelope are the server's
    // atomic replay identity. Keep these exact bytes through every retry.
    Request::account_observation(line)
}

fn retryable(error: &Failure) -> bool {
    matches!(error, Failure::Unavailable)
        || matches!(error, Failure::Rejected(problem) if problem.retryable)
}

async fn submit(
    connection: &Connection,
    request: &Request,
    expected: &AccountObservationSubmitV1,
    max_attempts: Option<u64>,
    retry_delay: Duration,
) -> Result<AccountObservationReceiptV1> {
    if max_attempts == Some(0) {
        return Err(Failure::Input);
    }
    let mut attempt = 1u64;
    loop {
        let result = async {
            let response = connection
                .checked(connection.send(request, None, None).await?, request.status)
                .await?;
            service_http::media(&response, "application/json")?;
            let bytes = service_http::body(response, None).await?;
            let receipt: AccountObservationReceiptV1 =
                service_http::decode(&bytes, &connection.credential)?;
            if receipt.resource.observation != *expected {
                return Err(Failure::Contract);
            }
            Ok(receipt)
        }
        .await;
        match result {
            Err(error) if max_attempts.is_none_or(|maximum| attempt < maximum) && retryable(&error) => {
                // No envelope or credential values in diagnostic output.
                eprintln!("CLI_ACCOUNT_RELAY_RETRY attempt={attempt}");
                tokio::time::sleep(retry_delay).await;
                attempt = attempt.checked_add(1).ok_or(Failure::Input)?;
            }
            result => return result,
        }
    }
}

pub async fn run(arguments: &super::Arguments, relay: &Arguments) -> Result<()> {
    // Account replay belongs to the immutable native envelope, never a file-wide key.
    if arguments.idempotency_key.is_some()
        || arguments.operator_grant.is_some()
        || relay.max_attempts == Some(0)
        || (arguments.preview && relay.follow)
    {
        return Err(Failure::Input);
    }
    let mut input = RetainedInput::open(&relay.input)?;
    if arguments.preview {
        let profile = if arguments.origin.is_none() && arguments.credential_file.is_none() {
            Some(session::Profile::load()?)
        } else {
            None
        };
        let origin = arguments
            .origin
            .as_deref()
            .or_else(|| profile.as_ref().map(|p| p.origin.as_str()))
            .ok_or(Failure::Configuration)?;
        while let Some(line) = input.next(false)? {
            let (request, _) = prepare(&line)?;
            write_json(&preview::inspect(
                &request,
                origin,
                arguments.development_http || profile.as_ref().is_some_and(|p| p.development_http),
                None,
                None,
            )?)?;
        }
        return Ok(());
    }
    let connection = Connection::open(arguments)?;
    loop {
        let Some(line) = input.next(relay.follow)? else {
            if !relay.follow {
                return Ok(());
            }
            tokio::time::sleep(Duration::from_millis(250)).await;
            continue;
        };
        let (request, observation) = prepare(&line)?;
        let receipt = submit(
            &connection,
            &request,
            &observation,
            relay.max_attempts,
            Duration::from_secs(1),
        )
        .await?;
        // Only a validated echo is receipted; output failure stops further sends.
        // Restart replays from the file's beginning, including this last record.
        write_json(&receipt)?;
    }
}

#[cfg(test)]
#[path = "account_transport_tests.rs"]
mod tests;
