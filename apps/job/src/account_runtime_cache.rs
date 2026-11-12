//! Native Cache persistence in the existing worker account directory. Values are
//! official Nautilus objects; event application and recovery remain in Nautilus.
use ahash::AHashMap;
use anyhow::{anyhow, bail, ensure, Result};
use bytes::Bytes;
use nautilus_common::{
    cache::{
        database::{
            register_loaded_currencies, CacheDatabaseAdapter, CacheDatabaseFactory, CacheMap,
        },
        Cache, CacheConfig,
    },
    signal::Signal,
};
use nautilus_core::{UnixNanos, UUID4};
use nautilus_model::{
    accounts::AccountAny,
    data::{Bar, CustomData, DataType, FundingRateUpdate, InstrumentClose, QuoteTick, TradeTick},
    events::{position::snapshot::PositionSnapshot, OrderEventAny, OrderSnapshot},
    identifiers::{
        AccountId, ActorId, ClientId, ClientOrderId, InstrumentId, PositionId, StrategyId,
        TraderId, VenueOrderId,
    },
    instruments::{Instrument, InstrumentAny, SyntheticInstrument},
    orderbook::OrderBook,
    orders::{Order, OrderAny, OrderList},
    position::Position,
    types::{Currency, Money},
};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use serde_json::Value;
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs::{self, File, OpenOptions},
    path::{Path, PathBuf},
    rc::Rc,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
};
use ustr::Ustr;

#[derive(Debug, Clone)]
pub struct AccountCacheFactory {
    pub directory: PathBuf,
    pub failed: Arc<AtomicBool>,
}
impl AccountCacheFactory {
    pub fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            failed: Arc::new(AtomicBool::new(false)),
        }
    }
    /// Install the official backing under its existing exclusive lock before node
    /// startup, so linked-order indexes exist before an execution client connects.
    pub async fn install(
        &self,
        cache: Rc<RefCell<Cache>>,
        trader: TraderId,
        instance: UUID4,
        config: CacheConfig,
    ) -> Result<()> {
        let database = self.create(trader, instance, config).await?;
        let mut cache = cache
            .try_borrow_mut()
            .map_err(|_| anyhow!("account_native_cache_busy"))?;
        cache.set_database(database);
        restore_file_cache(&mut cache)?;
        cache.build_index();
        restore_order_list_index(&mut cache)?;
        Ok(())
    }
    pub fn restored_balances(&self, account: &AccountId) -> Result<Option<Vec<Money>>> {
        let root = self.directory.join("native-cache");
        if !root.exists() {
            return Ok(None);
        }
        let stored = load_state(&root)?;
        ensure_current_pin(&stored)?;
        if let Some(currencies) = stored.records.get("currencies") {
            for value in currencies.values() {
                let currency: Currency = serde_json::from_value(value.clone())?;
                Currency::register(currency, false)?;
            }
        }
        stored
            .records
            .get("accounts")
            .and_then(|rows| rows.get(&account.to_string()))
            .cloned()
            .map(|value| {
                let account: AccountAny = serde_json::from_value(value)?;
                Ok(account.balances().into_values().map(|b| b.total).collect())
            })
            .transpose()
    }
}

fn restore_file_cache(cache: &mut Cache) -> Result<()> {
    // Our file adapter's load_all and every load_* it awaits execute only
    // synchronous reads. Poll the official restoration once without yielding
    // a RefCell guard. This is NOT support for an asynchronous remote adapter:
    // an unexpected Pending aborts startup rather than running a partial cache.
    let loaded = {
        use std::{
            future::Future,
            task::{Context, Waker},
        };
        let mut future = std::pin::pin!(cache.cache_all());
        future
            .as_mut()
            .poll(&mut Context::from_waker(Waker::noop()))
    };
    match loaded {
        std::task::Poll::Ready(result) => result?,
        std::task::Poll::Pending => {
            bail!("account_native_file_cache_restore_unexpectedly_pending")
        }
    }
    Ok(())
}

