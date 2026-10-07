//! Controlled audit counterexamples. No market or native-run evidence is fabricated.
use super::*;
use nautilus_core::UUID4;
use nautilus_model::{
    accounts::CashAccount,
    enums::{AccountType, LiquiditySide, OrderSide, OrderType},
    events::AccountState,
    identifiers::{AccountId, InstrumentId, Symbol},
    instruments::CurrencyPair,
    types::{AccountBalance, Price, Quantity},
};
fn balance(value: &str) -> AccountBalance {
    let total = Money::from(value);
    AccountBalance::new(total, Money::zero(total.currency), total)
}
fn fixture(posted: bool) -> (AccountAny, InstrumentAny, OrderFilled) {
    let id = InstrumentId::from("BTC-SPOT.HYPERLIQUID");
    let account_id = AccountId::from("HYPERLIQUID-001");
    let mut cash = CashAccount::new(
        AccountState::new(
            account_id,
            AccountType::Cash,
            vec![balance("1000 USDC")],
            vec![],
            true,
            UUID4::new(),
            1_u64.into(),
            1_u64.into(),
            None,
        ),
        true,
        false,
    );
    let pair = InstrumentAny::CurrencyPair(
        CurrencyPair::builder()
            .instrument_id(id)
            .raw_symbol(Symbol::new("BTC"))
            .base_currency(Currency::BTC())
            .quote_currency(Currency::USDC())
            .price_precision(2)
            .size_precision(4)
            .price_increment(Price::from("0.01"))
            .size_increment(Quantity::from("0.0001"))
            .ts_event(0_u64.into())
            .ts_init(0_u64.into())
            .build()
            .unwrap(),
    );
    let fill = OrderFilled::new(
        "TRADER-001".into(),
        "TARGET-001".into(),
        id,
        "O-001".into(),
        "V-001".into(),
        account_id,
        "T-001".into(),
        OrderSide::Buy,
        OrderType::Market,
        Quantity::from("5.0000"),
        Price::from("100.00"),
        Currency::USDC(),
        LiquiditySide::Taker,
        UUID4::new(),
        2_u64.into(),
        2_u64.into(),
        false,
        None,
        Some(Money::from("0.0035 BTC")),
        None,
    );
    if posted {
        cash.update_balances(&[balance("500 USDC"), balance("4.9965 BTC")])
            .unwrap();
        cash.base
            .try_update_commissions(fill.commission.unwrap())
            .unwrap();
    }
    (AccountAny::Cash(cash), pair, fill)
}
#[test]
fn audit_accepts_posted_native_cash_and_keeps_it_unchanged() {
    let (account, pair, fill) = fixture(true);
    let before = serde_json::to_value(&account).unwrap();
    reconcile(&account, &[fill], &[pair], 1, 3).unwrap();
    assert_eq!(serde_json::to_value(&account).unwrap(), before);
}
#[test]
fn audit_rejects_logged_fill_when_native_cash_posting_was_rolled_back() {
    let (account, pair, fill) = fixture(false);
    assert!(
        reconcile(&account, &[fill], &[pair], 1, 3)
            .unwrap_err()
            .to_string()
            .contains("SPOT_NATIVE_BALANCE_MISSING")
    );
}
#[test]
fn audit_rejects_missing_duplicated_or_unknown_fee_fill_evidence() {
    let (account, pair, fill) = fixture(true);
    assert!(reconcile(&account, &[], &[pair.clone()], 1, 3).is_err());
    assert!(
        reconcile(
            &account,
            &[fill.clone(), fill.clone()],
            &[pair.clone()],
            1,
            3
        )
        .unwrap_err()
        .to_string()
        .contains("REPLAY")
    );
    let mut unknown = fill;
    unknown.commission = None;
    assert!(
        reconcile(&account, &[unknown], &[pair], 1, 3)
            .unwrap_err()
            .to_string()
            .contains("COMMISSION_UNKNOWN")
    );
}
#[test]
fn audit_rejects_unexplained_external_cash_flow() {
    let (mut account, pair, fill) = fixture(true);
    let AccountAny::Cash(cash) = &mut account else {
        unreachable!()
    };
    cash.update_balances(&[balance("501 USDC")]).unwrap();
    assert!(reconcile(&account, &[fill], &[pair], 1, 3).is_err());
}
#[test]
fn audit_rejects_unfunded_execution_even_if_account_remained_nonnegative() {
    let (account, pair, mut fill) = fixture(false);
    fill.last_qty = Quantity::from("11.0000");
    assert!(
        reconcile(&account, &[fill], &[pair], 1, 3)
            .unwrap_err()
            .to_string()
            .contains("UNFUNDED_OR_ROLLED_BACK")
    );
}

