//! Native SSE framing with user-selected session bounds; disconnect never cancels a Run.
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
    seconds: Option<u32>,
    events: Option<u64>,
) -> Result<()> {
    if seconds == Some(0) || events == Some(0) {
        return Err(Failure::Input);
    }
    let mut sequence = cursor(run, after.as_deref())?;
    let source = response
        .bytes_stream()
        .map_err(|_| Failure::Unavailable);
    let mut source = source.eventsource();
    let mut received = 0u64;
    let deadline = async move {
        match seconds {
            Some(seconds) => tokio::time::sleep(Duration::from_secs(u64::from(seconds))).await,
            None => std::future::pending::<()>().await,
        }
    };
    tokio::pin!(deadline);
    let reason = loop {
        let next = tokio::select! {
            next = source.next() => next,
            _ = &mut deadline => break "TIME_BOUND_REACHED",
            _ = tokio::signal::ctrl_c() => break "CLIENT_INTERRUPTED",
        };
        let Some(event) = next else {
            break "CONNECTION_CLOSED";
        };
        let event = event.map_err(|_| Failure::Contract)?;
        if event.event == "reset-required" {
            return Err(Failure::ResetRequired);
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
        received = received.checked_add(1).ok_or(Failure::Contract)?;
        if events.is_some_and(|maximum| received >= maximum) {
            break "EVENT_BOUND_REACHED";
        }
    };
    write_json(&serde_json::json!({
        "schema_version":1,"watch_ended":reason,"run_id":run,
        "last_event_id":format!("{run}:{}", sequence.get()),"events_received":received,
        "cancellation_requested":false
    }))
}
