//! Account execution adapter hosted by the existing remote worker. Business
//! intents stay in TypeScript; every financial call below uses the official node.
//!
//! BOOT connects for official cache/reconciliation with no signal strategy registered.
//! Explicit START creates the configured native strategy, loads its official cache state,
//! and starts it through the official Controller. Recovery never invokes signal callbacks.
//!
//! Capital exits use the controller's upstream HEDGING OMS and an explicit existing
//! position ID, with reduce-only orders. Only a unique long position is supported;
//! ambiguous/short positions are rejected. Sandbox evidence is not a claim of live
//! exchange execution validation, nor generic strategy recovery compatibility.
use crate::{account_observer::project_snapshot, account_runtime_cache::AccountCacheFactory};
use anyhow::{anyhow, bail, ensure, Context, Result};
use contracts::{native_strategy::NativeCompiledStrategyV1, Id};
use indexmap::IndexMap;
use nautilus_common::{
    actor::{DataActor, DataActorNative},
    cache::{Cache, CacheConfig},
    component::{component_state, reset_component, Component},
    enums::{ComponentState, Environment},
    logging::logger::LoggerConfig,
    timer::TimeEvent,
};
use nautilus_core::DurationNanos;
use nautilus_execution::models::{
    fee::{FeeModelAny, MakerTakerFeeModel},
    fill::{DefaultFillModel, FillModelAny},
    latency::{LatencyModelAny, StaticLatencyModel},
};
use nautilus_hyperliquid::{
    common::{
        consts::HYPERLIQUID_CLIENT_ID,
        enums::{HyperliquidEnvironment, HyperliquidProductType},
    },
    http::client::HyperliquidHttpClient,
    HyperliquidDataClientConfig, HyperliquidDataClientFactory, HyperliquidExecutionClientConfig,
    HyperliquidExecutionClientFactory,
};
use nautilus_live::{
    config::{LiveExecutionEngineConfig, LiveRiskEngineConfig},
    execution::submission::SubmissionRecoveryPolicy,
    node::{LiveNode, LiveNodeHandle},
};
use nautilus_model::{
    data::QuoteTick,
    enums::{AccountType, BookType, OrderSide, TimeInForce},
    events::OrderEventAny,
    identifiers::{AccountId, ClientId, ClientOrderId, InstrumentId, StrategyId, TraderId, Venue},
    instruments::{Instrument, InstrumentAny},
    orders::Order,
    types::{Currency, Money, Price},
};
use nautilus_portfolio::config::PortfolioConfig;
use nautilus_sandbox::{SandboxExecutionClientConfig, SandboxExecutionClientFactory};
use nautilus_trading::{
    nautilus_strategy,
    strategy::{Strategy, StrategyConfig, StrategyCore, StrategyNative},
};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
    num::NonZeroUsize,
    path::PathBuf,
    str::FromStr,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
        Arc,
    },
};
use ustr::Ustr;

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AccountRuntimeConfig {
    pub client_id: String,
    pub account_id: String,
    pub mode: String,
    pub account_address: Option<String>,
    pub native_strategy: NativeStrategyDeployment,
    pub instruments: Vec<String>,
    pub starting_balance: Option<String>,
    pub paper_fee_rate: Option<String>,
    #[serde(default)]
    pub risk: AccountRiskConfig,
    pub paper_execution: Option<PaperExecutionConfig>,
    pub source_id: String,
    pub trader_id: Option<String>,
    pub data_dir: PathBuf,
    #[serde(default)]
    pub testnet: bool,
    /// Omitted retains all products, including existing external orders during recovery.
    pub execution_product_types: Option<Vec<HyperliquidProductType>>,
}
/// Explicit compiled upstream strategy bound to this physical account host.
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct NativeStrategyDeployment {
    pub instrument_id: String,
    pub strategy: NativeCompiledStrategyV1,
}

/// Only expose native limits, never the native bypass switch.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct AccountRiskConfig {
    pub max_order_submit_rate: Option<String>,
    pub max_order_modify_rate: Option<String>,
    pub max_notional_per_order: std::collections::HashMap<String, String>,
}
impl AccountRiskConfig {
    fn native(&self, instruments: &[String]) -> Result<LiveRiskEngineConfig> {
        ensure!(
            self.max_notional_per_order
                .keys()
                .all(|id| instruments.contains(id)),
            "account_native_risk_instrument_scope"
        );
        let mut config = LiveRiskEngineConfig::default();
        config.bypass = false;
        if let Some(rate) = &self.max_order_submit_rate {
            config.max_order_submit_rate = rate.clone();
        }
        if let Some(rate) = &self.max_order_modify_rate {
            config.max_order_modify_rate = rate.clone();
        }
        config.max_notional_per_order = self.max_notional_per_order.clone();
        Ok(config)
    }
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PaperExecutionConfig {
    pub prob_fill_on_limit: f64,
    pub prob_slippage: f64,
    pub random_seed: String,
    // Decimal strings retain nanosecond precision across the JSON boundary.
    pub base_latency_ns: String,
    pub insert_latency_ns: String,
    pub update_latency_ns: String,
    pub cancel_latency_ns: String,
    #[serde(default)]
    pub queue_position: bool,
}
impl PaperExecutionConfig {
    fn models(&self) -> Result<(FillModelAny, LatencyModelAny)> {
        let base = self.base_latency_ns.parse::<u64>()?;
        let insert = self.insert_latency_ns.parse::<u64>()?;
        let update = self.update_latency_ns.parse::<u64>()?;
        let cancel = self.cancel_latency_ns.parse::<u64>()?;
        // The upstream constructor adds each delay to base; reject overflow
        // before entering that infallible constructor.
        ensure!(
            [insert, update, cancel]
                .iter()
                .all(|delay| base.checked_add(*delay).is_some()),
            "account_native_latency_overflow"
        );
        Ok((
            FillModelAny::Default(DefaultFillModel::new(
                self.prob_fill_on_limit,
                self.prob_slippage,
                Some(self.random_seed.parse()?),
            )?),
            LatencyModelAny::Static(StaticLatencyModel::new(
                DurationNanos::new(base),
                DurationNanos::new(insert),
                DurationNanos::new(update),
                DurationNanos::new(cancel),
            )),
        ))
    }
}
#[derive(Clone, Debug, Deserialize)]
struct Request {
    #[serde(rename = "requestId")]
    id: String,
    operation: String,
    #[serde(flatten)]
    body: BTreeMap<String, Value>,
}
// Official node-wide activity indices. No account filter: unassigned orders and
// other strategies/accounts must conservatively prevent a zero-order proof.
fn official_node_order_activity(cache: &Cache) -> Value {
    json!({"open":cache.has_orders_open(None,None,None,None,None),
        "inflight":cache.has_orders_inflight(None,None,None,None,None),
        "active_local":cache.has_orders_active_local(None,None,None,None,None),
        "emulated":cache.has_orders_emulated(None,None,None,None,None)})
}
impl Request {
    fn ensure_external(&self) -> Result<()> {
        ensure!(
            self.operation != "SPOT_BALANCE_REFRESH",
            "account_native_internal_operation"
        );
        Ok(())
    }
    fn value(&self, key: &str) -> &Value {
        self.body.get(key).unwrap_or(&Value::Null)
    }
    fn text(&self, key: &str) -> Result<&str> {
        self.value(key)
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow!("account_native_{key}_required"))
    }
    fn decimal(&self, key: &str) -> Result<Decimal> {
        Decimal::from_str(self.text(key)?).map_err(|_| anyhow!("account_native_{key}_decimal"))
    }
}
fn respond(id: &str, result: Result<Value>) {
    let value = match result {
        Ok(result) => json!({"requestId":id,"result":result}),
        Err(error) => json!({"requestId":id,"error":error.to_string()}),
    };
    let mut out = io::stdout().lock();
    let _ = serde_json::to_writer(&mut out, &value);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}
fn event(value: Value) {
    let mut out = io::stdout().lock();
    let _ = serde_json::to_writer(&mut out, &value);
    let _ = out.write_all(b"\n");
    let _ = out.flush();
}
#[derive(Clone, Debug)]
struct Control {
    epoch: u64,
    revision: String,
    reserved: Decimal,
    buffer: Decimal,
    mode: String,
    exits: Vec<String>,
    deadline: Option<u64>,
    minimum_interval_ns: u64,
}
#[derive(Clone, Debug)]
struct SpotFacts {
    free: Decimal,
    total: Decimal,
    balances: Value,
    asof: u64,
    valid_until: u64,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
struct TargetAcceptance {
    original: BTreeMap<String, Value>,
    epoch: String,
    submitted: Vec<String>,
    receipt: Option<Value>,
}
/// Static user-crate hook, invoked only by explicit strategy START after admission.
/// The callback must register exactly one concrete upstream Strategy through `add`.
pub type AccountStrategyRegistration =
    fn(&mut AccountStrategyRegistrar<'_>, &str, &Value) -> Result<()>;

/// Thin registration context over the existing official account Trader and Cache.
/// User strategies retain upstream callbacks, parameters and order behavior.
pub struct AccountStrategyRegistrar<'a> {
    owner: &'a mut AccountController,
}
impl AccountStrategyRegistrar<'_> {
    pub fn instrument_id(&self) -> Result<InstrumentId> {
        Ok(self.owner.config.native_strategy.instrument_id.parse()?)
    }
    pub fn instruments(&self) -> &[InstrumentId] {
        &self.owner.ids
    }
    /// Original node trader identity; not a user-configured replacement.
    pub fn trader_id(&self) -> TraderId {
        self.owner.native_trader.borrow().trader_id
    }
    /// Original account identity used by this runtime.
    pub fn account_id(&self) -> AccountId {
        self.owner.account
    }
    /// Runtime execution client selected for this account. This is routing
    /// context, not a connectivity, balance, or trading-permission assertion.
    pub fn execution_client_id(&self) -> ClientId {
        self.owner.client
    }
    pub fn add<T>(&mut self, strategy: T) -> Result<()>
    where
        T: Strategy + StrategyNative + DataActorNative + Component + std::fmt::Debug + 'static,
    {
        ensure!(
            self.owner.signal_strategy_id.is_none(),
            "account_native_strategy_already_registered"
        );
        self.owner.install_signal(strategy)
    }
}

struct AccountController {
    core: StrategyCore,
    cancel_node: Option<crate::account_queued_cancel::BoundCancelNode>,
    compatibility: crate::native_runner::AccountCompatibility,
    signal_strategy_id: Option<StrategyId>,
    user_registration: Option<AccountStrategyRegistration>,
    signal_restore_error: Option<String>,
    native_cache: std::rc::Rc<std::cell::RefCell<nautilus_common::cache::Cache>>,
    native_risk: std::rc::Rc<std::cell::RefCell<nautilus_risk::engine::RiskEngine>>,
    native_trader: std::rc::Rc<std::cell::RefCell<nautilus_system::trader::Trader>>,
    native_managed_stop: bool,
    config: AccountRuntimeConfig,
    ids: Vec<InstrumentId>,
    client: ClientId,
    account: AccountId,
    inbox: Receiver<Request>,
    handle: LiveNodeHandle,
    boot_id: String,
    session: String,
    trader: String,
    fee_rate: Decimal,
    fee_source: Value,
    fee_valid_until_ns: u64,
    control: Option<Control>,
    snapshot: Option<Value>,
    last_tick_ns: u64,
    last_slice_ns: u64,
    cache_failed: Arc<AtomicBool>,
    spot_facts: Option<SpotFacts>,
    spot_balance_observation: Option<Value>,
    target_acceptances: BTreeMap<String, TargetAcceptance>,
    #[cfg(test)]
    replies: Option<std::rc::Rc<std::cell::RefCell<BTreeMap<String, Value>>>>,
}
impl std::fmt::Debug for AccountController {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountController")
            .field("account", &self.account)
            .finish()
    }
}
nautilus_strategy!(AccountController, {});

impl DataActor for AccountController {
    fn on_save(&self) -> Result<IndexMap<String, Vec<u8>>> {
        self.target_acceptances
            .iter()
            .map(|(id, value)| Ok((format!("target:{id}"), serde_json::to_vec(value)?)))
            .collect()
    }
    fn on_load(&mut self, state: IndexMap<String, Vec<u8>>) -> Result<()> {
        for (key, bytes) in state {
            if let Some(id) = key.strip_prefix("target:") {
                self.target_acceptances
                    .insert(id.into(), serde_json::from_slice(&bytes)?);
            }
        }
        self.control = None;
        Ok(())
    }
    fn on_start(&mut self) -> Result<()> {
        // Recovery starts no signal strategy, including no signal on_load/on_start.
        self.native_risk
            .borrow_mut()
            .set_trading_state(nautilus_model::enums::TradingState::Halted);
        let ids = self.ids.clone();
        self.subscribe_quotes(ids.clone(), Some(*HYPERLIQUID_CLIENT_ID), None);
        self.subscribe_book_at_interval(
            ids,
            BookType::L2_MBP,
            NonZeroUsize::new(20),
            NonZeroUsize::new(100).unwrap(),
            Some(*HYPERLIQUID_CLIENT_ID),
            None,
        );
        self.clock().set_timer_ns(
            "qz-account-native-input",
            DurationNanos::new(20_000_000),
            None,
            None,
            None,
            Some(false),
            Some(false),
        )?;
        respond(
            &self.boot_id,
            Ok(
                json!({"account_summary_protocol":1,"strategy_lifecycle":self.strategy_lifecycle(),"ready":true,"fenced":true,"native_version":crate::NAUTILUS_VERSION,"native_session_id":self.session,"recovery":"OFFICIAL_CACHE_AND_RECONCILIATION","recovery_only":self.signal_strategy_id.is_none(),"ordinary_venue_cancel":self.cancel_node.is_some(),"account_compatibility_declaration":self.compatibility.declaration(),"account_compatibility_verified":false,"ordinary_venue_cancel_scope":if self.cancel_node.is_none(){None}else if self.user_registration.is_some(){Some("DECLARED_TRUSTED_STRATEGY_ONLY_PAPER_ORDINARY_VENUE")}else{Some("KNOWN_OWNED_EMA_PAPER_ORDINARY_VENUE")}}),
            ),
        );
        Ok(())
    }
    fn on_time_event(&mut self, _: &TimeEvent) -> Result<()> {
        for _ in 0..32 {
            let Ok(request) = self.inbox.try_recv() else {
                break;
            };
            let result = self.process(&request);
            #[cfg(test)]
            if let Some(replies) = &self.replies {
                replies.borrow_mut().insert(
                    request.id.clone(),
                    match &result {
                        Ok(v) => v.clone(),
                        Err(e) => json!({"error":e.to_string()}),
                    },
                );
            }
            if !request.id.is_empty() {
                respond(&request.id, result);
            }
        }
        let now = self.clock().timestamp_ns().as_u64();
        if now.saturating_sub(self.last_tick_ns) >= 1_000_000_000 {
            self.last_tick_ns = now;
            event(json!({"event":"ACCOUNT_TICK","observed_at_ns":now.to_string()}));
        }
        Ok(())
    }
    fn on_quote(&mut self, _: &QuoteTick) -> Result<()> {
        let _ = self.refresh_snapshot();
        Ok(())
    }
}
// Keep official integer tokens intact across JavaScript JSON parsing. This is
// serialization only; legacy lastEvent remains for compatibility, not exact ns.
// lastEventJson is official serde output, not exchange wire bytes. Already
// persisted rounded objects are never upgraded or used to reconstruct it.
fn official_order_event_json(event: &OrderEventAny) -> Result<String> {
    Ok(serde_json::to_string(event)?)
}

