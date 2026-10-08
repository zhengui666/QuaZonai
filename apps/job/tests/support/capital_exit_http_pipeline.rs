//! Explicitly selected disposable HTTP/PG + original native Sandbox bridge.
//! Reuses the component fixture's Strategy; contains no second execution engine.
use super::*;
use anyhow::{Result, anyhow, ensure};
use contracts::{
    account_observation::{AccountObservationReceiptV2, AccountObservationSubmitV2},
    control::CommandResult,
};
use job::capital_exit_transport::CapitalExitOwnerTransport;
use serde::{Deserialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{
    io::Read,
    path::{Path, PathBuf},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    origin: String,
    token: String,
    browser_cookie: String,
    project: Id,
    registered: PathBuf,
    result: PathBuf,
}
async fn browser<T: DeserializeOwned>(
    input: &Input,
    http: &reqwest::Client,
    path: &str,
    key: &str,
    body: &impl serde::Serialize,
) -> Result<CommandResult<T>> {
    let reply = http
        .post(format!("{}{path}", input.origin))
        .header("origin", &input.origin)
        .header("cookie", &input.browser_cookie)
        .header("Idempotency-Key", key)
        .json(body)
        .send()
        .await?;
    ensure!(
        reply.status().is_success(),
        "controlled_browser_status:{}",
        reply.status().as_u16()
    );
    Ok(reply.json().await?)
}
async fn command(state: &Rc<RefCell<State>>, value: Command) -> Result<()> {
    ensure!(state.borrow().command.is_none(), "controlled_command_busy");
    state.borrow_mut().command = Some(value);
    get_data_event_sender().send(DataEvent::Data(Data::Quote(quote())))?;
    wait(state, || state.borrow().command.is_none()).await
}
async fn wait(state: &Rc<RefCell<State>>, ready: impl Fn() -> bool) -> Result<()> {
    wait_until(state, || Ok(ready())).await
}
async fn wait_until(state: &Rc<RefCell<State>>, ready: impl Fn() -> Result<bool>) -> Result<()> {
    let end = tokio::time::Instant::now() + Duration::from_secs(5);
    while !ready()? {
        if let Some(error) = &state.borrow().error {
            return Err(anyhow!(error.clone()));
        }
        ensure!(
            tokio::time::Instant::now() < end,
            "controlled_native_outcome_timeout"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    if let Some(error) = &state.borrow().error {
        return Err(anyhow!(error.clone()));
    }
    Ok(())
}
async fn retain_original_snapshot(
    state: &Rc<RefCell<State>>,
    observer: &NativeNodeObserver,
    path: &Path,
) -> Result<()> {
    // A heartbeat is only a lifecycle frame. Wait for the official Portfolio
    // timer to emit and fully write a snapshot AFTER the native outcome.
    // Neither cursor progress alone nor a fixed sleep proves a complete record.
    ensure!(observer.heartbeat()?, "native_lifecycle_frame_not_queued");
    let after = observer.cursor().0;
    wait_until(state, || {
        let bytes = std::fs::read(path)?;
        let complete = bytes
            .iter()
            .rposition(|b| *b == b'\n')
            .map_or(&[][..], |i| &bytes[..=i]);
        for line in complete.split_inclusive(|b| *b == b'\n') {
            let original: AccountObservationSubmitV2 = serde_json::from_slice(line)?;
            if original.observation.sequence > after && original.observation.snapshot.is_some() {
                return Ok(true);
            }
        }
        Ok(false)
    })
    .await
}
async fn relay(
    input: &Input,
    http: &reqwest::Client,
    owner: &NativeCapitalExitOwner,
    path: &Path,
    consumed: &mut usize,
    latest: &mut Option<(Id, Id)>,
) -> Result<()> {
    let bytes = std::fs::read(path)?;
    let complete = bytes
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(&[][..], |i| &bytes[..=i]);
    for line in complete.split_inclusive(|b| *b == b'\n').skip(*consumed) {
        let original: AccountObservationSubmitV2 = serde_json::from_slice(line)?;
        let requested_at = Utc::now();
        eprintln!(
            "native_intake_request {}",
            json!({"at":requested_at,"record_index":*consumed,"observation":original})
        );
        let response = http
            .post(format!(
                "{}/api/v2/forward/client-account-observations",
                input.origin
            ))
            .bearer_auth(&input.token)
            .json(&original)
            .send()
            .await?;
        let status = response.status();
        let registrations: Vec<_> = response
            .headers()
            .get_all("x-qz-capital-exit-source")
            .iter()
            .map(|value| value.to_str().map(str::to_owned))
            .collect();
        let bytes = response.bytes().await;
        // Deliberately log no authentication or response headers. The one
        // registration fact needed for diagnosis is its presence/count, not raw
        // header contents. Parent redacts known test secrets before publication.
        eprintln!(
            "native_intake_response {}",
            json!({"at":Utc::now(),"requested_at":requested_at,"record_index":*consumed,"status":status.as_u16(),"registration_count":registrations.len(),"body":bytes.as_ref().ok().map(|bytes|String::from_utf8_lossy(bytes)),"body_read_failed":bytes.is_err()})
        );
        ensure!(
            status.is_success(),
            "original_observation_intake_status:{}",
            status.as_u16()
        );
        let registered = registrations
            .first()
            .and_then(|value| value.as_ref().ok())
            .ok_or_else(|| anyhow!("native_owner_registration_missing"))?;
        ensure!(
            registrations.len() == 1,
            "native_owner_registration_duplicate"
        );
        let receipt: AccountObservationReceiptV2 = serde_json::from_slice(&bytes?)?;
        ensure!(
            *registered == receipt.resource.source_id.to_string(),
            "native_owner_registration_mismatch"
        );
        let observed = owner.bind_authenticated_source(&receipt)?;
        if receipt.resource.observation.snapshot.is_some() {
            *latest = Some((receipt.resource.source_id, observed));
        }
        *consumed += 1;
    }
    Ok(())
}
async fn post_latest(
    owner_http: &CapitalExitOwnerTransport,
    state: &Rc<RefCell<State>>,
) -> Result<CapitalExitViewV1> {
    let original = state
        .borrow()
        .evidence
        .last()
        .cloned()
        .ok_or_else(|| anyhow!("native_evidence_absent"))?;
    let first = owner_http.submit_evidence(&original).await?;
    // Exercise exact retained replay, not another synthetic event/financial act.
    let replay = owner_http.submit_evidence(&original).await?;
    ensure!(
        replay.replayed
            && replay.resource.id == first.resource.id
            && replay.resource.revision == first.resource.revision,
        "native_evidence_replay_mismatch"
    );
    Ok(first.resource)
}

#[test]
fn native_bridge_transport_keeps_numeric_loopback_boundary() {
    let credential = b"synthetic-transport-boundary-only-no-real-authority";
    for origin in ["http://127.0.0.1:32123", "https://qz.example"] {
        assert!(CapitalExitOwnerTransport::new(origin, credential).is_ok());
    }
    for origin in [
        "http://localhost:32123",
        "http://192.0.2.1:32123",
        "http://127.0.0.1:32123/not-an-origin",
        "http://user@127.0.0.1:32123",
        "http://127.0.0.1:32123?token=synthetic",
        "http://127.0.0.1:32123#fragment",
    ] {
        assert!(CapitalExitOwnerTransport::new(origin, credential).is_err());
    }
}

#[tokio::test(flavor = "current_thread")]
#[ignore = "requires the explicitly selected disposable Server/PG fixture on stdin"]
async fn real_http_capital_exit_uses_original_sandbox_events() {
    let mut bytes = Vec::new();
    std::io::stdin()
        .take(64 * 1024)
        .read_to_end(&mut bytes)
        .unwrap();
    let input: Input = serde_json::from_slice(&bytes).expect("controlled pipeline input");
    let http = reqwest::Client::builder()
        .no_proxy()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(10))
        .build()
        .unwrap();
    let owner_http = CapitalExitOwnerTransport::new(&input.origin, input.token.as_bytes()).unwrap();
    let directory = tempfile::tempdir().unwrap();
    let sandbox = SandboxExecutionClientConfig {
        account_id: AccountId::from(ACCOUNT),
        venue: Venue::from("QZEXIT"),
        starting_balances: vec![Money::from("100 USDC")],
        base_currency: Some(Currency::USDC()),
        account_type: AccountType::Cash,
        ..Default::default()
    };
    let mut node = LiveNode::builder(TraderId::from(ACCOUNT), Environment::Sandbox)
        .unwrap()
        .with_logging(LoggerConfig {
            bypass_logging: true,
            ..Default::default()
        })
        .with_load_state(false)
        .with_save_state(false)
        .with_reconciliation(false)
        .with_timeout_connection(2)
        .with_timeout_portfolio(2)
        .with_delay_post_stop_secs(0)
        .with_delay_shutdown_secs(0)
        .with_portfolio_config(PortfolioConfig {
            // This sequential real HTTP/PG bridge is not a 50 Hz load test.
            // Keep the official producer below intake throughput; relay every
            // emitted frame unchanged, rather than dropping/coalescing a backlog.
            snapshot_interval_ms: Some(1_000),
            ..Default::default()
        })
        .add_simulated_exec_client(
            Some(CLIENT.into()),
            Box::new(SandboxExecutionClientFactory::new()),
            Box::new(sandbox.clone()),
        )
        .unwrap()
        .build()
        .unwrap();
    let mut instrument = binary_option();
    instrument.id = InstrumentId::from(INSTRUMENT);
    instrument.activation_ns = UnixNanos::from(1);
    instrument.expiration_ns = UnixNanos::from(
        (Utc::now() + ChronoDuration::hours(1))
            .timestamp_nanos_opt()
            .unwrap() as u64,
    );
    node.kernel()
        .cache()
        .borrow_mut()
        .add_instrument(InstrumentAny::BinaryOption(instrument))
        .unwrap();
    let owner = Rc::new(
        NativeCapitalExitOwner::attach_source_bound_sandbox(
            &node,
            SandboxCapitalOwnerBinding {
                managed_account_key: "sandbox-physical-QZEXIT-001".into(),
                owner_binding_ref: "controlled-owner-v1".into(),
                account_source_id: None,
                strategy_id: StrategyId::from(STRATEGY),
                client_id: ClientId::from(CLIENT),
                instrument_id: InstrumentId::from(INSTRUMENT),
            },
            &sandbox,
            directory.path(),
        )
        .unwrap(),
    );
    let state = Rc::new(RefCell::new(State::default()));
    node.add_strategy(FixtureStrategy {
        core: StrategyCore::new(StrategyConfig {
            strategy_id: Some(StrategyId::from(STRATEGY)),
            ..Default::default()
        }),
        owner: owner.clone(),
        state: state.clone(),
        source_observation_id: Id::new(),
        sequence: 0,
    })
    .unwrap();
    let retained = directory.path().join("original-native.ndjson");
    let observer = NativeNodeObserver::attach_client_bound(
        &node,
        input.project,
        ClientId::from(CLIENT),
        &retained,
        1024,
    )
    .unwrap();
    let handle = node.handle();
    let cache = node.kernel().cache();
    let mut consumed = 0;
    let mut latest = None;
    let drive = async {
        let outcome=async{
            eprintln!("native_pipeline_stage {}",json!({"at":Utc::now(),"stage":"wait_node_running"}));
            wait(&state,||handle.is_running()).await?;
            eprintln!("native_pipeline_stage {}",json!({"at":Utc::now(),"stage":"seed_original_sandbox"}));
            command(&state,Command::Seed).await?;
            eprintln!("native_pipeline_stage {}",json!({"at":Utc::now(),"stage":"wait_original_seed_outcomes"}));
            wait(&state,||state.borrow().fills.len()==1 && cache.borrow().order(&ClientOrderId::from("OPENING-ORDER")).is_some_and(|o|o.status()==OrderStatus::Accepted)).await?;
            // Relay only native producer frames, including all original sequence
            // numbers. Freeze intake briefly while admitting the immutable plan.
            eprintln!("native_pipeline_stage {}",json!({"at":Utc::now(),"stage":"relay_original_observations"}));
            retain_original_snapshot(&state,&observer,&retained).await?;
            relay(&input,&http,&owner,&retained,&mut consumed,&mut latest).await?;
            let (source,observation)=latest.ok_or_else(||anyhow!("native_source_observation_absent"))?;
            wait(&state,||input.registered.is_file()).await?;
            command(&state,Command::Observation(observation)).await?;
            let request=request(source,observation);
            let blocked:CommandResult<CapitalExitPreviewV1>=browser(&input,&http,&format!("/api/v2/projects/{}/capital-exit-previews",input.project),"original-blocked-preview",&request).await?;
            ensure!(blocked.resource.capability==CapitalExitCapabilityV1::Blocked,"assessment_missing_must_block");
            let pending=owner_http.pending_assessments(None).await?;
            ensure!(pending.items.iter().any(|p|p.id==blocked.resource.id),"original_preview_not_routed_to_owner");
            command(&state,Command::Assess(request.clone())).await?;
            let assessment=state.borrow().assessment.clone().ok_or_else(||anyhow!("native_assessment_absent"))?;
            ensure!(assessment.capability==CapitalExitCapabilityV1::Supported,"native_assessment_blocked:{:?}",assessment.reason_codes);
            owner_http.submit_assessment(&assessment).await?;
            let preview:CommandResult<CapitalExitPreviewV1>=browser(&input,&http,&format!("/api/v2/projects/{}/capital-exit-previews",input.project),"original-supported-preview",&request).await?;
            ensure!(preview.resource.capability==CapitalExitCapabilityV1::Supported,"supported_preview_unavailable");
            let start=CapitalExitStartV1{schema_version:SchemaV1,preview_id:preview.resource.id,expected_account_control_revision:preview.resource.expected_account_control_revision.ok_or_else(||anyhow!("control_revision_missing"))?,acknowledged_plan_artifact_id:preview.resource.plan_artifact_id,expected_source_observation_id:observation};
            let started:CommandResult<CapitalExitViewV1>=browser(&input,&http,&format!("/api/v2/projects/{}/capital-exits",input.project),"start-once",&start).await?;
            let replay:CommandResult<CapitalExitViewV1>=browser(&input,&http,&format!("/api/v2/projects/{}/capital-exits",input.project),"start-once",&start).await?;
            ensure!(replay.replayed && replay.resource.id==started.resource.id,"start_replay_created_another_intent");
            let pending=owner_http.pending_intents(None).await?;ensure!(pending.items.iter().any(|v|v.id==started.resource.id),"intent_not_routed_to_bound_owner");
            let v=started.resource;
            let claim=CapitalExitClaimV1{schema_version:SchemaV1,expected_revision:v.revision,command_id:v.command_id,account_control_epoch:v.account_control_epoch,account_source_id:source,owner_binding_ref:v.owner_binding_ref.clone(),external_claim_id:"original-http-native-claim".into()};
            let mut view=owner_http.claim(v.id,"claim-original-command",&claim).await?.resource;
            command(&state,Command::Fence(view.clone())).await?;view=post_latest(&owner_http,&state).await?;
            command(&state,Command::TryObsoleteTarget).await?;ensure!(state.borrow().obsolete_blocked,"obsolete_native_target_not_fenced");
            command(&state,Command::Advance(view.clone())).await?;view=post_latest(&owner_http,&state).await?;
            wait(&state,||state.borrow().cancels.len()==1).await?;
            command(&state,Command::Advance(view.clone())).await?;view=post_latest(&owner_http,&state).await?;
            wait(&state,||state.borrow().fills.len()==2).await?;
            eprintln!("native_pipeline_stage {}",json!({"at":Utc::now(),"stage":"relay_after_original_exit_fill","original_fills":state.borrow().fills.len(),"original_cancellations":state.borrow().cancels.len(),"relayed_records":consumed}));
            retain_original_snapshot(&state,&observer,&retained).await?;
            relay(&input,&http,&owner,&retained,&mut consumed,&mut latest).await?;
            command(&state,Command::Observation(latest.unwrap().1)).await?;
            command(&state,Command::Advance(view.clone())).await?;view=post_latest(&owner_http,&state).await?;
            command(&state,Command::Available(view.clone())).await?;view=post_latest(&owner_http,&state).await?;
            ensure!(view.funds.withdrawability==CapitalExitWithdrawabilityV1::Simulated && view.funds.released_cash_amount.as_ref().is_some_and(|m|m.amount=="70".parse().unwrap()),"native_release_not_observed");
            ensure!(view.state!=CapitalExitStateV1::Completed,"cash_release_is_not_withdrawal");
            let position=cache.borrow().positions_open(None,None,None,Some(&AccountId::from(ACCOUNT)),None).into_iter().next().unwrap().clone();
            ensure!(position.quantity==Quantity::from("60.00") && owner.gate().issued_order_ids().len()==1,"native_partial_reduction_or_identity_mismatch");
            Ok::<Value,anyhow::Error>(json!({"intent":view,"source_id":source,"assessment":assessment,"native_position":position,"original_evidence":state.borrow().evidence,"original_fills":state.borrow().fills,"original_cancellations":state.borrow().cancels,"actual_execution":"OFFICIAL_SANDBOX_ONLY","withdrawal_performed":false}))
        }.await;
        if let Err(error) = &outcome {
            let state = state.borrow();
            eprintln!(
                "native_pipeline_failure {}",
                json!({"at":Utc::now(),"error":error.to_string(),"native_error":state.error,"original_fills":state.fills,"original_cancellations":state.cancels,"relayed_records":consumed})
            );
        }
        handle.stop();
        outcome
    };
    let (run, result) = tokio::join!(node.run_with_mode(NodeRunMode::Hosted), drive);
    run.unwrap();
    let (retained_records, dropped_events) = observer.finish().unwrap();
    let result = result.unwrap();
    relay(&input, &http, &owner, &retained, &mut consumed, &mut latest)
        .await
        .unwrap();
    assert_eq!(dropped_events, DbCounter::ZERO);
    assert_eq!(u64::try_from(consumed).unwrap(), retained_records.get());
    std::fs::write(&input.result, serde_json::to_vec(&result).unwrap()).unwrap();
}
