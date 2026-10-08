//! Synthetic engineering acceptance of the production streaming Paper adapter.
//! Uses the original BacktestEngine/TargetReplay, its native fee/latency and
//! generated order events. No market connection, real claim, credentials or PG.
use super::*;
use crate::paper_capital_exit::{NativeRequest, NativeResponse};
use contracts::{DbCounter, Id, Revision, SchemaV1, account_observation::*, capital_exit::*};
use nautilus_model::{
    accounts::Account,
    data::QuoteTick,
    orders::Order,
    types::{Price, Quantity},
};
use rust_decimal::Decimal;
use std::time::Duration;

#[path = "polymarket.rs"]
mod polymarket;
#[path = "../../../../tests/support/portfolio.rs"]
mod portfolio;

const SECOND: u64 = 1_000_000_000;

struct Fixture {
    session: PolymarketStreamingPaper,
    _directory: tempfile::TempDir,
    source: Id,
    project: Id,
    downstream: Id,
    last_quote: u64,
}

fn quote(at: u64, bid: &str, size: &str) -> Data {
    Data::Quote(QuoteTick::new(
        InstrumentId::from(polymarket::IDS[0]),
        Price::from(bid),
        Price::from("0.5100"),
        Quantity::from(size),
        Quantity::from("1000000.000000"),
        at.into(),
        at.into(),
    ))
}

impl Fixture {
    fn unbound(claim: contracts::strategy_portfolio::HandoffClaimViewV2) -> Self {
        let input: contracts::portfolio::AllocationInputV1 = serde_json::from_str(include_str!(
            "../../../../tests/contracts/allocation-input.json"
        ))
        .unwrap();
        let request = portfolio::request(&input);
        let mut settings = request.execution_settings;
        settings.starting_capital = "1000".parse().unwrap();
        settings.latency_model = portfolio::execution_models::latency(1_000_000);
        let now = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
        let base = (now / SECOND - 7) * SECOND;
        polymarket::settings(&mut settings, "0.02", now + 120 * SECOND);
        settings.fee_rates.truncate(1);
        let mut constraints = request.mandate.constraints;
        constraints.max_cash_weight = "1".parse().unwrap();
        constraints.min_net_exposure = "0".parse().unwrap();
        let account = FreshPaperCashV1 {
            downstream_id: claim.handoff.downstream_id,
            trader_id: "QZ-CAPITAL-PAPER-001".into(),
            account_id: "POLYMARKET-001".into(),
            base_currency: settings.base_currency.clone(),
            starting_capital: settings.starting_capital.clone(),
            execution_assumptions_id: Id::new(),
        };
        let target = NativeTargetPointV1 {
            schema_version: SchemaV1,
            asof_ns: DbCounter::new(base).unwrap(),
            valid_until_ns: DbCounter::new(now + 90 * SECOND).unwrap(),
            targets: vec![contracts::portfolio::AllocationTargetV1 {
                instrument_id: polymarket::IDS[0].into(),
                weight: "0.2".parse().unwrap(),
                currency: "pUSD".into(),
            }],
            cash_weight: "0.8".parse().unwrap(),
        };
        let mut instruments = polymarket::instruments("0.02", now + 120 * SECOND);
        instruments.truncate(1);
        let directory = tempfile::tempdir().unwrap();
        let session = PolymarketStreamingPaper::new_managed(
            &account,
            &settings,
            &constraints,
            target,
            instruments,
            vec![
                format!("{}-1-SECOND-MID-INTERNAL", polymarket::IDS[0])
                    .parse()
                    .unwrap(),
            ],
            &claim,
            directory.path(),
        )
        .unwrap();
        let mut fixture = Self {
            session,
            _directory: directory,
            source: Id::new(),
            project: claim.handoff.project_id,
            downstream: claim.handoff.downstream_id,
            last_quote: base,
        };
        fixture
            .session
            .push(quote(base, "0.4900", "1000000.000000"))
            .unwrap();
        fixture
            .session
            .push(quote(base + SECOND / 2, "0.4900", "1000000.000000"))
            .unwrap();
        assert!(
            fixture
                .session
                .engine
                .kernel()
                .cache
                .borrow()
                .orders(None, None, None, None, None)
                .is_empty(),
            "before authenticated source the original target is held without inventing a failure"
        );
        fixture
    }

