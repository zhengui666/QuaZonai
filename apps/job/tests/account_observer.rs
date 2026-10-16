//! Official native Portfolio -> public snapshot -> Q observer, no live connection.
use contracts::{account_observation::*, DbCounter};
use job::account_observer::{project_snapshot, NativeAccountObserver};
#[path = "support/native_portfolio.rs"]
mod native_portfolio;
use nautilus_core::{UnixNanos, UUID4};
use nautilus_model::{enums::AccountType, identifiers::AccountId, types::Money};
use std::sync::mpsc::sync_channel;

#[test]
fn native_portfolio_snapshot_retains_exact_money_and_reports_queue_gaps() {
    let fixture: AccountObservationSubmitV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/native-account-paper-snapshot.json"
    ))
    .unwrap();
    let mut native = native_portfolio::CashPortfolio::new();
    let account_id = native.account_id;
    let snapshot = native.portfolio.build_snapshot(&account_id).unwrap();
    let projected = project_snapshot(&snapshot).unwrap();
    assert_eq!(projected.total_equity.len(), 2);
    let usd = projected
        .total_equity
        .iter()
        .find(|m| m.currency == "USD")
        .unwrap();
    let usdt = projected
        .total_equity
        .iter()
        .find(|m| m.currency == "USDT")
        .unwrap();
    assert_eq!(serde_json::to_value(&usd.amount).unwrap(), "1000.25");
    assert_eq!(serde_json::to_value(&usdt.amount).unwrap(), "0.12345678");
    assert!(projected.base_currency_equity.is_none());
    assert!(native
        .portfolio
        .build_snapshot(&AccountId::new("MISSING-001"))
        .is_none());
    assert_eq!(projected.ts_event.get(), 1700000000000000001);
    assert!(
        projected.realized_pnls.is_empty(),
        "do not manufacture a missing zero PnL"
    );
    let observer =
        NativeAccountObserver::new(fixture.binding, DbCounter::ZERO, DbCounter::ZERO).unwrap();
    assert!(observer
        .capture(&mut native.portfolio, &AccountId::new("OTHER-001"))
        .is_err());
    assert!(observer
        .capture(&mut native.portfolio, &account_id)
        .unwrap()
        .is_some());
    let (sender, receiver) = sync_channel(1);
    let now = DbCounter::new(1700000000000000011).unwrap();
    let handler = observer.handler(sender.clone(), move || now, || AccountConnectionV1::Unknown);
    handler.handle(&snapshot);
    handler.handle(&snapshot); // bounded queue is full; native thread never blocks
    let first = receiver.try_recv().unwrap();
    assert_eq!(first.snapshot.unwrap(), projected);
    assert_eq!(first.connection, AccountConnectionV1::Unknown);
    assert!(observer
        .try_emit(None, now, AccountConnectionV1::Disconnected, &sender)
        .unwrap());
    let after_gap = receiver.try_recv().unwrap();
    assert_eq!(after_gap.sequence.get(), 3);
    assert_eq!(after_gap.dropped_events.get(), 1);
    assert!(after_gap.snapshot.is_none());
    domain::account_observation::observation(&after_gap).unwrap();
    native.advance_account();
    let next = native.portfolio.build_snapshot(&account_id).unwrap();
    assert_eq!(next.ts_event.as_u64(), native_portfolio::SECOND_TIME);
    assert_eq!(next.ts_init.as_u64(), native_portfolio::SECOND_TIME);
    assert_ne!(next.event_id, snapshot.event_id);
    assert!(next.total_equity.contains(&Money::from("1001.75 USD")));
    assert!(next.total_equity.contains(&Money::from("0.22345678 USDT")));
}

#[test]
fn native_valuation_metadata_is_preserved_without_promoting_partial_equity() {
    use nautilus_model::{events::PortfolioSnapshot, identifiers::InstrumentId};
    let snapshot = PortfolioSnapshot::new(
        AccountId::new("SIM-001"),
        AccountType::Cash,
        None,
        vec![],
        vec![],
        vec![],
        vec![],
        vec![Money::from("1000 USD"), Money::from("0.12345678 USDT")],
        None,
        true,
        vec![InstrumentId::from("BTCUSDT.SIM")],
        vec![],
        vec![InstrumentId::from("ETHUSDT.SIM")],
        UUID4::new(),
        UnixNanos::from(1700000000000000001_u64),
        UnixNanos::from(1700000000000000002_u64),
    );
    let projected = project_snapshot(&snapshot).unwrap();
    assert!(projected.is_stale);
    assert_eq!(projected.unpriced_instruments, vec!["ETHUSDT.SIM"]);
    assert_eq!(projected.stale_instruments, vec!["BTCUSDT.SIM"]);
    assert_eq!(
        domain::account_observation::valuation(Some(&projected)),
        AccountValuationV1::Unpriced
    );
    assert_eq!(
        projected.total_equity.len(),
        2,
        "retain partial native values only with the unpriced status"
    );
}

#[test]
fn invalid_native_host_clock_consumes_a_drop_without_fabricating_a_timestamp() {
    let fixture: AccountObservationSubmitV1 = serde_json::from_str(include_str!(
        "../../../tests/contracts/native-account-paper-snapshot.json"
    ))
    .unwrap();
    let observer =
        NativeAccountObserver::new(fixture.binding, DbCounter::ZERO, DbCounter::ZERO).unwrap();
    let mut native = native_portfolio::CashPortfolio::new();
    let snapshot = native.portfolio.build_snapshot(&native.account_id).unwrap();
    let (sender, receiver) = sync_channel(1);
    let handler = observer.checked_handler(
        sender.clone(),
        || anyhow::bail!("clock unavailable"),
        || AccountConnectionV1::Connected,
    );
    handler.handle(&snapshot);
    assert!(receiver.try_recv().is_err());
    assert_eq!(observer.cursor().0.get(), 1);
    assert_eq!(observer.cursor().1.get(), 1);
    observer
        .try_emit(
            None,
            DbCounter::new(3).unwrap(),
            AccountConnectionV1::Unknown,
            &sender,
        )
        .unwrap();
    let next = receiver.try_recv().unwrap();
    assert_eq!(next.sequence.get(), 2);
    assert_eq!(next.dropped_events.get(), 1);
}
