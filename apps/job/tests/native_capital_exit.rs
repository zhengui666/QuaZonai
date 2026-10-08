//! Focused official Cash/Sandbox owner acceptance. These original native reports
//! are not HTTP/Store acceptance; the composed business-path fixture is separate.
//! No credentials, venue network, real cancellation, transfer or withdrawal.
use chrono::{Duration as ChronoDuration, Utc};
use contracts::{
    DbCounter, Id, Revision, SchemaV1, account_observation::AccountMoneyV1, capital_exit::*,
    forward::ForwardEnvironmentV1,
};
use job::{
    capital_exit_gate::CapitalExitGate,
    native_capital_exit::{NativeCapitalExitOwner, SandboxCapitalOwnerBinding},
    native_node_observer::NativeNodeObserver,
};
use nautilus_common::{
    actor::DataActor, enums::Environment, live::get_data_event_sender,
    logging::logger::LoggerConfig, messages::DataEvent,
};
use nautilus_core::UnixNanos;
use nautilus_live::node::{LiveNode, NodeRunMode};
use nautilus_model::{
    data::{Data, QuoteTick},
    enums::{AccountType, OrderSide, OrderStatus, TimeInForce},
    events::{OrderCanceled, OrderFilled},
    identifiers::{AccountId, ClientId, ClientOrderId, InstrumentId, StrategyId, TraderId, Venue},
    instruments::{InstrumentAny, stubs::binary_option},
    orders::Order,
    types::{Currency, Money, Price, Quantity},
};
use nautilus_portfolio::config::PortfolioConfig;
use nautilus_sandbox::{SandboxExecutionClientConfig, SandboxExecutionClientFactory};
use nautilus_trading::{
    nautilus_strategy,
    strategy::{Strategy, StrategyConfig, StrategyCore, StrategyNative},
};
use std::{cell::RefCell, rc::Rc, time::Duration};