    fn new() -> Self {
        let mut fixture = Self::unbound(crate::paper_service::tests::polymarket_claim());
        let base = fixture.last_quote;
        fixture.observe();
        for half in 2..=13 {
            fixture.last_quote = base + half * SECOND / 2;
            fixture
                .session
                .push(quote(
                    fixture.last_quote,
                    "0.4900",
                    if half == 13 {
                        "10.000000"
                    } else {
                        "1000000.000000"
                    },
                ))
                .unwrap();
        }
        assert_eq!(fixture.position(), Decimal::from(400));
        assert_eq!(
            fixture
                .session
                .engine
                .kernel()
                .trader
                .borrow()
                .strategy_ids(),
            vec![StrategyId::from("QZ-PAPER-001")]
        );
        assert_eq!(
            fixture
                .session
                .engine
                .kernel()
                .cache
                .borrow()
                .accounts(&AccountId::from("POLYMARKET-001"))
                .len(),
            1
        );
        fixture
    }

    fn observe(&mut self) -> Id {
        let NativeResponse::Observation(original) = self
            .session
            .control_capital_exit(NativeRequest::Observe)
            .unwrap()
        else {
            panic!("original observation");
        };
        let observation = Id::new();
        let receipt = AccountObservationReceiptV2 {
            schema_version: NativeClientObservationSchemaV2,
            replayed: false,
            native_client_id: original.native_client_id.clone(),
            resource: AccountObservationV1 {
                id: observation,
                source_id: self.source,
                downstream_id: self.downstream,
                observation: original.observation,
                gap_before: false,
                received_at: chrono::Utc::now(),
            },
        };
        let mut wrong = receipt.clone();
        wrong.native_client_id = "UNRELATED-DATA-ONLY-CLIENT".into();
        assert!(
            self.session
                .control_capital_exit(NativeRequest::Bind(Box::new(wrong)))
                .is_err()
        );
        self.session
            .control_capital_exit(NativeRequest::Bind(Box::new(receipt)))
            .unwrap();
        observation
    }

    fn position(&self) -> Decimal {
        self.session
            .engine
            .kernel()
            .cache
            .borrow()
            .positions(
                None,
                None,
                None,
                Some(&AccountId::from("POLYMARKET-001")),
                None,
            )
            .iter()
            .filter(|position| position.is_open())
            .map(|position| position.quantity.as_decimal())
            .sum()
    }

    fn free(&self) -> Decimal {
        self.session
            .engine
            .kernel()
            .cache
            .borrow()
            .account(&AccountId::from("POLYMARKET-001"))
            .unwrap()
            .balance_free(Some(Currency::from_str("pUSD").unwrap()))
            .unwrap()
            .as_decimal()
    }

    fn assess(&mut self, deadline: chrono::DateTime<chrono::Utc>) -> CapitalExitOwnerAssessmentV1 {
        let observation = self.observe();
        let scope = CapitalExitScopeV1::Amount {
            amount: "850".parse().unwrap(),
            currency: "pUSD".into(),
        };
        let policy = CapitalExitPolicyV1::BoundedLimit {
            deadline,
            legs: vec![CapitalExitReductionLegV1 {
                instrument_id: polymarket::IDS[0].into(),
                maximum_reduction_quantity: "200".parse().unwrap(),
                minimum_sell_price: "0.49".parse().unwrap(),
            }],
            max_execution_cost: CapitalExitCostLimitV1 {
                amount: "5".parse().unwrap(),
                currency: "pUSD".into(),
                reference_evidence_id: observation,
            },
        };
        let preview = CapitalExitPreviewV1 {
            schema_version: SchemaV1,
            id: Id::new(),
            project_id: self.project,
            account_source_id: self.source,
            original_observation_id: observation,
            environment: contracts::forward::ForwardEnvironmentV1::Paper,
            managed_account_key: Some(format!("paper-native:{}", self.source)),
            owner_binding_ref: Some(format!("nautilus-paper:{}", self.source)),
            expected_account_control_revision: Some(Revision::INITIAL),
            scope: scope.clone(),
            policy,
            plan_artifact_id: Id::new(),
            evidence_refs: vec![observation],
            valid_until: chrono::Utc::now() + chrono::Duration::seconds(4),
            funds: CapitalExitPreviewFundsV1 {
                requested: scope.money(),
                native_total_cash: None,
                native_free_cash: None,
                native_locked_cash: None,
                verified_idle_cash: None,
                estimated_release: None,
                native_equity: None,
                managed_capital_before: None,
                remaining_managed_capital: None,
                estimated_execution_cost: None,
                existing_unrealized_pnl: None,
            },
            proposed_cancellations: vec![],
            retained_protective_orders: vec![],
            reduction_legs: vec![],
            remaining_risk_evidence_id: None,
            capability: CapitalExitCapabilityV1::Blocked,
            reason_codes: vec![],
        };
        let NativeResponse::Assessment(value) = self
            .session
            .control_capital_exit(NativeRequest::Assess(Box::new(preview)))
            .unwrap()
        else {
            panic!("native assessment");
        };
        assert_eq!(
            value.capability,
            CapitalExitCapabilityV1::Supported,
            "{:?}",
            value.reason_codes
        );
        assert!(
            value
                .funds
                .estimated_execution_cost
                .as_ref()
                .unwrap()
                .amount
                .is_positive(),
            "original schedule is nonzero despite instrument fee labels"
        );
        value
    }