/// OrderList is an index, not a second order state machine. Its source of truth
/// is the already durable official OrderAny objects and their original links.
/// Native cache reload does not reconstruct this index; adapters need it before
/// connect to restore staged bracket children. Never re-submit an order here.
fn restore_order_list_index(cache: &mut Cache) -> Result<()> {
    let mut groups = BTreeMap::new();
    for order in cache.orders(None, None, None, None, None) {
        if let Some(id) = order.order_list_id() {
            groups
                .entry(id)
                .or_insert_with(Vec::new)
                .push((*order).clone());
        }
    }
    for (id, mut orders) in groups {
        orders.sort_by_key(|order| {
            (
                order.parent_order_id().is_some(),
                order.ts_init(),
                order.client_order_id(),
            )
        });
        let first = &orders[0];
        for order in &orders {
            ensure!(
                order.instrument_id().venue == first.instrument_id().venue
                    && order.strategy_id() == first.strategy_id(),
                "account_native_order_list_inconsistent:{id}"
            );
            for linked in order
                .linked_order_ids()
                .into_iter()
                .flatten()
                .copied()
                .chain(order.parent_order_id())
            {
                ensure!(
                    orders
                        .iter()
                        .any(|candidate| candidate.client_order_id() == linked),
                    "account_native_order_list_member_missing:{id}:{linked}"
                );
            }
        }
        if orders.iter().all(Order::is_closed) {
            continue;
        }
        let list = OrderList::from_orders(&orders, first.ts_init());
        list.validate()?;
        if let Some(existing) = cache.order_list(&id) {
            ensure!(
                existing.instrument_id == list.instrument_id
                    && existing.strategy_id == list.strategy_id
                    && existing.client_order_ids == list.client_order_ids,
                "account_native_order_list_index_conflict:{id}"
            );
        } else {
            cache.add_order_list(list)?;
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct Stored {
    version: String,
    revision: String,
    trader: String,
    records: BTreeMap<String, BTreeMap<String, Value>>,
}
// A successful serde decode is not evidence that live recovery across a native
// revision is safe. In particular, never relabel old cache metadata or consume
// its balances before the same compatibility decision used by node startup.
fn ensure_current_pin(state: &Stored) -> Result<()> {
    ensure!(
        state.version == crate::NAUTILUS_VERSION && state.revision == crate::NAUTILUS_REVISION,
        "account_native_cache_version_unsupported:stored={}/{}:runtime={}/{}:original_cache_unchanged",
        state.version,
        state.revision,
        crate::NAUTILUS_VERSION,
        crate::NAUTILUS_REVISION
    );
    Ok(())
}

struct AccountCache {
    path: PathBuf,
    state: Mutex<Stored>,
    _lock: File,
    failed: Arc<AtomicBool>,
}
fn load_state(path: &Path) -> Result<Stored> {
    let mut state: Stored = serde_json::from_slice(&fs::read(path.join("metadata.json"))?)?;
    state.records.clear();
    for kind in fs::read_dir(path)? {
        let kind = kind?;
        if !kind.file_type()?.is_dir() {
            continue;
        }
        let name = kind
            .file_name()
            .into_string()
            .map_err(|_| anyhow!("account_native_cache_kind"))?;
        let rows = state.records.entry(name).or_default();
        for entry in fs::read_dir(kind.path())? {
            let entry = entry?;
            if entry.path().extension().is_none_or(|v| v != "json") {
                continue;
            }
            let row: Value = serde_json::from_slice(&fs::read(entry.path())?)?;
            let key = row["key"]
                .as_str()
                .ok_or_else(|| anyhow!("account_native_cache_key"))?;
            rows.insert(key.into(), row["value"].clone());
        }
    }
    Ok(state)
}
fn atomic_value(path: &Path, value: &impl Serialize) -> Result<()> {
    use std::io::Write;
    let parent = path
        .parent()
        .ok_or_else(|| anyhow!("account_native_cache_directory"))?;
    fs::create_dir_all(parent)?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    serde_json::to_writer(&mut temporary, value)?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(path).map_err(|e| e.error)?;
    File::open(parent)?.sync_all()?;
    Ok(())
}
impl std::fmt::Debug for AccountCache {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountCache")
            .field("path", &self.path)
            .finish()
    }
}

#[async_trait::async_trait]
impl CacheDatabaseFactory for AccountCacheFactory {
    async fn create(
        &self,
        trader_id: TraderId,
        _instance_id: UUID4,
        config: CacheConfig,
    ) -> Result<Box<dyn CacheDatabaseAdapter>> {
        ensure!(
            !config.flush_on_start,
            "account_native_cache_reset_forbidden"
        );
        fs::create_dir_all(&self.directory)?;
        let lock = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join("native-cache.lock"))?;
        lock.try_lock()
            .map_err(|_| anyhow!("account_native_owner_already_running"))?;
        let path = self.directory.join("native-cache");
        let state = if path.exists() {
            load_state(&path)?
        } else {
            let state = Stored {
                version: crate::NAUTILUS_VERSION.into(),
                revision: crate::NAUTILUS_REVISION.into(),
                trader: trader_id.to_string(),
                records: BTreeMap::new(),
            };
            atomic_value(&path.join("metadata.json"), &state)?;
            state
        };
        ensure_current_pin(&state)?;
        ensure!(
            state.trader == trader_id.to_string(),
            "account_native_cache_identity_changed"
        );
        Ok(Box::new(AccountCache {
            path,
            state: Mutex::new(state),
            _lock: lock,
            failed: self.failed.clone(),
        }))
    }
}
impl AccountCache {
    fn read<T: DeserializeOwned>(&self, kind: &str, key: &str) -> Result<Option<T>> {
        self.state
            .lock()
            .map_err(|_| anyhow!("account_native_cache_poisoned"))?
            .records
            .get(kind)
            .and_then(|rows| rows.get(key))
            .cloned()
            .map(serde_json::from_value)
            .transpose()
            .map_err(Into::into)
    }
    fn all<K: DeserializeOwned + Eq + std::hash::Hash, T: DeserializeOwned>(
        &self,
        kind: &str,
    ) -> Result<AHashMap<K, T>> {
        let state = self
            .state
            .lock()
            .map_err(|_| anyhow!("account_native_cache_poisoned"))?;
        state
            .records
            .get(kind)
            .into_iter()
            .flat_map(|rows| rows.iter())
            .map(|(k, v)| {
                Ok((
                    serde_json::from_value(Value::String(k.clone()))?,
                    serde_json::from_value(v.clone())?,
                ))
            })
            .collect()
    }
    fn file(&self, kind: &str, key: &str) -> PathBuf {
        let encoded = key
            .as_bytes()
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect::<String>();
        self.path.join(kind).join(format!("{encoded}.json"))
    }
    fn put<T: Serialize>(&self, kind: &str, key: impl ToString, value: &T) -> Result<()> {
        ensure!(
            !self.failed.load(Ordering::SeqCst),
            "account_native_cache_write_failed"
        );
        let key = key.to_string();
        let value = serde_json::to_value(value).inspect_err(|_| {
            self.failed.store(true, Ordering::SeqCst);
        })?;
        if let Err(e) = atomic_value(
            &self.file(kind, &key),
            &serde_json::json!({"key":key,"value":value}),
        ) {
            self.failed.store(true, Ordering::SeqCst);
            return Err(e);
        }
        self.state
            .lock()
            .map_err(|_| anyhow!("account_native_cache_poisoned"))?
            .records
            .entry(kind.into())
            .or_default()
            .insert(key, value);
        Ok(())
    }
    fn remove(&self, kind: &str, key: &str) -> Result<()> {
        let path = self.file(kind, key);
        if path.exists() {
            fs::remove_file(&path)?;
            File::open(path.parent().unwrap())?.sync_all()?;
        }
        if let Some(rows) = self
            .state
            .lock()
            .map_err(|_| anyhow!("account_native_cache_poisoned"))?
            .records
            .get_mut(kind)
        {
            rows.remove(key);
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl CacheDatabaseAdapter for AccountCache {
    fn close(&mut self) -> Result<()> {
        Ok(())
    }
    fn flush(&mut self) -> Result<()> {
        bail!("account_native_cache_reset_forbidden")
    }
    async fn load_all(&self) -> Result<CacheMap> {
        let mut currencies = self.load_currencies().await?;
        register_loaded_currencies(&mut currencies)?;
        Ok(CacheMap {
            currencies,
            instruments: self.load_instruments().await?,
            instrument_closes: self.load_instrument_closes().await?,
            synthetics: self.load_synthetics().await?,
            accounts: self.load_accounts().await?,
            orders: self.load_orders().await?,
            positions: self.load_positions().await?,
            ..Default::default()
        })
    }
    fn load(&self) -> Result<AHashMap<String, Bytes>> {
        self.all("raw")
    }
    async fn load_currencies(&self) -> Result<AHashMap<Ustr, Currency>> {
        self.all("currencies")
    }
    async fn load_instruments(&self) -> Result<AHashMap<InstrumentId, InstrumentAny>> {
        self.all("instruments")
    }
    async fn load_instrument_closes(&self) -> Result<AHashMap<InstrumentId, InstrumentClose>> {
        self.all("closes")
    }
    async fn load_synthetics(&self) -> Result<AHashMap<InstrumentId, SyntheticInstrument>> {
        self.all("synthetics")
    }
    async fn load_accounts(&self) -> Result<AHashMap<AccountId, AccountAny>> {
        self.all("accounts")
    }
    async fn load_orders(&self) -> Result<AHashMap<ClientOrderId, OrderAny>> {
        self.all("orders")
    }
    async fn load_positions(&self) -> Result<AHashMap<PositionId, Position>> {
        self.all("positions")
    }
    fn load_index_order_position(&self) -> Result<AHashMap<ClientOrderId, PositionId>> {
        self.all("order_position")
    }
    fn load_index_order_client(&self) -> Result<AHashMap<ClientOrderId, ClientId>> {
        self.all("order_client")
    }
    async fn load_currency(&self, code: &Ustr) -> Result<Option<Currency>> {
        self.read("currencies", code.as_str())
    }
    async fn load_instrument(&self, id: &InstrumentId) -> Result<Option<InstrumentAny>> {
        self.read("instruments", &id.to_string())
    }
    async fn load_synthetic(&self, id: &InstrumentId) -> Result<Option<SyntheticInstrument>> {
        self.read("synthetics", &id.to_string())
    }
    async fn load_account(&self, id: &AccountId) -> Result<Option<AccountAny>> {
        self.read("accounts", id.as_ref())
    }
    async fn load_order(&self, id: &ClientOrderId) -> Result<Option<OrderAny>> {
        self.read("orders", id.as_str())
    }
    async fn load_position(&self, id: &PositionId) -> Result<Option<Position>> {
        self.read("positions", id.as_ref())
    }
    fn load_actor(&self, id: &ActorId) -> Result<AHashMap<String, Bytes>> {
        let prefix = format!("{id}:");
        Ok(self
            .all::<String, Bytes>("actor_state")?
            .into_iter()
            .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|k| (k.into(), v)))
            .collect())
    }
    fn load_strategy(&self, id: &StrategyId) -> Result<AHashMap<String, Bytes>> {
        let prefix = format!("{id}:");
        Ok(self
            .all::<String, Bytes>("strategy_state")?
            .into_iter()
            .filter_map(|(k, v)| k.strip_prefix(&prefix).map(|k| (k.into(), v)))
            .collect())
    }
    fn load_signals(&self, name: &str) -> Result<Vec<Signal>> {
        Ok(self.read("signals", name)?.unwrap_or_default())
    }
    fn load_custom_data(&self, _: &DataType) -> Result<Vec<CustomData>> {
        bail!("account_native_custom_data_unsupported")
    }
    fn load_order_snapshot(&self, id: &ClientOrderId) -> Result<Option<OrderSnapshot>> {
        self.read("order_snapshots", id.as_str())
    }
    fn load_position_snapshot(&self, id: &PositionId) -> Result<Option<PositionSnapshot>> {
        self.read("position_snapshots", id.as_ref())
    }
    fn load_quotes(&self, id: &InstrumentId) -> Result<Vec<QuoteTick>> {
        Ok(self.read("quotes", &id.to_string())?.unwrap_or_default())
    }
    fn load_trades(&self, id: &InstrumentId) -> Result<Vec<TradeTick>> {
        Ok(self.read("trades", &id.to_string())?.unwrap_or_default())
    }
    fn load_funding_rates(&self, id: &InstrumentId) -> Result<Vec<FundingRateUpdate>> {
        Ok(self.read("funding", &id.to_string())?.unwrap_or_default())
    }
    fn load_bars(&self, id: &InstrumentId) -> Result<Vec<Bar>> {
        Ok(self.read("bars", &id.to_string())?.unwrap_or_default())
    }
    fn add(&self, key: String, value: Bytes) -> Result<()> {
        self.put("raw", key, &value)
    }
    fn add_currency(&self, v: &Currency) -> Result<()> {
        self.put("currencies", v.code, &v)
    }
    fn add_instrument(&self, v: &InstrumentAny) -> Result<()> {
        self.put("instruments", v.id(), v)
    }
    fn add_instrument_close(&self, v: &InstrumentClose) -> Result<()> {
        self.put("closes", v.instrument_id, v)
    }
    fn add_synthetic(&self, v: &SyntheticInstrument) -> Result<()> {
        self.put("synthetics", v.id, v)
    }
    fn add_account(&self, v: &AccountAny) -> Result<()> {
        self.put("accounts", v.id(), v)
    }
    fn add_order(&self, v: &OrderAny, client: Option<ClientId>) -> Result<()> {
        // Persist the routing index before the native order. A crash between
        // these writes leaves an unused index, rather than an unroutable order.
        if let Some(c) = client {
            self.put("order_client", v.client_order_id(), &c)?;
        }
        self.put("orders", v.client_order_id(), v)
    }
    fn add_order_snapshot(&self, v: &OrderSnapshot) -> Result<()> {
        self.put("order_snapshots", v.client_order_id, v)
    }
    fn add_position(&self, v: &Position) -> Result<()> {
        self.put("positions", v.id, v)
    }
    fn add_position_snapshot(&self, v: &PositionSnapshot) -> Result<()> {
        self.put("position_snapshots", v.position_id, v)
    }
    fn add_order_book(&self, _: &OrderBook) -> Result<()> {
        bail!("account_native_market_data_persistence_disabled")
    }
    fn add_signal(&self, _: &Signal) -> Result<()> {
        bail!("account_native_signal_persistence_unsupported")
    }
    fn add_custom_data(&self, _: &CustomData) -> Result<()> {
        bail!("account_native_custom_data_unsupported")
    }
    fn add_quote(&self, _: &QuoteTick) -> Result<()> {
        bail!("account_native_market_data_persistence_disabled")
    }
    fn add_trade(&self, _: &TradeTick) -> Result<()> {
        bail!("account_native_market_data_persistence_disabled")
    }
    fn add_funding_rate(&self, _: &FundingRateUpdate) -> Result<()> {
        bail!("account_spot_funding_unsupported")
    }
    fn add_bar(&self, _: &Bar) -> Result<()> {
        bail!("account_native_market_data_persistence_disabled")
    }
    fn delete_actor(&self, id: &ActorId) -> Result<()> {
        for key in self.load_actor(id)?.keys() {
            self.remove("actor_state", &format!("{id}:{key}"))?;
        }
        Ok(())
    }
    fn delete_strategy(&self, id: &StrategyId) -> Result<()> {
        for key in self.load_strategy(id)?.keys() {
            self.remove("strategy_state", &format!("{id}:{key}"))?;
        }
        Ok(())
    }
    fn delete_order(&self, id: &ClientOrderId) -> Result<()> {
        self.remove("orders", id.as_str())
    }
    fn delete_position(&self, id: &PositionId) -> Result<()> {
        self.remove("positions", id.as_ref())
    }
    fn delete_account_event(&self, _: &AccountId, _: &str) -> Result<()> {
        bail!("account_native_event_deletion_forbidden")
    }
    fn index_venue_order_id(&self, client: ClientOrderId, venue: VenueOrderId) -> Result<()> {
        self.put("order_venue", client, &venue)
    }
    fn index_order_position(&self, client: ClientOrderId, position: PositionId) -> Result<()> {
        self.put("order_position", client, &position)
    }
    fn index_order_clients(&self, claims: &[(ClientOrderId, ClientId)]) -> Result<()> {
        for (id, client) in claims {
            self.put("order_client", id, client)?;
        }
        Ok(())
    }
    fn update_actor(&self, id: &ActorId, state: &AHashMap<String, Bytes>) -> Result<()> {
        for (key, value) in state {
            let key = format!("{id}:{key}");
            if self.read::<Bytes>("actor_state", &key)?.as_ref() != Some(value) {
                self.put("actor_state", key, value)?;
            }
        }
        Ok(())
    }
    fn update_strategy(&self, id: &StrategyId, state: &AHashMap<String, Bytes>) -> Result<()> {
        for (key, value) in state {
            let key = format!("{id}:{key}");
            if self.read::<Bytes>("strategy_state", &key)?.as_ref() != Some(value) {
                self.put("strategy_state", key, value)?;
            }
        }
        Ok(())
    }
    fn update_account(&self, v: &AccountAny) -> Result<()> {
        self.add_account(v)
    }
    fn update_order(&self, event: &OrderEventAny) -> Result<()> {
        let id = event.client_order_id();
        let mut order: OrderAny = self
            .read("orders", id.as_str())?
            .ok_or_else(|| anyhow!("account_native_order_initialization_missing"))?;
        order.apply(event.clone()).inspect_err(|_| {
            self.failed.store(true, Ordering::SeqCst);
        })?;
        self.put("orders", id, &order)?;
        // Exact process cut after the official fill is durable, before subsequent
        // position/contingency writes. Never present in the production binary.
        #[cfg(test)]
        if matches!(event, OrderEventAny::Filled(_))
            && std::env::var("QZ_TEST_PERSISTED_FILL_CUT").as_deref() == Ok(id.as_str())
        {
            std::process::exit(19);
        }
        Ok(())
    }
    fn update_position(&self, v: &Position) -> Result<()> {
        self.add_position(v)
    }
    fn snapshot_order_state(&self, v: &OrderAny) -> Result<()> {
        self.put("orders", v.client_order_id(), v)
    }
    fn snapshot_position_state(&self, v: &Position, _: UnixNanos, _: Option<Money>) -> Result<()> {
        self.add_position(v)
    }
    fn heartbeat(&self, v: UnixNanos) -> Result<()> {
        self.put("metadata", "heartbeat", &v)
    }
}

#[cfg(test)]
mod order_list_tests {
    use super::*;
    use nautilus_common::{clock::VirtualClock, factories::OrderFactory};
    use nautilus_model::{
        enums::{LiquiditySide, OrderSide, OrderStatus},
        events::{OrderAccepted, OrderFilled},
        identifiers::TradeId,
        types::{Price, Quantity},
    };

    fn bracket() -> Vec<OrderAny> {
        OrderFactory::new(
            TraderId::from("RECOVERY-001"),
            StrategyId::from("BRACKET-001"),
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
        .call()
    }

    fn partial_bracket() -> Result<Vec<OrderAny>> {
        let mut orders = bracket();
        let parent = &mut orders[0];
        let venue = VenueOrderId::from("VENUE-ENTRY");
        let account = AccountId::from("HYPERLIQUID-001");
        parent.apply(OrderEventAny::Accepted(OrderAccepted::new(
            parent.trader_id(),
            parent.strategy_id(),
            parent.instrument_id(),
            parent.client_order_id(),
            venue,
            account,
            UUID4::new(),
            1.into(),
            1.into(),
            false,
        )))?;
        parent.apply(OrderEventAny::Filled(OrderFilled::new(
            parent.trader_id(),
            parent.strategy_id(),
            parent.instrument_id(),
            parent.client_order_id(),
            venue,
            account,
            TradeId::from("TRADE-PARTIAL"),
            parent.order_side(),
            parent.order_type(),
            Quantity::from("0.25"),
            Price::from("100.00"),
            Currency::USDC(),
            LiquiditySide::Taker,
            UUID4::new(),
            2.into(),
            2.into(),
            false,
            None,
            None,
            None,
        )))?;
        ensure!(
            parent.status() == OrderStatus::PartiallyFilled,
            "native partial fill not applied"
        );
        Ok(orders)
    }

    #[tokio::test(flavor = "current_thread")]
    async fn synchronous_official_restore_preserves_registered_clients_and_routes() -> Result<()> {
        use nautilus_model::identifiers::Venue;
        let directory = tempfile::tempdir()?;
        let factory = AccountCacheFactory::new(directory.path().to_path_buf());
        let trader = TraderId::from("RECOVERY-001");
        let orders = bracket();
        let database = factory
            .create(trader, UUID4::new(), CacheConfig::default())
            .await?;
        for order in &orders {
            database.add_order(order, None)?;
        }
        drop(database);
        let cache = Rc::new(RefCell::new(Cache::default()));
        let client = ClientId::from("ORIGINAL");
        let account = AccountId::from("ORIGINAL-001");
        let venue = Venue::from("ORIGINAL");
        {
            let mut cache = cache.borrow_mut();
            cache.add_client_account(client, account);
            cache.add_client_route(client, venue);
            cache.set_default_client(client);
            cache.add_external_data_client(client);
            cache.add_external_exec_client(client);
        }
        factory
            .install(cache.clone(), trader, UUID4::new(), CacheConfig::default())
            .await?;
        let restored = cache.borrow();
        ensure!(
            restored.account_id_for_client(&client) == Some(&account),
            "client account lost"
        );
        ensure!(
            restored.client_id_for_venue(&venue) == Some(&client),
            "client route lost"
        );
        ensure!(
            restored.client_id_for_venue(&Venue::from("OTHER")) == Some(&client),
            "default client lost"
        );
        ensure!(
            restored.is_external_data_client(&client) && restored.is_external_exec_client(&client),
            "external client registration lost"
        );
        for order in &orders {
            ensure!(
                serde_json::to_value(&*restored.order(&order.client_order_id()).unwrap())?
                    == serde_json::to_value(order)?,
                "order changed on restore"
            );
        }
        ensure!(
            restored
                .order_list(&orders[0].order_list_id().unwrap())
                .is_some(),
            "bracket index lost"
        );
        Ok(())
    }

    #[test]
    fn native_index_reconstruction_is_idempotent_and_preserves_orders() -> Result<()> {
        let mut cache = Cache::default();
        let orders = bracket();
        for order in &orders {
            cache.add_order(order.clone(), None, None, false)?;
        }
        restore_order_list_index(&mut cache)?;
        restore_order_list_index(&mut cache)?;
        let group = cache
            .order_list(&orders[0].order_list_id().unwrap())
            .unwrap();
        ensure!(group.client_order_ids.len() == 3, "members lost");
        for order in &orders {
            ensure!(
                serde_json::to_value(&*cache.order(&order.client_order_id()).unwrap())?
                    == serde_json::to_value(order)?,
                "order state changed"
            );
        }
        Ok(())
    }

    #[test]
    fn incomplete_native_bracket_is_not_restored() -> Result<()> {
        let mut cache = Cache::default();
        let orders = bracket();
        cache.add_order(orders[0].clone(), None, None, false)?;
        let error = restore_order_list_index(&mut cache).unwrap_err();
        ensure!(
            error
                .to_string()
                .contains("account_native_order_list_member_missing"),
            "wrong failure"
        );
        ensure!(
            cache.order_lists(None, None, None, None).is_empty(),
            "partial index restored"
        );
        Ok(())
    }

    // Launched by the parent test in a separate process. Uses the actual durable
    // account adapter and native objects, without market or execution clients.
    #[test]
    fn persist_bracket_child() -> Result<()> {
        let Ok(directory) = std::env::var("QZ_ORDER_RECOVERY_TEST_DIR") else {
            return Ok(());
        };
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?
            .block_on(async {
                let factory = AccountCacheFactory::new(directory.into());
                let cache = Rc::new(RefCell::new(Cache::default()));
                factory
                    .install(
                        cache.clone(),
                        TraderId::from("RECOVERY-001"),
                        UUID4::new(),
                        CacheConfig::default(),
                    )
                    .await?;
                let limit = std::env::var("QZ_ORDER_RECOVERY_TEST_TRUNCATE")
                    .ok()
                    .map(|v| v.parse::<usize>())
                    .transpose()?
                    .unwrap_or(3);
                let orders = if limit < 3 {
                    bracket()
                } else {
                    partial_bracket()?
                };
                for order in orders.into_iter().take(limit) {
                    cache.borrow_mut().add_order(order, None, None, true)?;
                }
                if std::env::var("QZ_ORDER_RECOVERY_TEST_ABRUPT").as_deref() == Ok("1") {
                    // Leave without Drop/normal shutdown; each official object was fsynced.
                    std::process::exit(17);
                }
                anyhow::Ok(())
            })
    }

    #[test]
    fn normal_and_abrupt_process_exit_restore_native_order_lists() -> Result<()> {
        for abrupt in [false, true] {
            let directory = tempfile::tempdir()?;
            let status = std::process::Command::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "account_runtime_cache::order_list_tests::persist_bracket_child",
                    "--nocapture",
                ])
                .env("QZ_ORDER_RECOVERY_TEST_DIR", directory.path())
                .env(
                    "QZ_ORDER_RECOVERY_TEST_ABRUPT",
                    if abrupt { "1" } else { "0" },
                )
                .status()?;
            ensure!(
                status.code() == Some(if abrupt { 17 } else { 0 }),
                "child did not reach durable write"
            );
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(async {
                    let factory = AccountCacheFactory::new(directory.path().into());
                    let cache = Rc::new(RefCell::new(Cache::default()));
                    factory
                        .install(
                            cache.clone(),
                            TraderId::from("RECOVERY-001"),
                            UUID4::new(),
                            CacheConfig::default(),
                        )
                        .await?;
                    ensure!(
                        cache.borrow().order_lists(None, None, None, None).len() == 1,
                        "list not restored"
                    );
                    ensure!(
                        cache.borrow().orders(None, None, None, None, None).len() == 3,
                        "orders not restored"
                    );
                    let parent_id = partial_bracket()?[0].client_order_id();
                    ensure!(
                        cache.borrow().order(&parent_id).unwrap().status()
                            == OrderStatus::PartiallyFilled,
                        "partial fill state lost"
                    );
                    ensure!(
                        cache.borrow().order(&parent_id).unwrap().filled_qty()
                            == Quantity::from("0.25"),
                        "filled quantity changed"
                    );
                    // Mirrors LiveNode.prepare_cache after the explicit backing installation.
                    restore_file_cache(&mut cache.borrow_mut())?;
                    restore_order_list_index(&mut cache.borrow_mut())?;
                    ensure!(
                        cache.borrow().order_lists(None, None, None, None).len() == 1,
                        "native reload lost group"
                    );
                    anyhow::Ok(())
                })?;
        }
        Ok(())
    }
    #[test]
    fn process_exit_during_group_persistence_fails_without_fabricating_legs() -> Result<()> {
        for count in [1, 2] {
            let directory = tempfile::tempdir()?;
            let status = std::process::Command::new(std::env::current_exe()?)
                .args([
                    "--exact",
                    "account_runtime_cache::order_list_tests::persist_bracket_child",
                    "--nocapture",
                ])
                .env("QZ_ORDER_RECOVERY_TEST_DIR", directory.path())
                .env("QZ_ORDER_RECOVERY_TEST_ABRUPT", "1")
                .env("QZ_ORDER_RECOVERY_TEST_TRUNCATE", count.to_string())
                .status()?;
            ensure!(
                status.code() == Some(17),
                "child did not stop after prefix persistence"
            );
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()?
                .block_on(async {
                    let factory = AccountCacheFactory::new(directory.path().into());
                    let cache = Rc::new(RefCell::new(Cache::default()));
                    let error = factory
                        .install(
                            cache.clone(),
                            TraderId::from("RECOVERY-001"),
                            UUID4::new(),
                            CacheConfig::default(),
                        )
                        .await
                        .unwrap_err();
                    ensure!(
                        error
                            .to_string()
                            .contains("account_native_order_list_member_missing"),
                        "incomplete persistence not reported"
                    );
                    ensure!(
                        cache
                            .borrow()
                            .order_lists(None, None, None, None)
                            .is_empty(),
                        "incomplete group invented"
                    );
                    ensure!(
                        cache.borrow().orders(None, None, None, None, None).len() == count,
                        "original durable order prefix changed"
                    );
                    anyhow::Ok(())
                })?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod pin_guard_tests {
    use super::*;

    #[test]
    fn old_pin_is_rejected_before_reading_balances_without_rewriting_metadata() -> Result<()> {
        let dir = tempfile::tempdir()?;
        let root = dir.path().join("native-cache");
        let stored = Stored {
            version: "0.68.0".into(),
            revision: "dc69438c2a5f936760db845d7a4bfc786da23276".into(),
            trader: "RECOVERY-001".into(),
            records: BTreeMap::new(),
        };
        atomic_value(&root.join("metadata.json"), &stored)?;
        let before = fs::read(root.join("metadata.json"))?;
        let factory = AccountCacheFactory::new(dir.path().into());
        let error = factory
            .restored_balances(&AccountId::from("HYPERLIQUID-001"))
            .unwrap_err();
        ensure!(error
            .to_string()
            .contains("account_native_cache_version_unsupported"));
        ensure!(fs::read(root.join("metadata.json"))? == before);
        Ok(())
    }
}
