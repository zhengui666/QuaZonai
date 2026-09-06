//! Durable, scoped Run reads. Axum owns SSE framing and backpressure; there is no
//! in-memory event bus, detached producer or cancellation on browser disconnect.
use crate::{
    access::{idempotency_key, one_header, Authority},
    auth::json,
    error::{ApiError, Problem},
    AppState,
};
use axum::{
    extract::{
        rejection::{JsonRejection, PathRejection, QueryRejection},
        Path, Query, State,
    },
    http::{HeaderMap, StatusCode},
    response::{
        sse::{Event, KeepAlive},
        Sse,
    },
    Json,
};
use contracts::{
    control::{CommandResult, Page},
    lifecycle::{RunCancelV1, RunEventV1, RunListQuery},
    runs::RunSnapshotV1,
    DbCounter, Id,
};
use futures_util::{stream, Stream};
use std::{collections::VecDeque, convert::Infallible, time::Duration};
use store::authority::Actor;
use tokio::{sync::OwnedSemaphorePermit, time::Instant};

fn id(path: Result<Path<Id>, PathRejection>) -> Result<Id, ApiError> {
    path.map(|Path(id)| id).map_err(|_| ApiError::validation())
}

#[utoipa::path(get,path="/api/v2/runs",tag="Runs",params(("project_id"=Option<Id>,Query),("state"=Option<contracts::runs::RunState>,Query),("cursor"=Option<Id>,Query),("limit"=Option<u16>,Query,minimum=1,maximum=100)),responses((status=200,body=Page<RunSnapshotV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=422,body=Problem)))]
pub async fn list(
    State(state): State<AppState>,
    Authority(actor): Authority,
    q: Result<Query<RunListQuery>, QueryRejection>,
) -> Result<Json<Page<RunSnapshotV1>>, ApiError> {
    let Query(query) = q.map_err(|_| ApiError::validation())?;
    Ok(Json(state.store.list_runs(&actor, &query).await?))
}
#[utoipa::path(get,path="/api/v2/runs/{id}",tag="Runs",params(("id"=Id,Path)),responses((status=200,body=RunSnapshotV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem)))]
pub async fn get(
    State(state): State<AppState>,
    Authority(actor): Authority,
    path: Result<Path<Id>, PathRejection>,
) -> Result<Json<RunSnapshotV1>, ApiError> {
    Ok(Json(state.store.get_run(&actor, id(path)?).await?))
}
#[utoipa::path(post,path="/api/v2/runs/{id}/cancel",tag="Runs",params(("id"=Id,Path),("Idempotency-Key"=String,Header)),request_body=RunCancelV1,responses((status=202,body=CommandResult<RunSnapshotV1>),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=409,body=Problem),(status=422,body=Problem)))]
pub async fn cancel(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    path: Result<Path<Id>, PathRejection>,
    body: Result<Json<RunCancelV1>, JsonRejection>,
) -> Result<(StatusCode, Json<CommandResult<RunSnapshotV1>>), ApiError> {
    Ok((
        StatusCode::ACCEPTED,
        Json(
            state
                .store
                .cancel_run(&actor, idempotency_key(&headers)?, id(path)?, &json(body)?)
                .await?,
        ),
    ))
}
fn cursor(headers: &HeaderMap, run: Id) -> Result<DbCounter, ApiError> {
    let Some(value) = one_header(headers, "last-event-id")? else {
        return Ok(DbCounter::ZERO);
    };
    let (identity, seq) = value.split_once(':').ok_or_else(ApiError::validation)?;
    if value.len() > 56 || Id::try_from(identity.to_owned()).ok() != Some(run) {
        return Err(ApiError::validation());
    }
    DbCounter::try_from(seq.to_owned()).map_err(|_| ApiError::validation())
}
struct Reading {
    state: AppState,
    actor: Actor,
    run: Id,
    cursor: DbCounter,
    pending: VecDeque<RunEventV1>,
    terminal: bool,
    expires: Instant,
    _permit: OwnedSemaphorePermit,
}
#[utoipa::path(get,path="/api/v2/runs/{id}/events",tag="Runs",params(("id"=Id,Path),("Last-Event-ID"=Option<String>,Header,description="Exact run UUID, colon, canonical decimal sequence; omitted starts at zero.")),responses((status=200,description="Durable RunEventV1 frames. Disconnect never cancels a run. Reconnect with the last emitted event ID.",content_type="text/event-stream",body=RunEventV1),(status=401,body=Problem),(status=403,body=Problem),(status=404,body=Problem),(status=410,body=Problem),(status=409,body=Problem),(status=422,body=Problem),(status=429,body=Problem)))]
pub async fn events(
    State(state): State<AppState>,
    Authority(actor): Authority,
    headers: HeaderMap,
    path: Result<Path<Id>, PathRejection>,
) -> Result<Sse<impl Stream<Item = Result<Event, Infallible>>>, ApiError> {
    let run = id(path)?;
    let cursor = cursor(&headers, run)?;
    let permit = state
        .run_stream_slots
        .clone()
        .try_acquire_owned()
        .map_err(|_| {
            ApiError::new(
                StatusCode::TOO_MANY_REQUESTS,
                "STREAM_LIMIT",
                "运行事件连接数量已达上限。",
            )
        })?;
    // Fail before HTTP streaming begins, including expired cursor and authority.
    let first = state.store.run_events(&actor, run, cursor, 16).await?;
    let complete = first.state.is_terminal()
        && first.events.last().map_or(cursor, |event| event.seq) == first.last_event_seq;
    let reading = Reading {
        state,
        actor,
        run,
        cursor,
        pending: first.events.into(),
        terminal: complete,
        expires: Instant::now() + Duration::from_secs(60),
        _permit: permit,
    };
    let stream = stream::unfold(Some(reading), |context| async move {
        let mut context = context?;
        loop {
            if Instant::now() >= context.expires {
                return None;
            }
            if let Some(item) = context.pending.pop_front() {
                let frame = Event::default()
                    .id(format!("{}:{}", item.run_id, item.seq.get()))
                    .event(&item.event_type)
                    .json_data(&item);
                match frame {
                    Ok(frame) => {
                        context.cursor = item.seq;
                        return Some((Ok(frame), Some(context)));
                    }
                    Err(_) => {
                        return Some((
                            Ok(Event::default()
                                .event("reset-required")
                                .data("{\"code\":\"CONTRACT_VERSION_UNSUPPORTED\"}")),
                            None,
                        ))
                    }
                }
            }
            if context.terminal {
                return None;
            }
            tokio::time::sleep(Duration::from_millis(500)).await;
            let query =
                context
                    .state
                    .store
                    .run_events(&context.actor, context.run, context.cursor, 16);
            match tokio::time::timeout_at(context.expires, query).await {
                Ok(Ok(batch)) => {
                    context.terminal = batch.state.is_terminal()
                        && batch
                            .events
                            .last()
                            .map_or(context.cursor, |event| event.seq)
                            == batch.last_event_seq;
                    context.pending = batch.events.into();
                }
                Err(_) => return None,
                Ok(Err(_)) => {
                    // No raw Store/SQL/provider errors or new cursor is exposed.
                    return Some((
                        Ok(Event::default()
                            .event("reset-required")
                            .data("{\"code\":\"RELOAD_AND_REAUTHENTICATE\"}")),
                        None,
                    ));
                }
            }
        }
    });
    Ok(Sse::new(stream).keep_alive(KeepAlive::new().interval(Duration::from_secs(10))))
}