    fn claimed(&self, assessment: &CapitalExitOwnerAssessmentV1) -> CapitalExitViewV1 {
        let amount = assessment.request.scope.money();
        let command_id = Id::new();
        CapitalExitViewV1 {
            schema_version: SchemaV1,
            id: Id::new(),
            project_id: self.project,
            account_source_id: self.source,
            environment: contracts::forward::ForwardEnvironmentV1::Paper,
            managed_account_key: format!("paper-native:{}", self.source),
            owner_binding_ref: format!("nautilus-paper:{}", self.source),
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
            external_claim_id: Some("synthetic-production-adapter-native-test".into()),
            remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
            funds: CapitalExitFundsV1 {
                requested_amount: amount.clone(),
                reserved_amount: amount,
                released_cash_amount: None,
                verified_withdrawable_amount: None,
                reconciled_withdrawal_amount: AccountMoneyV1 {
                    amount: "0".parse().unwrap(),
                    currency: "pUSD".into(),
                },
                unreleased_amount: None,
                evidence_asof: None,
                evidence_valid_until: None,
                withdrawability: CapitalExitWithdrawabilityV1::Unverified,
            },
            evidence_refs: vec![],
            created_at: chrono::Utc::now(),
            updated_at: chrono::Utc::now(),
        }
    }

    fn next_quotes(&mut self, bid: &str, size: &str) {
        // Actual wall spacing in this controlled producer, never future native
        // timestamps invented merely to get the latency model to settle.
        std::thread::sleep(Duration::from_millis(2));
        self.last_quote = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
        self.session
            .push(quote(self.last_quote, bid, size))
            .unwrap();
        std::thread::sleep(Duration::from_millis(2));
        self.last_quote = chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64;
        self.session
            .push(quote(self.last_quote, bid, size))
            .unwrap();
    }

    fn evidence(&mut self, request: NativeRequest) -> CapitalExitOwnerEvidenceV1 {
        let NativeResponse::Evidence(value) = self.session.control_capital_exit(request).unwrap()
        else {
            panic!("native evidence");
        };
        value
    }
}

