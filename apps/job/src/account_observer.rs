//! Q-owned read-only adapter over official Nautilus 0.63.0 public types.
//! Run on the native event thread; send through a bounded queue to a separate
//! transport consumer. The consumer retries the exact envelope until receipted.
use anyhow::{anyhow, bail, Result};
use contracts::{account_observation::*, DbCounter, SchemaV1};
use nautilus_common::msgbus::typed_handler::TypedHandler;
use nautilus_model::{
    enums::AccountType, events::PortfolioSnapshot, identifiers::AccountId, types::Money,
};
use nautilus_portfolio::portfolio::Portfolio;
use std::{cell::RefCell, rc::Rc, sync::mpsc::SyncSender};

fn money(value: &Money) -> Result<AccountMoneyV1> {
    Ok(AccountMoneyV1 {
        amount: value
            .as_decimal()
            .to_string()
            .parse()
            .map_err(|_| anyhow!("native money outside wire range"))?,
        currency: value.currency.to_string(),
    })
}

/// Copies public native values without evaluating positions or combining currencies.
pub fn project_snapshot(snapshot: &PortfolioSnapshot) -> Result<NativePortfolioSnapshotV1> {
    let projected = NativePortfolioSnapshotV1 {
        account_id: snapshot.account_id.to_string(),
        account_type: match snapshot.account_type {
            AccountType::Cash => NativeAccountTypeV1::Cash,
            AccountType::Margin => NativeAccountTypeV1::Margin,
            AccountType::Betting => NativeAccountTypeV1::Betting,
            AccountType::Wallet => NativeAccountTypeV1::Wallet,
        },
        base_currency: snapshot.base_currency.map(|c| c.to_string()),
        balances: snapshot
            .balances
            .iter()
            .map(|b| {
                Ok(NativeAccountBalanceV1 {
                    total: money(&b.total)?,
                    locked: money(&b.locked)?,
                    free: money(&b.free)?,
                })
            })
            .collect::<Result<_>>()?,
        margins: snapshot
            .margins
            .iter()
            .map(|m| {
                Ok(NativeAccountMarginV1 {
                    initial: money(&m.initial)?,
                    maintenance: money(&m.maintenance)?,
                    instrument_id: m.instrument_id.map(|id| id.to_string()),
                })
            })
            .collect::<Result<_>>()?,
        unrealized_pnls: snapshot
            .unrealized_pnls
            .iter()
            .map(money)
            .collect::<Result<_>>()?,
        realized_pnls: snapshot
            .realized_pnls
            .iter()
            .map(money)
            .collect::<Result<_>>()?,
        total_equity: snapshot
            .total_equity
            .iter()
            .map(money)
            .collect::<Result<_>>()?,
        base_currency_equity: snapshot
            .base_currency_equity
            .as_ref()
            .map(money)
            .transpose()?,
        is_stale: snapshot.is_stale,
        stale_instruments: snapshot
            .stale_instruments
            .iter()
            .map(ToString::to_string)
            .collect(),
        stale_currencies: snapshot
            .stale_currencies
            .iter()
            .map(ToString::to_string)
            .collect(),
        unpriced_instruments: snapshot
            .unpriced_instruments
            .iter()
            .map(ToString::to_string)
            .collect(),
        event_id: snapshot.event_id.to_string(),
        ts_event: DbCounter::new(snapshot.ts_event.as_u64()).map_err(|e| anyhow!(e))?,
        ts_init: DbCounter::new(snapshot.ts_init.as_u64()).map_err(|e| anyhow!(e))?,
    };
    Ok(projected)
}

/// Source metadata is configured by the authorized downstream host, never inferred
/// from an Alpha/release. A new observer session is required when its cursor is lost.
#[derive(Clone)]
pub struct NativeAccountObserver {
    state: Rc<RefCell<State>>,
}
struct State {
    binding: NativeAccountBindingV1,
    sequence: DbCounter,
    dropped: DbCounter,
}

