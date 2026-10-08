//! Protocol scheduling only: loopback DTO responses and a synthetic native inbox.
//! No native order, account authority, PostgreSQL or external venue is created.
use super::*;
use axum::response::IntoResponse;
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
struct Probe {
    state: Arc<Mutex<ProbeState>>,
    delay: Duration,
    complete: Arc<tokio::sync::Notify>,
}
struct ProbeState {
    view: CapitalExitViewV1,
    calls: Vec<(String, Option<String>, Vec<u8>)>,
    completed: bool,
}

fn original() -> CapitalExitOwnerEvidenceV1 {
    serde_json::from_str(include_str!(
        "../../../../tests/contracts/paper-capital-resume-progress-original.json"
    ))
    .unwrap()
}
fn fence() -> CapitalExitEvidenceKindV1 {
    CapitalExitEvidenceKindV1::FenceApplied {
        native_gate_report_ref: "synthetic-scheduling-gate".into(),
        controlled_strategy_ids: vec!["SYNTHETIC".into()],
        invalidated_target_claim_refs: vec![],
        invalidated_child_timer_refs: vec![],
        resolved_inflight_report_ref: "synthetic-scheduling-resolved".into(),
        remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
        remaining_target_authority_ref: None,
    }
}
fn evidence(
    view: &CapitalExitViewV1,
    kind: CapitalExitEvidenceKindV1,
    sequence: u64,
) -> CapitalExitOwnerEvidenceV1 {
    let mut value = original();
    value.command_id = view.command_id;
    value.account_control_epoch = view.account_control_epoch;
    value.external_claim_id = view.external_claim_id.clone().unwrap();
    value.external_message_id = format!("synthetic-scheduling-{}-{sequence}", view.command_id);
    value.sequence = DbCounter::new(sequence).unwrap();
    value.evidence = kind;
    value
}
fn progress(phase: CapitalExitStateV1) -> CapitalExitEvidenceKindV1 {
    let mut kind = original().evidence;
    let CapitalExitEvidenceKindV1::NativeProgress { phase: current, .. } = &mut kind else {
        panic!("expected original native progress shape");
    };
    *current = phase;
    kind
}
fn observation() -> AccountObservationSubmitV2 {
    let observation: AccountObservationSubmitV1 = serde_json::from_str(include_str!(
        "../../../../tests/contracts/native-account-paper-snapshot.json"
    ))
    .unwrap();
    AccountObservationSubmitV2 {
        schema_version: NativeClientObservationSchemaV2,
        native_client_id: "SYNTHETIC-SCHEDULING-CLIENT".into(),
        observation,
    }
}

