//! Explicitly selected engineering fixture. Real Poller/NativeInbox, real Server/PG
//! receipts and the existing production Paper engine. The old fixture supplies
//! synthetic quotes/target; this does not run Serve's data host or qualify research.
//! There are no handwritten native commands, receipts, fills, accounts or fees.
use super::*;
use anyhow::{Result, anyhow, ensure};
use contracts::control::CommandResult;
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    io::{Read, Write},
    path::PathBuf,
    rc::Rc,
    sync::{
        Arc, Mutex,
        atomic::{AtomicUsize, Ordering},
    },
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    origin: String,
    token: String,
    browser_cookie: String,
    project: Id,
    downstream: Id,
    scenario: String,
    result: PathBuf,
}
#[derive(Clone, Serialize)]
struct Attempt {
    method: String,
    status: u16,
    path: String,
    key: Option<String>,
    body: Vec<u8>,
    response: Value,
    withheld: Option<String>,
}
#[derive(Clone)]
struct Proxy {
    origin: String,
    http: reqwest::Client,
    attempts: Arc<Mutex<Vec<Attempt>>>,
    active: Arc<AtomicUsize>,
    trace: Arc<Mutex<Option<std::fs::File>>>,
}
// Independent failure evidence only. No headers, stdin, token or configuration
// enter this trace. A broken diagnostic sink must not change the business result.
fn trace(proxy: &Proxy, value: Value) {
    if let Ok(mut file) = proxy.trace.lock() {
        if let Some(file) = file.as_mut() {
            if let Err(error) = writeln!(file, "{value}") {
                eprintln!("paper_http_trace_write_failed:{error}");
            }
        }
    }
}

