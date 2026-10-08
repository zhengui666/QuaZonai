//! Narrow event-thread adapter on an existing official Sandbox LiveNode.
//!
//! It never creates an account/position/PnL engine or connects a venue. The trusted
//! host supplies its original Sandbox configuration, registers the same binding
//! with Store, authenticates transport, and installs the gate in every strategy.
//! LIVE, unsupported fee models, algorithms, emulation, multiple strategies, native
//! recovery and competing protection quantities are explicitly unsupported here.
use crate::capital_exit_gate::{CapitalExitGate, same_control_binding};
use anyhow::{Result, anyhow, ensure};
use chrono::{DateTime, Duration, Utc};
use contracts::{
    DbCounter, DecimalValue, Id, Revision, SchemaV1, account_observation::AccountMoneyV1,
    capital_exit::*, forward::ForwardEnvironmentV1,
};
use nautilus_common::{actor::DataActorNative, enums::Environment};
use nautilus_execution::models::fee::{FeeModel, FeeModelAny, FeeModelHandle, MakerTakerFeeModel};
use nautilus_live::node::{LiveNode, NodeState};
use nautilus_model::{
    data::QuoteTick,
    enums::{AccountType, LiquiditySide, OrderSide, OrderStatus, TimeInForce},
    events::{AccountState, PortfolioSnapshot},
    identifiers::{AccountId, ClientId, ClientOrderId, InstrumentId, StrategyId},
    instruments::{Instrument, InstrumentAny},
    orders::{Order, OrderAny},
    position::Position,
    types::Currency,
};
use nautilus_sandbox::SandboxExecutionClientConfig;
use nautilus_trading::strategy::{Strategy, StrategyNative};
use rust_decimal::Decimal;
use serde_json::{Value, json};
use std::{cell::RefCell, path::Path, str::FromStr};

/// Trusted in-process registration. None of these fields establishes authority
/// when supplied in an HTTP assessment; Store independently binds the owner.
#[derive(Clone, Debug)]
pub struct SandboxCapitalOwnerBinding {
    pub managed_account_key: String,
    pub owner_binding_ref: String,
    pub account_source_id: Option<Id>,
    pub strategy_id: StrategyId,
    pub client_id: ClientId,
    pub instrument_id: InstrumentId,
}

pub struct NativeCapitalExitOwner {
    binding: SandboxCapitalOwnerBinding,
    account_id: AccountId,
    currency: Currency,
    session_id: String,
    trader_id: String,
    gate: CapitalExitGate,
    fee_model: FeeModelHandle,
    fee_shape: FeeShape,
    resolved_source: RefCell<Option<Id>>,
    check_runtime: Box<dyn Fn() -> Result<()>>,
    /// The frozen original quote survives later marks; not the cost basis/PnL.
    reference_quotes: RefCell<Vec<(Id, QuoteTick, Decimal, Decimal)>>,
    /// Backtest clocks follow source time; they cannot prove wall-time freshness
    /// while a live public-data producer is silent.
    streaming_paper: bool,
    source_scoped_paper: bool,
}

#[derive(Clone, Copy)]
enum FeeShape {
    MakerTaker,
    ProbabilityPrice,
    Polymarket,
}

struct NativeReport {
    snapshot: PortfolioSnapshot,
    account: AccountState,
    instrument: InstrumentAny,
    quote: QuoteTick,
    orders: Vec<OrderAny>,
    positions: Vec<Position>,
    raw: Value,
}
/// Cancellation observes only the original owned order identities/events. It
/// deliberately carries no refreshed valuation or withdrawability assertion.
struct NativeCancellationReport {
    orders: Vec<OrderAny>,
    account: AccountState,
    observed_at: DateTime<Utc>,
}
impl NativeCancellationReport {
    fn unresolved(&self) -> bool {
        self.orders.iter().any(|order| !order.is_closed())
    }
    fn report_ref(&self) -> String {
        self.orders
            .iter()
            .max_by_key(|order| order.ts_last())
            .map(|order| order.last_event().clone().into_boxed().id().to_string())
            .unwrap_or_else(|| self.account.event_id.to_string())
    }
}
pub(crate) fn stopping(view: &CapitalExitViewV1) -> bool {
    matches!(
        &view.owner_command.instruction,
        CapitalExitOwnerInstructionV1::Pause {}
            | CapitalExitOwnerInstructionV1::Cancel {}
            | CapitalExitOwnerInstructionV1::ReconcileWithdrawal { .. }
    ) || matches!(&view.policy, CapitalExitPolicyV1::BoundedLimit {deadline,..} if *deadline <= Utc::now())
}
fn exact(value: &DecimalValue) -> Result<Decimal> {
    Decimal::from_str(&value.as_decimal().to_plain_string())
        .map_err(|_| anyhow!("capital_exit_native_decimal_range"))
}
fn decimal(value: Decimal) -> Result<DecimalValue> {
    value
        .normalize()
        .to_string()
        .parse()
        .map_err(anyhow::Error::msg)
}
fn money(value: Decimal, currency: Currency) -> Result<AccountMoneyV1> {
    Ok(AccountMoneyV1 {
        amount: decimal(value)?,
        currency: currency.to_string(),
    })
}
fn event_refs(order: &OrderAny) -> Result<Vec<String>> {
    Ok(order
        .events()
        .iter()
        .map(|event| (*event).clone().into_boxed().id().to_string())
        .collect())
}

fn order_ref(order: &OrderAny) -> Result<CapitalExitNativeOrderRefV1> {
    Ok(CapitalExitNativeOrderRefV1 {
        native_client_order_id: order.client_order_id().to_string(),
        native_strategy_id: order.strategy_id().to_string(),
        native_instrument_id: order.instrument_id().to_string(),
        original_event_refs: event_refs(order)?,
    })
}
impl NativeReport {
    fn balance(&self) -> Result<&nautilus_model::types::AccountBalance> {
        self.snapshot
            .balances
            .iter()
            .find(|v| Some(v.currency) == self.snapshot.base_currency)
            .ok_or_else(|| anyhow!("capital_exit_native_cash_unavailable"))
    }
    fn openers(&self) -> impl Iterator<Item = &OrderAny> {
        self.orders
            .iter()
            .filter(|o| !o.is_closed() && o.order_side() == OrderSide::Buy)
    }
    fn protection(&self) -> impl Iterator<Item = &OrderAny> {
        self.orders
            .iter()
            .filter(|o| !o.is_closed() && o.order_side() == OrderSide::Sell)
    }
    fn report_ref(&self) -> String {
        self.snapshot.event_id.to_string()
    }
}

