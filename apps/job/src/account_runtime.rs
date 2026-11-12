//! Account execution adapter hosted by the existing remote worker. Business
//! intents stay in TypeScript; every financial call below uses the official node.
//!
//! Current account strategy wiring: the upstream EMA_CROSS example only. The same
//! registry constructs it for replay. BOOT parks it before live dispatch; explicit
//! START uses upstream reset/start, so indicators cold-warm rather than retaining
//! an arbitrary strategy's private state. STOP keeps upstream semantics.
//! OrderBookImbalance is replay-only here: its on_stop closes positions, so parking
//! it after LiveNode auto-start would violate the no-automatic-trading boot policy.
//!
//! Capital exits use the controller's upstream HEDGING OMS and an explicit existing
//! position ID, with reduce-only orders. Only a unique long position is supported;
//! ambiguous/short positions are rejected. Sandbox evidence is not a claim of live
//! exchange execution validation, nor generic strategy recovery compatibility.
use crate::{account_observer::project_snapshot, account_runtime_cache::AccountCacheFactory};
use anyhow::{Context, Result, anyhow, bail, ensure};
use contracts::{Id, native_strategy::NativeCompiledStrategyV1};
use indexmap::IndexMap;
use nautilus_common::{
    actor::DataActor,
    cache::CacheConfig,
    component::{component_state, reset_component, stop_component},
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
    HyperliquidDataClientConfig, HyperliquidDataClientFactory, HyperliquidExecutionClientConfig,
    HyperliquidExecutionClientFactory,
    common::{
        consts::HYPERLIQUID_CLIENT_ID,
        enums::{HyperliquidEnvironment, HyperliquidProductType},
    },
    http::client::HyperliquidHttpClient,
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
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
    num::NonZeroUsize,
    path::PathBuf,
    str::FromStr,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{self, Receiver},
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
impl Request {
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
struct AccountController {
    core: StrategyCore,
    signal_strategy_id: Option<StrategyId>,
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
    stamp: String,
    last_tick_ns: u64,
    last_slice_ns: u64,
    submission_attempted: bool,
    cache_failed: Arc<AtomicBool>,
    spot_facts: Option<SpotFacts>,
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
        // Trader starts registered components synchronously before live data dispatch.
        // The official signal strategy is registered first; park it before returning
        // startup readiness. Only an explicit STRATEGY_START may resume it.
        if let Some(id) = self.signal_strategy_id {
            if component_state(&id.inner())? == ComponentState::Running {
                stop_component(&id.inner())?;
            }
        }
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
                json!({"ready":true,"fenced":true,"native_version":crate::NAUTILUS_VERSION,"native_session_id":self.session,"recovery":"OFFICIAL_CACHE_AND_RECONCILIATION"}),
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
impl AccountController {
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
        let cache = self.cache();
        let account_event = cache
            .account(&account)
            .and_then(|v| v.last_event())
            .ok_or_else(|| anyhow!("account_native_account_unavailable"))?;
        let mut stamp = account_event.event_id.to_string();
        for id in &self.ids {
            if let Some(q) = cache.quote(id) {
                stamp.push_str(&format!(":{}:{}", id, q.ts_init));
            }
            if let Some(book) = cache.order_book(id) {
                stamp.push_str(&format!(":book:{}", book.ts_last));
            }
        }
        let orders = cache.orders(None, None, None, Some(&account), None);
        for o in &orders {
            stamp.push_str(&format!(
                ":{}:{}:{}",
                o.client_order_id(),
                o.status(),
                o.filled_qty()
            ));
        }
        if stamp == self.stamp {
            return Ok(());
        }
        drop(cache);
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
        let orders = cache.orders(None, None, None, Some(&account), None);
        let mut fills = vec![];
        let mut order_rows:Vec<_>=orders.iter().map(|o|{
            for event in o.events() {if let OrderEventAny::Filled(f)=event {fills.push(json!({"trade_id":f.trade_id.to_string(),"event_id":f.event_id.to_string(),"client_order_id":f.client_order_id.to_string(),"instrument_id":f.instrument_id.to_string(),"side":f.order_side.to_string(),"quantity":f.last_qty.as_decimal().to_string(),"price":f.last_px.as_decimal().to_string(),"commission":f.commission.map(|v|v.as_decimal().to_string()),"commission_currency":f.commission.map(|v|v.currency.to_string()),"ts_event_ns":f.ts_event.as_u64().to_string()}));}}
            json!({"client_order_id":o.client_order_id().to_string(),"venue_order_id":o.venue_order_id().map(|v|v.to_string()),"instrument_id":o.instrument_id().to_string(),"side":o.order_side().to_string(),
                "quantity":o.quantity().as_decimal().to_string(),"filled_quantity":o.filled_qty().as_decimal().to_string(),"leaves_quantity":o.leaves_qty().as_decimal().to_string(),"status":o.status().to_string(),
                "limit_price":o.price().map(|v|v.as_decimal().to_string()),"reduce_only":o.is_reduce_only(),"exit_id":exit_id(o.tags()),
                "order_type":o.order_type().to_string(),"trigger_price":o.trigger_price().map(|v|v.as_decimal().to_string()),
                "order_list_id":o.order_list_id().map(|v|v.to_string()),"parent_order_id":o.parent_order_id().map(|v|v.to_string()),
                "contingency_type":o.contingency_type().map(|v|v.to_string()),
                "protection_target_id":o.tags().and_then(|tags|tags.iter().find_map(|tag|tag.as_str().strip_prefix("QZ-PROTECTION:").map(str::to_owned)))})
        }).collect();
        let mut positions:Vec<_>=cache.positions(None,None,None,Some(&account),None).iter().filter(|p|p.is_open()).map(|p|{
            let quote=cache.quote(&p.instrument_id);
            let value=quote.and_then(|q|p.quantity.as_decimal().checked_mul(q.bid_price.as_decimal()));
            let pnl=quote.and_then(|q|p.unrealized_pnl(q.bid_price).ok());
            let sell_orders=orders.iter().filter(|o|!o.is_closed()&&o.instrument_id()==p.instrument_id&&o.order_side()==OrderSide::Sell).map(|o|o.leaves_qty().as_decimal()).sum::<Decimal>();
            json!({"instrument_id":p.instrument_id.to_string(),"quantity":p.signed_decimal_qty().to_string(),"free_quantity":(p.quantity.as_decimal()-sell_orders).max(Decimal::ZERO).to_string(),"value":value.map(|v|v.to_string()),"pnl":pnl.map(|v|v.as_decimal().to_string())})
        }).collect();
        order_rows.sort_by_key(|v| v["client_order_id"].as_str().unwrap_or_default().to_owned());
        positions.sort_by_key(|v| v["instrument_id"].as_str().unwrap_or_default().to_owned());
        fills.sort_by_key(|v| {
            (
                v["ts_event_ns"].as_str().unwrap_or_default().to_owned(),
                v["trade_id"].as_str().unwrap_or_default().to_owned(),
            )
        });
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
            "native_session_id":self.session,"native_trader_id":self.trader,"native_account_id":account.to_string(),"native_snapshot":projected,"positions":positions,"instruments":instruments,"orders":order_rows,"fills":fills,"cash":cash,"equity":equity,"currency":"USDC",
            "native_version":crate::NAUTILUS_VERSION,"currency_precision":Currency::USDC().precision,"control_epoch":self.control.as_ref().map(|c|c.epoch.to_string()),"control_revision":self.control.as_ref().map(|c|c.revision.clone()),"fenced":self.control.as_ref().is_none_or(|c|c.mode=="PAUSED")||self.cache_failed.load(Ordering::SeqCst),
            "remaining_trading":"NATIVE_STRATEGY_EXPLICIT_LIFECYCLE"}),
        );
        self.stamp = stamp;
        Ok(())
    }
    fn process(&mut self, r: &Request) -> Result<Value> {
        match r.operation.as_str() {
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
                    self.native_trader.borrow().start_strategy(&id)?;
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
            "SNAPSHOT" => {
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
                Ok(frame)
            }
            "FEE_REFRESH" => {
                let rate = r.decimal("rate")?;
                ensure!(rate >= Decimal::ZERO, "account_native_fee_positive");
                self.fee_rate = rate;
                self.fee_source = r.value("source").clone();
                let asof = r.text("asof_ns")?.parse::<u64>()?;
                self.fee_valid_until_ns = asof.saturating_add(60_000_000_000);
                self.stamp.clear();
                Ok(json!({"updated":true}))
            }
            "CONTROL" => self.apply_control(r),
            "SLICE" => {
                self.submission_attempted = false;
                match self.submit_slice(r) {
                    Ok(v) => Ok(v),
                    Err(e) if !self.submission_attempted => {
                        Ok(json!({"admission":"REJECTED","submitted":false,"reason":e.to_string()}))
                    }
                    Err(_) => Err(anyhow!("account_native_submit_outcome_uncertain")),
                }
            }
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
    fn strategy_status(&self) -> Result<Value> {
        let id = self
            .signal_strategy_id
            .ok_or_else(|| anyhow!("account_native_strategy_not_configured"))?;
        Ok(
            json!({"strategy_id":id.to_string(),"state":format!("{:?}",component_state(&id.inner())?).to_ascii_uppercase(),
            "native_version":crate::NAUTILUS_VERSION,"instrument_id":self.config.native_strategy.instrument_id,
            "config":self.config.native_strategy.strategy,"default_start":"PAUSED",
            "restart_warmup":"OFFICIAL_STRATEGY_REINITIALIZES_INDICATORS","start_semantics":"UPSTREAM_RESET_THEN_START"}),
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
                    self.stamp.clear();
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
        self.last_slice_ns = self.last_slice_ns.max(
            self.cache()
                .orders(None, None, None, Some(&self.account), None)
                .iter()
                .filter(|o| exit_id(o.tags()).is_some())
                .map(|o| o.ts_submitted().unwrap_or(o.ts_init()).as_u64())
                .max()
                .unwrap_or(0),
        );
        let pending: Vec<_> = self
            .cache()
            .orders(None, None, None, Some(&self.account), None)
            .iter()
            .filter(|o| !o.is_closed() && o.order_side() == OrderSide::Buy)
            .map(|o| o.client_order_id())
            .collect();
        for id in pending {
            self.cancel_order(id, Some(self.client), None)?;
        }
        self.stamp.clear();
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
    fn quote_and_instrument(&self, id: InstrumentId) -> Result<(InstrumentAny, QuoteTick)> {
        ensure!(
            self.ids.contains(&id),
            "account_native_instrument_out_of_scope"
        );
        let cache = self.cache();
        let instrument = cache
            .instrument(&id)
            .ok_or_else(|| anyhow!("account_native_instrument_unavailable"))?;
        ensure!(
            matches!(&instrument, InstrumentAny::CurrencyPair(_))
                && instrument.quote_currency() == Currency::USDC(),
            "account_native_spot_usdc_required"
        );
        let quote = cache
            .quote(&id)
            .ok_or_else(|| anyhow!("account_native_quote_unavailable"))?;
        let now = self.clock().timestamp_ns().as_u64();
        ensure!(
            quote.ts_init.as_u64() <= now && now - quote.ts_init.as_u64() <= 5_000_000_000,
            "account_native_quote_stale"
        );
        Ok((instrument, quote))
    }
    fn submit_slice(&mut self, r: &Request) -> Result<Value> {
        let epoch = r.text("epoch")?.parse()?;
        let c = self.fence(epoch)?.clone();
        ensure!(
            c.mode == "BOUNDED_LIMIT"
                && c.exits.iter().any(|e| e == r.text("exit_id").unwrap_or("")),
            "account_native_exit_control_mismatch"
        );
        let now = self.clock().timestamp_ns().as_u64();
        ensure!(
            self.spot_cash()? < c.reserved + c.buffer,
            "account_native_cash_goal_already_reached"
        );
        ensure!(
            now < self.fee_valid_until_ns,
            "account_native_spot_fee_evidence_expired"
        );
        let deadline = r
            .value("deadline")
            .as_str()
            .map(deadline_ns)
            .transpose()?
            .into_iter()
            .chain(c.deadline)
            .min();
        ensure!(
            deadline.is_none_or(|d| now < d),
            "account_native_exit_deadline"
        );
        let minimum_interval =
            interval_ns(r.value("minimum_interval_seconds"))?.max(c.minimum_interval_ns);
        ensure!(
            now.saturating_sub(self.last_slice_ns) >= minimum_interval,
            "account_native_exit_pacing"
        );
        let id = InstrumentId::from_str(r.text("instrument_id")?)?;
        let client = ClientOrderId::new_checked(r.text("client_order_id")?)?;
        if let Some(order) = self.cache().order(&client) {
            return Ok(
                json!({"submitted":false,"already_observed":true,"client_order_id":client.to_string(),"status":order.status().to_string()}),
            );
        }
        ensure!(
            !self
                .cache()
                .orders(None, None, None, Some(&self.account), None)
                .iter()
                .any(|o| !o.is_closed() && exit_id(o.tags()).is_some()),
            "account_native_previous_exit_order_unsettled"
        );
        ensure!(
            self.protection_orders(None).is_empty(),
            "account_native_cancel_protection_before_exit"
        );
        let (instrument, quote) = self.quote_and_instrument(id)?;
        let quantity = r.decimal("quantity")?;
        let floor = r.decimal("minimum_price")?;
        ensure!(
            quantity > Decimal::ZERO
                && floor > Decimal::ZERO
                && quote.bid_price.as_decimal() >= floor,
            "account_native_exit_price_or_quantity"
        );
        let (holding, position_id) = {
            let cache = self.cache();
            let positions = cache.positions(None, Some(&id), None, Some(&self.account), None);
            let open = positions.iter().filter(|p| p.is_open()).collect::<Vec<_>>();
            ensure!(
                open.len() == 1 && open[0].is_long(),
                "account_native_exit_position_selection_required"
            );
            (open[0].quantity.as_decimal(), open[0].id)
        };
        let sold_reserved = self
            .cache()
            .orders(
                None,
                Some(&id),
                None,
                Some(&self.account),
                Some(OrderSide::Sell),
            )
            .iter()
            .filter(|o| !o.is_closed())
            .map(|o| o.leaves_qty().as_decimal())
            .sum::<Decimal>();
        ensure!(
            quantity <= holding - sold_reserved,
            "account_native_protective_or_inventory_overlap"
        );
        let qty = instrument.try_make_qty_from_decimal(quantity, Some(true))?;
        ensure!(
            qty.as_decimal() == quantity,
            "account_native_quantity_precision"
        );
        let limit = valid_sell_floor(
            floor,
            instrument.price_precision(),
            instrument.size_precision(),
        )?;
        let price = instrument.try_make_price_from_decimal(limit)?;
        ensure!(
            price.as_decimal() >= floor,
            "account_native_price_precision"
        );
        ensure!(
            quote.bid_price.as_decimal() >= price.as_decimal(),
            "account_native_price_floor_unavailable"
        );
        let reference = r.decimal("reference_bid")?;
        let remaining_cost = r.decimal("remaining_cost_budget")?;
        let fee_bound = r.decimal("fee_rate_bound")?;
        let slice_notional = r.decimal("maximum_slice_notional")?;
        ensure!(
            reference > Decimal::ZERO
                && remaining_cost >= Decimal::ZERO
                && fee_bound >= self.fee_rate
                && fee_bound < Decimal::ONE
                && slice_notional > Decimal::ZERO,
            "account_native_cost_authorization_invalid_or_fee_changed"
        );
        let notional = quantity
            .checked_mul(quote.bid_price.as_decimal())
            .ok_or_else(|| anyhow!("account_native_decimal_range"))?;
        ensure!(
            notional <= slice_notional,
            "account_native_slice_notional_limit"
        );
        let loss = maximum_slice_loss(
            quantity,
            reference,
            price.as_decimal(),
            fee_bound,
            instrument.size_increment().as_decimal(),
        )?;
        ensure!(
            loss.max(Decimal::ZERO) <= remaining_cost,
            "account_native_remaining_cost_exceeded"
        );
        ensure!(
            instrument.min_quantity().is_none_or(|min| qty >= min)
                && instrument
                    .min_notional()
                    .is_none_or(|min| qty.as_decimal() * price.as_decimal() >= min.as_decimal()),
            "account_native_minimum_trade_size"
        );
        let participation = r
            .value("maximum_participation")
            .as_str()
            .map(Decimal::from_str)
            .transpose()?
            .unwrap_or(Decimal::new(1, 1));
        ensure!(
            participation > Decimal::ZERO && participation <= Decimal::ONE,
            "account_native_participation"
        );
        let depth = {
            let cache = self.cache();
            let book = cache
                .order_book(&id)
                .ok_or_else(|| anyhow!("account_native_book_required"))?;
            ensure!(
                book.ts_last.as_u64() <= now && now - book.ts_last.as_u64() <= 5_000_000_000,
                "account_native_book_stale"
            );
            book.bids(Some(20))
                .filter(|l| l.price.value.as_decimal() >= price.as_decimal())
                .map(|l| l.size_decimal())
                .sum::<Decimal>()
        };
        ensure!(
            quantity <= depth * participation,
            "account_native_depth_participation"
        );
        self.fence(epoch)?;
        let order = self.order_factory().limit(
            id,
            OrderSide::Sell,
            qty,
            price,
            Some(TimeInForce::Ioc),
            None,
            Some(false),
            Some(true),
            Some(false),
            None,
            None,
            None,
            None,
            None,
            Some(vec![Ustr::from(&format!(
                "QZ-CAPITAL-EXIT:{}",
                r.text("exit_id")?
            ))]),
            Some(client),
        );
        self.submission_attempted = true;
        self.last_slice_ns = now;
        self.submit_order(order, Some(position_id), Some(self.client), None)?;
        self.stamp.clear();
        Ok(
            json!({"submitted":true,"client_order_id":client.to_string(),"epoch":epoch.to_string(),"acknowledgement":"NATIVE_SUBMIT_ONLY"}),
        )
    }
    fn cancel_exit(&mut self, r: &Request) -> Result<Value> {
        let epoch = r.text("epoch")?.parse::<u64>()?;
        let c = self
            .control
            .as_ref()
            .ok_or_else(|| anyhow!("account_native_budget_fence_not_applied"))?;
        ensure!(c.epoch == epoch, "account_native_obsolete_control_epoch");
        let exit = r.text("exit_id")?;
        let ids: Vec<_> = self
            .cache()
            .orders(None, None, None, Some(&self.account), None)
            .iter()
            .filter(|o| !o.is_closed() && exit_id(o.tags()).as_deref() == Some(exit))
            .map(|o| o.client_order_id())
            .collect();
        for id in &ids {
            self.cancel_order(*id, Some(self.client), None)?;
        }
        Ok(json!({"cancellation_requested":ids.len(),"settled":ids.is_empty()}))
    }
    fn protection_orders(&self, target: Option<&str>) -> Vec<ClientOrderId> {
        self.cache()
            .orders(None, None, None, None, None)
            .iter()
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
            .collect()
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
        self.stamp.clear();
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
    fn spot_cash(&self) -> Result<Decimal> {
        if self.config.mode == "live" {
            let facts = self
                .spot_facts
                .as_ref()
                .ok_or_else(|| anyhow!("account_native_spot_facts_required"))?;
            let now = self.clock().timestamp_ns().as_u64();
            ensure!(
                facts.asof <= now && now < facts.valid_until,
                "account_native_spot_facts_expired"
            );
            // A venue HTTP response can precede fills already queued in the
            // native event thread. The fact timestamp is the HTTP request's
            // start, so debit every subsequent native BUY fill conservatively.
            // Sales are credited only by the next real spot balance response.
            let mut spent = Decimal::ZERO;
            for order in self
                .cache()
                .orders(None, None, None, Some(&self.account), None)
            {
                for event in order.events() {
                    if let OrderEventAny::Filled(fill) = event {
                        if fill.order_side == OrderSide::Buy && fill.ts_init.as_u64() >= facts.asof
                        {
                            let fee = fill.commission.ok_or_else(|| {
                                anyhow!("account_native_buy_fee_evidence_required")
                            })?;
                            let debit = fill
                                .last_qty
                                .as_decimal()
                                .checked_mul(fill.last_px.as_decimal())
                                .and_then(|v| {
                                    v.checked_add(if fee.currency == Currency::USDC() {
                                        fee.as_decimal()
                                    } else {
                                        Decimal::ZERO
                                    })
                                })
                                .ok_or_else(|| anyhow!("account_native_decimal_range"))?;
                            spent = spent
                                .checked_add(debit)
                                .ok_or_else(|| anyhow!("account_native_decimal_range"))?;
                        }
                    }
                }
            }
            return Ok((facts.free - spent).max(Decimal::ZERO));
        }
        self.cache()
            .account(&self.account)
            .and_then(|a| a.last_event())
            .and_then(|a| {
                a.balances
                    .iter()
                    .find(|b| b.free.currency == Currency::USDC())
                    .map(|b| b.free.as_decimal())
            })
            .ok_or_else(|| anyhow!("account_native_cash_unavailable"))
    }
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
fn valid_sell_floor(floor: Decimal, price_precision: u8, size_precision: u8) -> Result<Decimal> {
    ensure!(floor > Decimal::ZERO, "account_native_price_positive");
    if floor.fract().is_zero() {
        return Ok(floor);
    }
    let magnitude = floor.mantissa().unsigned_abs().ilog10() as i32 - floor.scale() as i32;
    let decimals = (4 - magnitude)
        .min(i32::from(8_u8.saturating_sub(size_precision)))
        .min(i32::from(price_precision));
    let step = if decimals >= 0 {
        Decimal::ONE / Decimal::from(10_u64.pow(decimals as u32))
    } else {
        Decimal::from(10_u64.pow((-decimals) as u32))
    };
    // The venue permits arbitrary significant digits for integer prices.
    Ok(((floor / step).ceil() * step).min(floor.ceil()))
}
fn maximum_slice_loss(
    quantity: Decimal,
    reference: Decimal,
    floor: Decimal,
    fee_bound: Decimal,
    size_increment: Decimal,
) -> Result<Decimal> {
    let net_floor = floor
        .checked_mul(Decimal::ONE - fee_bound)
        .ok_or_else(|| anyhow!("account_native_decimal_range"))?;
    let rounding = fee_rounding_reserve(quantity, fee_bound, size_increment)?;
    quantity
        .checked_mul(reference - net_floor)
        .and_then(|v| v.checked_add(rounding))
        .map(|v| v.max(Decimal::ZERO))
        .ok_or_else(|| anyhow!("account_native_decimal_range"))
}
fn half_fee_quantum(fee_bound: Decimal) -> Decimal {
    if fee_bound.is_zero() {
        Decimal::ZERO
    } else {
        Decimal::new(1, u32::from(Currency::USDC().precision)) / Decimal::from(2)
    }
}
fn fee_rounding_reserve(
    quantity: Decimal,
    fee_bound: Decimal,
    size_increment: Decimal,
) -> Result<Decimal> {
    let maximum_fills = quantity
        .checked_div(size_increment)
        .ok_or_else(|| anyhow!("account_native_decimal_range"))?
        .ceil();
    half_fee_quantum(fee_bound)
        .checked_mul(maximum_fills)
        .ok_or_else(|| anyhow!("account_native_decimal_range"))
}
fn buy_cost_per_unit(
    price: Decimal,
    fee_bound: Decimal,
    size_increment: Decimal,
) -> Result<Decimal> {
    price
        .checked_mul(Decimal::ONE + fee_bound)
        .and_then(|v| {
            half_fee_quantum(fee_bound)
                .checked_div(size_increment)
                .and_then(|rounding| v.checked_add(rounding))
        })
        .ok_or_else(|| anyhow!("account_native_decimal_range"))
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

pub async fn run() -> Result<()> {
    let mut first = String::new();
    io::stdin().read_line(&mut first)?;
    let boot: Request = serde_json::from_str(&first)?;
    ensure!(boot.operation == "BOOT", "account_native_boot_required");
    let config: AccountRuntimeConfig = serde_json::from_value(boot.value("config").clone())?;
    let result = run_config(config, boot.id.clone()).await;
    if let Err(e) = &result {
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

async fn run_config(config: AccountRuntimeConfig, boot_id: String) -> Result<()> {
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
        !matches!(config.native_strategy.strategy, NativeCompiledStrategyV1::GridMarketMaker { .. }),
        "account_native_strategy_account_lifecycle_unsupported_for_grid_market_maker"
    );
    ensure!(
        matches!(
            config.native_strategy.strategy,
            NativeCompiledStrategyV1::EmaCross { .. }
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
    .with_cache_config(CacheConfig {
        flush_on_start: false,
        save_market_data: false,
        ..Default::default()
    })
    .with_risk_engine_config(config.risk.native(&config.instruments)?)
    .with_reconciliation(true)
    .with_exec_engine_config(LiveExecutionEngineConfig {
        load_cache: true,
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
        .install(node.kernel().cache(), trader, node.instance_id())
        .await?;
    install_cache_failure_risk_fence(&node, cache_failed.clone());
    let handle = node.handle();
    let session = node.instance_id().to_string();
    let (sender, inbox) = mpsc::sync_channel::<Request>(64);
    if config.mode == "live" {
        let address = config.account_address.clone().unwrap();
        let updates = sender.clone();
        tokio::spawn(async move {
            let Ok(http) = HyperliquidHttpClient::new(environment, 20, None) else {
                return;
            };
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                let Ok(source) = http.info_user_fees(&address).await else {
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
    let instrument_id: InstrumentId = config.native_strategy.instrument_id.parse()?;
    ensure!(
        ids.contains(&instrument_id),
        "account_native_strategy_instrument_not_configured"
    );
    let compiled = crate::native_strategy::registry::build_strategy(
        instrument_id,
        &config.native_strategy.strategy,
    )?;
    let native_managed_stop = match &compiled {
        crate::native_strategy::registry::CompiledNativeStrategy::EmaCross(s) => {
            s.strategy_core().config.manage_stop
        }
        crate::native_strategy::registry::CompiledNativeStrategy::GridMarketMaker(_) => anyhow::bail!("account_native_strategy_account_lifecycle_unsupported_for_grid_market_maker"),
        crate::native_strategy::registry::CompiledNativeStrategy::OrderBookImbalance(s) => {
            s.strategy_core().config.manage_stop
        }
    };
    match compiled {
        crate::native_strategy::registry::CompiledNativeStrategy::EmaCross(strategy) => {
            node.add_strategy(strategy)?
        }
        crate::native_strategy::registry::CompiledNativeStrategy::GridMarketMaker(_) => anyhow::bail!("account_native_strategy_account_lifecycle_unsupported_for_grid_market_maker"),
        crate::native_strategy::registry::CompiledNativeStrategy::OrderBookImbalance(strategy) => {
            node.add_strategy(strategy)?
        }
    }
    let signal_strategy_id = *node
        .kernel()
        .trader
        .borrow()
        .strategy_ids()
        .last()
        .ok_or_else(|| anyhow!("account_native_strategy_registration_failed"))?;
    node.add_strategy(AccountController {
        signal_strategy_id: Some(signal_strategy_id),
        native_trader: node.kernel().trader.clone(),
        native_managed_stop,
        core: StrategyCore::new(StrategyConfig {
            // Sandbox dispatches execution events asynchronously. The official strategy
            // manager reconciles OTO quantities after fills reach the shared cache.
            // Do not double-manage Hyperliquid adapter-staged child orders.
            manage_contingent_orders: config.mode == "paper",
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
        stamp: String::new(),
        last_tick_ns: 0,
        last_slice_ns: 0,
        submission_attempted: false,
        cache_failed,
        spot_facts: None,
        target_acceptances: BTreeMap::new(),
        #[cfg(test)]
        replies: None,
    })?;
    node.run().await.context("account_native_node_run")
}

#[cfg(test)]
mod tests {
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
        assert!(
            serde_json::from_value::<super::AccountRiskConfig>(serde_json::json!({"bypass":true}))
                .is_err()
        );
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
        let error = super::run_config(config, "unused".into()).await.unwrap_err();
        assert_eq!(error.to_string(), "account_native_strategy_account_lifecycle_unsupported_for_grid_market_maker");
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
    use nautilus_core::{UUID4, UnixNanos};
    use nautilus_live::node::NodeRunMode;
    use nautilus_model::{
        data::{BookOrder, Data},
        identifiers::Symbol,
        instruments::CurrencyPair,
        orderbook::OrderBook,
        types::{Price, Quantity},
    };
    use std::{cell::RefCell, rc::Rc, time::Duration};

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
        let mut node = LiveNode::builder(trader, Environment::Sandbox)?
            .with_logging(LoggerConfig {
                bypass_logging: true,
                ..Default::default()
            })
            .with_load_state(true)
            .with_save_state(true)
            .with_cache_config(CacheConfig {
                flush_on_start: false,
                save_market_data: false,
                ..Default::default()
            })
            .with_reconciliation(true)
            .with_exec_engine_config(LiveExecutionEngineConfig {
                load_cache: true,
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
            .install(node.kernel().cache(), trader, node.instance_id())
            .await?;
        install_cache_failure_risk_fence(&node, factory.failed.clone());
        let (tx, rx) = mpsc::sync_channel(64);
        let replies = Rc::new(RefCell::new(BTreeMap::new()));
        let native_config = NativeCompiledStrategyV1::EmaCross {
            trade_size: "1".into(),
            fast_period: 2,
            slow_period: 3,
            base: None,
        };
        match crate::native_strategy::registry::build_strategy(ids[0], &native_config)? {
            crate::native_strategy::registry::CompiledNativeStrategy::EmaCross(strategy) => {
                node.add_strategy(strategy)?
            }
            _ => unreachable!(),
        }
        let signal_strategy_id = *node.kernel().trader.borrow().strategy_ids().last().unwrap();
        node.add_strategy(AccountController {
            signal_strategy_id: Some(signal_strategy_id),
            native_trader: node.kernel().trader.clone(),
            native_managed_stop: false,
            core: StrategyCore::new(StrategyConfig {
                manage_contingent_orders: true,
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
            stamp: String::new(),
            last_tick_ns: 0,
            last_slice_ns: 0,
            submission_attempted: false,
            cache_failed: factory.failed.clone(),
            spot_facts: None,
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
        let now = UnixNanos::from(chrono::Utc::now().timestamp_nanos_opt().unwrap() as u64);
        let id = InstrumentId::from("BTC-USDC-SPOT.HYPERLIQUID");
        let price = Price::from_str(price).map_err(anyhow::Error::msg)?;
        let size = Quantity::from("1000.00");
        let mut book = OrderBook::new(id, BookType::L2_MBP);
        book.add(BookOrder::new(OrderSide::Buy, price, size, 1), 0, 1, now);
        book.add(BookOrder::new(OrderSide::Sell, price, size, 2), 0, 2, now);
        cache.borrow_mut().add_order_book(book)?;
        get_data_event_sender().send(DataEvent::Data(Data::Quote(QuoteTick::new(
            id, price, price, size, size, now, now,
        ))))?;
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
            drop(orders);
            drop(cached);
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
                    && exit["reason"] == "account_native_cache_write_failed",
                "fault_did_not_reject_new_exit"
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
                let initial = request(&tx, &replies, "STRATEGY_STATUS", json!({})).await?;
                ensure!(
                    initial["state"] == "STOPPED",
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
                        slice["submitted"] == true,
                        "capital_slice_not_submitted: {slice}"
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
                        after == before - Decimal::new(25, 2),
                        "capital_slice_did_not_reduce_original_position: before={before} after={after} orders={:?}",
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

    #[test]
    fn hyperliquid_floor_never_rounds_below_authorization() -> Result<()> {
        for raw in ["0.0000123456", "1.2345678", "123.45678", "99999.9999"] {
            let floor = Decimal::from_str(raw)?;
            let price = valid_sell_floor(floor, 8, 2)?;
            ensure!(price >= floor, "sell_floor_rounded_down");
        }
        ensure!(
            valid_sell_floor(Decimal::from(100001), 8, 2)? == Decimal::from(100001),
            "integer_price_exception_lost"
        );
        ensure!(
            valid_sell_floor(Decimal::from_str("100000.1")?, 8, 2)? == Decimal::from(100001),
            "integer_price_exception_not_nearest_floor"
        );
        ensure!(
            maximum_slice_loss(
                Decimal::ONE,
                Decimal::from(100),
                Decimal::from(100),
                Decimal::ZERO,
                Decimal::ONE
            )?
            .is_zero(),
            "zero_fee_invented_rounding_cost"
        );
        ensure!(
            maximum_slice_loss(
                Decimal::ONE,
                Decimal::from(100),
                Decimal::from(101),
                Decimal::new(1, 3),
                Decimal::ONE
            )?
            .is_zero(),
            "favorable_price_failed_to_cover_fee_rounding"
        );
        ensure!(
            maximum_slice_loss(
                Decimal::new(1, 1),
                Decimal::from(100),
                Decimal::from(100),
                Decimal::new(1, 3),
                Decimal::new(1, 2)
            )? == Decimal::from_str("0.01000005")?,
            "per_fill_rounding_cost_bound_incorrect"
        );
        ensure!(
            buy_cost_per_unit(Decimal::from(100), Decimal::new(1, 3), Decimal::new(1, 2))?
                == Decimal::from_str("100.1000005")?,
            "buy_per_unit_rounding_reserve_incorrect"
        );
        ensure!(
            buy_cost_per_unit(Decimal::from(100), Decimal::ZERO, Decimal::new(1, 2))?
                == Decimal::from(100),
            "zero_fee_buy_invented_rounding_cost"
        );
        Ok(())
    }
}