async fn endpoint(
    axum::extract::State(probe): axum::extract::State<Probe>,
    request: axum::extract::Request,
) -> axum::response::Response {
    let path = request.uri().to_string();
    let key = request
        .headers()
        .get("idempotency-key")
        .map(|v| v.to_str().unwrap().to_owned());
    let bytes = axum::body::to_bytes(request.into_body(), usize::MAX)
        .await
        .unwrap();
    {
        let mut state = probe.state.lock().unwrap();
        state.calls.push((path.clone(), key, bytes.to_vec()));
        if path == "/api/v2/downstream/capital-exits" && state.completed {
            // A subsequent fresh intent GET proves run() consumed the successful
            // Availability ACK and cleared pending; an unknown reply only replays.
            probe.complete.notify_one();
        }
    }
    // Fixed test-only per-request latency reproduces the observed ~0.55s cost.
    // It neither changes production pacing nor fabricates a native outcome.
    tokio::time::sleep(probe.delay).await;
    let mut state = probe.state.lock().unwrap();
    if path == "/api/v2/downstream/capital-exits" {
        return axum::Json(contracts::control::Page {
            schema_version: SchemaV1,
            items: vec![state.view.clone()],
            next_cursor: None,
        })
        .into_response();
    }
    if path.starts_with("/api/v2/downstream/capital-exit-assessments") {
        // Two complete pages with no matching request represent irrelevant
        // history; the ordinary discovery call still has to traverse both.
        return axum::Json(contracts::control::Page::<CapitalExitPreviewV1> {
            schema_version: SchemaV1,
            items: vec![],
            next_cursor: if path.contains('?') {
                None
            } else {
                Some(state.view.preview_id)
            },
        })
        .into_response();
    }
    if path.ends_with("/client-account-observations") {
        let request: AccountObservationSubmitV2 = serde_json::from_slice(&bytes).unwrap();
        let receipt = AccountObservationReceiptV2 {
            schema_version: NativeClientObservationSchemaV2,
            replayed: false,
            native_client_id: request.native_client_id,
            resource: AccountObservationV1 {
                id: Id::new(),
                source_id: state.view.account_source_id,
                downstream_id: Id::new(),
                observation: request.observation,
                gap_before: false,
                received_at: chrono::Utc::now(),
            },
        };
        return (
            axum::http::StatusCode::CREATED,
            [(
                "x-qz-capital-exit-source",
                state.view.account_source_id.to_string(),
            )],
            axum::Json(receipt),
        )
            .into_response();
    }
    if path.ends_with("/claim") {
        let request: CapitalExitClaimV1 = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(request.command_id, state.view.command_id);
        assert_eq!(
            request.account_control_epoch,
            state.view.account_control_epoch
        );
        state.view.external_claim_id = Some(request.external_claim_id);
        if matches!(
            state.view.owner_command.instruction,
            CapitalExitOwnerInstructionV1::Resume {} | CapitalExitOwnerInstructionV1::Start {}
        ) {
            state.view.state = CapitalExitStateV1::Fencing;
        }
    } else if path.ends_with("/evidence") {
        let request: CapitalExitOwnerEvidenceV1 = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(request.command_id, state.view.command_id);
        assert_eq!(
            request.account_control_epoch,
            state.view.account_control_epoch
        );
        match request.evidence {
            CapitalExitEvidenceKindV1::FenceApplied { .. } => {
                if matches!(
                    state.view.owner_command.instruction,
                    CapitalExitOwnerInstructionV1::Resume {}
                        | CapitalExitOwnerInstructionV1::Start {}
                ) {
                    state.view.state = CapitalExitStateV1::WaitingEvidence;
                }
            }
            CapitalExitEvidenceKindV1::NativeProgress { phase, .. } => state.view.state = phase,
            CapitalExitEvidenceKindV1::WithdrawabilityObserved { .. } => {
                state.completed = true;
            }
            _ => panic!("unexpected synthetic scheduling evidence"),
        }
    } else {
        panic!("unexpected synthetic scheduling path: {path}");
    }
    state.view.revision = state.view.revision.next().unwrap();
    (
        axum::http::StatusCode::CREATED,
        axum::Json(contracts::control::CommandResult {
            schema_version: SchemaV1,
            replayed: false,
            resource: state.view.clone(),
        }),
    )
        .into_response()
}

async fn fixture(
    delay: Duration,
) -> (
    Poller,
    NativeInbox,
    Probe,
    tokio::task::JoinHandle<()>,
    tempfile::TempDir,
) {
    let retained = original();
    let mut view: CapitalExitViewV1 =
        serde_json::from_value(retained.native_evidence["owner_control_report"]["control"].clone())
            .unwrap();
    view.state = CapitalExitStateV1::Requested;
    view.external_claim_id = None;
    let probe = Probe {
        state: Arc::new(Mutex::new(ProbeState {
            view: view.clone(),
            calls: vec![],
            completed: false,
        })),
        delay,
        complete: Arc::new(tokio::sync::Notify::new()),
    };
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let origin = format!("http://{}/", listener.local_addr().unwrap());
    let router = axum::Router::new()
        .fallback(endpoint)
        .with_state(probe.clone());
    let server = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    let root = tempfile::tempdir().unwrap();
    let (inbox, sender) = channel(root.path().to_owned());
    let transport = crate::capital_exit_transport::CapitalExitOwnerTransport::new(
        &origin,
        b"synthetic-continuation-protocol-no-authority",
    )
    .unwrap();
    let mut poller = Poller::new(transport, sender, root.path().to_owned());
    poller.source = Some(view.account_source_id);
    poller.observation_required = true;
    (poller, inbox, probe, server, root)
}