impl NativeCapitalExitOwner {
    /// Attach only on the owning native thread before node start. `sandbox` must
    /// be the exact trusted config used by add_simulated_exec_client. No execution
    /// controls are exposed by the existing Paper hosts until this adapter is
    /// wired and accepted there; an assessment alone does not install a gate.
    pub fn attach_sandbox(
        node: &LiveNode,
        binding: SandboxCapitalOwnerBinding,
        sandbox: &SandboxExecutionClientConfig,
        stable_volume: &Path,
    ) -> Result<Self> {
        ensure!(
            node.environment() == Environment::Sandbox && node.state() == NodeState::Idle,
            "capital_exit_supported_owner_unavailable"
        );
        ensure!(
            sandbox.account_type == AccountType::Cash
                && sandbox.base_currency.is_some()
                && sandbox.starting_balances.len() == 1
                && !sandbox.frozen_account
                && sandbox.use_reduce_only
                && sandbox.fee_model.as_ref().is_none_or(|m| matches!(
                    m,
                    FeeModelAny::MakerTaker(_) | FeeModelAny::ProbabilityPrice(_)
                )),
            "capital_exit_sandbox_account_or_fee_model_unsupported"
        );
        let currency = sandbox
            .base_currency
            .ok_or_else(|| anyhow!("capital_exit_collateral_unavailable"))?;
        ensure!(
            sandbox.starting_balances[0].currency == currency
                && binding.instrument_id.venue == sandbox.venue,
            "capital_exit_owner_scope_mismatch"
        );
        let engine = node.kernel().exec_engine().clone();
        {
            let engine = engine.borrow();
            let client = engine
                .get_client(&binding.client_id)
                .ok_or_else(|| anyhow!("capital_exit_owner_client_unavailable"))?;
            ensure!(
                client.account_id() == sandbox.account_id && client.venue() == sandbox.venue,
                "capital_exit_owner_binding_mismatch"
            );
        }
        let account_id = sandbox.account_id;
        let trader = node.kernel().trader().clone();
        let handle = node.handle();
        let strategy_id = binding.strategy_id;
        let client_id = binding.client_id;
        let gate = CapitalExitGate::open_fresh_sandbox(
            stable_volume,
            &binding.managed_account_key,
            &binding.owner_binding_ref,
            &account_id.to_string(),
        )?;
        let check_runtime = Box::new(move || {
            let trader = trader
                .try_borrow()
                .map_err(|_| anyhow!("capital_exit_native_owner_busy"))?;
            ensure!(
                trader.strategy_ids() == vec![strategy_id]
                    && trader.exec_algorithm_ids().is_empty()
                    && trader.actor_ids().is_empty(),
                "capital_exit_uncovered_strategy_actor_or_algorithm"
            );
            let engine = engine
                .try_borrow()
                .map_err(|_| anyhow!("capital_exit_native_owner_busy"))?;
            let client = engine
                .get_client(&client_id)
                .ok_or_else(|| anyhow!("capital_exit_owner_client_unavailable"))?;
            ensure!(
                handle.state() == NodeState::Running
                    && client.is_connected()
                    && client.account_id() == account_id,
                "capital_exit_native_client_disconnected"
            );
            Ok(())
        });
        let resolved_source = RefCell::new(binding.account_source_id);
        Ok(Self {
            binding,
            resolved_source,
            account_id,
            currency,
            session_id: node.instance_id().to_string(),
            trader_id: node.trader_id().to_string(),
            gate,
            fee_model: FeeModelHandle::new(
                sandbox
                    .fee_model
                    .clone()
                    .unwrap_or(FeeModelAny::MakerTaker(MakerTakerFeeModel)),
            ),
            fee_shape: if matches!(sandbox.fee_model, Some(FeeModelAny::ProbabilityPrice(_))) {
                FeeShape::ProbabilityPrice
            } else {
                FeeShape::MakerTaker
            },
            check_runtime,
            reference_quotes: RefCell::new(vec![]),
            streaming_paper: false,
            source_scoped_paper: false,
        })
    }

    /// Controlled Sandbox bridge using the same source-scoped Paper registration
    /// as the production adapter. The source is still bound only from intake.
    pub fn attach_source_bound_sandbox(
        node: &LiveNode,
        binding: SandboxCapitalOwnerBinding,
        sandbox: &SandboxExecutionClientConfig,
        stable_volume: &Path,
    ) -> Result<Self> {
        let mut owner = Self::attach_sandbox(node, binding, sandbox, stable_volume)?;
        owner.source_scoped_paper = true;
        Ok(owner)
    }

    /// Adapter over the already-created official Paper engine. This does not
    /// initialize an account, start a client, add a strategy, or advance time.
    #[cfg(feature = "native-paper")]
    pub(crate) fn attach_streaming_paper(
        engine: &nautilus_backtest::engine::BacktestEngine,
        instrument_id: InstrumentId,
        settings: &contracts::science::NativeSimulationSettingsV1,
        fee_model: FeeModelHandle,
        claim: &contracts::strategy_portfolio::HandoffClaimViewV2,
        stable_volume: &Path,
    ) -> Result<Self> {
        ensure!(
            engine.iteration() == 0 && engine.run_id().is_none(),
            "capital_exit_paper_owner_already_started"
        );
        ensure!(
            settings.account_kind == contracts::science::NativeAccountKind::Cash
                && instrument_id.venue == nautilus_model::identifiers::Venue::from("POLYMARKET")
                && domain::prediction::uses_native_fee(&settings.fee_model),
            "capital_exit_paper_owner_scope_unsupported"
        );
        let client_id = ClientId::from("POLYMARKET");
        let execution = engine.kernel().exec_engine.clone();
        let account_id = {
            let execution = execution.borrow();
            let client = execution
                .get_client(&client_id)
                .ok_or_else(|| anyhow!("capital_exit_owner_client_unavailable"))?;
            ensure!(
                client.client_id() == client_id && client.venue() == instrument_id.venue,
                "capital_exit_owner_binding_mismatch"
            );
            client.account_id()
        };
        let session_id = engine.instance_id().to_string();
        let binding = SandboxCapitalOwnerBinding {
            managed_account_key: format!("paper-native:{session_id}"),
            owner_binding_ref: format!("nautilus-paper:{session_id}"),
            account_source_id: None,
            strategy_id: StrategyId::from("QZ-PAPER-001"),
            client_id,
            instrument_id,
        };
        let gate = CapitalExitGate::open_fresh_paper(
            stable_volume,
            &binding.managed_account_key,
            &binding.owner_binding_ref,
            &account_id.to_string(),
            claim,
        )?;
        ensure!(
            !gate.recovery_required(),
            "capital_exit_owner_recovery_required"
        );
        let trader = engine.kernel().trader.clone();
        let strategy_id = binding.strategy_id;
        let check_runtime = Box::new(move || {
            let trader = trader
                .try_borrow()
                .map_err(|_| anyhow!("capital_exit_native_owner_busy"))?;
            ensure!(
                trader.strategy_ids() == vec![strategy_id]
                    && trader.actor_ids().is_empty()
                    && trader.exec_algorithm_ids().is_empty(),
                "capital_exit_uncovered_strategy_actor_or_algorithm"
            );
            let execution = execution
                .try_borrow()
                .map_err(|_| anyhow!("capital_exit_native_owner_busy"))?;
            let client = execution
                .get_client(&client_id)
                .ok_or_else(|| anyhow!("capital_exit_owner_client_unavailable"))?;
            ensure!(
                client.is_connected() && client.account_id() == account_id,
                "capital_exit_native_client_disconnected"
            );
            Ok(())
        });
        Ok(Self {
            binding,
            account_id,
            currency: Currency::from_str(&settings.base_currency)?,
            session_id,
            trader_id: engine.trader_id().to_string(),
            gate,
            fee_model,
            fee_shape: FeeShape::Polymarket,
            resolved_source: RefCell::new(None),
            check_runtime,
            reference_quotes: RefCell::new(vec![]),
            streaming_paper: true,
            source_scoped_paper: true,
        })
    }
    pub fn gate(&self) -> CapitalExitGate {
        self.gate.clone()
    }