#[test]
fn production_paper_reuses_account_strategy_and_nonzero_native_fee() {
    std::thread::spawn(|| {
        let mut fixture = Fixture::new();
        let session = fixture.session.session_id();
        let initial_cash = fixture.free();
        assert!(
            initial_cash < Decimal::from(796),
            "original target fee was charged"
        );
        let assessment = fixture.assess(chrono::Utc::now() + chrono::Duration::seconds(30));
        let view = fixture.claimed(&assessment);
        fixture.evidence(NativeRequest::Fence(Box::new(view.clone())));
        assert!(
            fixture
                .session
                .control_capital_exit(NativeRequest::Advance(Box::new(view.clone())))
                .err()
                .unwrap()
                .to_string()
                .contains("capital_exit_native_command_time_pending")
        );
        assert!(
            fixture
                .session
                .capital_exit
                .as_ref()
                .unwrap()
                .gate()
                .issued_order_ids()
                .is_empty()
        );
        fixture.next_quotes("0.4900", "1000000.000000");
        fixture.evidence(NativeRequest::Advance(Box::new(view.clone())));
        fixture.next_quotes("0.4900", "1000000.000000");
        fixture.next_quotes("0.4900", "1000000.000000");
        assert_eq!(fixture.session.session_id(), session);
        assert!(fixture.position() > Decimal::from(200) && fixture.position() < Decimal::from(400));
        assert!(fixture.free() >= Decimal::from(850));
        assert!(
            fixture.free() < Decimal::from(851),
            "fee conservatism is bounded, not a full liquidation"
        );
        fixture.observe();
        fixture.evidence(NativeRequest::Advance(Box::new(view.clone())));
        let availability = fixture.evidence(NativeRequest::Availability(Box::new(view.clone())));
        assert!(matches!(
            availability.evidence,
            CapitalExitEvidenceKindV1::WithdrawabilityObserved {
                basis: CapitalExitAvailabilityBasisV1::ControlledSandbox,
                ..
            }
        ));
        let orders_before = fixture
            .session
            .engine
            .kernel()
            .cache
            .borrow()
            .orders(None, None, None, None, None)
            .len();
        fixture.next_quotes("0.4900", "1000000.000000");
        fixture.evidence(NativeRequest::Advance(Box::new(view)));
        assert_eq!(
            fixture
                .session
                .engine
                .kernel()
                .cache
                .borrow()
                .orders(None, None, None, None, None)
                .len(),
            orders_before
        );
        assert_eq!(fixture.session.consumed_targets(), 1);
    })
    .join()
    .unwrap();
}

fn silent_stop(mode: &str, race: bool) {
    let mut fixture = Fixture::new();
    let deadline =
        chrono::Utc::now() + chrono::Duration::seconds(if mode == "deadline" { 2 } else { 60 });
    let assessment = fixture.assess(deadline);
    let mut view = fixture.claimed(&assessment);
    fixture.evidence(NativeRequest::Fence(Box::new(view.clone())));
    fixture.next_quotes("0.4900", "10.000000");
    fixture.evidence(NativeRequest::Advance(Box::new(view.clone())));
    fixture.next_quotes("0.4900", "10.000000");
    fixture.next_quotes("0.4900", "10.000000");
    // Retain the last original timestamp exactly as production batching does.
    // On resumption its real book update, then the existing native cancel queue,
    // determines whether a concurrent fill occurs before cancellation.
    fixture.last_quote += 1;
    fixture
        .session
        .push(quote(
            fixture.last_quote,
            if race { "0.5000" } else { "0.4800" },
            "10.000000",
        ))
        .unwrap();
    let id = fixture
        .session
        .capital_exit
        .as_ref()
        .unwrap()
        .gate()
        .issued_order_ids()[0]
        .clone();
    let order_id = nautilus_model::identifiers::ClientOrderId::from(id.as_str());
    let before = fixture
        .session
        .engine
        .kernel()
        .cache
        .borrow()
        .try_order_owned(&order_id)
        .unwrap();
    assert!(before.filled_qty().as_decimal() > Decimal::ZERO && !before.is_closed());
    let clock = fixture
        .session
        .engine
        .kernel()
        .clock
        .borrow()
        .timestamp_ns();
    let original_quote = fixture
        .session
        .engine
        .kernel()
        .cache
        .borrow()
        .quote(&InstrumentId::from(polymarket::IDS[0]))
        .copied()
        .unwrap();
    let original_frames = fixture.session.original_frames;
    std::thread::sleep(Duration::from_millis(5200));
    assert!(
        fixture
            .session
            .control_capital_exit(NativeRequest::Availability(Box::new(view.clone())))
            .is_err()
    );
    if mode == "deadline" {
        fixture.session.capital_exit_deadline().unwrap();
    } else {
        view.command_id = Id::new();
        view.account_control_epoch = DbCounter::new(2).unwrap();
        view.owner_command = CapitalExitOwnerCommandV1 {
            schema_version: SchemaV1,
            command_id: view.command_id,
            account_control_epoch: view.account_control_epoch,
            instruction: if mode == "pause" {
                CapitalExitOwnerInstructionV1::Pause {}
            } else {
                CapitalExitOwnerInstructionV1::Cancel {}
            },
        };
        fixture.evidence(NativeRequest::Fence(Box::new(view.clone())));
    }
    let pending = fixture.evidence(NativeRequest::Advance(Box::new(view.clone())));
    assert!(matches!(
        pending.evidence,
        CapitalExitEvidenceKindV1::NativeProgress {
            phase: CapitalExitStateV1::CancellingExit,
            released_cash_amount: None,
            ..
        }
    ));
    assert_eq!(
        fixture
            .session
            .engine
            .kernel()
            .clock
            .borrow()
            .timestamp_ns(),
        clock
    );
    assert_eq!(fixture.session.original_frames, original_frames);
    assert_eq!(
        fixture
            .session
            .engine
            .kernel()
            .cache
            .borrow()
            .quote(&InstrumentId::from(polymarket::IDS[0]))
            .copied()
            .unwrap(),
        original_quote
    );
    assert!(
        !fixture
            .session
            .engine
            .kernel()
            .cache
            .borrow()
            .order(&order_id)
            .unwrap()
            .is_closed()
    );
    fixture.next_quotes(if race { "0.5000" } else { "0.4800" }, "10.000000");
    fixture.next_quotes(if race { "0.5000" } else { "0.4800" }, "10.000000");
    let after = fixture
        .session
        .engine
        .kernel()
        .cache
        .borrow()
        .try_order_owned(&order_id)
        .unwrap();
    assert!(after.is_closed());
    if race {
        assert!(
            after.filled_qty() > before.filled_qty(),
            "original quote/matcher owns cancel-fill ordering"
        );
    } else {
        assert_eq!(after.filled_qty(), before.filled_qty());
    }
    let terminal = fixture.evidence(NativeRequest::Advance(Box::new(view.clone())));
    let expected = if mode == "cancel" {
        CapitalExitStateV1::CancelledReserved
    } else {
        CapitalExitStateV1::Paused
    };
    assert!(
        matches!(terminal.evidence, CapitalExitEvidenceKindV1::NativeProgress { phase, released_cash_amount: None, .. } if phase == expected)
    );
    assert_eq!(
        view.funds
            .reserved_amount
            .amount
            .as_decimal()
            .to_plain_string(),
        "850"
    );
}

