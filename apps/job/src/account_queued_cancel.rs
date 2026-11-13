//! Internal, uninstalled ordinary-venue cancellation experiment. Not admission,
//! persistence or a replacement for Strategy's emulator/algo/GTD handling.
use anyhow::{anyhow, ensure, Result};
use nautilus_common::{
    cache::Cache,
    clock::Clock,
    messages::execution::{CancelOrder, TradingCommand},
    msgbus::{self, MessagingSwitchboard},
    runner::try_get_trading_cmd_sender,
};
use nautilus_core::UUID4;
use nautilus_execution::engine::ExecutionEngine;
use nautilus_live::node::{LiveNode, LiveNodeHandle};
use nautilus_model::{
    enums::OrderStatus,
    identifiers::{AccountId, ClientOrderId, InstrumentId, StrategyId, TraderId},
    orders::Order,
};
use std::{cell::RefCell, rc::Rc};

pub(crate) struct OriginalOrder {
    pub trader: TraderId,
    pub account: AccountId,
    pub strategy: StrategyId,
    pub instrument: InstrumentId,
    pub client_order_id: ClientOrderId,
}

/// Internal node-owned binding, not a production admission or source/build proof.
/// Capture all native references together; never accept independently supplied
/// cache/engine/clock at a transport boundary. Rc also prevents Send/Sync transfer.
/// This does NOT establish ownership of thread-global message-bus endpoints or
/// TLS trading sender: a second node on this thread can replace both. Installing
/// this component requires a verified exclusive single-node runner lifecycle;
/// multi-node owner threads are unsupported. No production installation exists.
#[allow(dead_code)]
pub(crate) struct BoundCancelNode {
    session: UUID4,
    trader: TraderId,
    environment: nautilus_common::enums::Environment,
    owner: std::thread::ThreadId,
    handle: LiveNodeHandle,
    cache: Rc<RefCell<Cache>>,
    engine: Rc<RefCell<ExecutionEngine>>,
    clock: Rc<RefCell<dyn Clock>>,
}

#[allow(dead_code)]
impl BoundCancelNode {
    pub(crate) fn capture(node: &LiveNode) -> Self {
        Self {
            session: node.instance_id(),
            trader: node.trader_id(),
            environment: node.kernel().environment(),
            owner: std::thread::current().id(),
            handle: node.handle(),
            cache: node.kernel().cache(),
            engine: node.kernel().exec_engine().clone(),
            clock: node.kernel().clock(),
        }
    }

    /// The expected native instance and environment must come from the original
    /// durable command. This still does not authenticate a source/build or prove
    /// the Pi send checkpoint was committed; no production caller installs it.
    pub(crate) fn dispatch_attempt(
        &self,
        session: UUID4,
        environment: nautilus_common::enums::Environment,
        original: &OriginalOrder,
    ) -> Result<UUID4> {
        ensure!(
            self.owner == std::thread::current().id()
                && self.session == session
                && self.environment == environment
                && self.trader == original.trader,
            "native_cancel_node_binding_mismatch"
        );
        dispatch_attempt(
            &self.handle,
            &self.cache,
            &self.engine,
            original,
            &self.clock,
        )
    }
}

/// Call only from the owning running node thread, after the original Pi command
/// and its complete session identity are committed UNCERTAIN. No retry or receipt
/// guarantee: the official sender returns void and may log a failed dispatch.
#[allow(dead_code)]
pub(crate) fn dispatch_attempt(
    handle: &LiveNodeHandle,
    cache: &Rc<RefCell<Cache>>,
    engine: &Rc<RefCell<ExecutionEngine>>,
    original: &OriginalOrder,
    clock: &Rc<RefCell<dyn Clock>>,
) -> Result<UUID4> {
    ensure!(
        handle.is_running() && try_get_trading_cmd_sender().is_some(),
        "native_cancel_deferred_runner_unavailable"
    );
    let (order, client) = {
        let cache = cache.borrow();
        let order = cache
            .order(&original.client_order_id)
            .ok_or_else(|| anyhow!("native_cancel_order_unknown"))?
            .clone();
        let client = *cache
            .client_id(&original.client_order_id)
            .ok_or_else(|| anyhow!("native_cancel_original_client_unknown"))?;
        (order, client)
    };
    ensure!(
        order.trader_id() == original.trader
            && order.account_id() == Some(original.account)
            && order.strategy_id() == original.strategy
            && order.instrument_id() == original.instrument,
        "native_cancel_original_identity_mismatch"
    );
    ensure!(
        matches!(
            order.status(),
            OrderStatus::Accepted | OrderStatus::PartiallyFilled
        ) && order.venue_order_id().is_some()
            && !order.is_emulated()
            && !order.is_active_local()
            && order.emulation_trigger().is_none()
            && order.exec_algorithm_id().is_none(),
        "native_cancel_requires_ordinary_venue_order"
    );
    let id = UUID4::new();
    let command = TradingCommand::CancelOrder(CancelOrder::new(
        order.trader_id(),
        Some(client),
        order.strategy_id(),
        order.instrument_id(),
        order.client_order_id(),
        order.venue_order_id(),
        id,
        clock.borrow().timestamp_ns(),
        None,
        None,
    ));
    {
        let engine = engine.borrow();
        let registered = engine
            .get_client(&client)
            .ok_or_else(|| anyhow!("native_cancel_original_client_not_registered"))?;
        ensure!(
            registered.client_id() == client && registered.account_id() == original.account,
            "native_cancel_registered_client_mismatch"
        );
        let routed = engine
            .find_client_for_command(&command)
            .ok_or_else(|| anyhow!("native_cancel_route_unavailable"))?;
        ensure!(
            routed.client_id == client && routed.account_id == original.account,
            "native_cancel_route_mismatch"
        );
    }
    // No cache/engine/strategy borrow crosses official deferred dispatch.
    msgbus::send_trading_command(MessagingSwitchboard::exec_engine_queue_execute(), command);
    Ok(id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use nautilus_common::{enums::Environment, logging::logger::LoggerConfig};
    use nautilus_live::node::LiveNode;
    #[test]
    fn absent_sender_and_unstarted_node_are_rejected() {
        std::thread::spawn(|| {
            assert!(try_get_trading_cmd_sender().is_none());
            let mut node =
                LiveNode::builder(TraderId::from("CANCEL-GUARD-001"), Environment::Sandbox)
                    .unwrap()
                    .with_logging(LoggerConfig {
                        bypass_logging: true,
                        ..Default::default()
                    })
                    .build()
                    .unwrap();
            let original = OriginalOrder {
                trader: TraderId::from("CANCEL-GUARD-001"),
                account: AccountId::from("PAPER-001"),
                strategy: StrategyId::from("S-001"),
                instrument: InstrumentId::from("BTC-USDC-SPOT.HYPERLIQUID"),
                client_order_id: ClientOrderId::from("absent"),
            };
            let error = dispatch_attempt(
                &node.handle(),
                &node.kernel().cache(),
                node.kernel().exec_engine(),
                &original,
                &node.kernel().clock(),
            )
            .unwrap_err();
            assert_eq!(
                error.to_string(),
                "native_cancel_deferred_runner_unavailable"
            );
            node.dispose();
        })
        .join()
        .unwrap();
    }
}