impl AccountController {
    fn install_signal<T>(&mut self, strategy: T) -> Result<()>
    where
        T: Strategy + StrategyNative + DataActorNative + Component + std::fmt::Debug + 'static,
    {
        self.native_managed_stop = strategy.strategy_core().config.manage_stop;
        let controller = nautilus_system::Controller::new(self.native_trader.clone(), None);
        let id = controller.create_strategy(strategy, false)?;
        self.signal_strategy_id = Some(id);
        let restore = (|| -> Result<()> {
            let state = self.native_cache.borrow().load_strategy_state(&id)?;
            if let Some(state) = state.filter(|state| !state.is_empty()) {
                let mut actor =
                    nautilus_common::actor::registry::try_get_actor_unchecked::<T>(&id.inner())
                        .ok_or_else(|| anyhow!("account_native_strategy_registration_missing"))?;
                actor.on_load(state)?;
            }
            Ok(())
        })();
        if let Err(error) = restore {
            self.signal_restore_error = Some(format!(
                "account_native_strategy_state_restore_failed: {error:#}"
            ));
            return Err(error);
        }
        Ok(())
    }
    fn register_signal(&mut self) -> Result<()> {
        let id: InstrumentId = self.config.native_strategy.instrument_id.parse()?;
        ensure!(
            self.ids.contains(&id),
            "account_native_strategy_instrument_not_configured"
        );
        #[cfg(test)]
        if self.config.client_id == "test-on-start-resting" {
            return self.install_signal(tests::OnStartResting {
                core: StrategyCore::new(StrategyConfig {
                    strategy_id: Some(StrategyId::from("RESTING-001")),
                    order_id_tag: Some("001".into()),
                    ..Default::default()
                }),
                instrument: id,
                client: self.client,
            });
        }
        #[cfg(test)]
        if self.config.client_id == "test-on-start" {
            return self.install_signal(tests::OnStartSubmit {
                core: StrategyCore::new(StrategyConfig {
                    strategy_id: Some(StrategyId::from("ONSTART-001")),
                    order_id_tag: Some("001".into()),
                    ..Default::default()
                }),
                instrument: id,
                client: self.client,
                trade_size: nautilus_model::types::Quantity::from("0.10"),
            });
        }
        if let NativeCompiledStrategyV1::Registered { name, config } =
            &self.config.native_strategy.strategy
        {
            let registration = self.user_registration.ok_or_else(|| {
                anyhow!("NATIVE_REGISTERED_ACCOUNT_STRATEGY_REQUIRES_USER_RUNNER")
            })?;
            let name = name.clone();
            let config = config.clone();
            if let Err(error) = registration(
                &mut AccountStrategyRegistrar { owner: self },
                &name,
                &config,
            ) {
                if self.signal_strategy_id.is_some() {
                    self.signal_restore_error = Some(format!(
                        "account_native_user_registration_failed: {error:#}"
                    ));
                }
                return Err(error);
            }
            if let Some(error) = &self.signal_restore_error {
                bail!("{error}");
            }
            ensure!(
                self.signal_strategy_id.is_some(),
                "NATIVE_ACCOUNT_REGISTRATION_DID_NOT_ADD_STRATEGY"
            );
            return Ok(());
        }
        match crate::native_strategy::registry::build_strategy(
            id,
            &self.config.native_strategy.strategy,
        )? {
            crate::native_strategy::registry::CompiledNativeStrategy::EmaCross(s) => {
                self.install_signal(s)
            }
            crate::native_strategy::registry::CompiledNativeStrategy::OrderBookImbalance(s) => {
                self.install_signal(s)
            }
            _ => bail!("account_native_strategy_account_lifecycle_not_integrated"),
        }
    }
    fn fence(&self, epoch: u64) -> Result<&Control> {
        ensure!(
            !self.cache_failed.load(Ordering::SeqCst),
            "account_native_cache_write_failed"
        );
        let c = self
            .control
            .as_ref()
            .ok_or_else(|| anyhow!("account_native_budget_fence_not_applied"))?;
        ensure!(c.epoch == epoch, "account_native_obsolete_control_epoch");
        ensure!(c.mode != "PAUSED", "account_native_account_paused");
        Ok(c)
    }
    fn refresh_snapshot(&mut self) -> Result<()> {
        let account = self.account;
        let native = self
            .portfolio_rc()
            .try_borrow_mut()
            .map_err(|_| anyhow!("account_native_portfolio_busy"))?
            .build_snapshot(&account)
            .ok_or_else(|| anyhow!("account_native_snapshot_unavailable"))?;
        let projected = project_snapshot(&native)?;
        let cache = self.cache();
        let mut instruments = vec![];
        let now = self.clock().timestamp_ns().as_u64();
        for id in &self.ids {
            let Some(instrument) = cache.instrument(id) else {
                continue;
            };
            let Some(quote) = cache.quote(id) else {
                continue;
            };
            let depth=cache.order_book(id).map(|book|book.bids(Some(20)).map(|level|json!({"price":level.price.value.as_decimal().to_string(),"quantity":level.size_decimal().to_string()})).collect::<Vec<_>>()).unwrap_or_default();
            let book_ns = cache.order_book(id).map(|b| b.ts_last.as_u64());
            instruments.push(json!({"instrument_id":id.to_string(),"base_currency":instrument.base_currency().map(|v|v.to_string()),"quote_currency":instrument.quote_currency().to_string(),
                "size_increment":instrument.size_increment().as_decimal().to_string(),"price_increment":instrument.price_increment().as_decimal().to_string(),
                "minimum_quantity":instrument.min_quantity().map(|v|v.as_decimal().to_string()),"minimum_notional":instrument.min_notional().map(|v|v.as_decimal().to_string()),
                "taker_fee_rate":if now<self.fee_valid_until_ns{Some(self.fee_rate.to_string())}else{None},"fee_source":self.fee_source,"fee_valid_until_ns":self.fee_valid_until_ns.to_string(),"bid":quote.bid_price.as_decimal().to_string(),"ask":quote.ask_price.as_decimal().to_string(),
                "bid_depth":depth,"asof_ns":quote.ts_init.as_u64().to_string(),"book_asof_ns":book_ns.map(|v|v.to_string())}));
        }
        let mut positions:Vec<_>=cache.positions_open(None,None,None,Some(&account),None).iter().map(|p|{
            // Public per-position PnL requires a caller-selected price. The official
            // Portfolio price/staleness selection is private and its public PnL is
            // aggregate; do not recreate that policy or duplicate an aggregate here.
            json!({"instrument_id":p.instrument_id.to_string(),"quantity":p.signed_decimal_qty().to_string(),"value":Value::Null,"pnl":Value::Null})
        }).collect();
        positions.sort_by_key(|v| v["instrument_id"].as_str().unwrap_or_default().to_owned());
        let cash = native
            .balances
            .iter()
            .find(|v| v.total.currency == Currency::USDC())
            .map(|v| v.free.as_decimal().to_string());
        let equity = native
            .total_equity
            .iter()
            .find(|v| v.currency == Currency::USDC())
            .map(|v| v.as_decimal().to_string());
        let fresh = instruments.len() == self.ids.len()
            && instruments.iter().all(|i| {
                i["asof_ns"]
                    .as_str()
                    .and_then(|s| s.parse::<u64>().ok())
                    .is_some_and(|t| t <= now && now - t <= 5_000_000_000)
            });
        self.snapshot = Some(
            json!({"ready":fresh&&!native.is_stale,"recovery_required":false,"reason":if fresh {None}else{Some("account_native_market_data_stale_or_unavailable")},
            "sequence":native.ts_init.as_u64().to_string(),"observed_at_ns":native.ts_init.as_u64().to_string(),"observation_id":Id::new().to_string(),
            "native_session_id":self.session,"native_trader_id":self.trader,"native_account_id":account.to_string(),"native_snapshot":projected,"positions":positions,"instruments":instruments,"official_node_order_activity":official_node_order_activity(&self.native_cache.borrow()),"cash":cash,"equity":equity,"currency":"USDC",
            "native_version":crate::NAUTILUS_VERSION,"currency_precision":Currency::USDC().precision,"control_epoch":self.control.as_ref().map(|c|c.epoch.to_string()),"control_revision":self.control.as_ref().map(|c|c.revision.clone()),"fenced":self.control.as_ref().is_none_or(|c|c.mode=="PAUSED")||self.cache_failed.load(Ordering::SeqCst),
            "spot_balance_observation":self.spot_balance_observation,"remaining_trading":"NATIVE_STRATEGY_EXPLICIT_LIFECYCLE"}),
        );
        Ok(())
    }
    // Explicit legacy export only. Periodic observations never enumerate order history.
    // This preserves full historical export semantics; it is not a bounded endpoint.
    fn append_order_history(&self, frame: &mut Value) {
        let account = self.account;
        let cache = self.cache();
        let orders = cache.orders(None, None, None, Some(&account), None);
        let mut fills = vec![];
        let mut order_rows:Vec<_>=orders.iter().map(|o|{
            for event in o.events() {if let OrderEventAny::Filled(f)=event {fills.push(json!({"trade_id":f.trade_id.to_string(),"event_id":f.event_id.to_string(),"client_order_id":f.client_order_id.to_string(),"instrument_id":f.instrument_id.to_string(),"side":f.order_side.to_string(),"quantity":f.last_qty.as_decimal().to_string(),"price":f.last_px.as_decimal().to_string(),"commission":f.commission.map(|v|v.as_decimal().to_string()),"commission_currency":f.commission.map(|v|v.currency.to_string()),"ts_event_ns":f.ts_event.as_u64().to_string()}));}}
            json!({"trader_id":o.trader_id().to_string(),"strategy_id":o.strategy_id().to_string(),"account_id":o.account_id().map(|v|v.to_string()),"is_active_local":o.is_active_local(),"emulation_trigger":o.emulation_trigger().map(|v|v.to_string()),"exec_algorithm_id":o.exec_algorithm_id().map(|v|v.to_string()),"client_order_id":o.client_order_id().to_string(),"venue_order_id":o.venue_order_id().map(|v|v.to_string()),"instrument_id":o.instrument_id().to_string(),"side":o.order_side().to_string(),
                "quantity":o.quantity().as_decimal().to_string(),"filled_quantity":o.filled_qty().as_decimal().to_string(),"leaves_quantity":o.leaves_qty().as_decimal().to_string(),"status":o.status().to_string(),
                "limit_price":o.price().map(|v|v.as_decimal().to_string()),"reduce_only":o.is_reduce_only(),"exit_id":exit_id(o.tags()),
                "order_type":o.order_type().to_string(),"trigger_price":o.trigger_price().map(|v|v.as_decimal().to_string()),
                "order_list_id":o.order_list_id().map(|v|v.to_string()),"parent_order_id":o.parent_order_id().map(|v|v.to_string()),
                "contingency_type":o.contingency_type().map(|v|v.to_string()),
                "protection_target_id":o.tags().and_then(|tags|tags.iter().find_map(|tag|tag.as_str().strip_prefix("QZ-PROTECTION:").map(str::to_owned)))})
        }).collect();
        order_rows.sort_by_key(|v| v["client_order_id"].as_str().unwrap_or_default().to_owned());
        fills.sort_by_key(|v| {
            (
                v["ts_event_ns"].as_str().unwrap_or_default().to_owned(),
                v["trade_id"].as_str().unwrap_or_default().to_owned(),
            )
        });
        frame["orders"] = json!(order_rows);
        frame["fills"] = json!(fills);
    }
    // Installed only by the owned runtime's known official paper path. Public
    // async/user-factory/live entry points cannot claim this node ownership.
    fn cancel_order_operation(&self, r: &Request) -> Result<Value> {
        use crate::account_queued_cancel::OriginalOrder;
        let node = self
            .cancel_node
            .as_ref()
            .ok_or_else(|| anyhow!("native_cancel_uninstalled"))?;
        let expected = r
            .body
            .get("expected_native_session_id")
            .and_then(Value::as_str)
            .ok_or_else(|| anyhow!("native_cancel_session_required"))?;
        ensure!(
            expected == self.session,
            "native_cancel_node_binding_mismatch"
        );
        let command = r
            .body
            .get("command")
            .ok_or_else(|| anyhow!("native_cancel_command_required"))?;
        ensure!(
            command["schemaVersion"] == 1
                && command["action"] == "CANCEL"
                && command["requestId"].as_str().is_some_and(|s| !s.is_empty()),
            "native_cancel_command_invalid"
        );
        let binding = &command["binding"];
        ensure!(
            binding["nativeSessionId"] == expected
                && binding["environmentId"] == "Sandbox"
                && binding["sourceId"] == self.config.source_id
                && binding["buildId"]
                    == std::env::current_exe()?
                        .canonicalize()?
                        .to_string_lossy()
                        .as_ref(),
            "native_cancel_original_binding_mismatch"
        );
        let text = |key: &str| -> Result<&str> {
            command["order"][key]
                .as_str()
                .ok_or_else(|| anyhow!("native_cancel_order_invalid"))
        };
        let original = OriginalOrder {
            trader: text("traderId")?.parse()?,
            account: text("accountId")?.parse()?,
            strategy: command["strategyId"]
                .as_str()
                .ok_or_else(|| anyhow!("native_cancel_strategy_invalid"))?
                .parse()?,
            instrument: text("instrumentId")?.parse()?,
            client_order_id: text("clientOrderId")?.parse()?,
        };
        let session = expected.parse().map_err(anyhow::Error::msg)?;
        if r.operation == "ORDER_CANCEL_ATTEMPT" {
            let id = node.dispatch_attempt(session, Environment::Sandbox, &original)?;
            Ok(json!({"command":command,"status":"UNKNOWN","attemptId":id.to_string()}))
        } else {
            let order = node.observe_order(session, Environment::Sandbox, &original)?;
            let event = order.last_event();
            let event_json = official_order_event_json(event)?;
            Ok(
                json!({"command":command,"status":"UNKNOWN","orderObservation":{
                "status":order.status().to_string(),"filledQuantity":order.filled_qty().to_string(),
                "lastEvent":event,"lastEventJson":event_json}}),
            )
        }
    }
    fn process(&mut self, r: &Request) -> Result<Value> {
        match r.operation.as_str() {
            "ORDER_CANCEL_ATTEMPT" | "ORDER_OBSERVE" => self.cancel_order_operation(r),
            #[cfg(test)]
            "TEST_RESTING_LIMIT" => {
                let instrument = self.ids[0];
                let order = self.order_factory().limit(
                    instrument,
                    OrderSide::Buy,
                    nautilus_model::types::Quantity::from("1.00"),
                    Price::from("1.00"),
                    Some(TimeInForce::Gtc),
                    None,
                    Some(false),
                    Some(false),
                    Some(false),
                    None,
                    None,
                    None,
                    None,
                    None,
                    None,
                    Some(ClientOrderId::from("fault-resting")),
                );
                self.submit_order(order, None, Some(self.client), None)?;
                Ok(json!({"sent":true}))
            }
            #[cfg(test)]
            "TEST_BRACKET" => {
                let instrument = self.ids[0];
                let orders = self
                    .order_factory()
                    .bracket()
                    .instrument_id(instrument)
                    .order_side(OrderSide::Buy)
                    .quantity(nautilus_model::types::Quantity::from("1.00"))
                    .tp_price(Price::from("110.00"))
                    .sl_trigger_price(Price::from("90.00"))
                    .call();
                self.submit_order_list(orders, None, Some(self.client), None)?;
                Ok(json!({"sent":true}))
            }
            #[cfg(test)]
            "TEST_MODIFY_LIMIT" => {
                self.modify_order(
                    ClientOrderId::from("fault-resting"),
                    None,
                    Some(Price::from("2.00")),
                    None,
                    Some(self.client),
                    None,
                )?;
                Ok(json!({"sent":true}))
            }
            #[cfg(test)]
            "TEST_CANCEL_LIMIT" => {
                self.cancel_order(
                    ClientOrderId::from("fault-resting"),
                    Some(self.client),
                    None,
                )?;
                Ok(json!({"sent":true}))
            }
            "RECOVERY_STATUS" => Ok(json!({
                "recovery_only":self.signal_strategy_id.is_none(),
                "native_version":crate::NAUTILUS_VERSION,"native_session_id":self.session,
                "ready":!self.cache_failed.load(Ordering::SeqCst),
                "recovery_required":self.cache_failed.load(Ordering::SeqCst),
                "signal_strategy_id":self.signal_strategy_id,
                "trading_state":format!("{:?}",self.native_risk.borrow().trading_state()).to_ascii_uppercase()
            })),
            "STRATEGY_STATUS" => self.strategy_status(),
            "STRATEGY_START" => {
                let prepared = (|| -> Result<_> {
                    let control = self.fence(r.text("epoch")?.parse()?)?;
                    ensure!(
                        control.mode == "NORMAL"
                            && control.reserved == Decimal::ZERO
                            && control.buffer == Decimal::ZERO
                            && control.exits.is_empty(),
                        "account_native_strategy_capital_reserved"
                    );
                    ensure!(
                        !self.cache_failed.load(Ordering::SeqCst),
                        "account_native_cache_write_failed"
                    );
                    if let Some(error) = &self.signal_restore_error {
                        bail!("{error}");
                    }
                    if self.signal_strategy_id.is_none() {
                        self.register_signal()?;
                    }
                    let id = self
                        .signal_strategy_id
                        .ok_or_else(|| anyhow!("account_native_strategy_not_configured"))?;
                    let state = component_state(&id.inner())?;
                    Ok((id, state))
                })();
                let (id, state) = match prepared {
                    Ok(value) => value,
                    Err(error) => {
                        return Ok(
                            json!({"admission":"REJECTED","started":false,"reason":error.to_string()}),
                        );
                    }
                };
                if state == ComponentState::Stopped {
                    reset_component(&id.inner())?;
                }
                if state != ComponentState::Running {
                    self.native_risk
                        .borrow_mut()
                        .set_trading_state(nautilus_model::enums::TradingState::Active);
                    if let Err(error) =
                        nautilus_system::Controller::new(self.native_trader.clone(), None)
                            .start_strategy(&id)
                    {
                        self.native_risk
                            .borrow_mut()
                            .set_trading_state(nautilus_model::enums::TradingState::Halted);
                        return Err(error);
                    }
                }
                self.strategy_status()
            }
            "STRATEGY_STOP" => {
                self.stop_signal_strategy()?;
                self.strategy_status()
            }

            "FIXED_RISK_SIZE" => {
                let id = InstrumentId::from_str(r.text("instrument_id")?)?;
                ensure!(self.ids.contains(&id), "account_native_instrument_scope");
                let instrument = self
                    .cache()
                    .instrument(&id)
                    .ok_or_else(|| anyhow!("account_native_instrument_unavailable"))?;
                let risk = r.decimal("risk")?;
                ensure!(
                    risk > Decimal::ZERO && risk <= Decimal::ONE,
                    "account_native_risk_fraction"
                );
                let quantity = nautilus_risk::sizing::calculate_fixed_risk_position_size(
                    &instrument,
                    Price::from_str(r.text("entry")?).map_err(anyhow::Error::msg)?,
                    Price::from_str(r.text("stop_loss")?).map_err(anyhow::Error::msg)?,
                    Money::from_decimal_exact(
                        Decimal::from_str_exact(r.text("equity")?)?,
                        Currency::from_str(r.text("currency")?)?,
                    )?,
                    risk,
                    r.decimal("commission_rate")?,
                    r.decimal("exchange_rate")?,
                    if r.value("hard_limit").is_null() {
                        None
                    } else {
                        Some(r.decimal("hard_limit")?)
                    },
                    r.decimal("unit_batch_size")?,
                    1,
                )?;
                Ok(
                    json!({"instrument_id":id.to_string(),"quantity":quantity.to_string(),"model":"NAUTILUS_FIXED_RISK","preview_only":true,"input_source":"REQUEST_SCENARIO","account_balance_verified":false}),
                )
            }
            "ORDER_PREVIEW" => {
                let id = InstrumentId::from_str(r.text("instrument_id")?)?;
                ensure!(
                    self.ids.contains(&id),
                    "account_native_instrument_not_configured"
                );
                let request = serde_json::from_value(r.value("order").clone())?;
                let cache = self.cache();
                let instrument = cache
                    .instrument(&id)
                    .ok_or_else(|| anyhow!("account_native_instrument_unavailable"))?;
                crate::account_order_preview::preview(&instrument, request, &self.config.mode)
            }
            "QUANTIZE_MONEY" => {
                ensure!(r.text("currency")? == "USDC", "account_native_currency");
                let money = Money::from_decimal(r.decimal("amount")?, Currency::USDC())?;
                Ok(
                    json!({"amount":money.as_decimal().to_string(),"precision":money.currency.precision}),
                )
            }
            "ORDER_PAGE" => {
                ensure!(r.text("expected_native_session_id")? == self.session, "native_order_session_changed");
                let offset = r.value("offset").as_u64().ok_or_else(|| anyhow!("native_order_offset_invalid"))?;
                let offset = usize::try_from(offset)?;
                let cache = self.native_cache.borrow();
                // Official lazy ID iterator: at most offset + 51 matching IDs yielded, only
                // 50 borrowed orders projected, never clone orders or event histories.
                // Cache iteration is live, not a stable historical snapshot.
                let mut ids = cache.iter_client_order_ids(None, None, None, Some(&self.account)).skip(offset);
                let mut items = Vec::with_capacity(50);
                for id in ids.by_ref().take(50) {
                    let o = cache.order_ref(&id).ok_or_else(|| anyhow!("native_order_missing"))?;
                    items.push(json!({"trader_id":o.trader_id().to_string(),"strategy_id":o.strategy_id().to_string(),"account_id":o.account_id().map(|v|v.to_string()),
                        "client_order_id":o.client_order_id().to_string(),"venue_order_id":o.venue_order_id().map(|v|v.to_string()),"instrument_id":o.instrument_id().to_string(),
                        "status":o.status().to_string(),"quantity":o.quantity().as_decimal().to_string(),"filled_quantity":o.filled_qty().as_decimal().to_string(),
                        "is_active_local":o.is_active_local(),"emulation_trigger":o.emulation_trigger().map(|v|v.to_string()),"exec_algorithm_id":o.exec_algorithm_id().map(|v|v.to_string())}));
                }
                let next = if ids.next().is_some() {
                    Some(
                        offset
                            .checked_add(50)
                            .ok_or_else(|| anyhow!("native_order_offset_invalid"))?,
                    )
                } else {
                    None
                };
                Ok(
                    json!({"native_session_id":self.session,"observed_at_ns":self.clock().timestamp_ns().as_u64().to_string(),"items":items,"offset":offset,"nextOffset":next,"total":Value::Null}),
                )
            }
            "ORDER_STATUS" => {
                ensure!(
                    r.text("expected_native_session_id")? == self.session,
                    "native_order_session_changed"
                );
                let id: ClientOrderId = r.text("client_order_id")?.parse()?;
                let cache = self.native_cache.borrow();
                let order = cache.order_ref(&id);
                if let Some(ref order) = order {
                    ensure!(
                        order.account_id() == Some(self.account)
                            && order.trader_id() == self.trader.parse()?,
                        "native_order_identity_mismatch"
                    );
                }
                Ok(
                    json!({"native_session_id":self.session,"account_id":self.account.to_string(),"client_order_id":id.to_string(),"status":order.map(|o|o.status().to_string())}),
                )
            }
            "ACCOUNT_SUMMARY" | "SNAPSHOT" => {
                self.refresh_snapshot()?;
                let mut frame = self
                    .snapshot
                    .clone()
                    .ok_or_else(|| anyhow!("account_native_snapshot_unavailable"))?;
                let now = self.clock().timestamp_ns().as_u64();
                if frame["instruments"].as_array().is_some_and(|a| {
                    a.iter().any(|i| {
                        i["asof_ns"]
                            .as_str()
                            .and_then(|s| s.parse::<u64>().ok())
                            .is_none_or(|n| n > now || now - n > 5_000_000_000)
                    })
                }) {
                    frame["ready"] = json!(false);
                    frame["reason"] = json!("account_native_market_data_stale_or_unavailable");
                }
                if now >= self.fee_valid_until_ns {
                    if let Some(rows) = frame["instruments"].as_array_mut() {
                        for row in rows {
                            row["taker_fee_rate"] = Value::Null;
                        }
                    }
                }
                if self.cache_failed.load(Ordering::SeqCst) {
                    frame["ready"] = json!(false);
                    frame["recovery_required"] = json!(true);
                    frame["fenced"] = json!(true);
                    frame["reason"] = json!("account_native_cache_write_failed");
                }
                if r.operation == "SNAPSHOT" {
                    self.append_order_history(&mut frame);
                }
                Ok(frame)
            }
            "SPOT_BALANCE_REFRESH" => {
                let observation = r.value("observation");
                ensure!(
                    self.config.mode == "live"
                        && observation["account_id"].as_str()
                            == Some(self.config.account_id.as_str())
                        && observation["source_id"].as_str()
                            == Some(self.config.source_id.as_str())
                        && observation["account_address"].as_str()
                            == self.config.account_address.as_deref()
                        && observation["network"].as_str()
                            == Some(if self.config.testnet {
                                "testnet"
                            } else {
                                "mainnet"
                            }),
                    "account_native_spot_observation_binding_mismatch"
                );
                self.spot_balance_observation = Some(observation.clone());
                Ok(json!({"updated":true,"trade_permission":false}))
            }
            "FEE_REFRESH" => {
                let rate = r.decimal("rate")?;
                ensure!(rate >= Decimal::ZERO, "account_native_fee_positive");
                self.fee_rate = rate;
                self.fee_source = r.value("source").clone();
                let asof = r.text("asof_ns")?.parse::<u64>()?;
                self.fee_valid_until_ns = asof.saturating_add(60_000_000_000);
                Ok(json!({"updated":true}))
            }
            "CONTROL" => self.apply_control(r),
            "SLICE" => Ok(
                json!({"admission":"REJECTED","submitted":false,"reason":"capital_exit_planning_retired"}),
            ),
            "TARGET" => bail!("account_native_target_execution_retired"),
            "TARGET_STATUS" => self.target_status(r),
            "CANCEL_EXIT" => self.cancel_exit(r),
            "CANCEL_PROTECTION" => self.cancel_protection(r),
            "SHUTDOWN" => {
                self.control = None;
                self.handle.stop();
                Ok(json!({"stopping":true}))
            }
            _ => bail!("account_native_operation_unsupported"),
        }
    }
    // Reports installed dispatch paths only; never invokes or certifies owner code.
    fn strategy_lifecycle(&self) -> Value {
        let (kind, supported) = match &self.config.native_strategy.strategy {
            NativeCompiledStrategyV1::EmaCross { .. } => ("EMA_CROSS", true),
            NativeCompiledStrategyV1::Registered { .. } => {
                ("REGISTERED", self.user_registration.is_some())
            }
            _ => ("UNSUPPORTED", false),
        };
        json!({"configuredKind":kind,"supported":supported,
            "reason":if supported {None}else{Some("account_native_strategy_registration_unavailable")},
            "scope":"INSTALLED_DISPATCH_ONLY","strategyVerified":false})
    }
    fn strategy_status(&self) -> Result<Value> {
        let Some(id) = self.signal_strategy_id else {
            return Ok(
                json!({"strategy_id":null,"state":"UNREGISTERED","recovery_only":true,
                "native_version":crate::NAUTILUS_VERSION,"instrument_id":self.config.native_strategy.instrument_id,
                "config":self.config.native_strategy.strategy,"default_start":"UNREGISTERED"}),
            );
        };
        Ok(
            json!({"strategy_id":id.to_string(),"recovery_only":false,"state":format!("{:?}",component_state(&id.inner())?).to_ascii_uppercase(),
            "native_version":crate::NAUTILUS_VERSION,"instrument_id":self.config.native_strategy.instrument_id,
            "config":self.config.native_strategy.strategy,"default_start":"PAUSED",
            "restart_warmup":if matches!(self.config.native_strategy.strategy, NativeCompiledStrategyV1::Registered { .. }) { "UPSTREAM_STRATEGY_DEFINED" } else { "OFFICIAL_STRATEGY_REINITIALIZES_INDICATORS" },"start_semantics":"UPSTREAM_RESET_THEN_START"}),
        )
    }
    fn stop_signal_strategy(&self) -> Result<()> {
        if let Some(id) = self.signal_strategy_id {
            if component_state(&id.inner())? == ComponentState::Running {
                self.native_trader.borrow_mut().stop_strategy(&id)?;
            }
        }
        Ok(())
    }
    fn apply_control(&mut self, r: &Request) -> Result<Value> {
        let epoch = r.text("epoch")?.parse::<u64>()?;
        let reserved = r.decimal("reserved_amount")?;
        let buffer = r.decimal("cash_buffer")?;
        ensure!(
            reserved >= Decimal::ZERO && buffer >= Decimal::ZERO,
            "account_native_negative_budget"
        );
        let mode = r.text("mode")?.to_owned();
        ensure!(
            [
                "NORMAL",
                "CASH_ONLY",
                "NATURAL_RELEASE",
                "BOUNDED_LIMIT",
                "PAUSED"
            ]
            .contains(&mode.as_str()),
            "account_native_control_mode"
        );
        let exits = if let Some(values) = r.value("exit_ids").as_array() {
            values
                .iter()
                .map(|v| {
                    v.as_str()
                        .map(str::to_owned)
                        .ok_or_else(|| anyhow!("account_native_exit_ids"))
                })
                .collect::<Result<Vec<_>>>()?
        } else {
            r.value("exit_id")
                .as_str()
                .map(|s| vec![s.to_owned()])
                .unwrap_or_default()
        };
        let deadline = r.value("deadline").as_str().map(deadline_ns).transpose()?;
        let minimum_interval_ns = interval_ns(r.value("minimum_interval_seconds"))?;
        let spot_candidate = (|| -> Result<Option<SpotFacts>> {
            if self.config.mode != "live" || r.value("spot_free_cash").is_null() {
                return Ok(None);
            }
            let free = r.decimal("spot_free_cash")?;
            let total = r.decimal("spot_total_cash")?;
            let asof = r.text("spot_facts_asof_ns")?.parse::<u64>()?;
            let valid_until = r.text("spot_facts_valid_until_ns")?.parse::<u64>()?;
            ensure!(
                free >= Decimal::ZERO
                    && total >= free
                    && asof <= self.clock().timestamp_ns().as_u64()
                    && valid_until > asof
                    && self.spot_facts.as_ref().is_none_or(|old| asof >= old.asof),
                "account_native_spot_facts_invalid"
            );
            Ok(Some(SpotFacts {
                free,
                total,
                balances: r.value("spot_balances").clone(),
                asof,
                valid_until,
            }))
        })();
        // Stopping remains available without valid financial observations.
        let spot_candidate = if mode == "PAUSED" {
            spot_candidate
                .ok()
                .flatten()
                .filter(|facts| self.clock().timestamp_ns().as_u64() < facts.valid_until)
        } else {
            spot_candidate?
        };
        if let Some(old) = &self.control {
            ensure!(epoch >= old.epoch, "account_native_obsolete_control_epoch");
            if epoch == old.epoch {
                ensure!(
                    old.revision == r.text("revision")?
                        && old.reserved == reserved
                        && old.buffer == buffer
                        && old.mode == mode
                        && old.exits == exits
                        && old.deadline == deadline
                        && old.minimum_interval_ns == minimum_interval_ns,
                    "account_native_control_replay_conflict"
                );
                let revision = old.revision.clone();
                if self.update_spot_observation(spot_candidate) {
                    self.refresh_snapshot()?;
                }
                return Ok(
                    json!({"applied":true,"epoch":epoch.to_string(),"revision":revision,"remaining_trading":"NATIVE_STRATEGY_EXPLICIT_LIFECYCLE"}),
                );
            }
        }
        if self.native_managed_stop
            && self.signal_strategy_id.is_some_and(|id| {
                component_state(&id.inner()).ok() == Some(ComponentState::Running)
            })
        {
            bail!("account_native_managed_stop_requires_explicit_stop");
        }
        self.stop_signal_strategy()?;
        ensure!(self.signal_strategy_id.is_none_or(|id|component_state(&id.inner()).ok()!=Some(ComponentState::Running)),"account_native_strategy_stop_unsettled");
        // This synchronous owner-thread fence is visible before cancel commands
        // enter the native queue; old timer callbacks cannot pass submit checks.
        self.control = Some(Control {
            epoch,
            revision: r.text("revision")?.into(),
            reserved,
            buffer,
            mode,
            exits,
            deadline,
            minimum_interval_ns,
        });
        self.update_spot_observation(spot_candidate);
        let latest_exit_ns = {
            let cache = self.native_cache.borrow();
            cache.iter_client_order_ids(None, None, None, Some(&self.account))
                .map(|id| required_order_ref(&cache, &id))
                .filter(|o| exit_id(o.tags()).is_some())
                .map(|o| o.ts_submitted().unwrap_or(o.ts_init()).as_u64())
                .max()
                .unwrap_or(0)
        };
        self.last_slice_ns = self.last_slice_ns.max(latest_exit_ns);
        let mut pending: Vec<_> = {
            let cache = self.native_cache.borrow();
            cache.iter_client_order_ids(None, None, None, Some(&self.account))
                .map(|id| required_order_ref(&cache, &id))
                .filter(|o| !o.is_closed() && o.order_side() == OrderSide::Buy)
                .map(|o| o.client_order_id())
                .collect()
        };
        pending.sort();
        for id in pending {
            self.cancel_order(id, Some(self.client), None)?;
        }
        self.refresh_snapshot()?;
        Ok(
            json!({"applied":true,"epoch":epoch.to_string(),"revision":r.text("revision")?,"remaining_trading":"PAUSED_NATIVE_STRATEGY","fenced_old_targets":true}),
        )
    }
    fn update_spot_observation(&mut self, candidate: Option<SpotFacts>) -> bool {
        let Some(facts) = candidate else {
            return false;
        };
        let changed = self.spot_facts.as_ref().is_none_or(|old| {
            old.free != facts.free || old.total != facts.total || old.balances != facts.balances
        });
        self.spot_facts = Some(facts);
        changed
    }
    fn cancel_exit(&mut self, r: &Request) -> Result<Value> {
        let epoch = r.text("epoch")?.parse::<u64>()?;
        let c = self
            .control
            .as_ref()
            .ok_or_else(|| anyhow!("account_native_budget_fence_not_applied"))?;
        ensure!(c.epoch == epoch, "account_native_obsolete_control_epoch");
        let exit = r.text("exit_id")?;
        let mut ids: Vec<_> = {
            let cache = self.native_cache.borrow();
            cache.iter_client_order_ids(None, None, None, Some(&self.account))
                .map(|id| required_order_ref(&cache, &id))
                .filter(|o| !o.is_closed() && exit_id(o.tags()).as_deref() == Some(exit))
                .map(|o| o.client_order_id())
                .collect()
        };
        ids.sort();
        for id in &ids {
            self.cancel_order(*id, Some(self.client), None)?;
        }
        Ok(json!({"cancellation_requested":ids.len(),"settled":ids.is_empty()}))
    }
    fn protection_orders(&self, target: Option<&str>) -> Vec<ClientOrderId> {
        let cache = self.native_cache.borrow();
        let mut ids: Vec<_> = cache.iter_client_order_ids(None, None, None, None)
            .map(|id| required_order_ref(&cache, &id))
            .filter(|o| {
                !o.is_closed()
                    && o.tags().is_some_and(|tags| {
                        tags.iter().any(|tag| {
                            tag.as_str()
                                .strip_prefix("QZ-PROTECTION:")
                                .is_some_and(|id| target.is_none_or(|target| target == id))
                        })
                    })
            })
            .map(|o| o.client_order_id())
            .collect();
        ids.sort();
        ids
    }
    fn cancel_protection(&mut self, r: &Request) -> Result<Value> {
        let target = r.text("id")?;
        ensure!(
            self.target_acceptances.contains_key(target),
            "account_native_target_unknown"
        );
        let ids = self.protection_orders(Some(target));
        for id in &ids {
            self.cancel_order(*id, Some(self.client), None)?;
        }
        Ok(json!({"target_id":target,"cancellation_requested":ids.len(),"settled":ids.is_empty()}))
    }
    fn persist_acceptances(&self) -> Result<()> {
        let id = self
            .strategy_id()
            .ok_or_else(|| anyhow!("account_native_strategy_not_registered"))?;
        nautilus_common::actor::DataActorNative::cache_ref(self)
            .update_strategy_state(&id, &DataActor::on_save(self)?)?;
        ensure!(
            !self.cache_failed.load(Ordering::SeqCst),
            "account_native_cache_write_failed"
        );
        Ok(())
    }
    fn finish_unsubmitted_target(&mut self, id: &str, reason: String) -> Result<Value> {
        let now = self.clock().timestamp_ns().as_u64();
        let saved = self
            .target_acceptances
            .get_mut(id)
            .ok_or_else(|| anyhow!("account_native_target_not_observed"))?;
        ensure!(
            saved.submitted.is_empty(),
            "account_native_original_target_requires_reconciliation"
        );
        if let Some(receipt) = &saved.receipt {
            return Ok(receipt.clone());
        }
        let expires = saved
            .original
            .get("expires_at")
            .and_then(Value::as_str)
            .and_then(|v| v.parse::<u64>().ok());
        let state = if expires.is_some_and(|expiry| now >= expiry) {
            "EXPIRED"
        } else {
            "REJECTED"
        };
        let receipt = json!({"acknowledged":false,"target_id":id,"epoch":if saved.epoch.is_empty(){None}else{Some(saved.epoch.as_str())},"state":state,"admission":"NOT_SUBMITTED","submitted_client_order_ids":[],"observed_at_ns":now.to_string(),"expires_at_ns":expires.map(|v|v.to_string()),"reason":reason,"recovery":"OFFICIAL_CACHE_AND_RECONCILIATION"});
        saved.receipt = Some(receipt.clone());
        self.persist_acceptances()?;
        Ok(receipt)
    }
    fn target_status(&mut self, r: &Request) -> Result<Value> {
        let id = r.text("id")?.to_owned();
        if let Some(saved) = self.target_acceptances.get(&id) {
            if let Some(receipt) = &saved.receipt {
                return Ok(receipt.clone());
            }
            if !saved.submitted.is_empty() {
                return Ok(
                    json!({"acknowledged":false,"target_id":id,"epoch":saved.epoch,"state":"PENDING","submitted_client_order_ids":saved.submitted,"admission":"UNCERTAIN","observed_at_ns":self.clock().timestamp_ns().as_u64().to_string()}),
                );
            }
        } else {
            return Ok(
                json!({"acknowledged":false,"target_id":id,"state":"UNKNOWN","admission":"UNCERTAIN",
                "reason":"legacy_target_owner_state_not_loaded","submitted_client_order_ids":[]}),
            );
        }
        self.finish_unsubmitted_target(&id, "account_native_target_not_submitted".into())
    }
}
// Preserve the official Cache::orders failure for an inconsistent order index.
fn required_order_ref<'a>(cache: &'a Cache, id: &ClientOrderId) -> nautilus_common::cache::OrderRef<'a> {
    cache.order_ref(id).unwrap_or_else(|| panic!("Order {id} not found"))
}