struct InFlight(Arc<AtomicUsize>);
impl Drop for InFlight {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

async fn forward(
    axum::extract::State(proxy): axum::extract::State<Proxy>,
    request: axum::extract::Request,
) -> Result<axum::response::Response, axum::http::StatusCode> {
    use axum::{body::Body, http::StatusCode};
    proxy.active.fetch_add(1, Ordering::SeqCst);
    let _in_flight = InFlight(proxy.active.clone());
    let requested_at = chrono::Utc::now();
    let method = request.method().clone();
    let path = request.uri().to_string();
    let headers = request.headers().clone();
    let body = axum::body::to_bytes(request.into_body(), 8 * 1024 * 1024)
        .await
        .map_err(|_| StatusCode::BAD_REQUEST)?;
    trace(
        &proxy,
        json!({"event":"request", "at":requested_at, "method":method.as_str(), "path":path, "body":String::from_utf8_lossy(&body)}),
    );
    let mut outgoing = proxy
        .http
        .request(method.clone(), format!("{}{path}", proxy.origin));
    // No credentials are saved in the trace; only forward the test connection's
    // original auth to the same explicitly selected disposable Server.
    for name in ["authorization", "content-type", "idempotency-key"] {
        if let Some(value) = headers.get(name) {
            outgoing = outgoing.header(name, value);
        }
    }
    let upstream = outgoing
        .body(body.to_vec())
        .send()
        .await
        .map_err(|error| {
            trace(&proxy, json!({"event":"transport_failed", "at":chrono::Utc::now(), "requested_at":requested_at, "method":method.as_str(), "path":path, "timeout":error.is_timeout(), "connect":error.is_connect()}));
            StatusCode::BAD_GATEWAY
        })?;
    let status = upstream.status();
    let mut response_headers = upstream.headers().clone();
    let original = upstream
        .bytes()
        .await
        .map_err(|_| StatusCode::BAD_GATEWAY)?;
    // Preserve the original successful OR rejected reply before the deliberate
    // test-only response loss. This also survives a later fixture timeout.
    trace(
        &proxy,
        json!({"event":"response", "at":chrono::Utc::now(), "requested_at":requested_at, "method":method.as_str(), "path":path, "status":status.as_u16(), "body":String::from_utf8_lossy(&original)}),
    );
    let mut returned = original.to_vec();
    if method == axum::http::Method::POST
        || (method == axum::http::Method::GET
            && path.starts_with("/api/v2/downstream/capital-exit"))
    {
        let kind = if method != axum::http::Method::POST || !status.is_success() {
            None
        } else if path.ends_with("/client-account-observations") {
            Some("registration_header")
        } else if path.ends_with("/claim") {
            Some("claim_reply")
        } else if path.ends_with("/evidence") {
            Some("fence_reply")
        } else {
            None
        };
        let mut attempts = proxy.attempts.lock().unwrap();
        let withheld =
            kind.filter(|kind| !attempts.iter().any(|a| a.withheld.as_deref() == Some(kind)));
        if let Some(kind) = withheld {
            trace(
                &proxy,
                json!({"event":"withheld", "at":chrono::Utc::now(), "requested_at":requested_at, "method":method.as_str(), "path":path, "kind":kind}),
            );
            if kind == "registration_header" {
                // Native source must stay unbound after a real accepted intake
                // whose registration acknowledgement did not reach the caller.
                response_headers.remove("x-qz-capital-exit-source");
            } else {
                // Commit first in real PG, then truncate only the returning body.
                // The actual receipt remains in `response` for replay checks.
                returned = b"{".to_vec();
                response_headers.remove("content-length");
            }
        }
        attempts.push(Attempt {
            method: method.to_string(),
            status: status.as_u16(),
            path: path.clone(),
            key: headers
                .get("idempotency-key")
                .and_then(|v| v.to_str().ok())
                .map(str::to_owned),
            body: body.to_vec(),
            response: serde_json::from_slice(&original).map_err(|_| StatusCode::BAD_GATEWAY)?,
            withheld: withheld.map(str::to_owned),
        });
    }
    let mut response = axum::response::Response::new(Body::from(returned));
    *response.status_mut() = status;
    *response.headers_mut() = response_headers;
    Ok(response)
}

async fn browser<T: DeserializeOwned>(
    input: &Input,
    http: &reqwest::Client,
    path: &str,
    key: &str,
    body: &impl Serialize,
) -> Result<CommandResult<T>> {
    let response = http
        .post(format!("{}{path}", input.origin))
        .header("origin", &input.origin)
        .header("cookie", &input.browser_cookie)
        .header("Idempotency-Key", key)
        .json(body)
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "paper_browser_status:{}",
        response.status().as_u16()
    );
    Ok(response.json().await?)
}
async fn view(input: &Input, http: &reqwest::Client, id: Id) -> Result<CapitalExitViewV1> {
    let response = http
        .get(format!("{}/api/v2/capital-exits/{id}", input.origin))
        .header("cookie", &input.browser_cookie)
        .send()
        .await?;
    ensure!(
        response.status().is_success(),
        "paper_read_status:{}",
        response.status().as_u16()
    );
    Ok(response.json().await?)
}
async fn resume_late(
    input: &Input,
    http: &reqwest::Client,
    id: Id,
    command: Id,
    preview: &CapitalExitPreviewV1,
    prefix: &str,
) -> Result<(CommandResult<CapitalExitViewV1>, Vec<Value>)> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(5);
    let mut conflicts = Vec::new();
    for attempt in 0..16 {
        let current = view(input, http, id).await?;
        ensure!(
            current.command_id == command && current.account_source_id == preview.account_source_id,
            "late_resume_original_intent_changed_before_browser_action"
        );
        let request = CapitalExitActionV1::Resume {
            schema_version: SchemaV1,
            expected_revision: current.revision,
            preview_id: preview.id,
        };
        let key = format!("{prefix}-{attempt}-{}", current.revision.get());
        let response = http
            .post(format!("{}/api/v2/capital-exits/{id}/resume", input.origin))
            .header("origin", &input.origin)
            .header("cookie", &input.browser_cookie)
            .header("Idempotency-Key", &key)
            .json(&request)
            .send()
            .await?;
        let status = response.status();
        if status.is_success() {
            return Ok((response.json().await?, conflicts));
        }
        let problem: Value = response.json().await?;
        // Availability can legitimately advance the intent revision between the
        // browser's GET and POST. Retry only that exact conflict, with the new
        // real revision and a new command key. Do not pause the producer/Poller,
        // suppress any other rejection, or change a timed-out write identity.
        ensure!(
            status.as_u16() == 409 && problem["code"] == "REVISION_CONFLICT",
            "late_resume_rejected:{}:{}",
            status.as_u16(),
            problem["code"]
        );
        conflicts.push(json!({"key":key,"request":request,"response":problem}));
        ensure!(
            tokio::time::Instant::now() < deadline,
            "late_resume_revision_never_stable"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Err(anyhow!("late_resume_revision_retry_limit"))
}

async fn wait(label: &str, mut ready: impl FnMut() -> bool) -> Result<()> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    while !ready() {
        ensure!(
            tokio::time::Instant::now() < deadline,
            "paper_acceptance_timeout:{label}"
        );
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    Ok(())
}
fn latest_receipt(proxy: &Proxy) -> Option<AccountObservationReceiptV2> {
    proxy
        .attempts
        .lock()
        .unwrap()
        .iter()
        .rev()
        .find(|a| {
            a.path.ends_with("/client-account-observations")
                && (200..300).contains(&a.status)
                && a.withheld.is_none()
        })
        .map(|a| serde_json::from_value(a.response.clone()).unwrap())
}
fn issued(fixture: &Fixture) -> Vec<String> {
    fixture
        .session
        .capital_exit
        .as_ref()
        .unwrap()
        .gate()
        .issued_order_ids()
}
fn native_snapshot(fixture: &Fixture) -> Value {
    let cache = fixture.session.engine.kernel().cache.borrow();
    // OrderRef is a borrowed cache guard, not a serializable native DTO.
    // Preserve each original order identity and complete official event history.
    let mut orders: Vec<_> = cache
        .orders(None, None, None, None, None)
        .into_iter()
        .map(|order| {
            (
                order.client_order_id().to_string(),
                serde_json::to_value(order.events()).unwrap(),
            )
        })
        .collect();
    orders.sort_by(|left, right| left.0.cmp(&right.0));
    json!({
        "clock": fixture.session.engine.kernel().clock.borrow().timestamp_ns().as_u64(),
        "quote": cache.quote(&InstrumentId::from(polymarket::IDS[0])),
        "original_frames": fixture.session.original_frames,
        "orders": orders,
        "position": fixture.position().to_string(),
        "free_cash": fixture.free().to_string(),
        "issued": issued(fixture),
    })
}
fn retained_progress(
    root: &std::path::Path,
    command: Id,
    phase: CapitalExitStateV1,
) -> Option<CapitalExitOwnerEvidenceV1> {
    std::fs::read_dir(root).ok()?.filter_map(|entry| entry.ok())
        .filter_map(|entry| std::fs::read(entry.path()).ok())
        .filter_map(|bytes| serde_json::from_slice::<CapitalExitOwnerEvidenceV1>(&bytes).ok())
        .find(|value| value.command_id == command && matches!(&value.evidence, CapitalExitEvidenceKindV1::NativeProgress { phase: found, released_cash_amount: None, .. } if *found == phase))
}
fn accepted_progress(proxy: &Proxy, command: Id, phase: CapitalExitStateV1) -> bool {
    proxy.attempts.lock().unwrap().iter().any(|attempt| {
        attempt.path.ends_with("/evidence") && (200..300).contains(&attempt.status) && attempt.withheld.is_none()
            && serde_json::from_slice::<CapitalExitOwnerEvidenceV1>(&attempt.body).is_ok_and(|value|
                value.command_id == command && matches!(&value.evidence, CapitalExitEvidenceKindV1::NativeProgress { phase: found, .. } if *found == phase))
    })
}
fn discovery_after(proxy: &Proxy, index: usize) -> bool {
    proxy
        .attempts
        .lock()
        .unwrap()
        .iter()
        .skip(index)
        .any(|attempt| {
            attempt.method == "GET"
                && (200..300).contains(&attempt.status)
                && attempt
                    .path
                    .starts_with("/api/v2/downstream/capital-exit-assessments")
        })
}
fn availability_count(proxy: &Proxy, command: Id) -> usize {
    proxy
        .attempts
        .lock()
        .unwrap()
        .iter()
        .filter(|attempt| {
            attempt.method == "POST"
                && attempt.path.ends_with("/evidence")
                && (200..300).contains(&attempt.status)
                && attempt.withheld.is_none()
                && serde_json::from_slice::<CapitalExitOwnerEvidenceV1>(&attempt.body).is_ok_and(
                    |value| {
                        value.command_id == command
                            && matches!(
                                value.evidence,
                                CapitalExitEvidenceKindV1::WithdrawabilityObserved { .. }
                            )
                    },
                )
        })
        .count()
}
fn terminal_progress_attempts(proxy: &Proxy, command: Id, phase: CapitalExitStateV1) -> usize {
    proxy.attempts.lock().unwrap().iter().filter(|attempt| {
        attempt.method == "POST" && attempt.path.ends_with("/evidence")
            && serde_json::from_slice::<CapitalExitOwnerEvidenceV1>(&attempt.body).is_ok_and(|value|
                value.command_id == command && matches!(value.evidence, CapitalExitEvidenceKindV1::NativeProgress { phase: found, .. } if found == phase))
    }).count()
}

fn after_terminal_ack(proxy: &Proxy, command: Id, phase: CapitalExitStateV1) -> Option<usize> {
    proxy.attempts.lock().unwrap().iter().position(|attempt| {
        attempt.method == "POST" && attempt.path.ends_with("/evidence")
            && (200..300).contains(&attempt.status) && attempt.withheld.is_none()
            && serde_json::from_slice::<CapitalExitOwnerEvidenceV1>(&attempt.body).is_ok_and(|value|
                value.command_id == command && matches!(value.evidence, CapitalExitEvidenceKindV1::NativeProgress { phase: found, .. } if found == phase))
    }).map(|index|index+1)
}

fn quote_matches(fixture: &Fixture, bid: &str, size: &str) -> bool {
    fixture
        .session
        .engine
        .kernel()
        .cache
        .borrow()
        .quote(&InstrumentId::from(polymarket::IDS[0]))
        .is_some_and(|quote| {
            quote.bid_price == Price::from(bid) && quote.bid_size == Quantity::from(size)
        })
}

fn request(receipt: &AccountObservationReceiptV2) -> CapitalExitPreviewRequestV1 {
    CapitalExitPreviewRequestV1 {
        schema_version: SchemaV1,
        account_source_id: receipt.resource.source_id,
        expected_source_observation_id: receipt.resource.id,
        scope: CapitalExitScopeV1::Amount {
            amount: "850".parse().unwrap(),
            currency: "pUSD".into(),
        },
        policy: CapitalExitPolicyV1::BoundedLimit {
            deadline: chrono::Utc::now() + chrono::Duration::seconds(50),
            legs: vec![CapitalExitReductionLegV1 {
                instrument_id: polymarket::IDS[0].into(),
                maximum_reduction_quantity: "200".parse().unwrap(),
                minimum_sell_price: "0.49".parse().unwrap(),
            }],
            max_execution_cost: CapitalExitCostLimitV1 {
                amount: "5".parse().unwrap(),
                currency: "pUSD".into(),
                reference_evidence_id: receipt.resource.id,
            },
        },
    }
}
async fn supported_preview(
    input: &Input,
    http: &reqwest::Client,
    proxy: &Proxy,
    label: &str,
) -> Result<CapitalExitPreviewV1> {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(12);
    let mut attempt = 0;
    let mut original_request: Option<CapitalExitPreviewRequestV1> = None;
    loop {
        let receipt =
            latest_receipt(proxy).ok_or_else(|| anyhow!("original_registered_receipt_missing"))?;
        if original_request
            .as_ref()
            .is_none_or(|r| r.expected_source_observation_id != receipt.resource.id)
        {
            original_request = Some(request(&receipt));
        }
        let response: CommandResult<CapitalExitPreviewV1> = browser(
            input,
            http,
            &format!("/api/v2/projects/{}/capital-exit-previews", input.project),
            &format!("{label}-{attempt}"),
            original_request.as_ref().unwrap(),
        )
        .await?;
        if attempt == 0 && label == "bootstrap" {
            ensure!(
                response.resource.capability == CapitalExitCapabilityV1::Blocked
                    && response
                        .resource
                        .reason_codes
                        .iter()
                        .any(|code| code == "capital_exit_owner_assessment_unavailable")
                    && response.resource.valid_until <= chrono::Utc::now(),
                "bootstrap_must_be_actual_expired_blocked_store_preview"
            );
        }
        if response.resource.capability == CapitalExitCapabilityV1::Supported {
            return Ok(response.resource);
        }
        // A pending preview may have valid_until == now. Keep the production
        // Poller responsible for assessment; never call the native owner here.
        ensure!(
            tokio::time::Instant::now() < deadline,
            "paper_pending_assessment_not_serviced:{label}:{:?}",
            response.resource.reason_codes
        );
        attempt += 1;
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
fn assert_replays(proxy: &Proxy) {
    let attempts = proxy.attempts.lock().unwrap();
    for kind in ["registration_header", "claim_reply", "fence_reply"] {
        let (index, lost) = attempts
            .iter()
            .enumerate()
            .find(|(_, a)| a.withheld.as_deref() == Some(kind))
            .unwrap();
        let replay = attempts
            .iter()
            .skip(index + 1)
            .find(|a| a.path == lost.path && a.key == lost.key && a.body == lost.body)
            .expect("exact original request was never retried");
        assert_eq!(
            replay.response["replayed"], true,
            "real PG receipt must prove replay: {kind}"
        );
        assert_eq!(
            replay.response["resource"], lost.response["resource"],
            "replay changed original receipt: {kind}"
        );
    }
}

fn assert_first_availability_before_discovery(proxy: &Proxy) -> Result<()> {
    let attempts = proxy.attempts.lock().unwrap();
    let mut released_commands = std::collections::BTreeSet::new();
    for (index, attempt) in attempts.iter().enumerate() {
        if !attempt.path.ends_with("/evidence")
            || !(200..300).contains(&attempt.status)
            || attempt.withheld.is_some()
        {
            continue;
        }
        let value: CapitalExitOwnerEvidenceV1 = serde_json::from_slice(&attempt.body)?;
        if !matches!(
            value.evidence,
            CapitalExitEvidenceKindV1::NativeProgress {
                phase: CapitalExitStateV1::WaitingEvidence,
                ..
            }
        ) || !released_commands.insert(value.command_id)
        {
            continue;
        }
        let first_availability = attempts
            .iter()
            .enumerate()
            .skip(index + 1)
            .find(|(_, next)| {
                next.path.ends_with("/evidence")
                    && serde_json::from_slice::<CapitalExitOwnerEvidenceV1>(&next.body).is_ok_and(
                        |next| {
                            next.command_id == value.command_id
                                && matches!(
                                    next.evidence,
                                    CapitalExitEvidenceKindV1::WithdrawabilityObserved { .. }
                                )
                        },
                    )
            })
            .map(|(index, _)| index)
            .ok_or_else(|| anyhow!("accepted_release_missing_first_availability_attempt"))?;
        ensure!(
            !attempts[index + 1..first_availability]
                .iter()
                .any(|next| next.method == "GET"
                    && next
                        .path
                        .starts_with("/api/v2/downstream/capital-exit-assessments")),
            "assessment_discovery_delayed_first_native_availability"
        );
    }
    ensure!(
        !released_commands.is_empty(),
        "original_native_release_ack_missing"
    );
    Ok(())
}

async fn run(input: Input) -> Result<()> {
    ensure!(
        matches!(
            input.scenario.as_str(),
            "release" | "pause_resume" | "cancel_resume" | "waiting_resume"
        ),
        "unknown_fixture_scenario"
    );
    let destination = reqwest::Url::parse(&input.origin)?;
    ensure!(
        destination.scheme() == "http"
            && destination.username().is_empty()
            && destination.password().is_none()
            && destination.path() == "/"
            && destination.query().is_none()
            && destination.fragment().is_none()
            && destination
                .host_str()
                .is_some_and(|host| host == "localhost"
                    || host
                        .parse::<std::net::IpAddr>()
                        .is_ok_and(|ip| ip.is_loopback())),
        "disposable_loopback_server_required"
    );
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()?;
    let proxy = Proxy {
        origin: input.origin.clone(),
        http: http.clone(),
        attempts: Arc::new(Mutex::new(Vec::new())),
        active: Arc::new(AtomicUsize::new(0)),
        trace: Arc::new(Mutex::new(
            match std::fs::File::create(input.result.with_extension("http.ndjson")) {
                Ok(file) => Some(file),
                Err(error) => {
                    eprintln!("paper_http_trace_create_failed:{error}");
                    None
                }
            },
        )),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await?;
    let origin = format!("http://{}", listener.local_addr()?);
    let router = axum::Router::new()
        .fallback(forward)
        .with_state(proxy.clone());
    let proxy_task = tokio::spawn(async move { axum::serve(listener, router).await });
    let mut claim = crate::paper_service::tests::polymarket_claim();
    claim.handoff.project_id = input.project;
    claim.handoff.downstream_id = input.downstream;
    let fixture = Rc::new(RefCell::new(Fixture::unbound(claim)));
    let original_session = fixture.borrow().session.session_id();
    let root = fixture.borrow()._directory.path().to_owned();
    let (inbox, sender) = crate::paper_capital_exit::channel(root.clone());
    let transport = crate::capital_exit_transport::CapitalExitOwnerTransport::new(
        &origin,
        input.token.as_bytes(),
    )?;
    let poller = crate::paper_capital_exit::Poller::new(transport, sender, root.clone());
    let (status, status_receiver) =
        tokio::sync::watch::channel(crate::paper_service::PaperStatus::idle_with_profile(
            crate::paper_service::PaperProfile::Polymarket,
        ));
    let poller_task = tokio::spawn(poller.run(status));
    let done = Cell::new(false);
    let quotes = Cell::new(true);
    let bid = RefCell::new("0.4900");
    let liquidity = RefCell::new("1000000.000000");
    let withheld_fence_checked = Cell::new(false);
    let drive = async {
        while !done.get() {
            {
                let mut fixture = fixture.borrow_mut();
                inbox.service(&mut fixture.session)?;
                let pending_fence = {
                    let attempts = proxy.attempts.lock().unwrap();
                    attempts
                        .iter()
                        .enumerate()
                        .find(|(_, a)| a.withheld.as_deref() == Some("fence_reply"))
                        .is_some_and(|(i, lost)| {
                            !attempts.iter().skip(i + 1).any(|a| {
                                a.path == lost.path && a.body == lost.body && a.withheld.is_none()
                            })
                        })
                };
                if pending_fence {
                    ensure!(
                        issued(&fixture).is_empty(),
                        "native_reduction_before_fence_receipt"
                    );
                    withheld_fence_checked.set(true);
                }
                let unregistered = {
                    let attempts = proxy.attempts.lock().unwrap();
                    attempts
                        .iter()
                        .any(|a| a.withheld.as_deref() == Some("registration_header"))
                        && !attempts.iter().any(|a| {
                            a.path.ends_with("/client-account-observations")
                                && (200..300).contains(&a.status)
                                && a.withheld.is_none()
                        })
                };
                if unregistered {
                    ensure!(
                        fixture
                            .session
                            .engine
                            .kernel()
                            .cache
                            .borrow()
                            .orders(None, None, None, None, None)
                            .is_empty(),
                        "target_started_without_registration_ack"
                    );
                }
                if quotes.get() {
                    let at = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
                    fixture.last_quote = at;
                    fixture
                        .session
                        .push(quote(at, &bid.borrow(), &liquidity.borrow()))?;
                }
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        Ok::<(), anyhow::Error>(())
    };
    let scenario = async {
        let result = async {
            wait("original_target_native_fill", || fixture.borrow().position() == Decimal::from(400)).await?;
            ensure!(fixture.borrow().free() < Decimal::from(796), "official_initial_fee_missing");
            let seeded = native_snapshot(&fixture.borrow());
            if matches!(input.scenario.as_str(), "pause_resume" | "cancel_resume") {
                *liquidity.borrow_mut() = "10.000000";
                wait("original_low_liquidity_quote_before_assessment", || quote_matches(&fixture.borrow(), "0.4900", "10.000000")).await?;
            }
            let seeded_observation = latest_receipt(&proxy).unwrap().resource.id;
            wait("new_original_observation_after_native_seed", || latest_receipt(&proxy).is_some_and(|r|r.resource.id != seeded_observation)).await?;
            let bootstrap = supported_preview(&input, &http, &proxy, "bootstrap").await?;
            // Stop creating preview requests. The original accepted assessment
            // must not be emitted forever from its still-discoverable receipt.
            trace(&proxy, json!({"event":"await_observation_after_assessment", "at":chrono::Utc::now(), "preview_id":bootstrap.id, "original_observation_id":bootstrap.original_observation_id}));
            wait("assessed_preview_must_not_starve_original_observation", || latest_receipt(&proxy).is_some_and(|r|r.resource.id != bootstrap.original_observation_id)).await?;
            // Browser retries create distinct immutable preview/plan receipts.
            // They must consume one original assessment for the exact request
            // and revision, never rebind its reference to later source quotes.
            let bootstrap_assessments = proxy.attempts.lock().unwrap().iter().filter(|attempt| {
                attempt.method == "POST" && attempt.path.ends_with("/capital-exit-assessments")
                    && (200..300).contains(&attempt.status) && attempt.withheld.is_none()
                    && serde_json::from_slice::<CapitalExitOwnerAssessmentV1>(&attempt.body).is_ok_and(|value|
                        value.request.account_source_id == bootstrap.account_source_id
                            && value.request.expected_source_observation_id == bootstrap.original_observation_id
                            && value.request.scope == bootstrap.scope && value.request.policy == bootstrap.policy
                            && Some(value.expected_account_control_revision) == bootstrap.expected_account_control_revision)
            }).count();
            ensure!(bootstrap_assessments == 1, "same_request_reassessed_from_distinct_preview_receipts:{bootstrap_assessments}");
            let preview = supported_preview(&input, &http, &proxy, "start-fresh").await?;
            ensure!(preview.account_source_id == bootstrap.account_source_id
                && preview.original_observation_id != bootstrap.original_observation_id,
                "new_original_observation_was_not_reassessed_for_same_source");
            ensure!(preview.funds.estimated_execution_cost.as_ref().is_some_and(|m| m.amount.is_positive()), "official_fee_bound_missing");
            let start = CapitalExitStartV1 {
                schema_version: SchemaV1, preview_id: preview.id,
                expected_account_control_revision: preview.expected_account_control_revision.ok_or_else(||anyhow!("registration_revision_missing"))?,
                acknowledged_plan_artifact_id: preview.plan_artifact_id,
                expected_source_observation_id: preview.original_observation_id,
            };
            let path = format!("/api/v2/projects/{}/capital-exits", input.project);
            let started: CommandResult<CapitalExitViewV1> = browser(&input,&http,&path,"paper-poller-start-once",&start).await?;
            let replay: CommandResult<CapitalExitViewV1> = browser(&input,&http,&path,"paper-poller-start-once",&start).await?;
            ensure!(replay.replayed && replay.resource == started.resource, "duplicate_start_changed_intent");
            let id = started.resource.id;
            // This is the regression's central assertion: actual Poller must
            // leave the post-FENCE WaitingEvidence phase and submit through the
            // sole original strategy. No manual NativeRequest::Advance exists.
            wait("poller_must_advance_after_accepted_fence", || !issued(&fixture.borrow()).is_empty()).await?;
            let mut silence = Value::Null;
            let mut late_resume = Value::Null;
            if matches!(input.scenario.as_str(), "pause_resume" | "cancel_resume") {
                wait("official_partial_fill", || {
                    let fixture = fixture.borrow();
                    let ids = issued(&fixture);
                    let cache = fixture.session.engine.kernel().cache.borrow();
                    cache.order(&nautilus_model::identifiers::ClientOrderId::from(ids[0].as_str()))
                        .is_some_and(|order| order.filled_qty().as_decimal() > Decimal::ZERO && !order.is_closed())
                }).await?;
                quotes.set(false);
                // Stop source progress before the new Pause command. A quote
                // retained in the original batch may win the actual cancel race.
                let before = native_snapshot(&fixture.borrow());
                tokio::time::sleep(Duration::from_millis(5200)).await;
                let mut accepted = None;
                for n in 0..10 {
                    let current = view(&input,&http,id).await?;
                    let (operation, request) = if input.scenario == "cancel_resume" {
                        ("cancel", CapitalExitActionV1::Cancel { schema_version: SchemaV1, expected_revision: current.revision })
                    } else {
                        ("pause", CapitalExitActionV1::Pause { schema_version: SchemaV1, expected_revision: current.revision })
                    };
                    let response = http.post(format!("{}/api/v2/capital-exits/{id}/{operation}",input.origin))
                        .header("origin",&input.origin).header("cookie",&input.browser_cookie)
                        .header("Idempotency-Key",format!("pause-{n}")).json(&request).send().await?;
                    if response.status().is_success() { accepted = Some(response.json::<CommandResult<CapitalExitViewV1>>().await?.resource); break; }
                    ensure!(response.status().as_u16()==409,"pause_unexpected_status:{}",response.status().as_u16());
                }
                let paused = accepted.ok_or_else(||anyhow!("pause_revision_never_stable"))?;
                let stop_phase = if input.scenario == "cancel_resume" { CapitalExitStateV1::CancelledReserved } else { CapitalExitStateV1::Paused };
                wait("silent_pause_native_pending_scope", || retained_progress(&root, paused.command_id, CapitalExitStateV1::CancellingExit).is_some()).await?;
                let native_pending=retained_progress(&root, paused.command_id, CapitalExitStateV1::CancellingExit).unwrap();
                let after=native_snapshot(&fixture.borrow());
                for key in ["clock","quote","original_frames","position","free_cash"] {ensure!(before[key]==after[key],"silent_pause_changed_original_{key}");}
                let pending=view(&input,&http,id).await?;
                ensure!(pending.command_id==paused.command_id && pending.funds.released_cash_amount==paused.funds.released_cash_amount,"silent_pause_replaced_original_cash_evidence");
                {
                    let fixture=fixture.borrow();
                    let gate=fixture.session.capital_exit.as_ref().unwrap().gate();
                    let control=gate.control().ok_or_else(||anyhow!("silent_pause_missing_durable_gate"))?;
                    ensure!(control.command_id==paused.command_id && control.account_control_epoch==paused.account_control_epoch
                        && control.external_claim_id==pending.external_claim_id && Some(native_pending.external_claim_id.as_str())==pending.external_claim_id.as_deref(),"silent_pause_changed_original_control_identity");
                    let ids=issued(&fixture);
                    let cache=fixture.session.engine.kernel().cache.borrow();
                    ensure!(ids.iter().any(|id|cache.order(&nautilus_model::identifiers::ClientOrderId::from(id.as_str())).is_some_and(|order|!order.is_closed())),"silent_pause_fabricated_native_terminal");
                }
                ensure!(pending.funds.verified_withdrawable_amount.is_none() && pending.funds.withdrawability!=CapitalExitWithdrawabilityV1::Verified,"silent_pause_claimed_new_withdrawability");
                // Store PAUSE can project PAUSED/BLOCKED while native cancellation
                // is still pending. A stale original report need not be ACKed.
                // Its durable original phase, not the public label, proves wait.
                silence=json!({"before":before,"after":after,"action_receipt":paused,"pending":pending,"native_pending":native_pending});
                *bid.borrow_mut()="0.4800";
                quotes.set(true);
                let deadline=tokio::time::Instant::now()+Duration::from_secs(8);
                loop {
                    let all_closed = {
                        let fixture = fixture.borrow();
                        let ids = issued(&fixture);
                        let cache = fixture.session.engine.kernel().cache.borrow();
                        ids.iter().all(|id|cache.order(&nautilus_model::identifiers::ClientOrderId::from(id.as_str())).is_some_and(|order|order.is_closed()))
                    };
                    if all_closed && accepted_progress(&proxy,paused.command_id,stop_phase) {break;}
                    ensure!(tokio::time::Instant::now()<deadline,"native_cancel_never_reached_paused_receipt");
                    tokio::time::sleep(Duration::from_millis(50)).await;
                }
                *bid.borrow_mut()="0.4900";
                *liquidity.borrow_mut()="1000000.000000";
                wait("original_liquidity_restored_before_resume_assessment", || quote_matches(&fixture.borrow(), "0.4900", "1000000.000000")).await?;
                if input.scenario == "cancel_resume" {
                    let terminal = view(&input,&http,id).await?;
                    ensure!(terminal.command_id==paused.command_id && terminal.state==CapitalExitStateV1::CancelledReserved,
                        "late_resume_requires_actual_cancelled_reserved_receipt");
                    let marker=after_terminal_ack(&proxy,paused.command_id,stop_phase).ok_or_else(||anyhow!("terminal_cancel_ack_missing"))?;
                    // Submit nothing until the original Poller has completed a
                    // discovery turn after the accepted terminal CANCEL. This
                    // consumes the one-shot opportunity that used to be lost.
                    wait("post_cancel_discovery_before_late_preview", || discovery_after(&proxy,marker)).await?;
                    ensure!(terminal_progress_attempts(&proxy,paused.command_id,stop_phase)==1,
                        "terminal_cancel_was_reposted_instead_of_servicing_late_discovery");
                    late_resume=json!({"from_state":"CANCELLED_RESERVED","terminal_receipt":terminal,"native_before_resume":native_snapshot(&fixture.borrow()),"post_terminal_discovery_observed":true,"original_command_id":paused.command_id});
                } else {
                    // Preserve the already frozen Pause fixture's timing.
                    tokio::time::sleep(Duration::from_millis(600)).await;
                }
                // The same active intent is still present. A new real assessment
                // must be serviced for Resume instead of starved by intent polling.
                let preview=supported_preview(&input,&http,&proxy,"active-resume").await?;
                if input.scenario=="cancel_resume" {
                    let (resumed,conflicts)=resume_late(&input,&http,id,paused.command_id,&preview,"late-cancel-resume").await?;
                    ensure!(resumed.resource.command_id!=paused.command_id && resumed.resource.account_control_epoch>paused.account_control_epoch,
                        "late_cancel_resume_did_not_create_new_control_authority");
                    late_resume["resume_receipt"]=json!(resumed.resource);
                    late_resume["browser_revision_conflicts"]=json!(conflicts);
                } else {
                    // Preserve the original r2 Pause test's action helper.
                    let current=view(&input,&http,id).await?;
                    let request=CapitalExitActionV1::Resume {schema_version:SchemaV1,expected_revision:current.revision,preview_id:preview.id};
                    let _:CommandResult<CapitalExitViewV1>=browser(&input,&http,&format!("/api/v2/capital-exits/{id}/resume"),"resume-once",&request).await?;
                }
            }
            if input.scenario=="waiting_resume" {
                // Require original release and at least two actual Availability
                // ACKs, so the new preview is truly later than the first unused
                // post-Progress assessment opportunity. A FENCE projection alone
                // is never treated as native release.
                wait("repeated_real_availability_before_late_preview", || availability_count(&proxy,started.resource.command_id)>=2).await?;
                let available_deadline=tokio::time::Instant::now()+Duration::from_secs(12);
                let available=loop {
                    let current=view(&input,&http,id).await?;
                    ensure!(current.command_id==started.resource.command_id,"available_original_command_changed");
                    if current.state==CapitalExitStateV1::WaitingEvidence
                        && current.funds.withdrawability==CapitalExitWithdrawabilityV1::Simulated
                        && current.funds.released_cash_amount.as_ref().is_some_and(|m|m.amount=="850".parse().unwrap()) {break current;}
                    // An original observation can invalidate the current read
                    // projection between two accepted Availability records. Wait
                    // for its real replacement instead of fabricating freshness.
                    ensure!(tokio::time::Instant::now()<available_deadline,"late_resume_requires_current_native_availability");
                    tokio::time::sleep(Duration::from_millis(25)).await;
                };
                let before=native_snapshot(&fixture.borrow());
                let preview=supported_preview(&input,&http,&proxy,"late-available-resume").await?;
                let (resumed,conflicts)=resume_late(&input,&http,id,available.command_id,&preview,"late-available-resume").await?;
                ensure!(resumed.resource.command_id!=available.command_id && resumed.resource.account_control_epoch>available.account_control_epoch,
                    "late_available_resume_did_not_create_new_control_authority");
                late_resume=json!({"from_state":"WAITING_EVIDENCE","terminal_receipt":available,"native_before_resume":before,"repeated_availability_observed":true,"original_command_id":started.resource.command_id,"resume_receipt":resumed.resource,"browser_revision_conflicts":conflicts});
            }
            let deadline=tokio::time::Instant::now()+Duration::from_secs(12);
            let final_view=loop {
                let current=view(&input,&http,id).await?;
                if current.funds.withdrawability==CapitalExitWithdrawabilityV1::Simulated && current.funds.released_cash_amount.as_ref().is_some_and(|m|m.amount=="850".parse().unwrap()) {break current;}
                ensure!(tokio::time::Instant::now()<deadline,"poller_native_release_evidence_missing");
                tokio::time::sleep(Duration::from_millis(50)).await;
            };
            if input.scenario=="cancel_resume" {
                let old:Id=serde_json::from_value(late_resume["original_command_id"].clone())?;
                ensure!(terminal_progress_attempts(&proxy,old,CapitalExitStateV1::CancelledReserved)==1,
                    "accepted_terminal_cancel_was_reposted_after_late_resume");
            }
            if input.scenario=="waiting_resume" {
                let after=native_snapshot(&fixture.borrow());
                for key in ["issued","position","free_cash"] {
                    ensure!(late_resume["native_before_resume"][key]==after[key],"already_available_resume_changed_native_{key}");
                }
            }
            if !late_resume.is_null() {
                ensure!(json!(final_view.command_id)==late_resume["resume_receipt"]["command_id"],"availability_belongs_to_prior_command");
                ensure!(matches!(final_view.owner_command.instruction,CapitalExitOwnerInstructionV1::Resume {}),"late_resume_instruction_not_preserved");
            }
            ensure!(final_view.state!=CapitalExitStateV1::Completed,"simulation_claimed_withdrawal");
            ensure!(fixture.borrow().position()>Decimal::from(200) && fixture.borrow().position()<Decimal::from(400),"not_a_bounded_partial_reduction");
            ensure!(fixture.borrow().free()>=Decimal::from(850) && fixture.borrow().free()<Decimal::from(851),"fee_bound_not_tight");
            ensure!(fixture.borrow().session.session_id()==original_session,"engine_session_replaced");
            ensure!(fixture.borrow().session.consumed_targets()==1,"initial_target_replayed");
            ensure!(withheld_fence_checked.get(),"missing_observation_window_for_ambiguous_fence");
            Ok::<Value,anyhow::Error>(json!({"scenario":input.scenario,"actual_execution":"PRODUCTION_POLLER_ORIGINAL_PAPER_ENGINE_FIXTURE","scientific_qualification":"NOT_ASSESSED","full_serve_host_exercised":false,"withdrawal_performed":false,"seeded":seeded,"final_native":native_snapshot(&fixture.borrow()),"intent":final_view,"silence":silence,"late_resume":late_resume,"bootstrap_preview":bootstrap}))
        }.await;
        if let Err(error) = &result {
            trace(
                &proxy,
                json!({"event":"scenario_failed", "at":chrono::Utc::now(), "error":error.to_string(), "poller_reason":status_receiver.borrow().reason_code}),
            );
        }
        done.set(true);
        result
    };
    let (driver, result) = tokio::join!(drive, scenario);
    poller_task.abort();
    let _ = poller_task.await;
    // Client abort does not mean an already forwarded Server transaction rolled
    // back. Drain every proxy handler before freezing original evidence and PG
    // reconciliation inputs. Never hide additional committed rows with a filter.
    wait("drain_original_server_responses", || {
        proxy.active.load(Ordering::SeqCst) == 0
    })
    .await?;
    proxy_task.abort();
    let _ = proxy_task.await;
    driver?;
    let mut result = result?;
    assert_replays(&proxy);
    assert_first_availability_before_discovery(&proxy)?;
    let attempts = proxy.attempts.lock().unwrap().clone();
    let mut evidence = BTreeMap::new();
    for attempt in attempts
        .iter()
        .filter(|a| a.path.ends_with("/evidence") && (200..300).contains(&a.status))
    {
        let value: Value = serde_json::from_slice(&attempt.body)?;
        evidence.insert(
            value["external_message_id"].as_str().unwrap().to_owned(),
            value,
        );
        ensure!(
            std::fs::read_dir(&root)?
                .filter_map(|e| e.ok())
                .filter_map(|e| std::fs::read(e.path()).ok())
                .any(|bytes| bytes == attempt.body),
            "http_evidence_missing_original_durable_bytes"
        );
    }
    result["original_evidence"] = json!(evidence.into_values().collect::<Vec<_>>());
    result["attempts"] = json!(attempts);
    std::fs::write(&input.result, serde_json::to_vec(&result)?)?;
    Ok(())
}

#[test]
#[ignore = "requires this composed candidate's disposable Server/PG bridge on stdin"]
fn production_poller_uses_registered_original_paper_and_real_receipts() {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(64 * 1024)
        .read_to_end(&mut bytes)
        .unwrap();
    let input: Input =
        serde_json::from_slice(&bytes).expect("explicit disposable paper bridge input");
    std::thread::spawn(move || {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap()
            .block_on(run(input))
    })
    .join()
    .unwrap()
    .unwrap();
}
