//! Controlled native cash account shared by the observer test and snapshot exporter.
use nautilus_common::{cache::Cache, clock::TestClock};
use nautilus_core::{UnixNanos, UUID4};
use nautilus_model::{
    accounts::{AccountAny, CashAccount},
    enums::AccountType,
    events::AccountState,
    identifiers::AccountId,
    types::{AccountBalance, Money},
};
use nautilus_portfolio::portfolio::Portfolio;
use std::{cell::RefCell, rc::Rc};

pub const FIRST_TIME: u64 = 1700000000000000001;
pub const SECOND_TIME: u64 = 1700000000000000002;

pub struct CashPortfolio {
    pub portfolio: Portfolio,
    pub account_id: AccountId,
    clock: Rc<RefCell<TestClock>>,
    cache: Rc<RefCell<Cache>>,
}

impl CashPortfolio {
    pub fn new() -> Self {
        let clock = Rc::new(RefCell::new(TestClock::new()));
        clock
            .borrow_mut()
            .advance_time(UnixNanos::from(FIRST_TIME), true);
        let cache = Rc::new(RefCell::new(Cache::default()));
        let account_id = AccountId::new("SIM-001");
        let event = account_state(account_id, FIRST_TIME, "1000.25 USD", "0.12345678 USDT");
        cache
            .borrow_mut()
            .add_account(AccountAny::Cash(CashAccount::new(event, false, false)))
            .unwrap();
        let portfolio = Portfolio::new(clock.clone(), cache.clone(), None);
        Self {
            portfolio,
            account_id,
            clock,
            cache,
        }
    }

    pub fn advance_account(&mut self) {
        // A native account-state update, not an order/fill or a Q equity calculation.
        self.cache
            .borrow_mut()
            .update_account_state(&account_state(
                self.account_id,
                SECOND_TIME,
                "1001.75 USD",
                "0.22345678 USDT",
            ))
            .unwrap();
        self.clock
            .borrow_mut()
            .advance_time(UnixNanos::from(SECOND_TIME), true);
    }
}

fn account_state(account_id: AccountId, timestamp: u64, usd: &str, usdt: &str) -> AccountState {
    AccountState::new(
        account_id,
        AccountType::Cash,
        vec![
            AccountBalance::new(Money::from(usd), Money::from("0 USD"), Money::from(usd)),
            AccountBalance::new(Money::from(usdt), Money::from("0 USDT"), Money::from(usdt)),
        ],
        vec![],
        true,
        UUID4::new(),
        UnixNanos::from(timestamp),
        UnixNanos::from(timestamp),
        None,
    )
}