fn exit_id(tags: Option<&[Ustr]>) -> Option<String> {
    tags?.iter().find_map(|t| {
        t.as_str()
            .strip_prefix("QZ-CAPITAL-EXIT:")
            .map(str::to_owned)
    })
}
fn deadline_ns(value: &str) -> Result<u64> {
    if let Ok(n) = value.parse::<u64>() {
        return Ok(n);
    }
    chrono::DateTime::parse_from_rfc3339(value)?
        .timestamp_nanos_opt()
        .and_then(|n| u64::try_from(n).ok())
        .ok_or_else(|| anyhow!("account_native_deadline_range"))
}
fn interval_ns(value: &Value) -> Result<u64> {
    if value.is_null() {
        return Ok(5_000_000_000);
    }
    let seconds = value
        .as_u64()
        .ok_or_else(|| anyhow!("account_native_interval_seconds"))?;
    ensure!(seconds > 0, "account_native_interval_positive");
    seconds
        .checked_mul(1_000_000_000)
        .ok_or_else(|| anyhow!("account_native_interval_range"))
}
// Keep the official risk engine and queued dispatch. Cache persistence failures
// are otherwise log-only upstream; halt at the execution boundary, not a timer.
fn install_cache_failure_risk_fence(node: &LiveNode, failed: Arc<AtomicBool>) {
    use nautilus_common::{
        messages::execution::TradingCommand,
        msgbus::{self, MessagingSwitchboard, TypedIntoHandler},
    };
    let risk = std::rc::Rc::downgrade(node.kernel().risk_engine());
    msgbus::register_trading_command_endpoint(
        MessagingSwitchboard::risk_engine_execute(),
        TypedIntoHandler::from(move |command: TradingCommand| {
            if let Some(risk) = risk.upgrade() {
                let mut risk = risk.borrow_mut();
                if failed.load(Ordering::SeqCst) {
                    risk.set_trading_state(nautilus_model::enums::TradingState::Halted);
                }
                risk.execute(command);
            }
        }),
    );
}