const ACCOUNT: &str = "QZEXIT-001";
const CLIENT: &str = "QZ-EXIT-SANDBOX";
const STRATEGY: &str = "EXIT-FIXTURE-001";
const INSTRUMENT: &str = "YES.QZEXIT";
#[derive(Clone)]
enum Command {
    Seed,
    Assess(CapitalExitPreviewRequestV1),
    Fence(CapitalExitViewV1),
    Advance(CapitalExitViewV1),
    Available(CapitalExitViewV1),
    TryObsoleteTarget,
    Observation(Id),
    CancellationProbe(CapitalExitViewV1, Option<QuoteTick>),
    ExpectStaleEvidence(CapitalExitViewV1),
}
#[derive(Default)]
struct State {
    command: Option<Command>,
    assessment: Option<CapitalExitOwnerAssessmentV1>,
    evidence: Vec<CapitalExitOwnerEvidenceV1>,
    fills: Vec<OrderFilled>,
    cancels: Vec<OrderCanceled>,
    error: Option<String>,
    obsolete_blocked: bool,
    rejected_payloads: usize,
    rejected_refences: usize,
    rejected_stale_evidence: usize,
    timer_commands: bool,
}
struct FixtureStrategy {
    core: StrategyCore,
    owner: Rc<NativeCapitalExitOwner>,
    state: Rc<RefCell<State>>,
    source_observation_id: Id,
    sequence: u64,
}
impl std::fmt::Debug for FixtureStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FixtureStrategy").finish_non_exhaustive()
    }
}
nautilus_strategy!(FixtureStrategy, {
    fn on_order_filled(&mut self, event: &OrderFilled) {
        self.state.borrow_mut().fills.push(event.clone());
    }
    fn on_order_canceled(&mut self, event: &OrderCanceled) {
        self.state.borrow_mut().cancels.push(event.clone());
    }
});
impl FixtureStrategy {
    fn process(&mut self, command: Command) -> anyhow::Result<()> {
        let owner = self.owner.clone();
        self.sequence += 1;
        let sequence = DbCounter::new(self.sequence).unwrap();
        match command {
            Command::Observation(id) => self.source_observation_id = id,
            Command::Seed => {
                for (quantity, price, id) in [
                    ("100.00", "0.500", "SEED-HOLDINGS"),
                    ("10.00", "0.300", "OPENING-ORDER"),
                ] {
                    let order = self.order_factory().limit(
                        InstrumentId::from(INSTRUMENT),
                        OrderSide::Buy,
                        Quantity::from(quantity),
                        Price::from(price),
                        Some(TimeInForce::Gtc),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(ClientOrderId::from(id)),
                    );
                    owner.gate().check_target(DbCounter::ZERO, &order)?;
                    self.submit_order(order, None, Some(ClientId::from(CLIENT)), None)?;
                }
            }
            Command::Assess(request) => {
                self.state.borrow_mut().assessment =
                    Some(owner.assess(self, request, Revision::INITIAL, sequence)?)
            }
            Command::Fence(view) => {
                let evidence = owner.fence(self, &view, self.source_observation_id, sequence)?;
                self.state.borrow_mut().evidence.push(evidence);
                let original = owner.gate().control().unwrap();
                for field in ["reserve", "plan", "preview", "claim"] {
                    let mut altered = view.clone();
                    match field {
                        "reserve" => altered.funds.reserved_amount.amount = "99".parse().unwrap(),
                        "plan" => altered.plan_artifact_id = Id::new(),
                        "preview" => altered.preview_id = Id::new(),
                        "claim" => altered.external_claim_id = Some("REPLACED-CLAIM".into()),
                        _ => unreachable!(),
                    }
                    let error = owner
                        .fence(self, &altered, self.source_observation_id, sequence)
                        .unwrap_err();
                    anyhow::ensure!(
                        error.to_string().contains("control_replay_conflict"),
                        "same_epoch_refence_mutation_accepted"
                    );
                    anyhow::ensure!(
                        owner.gate().control().as_ref() == Some(&original),
                        "refence_changed_durable_authority"
                    );
                    self.state.borrow_mut().rejected_refences += 1;
                }
            }
            Command::ExpectStaleEvidence(view) => {
                let error = owner
                    .simulated_availability(self, &view, self.source_observation_id, sequence)
                    .unwrap_err();
                anyhow::ensure!(
                    error.to_string().contains("quote_stale"),
                    "stale_availability_not_rejected"
                );
                self.state.borrow_mut().rejected_stale_evidence += 1;
                if matches!(
                    &view.owner_command.instruction,
                    CapitalExitOwnerInstructionV1::Start {}
                        | CapitalExitOwnerInstructionV1::Resume {}
                ) && matches!(&view.policy, CapitalExitPolicyV1::BoundedLimit {deadline,..} if *deadline > Utc::now())
                {
                    let error = owner
                        .advance(self, &view, self.source_observation_id, sequence)
                        .unwrap_err();
                    anyhow::ensure!(
                        error.to_string().contains("quote_stale"),
                        "stale_new_reduction_not_rejected"
                    );
                    self.state.borrow_mut().rejected_stale_evidence += 1;
                }
            }
            Command::CancellationProbe(view, race) => {
                // LiveNode queues native execution commands. Inspect twice on
                // this owner callback before that queue can deliver cancellation.
                for _ in 0..2 {
                    self.sequence += 1;
                    let evidence = owner.advance(
                        self,
                        &view,
                        self.source_observation_id,
                        DbCounter::new(self.sequence).unwrap(),
                    )?;
                    anyhow::ensure!(
                        matches!(&evidence.evidence, CapitalExitEvidenceKindV1::NativeProgress {phase:CapitalExitStateV1::CancellingExit,released_cash_amount:None,reason_codes,..}
                        if reason_codes.iter().any(|r|r=="capital_exit_native_cancellation_pending")),
                        "pending_native_cancel_reported_terminal"
                    );
                    self.state.borrow_mut().evidence.push(evidence);
                }
                if let Some(quote) = race {
                    // Controlled public native input seam: make the matching
                    // quote arrive before the queued cancel reaches Sandbox.
                    // The official matcher creates the fill, never this fixture.
                    self.unsubscribe_quotes(InstrumentId::from(INSTRUMENT), None, None);
                    nautilus_common::msgbus::publish_quote(
                        nautilus_common::msgbus::switchboard::get_quotes_topic(quote.instrument_id),
                        &quote,
                    );
                }
            }
            Command::Advance(view) => {
                // Reusing a real command ID/epoch cannot replace its frozen policy.
                let mut altered = view.clone();
                altered.policy = CapitalExitPolicyV1::CashOnly {};
                let error = owner
                    .advance(self, &altered, self.source_observation_id, sequence)
                    .unwrap_err();
                anyhow::ensure!(
                    error.to_string().contains("control_payload_mismatch"),
                    "changed_policy_reached_native_action"
                );
                self.state.borrow_mut().rejected_payloads += 1;
                let evidence = owner.advance(self, &view, self.source_observation_id, sequence)?;
                self.state.borrow_mut().evidence.push(evidence);
            }
            Command::Available(view) => {
                let evidence = owner.simulated_availability(
                    self,
                    &view,
                    self.source_observation_id,
                    sequence,
                )?;
                self.state.borrow_mut().evidence.push(evidence);
            }
            Command::TryObsoleteTarget => {
                let order = self.order_factory().market(
                    InstrumentId::from(INSTRUMENT),
                    OrderSide::Buy,
                    Quantity::from("1.00"),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                );
                self.state.borrow_mut().obsolete_blocked =
                    owner.gate().check_target(DbCounter::ZERO, &order).is_err();
            }
        }
        Ok(())
    }
}
impl DataActor for FixtureStrategy {
    fn on_start(&mut self) -> anyhow::Result<()> {
        self.subscribe_quotes(InstrumentId::from(INSTRUMENT), None, None);
        self.clock().set_timer_ns(
            "controlled-owner-commands",
            10_000_000,
            None,
            None,
            None,
            Some(false),
            Some(false),
        )?;
        Ok(())
    }
    fn on_time_event(&mut self, _event: &nautilus_common::timer::TimeEvent) -> anyhow::Result<()> {
        if self.state.borrow().timer_commands {
            self.run_pending();
        }
        Ok(())
    }
    fn on_quote(&mut self, _quote: &QuoteTick) -> anyhow::Result<()> {
        self.run_pending();
        Ok(())
    }
}
impl FixtureStrategy {
    fn run_pending(&mut self) {
        let command = self.state.borrow_mut().command.take();
        if let Some(command) = command {
            if let Err(error) = self.process(command) {
                self.state.borrow_mut().error = Some(error.to_string());
            }
        }
    }
}
fn quote() -> QuoteTick {
    let now = UnixNanos::from(Utc::now().timestamp_nanos_opt().unwrap() as u64);
    QuoteTick::new(
        InstrumentId::from(INSTRUMENT),
        Price::from("0.500"),
        Price::from("0.500"),
        Quantity::from("1000.00"),
        Quantity::from("1000.00"),
        now,
        now,
    )
}
fn request(source: Id, observation: Id) -> CapitalExitPreviewRequestV1 {
    CapitalExitPreviewRequestV1 {
        schema_version: SchemaV1,
        account_source_id: source,
        expected_source_observation_id: observation,
        scope: CapitalExitScopeV1::Amount {
            amount: "70".parse().unwrap(),
            currency: "USDC".into(),
        },
        policy: CapitalExitPolicyV1::BoundedLimit {
            deadline: Utc::now() + ChronoDuration::seconds(30),
            legs: vec![CapitalExitReductionLegV1 {
                instrument_id: INSTRUMENT.into(),
                maximum_reduction_quantity: "60".parse().unwrap(),
                minimum_sell_price: "0.5".parse().unwrap(),
            }],
            max_execution_cost: CapitalExitCostLimitV1 {
                amount: "0".parse().unwrap(),
                currency: "USDC".into(),
                reference_evidence_id: observation,
            },
        },
    }
}
fn claimed(assessment: &CapitalExitOwnerAssessmentV1) -> CapitalExitViewV1 {
    let amount = assessment.request.scope.money();
    let zero = AccountMoneyV1 {
        amount: "0".parse().unwrap(),
        currency: "USDC".into(),
    };
    let now = Utc::now();
    let command_id = Id::new();
    CapitalExitViewV1 {
        schema_version: SchemaV1,
        id: Id::new(),
        project_id: Id::new(),
        account_source_id: assessment.account_source_id,
        environment: ForwardEnvironmentV1::Paper,
        managed_account_key: "sandbox-physical-QZEXIT-001".into(),
        owner_binding_ref: "controlled-owner-v1".into(),
        account_control_epoch: DbCounter::new(1).unwrap(),
        account_control_revision: Revision::INITIAL,
        revision: Revision::INITIAL,
        state: CapitalExitStateV1::Fencing,
        last_phase: CapitalExitStateV1::Requested,
        reason_codes: vec![],
        preview_id: Id::new(),
        plan_artifact_id: Id::new(),
        scope: assessment.request.scope.clone(),
        policy: assessment.request.policy.clone(),
        command_id,
        owner_command: CapitalExitOwnerCommandV1 {
            schema_version: SchemaV1,
            command_id,
            account_control_epoch: DbCounter::new(1).unwrap(),
            instruction: CapitalExitOwnerInstructionV1::Start {},
        },
        external_claim_id: Some("local-owner-component-fixture".into()),
        remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
        funds: CapitalExitFundsV1 {
            requested_amount: amount.clone(),
            reserved_amount: amount,
            released_cash_amount: None,
            verified_withdrawable_amount: None,
            reconciled_withdrawal_amount: zero,
            unreleased_amount: None,
            evidence_asof: None,
            evidence_valid_until: None,
            withdrawability: CapitalExitWithdrawabilityV1::Unverified,
        },
        evidence_refs: vec![],
        created_at: now,
        updated_at: now,
    }
}

