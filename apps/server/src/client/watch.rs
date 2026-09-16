//! Native SSE framing with explicit session bounds; disconnect never cancels a Run.
use super::{verify, write_json, Failure, Result};
use contracts::{lifecycle::RunEventV1, DbCounter, Id};
use eventsource_stream::Eventsource;
use futures_util::{StreamExt, TryStreamExt};
use std::time::Duration;

pub(super) fn cursor(run: Id, supplied: Option<&str>) -> Result<DbCounter> {
    let Some(value) = supplied else {
        return Ok(DbCounter::ZERO);
    };
    let (identity, sequence) = value.split_once(':').ok_or(Failure::Input)?;
    let parsed: Id = identity.to_owned().try_into().map_err(|_| Failure::Input)?;
    let sequence: DbCounter = sequence.to_owned().try_into().map_err(|_| Failure::Input)?;
    if parsed != run || value != format!("{run}:{}", sequence.get()) {
        return Err(Failure::Input);
    }
    Ok(sequence)
}

pub(super) async fn stream(
    response: reqwest::Response,
    credential: &str,
    run: Id,
    after: Option<String>,
    seconds: u32,
    events: u32,
) -> Result<()> {
    let mut sequence = cursor(run, after.as_deref())?;
    let mut bytes = 0usize;
    let source = response
        .bytes_stream()
        .map_err(|_| Failure::Unavailable)
        .and_then(move |chunk| {
            bytes = bytes.saturating_add(chunk.len());
            futures_util::future::ready(if bytes > 16 * 1024 * 1024 {
                Err(Failure::ResponseLimit)
            } else {
                Ok(chunk)
            })
        });
    let mut source = source.eventsource();
    let mut received = 0u32;
    let deadline = tokio::time::Instant::now() + Duration::from_secs(u64::from(seconds));
    let reason = loop {
        let next = tokio::select! {
            next = source.next() => next,
            _ = tokio::time::sleep_until(deadline) => break "TIME_BOUND_REACHED",
            _ = tokio::signal::ctrl_c() => break "CLIENT_INTERRUPTED",
        };
        let Some(event) = next else {
            break "CONNECTION_CLOSED";
        };
        let event = event.map_err(|_| Failure::Contract)?;
        if event.event == "reset-required" {
            return Err(Failure::ResetRequired);
        }
        if event.data.len() > 1024 * 1024 {
            return Err(Failure::ResponseLimit);
        }
        verify(event.data.as_bytes(), credential)?;
        let document: RunEventV1 =
            serde_json::from_str(&event.data).map_err(|_| Failure::Contract)?;
        if document.run_id != run
            || document.seq <= sequence
            || event.id != format!("{}:{}", document.run_id, document.seq.get())
            || event.event != document.event_type
        {
            return Err(Failure::Contract);
        }
        sequence = document.seq;
        // The public envelope and native event identity are retained even for an
        // unknown future event_type. This CLI does not infer a new business state.
        write_json(&serde_json::json!({"schema_version":1,"event_id":event.id,"event":document}))?;
        received += 1;
        if received >= events {
            break "EVENT_BOUND_REACHED";
        }
    };
    write_json(&serde_json::json!({
        "schema_version":1,"watch_ended":reason,"run_id":run,
        "last_event_id":format!("{run}:{}", sequence.get()),"events_received":received,
        "cancellation_requested":false
    }))
}
