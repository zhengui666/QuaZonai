//! Regression fixtures use the stock LiveNode queues, native wall clock and
//! Sandbox matcher. No fabricated order events, account edits or matching rules.
use super::*;
use anyhow::{Result, ensure};

async fn wait_for(state: &Rc<RefCell<State>>, ready: impl Fn() -> bool) -> Result<()> {
    let end = tokio::time::Instant::now() + Duration::from_secs(5);
    while !ready() {
        if let Some(error) = &state.borrow().error {
            return Err(anyhow::anyhow!(error.clone()));
        }
        ensure!(
            tokio::time::Instant::now() < end,
            "native_lifecycle_condition_timeout"
        );
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    if let Some(error) = &state.borrow().error {
        return Err(anyhow::anyhow!(error.clone()));
    }
    Ok(())
}
async fn dispatch(state: &Rc<RefCell<State>>, command: Command) -> Result<()> {
    ensure!(
        state.borrow().command.is_none(),
        "native_lifecycle_command_busy"
    );
    state.borrow_mut().command = Some(command);
    // Deliberately no quote publication here. A stock native timer delivers it.
    wait_for(state, || state.borrow().command.is_none()).await
}
#[derive(Clone, Copy, PartialEq)]
enum StopCase {
    Pause,
    Cancel,
    Deadline,
    BeforeReduction,
    OpenerFillRace,
}

async fn native_lifecycle(case: StopCase) {
    let directory = tempfile::tempdir().unwrap();
    let sandbox = SandboxExecutionClientConfig {
        account_id: AccountId::from(ACCOUNT),
        venue: Venue::from("QZEXIT"),
        starting_balances: vec![Money::from("100 USDC")],
        base_currency: Some(Currency::USDC()),
        account_type: AccountType::Cash,
        liquidity_consumption: true,
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
            snapshot_interval_ms: Some(20),
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
    let source = Id::new();
    let observation = Id::new();
    let owner = Rc::new(
        NativeCapitalExitOwner::attach_sandbox(
            &node,
            SandboxCapitalOwnerBinding {
                managed_account_key: "sandbox-physical-QZEXIT-001".into(),
                owner_binding_ref: "controlled-owner-v1".into(),
                account_source_id: Some(source),
                strategy_id: StrategyId::from(STRATEGY),
                client_id: ClientId::from(CLIENT),
                instrument_id: InstrumentId::from(INSTRUMENT),
            },
            &sandbox,
            directory.path(),
        )
        .unwrap(),
    );
    let state = Rc::new(RefCell::new(State {
        timer_commands: true,
        ..Default::default()
    }));
    node.add_strategy(FixtureStrategy {
        core: StrategyCore::new(StrategyConfig {
            strategy_id: Some(StrategyId::from(STRATEGY)),
            ..Default::default()
        }),
        owner: owner.clone(),
        state: state.clone(),
        source_observation_id: observation,
        sequence: 0,
    })
    .unwrap();
    let retained = directory.path().join("original-native.ndjson");
    let observer = NativeNodeObserver::attach_client_bound(
        &node,
        Id::new(),
        ClientId::from(CLIENT),
        &retained,
        1024,
    )
    .unwrap();
    let handle = node.handle();
    let cache = node.kernel().cache();
    let before_reduction = matches!(case, StopCase::BeforeReduction | StopCase::OpenerFillRace);
    let drive = async {
        let result=async {
            wait_for(&state,||handle.is_running()).await?;
            let mut initial=quote();initial.bid_size=Quantity::from("5.00");
            get_data_event_sender().send(DataEvent::Data(Data::Quote(initial)))?;
            wait_for(&state,||cache.borrow().quote(&InstrumentId::from(INSTRUMENT)).is_some()).await?;
            dispatch(&state,Command::Seed).await?;
            wait_for(&state,||state.borrow().fills.len()==1 && cache.borrow().order(&ClientOrderId::from("OPENING-ORDER")).is_some_and(|o|o.status()==OrderStatus::Accepted)).await?;
            let mut request=request(source,observation);
            if case==StopCase::Deadline {
                if let CapitalExitPolicyV1::BoundedLimit{deadline,..}=&mut request.policy {*deadline=Utc::now()+ChronoDuration::milliseconds(5500);}
            }
            dispatch(&state,Command::Assess(request)).await?;
            let mut view=claimed(state.borrow().assessment.as_ref().unwrap());
            dispatch(&state,Command::Fence(view.clone())).await?;
            if !before_reduction {
                dispatch(&state,Command::Advance(view.clone())).await?;
                wait_for(&state,||state.borrow().cancels.len()==1).await?;
                dispatch(&state,Command::Advance(view.clone())).await?;
                wait_for(&state,||state.borrow().fills.len()==2).await?;
                let exit=owner.gate().issued_order_ids()[0].clone();
                ensure!(cache.borrow().order(&ClientOrderId::from(exit.as_str())).is_some_and(|o|o.status()==OrderStatus::PartiallyFilled && o.leaves_qty()==Quantity::from("35.00")),"native_gtc_remainder_missing");
                // Let the real original quote age without altering cache/clocks.
                tokio::time::sleep(Duration::from_millis(5650)).await;
                ensure!(Utc::now().timestamp_nanos_opt().unwrap() as u64 - cache.borrow().quote(&InstrumentId::from(INSTRUMENT)).unwrap().ts_init.as_u64() > 5_000_000_000,"quote_did_not_age");
                dispatch(&state,Command::ExpectStaleEvidence(view.clone())).await?;
            }
            if case!=StopCase::Deadline {
                view.command_id=Id::new();view.account_control_epoch=DbCounter::new(2).unwrap();
                view.state=if case==StopCase::Pause{CapitalExitStateV1::Paused}else{CapitalExitStateV1::CancellingExit};
                view.owner_command=CapitalExitOwnerCommandV1{schema_version:SchemaV1,command_id:view.command_id,account_control_epoch:view.account_control_epoch,instruction:if case==StopCase::Pause{CapitalExitOwnerInstructionV1::Pause{}}else{CapitalExitOwnerInstructionV1::Cancel{}}};
            }
            dispatch(&state,Command::Fence(view.clone())).await?;
            let race=if case==StopCase::OpenerFillRace{let mut q=quote();q.bid_price=Price::from("0.300");q.ask_price=Price::from("0.300");Some(q)}else{None};
            dispatch(&state,Command::CancellationProbe(view.clone(),race)).await?;
            let target=if before_reduction{ClientOrderId::from("OPENING-ORDER")}else{ClientOrderId::from(owner.gate().issued_order_ids()[0].as_str())};
            wait_for(&state,||cache.borrow().order(&target).is_some_and(|o|o.is_closed())).await?;
            dispatch(&state,Command::Advance(view.clone())).await?;
            let expected=if matches!(case,StopCase::Pause|StopCase::Deadline){CapitalExitStateV1::Paused}else{CapitalExitStateV1::CancelledReserved};
            ensure!(matches!(&state.borrow().evidence.last().unwrap().evidence,CapitalExitEvidenceKindV1::NativeProgress{phase,released_cash_amount:None,..} if *phase==expected),"native_terminal_phase_mismatch");
            ensure!(owner.gate().control().unwrap().funds.reserved_amount.amount=="70".parse().unwrap(),"cancellation_changed_reserved_budget");
            let expected_qty=if case==StopCase::OpenerFillRace{"110.00"}else if before_reduction{"100.00"}else{"95.00"};
            ensure!(cache.borrow().positions_open(None,None,None,Some(&AccountId::from(ACCOUNT)),None)[0].quantity==Quantity::from(expected_qty),"cancellation_bought_back_or_missed_original_fill");
            if before_reduction {ensure!(owner.gate().issued_order_ids().is_empty(),"cancel_before_reduction_submitted_exit");}
            else {ensure!(owner.gate().issued_order_ids().len()==1 && state.borrow().fills[1].last_qty==Quantity::from("5.00"),"partial_fill_replayed_or_replaced");}
            if case==StopCase::OpenerFillRace {ensure!(state.borrow().fills.len()==2 && state.borrow().fills[1].client_order_id==ClientOrderId::from("OPENING-ORDER") && state.borrow().cancels.is_empty(),"native_fill_race_was_fabricated_as_cancel");}
            let fills=state.borrow().fills.len();
            dispatch(&state,Command::Advance(view.clone())).await?;
            ensure!(state.borrow().fills.len()==fills,"replayed_cancel_duplicated_native_fill");
            if !before_reduction {dispatch(&state,Command::ExpectStaleEvidence(view)).await?;}
            Ok::<(),anyhow::Error>(())
        }.await;
        handle.stop();
        result
    };
    let (run, result) = tokio::join!(node.run_with_mode(NodeRunMode::Hosted), drive);
    run.unwrap();
    observer.finish().unwrap();
    result.unwrap();
}
#[tokio::test(flavor = "current_thread")]
async fn stale_quote_pause_cancels_original_partial_gtc() {
    native_lifecycle(StopCase::Pause).await;
}
#[tokio::test(flavor = "current_thread")]
async fn stale_quote_cancel_waits_for_all_native_terminal_orders() {
    native_lifecycle(StopCase::Cancel).await;
}
#[tokio::test(flavor = "current_thread")]
async fn stale_quote_deadline_cancels_original_partial_gtc() {
    native_lifecycle(StopCase::Deadline).await;
}
#[tokio::test(flavor = "current_thread")]
async fn cancel_before_reduction_waits_through_pending_cancel() {
    native_lifecycle(StopCase::BeforeReduction).await;
}
#[tokio::test(flavor = "current_thread")]
async fn opening_fill_race_is_observed_once_before_cancel_terminal() {
    native_lifecycle(StopCase::OpenerFillRace).await;
}