impl NativeAccountObserver {
    pub fn new(
        binding: NativeAccountBindingV1,
        last_sequence: DbCounter,
        dropped: DbCounter,
    ) -> Result<Self> {
        domain::account_observation::binding(&binding)?;
        if dropped > last_sequence {
            bail!("invalid observer cursor");
        }
        Ok(Self {
            state: Rc::new(RefCell::new(State {
                binding,
                sequence: last_sequence,
                dropped,
            })),
        })
    }

    pub fn cursor(&self) -> (DbCounter, DbCounter) {
        let state = self.state.borrow();
        (state.sequence, state.dropped)
    }

    /// None means no native account snapshot was available, never a zero account.
    pub fn capture(
        &self,
        portfolio: &mut Portfolio,
        account: &AccountId,
    ) -> Result<Option<NativePortfolioSnapshotV1>> {
        if account.to_string() != self.state.borrow().binding.native_account_id {
            bail!("native account binding mismatch");
        }
        portfolio
            .build_snapshot(account)
            .as_ref()
            .map(project_snapshot)
            .transpose()
    }

    /// True means queued, not durably received. Conversion/queue failures consume
    /// sequence and increment the drop counter so the next accepted frame exposes a gap.
    pub fn try_emit(
        &self,
        snapshot: Option<&PortfolioSnapshot>,
        observed_at_ns: DbCounter,
        connection: AccountConnectionV1,
        sender: &SyncSender<AccountObservationSubmitV1>,
    ) -> Result<bool> {
        let mut state = self.state.borrow_mut();
        state.sequence = state
            .sequence
            .checked_add(1)
            .ok_or_else(|| anyhow!("observer sequence exhausted; start a new session"))?;
        let result = (|| {
            let request = AccountObservationSubmitV1 {
                schema_version: SchemaV1,
                binding: state.binding.clone(),
                sequence: state.sequence,
                dropped_events: state.dropped,
                observed_at_ns,
                connection,
                snapshot: snapshot.map(project_snapshot).transpose()?,
            };
            domain::account_observation::observation(&request)?;
            Ok(sender.try_send(request).is_ok())
        })();
        if !matches!(&result, Ok(true)) {
            state.dropped = state
                .dropped
                .checked_add(1)
                .ok_or_else(|| anyhow!("observer drop counter exhausted"))?;
        }
        result
    }

    /// Subscribe this official typed handler with native subscribe_portfolio_snapshot.
    /// Snapshot arrival alone does not assert broker connectivity: heartbeat state
    /// must come from the host's independently observed connection lifecycle.
    pub fn handler(
        &self,
        sender: SyncSender<AccountObservationSubmitV1>,
        now: impl Fn() -> DbCounter + 'static,
        connection: impl Fn() -> AccountConnectionV1 + 'static,
    ) -> TypedHandler<PortfolioSnapshot> {
        self.checked_handler(sender, move || Ok(now()), connection)
    }

    /// Fallible wall clock for native hosts. Clock failure is an explicit dropped
    /// observation, never a fabricated timestamp or a panic in native dispatch.
    pub fn checked_handler(
        &self,
        sender: SyncSender<AccountObservationSubmitV1>,
        now: impl Fn() -> Result<DbCounter> + 'static,
        connection: impl Fn() -> AccountConnectionV1 + 'static,
    ) -> TypedHandler<PortfolioSnapshot> {
        let observer = self.clone();
        TypedHandler::from(move |snapshot: &PortfolioSnapshot| {
            if snapshot.account_id.to_string() == observer.state.borrow().binding.native_account_id
            {
                // The host can inspect cursor().1; errors never fabricate values.
                match now() {
                    Ok(now) => {
                        let _ = observer.try_emit(Some(snapshot), now, connection(), &sender);
                    }
                    Err(_) => {
                        let mut state = observer.state.borrow_mut();
                        if let (Some(sequence), Some(dropped)) =
                            (state.sequence.checked_add(1), state.dropped.checked_add(1))
                        {
                            state.sequence = sequence;
                            state.dropped = dropped;
                        }
                    }
                }
            }
        })
    }
}