#[test]
fn production_paper_silent_pause_waits_for_original_cancel() {
    std::thread::spawn(|| silent_stop("pause", false))
        .join()
        .unwrap();
}

#[test]
fn production_paper_silent_cancel_observes_original_fill_race() {
    std::thread::spawn(|| silent_stop("cancel", true))
        .join()
        .unwrap();
}

#[test]
fn production_paper_silent_deadline_waits_without_advancing_clock() {
    std::thread::spawn(|| silent_stop("deadline", false))
        .join()
        .unwrap();
}

#[path = "paper_service_acceptance.rs"]
mod paper_service_acceptance;

/// Partial native regression for a first failed fence: an actually empty opener
/// set is still an original scope, and differs from never having captured it.
/// This deliberately does not claim the separate open-order/PG/Poller branch.
#[test]
fn production_paper_first_stale_fence_retains_empty_scope_for_silent_deadline() {
    std::thread::spawn(|| {
        let mut fixture = Fixture::new();
        let deadline = chrono::Utc::now() + chrono::Duration::seconds(7);
        let assessment = fixture.assess(deadline);
        let view = fixture.claimed(&assessment);
        let gate = fixture.session.capital_exit.as_ref().unwrap().gate();
        assert!(gate.opening_order_ids().is_none());
        assert!(gate.control().is_none());
        let clock = fixture
            .session
            .engine
            .kernel()
            .clock
            .borrow()
            .timestamp_ns();
        let original_quote = fixture
            .session
            .engine
            .kernel()
            .cache
            .borrow()
            .quote(&InstrumentId::from(polymarket::IDS[0]))
            .copied()
            .unwrap();
        let frames = fixture.session.original_frames;
        let position = fixture.position();
        let free = fixture.free();
        std::thread::sleep(Duration::from_millis(5200));
        let failure = fixture
            .session
            .control_capital_exit(NativeRequest::Fence(Box::new(view.clone())))
            .err()
            .expect("first original valuation is stale");
        assert!(
            failure
                .to_string()
                .contains("capital_exit_native_quote_stale")
        );
        assert_eq!(
            gate.control(),
            Some(view.clone()),
            "durable fence survives valuation rejection"
        );
        assert_eq!(
            gate.opening_order_ids(),
            Some(Vec::new()),
            "first fence must capture actual empty scope before inspect fails"
        );
        while chrono::Utc::now() <= deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        fixture
            .session
            .capital_exit_deadline()
            .expect("silent deadline must not terminate host with unknown opening scope");
        assert_eq!(gate.control(), Some(view));
        assert!(gate.issued_order_ids().is_empty());
        assert_eq!(
            fixture
                .session
                .engine
                .kernel()
                .clock
                .borrow()
                .timestamp_ns(),
            clock
        );
        assert_eq!(
            fixture
                .session
                .engine
                .kernel()
                .cache
                .borrow()
                .quote(&InstrumentId::from(polymarket::IDS[0]))
                .copied(),
            Some(original_quote)
        );
        assert_eq!(fixture.session.original_frames, frames);
        assert_eq!(fixture.position(), position);
        assert_eq!(fixture.free(), free);
        let evidence: Vec<CapitalExitOwnerEvidenceV1> =
            std::fs::read_dir(fixture._directory.path())
                .unwrap()
                .filter_map(|entry| entry.ok())
                .filter_map(|entry| std::fs::read(entry.path()).ok())
                .filter_map(|bytes| serde_json::from_slice(&bytes).ok())
                .collect();
        assert!(
            evidence.iter().any(|value| matches!(
                value.evidence,
                CapitalExitEvidenceKindV1::NativeProgress {
                    phase: CapitalExitStateV1::Paused,
                    released_cash_amount: None,
                    ..
                }
            )),
            "original deadline result was not retained"
        );
    })
    .join()
    .unwrap();
}