#[test]
fn audit_rejects_mismatched_existing_native_base_balance() {
    let (mut account, pair, fill) = fixture(true);
    let AccountAny::Cash(cash) = &mut account else {
        unreachable!()
    };
    cash.update_balances(&[balance("4.9964 BTC")]).unwrap();
    assert_eq!(
        reconcile(&account, &[fill], &[pair], 1, 3)
            .unwrap_err()
            .to_string(),
        "SPOT_NATIVE_ACCOUNT_FILL_RECONCILIATION_FAILED"
    );
}

#[test]
fn execution_price_age_uses_actual_decision_clock_including_deferred_funding() {
    let policy = NativeSpotCashPolicyV1 {
        schema_version: SchemaV1,
        mode: NativeSpotCashModeV1::MultiCurrencyCash,
        report_currency: "USDC".into(),
        price_method: NativeSpotPriceMethodV1::ClosedBarClose,
        allowed_instrument_ids: vec!["BTC-SPOT.HYPERLIQUID".into()],
        returns_policy: ReportCurrencyDailyPolicyV1::FreshSimulationNoExternalFlows,
        daily_sampling: NativeSpotDailySamplingV1::NativeUtcMidnightBoundaries,
        maximum_price_age_ns: count(1_000_000_000).unwrap(),
    };
    let mut snapshot = SpotCashDecisionSnapshot {
        asof_ns: count(2_000_000_000).unwrap(),
        total: "1000".parse().unwrap(),
        balances: vec![],
        prices: vec![NativeSpotPriceV1 {
            session_id: Id::new(),
            native_instance_id: UUID4::new().to_string().parse().unwrap(),
            dataset_revision_id: Id::new(),
            source_row_key: "CONTROLLED_CLOSE_ROW".into(),
            instrument_id: "BTC-SPOT.HYPERLIQUID".into(),
            method: NativeSpotPriceMethodV1::ClosedBarClose,
            observed_sequence: count(1).unwrap(),
            bar_open_ns: count(1).unwrap(),
            event_ns: count(1_000_000_000).unwrap(),
            available_ns: count(1_000_000_000).unwrap(),
            price: "100".parse().unwrap(),
        }],
    };
    let mut bar = Bar::new_checked(
        "BTC-SPOT.HYPERLIQUID-1-MINUTE-LAST-EXTERNAL"
            .parse()
            .unwrap(),
        Price::from("100.00"),
        Price::from("100.00"),
        Price::from("100.00"),
        Price::from("100.00"),
        Quantity::from("1.0000"),
        1_000_000_000_u64.into(),
        1_000_000_000_u64.into(),
    )
    .unwrap();
    // Exactly at the bound is reliable, later native execution is stale even
    // when the previous sizing snapshot had been within policy.
    assert_eq!(
        execution_price(&snapshot, &policy, &bar).unwrap(),
        Decimal::from(100)
    );
    snapshot.asof_ns = count(2_000_000_001).unwrap();
    assert_eq!(
        execution_price(&snapshot, &policy, &bar)
            .unwrap_err()
            .to_string(),
        "SPOT_EXECUTION_PRICE_STALE"
    );
    bar.ts_init = 3_000_000_000_u64.into();
    assert_eq!(
        execution_price(&snapshot, &policy, &bar)
            .unwrap_err()
            .to_string(),
        "SPOT_EXECUTION_PRICE_CLOCK_INVALID"
    );
}