// The upstream emulator reactivates persisted emulated orders on every node start.
// Recovery is therefore supported only after the official, exclusively locked cache
// is loaded and verified to contain no emulated orders. Never filter or delete them.
async fn prepare_recovery_cache(node: &LiveNode) -> Result<()> {
    ensure!(
        node.kernel().cache().borrow().has_backing(),
        "account_native_recovery_cache_backing_required"
    );
    node.kernel()
        .exec_engine()
        .borrow_mut()
        .load_cache()
        .await
        .context("account_native_recovery_cache_load_failed")?;
    let cache = node.kernel().cache();
    let mut cache = cache.borrow_mut();
    ensure!(
        cache.check_integrity(),
        "account_native_recovery_cache_integrity_failed"
    );
    ensure!(
        cache.orders_emulated_count(None, None, None, None, None) == 0,
        "account_native_recovery_emulated_orders_require_explicit_handling"
    );
    Ok(())
}

pub async fn run() -> Result<()> {
    run_with_registration(None).await
}

/// Entry point for a statically linked user account runner. BOOT only recovers;
/// it never invokes this callback or starts a user strategy.
pub async fn run_with_registration(
    registration: Option<AccountStrategyRegistration>,
) -> Result<()> {
    // External callers may drive several nodes on one thread. No ownership claim.
    run_input(
        registration,
        false,
        crate::native_runner::AccountCompatibility::Unspecified,
    )
    .await
}
pub(crate) async fn run_owned_registration(
    registration: Option<AccountStrategyRegistration>,
    _owner: &crate::native_runner::OwnedAccountRuntime,
    compatibility: crate::native_runner::AccountCompatibility,
) -> Result<()> {
    run_input(registration, true, compatibility).await
}
async fn run_input(
    registration: Option<AccountStrategyRegistration>,
    owned_runtime: bool,
    compatibility: crate::native_runner::AccountCompatibility,
) -> Result<()> {
    let mut first = String::new();
    io::stdin().read_line(&mut first)?;
    let boot: Request = serde_json::from_str(&first)?;
    ensure!(boot.operation == "BOOT", "account_native_boot_required");
    let config: AccountRuntimeConfig = serde_json::from_value(boot.value("config").clone())?;
    let result = run_config_owned(
        config,
        boot.id.clone(),
        registration,
        owned_runtime,
        compatibility,
    )
    .await;
    if let Err(e) = &result {
        // Full causal diagnostics stay on the private process stderr channel.
        // The public protocol keeps its existing stable top-level error.
        eprintln!("account native diagnostic: {e:#}");
        respond(&boot.id, Err(anyhow!(e.to_string())));
    }
    result
}
fn execution_product_types(config: &AccountRuntimeConfig) -> Result<Vec<HyperliquidProductType>> {
    let products = config
        .execution_product_types
        .clone()
        .unwrap_or_else(|| HyperliquidExecutionClientConfig::default().product_types);
    ensure!(
        !products.is_empty(),
        "account_native_execution_product_types_empty"
    );
    for instrument in &config.instruments {
        let id = InstrumentId::from_str(instrument)?;
        let product = HyperliquidProductType::from_symbol(id.symbol.as_str())?;
        ensure!(
            products.contains(&product),
            "account_native_instrument_outside_execution_scope"
        );
    }
    Ok(products)
}

#[cfg(test)]
async fn run_config(config: AccountRuntimeConfig, boot_id: String) -> Result<()> {
    run_config_with_registration(config, boot_id, None).await
}
#[cfg(test)]
async fn run_config_with_registration(
    config: AccountRuntimeConfig,
    boot_id: String,
    registration: Option<AccountStrategyRegistration>,
) -> Result<()> {
    run_config_owned(
        config,
        boot_id,
        registration,
        false,
        crate::native_runner::AccountCompatibility::Unspecified,
    )
    .await
}
fn ordinary_cancel_supported(
    owned_runtime: bool,
    mode: &str,
    has_registration: bool,
    strategy: &NativeCompiledStrategyV1,
) -> bool {
    owned_runtime
        && mode == "paper"
        && !has_registration
        && matches!(strategy, NativeCompiledStrategyV1::EmaCross { .. })
}
async fn run_config_owned(
    config: AccountRuntimeConfig,
    boot_id: String,
    registration: Option<AccountStrategyRegistration>,
    owned_runtime: bool,
    compatibility: crate::native_runner::AccountCompatibility,
) -> Result<()> {
    ensure!(
        ["paper", "live"].contains(&config.mode.as_str()),
        "account_native_mode"
    );
    ensure!(
        !config.instruments.is_empty() && config.instruments.len() <= 256,
        "account_native_instrument_count"
    );
    ensure!(
        config.mode == "paper" || config.paper_execution.is_none(),
        "account_native_paper_execution_requires_paper"
    );
    ensure!(
        !matches!(
            config.native_strategy.strategy,
            NativeCompiledStrategyV1::HurstVpinDirectional { .. }
        ),
        "account_native_strategy_account_lifecycle_unsupported_for_hurst_vpin_directional"
    );
    ensure!(
        !matches!(
            config.native_strategy.strategy,
            NativeCompiledStrategyV1::GridMarketMaker { .. }
        ),
        "account_native_strategy_account_lifecycle_unsupported_for_grid_market_maker"
    );
    ensure!(
        matches!(
            config.native_strategy.strategy,
            NativeCompiledStrategyV1::EmaCross { .. } | NativeCompiledStrategyV1::Registered { .. }
        ),
        "account_native_strategy_account_lifecycle_unsupported_for_orderbook_imbalance"
    );
    let _: Id = config
        .source_id
        .clone()
        .try_into()
        .map_err(anyhow::Error::msg)?;
    ensure!(
        config.data_dir.is_absolute(),
        "account_native_data_directory_absolute"
    );
    let ids: Vec<_> = config
        .instruments
        .iter()
        .map(|s| InstrumentId::from_str(s))
        .collect::<std::result::Result<_, _>>()?;
    ensure!(
        ids.iter().all(|id| id.venue == Venue::from("HYPERLIQUID")),
        "account_native_hyperliquid_scope"
    );
    let execution_product_types = execution_product_types(&config)?;
    let environment = if config.testnet {
        HyperliquidEnvironment::Testnet
    } else {
        HyperliquidEnvironment::Mainnet
    };
    let fee_source;
    let fee_rate = if config.mode == "paper" {
        let rate = Decimal::from_str(
            config
                .paper_fee_rate
                .as_deref()
                .ok_or_else(|| anyhow!("account_native_paper_fee_required"))?,
        )?;
        ensure!(rate >= Decimal::ZERO, "account_native_fee_positive");
        fee_source = json!({"kind":"EXPLICIT_PAPER_FEE_SCENARIO","rate":rate.to_string()});
        rate
    } else {
        let address = config
            .account_address
            .as_deref()
            .ok_or_else(|| anyhow!("account_native_master_address_required"))?;
        let http = HyperliquidHttpClient::new(environment, 30, None)?;
        let fees = http.info_user_fees(address).await?;
        let rate = Decimal::from_str(
            fees["userSpotCrossRate"]
                .as_str()
                .ok_or_else(|| anyhow!("account_native_spot_fee_evidence_unavailable"))?,
        )?;
        fee_source =
            json!({"kind":"HYPERLIQUID_USER_FEES","account_address":address,"response":fees});
        rate
    };
    let account = AccountId::from_str(&config.account_id)?;
    let trader = TraderId::from_str(config.trader_id.as_deref().unwrap_or("QZ-ACCOUNT-001"))?;
    let strategy_id = StrategyId::from("QZ-ACCOUNT-OWNER-001");
    let client = if config.mode == "paper" {
        ClientId::from("QZ-PAPER")
    } else {
        *HYPERLIQUID_CLIENT_ID
    };
    let cache_factory = AccountCacheFactory::new(config.data_dir.clone());
    let cache_failed = cache_factory.failed.clone();
    let cache_config = CacheConfig {
        flush_on_start: false,
        save_market_data: false,
        ..Default::default()
    };
    let mut builder = LiveNode::builder(
        trader,
        if config.mode == "paper" {
            Environment::Sandbox
        } else {
            Environment::Live
        },
    )?
    .with_logging(LoggerConfig {
        bypass_logging: true,
        ..Default::default()
    })
    .with_load_state(true)
    .with_save_state(true)
    .with_cache_config(cache_config.clone())
    .with_risk_engine_config(config.risk.native(&config.instruments)?)
    .with_reconciliation(true)
    .with_exec_engine_config(LiveExecutionEngineConfig {
        load_cache: false,
        submission_recovery_policy: SubmissionRecoveryPolicy::RetainUnresolved,
        open_check_interval_secs: Some(10.0),
        position_check_interval_secs: Some(30.0),
        ..Default::default()
    })
    .with_portfolio_config(PortfolioConfig {
        snapshot_interval_ms: Some(1000),
        ..Default::default()
    })
    .with_delay_post_stop_secs(0)
    .with_delay_shutdown_secs(0)
    .add_data_client(
        Some("HYPERLIQUID".into()),
        Box::new(HyperliquidDataClientFactory::new()),
        Box::new(HyperliquidDataClientConfig {
            environment,
            ..Default::default()
        }),
    )?;
    if config.mode == "paper" {
        let initial = Decimal::from_str(
            config
                .starting_balance
                .as_deref()
                .ok_or_else(|| anyhow!("account_native_paper_starting_balance_required"))?,
        )?;
        ensure!(
            initial > Decimal::ZERO,
            "account_native_initial_cash_positive"
        );
        let balances = cache_factory
            .restored_balances(&account)?
            .unwrap_or(vec![Money::from_decimal(initial, Currency::USDC())?]);
        let models = config
            .paper_execution
            .as_ref()
            .map(PaperExecutionConfig::models)
            .transpose()?;
        let (fill_model, latency_model) = models
            .map(|(fill, latency)| (Some(fill), Some(latency)))
            .unwrap_or_default();
        builder = builder.add_simulated_exec_client(
            Some("QZ-PAPER".into()),
            Box::new(SandboxExecutionClientFactory::new()),
            Box::new(SandboxExecutionClientConfig {
                account_id: account,
                venue: Venue::from("HYPERLIQUID"),
                starting_balances: balances,
                base_currency: Some(Currency::USDC()),
                account_type: AccountType::Cash,
                fee_model: Some(FeeModelAny::MakerTaker(MakerTakerFeeModel::new(
                    fee_rate, fee_rate,
                ))),
                fill_model,
                latency_model,
                queue_position: config
                    .paper_execution
                    .as_ref()
                    .is_some_and(|scenario| scenario.queue_position),
                liquidity_consumption: true,
                ..Default::default()
            }),
        )?;
    } else {
        builder = builder.add_exec_client(
            Some("HYPERLIQUID".into()),
            Box::new(HyperliquidExecutionClientFactory::new()),
            Box::new(HyperliquidExecutionClientConfig {
                product_types: execution_product_types,
                account_id: account,
                account_address: config.account_address.clone(),
                environment,
                normalize_prices: false,
                include_builder_attribution: false,
                ..Default::default()
            }),
        )?;
    }
    let mut node = builder.build()?;
    cache_factory
        .install(
            node.kernel().cache(),
            trader,
            node.instance_id(),
            cache_config,
        )
        .await?;
    prepare_recovery_cache(&node).await?;
    install_cache_failure_risk_fence(&node, cache_failed.clone());
    let handle = node.handle();
    let session = node.instance_id().to_string();
    let (sender, inbox) = mpsc::sync_channel::<Request>(64);
    if config.mode == "live" {
        let address = config.account_address.clone().unwrap();
        let updates = sender.clone();
        let spot_binding = crate::account_spot_observation::SpotObservationBinding {
            account_id: config.account_id.clone(),
            source_id: config.source_id.clone(),
            address: address.clone(),
            network: if config.testnet { "testnet" } else { "mainnet" },
        };
        tokio::spawn(async move {
            let Ok(http) = HyperliquidHttpClient::new(environment, 20, None) else {
                return;
            };
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let (spot, fees) = tokio::join!(
                    crate::account_spot_observation::read(&http, &spot_binding),
                    http.info_user_fees(&address)
                );
                if let Ok(observation) = spot {
                    let _ = updates.try_send(Request {
                        id: String::new(),
                        operation: "SPOT_BALANCE_REFRESH".into(),
                        body: BTreeMap::from([("observation".into(), observation)]),
                    });
                }
                let Ok(source) = fees else {
                    continue;
                };
                let Some(rate) = source["userSpotCrossRate"].as_str() else {
                    continue;
                };
                let asof = chrono::Utc::now()
                    .timestamp_nanos_opt()
                    .unwrap_or(0)
                    .to_string();
                let _ = updates.try_send(Request {
                    id: String::new(),
                    operation: "FEE_REFRESH".into(),
                    body: BTreeMap::from([
                        ("rate".into(), json!(rate)),
                        ("source".into(), source),
                        ("asof_ns".into(), json!(asof)),
                    ]),
                });
            }
        });
    }
    let closed = handle.clone();
    std::thread::Builder::new()
        .name("qz-account-native-input".into())
        .spawn(move || {
            for line in io::stdin().lock().lines() {
                match line {
                    Ok(line) => match serde_json::from_str::<Request>(&line) {
                        Ok(r) => {
                            if let Err(error) = r.ensure_external() {
                                respond(&r.id, Err(error));
                                continue;
                            }
                            if sender.send(r).is_err() {
                                break;
                            }
                        }
                        Err(_) => respond("", Err(anyhow!("account_native_request_invalid"))),
                    },
                    Err(_) => break,
                }
            }
            closed.stop();
        })?;
    let fee_valid_until_ns = if config.mode == "paper" {
        u64::MAX
    } else {
        u64::try_from(
            chrono::Utc::now()
                .timestamp_nanos_opt()
                .ok_or_else(|| anyhow!("account_native_clock_range"))?,
        )?
        .saturating_add(60_000_000_000)
    };
    // No signal instance is registered during recovery. Its persisted state and
    // original order ownership stay in the official cache until explicit START.
    node.kernel()
        .risk_engine()
        .borrow_mut()
        .set_trading_state(nautilus_model::enums::TradingState::Halted);
    node.add_strategy(AccountController {
        compatibility,
        cancel_node: (ordinary_cancel_supported(
            owned_runtime,
            &config.mode,
            registration.is_some(),
            &config.native_strategy.strategy,
        ) || (owned_runtime && config.mode == "paper" && registration.is_some()
            && compatibility == crate::native_runner::AccountCompatibility::TrustedStrategyOnlySingleNode
            && matches!(config.native_strategy.strategy,NativeCompiledStrategyV1::Registered { .. })))
        .then(|| crate::account_queued_cancel::BoundCancelNode::capture(&node)),
        signal_strategy_id: None,
        user_registration: registration,
        signal_restore_error: None,
        native_cache: node.kernel().cache(),
        native_risk: node.kernel().risk_engine().clone(),
        native_trader: node.kernel().trader.clone(),
        native_managed_stop: false,
        core: StrategyCore::new(StrategyConfig {
            // Recovery must not react to old contingent-order events with new commands.
            // Original order facts remain in the official cache; explicit strategies
            // retain their own upstream order-manager behavior when started.
            manage_contingent_orders: false,
            strategy_id: Some(strategy_id),
            order_id_tag: Some("CONTROL".into()),
            // Explicit position IDs allow authorized exits to reduce the signal strategy
            // position. Never infer a controller-owned NETTING position.
            oms_type: Some(nautilus_model::enums::OmsType::Hedging),
            external_order_instrument_ids: None,
            log_events: false,
            log_commands: false,
            ..Default::default()
        }),
        config,
        ids,
        client,
        account,
        inbox,
        handle,
        boot_id,
        session,
        trader: trader.to_string(),
        fee_rate,
        fee_source,
        fee_valid_until_ns,
        control: None,
        snapshot: None,
        last_tick_ns: 0,
        last_slice_ns: 0,
        cache_failed,
        spot_facts: None,
        spot_balance_observation: None,
        target_acceptances: BTreeMap::new(),
        #[cfg(test)]
        replies: None,
    })?;
    node.run().await.context("account_native_node_run")
}