#[tokio::test]
async fn confirmed_continuations_complete_delayed_resume_chain_without_discovery_or_freshness_shortcuts()
 {
    let (poller, inbox, probe, server, _root) = fixture(Duration::from_millis(550)).await;
    let original_command = probe.state.lock().unwrap().view.command_id;
    let (status, _) =
        tokio::sync::watch::channel(crate::paper_service::PaperStatus::idle_with_profile(
            crate::paper_service::PaperProfile::Polymarket,
        ));
    let runner = tokio::spawn(poller.run(status));
    let native_calls = Arc::new(Mutex::new(Vec::new()));
    let observed_calls = native_calls.clone();
    let owner = tokio::spawn(async move {
        let mut sequence = 20;
        let mut advances = 0;
        loop {
            let envelope = match inbox.receiver.try_recv() {
                Ok(envelope) => envelope,
                Err(mpsc::TryRecvError::Empty) => {
                    tokio::time::sleep(Duration::from_millis(1)).await;
                    continue;
                }
                Err(mpsc::TryRecvError::Disconnected) => break,
            };
            sequence += 1;
            let (label, response) = match envelope.request {
                NativeRequest::Observe => ("observe", NativeResponse::Observation(observation())),
                NativeRequest::Bind(receipt) => {
                    ("bind", NativeResponse::Bound(receipt.resource.source_id))
                }
                NativeRequest::Fence(view) => {
                    assert_eq!(view.command_id, original_command);
                    (
                        "fence",
                        NativeResponse::Evidence(evidence(&view, fence(), sequence)),
                    )
                }
                NativeRequest::Advance(view) => {
                    assert_eq!(view.command_id, original_command);
                    advances += 1;
                    let phase = if advances == 1 {
                        CapitalExitStateV1::Reducing
                    } else {
                        CapitalExitStateV1::WaitingEvidence
                    };
                    (
                        "advance",
                        NativeResponse::Evidence(evidence(&view, progress(phase), sequence)),
                    )
                }
                NativeRequest::Availability(view) => {
                    assert_eq!(advances, 2);
                    let kind = CapitalExitEvidenceKindV1::WithdrawabilityObserved {
                        available_cash: AccountMoneyV1 {
                            amount: "850".parse().unwrap(),
                            currency: "pUSD".into(),
                        },
                        basis: CapitalExitAvailabilityBasisV1::ControlledSandbox,
                        venue: "SYNTHETIC".into(),
                        native_availability_report_ref: "synthetic".into(),
                        settlement_report_ref: "synthetic".into(),
                        margin_report_ref: "synthetic".into(),
                        open_orders_report_ref: "synthetic".into(),
                    };
                    (
                        "availability",
                        NativeResponse::Evidence(evidence(&view, kind, sequence)),
                    )
                }
                NativeRequest::Assess(_) => {
                    panic!("historical nonmatching pages must not fabricate an assessment")
                }
            };
            observed_calls.lock().unwrap().push(label);
            assert!(envelope.reply.send(Ok(response)).is_ok());
        }
    });
    tokio::time::timeout(Duration::from_secs(12), probe.complete.notified())
        .await
        .expect("delayed confirmed Resume chain failed its unchanged 12-second budget");
    let state = probe.state.lock().unwrap();
    assert!(state.completed);
    assert_eq!(
        state
            .calls
            .iter()
            .filter(|(p, _, _)| p.ends_with("/claim"))
            .count(),
        1
    );
    assert_eq!(
        state
            .calls
            .iter()
            .filter(|(p, _, _)| p.starts_with("/api/v2/downstream/capital-exit-assessments"))
            .count(),
        2,
        "continuation optimization must retain complete historical discovery"
    );
    let evidence_kinds: Vec<_> = state
        .calls
        .iter()
        .filter(|(p, _, _)| p.ends_with("/evidence"))
        .map(|(_, _, body)| {
            serde_json::from_slice::<CapitalExitOwnerEvidenceV1>(body)
                .unwrap()
                .evidence
        })
        .collect();
    assert!(matches!(
        evidence_kinds.as_slice(),
        [
            CapitalExitEvidenceKindV1::FenceApplied { .. },
            CapitalExitEvidenceKindV1::NativeProgress {
                phase: CapitalExitStateV1::Reducing,
                ..
            },
            CapitalExitEvidenceKindV1::NativeProgress {
                phase: CapitalExitStateV1::WaitingEvidence,
                ..
            },
            CapitalExitEvidenceKindV1::WithdrawabilityObserved { .. }
        ]
    ));
    assert!(
        state
            .calls
            .iter()
            .filter(|(p, _, _)| p.ends_with("/client-account-observations"))
            .count()
            >= 2,
        "latency optimization skipped original observation refresh"
    );
    drop(state);
    assert_eq!(
        native_calls
            .lock()
            .unwrap()
            .iter()
            .filter(|&&p| p == "advance")
            .count(),
        2
    );
    runner.abort();
    let _ = runner.await;
    owner.abort();
    let _ = owner.await;
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn new_stop_command_is_refetched_before_confirmed_continuation() {
    for instruction in [
        CapitalExitOwnerInstructionV1::Pause {},
        CapitalExitOwnerInstructionV1::Cancel {},
    ] {
        let (mut poller, inbox, probe, server, _root) = fixture(Duration::ZERO).await;
        poller.step().await.unwrap(); // The original Resume claim is accepted.
        assert!(poller.confirmed_continuation);
        let next_command = Id::new();
        {
            let mut state = probe.state.lock().unwrap();
            state.view.command_id = next_command;
            state.view.account_control_epoch =
                DbCounter::new(state.view.account_control_epoch.get() + 1).unwrap();
            state.view.owner_command = CapitalExitOwnerCommandV1 {
                schema_version: SchemaV1,
                command_id: next_command,
                account_control_epoch: state.view.account_control_epoch,
                instruction: instruction.clone(),
            };
            state.view.external_claim_id = None;
            state.view.state = if matches!(instruction, CapitalExitOwnerInstructionV1::Pause {}) {
                CapitalExitStateV1::Paused
            } else {
                CapitalExitStateV1::CancellingExit
            };
        }
        poller.step().await.unwrap(); // A fresh intent read must claim the new stop.
        assert!(poller.confirmed_continuation);
        let service = async {
            loop {
                if let Ok(envelope) = inbox.receiver.try_recv() {
                    let NativeRequest::Fence(view) = envelope.request else {
                        panic!("stop did not take precedence over original native continuation");
                    };
                    assert_eq!(view.command_id, next_command);
                    assert_eq!(view.owner_command.instruction, instruction);
                    assert!(
                        envelope
                            .reply
                            .send(Ok(NativeResponse::Evidence(evidence(&view, fence(), 30))))
                            .is_ok()
                    );
                    break;
                }
                tokio::task::yield_now().await;
            }
            Ok::<(), anyhow::Error>(())
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::try_join!(poller.step(), service)
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(poller.acknowledged_command, Some(next_command));
        server.abort();
        let _ = server.await;
    }
}

#[tokio::test]
async fn repeated_progress_new_timestamp_and_sequence_do_not_create_continuation_loops() {
    let (mut poller, _inbox, probe, server, _root) = fixture(Duration::ZERO).await;
    poller.step().await.unwrap();
    let view = probe.state.lock().unwrap().view.clone();
    for (index, phase, immediate) in [
        (1, CapitalExitStateV1::Reducing, false),
        (2, CapitalExitStateV1::WaitingEvidence, true),
        (3, CapitalExitStateV1::WaitingEvidence, false),
        (4, CapitalExitStateV1::Paused, false),
        (5, CapitalExitStateV1::CancelledReserved, false),
    ] {
        let mut request = evidence(&view, progress(phase), 30 + index);
        request.asof = chrono::Utc::now();
        request.valid_until = request.asof + chrono::Duration::seconds(5);
        poller.pending = Some(Pending::Evidence(request));
        poller.step().await.unwrap();
        assert_eq!(poller.confirmed_continuation, immediate, "{phase:?}");
    }
    server.abort();
    let _ = server.await;
}

#[tokio::test]
async fn consecutive_observation_only_acknowledgements_do_not_create_continuation_loops() {
    let (mut poller, inbox, _probe, server, _root) = fixture(Duration::ZERO).await;
    for expected in [true, false, false] {
        poller.pending = Some(Pending::Observation(observation()));
        let service = async {
            loop {
                if let Ok(envelope) = inbox.receiver.try_recv() {
                    let NativeRequest::Bind(receipt) = envelope.request else {
                        panic!("observation acknowledgement changed native dispatch");
                    };
                    assert!(
                        envelope
                            .reply
                            .send(Ok(NativeResponse::Bound(receipt.resource.source_id)))
                            .is_ok()
                    );
                    break;
                }
                tokio::task::yield_now().await;
            }
            Ok::<(), anyhow::Error>(())
        };
        tokio::time::timeout(Duration::from_secs(5), async {
            tokio::try_join!(poller.step(), service)
        })
        .await
        .unwrap()
        .unwrap();
        assert_eq!(poller.confirmed_continuation, expected);
    }
    server.abort();
    let _ = server.await;
}