    /// Bind only the successful response to the existing authenticated original
    /// client-observation intake. Configuration and stable account registration
    /// remain separate; a source receipt does not create an owner registration.
    pub fn bind_authenticated_source(
        &self,
        receipt: &contracts::account_observation::AccountObservationReceiptV2,
    ) -> Result<Id> {
        let binding = &receipt.resource.observation.binding;
        ensure!(
            receipt.native_client_id == self.binding.client_id.to_string()
                && binding.native_session_id == self.session_id
                && binding.native_trader_id == self.trader_id
                && binding.native_account_id == self.account_id.to_string()
                && binding.environment == ForwardEnvironmentV1::Paper,
            "capital_exit_source_binding_mismatch"
        );
        let mut source = self.resolved_source.borrow_mut();
        ensure!(
            source.is_none_or(|id| id == receipt.resource.source_id),
            "capital_exit_source_binding_conflict"
        );
        if self.source_scoped_paper {
            self.gate.bind_paper_source(receipt.resource.source_id)?;
        }
        *source = Some(receipt.resource.source_id);
        Ok(receipt.resource.id)
    }
    fn source_id(&self) -> Result<Id> {
        self.resolved_source
            .borrow()
            .ok_or_else(|| anyhow!("capital_exit_authenticated_source_unavailable"))
    }

    fn owner_binding_ref(&self) -> Result<String> {
        Ok(if self.source_scoped_paper {
            format!("nautilus-paper:{}", self.source_id()?)
        } else {
            self.binding.owner_binding_ref.clone()
        })
    }

    fn managed_account_key(&self) -> Result<String> {
        Ok(if self.source_scoped_paper {
            format!("paper-native:{}", self.source_id()?)
        } else {
            self.binding.managed_account_key.clone()
        })
    }