#[cfg(test)]
mod tests {
    #[test]
    fn official_event_json_keeps_adjacent_nanoseconds() -> Result<()> {
        use nautilus_model::events::OrderCanceled;
        let event = OrderEventAny::Canceled(OrderCanceled::new(
            "T-001".parse()?, "S-001".parse()?, "TEST.SIM".parse()?, "original".parse()?,
            UUID4::new(), 1794603862176207674_u64.into(), 1794603862176207675_u64.into(),
            false, None, None, None,
        ));
        let raw = official_order_event_json(&event)?;
        assert_eq!(raw, serde_json::to_string(&event)?);
        assert!(raw.contains("1794603862176207674"));
        assert!(raw.contains("1794603862176207675"));
        let wire = json!({"lastEvent":event,"lastEventJson":raw});
        let decoded: Value = serde_json::from_str(&serde_json::to_string(&wire)?)?;
        assert_eq!(decoded["lastEventJson"].as_str(), Some(raw.as_str()));
        Ok(())
    }

    #[test]
    #[should_panic(expected = "Order MISSING not found")]
    fn lazy_control_scan_preserves_missing_order_failure() {
        let cache = Cache::default();
        let _ = required_order_ref(&cache, &ClientOrderId::from("MISSING"));
    }

    #[test]
    fn summary_activity_uses_official_node_indices_including_unassigned_orders() -> Result<()> {
        use nautilus_model::{enums::{OrderType, LiquiditySide, TriggerType},
            events::{OrderPendingCancel,OrderPendingUpdate,OrderEmulated,OrderReleased,OrderTriggered,OrderFilled},
            identifiers::{VenueOrderId, TradeId}, orders::{builder::OrderTestBuilder,stubs::TestOrderEventStubs}};
        let empty = json!({"open":false,"inflight":false,"active_local":false,"emulated":false});
        for name in ["INITIALIZED","SUBMITTED","ACCEPTED","PENDING_CANCEL","PENDING_UPDATE","PARTIALLY_FILLED","EMULATED","RELEASED","TRIGGERED"] {
            for suffix in ["001","002"] {
                let mut cache=Cache::default();
                let account=AccountId::from(format!("OTHER-{suffix}"));
                let mut builder=OrderTestBuilder::new(OrderType::StopLimit);
                builder.instrument_id(InstrumentId::from("BTC-USDC-SPOT.HYPERLIQUID")).side(OrderSide::Buy).strategy_id(StrategyId::from(format!("OTHER-{suffix}"))).quantity(Quantity::from("2.00"))
                    .price(Price::from("100.00")).trigger_price(Price::from("101.00"));
                if ["EMULATED","RELEASED"].contains(&name) {builder.emulation_trigger(TriggerType::LastPrice);}
                let mut order=builder.build();
                assert!(order.account_id().is_none());
                cache.add_order(order.clone(),None,None,false)?;
                assert_ne!(official_node_order_activity(&cache),empty,"unassigned initialized order");
                let id=order.client_order_id();let trader=order.trader_id();let strategy=order.strategy_id();let instrument=order.instrument_id();
                if ["EMULATED","RELEASED"].contains(&name) {
                    order=cache.update_order(&OrderEventAny::Emulated(OrderEmulated::new(trader,strategy,instrument,id,UUID4::new(),1.into(),1.into())))?;
                    if name=="RELEASED" {order=cache.update_order(&OrderEventAny::Released(OrderReleased::new(trader,strategy,instrument,id,Price::from("101.00"),UUID4::new(),2.into(),2.into())))?;}
                } else if name!="INITIALIZED" {
                    order=cache.update_order(&TestOrderEventStubs::submitted(&order,account))?;
                    if name!="SUBMITTED" {
                        order=cache.update_order(&TestOrderEventStubs::accepted(&order,account,VenueOrderId::from("V-1")))?;
                        let event=match name {
                            "PENDING_CANCEL"=>Some(OrderEventAny::PendingCancel(OrderPendingCancel::new(trader,strategy,instrument,id,Some(account),UUID4::new(),3.into(),3.into(),false,Some(VenueOrderId::from("V-1"))))),
                            "PENDING_UPDATE"=>Some(OrderEventAny::PendingUpdate(OrderPendingUpdate::new(trader,strategy,instrument,id,Some(account),UUID4::new(),3.into(),3.into(),false,Some(VenueOrderId::from("V-1"))))),
                            "TRIGGERED"=>Some(OrderEventAny::Triggered(OrderTriggered::new(trader,strategy,instrument,id,UUID4::new(),3.into(),3.into(),false,Some(VenueOrderId::from("V-1")),Some(account)))),
                            "PARTIALLY_FILLED"=>Some(OrderEventAny::Filled(OrderFilled::new(trader,strategy,instrument,id,VenueOrderId::from("V-1"),account,TradeId::from("T-1"),order.order_side(),order.order_type(),Quantity::from("1.00"),Price::from("100.00"),Currency::USDC(),LiquiditySide::Taker,UUID4::new(),3.into(),3.into(),false,None,None,None))),
                            _=>None,
                        };
                        if let Some(event)=event {order=cache.update_order(&event)?;}
                    }
                }
                assert_eq!(order.status().to_string(),name);
                // The lazy scan must preserve the former account-filtered set,
                // including the absence of an account on local orders.
                for filter in [None, Some(&account)] {
                    let former: Vec<_> = cache.orders(None, None, None, filter, None)
                        .iter().map(|o| o.client_order_id().to_string()).collect();
                    let mut lazy: Vec<_> = cache.iter_client_order_ids(None, None, None, filter)
                        .filter_map(|id| cache.order_ref(&id))
                        .map(|o| o.client_order_id().to_string()).collect();
                    lazy.sort();
                    assert_eq!(former, lazy, "lazy scan {name}/{suffix}");
                }
                assert_ne!(official_node_order_activity(&cache),empty,"{name}/{suffix}");
                cache.clear_index();cache.build_index();
                assert_ne!(official_node_order_activity(&cache),empty,"rebuilt {name}/{suffix}");
                cache.update_order(&TestOrderEventStubs::canceled(&order,account,order.venue_order_id()))?;
                assert_eq!(official_node_order_activity(&cache),empty,"closed {name}/{suffix}");
            }
        }
        let mut cache=Cache::default();
        let mut originals=vec![];
        for suffix in ["001","002"] {
            let order=OrderTestBuilder::new(OrderType::Limit).instrument_id(InstrumentId::from("BTC-USDC-SPOT.HYPERLIQUID"))
                .side(OrderSide::Buy).strategy_id(StrategyId::from(format!("OTHER-{suffix}")))
                .client_order_id(ClientOrderId::from(format!("ORIGINAL-{suffix}")))
                .quantity(Quantity::from("2.00")).price(Price::from("100.00")).build();
            cache.add_order(order.clone(),None,None,false)?;
            let account=AccountId::from(format!("OTHER-{suffix}"));
            let order=cache.update_order(&TestOrderEventStubs::submitted(&order,account))?;
            originals.push((order,account));
        }
        let former: Vec<_> = cache.orders(None, None, None, None, None)
            .iter().map(|o| o.client_order_id()).collect();
        let mut lazy: Vec<_> = cache.iter_client_order_ids(None, None, None, None)
            .map(|id| required_order_ref(&cache, &id).client_order_id()).collect();
        lazy.sort();
        assert_eq!(former, lazy, "preserve official cancellation order");
        for (index,(order,account)) in originals.iter().enumerate() {
            assert_ne!(official_node_order_activity(&cache),empty);
            cache.update_order(&TestOrderEventStubs::canceled(order,*account,None))?;
            assert_eq!(official_node_order_activity(&cache)==empty,index==1,"other account/strategy still blocks zero");
        }
        Ok(())
    }