#[tokio::test(flavor = "current_thread")]
async fn cash_sandbox_fences_cancels_and_reduces_only_needed_quantity() {
    let directory = tempfile::tempdir().unwrap();
    let sandbox = SandboxExecutionClientConfig {
        account_id: AccountId::from(ACCOUNT),
        venue: Venue::from("QZEXIT"),
        starting_balances: vec![Money::from("100 USDC")],
        base_currency: Some(Currency::USDC()),
        account_type: AccountType::Cash,
        ..Default::default()
    };
    let mut node = LiveNode::builder(TraderId::from("QZEXIT-001"), Environment::Sandbox)
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
            snapshot_interval_ms: Some(10),
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
    let gate = owner.gate();
    let state = Rc::new(RefCell::new(State::default()));
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
        256,
    )
    .unwrap();
    let handle = node.handle();
    let cache = node.kernel().cache();
    let mut stage = 0;
    let mut control = None;
    let drive = async {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(8);
        let mut ticks = tokio::time::interval(Duration::from_millis(20));
        loop {
            ticks.tick().await;
            if state.borrow().error.is_some() || tokio::time::Instant::now() >= deadline {
                handle.stop();
                break;
            }
            if !handle.is_running() {
                continue;
            }
            observer.heartbeat().unwrap();
            let command = match stage {
                0 => {
                    stage = 1;
                    Some(Command::Seed)
                }
                1 if state.borrow().fills.len() == 1
                    && cache
                        .borrow()
                        .order(&ClientOrderId::from("OPENING-ORDER"))
                        .is_some_and(|o| o.status() == OrderStatus::Accepted) =>
                {
                    stage = 2;
                    Some(Command::Assess(request(source, observation)))
                }
                2 if state.borrow().assessment.is_some() => {
                    let view = claimed(state.borrow().assessment.as_ref().unwrap());
                    control = Some(view.clone());
                    stage = 3;
                    Some(Command::Fence(view))
                }
                3 if !state.borrow().evidence.is_empty() => {
                    stage = 4;
                    Some(Command::TryObsoleteTarget)
                }
                4 if state.borrow().obsolete_blocked => {
                    stage = 5;
                    Some(Command::Advance(control.clone().unwrap()))
                }
                5 if !state.borrow().cancels.is_empty() => {
                    stage = 6;
                    Some(Command::Advance(control.clone().unwrap()))
                }
                6 if state.borrow().fills.len() == 2 => {
                    stage = 7;
                    Some(Command::Advance(control.clone().unwrap()))
                }
                7 if state.borrow().evidence.len() >= 4 => {
                    stage = 8;
                    Some(Command::Available(control.clone().unwrap()))
                }
                8 if state.borrow().evidence.iter().any(|e| {
                    matches!(
                        e.evidence,
                        CapitalExitEvidenceKindV1::WithdrawabilityObserved { .. }
                    )
                }) =>
                {
                    let mut paused = control.clone().unwrap();
                    paused.command_id = Id::new();
                    paused.account_control_epoch = DbCounter::new(2).unwrap();
                    paused.state = CapitalExitStateV1::Paused;
                    paused.owner_command = CapitalExitOwnerCommandV1 {
                        schema_version: SchemaV1,
                        command_id: paused.command_id,
                        account_control_epoch: paused.account_control_epoch,
                        instruction: CapitalExitOwnerInstructionV1::Pause {},
                    };
                    control = Some(paused.clone());
                    stage = 9;
                    Some(Command::Fence(paused))
                }
                9 if state.borrow().evidence.len() >= 6 => {
                    stage = 10;
                    Some(Command::Advance(control.clone().unwrap()))
                }
                10 if state.borrow().evidence.len() >= 7 => {
                    state.borrow_mut().obsolete_blocked = false;
                    stage = 11;
                    Some(Command::TryObsoleteTarget)
                }
                11 if state.borrow().obsolete_blocked => {
                    handle.stop();
                    break;
                }
                _ => None,
            };
            if let Some(command) = command {
                state.borrow_mut().command = Some(command);
                get_data_event_sender()
                    .send(DataEvent::Data(Data::Quote(quote())))
                    .unwrap();
            }
        }
    };
    let (result, ()) = tokio::join!(node.run_with_mode(NodeRunMode::Hosted), drive);
    result.unwrap();
    observer.finish().unwrap();
    assert!(state.borrow().error.is_none(), "{:?}", state.borrow().error);
    assert_eq!(stage, 11, "native fixture never completed");
    let state = state.borrow();
    let assessment = state.assessment.as_ref().unwrap();
    assert_eq!(assessment.capability, CapitalExitCapabilityV1::Supported);
    assert_eq!(assessment.proposed_cancellations.len(), 1);
    assert_eq!(
        assessment.proposed_cancellations[0].native_client_order_id,
        "OPENING-ORDER"
    );
    assert!(state.obsolete_blocked);
    assert!(state.rejected_payloads >= 4);
    assert!(state.rejected_refences >= 8);
    assert_eq!(state.cancels.len(), 1);
    assert_eq!(state.fills.len(), 2);
    assert_eq!(state.fills[1].order_side, OrderSide::Sell);
    assert_eq!(state.fills[1].last_qty, Quantity::from("40.00"));
    let cache = cache.borrow();
    let position = cache.positions_open(None, None, None, Some(&AccountId::from(ACCOUNT)), None);
    assert_eq!(position.len(), 1);
    assert_eq!(position[0].quantity, Quantity::from("60.00"));
    let exits = gate.issued_order_ids();
    assert_eq!(exits.len(), 1);
    let order = cache
        .order(&ClientOrderId::from(exits[0].as_str()))
        .unwrap();
    assert!(order.is_reduce_only());
    assert_eq!(order.price(), Some(Price::from("0.500")));
    assert_eq!(order.status(), OrderStatus::Filled);
    let available = state
        .evidence
        .iter()
        .find_map(|e| match &e.evidence {
            CapitalExitEvidenceKindV1::WithdrawabilityObserved {
                available_cash,
                basis,
                ..
            } => Some((available_cash, basis)),
            _ => None,
        })
        .unwrap();
    assert_eq!(available.0.amount, "70".parse().unwrap());
    assert_eq!(
        *available.1,
        CapitalExitAvailabilityBasisV1::ControlledSandbox
    );
    assert!(std::fs::read_to_string(retained).unwrap().lines().any(|l| {
        serde_json::from_str::<serde_json::Value>(l).unwrap()["observation"]["snapshot"].is_object()
    }));
    assert_eq!(gate.epoch().get(), 2);
    assert!(matches!(
        state.evidence.last().unwrap().evidence,
        CapitalExitEvidenceKindV1::NativeProgress {
            phase: CapitalExitStateV1::Paused,
            ..
        }
    ));
}