    fn inspect<S: Strategy + StrategyNative + DataActorNative>(
        &self,
        strategy: &S,
    ) -> Result<NativeReport> {
        (self.check_runtime)()?;
        ensure!(
            !self.gate.recovery_required(),
            "capital_exit_owner_recovery_required"
        );
        ensure!(
            strategy.strategy_id() == Some(self.binding.strategy_id),
            "capital_exit_strategy_binding_mismatch"
        );
        let snapshot = strategy
            .portfolio_rc()
            .try_borrow_mut()
            .map_err(|_| anyhow!("capital_exit_native_portfolio_busy"))?
            .build_snapshot(&self.account_id)
            .ok_or_else(|| anyhow!("capital_exit_native_portfolio_unavailable"))?;
        ensure!(
            snapshot.account_type == AccountType::Cash
                && snapshot.base_currency == Some(self.currency)
                && snapshot.balances.len() == 1
                && !snapshot.is_stale
                && snapshot.unpriced_instruments.is_empty()
                && snapshot.margins.is_empty(),
            "capital_exit_native_cash_scope_unsupported"
        );
        let cache = strategy.cache();
        let account = cache
            .account(&self.account_id)
            .ok_or_else(|| anyhow!("capital_exit_native_account_unavailable"))?
            .last_event()
            .ok_or_else(|| anyhow!("capital_exit_native_account_event_unavailable"))?;
        let instrument = cache
            .instrument(&self.binding.instrument_id)
            .ok_or_else(|| anyhow!("capital_exit_native_instrument_unavailable"))?;
        ensure!(
            instrument.maker_fee() >= Decimal::ZERO
                && instrument.taker_fee() >= Decimal::ZERO
                && instrument.quote_currency() == self.currency
                && instrument.multiplier().as_decimal() == Decimal::ONE
                && !instrument.is_inverse(),
            "capital_exit_fee_or_instrument_model_unsupported"
        );
        if instrument.maker_fee() != Decimal::ZERO || instrument.taker_fee() != Decimal::ZERO {
            ensure!(
                matches!(&instrument, InstrumentAny::BinaryOption(_)),
                "capital_exit_fee_price_bound_unavailable"
            );
        }
        let quote = cache
            .quote(&self.binding.instrument_id)
            .ok_or_else(|| anyhow!("capital_exit_native_quote_unavailable"))?;
        ensure!(
            quote.bid_price.as_decimal() > Decimal::ZERO
                && quote.bid_price <= quote.ask_price
                && (!matches!(instrument, InstrumentAny::BinaryOption(_))
                    || quote.ask_price.as_decimal() <= Decimal::ONE),
            "capital_exit_native_quote_invalid"
        );
        let now_ns = strategy.clock().timestamp_ns().as_u64();
        if self.streaming_paper {
            let wall = Utc::now()
                .timestamp_nanos_opt()
                .and_then(|value| u64::try_from(value).ok())
                .ok_or_else(|| anyhow!("capital_exit_wall_clock_unavailable"))?;
            ensure!(
                quote.ts_init.as_u64() <= wall && wall - quote.ts_init.as_u64() <= 5_000_000_000,
                "capital_exit_native_quote_stale"
            );
        }
        ensure!(
            quote.ts_event <= quote.ts_init
                && quote.ts_init.as_u64() <= now_ns
                && now_ns - quote.ts_init.as_u64() <= 5_000_000_000
                && instrument.ts_init().as_u64() <= quote.ts_init.as_u64(),
            "capital_exit_native_quote_stale"
        );
        // orders(), unlike only open/inflight indexes, includes initialized
        // commands before an account ID has been assigned. Filter by exact owner
        // and include any foreign active order at this venue as a blocker.
        let orders = cache.orders(
            Some(&self.binding.instrument_id.venue),
            None,
            None,
            None,
            None,
        );
        ensure!(
            orders.len() <= 4096,
            "capital_exit_native_order_scope_limit"
        );
        for order in orders.iter().filter(|o| !o.is_closed()) {
            ensure!(
                order.strategy_id() == self.binding.strategy_id
                    && order.instrument_id() == self.binding.instrument_id
                    && order.account_id().is_none_or(|a| a == self.account_id),
                "capital_exit_competing_owner_or_instrument"
            );
            ensure!(
                order.exec_algorithm_id().is_none()
                    && order.exec_spawn_id().is_none()
                    && order.emulation_trigger().is_none()
                    && order.parent_order_id().is_none()
                    && (!order.is_active_local() || order.status() == OrderStatus::Initialized)
                    && !order.is_emulated(),
                "capital_exit_child_path_unsupported"
            );
        }
        let orders: Vec<_> = orders
            .into_iter()
            .filter(|o| {
                o.strategy_id() == self.binding.strategy_id
                    && o.instrument_id() == self.binding.instrument_id
                    && o.account_id().is_none_or(|a| a == self.account_id)
            })
            .collect();
        let positions = cache.positions(None, None, None, Some(&self.account_id), None);
        ensure!(
            positions
                .iter()
                .all(|p| p.instrument_id == self.binding.instrument_id
                    && p.strategy_id == self.binding.strategy_id
                    && !p.is_short()),
            "capital_exit_pooled_position_scope_unsupported"
        );
        ensure!(
            positions.iter().filter(|p| p.is_open()).count() <= 1,
            "capital_exit_netting_scope_unsupported"
        );
        let original_orders = orders
            .iter()
            .map(|o| serde_json::to_value(o.events()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let raw = json!({"native_version":nautilus_core::consts::NAUTILUS_VERSION_CORE,"native_fee_model": match self.fee_shape { FeeShape::MakerTaker => "nautilus_execution::models::fee::MakerTakerFeeModel", FeeShape::ProbabilityPrice => "nautilus_execution::models::fee::ProbabilityPriceFeeModel", FeeShape::Polymarket => "nautilus_polymarket::models::PolymarketFeeModel" },"environment":if self.streaming_paper {"PAPER_BACKTEST_ENGINE"} else {"SANDBOX"},"native_session_id":self.session_id,"native_trader_id":self.trader_id,"native_client_id":self.binding.client_id.to_string(),"snapshot":snapshot,"account_event":account,"instrument":instrument,"quote":quote,"order_events":original_orders,"positions":positions});
        Ok(NativeReport {
            snapshot,
            account,
            instrument,
            quote,
            orders,
            positions,
            raw,
        })
    }

    /// Official fee evaluation, with a conservative allowance for rounding once
    /// per possible minimum-lot fill. This plans a bound; it does not book fees.
    /// Nonzero fees need a bounded binary payout price. Other models are blocked.
    fn fee_bound(
        &self,
        report: &NativeReport,
        quantity: Decimal,
        minimum_price: Decimal,
    ) -> Result<Decimal> {
        if quantity == Decimal::ZERO {
            return Ok(Decimal::ZERO);
        }
        let zero_rates = if matches!(self.fee_shape, FeeShape::Polymarket) {
            domain::prediction::planning_fee(&crate::prediction::native_payload(
                &report.instrument,
            )?)?
            .as_decimal()
                == &bigdecimal::BigDecimal::from(0)
        } else {
            report.instrument.maker_fee() == Decimal::ZERO
                && report.instrument.taker_fee() == Decimal::ZERO
        };
        if !zero_rates {
            ensure!(
                matches!(report.instrument, InstrumentAny::BinaryOption(_))
                    && minimum_price <= Decimal::ONE,
                "capital_exit_fee_price_bound_unavailable"
            );
        }
        let price = match self.fee_shape {
            FeeShape::MakerTaker => {
                if zero_rates {
                    minimum_price
                } else {
                    Decimal::ONE
                }
            }
            FeeShape::ProbabilityPrice | FeeShape::Polymarket => {
                minimum_price.max(Decimal::new(5, 1))
            }
        };
        let quantity_native = report
            .instrument
            .try_make_qty_from_decimal(quantity, Some(true))?;
        ensure!(
            quantity_native.as_decimal() == quantity,
            "capital_exit_native_precision_mismatch"
        );
        let price_native = report.instrument.try_make_price_from_decimal(price)?;
        let Some(original) = report
            .orders
            .iter()
            .find(|o| o.instrument_id() == self.binding.instrument_id)
        else {
            // With sufficient idle cash no hypothetical reduction or fee is needed.
            return if zero_rates {
                Ok(Decimal::ZERO)
            } else {
                Err(anyhow!("capital_exit_native_fee_basis_unavailable"))
            };
        };
        let mut planning_copy = original.clone();
        let mut bound = Decimal::ZERO;
        for liquidity in [LiquiditySide::Maker, LiquiditySide::Taker] {
            planning_copy.set_liquidity_side(liquidity);
            let native = self.fee_model.get_commission(
                &planning_copy,
                quantity_native,
                price_native,
                &report.instrument,
            )?;
            ensure!(
                native.currency == self.currency
                    && (native.as_decimal() >= Decimal::ZERO
                        || matches!(self.fee_shape, FeeShape::Polymarket)),
                "capital_exit_native_fee_currency_unsupported"
            );
            bound = bound.max(native.as_decimal());
        }
        if zero_rates {
            return Ok(bound);
        }
        let monetary_quantum = Decimal::new(1, u32::from(self.currency.precision));
        let model_quantum = if matches!(
            self.fee_shape,
            FeeShape::ProbabilityPrice | FeeShape::Polymarket
        ) {
            Decimal::new(1, 5)
        } else {
            Decimal::ZERO
        };
        // The existing Polymarket simulation wrapper rejects each fill below
        // one collateral unit and price is at most one. That existing minimum,
        // not an invented lot or a new fee engine, bounds the number of fills.
        let maximum_fills = if matches!(self.fee_shape, FeeShape::Polymarket) {
            quantity.ceil()
        } else {
            quantity
                .checked_div(Decimal::new(
                    1,
                    u32::from(report.instrument.size_precision()),
                ))
                .ok_or_else(|| anyhow!("capital_exit_native_decimal_range"))?
                .ceil()
        };
        bound
            .checked_add(
                monetary_quantum
                    .checked_add(model_quantum)
                    .and_then(|q| q.checked_mul(maximum_fills))
                    .ok_or_else(|| anyhow!("capital_exit_native_decimal_range"))?,
            )
            .ok_or_else(|| anyhow!("capital_exit_native_decimal_range"))
    }

    /// Read-only assessment. It cannot reserve capital, cancel or submit orders.
    pub fn assess<S: Strategy + StrategyNative + DataActorNative>(
        &self,
        strategy: &S,
        request: CapitalExitPreviewRequestV1,
        control_revision: Revision,
        sequence: DbCounter,
    ) -> Result<CapitalExitOwnerAssessmentV1> {
        ensure!(
            request.account_source_id == self.source_id()?,
            "capital_exit_source_binding_mismatch"
        );
        let report = self.inspect(strategy)?;
        let requested = request.scope.money();
        ensure!(
            requested.currency == self.currency.to_string() && requested.amount.is_positive(),
            "capital_exit_requested_amount"
        );
        let amount = exact(&requested.amount)?;
        let balance = report.balance()?;
        let equity = report
            .snapshot
            .total_equity
            .iter()
            .find(|v| v.currency == self.currency)
            .ok_or_else(|| anyhow!("capital_exit_native_equity_unavailable"))?
            .as_decimal();
        let mut reasons = vec![];
        if matches!(request.scope, CapitalExitScopeV1::PortfolioScope { .. }) {
            reasons.push("capital_exit_portfolio_scope_unavailable".into());
        }
        if let Some(control) = self.gate.control() {
            if control.funds.reserved_amount != requested
                || control.account_control_revision != control_revision
            {
                reasons.push("capital_exit_account_reservation_scope_mismatch".into());
            }
        }
        if equity < amount {
            reasons.push("capital_exit_amount_exceeds_managed_capital".into());
        }
        let idle = balance.free.as_decimal();
        let mut legs = vec![];
        let mut cost = None;
        let mut release = idle.min(amount);
        match &request.policy {
            CapitalExitPolicyV1::CashOnly {} => {
                if idle < amount {
                    reasons.push("capital_exit_cash_only_insufficient".into());
                }
            }
            CapitalExitPolicyV1::BoundedLimit {
                deadline,
                legs: approved,
                max_execution_cost,
            } => {
                ensure!(
                    *deadline > Utc::now()
                        && approved.len() == 1
                        && approved[0].instrument_id == self.binding.instrument_id.to_string()
                        && max_execution_cost.currency == requested.currency,
                    "capital_exit_execution_bounds"
                );
                let leg = &approved[0];
                let floor = exact(&leg.minimum_sell_price)?;
                let maximum = exact(&leg.maximum_reduction_quantity)?;
                ensure!(
                    floor > Decimal::ZERO && maximum > Decimal::ZERO,
                    "capital_exit_execution_bounds"
                );
                ensure!(
                    report
                        .instrument
                        .try_make_price_from_decimal(floor)?
                        .as_decimal()
                        == floor
                        && report
                            .instrument
                            .try_make_qty_from_decimal(maximum, Some(true))?
                            .as_decimal()
                            == maximum,
                    "capital_exit_native_precision_mismatch"
                );
                let fee_bound = self.fee_bound(&report, maximum, floor)?;
                let worst_cost = (report.quote.bid_price.as_decimal() - floor)
                    .max(Decimal::ZERO)
                    .checked_mul(maximum)
                    .and_then(|v| v.checked_add(fee_bound))
                    .ok_or_else(|| anyhow!("capital_exit_native_decimal_range"))?;
                cost = Some(money(worst_cost, self.currency)?);
                if worst_cost > exact(&max_execution_cost.amount)? {
                    reasons.push("capital_exit_cost_bound_exceeded".into());
                }
                if max_execution_cost.reference_evidence_id
                    != request.expected_source_observation_id
                {
                    reasons.push("capital_exit_quote_reference_mismatch".into());
                }
                // Freeze the actual original quote keyed to the retained assessment
                // source, without substituting a future quote on start/resume.
                let mut quotes = self.reference_quotes.borrow_mut();
                if let Some((_, previous, maker, taker)) = quotes
                    .iter()
                    .find(|(id, ..)| *id == max_execution_cost.reference_evidence_id)
                {
                    if previous != &report.quote
                        || *maker != report.instrument.maker_fee()
                        || *taker != report.instrument.taker_fee()
                    {
                        reasons.push("capital_exit_reference_evidence_conflict".into());
                    }
                } else {
                    ensure!(quotes.len() < 1024, "capital_exit_reference_limit");
                    quotes.push((
                        max_execution_cost.reference_evidence_id,
                        report.quote,
                        report.instrument.maker_fee(),
                        report.instrument.taker_fee(),
                    ));
                }
                let holdings = report
                    .positions
                    .iter()
                    .filter(|p| p.is_open())
                    .map(|p| p.quantity.as_decimal())
                    .try_fold(Decimal::ZERO, |a, b| a.checked_add(b))
                    .ok_or_else(|| anyhow!("capital_exit_native_decimal_range"))?;
                if report.protection().next().is_some() && idle < amount {
                    reasons.push("capital_exit_protective_quantity_overlap".into());
                }
                release = idle
                    .checked_add(
                        maximum
                            .min(holdings)
                            .checked_mul(floor)
                            .ok_or_else(|| anyhow!("capital_exit_native_decimal_range"))?,
                    )
                    .and_then(|v| v.checked_sub(fee_bound))
                    .ok_or_else(|| anyhow!("capital_exit_native_decimal_range"))?
                    .max(Decimal::ZERO)
                    .min(amount);
                legs = approved.clone();
            }
        }
        let asof = DateTime::from_timestamp_nanos(report.snapshot.ts_init.as_u64() as i64);
        let valid_until = (asof + Duration::seconds(5)).min(
            DateTime::from_timestamp_nanos(report.quote.ts_init.as_u64() as i64)
                + Duration::seconds(5),
        );
        let capability = if reasons.is_empty() {
            CapitalExitCapabilityV1::Supported
        } else {
            CapitalExitCapabilityV1::Blocked
        };
        let assessment = CapitalExitOwnerAssessmentV1 {
            schema_version: SchemaV1,
            account_source_id: self.source_id()?,
            owner_binding_ref: self.owner_binding_ref()?,
            expected_account_control_revision: control_revision,
            external_message_id: format!("assessment:{}:{}", report.report_ref(), sequence.get()),
            sequence,
            request: request.clone(),
            asof,
            valid_until,
            funds: CapitalExitPreviewFundsV1 {
                requested,
                native_total_cash: Some(money(balance.total.as_decimal(), self.currency)?),
                native_free_cash: Some(money(idle, self.currency)?),
                native_locked_cash: Some(money(balance.locked.as_decimal(), self.currency)?),
                verified_idle_cash: Some(money(idle, self.currency)?),
                estimated_release: Some(money(release, self.currency)?),
                native_equity: Some(money(equity, self.currency)?),
                managed_capital_before: Some(money(equity, self.currency)?),
                remaining_managed_capital: if equity >= amount {
                    Some(money(equity - amount, self.currency)?)
                } else {
                    None
                },
                estimated_execution_cost: cost,
                existing_unrealized_pnl: report
                    .snapshot
                    .unrealized_pnls
                    .iter()
                    .find(|v| v.currency == self.currency)
                    .map(|v| money(v.as_decimal(), self.currency))
                    .transpose()?,
            },
            proposed_cancellations: report.openers().map(order_ref).collect::<Result<_>>()?,
            retained_protective_orders: report
                .protection()
                .map(order_ref)
                .collect::<Result<_>>()?,
            reduction_legs: legs,
            evidence_refs: vec![request.expected_source_observation_id],
            remaining_risk_evidence_id: None,
            native_report_ref: report.report_ref(),
            native_evidence: report.raw,
            capability,
            reason_codes: reasons,
        };
        self.gate
            .retain_original_message("assessment", sequence, &assessment)?;
        Ok(assessment)
    }

    fn bound_control(&self, view: &CapitalExitViewV1) -> Result<()> {
        ensure!(
            view.owner_command.command_id == view.command_id
                && view.owner_command.account_control_epoch == view.account_control_epoch,
            "capital_exit_owner_command_mismatch"
        );
        ensure!(
            view.environment == ForwardEnvironmentV1::Paper
                && view.account_source_id == self.source_id()?
                && view.owner_binding_ref == self.owner_binding_ref()?
                && view.managed_account_key == self.managed_account_key()?
                && view.funds.reserved_amount.currency == self.currency.to_string(),
            "capital_exit_claim_binding_mismatch"
        );
        Ok(())
    }

    fn admitted_control(&self, view: &CapitalExitViewV1) -> Result<()> {
        self.bound_control(view)?;
        let current = self
            .gate
            .control()
            .ok_or_else(|| anyhow!("capital_exit_fence_required"))?;
        ensure!(
            same_control_binding(&current, view),
            "capital_exit_control_payload_mismatch"
        );
        Ok(())
    }

    fn capture_openers<S: Strategy + StrategyNative + DataActorNative>(
        &self,
        strategy: &S,
    ) -> Result<()> {
        if self.gate.opening_order_ids().is_some() {
            return Ok(());
        }
        (self.check_runtime)()?;
        ensure!(
            strategy.strategy_id() == Some(self.binding.strategy_id),
            "capital_exit_strategy_binding_mismatch"
        );
        let cache = strategy.cache();
        let orders = cache.orders(
            Some(&self.binding.instrument_id.venue),
            Some(&self.binding.instrument_id),
            Some(&self.binding.strategy_id),
            None,
            Some(OrderSide::Buy),
        );
        let mut ids = Vec::new();
        for order in orders.into_iter().filter(|order| !order.is_closed()) {
            self.cancellation_identity(strategy, &order)?;
            ids.push(order.client_order_id().to_string());
        }
        self.gate.retain_original_openers(ids)
    }

    fn cancellation_identity<S: Strategy + StrategyNative + DataActorNative>(
        &self,
        strategy: &S,
        order: &OrderAny,
    ) -> Result<()> {
        ensure!(
            order.strategy_id() == self.binding.strategy_id
                && order.instrument_id() == self.binding.instrument_id
                && order.account_id().is_none_or(|id| id == self.account_id)
                && (order.account_id() == Some(self.account_id)
                    || strategy.cache().client_id(&order.client_order_id())
                        == Some(self.binding.client_id)),
            "capital_exit_original_order_binding_mismatch"
        );
        Ok(())
    }

    fn cancellation_report<S: Strategy + StrategyNative + DataActorNative>(
        &self,
        strategy: &S,
    ) -> Result<NativeCancellationReport> {
        (self.check_runtime)()?;
        ensure!(
            !self.gate.recovery_required(),
            "capital_exit_owner_recovery_required"
        );
        ensure!(
            strategy.strategy_id() == Some(self.binding.strategy_id),
            "capital_exit_strategy_binding_mismatch"
        );
        let mut ids = self
            .gate
            .opening_order_ids()
            .ok_or_else(|| anyhow!("capital_exit_original_opening_scope_unavailable"))?;
        ids.extend(self.gate.issued_order_ids());
        ids.sort();
        ids.dedup();
        let cache = strategy.cache();
        let mut orders = Vec::new();
        for id in ids {
            let order = cache
                .order(&ClientOrderId::new_checked(&id)?)
                .ok_or_else(|| anyhow!("capital_exit_original_order_reconciliation_required"))?;
            self.cancellation_identity(strategy, &order)?;
            orders.push(order);
        }
        let account = cache
            .account(&self.account_id)
            .and_then(|a| a.last_event())
            .ok_or_else(|| anyhow!("capital_exit_native_account_event_unavailable"))?;
        Ok(NativeCancellationReport {
            orders,
            account,
            observed_at: DateTime::from_timestamp_nanos(
                strategy.clock().timestamp_ns().as_u64() as i64
            ),
        })
    }

    fn cancellation_evidence(
        &self,
        view: &CapitalExitViewV1,
        source_observation_id: Id,
        sequence: DbCounter,
        report: &NativeCancellationReport,
        evidence: CapitalExitEvidenceKindV1,
    ) -> Result<CapitalExitOwnerEvidenceV1> {
        let reference = report.report_ref();
        let native_orders = report
            .orders
            .iter()
            .map(|order| serde_json::to_value(order.events()))
            .collect::<std::result::Result<Vec<_>, _>>()?;
        let mut original_event_refs = vec![reference.clone(), report.account.event_id.to_string()];
        original_event_refs.sort();
        original_event_refs.dedup();
        let message = CapitalExitOwnerEvidenceV1 {
            schema_version: SchemaV1,
            intent_id: view.id,
            command_id: view.command_id,
            account_source_id: self.source_id()?,
            owner_binding_ref: self.owner_binding_ref()?,
            native_session_id: self.session_id.clone(),
            native_account_id: self.account_id.to_string(),
            account_control_epoch: view.account_control_epoch,
            external_claim_id: view
                .external_claim_id
                .clone()
                .ok_or_else(|| anyhow!("capital_exit_claim_required"))?,
            external_message_id: format!("{}:{}:{}", view.command_id, reference, sequence.get()),
            sequence,
            source_observation_id,
            asof: report.observed_at,
            valid_until: report.observed_at + Duration::seconds(5),
            original_event_refs,
            native_evidence: json!({"native_order_events":native_orders,"native_account_event":report.account,
                "owner_control_report":self.gate.retained_control_report()?,"observation":"ORDER_CANCELLATION_SCOPE_ONLY"}),
            evidence,
        };
        self.gate
            .retain_original_message("evidence", sequence, &message)?;
        Ok(message)
    }

    fn stop_native_orders<S: Strategy + StrategyNative + DataActorNative>(
        &self,
        strategy: &mut S,
        view: &CapitalExitViewV1,
        source_observation_id: Id,
        sequence: DbCounter,
    ) -> Result<CapitalExitOwnerEvidenceV1> {
        let before = self.cancellation_report(strategy)?;
        for order in before.orders.iter().filter(|order| !order.is_closed()) {
            strategy.cancel_order(order.client_order_id(), Some(self.binding.client_id), None)?;
        }
        // Recompute after the native call. Accepted/PendingCancel/partial fills
        // remain unresolved; a fill is observed once by the original native cache.
        let current = self.cancellation_report(strategy)?;
        let mut reasons = Vec::new();
        if current.unresolved() {
            reasons.push("capital_exit_native_cancellation_pending".into());
        }
        if matches!(&view.policy,CapitalExitPolicyV1::BoundedLimit {deadline,..} if *deadline <= Utc::now())
        {
            reasons.push("capital_exit_deadline_paused".into());
        }
        let phase = if matches!(
            &view.owner_command.instruction,
            CapitalExitOwnerInstructionV1::ReconcileWithdrawal { .. }
        ) {
            reasons.push("capital_exit_native_cash_movement_unavailable".into());
            CapitalExitStateV1::Blocked
        } else if current.unresolved() {
            CapitalExitStateV1::CancellingExit
        } else if matches!(
            &view.owner_command.instruction,
            CapitalExitOwnerInstructionV1::Cancel {}
        ) {
            CapitalExitStateV1::CancelledReserved
        } else {
            CapitalExitStateV1::Paused
        };
        self.cancellation_evidence(
            view,
            source_observation_id,
            sequence,
            &current,
            CapitalExitEvidenceKindV1::NativeProgress {
                phase,
                released_cash_amount: None,
                native_order_refs: current
                    .orders
                    .iter()
                    .map(order_ref)
                    .collect::<Result<_>>()?,
                native_position_refs: vec![],
                native_report_ref: current.report_ref(),
                reason_codes: reasons,
            },
        )
    }

    /// Persist the obsolete-authority fence before examining/cancelling native
    /// orders. Unsupported child coverage returns BLOCKED and leaves the fence.
    pub fn fence<S: Strategy + StrategyNative + DataActorNative>(
        &self,
        strategy: &S,
        view: &CapitalExitViewV1,
        source_observation_id: Id,
        sequence: DbCounter,
    ) -> Result<CapitalExitOwnerEvidenceV1> {
        self.bound_control(view)?;
        self.gate.apply_control(view)?;
        // Original cancellation identities do not depend on a fresh valuation.
        // Retain them even if this first fence later waits for a current quote.
        self.capture_openers(strategy)?;
        if stopping(view) {
            let report = self.cancellation_report(strategy)?;
            return self.cancellation_evidence(
                view,
                source_observation_id,
                sequence,
                &report,
                CapitalExitEvidenceKindV1::FenceApplied {
                    native_gate_report_ref: format!(
                        "{}:{}",
                        view.command_id,
                        view.account_control_epoch.get()
                    ),
                    controlled_strategy_ids: vec![self.binding.strategy_id.to_string()],
                    invalidated_target_claim_refs: self.gate.original_target_claim_refs(),
                    invalidated_child_timer_refs: vec![],
                    resolved_inflight_report_ref: report.report_ref(),
                    remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
                    remaining_target_authority_ref: None,
                },
            );
        }
        let report = self.inspect(strategy)?;
        if let CapitalExitPolicyV1::BoundedLimit {
            max_execution_cost, ..
        } = &view.policy
        {
            let references = self.reference_quotes.borrow();
            let (_, quote, maker, taker) = references
                .iter()
                .find(|(id, ..)| *id == max_execution_cost.reference_evidence_id)
                .ok_or_else(|| anyhow!("capital_exit_original_cost_reference_unavailable"))?;
            // A pause/cancel must still be able to remove old native work when
            // market facts changed; only a new execution command re-admits price.
            if matches!(
                &view.owner_command.instruction,
                CapitalExitOwnerInstructionV1::Start {} | CapitalExitOwnerInstructionV1::Resume {}
            ) {
                ensure!(
                    quote.bid_price == report.quote.bid_price
                        && quote.ask_price == report.quote.ask_price
                        && quote.bid_size == report.quote.bid_size
                        && quote.ask_size == report.quote.ask_size
                        && *maker == report.instrument.maker_fee()
                        && *taker == report.instrument.taker_fee(),
                    "capital_exit_preview_quote_or_fee_changed"
                );
            }
        }
        ensure!(
            !report.orders.iter().any(|o| !o.is_closed()
                && matches!(
                    o.status(),
                    OrderStatus::Initialized | OrderStatus::Submitted | OrderStatus::PendingUpdate
                )),
            "capital_exit_inflight_reconciliation_required"
        );
        self.evidence(
            view,
            source_observation_id,
            sequence,
            &report,
            CapitalExitEvidenceKindV1::FenceApplied {
                native_gate_report_ref: format!(
                    "{}:{}",
                    view.command_id,
                    view.account_control_epoch.get()
                ),
                controlled_strategy_ids: vec![self.binding.strategy_id.to_string()],
                invalidated_target_claim_refs: self.gate.original_target_claim_refs(),
                invalidated_child_timer_refs: vec![],
                resolved_inflight_report_ref: report.report_ref(),
                remaining_trading: CapitalExitRemainingTradingV1::FencedPendingTarget,
                remaining_target_authority_ref: None,
            },
        )
    }

    /// Invoke on the owner event thread. New reductions require the authenticated
    /// fence evidence to be accepted first. Claimed stop commands may cancel their
    /// original IDs while valuation/evidence intake is unavailable; retain and
    /// relay their original outcome reports. A return is never a financial ACK.
    pub fn advance<S: Strategy + StrategyNative + DataActorNative>(
        &self,
        strategy: &mut S,
        view: &CapitalExitViewV1,
        source_observation_id: Id,
        sequence: DbCounter,
    ) -> Result<CapitalExitOwnerEvidenceV1> {
        self.admitted_control(view)?;
        if stopping(view) {
            return self.stop_native_orders(strategy, view, source_observation_id, sequence);
        }
        let report = self.inspect(strategy)?;
        let issued = self.gate.issued_order_ids();
        // Every original submitted ID must be reconciled by native cache. A lost
        // submit response does not authorize a fresh order ID.
        for id in &issued {
            ensure!(
                report
                    .orders
                    .iter()
                    .any(|o| o.client_order_id().as_str() == id),
                "capital_exit_original_order_reconciliation_required"
            );
        }
        let active_exit: Vec<_> = report
            .orders
            .iter()
            .filter(|o| {
                !o.is_closed() && issued.iter().any(|id| o.client_order_id().as_str() == id)
            })
            .collect();
        let (phase, reasons) = if report.openers().next().is_some() {
            for order in report.openers() {
                strategy.cancel_order(
                    order.client_order_id(),
                    Some(self.binding.client_id),
                    None,
                )?;
            }
            (CapitalExitStateV1::CancellingOpeners, vec![])
        } else if !active_exit.is_empty() {
            (CapitalExitStateV1::Reducing, vec![])
        } else {
            let needed = (exact(&view.funds.reserved_amount.amount)?
                - report.balance()?.free.as_decimal())
            .max(Decimal::ZERO);
            if needed == Decimal::ZERO {
                (CapitalExitStateV1::WaitingEvidence, vec![])
            } else if let CapitalExitPolicyV1::BoundedLimit {
                legs,
                max_execution_cost,
                ..
            } = &view.policy
            {
                if self.streaming_paper {
                    let admitted = self
                        .gate
                        .control()
                        .ok_or_else(|| anyhow!("capital_exit_fence_required"))?;
                    let command_at = admitted
                        .updated_at
                        .timestamp_nanos_opt()
                        .and_then(|value| u64::try_from(value).ok())
                        .ok_or_else(|| anyhow!("capital_exit_native_command_clock"))?;
                    // External wall-time authority must not create a simulated
                    // order in an earlier market batch. Wait for an original
                    // quote after this claimed command; never advance a clock.
                    ensure!(
                        report.quote.ts_init.as_u64() >= command_at
                            && report.snapshot.ts_init.as_u64() >= command_at,
                        "capital_exit_native_command_time_pending"
                    );
                }
                ensure!(
                    legs.len() == 1
                        && legs[0].instrument_id == self.binding.instrument_id.to_string(),
                    "capital_exit_execution_bounds"
                );
                ensure!(
                    report.protection().next().is_none(),
                    "capital_exit_protective_quantity_overlap"
                );
                let leg = &legs[0];
                let floor = exact(&leg.minimum_sell_price)?;
                let (original_quote, original_maker_fee, original_taker_fee) = self
                    .reference_quotes
                    .borrow()
                    .iter()
                    .find(|(id, ..)| *id == max_execution_cost.reference_evidence_id)
                    .map(|(_, q, maker, taker)| (*q, *maker, *taker))
                    .ok_or_else(|| anyhow!("capital_exit_original_cost_reference_unavailable"))?;
                ensure!(
                    report.instrument.maker_fee() == original_maker_fee
                        && report.instrument.taker_fee() == original_taker_fee,
                    "capital_exit_fee_evidence_changed"
                );
                let cumulative = report
                    .orders
                    .iter()
                    .filter(|o| issued.iter().any(|id| o.client_order_id().as_str() == id))
                    .map(|o| o.filled_qty().as_decimal())
                    .try_fold(Decimal::ZERO, |a, b| a.checked_add(b))
                    .ok_or_else(|| anyhow!("capital_exit_native_decimal_range"))?;
                let maximum =
                    (exact(&leg.maximum_reduction_quantity)? - cumulative).max(Decimal::ZERO);
                let position = report
                    .positions
                    .iter()
                    .find(|p| p.is_open())
                    .ok_or_else(|| anyhow!("capital_exit_native_holdings_unavailable"))?;
                let step = report.instrument.size_increment().as_decimal();
                let mut desired = needed
                    .checked_div(floor)
                    .and_then(|v| v.checked_div(step))
                    .and_then(|v| v.ceil().checked_mul(step))
                    .ok_or_else(|| anyhow!("capital_exit_native_decimal_range"))?;
                if self.streaming_paper {
                    // Include the same official fee model in the required gross
                    // proceeds. Converge upward at native quantity precision;
                    // never invent a fee rate or submit an underfunded tiny tail.
                    let mut covered = false;
                    for _ in 0..64 {
                        let fee = self.fee_bound(&report, desired, floor)?;
                        let next = needed
                            .checked_add(fee)
                            .and_then(|gross| gross.checked_div(floor))
                            .and_then(|quantity| quantity.checked_div(step))
                            .and_then(|quantity| quantity.ceil().checked_mul(step))
                            .ok_or_else(|| anyhow!("capital_exit_native_decimal_range"))?;
                        if next <= desired {
                            covered = true;
                            break;
                        }
                        desired = next;
                    }
                    ensure!(covered, "capital_exit_native_fee_bound_unavailable");
                }
                let quantity = report.instrument.try_make_qty_from_decimal(
                    desired.min(maximum).min(position.quantity.as_decimal()),
                    Some(true),
                )?;
                let approved = exact(&leg.maximum_reduction_quantity)?;
                let worst_cost = (original_quote.bid_price.as_decimal() - floor)
                    .max(Decimal::ZERO)
                    .checked_mul(approved)
                    .and_then(|v| v.checked_add(self.fee_bound(&report, approved, floor).ok()?))
                    .ok_or_else(|| anyhow!("capital_exit_native_fee_bound_unavailable"))?;
                ensure!(
                    worst_cost <= exact(&max_execution_cost.amount)?,
                    "capital_exit_cost_bound_exceeded"
                );
                if quantity.as_decimal() == Decimal::ZERO {
                    (
                        CapitalExitStateV1::Blocked,
                        vec!["capital_exit_authorized_quantity_exhausted".into()],
                    )
                } else {
                    ensure!(
                        report.quote.bid_price.as_decimal() >= floor,
                        "capital_exit_price_floor_unavailable"
                    );
                    let price = report.instrument.try_make_price_from_decimal(floor)?;
                    ensure!(
                        price.as_decimal() >= floor,
                        "capital_exit_price_floor_precision"
                    );
                    let id = ClientOrderId::new_checked(
                        format!("QZ-EXIT-{}-{}", view.id, issued.len()).as_str(),
                    )?;
                    let order = strategy.order_factory().limit(
                        self.binding.instrument_id,
                        OrderSide::Sell,
                        quantity,
                        price,
                        Some(TimeInForce::Gtc),
                        None,
                        Some(false),
                        Some(true),
                        Some(false),
                        None,
                        None,
                        None,
                        None,
                        None,
                        None,
                        Some(id),
                    );
                    // Original intent ID is durable BEFORE submitting through the
                    // native Strategy -> RiskEngine -> ExecutionEngine route.
                    self.gate.record_order_before_submit(view.id, id.as_str())?;
                    strategy.submit_order(
                        order,
                        Some(position.id),
                        Some(self.binding.client_id),
                        None,
                    )?;
                    (CapitalExitStateV1::Reducing, vec![])
                }
            } else {
                (
                    CapitalExitStateV1::Blocked,
                    vec!["capital_exit_cash_only_insufficient".into()],
                )
            }
        };
        // Re-read native evidence after cancel/submit; command success alone is
        // never a cancelled/fill/cash-release observation.
        let current = self.inspect(strategy)?;
        let released = money(
            current
                .balance()?
                .free
                .as_decimal()
                .min(exact(&view.funds.reserved_amount.amount)?),
            self.currency,
        )?;
        self.evidence(
            view,
            source_observation_id,
            sequence,
            &current,
            CapitalExitEvidenceKindV1::NativeProgress {
                phase,
                released_cash_amount: Some(released),
                native_order_refs: current
                    .orders
                    .iter()
                    .map(order_ref)
                    .collect::<Result<_>>()?,
                native_position_refs: current.positions.iter().map(|p| p.id.to_string()).collect(),
                native_report_ref: current.report_ref(),
                reason_codes: reasons,
            },
        )
    }

    /// Simulation-only availability. It is never real venue withdrawal proof;
    /// no LIVE adapter or withdrawal method exists in this component.
    pub fn simulated_availability<S: Strategy + StrategyNative + DataActorNative>(
        &self,
        strategy: &S,
        view: &CapitalExitViewV1,
        source_observation_id: Id,
        sequence: DbCounter,
    ) -> Result<CapitalExitOwnerEvidenceV1> {
        self.admitted_control(view)?;
        let report = self.inspect(strategy)?;
        ensure!(
            !report.orders.iter().any(|o| !o.is_closed()),
            "capital_exit_native_orders_unresolved"
        );
        let reference = report.report_ref();
        self.evidence(
            view,
            source_observation_id,
            sequence,
            &report,
            CapitalExitEvidenceKindV1::WithdrawabilityObserved {
                available_cash: money(
                    report
                        .balance()?
                        .free
                        .as_decimal()
                        .min(exact(&view.funds.reserved_amount.amount)?),
                    self.currency,
                )?,
                basis: CapitalExitAvailabilityBasisV1::ControlledSandbox,
                venue: self.binding.instrument_id.venue.to_string(),
                native_availability_report_ref: reference.clone(),
                settlement_report_ref: report.account.event_id.to_string(),
                margin_report_ref: reference.clone(),
                open_orders_report_ref: reference,
            },
        )
    }

    fn evidence(
        &self,
        view: &CapitalExitViewV1,
        source_observation_id: Id,
        sequence: DbCounter,
        report: &NativeReport,
        evidence: CapitalExitEvidenceKindV1,
    ) -> Result<CapitalExitOwnerEvidenceV1> {
        let asof = DateTime::from_timestamp_nanos(report.snapshot.ts_init.as_u64() as i64);
        let evidence = CapitalExitOwnerEvidenceV1 {
            schema_version: SchemaV1,
            intent_id: view.id,
            command_id: view.command_id,
            account_source_id: self.source_id()?,
            owner_binding_ref: self.owner_binding_ref()?,
            native_session_id: self.session_id.clone(),
            native_account_id: self.account_id.to_string(),
            account_control_epoch: view.account_control_epoch,
            external_claim_id: view
                .external_claim_id
                .clone()
                .ok_or_else(|| anyhow!("capital_exit_claim_required"))?,
            external_message_id: format!(
                "{}:{}:{}",
                view.command_id,
                report.report_ref(),
                sequence.get()
            ),
            sequence,
            source_observation_id,
            asof,
            valid_until: asof + Duration::seconds(5),
            original_event_refs: vec![report.report_ref(), report.account.event_id.to_string()],
            native_evidence: json!({"native_report":report.raw,"owner_control_report":self.gate.retained_control_report()?}),
            evidence,
        };
        self.gate
            .retain_original_message("evidence", sequence, &evidence)?;
        Ok(evidence)
    }
}