    #[test]
    fn ordinary_cancel_scope_does_not_claim_user_or_live_support() {
        let builtin: NativeCompiledStrategyV1 = serde_json::from_value(
            json!({"kind":"EMA_CROSS","trade_size":"1","fast_period":2,"slow_period":3}),
        )
        .unwrap();
        assert!(ordinary_cancel_supported(true, "paper", false, &builtin));
        assert!(!ordinary_cancel_supported(false, "paper", false, &builtin));
        assert!(!ordinary_cancel_supported(true, "live", false, &builtin));
        assert!(!ordinary_cancel_supported(true, "paper", true, &builtin));
        let registered: NativeCompiledStrategyV1 = serde_json::from_value(
            json!({"kind":"REGISTERED","name":"user.strategy","config":null}),
        )
        .unwrap();
        assert!(!ordinary_cancel_supported(
            true,
            "paper",
            false,
            &registered
        ));
        let obi: NativeCompiledStrategyV1 = serde_json::from_value(json!({"kind":"ORDER_BOOK_IMBALANCE","max_trade_size":"1","trigger_min_size":"1","trigger_imbalance_ratio":"0.2","min_seconds_between_triggers":1.0,"time_in_force":"IOC","dry_run":false})).unwrap();
        assert!(
            !ordinary_cancel_supported(true, "paper", false, &obi),
            "existing account lifecycle rejects OBI before installation"
        );
    }
    #[test]
    fn native_risk_defaults_and_explicit_limits() {
        let defaults = super::AccountRiskConfig::default().native(&[]).unwrap();
        assert!(!defaults.bypass);
        assert_eq!(
            defaults,
            nautilus_live::config::LiveRiskEngineConfig::default()
        );
        let config: super::AccountRiskConfig = serde_json::from_value(serde_json::json!({
            "max_order_submit_rate":"10/00:00:01", "max_order_modify_rate":"5/00:00:01",
            "max_notional_per_order":{"BTC-USDC-SPOT.HYPERLIQUID":"1000.25"}
        }))
        .unwrap();
        assert!(config.native(&[]).is_err());
        let native = config
            .native(&["BTC-USDC-SPOT.HYPERLIQUID".into()])
            .unwrap();
        assert_eq!(native.max_order_submit_rate, "10/00:00:01");
        assert_eq!(native.max_order_modify_rate, "5/00:00:01");
        assert_eq!(
            native.max_notional_per_order["BTC-USDC-SPOT.HYPERLIQUID"],
            "1000.25"
        );
        assert!(serde_json::from_value::<super::AccountRiskConfig>(
            serde_json::json!({"bypass":true})
        )
        .is_err());
    }
    #[tokio::test]
    async fn live_rejects_paper_scenario_before_connecting() {
        let config: super::AccountRuntimeConfig = serde_json::from_value(serde_json::json!({
            "client_id":"test", "account_id":"HYPERLIQUID-001", "mode":"live",
            "native_strategy":{"instrument_id":"BTC-USDC-SPOT.HYPERLIQUID","strategy":{"kind":"EMA_CROSS","trade_size":"1","fast_period":2,"slow_period":3}},
            "instruments":["BTC-USDC-SPOT.HYPERLIQUID"], "source_id":"unused", "data_dir":"/tmp/unused",
            "paper_execution":{"prob_fill_on_limit":1.0,"prob_slippage":0.0,"random_seed":"1",
                "base_latency_ns":"0","insert_latency_ns":"0","update_latency_ns":"0","cancel_latency_ns":"0"}
        })).unwrap();
        let error = super::run_config(config, "unused".into())
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "account_native_paper_execution_requires_paper"
        );
    }
    #[tokio::test]
    async fn orderbook_strategy_account_lifecycle_rejected_before_connecting() {
        let config: super::AccountRuntimeConfig = serde_json::from_value(serde_json::json!({
            "client_id":"test", "account_id":"HYPERLIQUID-001", "mode":"paper",
            "native_strategy":{"instrument_id":"BTC-USDC-SPOT.HYPERLIQUID","strategy":{"kind":"ORDER_BOOK_IMBALANCE","max_trade_size":"1","trigger_min_size":"1","trigger_imbalance_ratio":"0.2","min_seconds_between_triggers":1.0,"time_in_force":"IOC","dry_run":false}},
            "instruments":["BTC-USDC-SPOT.HYPERLIQUID"],"source_id":"unused","data_dir":"/tmp/unused"
        })).unwrap();
        let error = super::run_config(config, "unused".into())
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "account_native_strategy_account_lifecycle_unsupported_for_orderbook_imbalance"
        );
    }
    #[tokio::test]
    async fn grid_strategy_account_lifecycle_rejected_before_connecting() {
        let config: super::AccountRuntimeConfig = serde_json::from_value(serde_json::json!({
            "client_id":"test", "account_id":"HYPERLIQUID-001", "mode":"paper",
            "native_strategy":{"instrument_id":"BTC-USDC-SPOT.HYPERLIQUID","strategy":{"kind":"GRID_MARKET_MAKER","trade_size":"1","num_levels":2,"grid_step_bps":10,"skew_factor":0.1,"max_position":"2","requote_threshold_bps":5,"expire_time_secs":null,"on_cancel_resubmit":true}},
            "instruments":["BTC-USDC-SPOT.HYPERLIQUID"],"source_id":"unused","data_dir":"/tmp/unused"
        })).unwrap();
        let error = super::run_config(config, "unused".into())
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "account_native_strategy_account_lifecycle_unsupported_for_grid_market_maker"
        );
    }
    #[tokio::test]
    async fn hurst_strategy_account_lifecycle_rejected_before_connecting() {
        let config: super::AccountRuntimeConfig = serde_json::from_value(serde_json::json!({
            "client_id":"test", "account_id":"HYPERLIQUID-001", "mode":"paper",
            "native_strategy":{"instrument_id":"BTC-USDC-SPOT.HYPERLIQUID","strategy":{"kind":"HURST_VPIN_DIRECTIONAL","bar_type":"BTC-USDC-SPOT.HYPERLIQUID-100-VALUE-LAST-INTERNAL","trade_size":"1","hurst_window":8,"hurst_lags":[2,4],"hurst_enter":0.55,"hurst_exit":0.5,"vpin_window":2,"vpin_threshold":0.3,"max_holding_secs":60}},
            "instruments":["BTC-USDC-SPOT.HYPERLIQUID"],"source_id":"unused","data_dir":"/tmp/unused"
        })).unwrap();
        let error = super::run_config(config, "unused".into())
            .await
            .unwrap_err();
        assert_eq!(
            error.to_string(),
            "account_native_strategy_account_lifecycle_unsupported_for_hurst_vpin_directional"
        );
    }
    #[test]
    fn native_paper_models_validate_probabilities_and_exact_nanoseconds() {
        let mut scenario = super::PaperExecutionConfig {
            prob_fill_on_limit: 0.5,
            prob_slippage: 0.2,
            random_seed: "18446744073709551615".into(),
            base_latency_ns: "9007199254740993".into(),
            insert_latency_ns: "1".into(),
            update_latency_ns: "2".into(),
            cancel_latency_ns: "3".into(),
            queue_position: true,
        };
        assert!(scenario.models().is_ok());
        scenario.prob_slippage = 1.1;
        assert!(scenario.models().is_err());
        scenario.prob_slippage = 0.2;
        scenario.base_latency_ns = u64::MAX.to_string();
        assert!(scenario.models().is_err());
        scenario.base_latency_ns = "-1".into();
        assert!(scenario.models().is_err());
    }
    use super::*;

    #[test]
    fn execution_scope_preserves_legacy_product_visibility_and_accepts_explicit_spot() {
        let mut value = json!({
            "client_id": "QZ", "account_id": "HYPERLIQUID-001", "mode": "live",
            "instruments": ["HYPE-USDC-SPOT.HYPERLIQUID"],
            "native_strategy":{"instrument_id":"HYPE-USDC-SPOT.HYPERLIQUID","strategy":{"kind":"EMA_CROSS","trade_size":"1","fast_period":2,"slow_period":3}},
            "source_id": "source", "data_dir": "/tmp/account"
        });
        let config: AccountRuntimeConfig = serde_json::from_value(value.clone()).unwrap();
        let products = execution_product_types(&config).unwrap();
        assert!(products.contains(&HyperliquidProductType::Spot));
        assert!(products.contains(&HyperliquidProductType::Perp));
        assert!(products.contains(&HyperliquidProductType::Outcome));
        value["execution_product_types"] = json!(["SPOT"]);
        let config = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            execution_product_types(&config).unwrap(),
            vec![HyperliquidProductType::Spot]
        );
        value["execution_product_types"] = json!([]);
        assert!(execution_product_types(&serde_json::from_value(value.clone()).unwrap()).is_err());
        value["execution_product_types"] = json!(["PERP"]);
        assert!(execution_product_types(&serde_json::from_value(value.clone()).unwrap()).is_err());
        value["execution_product_types"] = json!(["INVALID"]);
        assert!(serde_json::from_value::<AccountRuntimeConfig>(value).is_err());
    }

    use nautilus_common::{
        cache::database::CacheDatabaseFactory, live::get_data_event_sender, messages::DataEvent,
    };
    use nautilus_core::{UnixNanos, UUID4};
    use nautilus_live::node::NodeRunMode;
    use nautilus_model::{
        data::{BookOrder, Data},
        identifiers::Symbol,
        instruments::CurrencyPair,
        orderbook::OrderBook,
        types::{Price, Quantity},
    };
    use std::{cell::RefCell, rc::Rc, time::Duration};

    #[derive(Debug)]
    pub(super) struct OnStartSubmit {
        pub(super) core: StrategyCore,
        pub(super) instrument: InstrumentId,
        pub(super) client: ClientId,
        pub(super) trade_size: Quantity,
    }
    nautilus_strategy!(OnStartSubmit, {});
    impl DataActor for OnStartSubmit {
        fn on_start(&mut self) -> Result<()> {
            let instrument = self.instrument;
            let trade_size = self.trade_size;
            let order = self.order_factory().market(
                instrument,
                OrderSide::Buy,
                trade_size,
                None,
                None,
                None,
                None,
                None,
                None,
                None,
            );
            self.submit_order(order, None, Some(self.client), None)
        }
    }

    #[derive(Debug)]
    pub(super) struct OnStartResting {
        pub(super) core: StrategyCore,
        pub(super) instrument: InstrumentId,
        pub(super) client: ClientId,
    }
    nautilus_strategy!(OnStartResting, {});
    impl DataActor for OnStartResting {
        fn on_start(&mut self) -> Result<()> {
            let instrument = self.instrument;
            let order = self.order_factory().limit(
                instrument,
                OrderSide::Buy,
                Quantity::from("1.00"),
                Price::from("1.00"),
                Some(TimeInForce::Gtc),
                None,
                Some(false),
                Some(false),
                Some(false),
                None,
                None,
                None,
                None,
                None,
                None,
                Some(ClientOrderId::from("signal-resting")),
            );
            self.submit_order(order, None, Some(self.client), None)
        }
    }

    type Replies = Rc<RefCell<BTreeMap<String, Value>>>;
    fn pair(id: &str, base: Currency) -> InstrumentAny {
        InstrumentAny::CurrencyPair(
            CurrencyPair::builder()
                .instrument_id(InstrumentId::from(id))
                .raw_symbol(Symbol::from(id.split('.').next().unwrap()))
                .base_currency(base)
                .quote_currency(Currency::USDC())
                .price_precision(2)
                .size_precision(2)
                .price_increment(Price::from("0.01"))
                .size_increment(Quantity::from("0.01"))
                .min_quantity(Quantity::from("0.01"))
                .min_notional(Money::from("1 USDC"))
                .ts_event(1.into())
                .ts_init(1.into())
                .build()
                .unwrap(),
        )
    }
    async fn node(
        directory: &std::path::Path,
    ) -> Result<(LiveNode, mpsc::SyncSender<Request>, Replies)> {
        node_with_scope(directory, false).await
    }
    // The official Sandbox executes these orders; only the owner's LIVE spot
    // scope guard is selected, to exercise isolation from merged native cash.
    async fn node_with_scope(
        directory: &std::path::Path,
        scoped: bool,
    ) -> Result<(LiveNode, mpsc::SyncSender<Request>, Replies)> {
        let ids = vec![
            InstrumentId::from("BTC-USDC-SPOT.HYPERLIQUID"),
            InstrumentId::from("ETH-USDC-SPOT.HYPERLIQUID"),
        ];
        let account = AccountId::from("HYPERLIQUID-001");
        let trader = TraderId::from("QZ-ACCOUNT-001");
        let factory = AccountCacheFactory::new(directory.into());
        // Persist only official native instrument definitions before the node's
        // public load path runs. No prices, fills or account values are invented.
        {
            let mut db = factory
                .create(
                    trader,
                    UUID4::new(),
                    CacheConfig {
                        flush_on_start: false,
                        ..Default::default()
                    },
                )
                .await?;
            for currency in [Currency::USDC(), Currency::BTC(), Currency::ETH()] {
                db.add_currency(&currency)?;
            }
            db.add_instrument(&pair(&ids[0].to_string(), Currency::BTC()))?;
            db.add_instrument(&pair(&ids[1].to_string(), Currency::ETH()))?;
            db.close()?;
        }
        let starting = factory
            .restored_balances(&account)?
            .unwrap_or(vec![Money::from("1000 USDC")]);
        let cache_config = CacheConfig {
            flush_on_start: false,
            save_market_data: false,
            ..Default::default()
        };
        let mut node = LiveNode::builder(trader, Environment::Sandbox)?
            .with_logging(LoggerConfig {
                bypass_logging: true,
                ..Default::default()
            })
            .with_load_state(true)
            .with_save_state(true)
            .with_cache_config(cache_config.clone())
            .with_reconciliation(true)
            .with_exec_engine_config(LiveExecutionEngineConfig {
                load_cache: false,
                ..Default::default()
            })
            .with_timeout_connection(2)
            .with_timeout_portfolio(2)
            .with_delay_post_stop_secs(0)
            .with_delay_shutdown_secs(0)
            .add_simulated_exec_client(
                Some("QZ-PAPER".into()),
                Box::new(SandboxExecutionClientFactory::new()),
                Box::new(SandboxExecutionClientConfig {
                    account_id: account,
                    venue: Venue::from("HYPERLIQUID"),
                    account_type: AccountType::Cash,
                    base_currency: Some(Currency::USDC()),
                    starting_balances: starting,
                    fee_model: Some(FeeModelAny::MakerTaker(MakerTakerFeeModel::new(
                        Decimal::new(1, 3),
                        Decimal::new(1, 3),
                    ))),
                    liquidity_consumption: true,
                    ..Default::default()
                }),
            )?
            .build()?;
        factory
            .install(
                node.kernel().cache(),
                trader,
                node.instance_id(),
                cache_config,
            )
            .await?;
        prepare_recovery_cache(&node).await?;
        install_cache_failure_risk_fence(&node, factory.failed.clone());
        let (tx, rx) = mpsc::sync_channel(64);
        let replies = Rc::new(RefCell::new(BTreeMap::new()));
        node.add_strategy(AccountController {
            compatibility: crate::native_runner::AccountCompatibility::Unspecified,
            cancel_node: Some(crate::account_queued_cancel::BoundCancelNode::capture(
                &node,
            )),
            signal_strategy_id: None,
            user_registration: None,
            signal_restore_error: None,
            native_cache: node.kernel().cache(),
            native_risk: node.kernel().risk_engine().clone(),
            native_trader: node.kernel().trader.clone(),
            native_managed_stop: false,
            core: StrategyCore::new(StrategyConfig {
                manage_contingent_orders: false,
                strategy_id: Some(StrategyId::from("QZ-ACCOUNT-OWNER-001")),
                order_id_tag: Some("CONTROL".into()),
                // Explicit position IDs allow authorized exits to reduce the signal strategy
                // position. Never infer a controller-owned NETTING position.
                oms_type: Some(nautilus_model::enums::OmsType::Hedging),
                external_order_instrument_ids: None,
                log_events: false,
                log_commands: false,
                ..Default::default()
            }),
            config: AccountRuntimeConfig {
                client_id: "test".into(),
                account_id: account.to_string(),
                mode: if scoped { "live" } else { "paper" }.into(),
                account_address: None,
                native_strategy: NativeStrategyDeployment {
                    instrument_id: ids[0].to_string(),
                    strategy: NativeCompiledStrategyV1::EmaCross {
                        trade_size: "1".into(),
                        fast_period: 2,
                        slow_period: 3,
                        base: None,
                    },
                },
                instruments: ids.iter().map(ToString::to_string).collect(),
                starting_balance: Some("1000".into()),
                paper_fee_rate: Some("0.001".into()),
                risk: AccountRiskConfig::default(),
                paper_execution: None,
                source_id: Id::new().to_string(),
                trader_id: None,
                data_dir: directory.into(),
                testnet: false,
                execution_product_types: None,
            },
            ids,
            client: ClientId::from("QZ-PAPER"),
            account,
            inbox: rx,
            handle: node.handle(),
            boot_id: "boot-test".into(),
            session: node.instance_id().to_string(),
            trader: trader.to_string(),
            fee_rate: Decimal::new(1, 3),
            fee_source: json!({"kind":"EXPLICIT_PAPER_FEE_SCENARIO","rate":"0.001"}),
            fee_valid_until_ns: u64::MAX,
            control: None,
            snapshot: None,
            last_tick_ns: 0,
            last_slice_ns: 0,
            cache_failed: factory.failed.clone(),
            spot_facts: None,
            spot_balance_observation: None,
            target_acceptances: BTreeMap::new(),
            replies: Some(replies.clone()),
        })?;
        Ok((node, tx, replies))
    }
    async fn request(
        tx: &mpsc::SyncSender<Request>,
        replies: &Replies,
        operation: &str,
        body: Value,
    ) -> Result<Value> {
        let id = Id::new().to_string();
        tx.send(Request {
            id: id.clone(),
            operation: operation.into(),
            body: serde_json::from_value(body)?,
        })?;
        let end = tokio::time::Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(v) = replies.borrow_mut().remove(&id) {
                if let Some(e) = v["error"].as_str() {
                    bail!("{e}")
                }
                return Ok(v);
            }
            ensure!(
                tokio::time::Instant::now() < end,
                "native_test_command_timeout"
            );
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    fn control(epoch: &str, mode: &str, reserved: &str) -> Value {
        json!({"epoch":epoch,"revision":epoch,"reserved_amount":reserved,"cash_buffer":"0","mode":mode,"exit_ids":["exit-original"],"minimum_interval_seconds":1})
    }
    fn publish_native_quote(
        cache: &Rc<RefCell<nautilus_common::cache::Cache>>,
        price: &str,
    ) -> Result<()> {
        publish_native_quote_size(cache, price, "1000.00")
    }
    fn publish_native_quote_size(
        cache: &Rc<RefCell<nautilus_common::cache::Cache>>,
        price: &str,
        size: &str,
    ) -> Result<()> {
        let now = UnixNanos::from(chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64);
        let id = InstrumentId::from("BTC-USDC-SPOT.HYPERLIQUID");
        let price = Price::from_str(price).map_err(anyhow::Error::msg)?;
        let size = Quantity::from_str(size).map_err(anyhow::Error::msg)?;
        let mut book = OrderBook::new(id, BookType::L2_MBP);
        book.add(BookOrder::new(OrderSide::Buy, price, size, 1), 0, 1, now);
        book.add(BookOrder::new(OrderSide::Sell, price, size, 2), 0, 2, now);
        cache.borrow_mut().add_order_book(book)?;
        get_data_event_sender().send(DataEvent::Data(Data::Quote(QuoteTick::new(
            id, price, price, size, size, now, now,
        ))))?;
        Ok(())
    }

    #[cfg(unix)]
    #[ignore = "explicit isolated Pi integration fixture"]
    #[tokio::test(flavor = "current_thread")]
    async fn pi_cancel_sandbox_transport_fixture() -> Result<()> {
        let path = std::env::var("QZ_CANCEL_SANDBOX_FIXTURE")?;
        let directory = std::path::PathBuf::from(path);
        ensure!(directory.is_dir(), "test_directory_required");
        let ndjson = std::env::var_os("QZ_CANCEL_NDJSON_FIXTURE").is_some();
        let service_fixture = std::env::var_os("QZ_CANCEL_SERVICE_FIXTURE").is_some();
        let boot: Option<Request> = if ndjson {
            let line = io::stdin()
                .lock()
                .lines()
                .next()
                .ok_or_else(|| anyhow!("test_boot_required"))??;
            let request: Request = serde_json::from_str(&line)?;
            ensure!(request.operation == "BOOT", "test_boot_required");
            Some(request)
        } else {
            None
        };
        let cache_directory = tempfile::tempdir()?;
        let (mut native, tx, replies) = node(cache_directory.path()).await?;
        let owner_id = *native
            .kernel()
            .trader
            .borrow()
            .strategy_ids()
            .last()
            .unwrap();
        {
            let mut owner = nautilus_common::actor::registry::try_get_actor_unchecked::<
                AccountController,
            >(&owner_id.inner())
            .unwrap();
            owner.config.client_id = "test-on-start-resting".into();
            owner.config.source_id = "test-source".into();
            if let Some(boot) = &boot {
                ensure!(
                    boot.value("config")["mode"] == "paper",
                    "test_requires_paper"
                );
                if service_fixture {
                    owner.config.source_id = boot.value("config")["source_id"]
                        .as_str().ok_or_else(|| anyhow!("test_source_required"))?.to_owned();
                } else {
                    ensure!(boot.value("config")["source_id"] == "test-source", "test_source_required");
                }
                owner.boot_id = boot.id.clone();
            }
        }

        let session = native.instance_id();
        let environment = native.kernel().environment();
        ensure!(environment == Environment::Sandbox, "test_requires_sandbox");
        let handle = native.handle();
        let stop = handle.clone();
        let cache = native.kernel().cache();
        let engine = native.kernel().exec_engine().clone();
        if ndjson {
            let input = tx.clone();
            let closed = handle.clone();
            std::thread::spawn(move || {
                for line in io::stdin().lock().lines() {
                    let Ok(line) = line else { break };
                    let Ok(request) = serde_json::from_str::<Request>(&line) else {
                        break;
                    };
                    if request.ensure_external().is_err() || input.send(request).is_err() {
                        break;
                    }
                }
                closed.stop();
            });
        }
        let write = |name: &str, value: &Value| -> Result<()> {
            use std::io::Write;
            use std::os::unix::fs::OpenOptionsExt;
            let pending = directory.join(format!("{name}.pending"));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&pending)?;
            file.write_all(&serde_json::to_vec(value)?)?;
            drop(file);
            std::fs::rename(pending, directory.join(name))?;
            Ok(())
        };
        let drive = async {
            while !handle.is_running() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            publish_native_quote(&cache, "100.00")?;
            tokio::time::sleep(Duration::from_millis(30)).await;
            if service_fixture {
                ensure!(cache.borrow().orders(None, None, None, None, None).is_empty(), "test_boot_created_order");
                write("boot-empty.json", &json!({"orders":0}))?;
                let deadline = std::time::Instant::now() + Duration::from_secs(60);
                while !directory.join("explicit-test-start").exists() {
                    ensure!(std::time::Instant::now() < deadline, "test_explicit_start_timeout");
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
            }
            let mut normal = control("1", "NORMAL", "0");
            normal["exit_ids"] = json!([]);
            request(&tx, &replies, "CONTROL", normal).await?;
            request(&tx, &replies, "STRATEGY_START", json!({"epoch":"1"})).await?;
            tokio::time::sleep(Duration::from_millis(40)).await;
            publish_native_quote_size(&cache, "1.00", "0.25")?;
            tokio::time::sleep(Duration::from_millis(60)).await;
            let order = cache
                .borrow()
                .order(&ClientOrderId::from("signal-resting"))
                .unwrap()
                .clone();
            ensure!(
                order.status() == nautilus_model::enums::OrderStatus::PartiallyFilled,
                "test_partial_fill_required"
            );
            let command = json!({"schemaVersion":1,"requestId":"sandbox-cancel-original","action":"CANCEL",
                "strategyId":order.strategy_id().to_string(),"order":{"traderId":order.trader_id().to_string(),"accountId":order.account_id().unwrap().to_string(),"instrumentId":order.instrument_id().to_string(),"clientOrderId":order.client_order_id().to_string()},
                "binding":{"sourceId":boot.as_ref().filter(|_| service_fixture).map(|b|b.value("config")["source_id"].clone()).unwrap_or(json!("test-source")),"buildId":std::env::current_exe()?.canonicalize()?.to_string_lossy(),"environmentId":"Sandbox","nativeSessionId":session.to_string()}});
            let before = engine.borrow().command_count();
            let wrong_session = request(
                &tx,
                &replies,
                "ORDER_CANCEL_ATTEMPT",
                json!({"expected_native_session_id":"wrong","command":command}),
            )
            .await
            .unwrap_err();
            assert_eq!(
                wrong_session.to_string(),
                "native_cancel_node_binding_mismatch"
            );
            let mut wrong_source = command.clone();
            wrong_source["binding"]["sourceId"] = json!("other");
            assert_eq!(
                request(
                    &tx,
                    &replies,
                    "ORDER_CANCEL_ATTEMPT",
                    json!({"expected_native_session_id":session.to_string(),"command":wrong_source})
                )
                .await
                .unwrap_err()
                .to_string(),
                "native_cancel_original_binding_mismatch"
            );
            assert_eq!(engine.borrow().command_count(), before);
            write(
                "ready.json",
                &json!({"command":command,"filledQuantity":order.filled_qty().to_string(),"commandCount":before}),
            )?;
            // Only this explicit ignored fixture creates the simulated test
            // order; production BOOT remains unchanged and recovery-only.
            if ndjson {
                let deadline = std::time::Instant::now() + Duration::from_secs(60);
                while handle.is_running() {
                    ensure!(std::time::Instant::now() < deadline, "test_ndjson_timeout");
                    tokio::time::sleep(Duration::from_millis(10)).await;
                }
                return Ok::<_, anyhow::Error>(());
            }
            let deadline = std::time::Instant::now() + Duration::from_secs(120);
            let mut sequence = 0;
            loop {
                ensure!(
                    std::time::Instant::now() < deadline,
                    "test_transport_timeout"
                );
                let name = format!("request-{sequence}.json");
                let path = directory.join(&name);
                if !path.exists() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                    continue;
                }
                let bytes = std::fs::read(path)?;
                ensure!(bytes.len() <= 16384, "test_request_too_large");
                let input: Value = serde_json::from_slice(&bytes)?;
                let op = input["op"].as_str().unwrap_or("");
                let response = if input["command"] != command {
                    json!({"error":"test_original_binding_mismatch"})
                } else if op == "CANCEL" {
                    let attempt = request(
                        &tx,
                        &replies,
                        "ORDER_CANCEL_ATTEMPT",
                        json!({"expected_native_session_id":session.to_string(),"command":command}),
                    )
                    .await?;
                    // A void official queue send is attempted, not a durable receipt.
                    attempt
                } else if op == "READ" {
                    let mut observed = request(
                        &tx,
                        &replies,
                        "ORDER_OBSERVE",
                        json!({"expected_native_session_id":session.to_string(),"command":command}),
                    )
                    .await?;
                    observed["commandCount"] = json!(engine.borrow().command_count());
                    observed
                } else if op == "STOP_FIXTURE" {
                    json!({"stopped":true})
                } else {
                    json!({"error":"test_operation_unsupported"})
                };
                write(&format!("response-{sequence}.json"), &response)?;
                sequence += 1;
                if op == "STOP_FIXTURE" && input["command"] == command {
                    break;
                }
            }
            Ok::<_, anyhow::Error>(())
        };
        let controlled = async {
            let result = drive.await;
            stop.stop();
            result
        };
        let (run, result) = tokio::join!(native.run_with_mode(NodeRunMode::Hosted), controlled);
        run?;
        result?;
        native.dispose();
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn registered_account_callback_parameters_are_used_only_on_explicit_start() -> Result<()>
    {
        fn register(
            registrar: &mut AccountStrategyRegistrar<'_>,
            name: &str,
            config: &Value,
        ) -> Result<()> {
            ensure!(
                name == "test.parameterized",
                "TEST_ACCOUNT_STRATEGY_UNKNOWN"
            );
            let instrument = registrar.instrument_id()?;
            assert_eq!(registrar.trader_id().to_string(), registrar.owner.trader);
            assert_eq!(registrar.account_id(), registrar.owner.account);
            assert_eq!(registrar.execution_client_id(), registrar.owner.client);
            let client = registrar.execution_client_id();
            let trade_size = Quantity::from_str(
                config["trade_size"]
                    .as_str()
                    .ok_or_else(|| anyhow!("missing trade size"))?,
            )
            .map_err(anyhow::Error::msg)?;
            registrar.add(OnStartSubmit {
                core: StrategyCore::new(StrategyConfig {
                    strategy_id: Some(StrategyId::from("CUSTOM-001")),
                    order_id_tag: Some("001".into()),
                    ..Default::default()
                }),
                instrument,
                client,
                trade_size,
            })
        }
        for amount in ["0.10", "0.20"] {
            let directory = tempfile::tempdir()?;
            let (mut native, tx, replies) = node(directory.path()).await?;
            let owner_id = *native
                .kernel()
                .trader
                .borrow()
                .strategy_ids()
                .last()
                .unwrap();
            let (expected_trader, expected_account) = {
                let mut owner = nautilus_common::actor::registry::try_get_actor_unchecked::<
                    AccountController,
                >(&owner_id.inner())
                .unwrap();
                owner.user_registration = Some(register);
                owner.config.native_strategy.strategy = NativeCompiledStrategyV1::Registered {
                    name: "test.parameterized".into(),
                    config: json!({"trade_size":amount}),
                };
                let trader = owner.native_trader.borrow().trader_id;
                (trader, owner.account)
            };
            let handle = native.handle();
            let stop = handle.clone();
            let cache = native.kernel().cache();
            let drive = async {
                while !handle.is_running() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                publish_native_quote(&cache, "100.00")?;
                tokio::time::sleep(Duration::from_millis(30)).await;
                assert!(cache
                    .borrow()
                    .orders(None, None, None, None, None)
                    .is_empty());
                let mut normal = control("1", "NORMAL", "0");
                normal["exit_ids"] = json!([]);
                request(&tx, &replies, "CONTROL", normal).await?;
                request(&tx, &replies, "STRATEGY_START", json!({"epoch":"1"})).await?;
                let status = request(&tx, &replies, "STRATEGY_STATUS", json!({})).await?;
                assert_eq!(status["restart_warmup"], "UPSTREAM_STRATEGY_DEFINED");
                tokio::time::sleep(Duration::from_millis(80)).await;
                ensure!(
                    cache
                        .borrow()
                        .orders(None, None, None, None, None)
                        .iter()
                        .any(|o| o.strategy_id() == StrategyId::from("CUSTOM-001")
                            && o.trader_id() == expected_trader
                            && o.account_id() == Some(expected_account)
                            && o.filled_qty() == Quantity::from_str(amount).unwrap()),
                    "custom parameter not passed to native strategy"
                );
                stop.stop();
                Ok::<_, anyhow::Error>(())
            };
            let (run, result) = tokio::join!(native.run_with_mode(NodeRunMode::Hosted), drive);
            run?;
            result?;
            native.dispose();
        }
        Ok(())
    }

    #[test]
    fn spot_observation_external_stdin_operation_is_rejected() -> Result<()> {
        let injected: Request = serde_json::from_str(
            r#"{"requestId":"injected","operation":"SPOT_BALANCE_REFRESH","observation":{"balances":[]}}"#,
        )?;
        assert_eq!(
            injected.ensure_external().unwrap_err().to_string(),
            "account_native_internal_operation"
        );
        let snapshot: Request =
            serde_json::from_str(r#"{"requestId":"read","operation":"SNAPSHOT"}"#)?;
        snapshot.ensure_external()?;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn spot_observation_wrong_binding_and_full_queue_preserve_old_time() -> Result<()> {
        let directory = tempfile::tempdir()?;
        // Official Sandbox client only. This node is never run or connected.
        let (mut native, tx, _replies) = node_with_scope(directory.path(), true).await?;
        let owner_id = *native
            .kernel()
            .trader
            .borrow()
            .strategy_ids()
            .last()
            .unwrap();
        {
            let mut owner = nautilus_common::actor::registry::try_get_actor_unchecked::<
                AccountController,
            >(&owner_id.inner())
            .unwrap();
            owner.config.account_address = Some("explicit-test-address".into());
            let old = json!({"account_id":owner.config.account_id,"source_id":owner.config.source_id,
                "account_address":"explicit-test-address","network":"mainnet","request_started_ns":"100","received_ns":"200",
                "balances":[],"trade_permission":false});
            let refresh = |value: Value| Request {
                id: String::new(),
                operation: "SPOT_BALANCE_REFRESH".into(),
                body: BTreeMap::from([("observation".into(), value)]),
            };
            owner.process(&refresh(old.clone()))?;
            for key in ["account_id", "source_id", "account_address", "network"] {
                let mut invalid = old.clone();
                invalid[key] = json!("wrong");
                assert!(owner.process(&refresh(invalid)).is_err());
                assert_eq!(owner.spot_balance_observation.as_ref(), Some(&old));
            }
            for _ in 0..64 {
                tx.try_send(Request {
                    id: String::new(),
                    operation: "SNAPSHOT".into(),
                    body: BTreeMap::new(),
                })?;
            }
            let mut newer = old.clone();
            newer["received_ns"] = json!("300");
            assert!(matches!(
                tx.try_send(refresh(newer)),
                Err(mpsc::TrySendError::Full(_))
            ));
            assert_eq!(owner.spot_balance_observation.as_ref(), Some(&old));
            assert_eq!(
                owner.spot_balance_observation.as_ref().unwrap()["received_ns"],
                "200",
                "queue loss never relabels an old observation as fresh"
            );
            assert!(
                owner.spot_facts.is_none(),
                "observation cannot create trade evidence"
            );
            assert!(owner.control.is_none());
        }
        native.dispose();
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn official_account_updates_and_spot_observations_keep_risk_halted() -> Result<()> {
        use nautilus_hyperliquid::{
            common::parse::parse_spot_account_balances, http::models::SpotClearinghouseState,
        };
        use nautilus_model::{enums::TradingState, events::AccountState, types::AccountBalance};

        let directory = tempfile::tempdir()?;
        // Only the existing Sandbox factory is installed. Never run/connect the node,
        // start a strategy, send CONTROL or submit an order in this component test.
        let (mut native, _tx, _replies) = node_with_scope(directory.path(), true).await?;
        let cache = native.kernel().cache();
        let portfolio = native.kernel().portfolio.clone();
        let risk = native.kernel().risk_engine().clone();
        let execution = native.kernel().exec_engine().clone();
        risk.borrow_mut().set_trading_state(TradingState::Halted);
        let risk_commands = risk.borrow().command_count();
        let execution_commands = execution.borrow().command_count();
        let account = AccountId::from("HYPERLIQUID-001");
        let parse = |total: &str, hold: &str| -> Result<Vec<AccountBalance>> {
            let state: SpotClearinghouseState = serde_json::from_value(json!({"balances":[{
                "coin":"USDC","token":0,"total":total,"hold":hold,"entryNtl":"0"
            }]}))?;
            parse_spot_account_balances(&state)
        };
        let event = |balances, time: u64| {
            AccountState::new(
                account,
                AccountType::Margin,
                balances,
                vec![],
                true,
                UUID4::new(),
                time.into(),
                time.into(),
                None,
            )
        };
        let initial = parse("100.25", "10")?;
        assert_eq!(
            initial,
            vec![AccountBalance::new(
                Money::from("100.25 USDC"),
                Money::from("10 USDC"),
                Money::from("90.25 USDC")
            )]
        );
        // Official Portfolio -> Cache -> Account apply path, not QZ balance arithmetic.
        portfolio
            .borrow_mut()
            .update_account(&event(initial.clone(), 100));
        assert_eq!(
            cache.borrow().account_ref(&account).unwrap().balances()[&Currency::USDC()],
            initial[0]
        );
        assert_eq!(
            portfolio
                .borrow_mut()
                .build_snapshot(&account)
                .unwrap()
                .balances,
            initial
        );
        let account_before = serde_json::to_value(&*cache.borrow().account_ref(&account).unwrap())?;
        let owner_id = *native
            .kernel()
            .trader
            .borrow()
            .strategy_ids()
            .last()
            .unwrap();
        {
            let mut owner = nautilus_common::actor::registry::try_get_actor_unchecked::<
                AccountController,
            >(&owner_id.inner())
            .unwrap();
            owner.config.account_address = Some("explicit-test-address".into());
            let observation = json!({"account_id":owner.config.account_id,"source_id":owner.config.source_id,
                "account_address":"explicit-test-address","network":"mainnet","request_started_ns":"200","received_ns":"300",
                "balances":parse("800", "1")?,"trade_permission":false,"atomic_reservation":false});
            owner.process(&Request {
                id: String::new(),
                operation: "SPOT_BALANCE_REFRESH".into(),
                body: BTreeMap::from([("observation".into(), observation.clone())]),
            })?;
            assert_eq!(owner.spot_balance_observation.as_ref(), Some(&observation));
            assert!(owner.spot_facts.is_none());
            assert!(owner.control.is_none());
        }
        assert_eq!(
            serde_json::to_value(&*cache.borrow().account_ref(&account).unwrap())?,
            account_before
        );
        assert_eq!(
            portfolio
                .borrow_mut()
                .build_snapshot(&account)
                .unwrap()
                .balances,
            initial
        );
        assert_eq!(risk.borrow().trading_state(), TradingState::Halted);

        let updated = parse("50", "5")?;
        portfolio
            .borrow_mut()
            .update_account(&event(updated.clone(), 400));
        assert_eq!(
            cache.borrow().account_ref(&account).unwrap().balances()[&Currency::USDC()],
            updated[0]
        );
        assert_eq!(
            portfolio
                .borrow_mut()
                .build_snapshot(&account)
                .unwrap()
                .balances,
            updated
        );
        // The spot-only parser omits zero tokens. Applying that empty update is
        // not a zero-balance proof: official Account retains omitted currencies.
        let zero = parse("0", "0")?;
        assert!(zero.is_empty());
        portfolio.borrow_mut().update_account(&event(zero, 500));
        assert_eq!(
            cache.borrow().account_ref(&account).unwrap().balances()[&Currency::USDC()],
            updated[0]
        );
        assert_eq!(
            portfolio
                .borrow_mut()
                .build_snapshot(&account)
                .unwrap()
                .balances,
            updated
        );
        let before_invalid = serde_json::to_value(&*cache.borrow().account_ref(&account).unwrap())?;
        assert!(parse("invalid", "0").is_err());
        assert_eq!(
            serde_json::to_value(&*cache.borrow().account_ref(&account).unwrap())?,
            before_invalid
        );
        assert_eq!(risk.borrow().trading_state(), TradingState::Halted);
        assert_eq!(risk.borrow().command_count(), risk_commands);
        assert_eq!(execution.borrow().command_count(), execution_commands);
        assert!(cache
            .borrow()
            .iter_client_order_ids(None, None, None, None)
            .next()
            .is_none());
        native.dispose();
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn signal_on_start_orders_only_after_explicit_start() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (mut native, tx, replies) = node(directory.path()).await?;
        {
            let owner_id = *native
                .kernel()
                .trader
                .borrow()
                .strategy_ids()
                .last()
                .unwrap();
            let mut owner = nautilus_common::actor::registry::try_get_actor_unchecked::<
                AccountController,
            >(&owner_id.inner())
            .unwrap();
            owner.config.client_id = "test-on-start".into();
        }
        let handle = native.handle();
        let stop = handle.clone();
        let cache = native.kernel().cache();
        let drive = async {
            while !handle.is_running() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            publish_native_quote(&cache, "100.00")?;
            tokio::time::sleep(Duration::from_millis(30)).await;
            assert!(cache
                .borrow()
                .orders(None, None, None, None, None)
                .is_empty());
            let status = request(&tx, &replies, "STRATEGY_STATUS", json!({})).await?;
            assert_eq!(status["state"], "UNREGISTERED");
            let mut normal = control("1", "NORMAL", "0");
            normal["exit_ids"] = json!([]);
            request(&tx, &replies, "CONTROL", normal).await?;
            request(&tx, &replies, "STRATEGY_START", json!({"epoch":"1"})).await?;
            tokio::time::sleep(Duration::from_millis(80)).await;
            ensure!(
                cache
                    .borrow()
                    .orders(None, None, None, None, None)
                    .iter()
                    .any(|o| o.strategy_id() == StrategyId::from("ONSTART-001")
                        && o.filled_qty() > Quantity::zero(2)),
                "explicit_start_did_not_run_native_callback"
            );
            // A position is not a currency balance or a QZ-computed available size.
            // Preserve official position quantities and original orders; no per-position valuation policy is recreated.
            let snapshot = request(&tx, &replies, "SNAPSHOT", json!({})).await?;
            // Periodic payload excludes all historical orders/events; explicit export remains intact.
            let summary = request(&tx, &replies, "ACCOUNT_SUMMARY", json!({})).await?;
            assert!(summary.get("orders").is_none());
            assert!(summary.get("fills").is_none());
            assert_eq!(summary["official_node_order_activity"], json!({"open":false,"inflight":false,"active_local":false,"emulated":false}));
            let row = snapshot["orders"].as_array().unwrap().first().unwrap();
            let session = summary["native_session_id"].clone();
            let status = request(&tx, &replies, "ORDER_STATUS", json!({"expected_native_session_id":session,"client_order_id":row["client_order_id"]})).await?;
            assert_eq!(status["status"], row["status"]);
            assert_eq!(status["account_id"], summary["native_account_id"]);
            assert_eq!(status["client_order_id"], row["client_order_id"]);
            assert_eq!(status["native_session_id"], session);
            assert!(request(&tx, &replies, "ORDER_STATUS", json!({"expected_native_session_id":"other","client_order_id":row["client_order_id"]})).await.unwrap_err().to_string().contains("native_order_session_changed"));
            let missing = request(
                &tx,
                &replies,
                "ORDER_STATUS",
                json!({"expected_native_session_id":session,"client_order_id":"absent-original"}),
            )
            .await?;
            assert!(missing["status"].is_null());
            let positions = snapshot["positions"].as_array().unwrap();
            ensure!(!positions.is_empty(), "native_position_not_projected");
            for row in positions {
                ensure!(
                    row.get("free_quantity").is_none(),
                    "derived_free_quantity_retained"
                );
                ensure!(row["value"].is_null(), "derived_position_value_retained");
                ensure!(
                    row["pnl"].is_null(),
                    "caller_selected_position_pnl_retained"
                );
                ensure!(
                    cache
                        .borrow()
                        .positions(None, None, None, None, None)
                        .iter()
                        .any(|p| p.instrument_id.to_string()
                            == row["instrument_id"].as_str().unwrap()
                            && p.signed_decimal_qty().to_string()
                                == row["quantity"].as_str().unwrap()),
                    "native_position_quantity_changed"
                );
            }
            stop.stop();
            Ok::<_, anyhow::Error>(())
        };
        let (run, result) = tokio::join!(native.run_with_mode(NodeRunMode::Hosted), drive);
        run?;
        result?;
        native.dispose();
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn recovery_preserves_native_bracket_without_automatic_commands() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let mut original = Vec::new();
        for seed in [true, false] {
            let (mut native, tx, replies) = node(directory.path()).await?;
            let handle = native.handle();
            let stop = handle.clone();
            let cache = native.kernel().cache();
            let execution = native.kernel().exec_engine().clone();
            let risk = native.kernel().risk_engine().clone();
            // Baseline before node.run: cover startup/reconciliation as well as quotes.
            let before_execution = execution.borrow().command_count();
            let before_risk = risk.borrow().command_count();
            let drive = async {
                while !handle.is_running() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                if seed {
                    publish_native_quote_size(&cache, "100.00", "0.25")?;
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    // Explicit test-only simulated submission. No real client is installed.
                    risk.borrow_mut()
                        .set_trading_state(nautilus_model::enums::TradingState::Active);
                    request(&tx, &replies, "TEST_BRACKET", json!({})).await?;
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    ensure!(
                        cache
                            .borrow()
                            .orders(None, None, None, None, None)
                            .iter()
                            .any(|o| o.filled_qty() > Quantity::zero(2)),
                        "sandbox_bracket_no_fill"
                    );
                } else {
                    let status = request(&tx, &replies, "RECOVERY_STATUS", json!({})).await?;
                    ensure!(
                        status["recovery_only"] == true && status["trading_state"] == "HALTED",
                        "{status}"
                    );
                    publish_native_quote_size(&cache, "100.00", "0.25")?;
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    assert_eq!(execution.borrow().command_count(), before_execution);
                    assert_eq!(risk.borrow().command_count(), before_risk);
                    let mut restored = cache
                        .borrow()
                        .orders(None, None, None, None, None)
                        .iter()
                        .map(|o| {
                            (
                                o.client_order_id().to_string(),
                                o.strategy_id().to_string(),
                                o.status().to_string(),
                                o.quantity().to_string(),
                                o.filled_qty().to_string(),
                                o.events().len(),
                            )
                        })
                        .collect::<Vec<_>>();
                    restored.sort();
                    assert_eq!(original, restored);
                }
                stop.stop();
                Ok::<_, anyhow::Error>(())
            };
            let (run, result) = tokio::join!(native.run_with_mode(NodeRunMode::Hosted), drive);
            run?;
            result?;
            if seed {
                original = cache
                    .borrow()
                    .orders(None, None, None, None, None)
                    .iter()
                    .map(|o| {
                        (
                            o.client_order_id().to_string(),
                            o.strategy_id().to_string(),
                            o.status().to_string(),
                            o.quantity().to_string(),
                            o.filled_qty().to_string(),
                            o.events().len(),
                        )
                    })
                    .collect();
                original.sort();
                ensure!(original.len() == 3, "sandbox_bracket_not_three_orders");
            }
            native.dispose();
        }
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn recovery_rejects_persisted_emulated_bracket_before_node_start() -> Result<()> {
        use nautilus_common::{clock::VirtualClock, factories::OrderFactory};
        use nautilus_model::{enums::TriggerType, events::OrderCanceled};
        let directory = tempfile::tempdir()?;
        {
            let (mut native, _, _) = node(directory.path()).await?;
            assert!(!native.is_running());
            let mut orders = OrderFactory::new(
                TraderId::from("QZ-ACCOUNT-001"),
                StrategyId::from("OLD-001"),
                None,
                None,
                Rc::new(RefCell::new(VirtualClock::default())),
                false,
                true,
            )
            .bracket()
            .instrument_id(InstrumentId::from("BTC-USDC-SPOT.HYPERLIQUID"))
            .order_side(OrderSide::Buy)
            .quantity(Quantity::from("1.00"))
            .tp_price(Price::from("110.00"))
            .sl_trigger_price(Price::from("90.00"))
            .emulation_trigger(TriggerType::LastPrice)
            .call();
            let parent = &mut orders[0];
            parent.apply(OrderEventAny::Canceled(OrderCanceled::new(
                parent.trader_id(),
                parent.strategy_id(),
                parent.instrument_id(),
                parent.client_order_id(),
                UUID4::new(),
                1.into(),
                1.into(),
                false,
                None,
                None,
                None,
            )))?;
            assert!(parent.is_closed());
            for order in orders {
                native
                    .kernel()
                    .cache()
                    .borrow_mut()
                    .add_order(order, None, None, false)?;
            }
            assert!(
                native
                    .kernel()
                    .cache()
                    .borrow()
                    .orders_emulated_count(None, None, None, None, None)
                    > 0
            );
            native.dispose();
        }
        let path = directory.path().join("native-cache/orders");
        let persisted = || -> Result<Vec<(std::path::PathBuf, Vec<u8>)>> {
            let mut files = std::fs::read_dir(&path)?
                .map(|e| e.map(|e| e.path()))
                .collect::<std::io::Result<Vec<_>>>()?;
            files.sort();
            files
                .into_iter()
                .map(|p| Ok((p.clone(), std::fs::read(p)?)))
                .collect()
        };
        let before = persisted()?;
        let result = node(directory.path()).await;
        let error = match result {
            Ok(_) => bail!("emulated recovery unexpectedly started"),
            Err(e) => e,
        };
        ensure!(
            error
                .to_string()
                .contains("account_native_recovery_emulated_orders_require_explicit_handling"),
            "{error:#}"
        );
        assert_eq!(before, persisted()?);
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn native_strategy_cache_failure_halts_before_submission() -> Result<()> {
        let directory = tempfile::tempdir()?;
        let (mut native, tx, replies) = node(directory.path()).await?;
        let handle = native.handle();
        let cache = native.kernel().cache();
        let risk = native.kernel().risk_engine().clone();
        let stop = handle.clone();
        let drive = async {
            while !handle.is_running() {
                tokio::time::sleep(Duration::from_millis(5)).await;
            }
            publish_native_quote(&cache, "100.00")?;
            tokio::time::sleep(Duration::from_millis(30)).await;
            let mut normal = control("1", "NORMAL", "0");
            normal["exit_ids"] = json!([]);
            request(&tx, &replies, "CONTROL", normal).await?;
            request(&tx, &replies, "STRATEGY_START", json!({"epoch":"1"})).await?;
            request(&tx, &replies, "TEST_RESTING_LIMIT", json!({})).await?;
            tokio::time::sleep(Duration::from_millis(50)).await;
            ensure!(
                cache
                    .borrow()
                    .order(&ClientOrderId::from("fault-resting"))
                    .is_some_and(|o| o.status() == nautilus_model::enums::OrderStatus::Accepted),
                "resting_order_not_accepted"
            );
            // A real backing-store failure, not a synthetic successful response.
            // Preserve existing test files while obstructing the orders directory.
            let orders = directory.path().join("native-cache/orders");
            std::fs::rename(&orders, directory.path().join("saved-orders"))?;
            std::fs::write(&orders, b"unwritable order directory")?;
            for price in [
                "100.00", "99.00", "98.00", "97.00", "103.00", "106.00", "90.00", "110.00",
            ] {
                publish_native_quote(&cache, price)?;
                tokio::time::sleep(Duration::from_millis(40)).await;
            }
            ensure!(
                risk.borrow().trading_state() == nautilus_model::enums::TradingState::Halted,
                "risk_not_halted"
            );
            {
                let cached = cache.borrow();
                let orders = cached.orders(None, None, None, None, None);
                ensure!(
                    orders
                        .iter()
                        .any(|o| o.strategy_id().to_string().starts_with("EMA_CROSS")),
                    "official_strategy_did_not_attempt_order"
                );
                ensure!(
                    orders
                        .iter()
                        .filter(|o| o.client_order_id() != ClientOrderId::from("fault-resting"))
                        .all(|o| o.ts_submitted().is_none() && o.filled_qty().is_zero()),
                    "cache_failure_reached_execution"
                );
                ensure!(
                    cached.positions(None, None, None, None, None).is_empty(),
                    "cache_failure_opened_position"
                );
            }
            request(&tx, &replies, "TEST_MODIFY_LIMIT", json!({})).await?;
            tokio::time::sleep(Duration::from_millis(40)).await;
            ensure!(
                cache
                    .borrow()
                    .order(&ClientOrderId::from("fault-resting"))
                    .unwrap()
                    .price()
                    == Some(Price::from("1.00")),
                "halted_order_modified"
            );
            request(&tx, &replies, "TEST_CANCEL_LIMIT", json!({})).await?;
            tokio::time::sleep(Duration::from_millis(40)).await;
            ensure!(
                cache
                    .borrow()
                    .order(&ClientOrderId::from("fault-resting"))
                    .unwrap()
                    .status()
                    == nautilus_model::enums::OrderStatus::Canceled,
                "halted_cancel_blocked"
            );
            let status = request(&tx, &replies, "SNAPSHOT", json!({})).await?;
            ensure!(
                status["recovery_required"] == true && status["fenced"] == true,
                "fault_not_exposed"
            );
            let exit = request(&tx, &replies, "SLICE", json!({"epoch":"1"})).await?;
            ensure!(
                exit["admission"] == "REJECTED"
                    && exit["submitted"] == false
                    && exit["reason"] == "capital_exit_planning_retired",
                "retired_exit_was_not_rejected"
            );
            let stopped = request(&tx, &replies, "STRATEGY_STOP", json!({})).await?;
            ensure!(stopped["state"] == "STOPPED", "explicit_stop_unavailable");
            let error = request(&tx, &replies, "STRATEGY_START", json!({"epoch":"1"})).await?;
            ensure!(
                error["admission"] == "REJECTED"
                    && error["reason"] == "account_native_cache_write_failed",
                "fault_allowed_restart"
            );
            Ok::<_, anyhow::Error>(())
        };
        let controlled = async {
            let result = drive.await;
            stop.stop();
            result
        };
        let (run, result) = tokio::join!(native.run_with_mode(NodeRunMode::Hosted), controlled);
        run?;
        result?;
        native.dispose();
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn compiled_strategy_explicit_start_stop_capital_pause_and_restart_warmup() -> Result<()>
    {
        let directory = tempfile::tempdir()?;
        let mut prior_orders = 0;
        for restart in [false, true] {
            let (mut native, tx, replies) = node(directory.path()).await?;
            let handle = native.handle();
            let cache = native.kernel().cache();
            let drive = async {
                while !handle.is_running() {
                    tokio::time::sleep(Duration::from_millis(5)).await;
                }
                let recovery = request(&tx, &replies, "RECOVERY_STATUS", json!({})).await?;
                ensure!(
                    recovery["recovery_only"] == true
                        && recovery["signal_strategy_id"].is_null()
                        && recovery["trading_state"] == "HALTED",
                    "recovery_node_not_unregistered_and_halted: {recovery}"
                );
                let initial = request(&tx, &replies, "STRATEGY_STATUS", json!({})).await?;
                ensure!(
                    initial["state"] == "UNREGISTERED"
                        && initial["strategy_id"].is_null()
                        && initial["recovery_only"] == true,
                    "native_strategy_boot_not_paused: {initial}"
                );
                ensure!(
                    cache.borrow().orders(None, None, None, None, None).len() == prior_orders,
                    "native_orders_not_restored"
                );
                publish_native_quote(&cache, "100.00")?;
                tokio::time::sleep(Duration::from_millis(30)).await;
                ensure!(
                    cache.borrow().orders(None, None, None, None, None).len() == prior_orders,
                    "paused_strategy_traded"
                );
                let mut normal = control("1", "NORMAL", "0");
                normal["exit_ids"] = json!([]);
                request(&tx, &replies, "CONTROL", normal).await?;
                let started =
                    request(&tx, &replies, "STRATEGY_START", json!({"epoch":"1"})).await?;
                ensure!(
                    started["state"] == "RUNNING",
                    "native_strategy_start_failed: {started}"
                );
                publish_native_quote(&cache, "100.00")?;
                tokio::time::sleep(Duration::from_millis(30)).await;
                ensure!(
                    cache.borrow().orders(None, None, None, None, None).len() == prior_orders,
                    "native_restart_skipped_official_warmup"
                );
                if !restart {
                    for price in ["99.00", "98.00", "97.00", "103.00", "106.00"] {
                        publish_native_quote(&cache, price)?;
                        tokio::time::sleep(Duration::from_millis(40)).await;
                    }
                    let count = cache.borrow().orders(None, None, None, None, None).len();
                    ensure!(count > prior_orders, "official_ema_did_not_submit");
                    ensure!(
                        cache
                            .borrow()
                            .orders(None, None, None, None, None)
                            .iter()
                            .all(|o| o.strategy_id().to_string().starts_with("EMA_CROSS")),
                        "controller_generated_signal_orders"
                    );
                    prior_orders = count;
                    let stopped = request(&tx, &replies, "STRATEGY_STOP", json!({})).await?;
                    ensure!(stopped["state"] == "STOPPED", "native_stop_failed");
                    publish_native_quote(&cache, "1.00")?;
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    ensure!(
                        cache.borrow().orders(None, None, None, None, None).len() == prior_orders,
                        "stopped_strategy_traded"
                    );
                    let before_reset_orders = cache
                        .borrow()
                        .orders(None, None, None, None, None)
                        .iter()
                        .map(|o| (o.client_order_id(), o.status()))
                        .collect::<Vec<_>>();
                    let before_reset_positions = cache
                        .borrow()
                        .positions(None, None, None, None, None)
                        .iter()
                        .map(|p| (p.id, p.quantity))
                        .collect::<Vec<_>>();
                    request(&tx, &replies, "STRATEGY_START", json!({"epoch":"1"})).await?;
                    ensure!(
                        cache
                            .borrow()
                            .orders(None, None, None, None, None)
                            .iter()
                            .map(|o| (o.client_order_id(), o.status()))
                            .collect::<Vec<_>>()
                            == before_reset_orders,
                        "reset_changed_cached_orders"
                    );
                    ensure!(
                        cache
                            .borrow()
                            .positions(None, None, None, None, None)
                            .iter()
                            .map(|p| (p.id, p.quantity))
                            .collect::<Vec<_>>()
                            == before_reset_positions,
                        "reset_changed_cached_positions"
                    );
                    request(
                        &tx,
                        &replies,
                        "CONTROL",
                        control("2", "NATURAL_RELEASE", "100"),
                    )
                    .await?;
                    let stopped = request(&tx, &replies, "STRATEGY_STATUS", json!({})).await?;
                    ensure!(
                        stopped["state"] == "STOPPED",
                        "capital_control_did_not_pause_strategy"
                    );
                    let rejected =
                        request(&tx, &replies, "STRATEGY_START", json!({"epoch":"2"})).await?;
                    ensure!(
                        rejected["admission"] == "REJECTED"
                            && rejected["reason"] == "account_native_strategy_capital_reserved",
                        "reserved_capital_not_protected"
                    );
                    let retired = request(&tx, &replies, "TARGET", json!({}))
                        .await
                        .unwrap_err()
                        .to_string();
                    ensure!(
                        retired == "account_native_target_execution_retired",
                        "legacy_target_admitted"
                    );
                    request(
                        &tx,
                        &replies,
                        "CONTROL",
                        control("3", "BOUNDED_LIMIT", "1000"),
                    )
                    .await?;
                    publish_native_quote(&cache, "100.00")?;
                    tokio::time::sleep(Duration::from_millis(30)).await;
                    let before = cache
                        .borrow()
                        .positions(None, None, None, None, None)
                        .into_iter()
                        .filter(|p| p.is_open())
                        .map(|p| p.quantity.as_decimal())
                        .sum::<Decimal>();
                    let slice = request(&tx,&replies,"SLICE",json!({"epoch":"3","exit_id":"exit-original",
                        "client_order_id":"native-strategy-capital-exit","instrument_id":"BTC-USDC-SPOT.HYPERLIQUID",
                        "quantity":"0.25","minimum_price":"100.00","reference_bid":"100.00",
                        "remaining_cost_budget":"10","fee_rate_bound":"0.001","maximum_slice_notional":"25",
                        "maximum_participation":"0.1"})).await?;
                    ensure!(
                        slice["submitted"] == false
                            && slice["admission"] == "REJECTED"
                            && slice["reason"] == "capital_exit_planning_retired",
                        "retired_capital_slice_was_admitted: {slice}"
                    );
                    tokio::time::sleep(Duration::from_millis(80)).await;
                    let positions = cache
                        .borrow()
                        .positions(None, None, None, None, None)
                        .into_iter()
                        .map(|p| (p.is_short(), p.is_open(), p.quantity.as_decimal()))
                        .collect::<Vec<_>>();
                    ensure!(
                        positions.iter().all(|p| !p.0),
                        "capital_controller_opened_separate_short"
                    );
                    let after = positions
                        .iter()
                        .filter(|p| p.1)
                        .map(|p| p.2)
                        .sum::<Decimal>();
                    ensure!(
                        after == before,
                        "retired_capital_slice_changed_original_position: before={before} after={after} orders={:?}",
                        cache.borrow().orders(None, None, None, None, None)
                    );
                    prior_orders = cache.borrow().orders(None, None, None, None, None).len();
                }
                Ok::<_, anyhow::Error>(())
            };
            let stop = handle.clone();
            let controlled = async {
                let result = drive.await;
                stop.stop();
                result
            };
            let (run, result) = tokio::join!(native.run_with_mode(NodeRunMode::Hosted), controlled);
            run?;
            result?;
            native.dispose();
            drop(cache);
            drop(native);
        }
        Ok(())
    }
}