#[test]
fn durable_gate_cannot_reset_by_session_alias_or_restart() {
    let directory = tempfile::tempdir().unwrap();
    let first =
        CapitalExitGate::open_fresh_sandbox(directory.path(), "physical-a", "owner-a", ACCOUNT)
            .unwrap();
    assert!(!first.recovery_required());
    assert!(
        CapitalExitGate::open_fresh_sandbox(directory.path(), "physical-a", "owner-a", ACCOUNT)
            .is_err()
    );
    drop(first);
    assert!(
        CapitalExitGate::open_fresh_sandbox(
            directory.path(),
            "project-session-alias",
            "owner-a",
            ACCOUNT
        )
        .is_err()
    );
    let restarted =
        CapitalExitGate::open_fresh_sandbox(directory.path(), "physical-a", "owner-a", ACCOUNT)
            .unwrap();
    assert!(restarted.recovery_required());
}

#[cfg(feature = "native-paper")]
#[path = "support/capital_exit_http_pipeline.rs"]
mod capital_exit_http_pipeline;

#[test]
fn unguarded_algorithm_emulator_child_and_wrong_epoch_are_rejected() {
    use nautilus_model::{
        enums::{OrderType, TriggerType},
        identifiers::ExecAlgorithmId,
        orders::builder::OrderTestBuilder,
    };
    let directory = tempfile::tempdir().unwrap();
    let gate =
        CapitalExitGate::open_fresh_sandbox(directory.path(), "physical-a", "owner-a", ACCOUNT)
            .unwrap();
    let ordinary = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(InstrumentId::from(INSTRUMENT))
        .quantity(Quantity::from("1.00"))
        .build();
    gate.check_target(DbCounter::ZERO, &ordinary).unwrap();
    assert!(
        gate.check_target(DbCounter::new(1).unwrap(), &ordinary)
            .unwrap_err()
            .to_string()
            .contains("obsolete_target_epoch")
    );
    let algorithm = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(InstrumentId::from(INSTRUMENT))
        .quantity(Quantity::from("1.00"))
        .exec_algorithm_id(ExecAlgorithmId::from("TWAP"))
        .exec_spawn_id(ClientOrderId::from("ORIGINAL-SPAWN"))
        .build();
    let child = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(InstrumentId::from(INSTRUMENT))
        .quantity(Quantity::from("1.00"))
        .parent_order_id(ClientOrderId::from("ORIGINAL-PARENT"))
        .build();
    let spawned = OrderTestBuilder::new(OrderType::Market)
        .instrument_id(InstrumentId::from(INSTRUMENT))
        .quantity(Quantity::from("1.00"))
        .exec_spawn_id(ClientOrderId::from("ORIGINAL-SPAWN"))
        .build();
    let emulated = OrderTestBuilder::new(OrderType::Limit)
        .instrument_id(InstrumentId::from(INSTRUMENT))
        .quantity(Quantity::from("1.00"))
        .price(Price::from("0.500"))
        .emulation_trigger(TriggerType::BidAsk)
        .build();
    for order in [algorithm, child, spawned, emulated] {
        assert!(
            gate.check_target(DbCounter::ZERO, &order)
                .unwrap_err()
                .to_string()
                .contains("child_path_unsupported")
        );
    }
    drop(gate);
    let restarted =
        CapitalExitGate::open_fresh_sandbox(directory.path(), "physical-a", "owner-a", ACCOUNT)
            .unwrap();
    assert!(
        restarted
            .check_target(DbCounter::ZERO, &ordinary)
            .unwrap_err()
            .to_string()
            .contains("recovery_required")
    );
}

#[path = "support/capital_exit_lifecycle.rs"]
mod capital_exit_lifecycle;