/// A local engineering fixture on the sole original Paper strategy/client. The
/// official OrderFactory/Strategy route creates the opener, and the official
/// matcher creates its acceptance/cancellation. No event or cash entry is made
/// by this test. It is not the full Poller/Server/PG acceptance bridge.
#[test]
fn production_paper_first_stale_fence_keeps_real_opener_pending_until_quote() {
    use nautilus_common::actor::registry::try_get_actor_unchecked;
    use nautilus_model::{
        enums::{OrderSide, OrderStatus, TimeInForce},
        events::OrderEventAny,
        identifiers::ClientOrderId,
    };
    use nautilus_trading::strategy::{Strategy, StrategyNative};

    std::thread::spawn(|| {
        let mut fixture = Fixture::new();
        let gate = fixture.session.capital_exit.as_ref().unwrap().gate();
        let original_session = fixture.session.session_id();
        let id = ClientOrderId::from("QZ-FIXTURE-ORIGINAL-OPENER");
        {
            // Same public registry seam as the production Paper adapter. This
            // executes between engine runs on its sole owning native thread.
            let mut strategy = try_get_actor_unchecked::<crate::simulation::TargetReplay>(
                &StrategyId::from("QZ-PAPER-001").inner(),
            ).expect("existing original TargetReplay");
            let order = strategy.order_factory().limit(
                InstrumentId::from(polymarket::IDS[0]),
                OrderSide::Buy,
                Quantity::from("10.000000"),
                Price::from("0.3000"),
                Some(TimeInForce::Gtc),
                None, None, None, None, None, None, None, None, None, None,
                Some(id),
            );
            gate.check_target(DbCounter::ZERO, &order).unwrap();
            strategy.submit_order(order, None, Some(ClientId::from("POLYMARKET")), None).unwrap();
        }
        fixture.next_quotes("0.4900", "1000000.000000");
        fixture.next_quotes("0.4900", "1000000.000000");
        let original = fixture.session.engine.kernel().cache.borrow().try_order_owned(&id).unwrap();
        assert_eq!(original.status(), OrderStatus::Accepted);
        assert_eq!(original.filled_qty().as_decimal(), Decimal::ZERO);
        assert!(original.events().iter().any(|event| matches!(event, OrderEventAny::Accepted(_))));
        assert_eq!(fixture.position(), Decimal::from(400));
        assert!(gate.opening_order_ids().is_none());
        assert!(gate.control().is_none());
        let deadline = chrono::Utc::now() + chrono::Duration::seconds(7);
        let assessment = fixture.assess(deadline);
        assert!(assessment.proposed_cancellations.iter().any(|reference|reference.native_client_order_id==id.to_string()));
        let view = fixture.claimed(&assessment);
        let clock = fixture.session.engine.kernel().clock.borrow().timestamp_ns();
        let original_quote = fixture.session.engine.kernel().cache.borrow()
            .quote(&InstrumentId::from(polymarket::IDS[0])).copied().unwrap();
        let frames = fixture.session.original_frames;
        let position = fixture.position();
        let free = fixture.free();
        std::thread::sleep(Duration::from_millis(5200));
        let failure = fixture.session.control_capital_exit(NativeRequest::Fence(Box::new(view.clone())))
            .err().expect("first fence must fail on the actually stale quote");
        assert!(failure.to_string().contains("capital_exit_native_quote_stale"));
        assert_eq!(gate.control(), Some(view.clone()));
        assert_eq!(gate.opening_order_ids(), Some(vec![id.to_string()]),
            "exact original opener identity must survive first valuation failure");
        while chrono::Utc::now() <= deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        let (inbox, _sender) = crate::paper_capital_exit::channel(fixture._directory.path().to_owned());
        inbox.service(&mut fixture.session).expect("silent host service must retain deadline cancellation instead of terminating");
        let pending = fixture.session.engine.kernel().cache.borrow().try_order_owned(&id).unwrap();
        assert!(!pending.is_closed(), "silence must not manufacture a native cancellation");
        assert_eq!(pending.filled_qty(), original.filled_qty());
        assert_eq!(fixture.session.engine.kernel().clock.borrow().timestamp_ns(), clock);
        assert_eq!(fixture.session.engine.kernel().cache.borrow().quote(&InstrumentId::from(polymarket::IDS[0])).copied(), Some(original_quote));
        assert_eq!(fixture.session.original_frames, frames);
        assert_eq!(fixture.position(), position);
        assert_eq!(fixture.free(), free);
        let retained: Vec<CapitalExitOwnerEvidenceV1> = std::fs::read_dir(fixture._directory.path()).unwrap()
            .filter_map(|entry|entry.ok()).filter_map(|entry|std::fs::read(entry.path()).ok())
            .filter_map(|bytes|serde_json::from_slice(&bytes).ok()).collect();
        assert!(retained.iter().any(|value|value.command_id==view.command_id && matches!(&value.evidence,
            CapitalExitEvidenceKindV1::NativeProgress { phase: CapitalExitStateV1::CancellingExit, released_cash_amount: None, native_order_refs, reason_codes, .. }
            if native_order_refs.iter().any(|reference|reference.native_client_order_id==id.to_string())
                && reason_codes.iter().any(|reason|reason=="capital_exit_native_cancellation_pending"))),
            "original pending cancellation scope was not durably retained");
        // Resume only genuine fixture source quotes. Their original timestamps
        // let the existing latency/matching queues emit the one native cancel.
        fixture.next_quotes("0.4900", "1000000.000000");
        fixture.next_quotes("0.4900", "1000000.000000");
        let terminal = fixture.session.engine.kernel().cache.borrow().try_order_owned(&id).unwrap();
        assert_eq!(terminal.status(), OrderStatus::Canceled);
        assert_eq!(terminal.filled_qty(), original.filled_qty());
        assert_eq!(terminal.events().iter().filter(|event| matches!(event, OrderEventAny::Canceled(_))).count(),1);
        assert_eq!(fixture.position(), position);
        assert_eq!(fixture.session.session_id(), original_session);
        assert!(gate.issued_order_ids().is_empty(), "deadline must never start a new reduction");
        let stopped = fixture.evidence(NativeRequest::Advance(Box::new(view)));
        assert!(matches!(stopped.evidence, CapitalExitEvidenceKindV1::NativeProgress {
            phase: CapitalExitStateV1::Paused, released_cash_amount: None, ..
        }));
    }).join().unwrap();
}
