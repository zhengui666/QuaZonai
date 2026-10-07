use super::*;
use contracts::{Id, science::NativeFeeRateV1};
#[path = "../../../../tests/support/execution_models.rs"]
mod models;

fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn money(value: &str) -> DecimalValue {
    value.parse().unwrap()
}
// Synthetic UUIDv4 fixtures are confined to tests; production copies native IDs.
fn native_id(value: u64) -> NativeSpotRuntimeIdV1 {
    format!("00000000-0000-4000-8000-{value:012x}")
        .parse()
        .unwrap()
}
fn settings() -> NativeSimulationSettingsV1 {
    NativeSimulationSettingsV1 {
        multi_currency_spot_cash: Some(NativeSpotCashPolicyV1 {
            schema_version: SchemaV1,
            mode: NativeSpotCashModeV1::MultiCurrencyCash,
            report_currency: "USDC".into(),
            price_method: NativeSpotPriceMethodV1::ClosedBarClose,
            allowed_instrument_ids: vec!["BTC-USDC.HYPERLIQUID".into()],
            returns_policy: ReportCurrencyDailyPolicyV1::FreshSimulationNoExternalFlows,
            daily_sampling: NativeSpotDailySamplingV1::NativeUtcMidnightBoundaries,
            maximum_price_age_ns: count(100),
        }),
        schema_version: SchemaV1,
        base_currency: "USDC".into(),
        starting_capital: money("200"),
        account_kind: NativeAccountKind::Cash,
        leverage: money("1"),
        fee_model: models::fee(),
        fill_model: models::fill(),
        latency_model: models::latency(1),
        snapshot_interval_ms: 86_400_000,
        exposure_tolerance: money("0.000001"),
        fee_rates: vec![NativeFeeRateV1 {
            instrument_id: "BTC-USDC.HYPERLIQUID".into(),
            maker: money("0"),
            taker: money("0.001"),
        }],
    }
}
fn instrument() -> NativeSpotInstrumentV1 {
    NativeSpotInstrumentV1 {
        instrument_id: "BTC-USDC.HYPERLIQUID".into(),
        venue: "HYPERLIQUID".into(),
        native_kind: "CurrencyPair".into(),
        base_currency: "BTC".into(),
        quote_currency: "USDC".into(),
        is_inverse: false,
        has_expiration: false,
        multiplier: money("1"),
    }
}
fn session() -> NativeSpotCashSessionV1 {
    NativeSpotCashSessionV1 {
        session_id: Id::new(),
        native_instance_id: native_id(1001),
        dataset_revision_id: Id::new(),
        account_id: "HYPERLIQUID-001".into(),
        venue: "HYPERLIQUID".into(),
        period_start_ns: count(DAY_NS),
        period_end_ns: count(4 * DAY_NS),
    }
}
fn context() -> SpotCashContext {
    SpotCashContext::new(&settings(), session(), &[instrument()]).unwrap()
}
fn day_context() -> SpotCashContext {
    let mut s = session();
    s.period_end_ns = count(2 * DAY_NS);
    SpotCashContext::new(&settings(), s, &[instrument()]).unwrap()
}
fn frame(ctx: &SpotCashContext, time: u64, sequence: u64) -> NativeSpotValuationFrameV1 {
    NativeSpotValuationFrameV1 {
        snapshot: NativeSpotCashSnapshotV1 {
            binding: NativeSpotSnapshotBindingV1 {
                origin: NativeSpotSnapshotOriginV1::NativePublication,
                native_instance_id: ctx.session.native_instance_id,
                event_id: native_id(sequence),
                dataset_revision_id: ctx.session.dataset_revision_id,
                snapshot_sequence: count(sequence),
                session_id: ctx.session.session_id,
                account_id: ctx.session.account_id.clone(),
                venue: ctx.session.venue.clone(),
                asof_ns: count(time),
            },
            account_kind: NativeAccountKind::Cash,
            native_base_currency: None,
            balances_complete: true,
            balances: vec![
                NativeSpotCashBalanceV1 {
                    currency: "USDC".into(),
                    total: money("100"),
                    free: money("80"),
                    locked: money("20"),
                },
                NativeSpotCashBalanceV1 {
                    currency: "BTC".into(),
                    total: money("2"),
                    free: money("1"),
                    locked: money("1"),
                },
            ],
        },
        prices: vec![NativeSpotPriceV1 {
            session_id: ctx.session.session_id,
            native_instance_id: ctx.session.native_instance_id,
            dataset_revision_id: ctx.session.dataset_revision_id,
            source_row_key: format!("bar:{time}"),
            instrument_id: instrument().instrument_id,
            method: NativeSpotPriceMethodV1::ClosedBarClose,
            observed_sequence: count(sequence - 1),
            bar_open_ns: count(time - 1_000_000_000),
            event_ns: count(time),
            available_ns: count(time),
            price: money("50"),
        }],
    }
}
fn flows(
    ctx: &SpotCashContext,
    frames: &[NativeSpotValuationFrameV1],
) -> NativeSpotCashFlowEvidenceV1 {
    NativeSpotCashFlowEvidenceV1 {
        native_run_id: native_id(1002),
        native_instance_id: ctx.session.native_instance_id,
        session_id: ctx.session.session_id,
        dataset_revision_id: ctx.session.dataset_revision_id,
        account_id: ctx.session.account_id.clone(),
        venue: ctx.session.venue.clone(),
        period_start_ns: ctx.session.period_start_ns,
        period_end_ns: ctx.session.period_end_ns,
        opening_snapshot: frames.first().unwrap().snapshot.binding.clone(),
        closing_snapshot: frames.last().unwrap().snapshot.binding.clone(),
        observed_snapshot_count: count(frames.len() as u64),
        external_flow_count: Some(count(0)),
    }
}
fn returns(
    ctx: &SpotCashContext,
    frames: &[NativeSpotValuationFrameV1],
) -> Result<ReportCurrencyDailyReturnsV1, DomainError> {
    ctx.daily_returns(frames, &flows(ctx, frames))
}
fn total(value: ReportCurrencyValuationV1) -> DecimalValue {
    match value.outcome {
        ReportCurrencyValuationOutcomeV1::Complete { total, .. } => total,
        other => panic!("expected complete: {other:?}"),
    }
}
fn fails(
    ctx: &SpotCashContext,
    value: &NativeSpotValuationFrameV1,
    expected: ReportValuationFailureV1,
) {
    match ctx.value(value).outcome {
        ReportCurrencyValuationOutcomeV1::Unavailable { reason } => assert_eq!(reason, expected),
        other => panic!("expected unavailable: {other:?}"),
    }
}

#[test]
fn cash_plan_is_native_multi_currency_unborrowed_and_report_only() {
    let plan = account_plan(&settings()).unwrap();
    assert_eq!(plan.account_kind, NativeAccountKind::Cash);
    assert!(plan.native_base_currency.is_none());
    assert!(!plan.allow_cash_borrowing);
    assert_eq!(plan.leverage, money("1"));
    assert_eq!(plan.starting_balances.len(), 1);
    assert_eq!(plan.starting_balances[0].currency, "USDC");
    assert_eq!(plan.starting_balances[0].amount, money("200"));
}

#[test]
fn legacy_wire_roundtrip_omits_the_new_mode() {
    let mut old = settings();
    old.multi_currency_spot_cash = None;
    let wire = serde_json::to_value(&old).unwrap();
    assert!(wire.get("multi_currency_spot_cash").is_none());
    let restored: NativeSimulationSettingsV1 = serde_json::from_value(wire.clone()).unwrap();
    assert!(restored.multi_currency_spot_cash.is_none());
    assert_eq!(serde_json::to_value(restored).unwrap(), wire);
    assert!(crate::portfolio::simulation_settings(&old).is_ok());
    assert!(matches!(
        crate::portfolio::simulation_settings(&settings()),
        Err(DomainError::CapabilityUnavailable(
            "multi_currency_spot_cash_not_integrated"
        ))
    ));
}

#[test]
fn invalid_settings_cannot_reach_account_plan() {
    for mutate in [
        |s: &mut NativeSimulationSettingsV1| s.account_kind = NativeAccountKind::Margin,
        |s: &mut NativeSimulationSettingsV1| s.leverage = money("2"),
        |s: &mut NativeSimulationSettingsV1| s.starting_capital = money("0"),
        |s: &mut NativeSimulationSettingsV1| s.base_currency = "USD".into(),
        |s: &mut NativeSimulationSettingsV1| s.snapshot_interval_ms = 0,
        |s: &mut NativeSimulationSettingsV1| {
            s.multi_currency_spot_cash
                .as_mut()
                .unwrap()
                .maximum_price_age_ns = count(0)
        },
    ] {
        let mut s = settings();
        mutate(&mut s);
        assert!(account_plan(&s).is_err());
    }
}

#[test]
fn instrument_scope_rejects_perps_cross_venue_quote_and_ambiguous_base() {
    for mutate in [
        |i: &mut NativeSpotInstrumentV1| i.native_kind = "CryptoPerpetual".into(),
        |i: &mut NativeSpotInstrumentV1| i.venue = "OTHER".into(),
        |i: &mut NativeSpotInstrumentV1| i.quote_currency = "USDT".into(),
        |i: &mut NativeSpotInstrumentV1| i.is_inverse = true,
        |i: &mut NativeSpotInstrumentV1| i.has_expiration = true,
        |i: &mut NativeSpotInstrumentV1| i.multiplier = money("10"),
    ] {
        let mut i = instrument();
        mutate(&mut i);
        assert!(SpotCashContext::new(&settings(), session(), &[i]).is_err());
    }
}

#[test]
fn native_total_is_used_once_and_locked_redistribution_does_not_change_equity() {
    let ctx = context();
    let mut input = frame(&ctx, DAY_NS, 2);
    assert_eq!(total(ctx.value(&input)), money("200"));
    input.snapshot.balances[1].free = money("0");
    input.snapshot.balances[1].locked = money("2");
    assert_eq!(total(ctx.value(&input)), money("200"));
    // The input/output have no position value, PnL or commission adjustment.
}

#[test]
fn missing_price_never_returns_a_partial_total() {
    let ctx = context();
    let mut input = frame(&ctx, DAY_NS, 2);
    input.prices.clear();
    fails(&ctx, &input, ReportValuationFailureV1::MissingPrice);
    let wire = serde_json::to_value(ctx.value(&input)).unwrap();
    assert!(wire["outcome"].get("total").is_none());
    assert!(wire["outcome"].get("legs").is_none());
}

#[test]
fn asof_clocks_order_and_frozen_price_source_are_required() {
    let ctx = context();
    let cases: [(
        fn(&mut NativeSpotValuationFrameV1),
        ReportValuationFailureV1,
    ); 9] = [
        (
            |f| f.prices[0].event_ns = count(DAY_NS + 1),
            ReportValuationFailureV1::FuturePrice,
        ),
        (
            |f| f.prices[0].available_ns = count(DAY_NS + 1),
            ReportValuationFailureV1::FuturePrice,
        ),
        (
            |f| f.prices[0].event_ns = count(DAY_NS - 101),
            ReportValuationFailureV1::StalePrice,
        ),
        (
            |f| f.prices[0].observed_sequence = count(2),
            ReportValuationFailureV1::AmbiguousPriceOrder,
        ),
        (
            |f| f.prices[0].dataset_revision_id = Id::new(),
            ReportValuationFailureV1::InvalidPriceSource,
        ),
        (
            |f| f.prices[0].native_instance_id = native_id(1003),
            ReportValuationFailureV1::InvalidPriceSource,
        ),
        (
            |f| f.prices[0].session_id = Id::new(),
            ReportValuationFailureV1::InvalidPriceSource,
        ),
        (
            |f| f.prices[0].source_row_key.clear(),
            ReportValuationFailureV1::InvalidPriceSource,
        ),
        (
            |f| f.prices[0].price = money("0"),
            ReportValuationFailureV1::NonPositivePrice,
        ),
    ];
    for (mutate, expected) in cases {
        let mut f = frame(&ctx, DAY_NS, 2);
        mutate(&mut f);
        fails(&ctx, &f, expected);
    }
}

#[test]
fn snapshot_balance_and_identity_failures_are_unavailable() {
    let ctx = context();
    let cases: [(
        fn(&mut NativeSpotValuationFrameV1),
        ReportValuationFailureV1,
    ); 9] = [
        (
            |f| f.snapshot.binding.session_id = Id::new(),
            ReportValuationFailureV1::InvalidSnapshotBinding,
        ),
        (
            |f| f.snapshot.binding.native_instance_id = native_id(1003),
            ReportValuationFailureV1::InvalidSnapshotBinding,
        ),
        (
            |f| f.snapshot.binding.dataset_revision_id = Id::new(),
            ReportValuationFailureV1::InvalidSnapshotBinding,
        ),
        (
            |f| f.snapshot.native_base_currency = Some("USDC".into()),
            ReportValuationFailureV1::UnsupportedAccount,
        ),
        (
            |f| f.snapshot.balances_complete = false,
            ReportValuationFailureV1::IncompleteBalances,
        ),
        (
            |f| f.snapshot.balances[1].total = money("-1"),
            ReportValuationFailureV1::InvalidBalance,
        ),
        (
            |f| f.snapshot.balances[1].free = money("0"),
            ReportValuationFailureV1::InvalidBalance,
        ),
        (
            |f| f.snapshot.balances[1].currency = "USDC".into(),
            ReportValuationFailureV1::DuplicateCurrency,
        ),
        (
            |f| f.snapshot.balances[1].currency = "USDT".into(),
            ReportValuationFailureV1::UnknownCurrency,
        ),
    ];
    for (mutate, expected) in cases {
        let mut f = frame(&ctx, DAY_NS, 2);
        mutate(&mut f);
        fails(&ctx, &f, expected);
    }
}

#[test]
fn exact_decimal_products_do_not_round_or_overflow() {
    let ctx = context();
    let mut f = frame(&ctx, DAY_NS, 2);
    f.snapshot.balances[1].total = money("0.000000000000000001");
    f.snapshot.balances[1].free = money("0.000000000000000001");
    f.snapshot.balances[1].locked = money("0");
    f.prices[0].price = money("0.1");
    fails(&ctx, &f, ReportValuationFailureV1::ArithmeticOutOfRange);
    f.snapshot.balances[1].total = money("99999999999999999999");
    f.snapshot.balances[1].free = f.snapshot.balances[1].total.clone();
    f.prices[0].price = money("2");
    fails(&ctx, &f, ReportValuationFailureV1::ArithmeticOutOfRange);
}

#[test]
fn zero_known_inventory_needs_no_price_but_unknown_stablecoin_is_not_one_to_one() {
    let ctx = context();
    let mut f = frame(&ctx, DAY_NS, 2);
    f.prices.clear();
    f.snapshot.balances[1].total = money("0");
    f.snapshot.balances[1].free = money("0");
    f.snapshot.balances[1].locked = money("0");
    assert_eq!(total(ctx.value(&f)), money("100"));
    f.snapshot.balances[1].currency = "USDT".into();
    fails(&ctx, &f, ReportValuationFailureV1::UnknownCurrency);
}

#[test]
fn report_daily_returns_are_separate_complete_utc_days() {
    let ctx = context();
    let mut frames: Vec<_> = (1..=4).map(|d| frame(&ctx, d * DAY_NS, d * 2)).collect();
    frames[1].prices[0].price = money("60");
    let output = returns(&ctx, &frames).unwrap();
    assert_eq!(
        output.kind,
        ReportCurrencyReturnsKindV1::ReportCurrencyDaily
    );
    assert_eq!(output.annualization_days, 365);
    assert_eq!(output.days.len(), 3);
    assert!((output.days[0].value.unwrap() - 0.1).abs() < 1e-12);
    assert!(output.days.iter().all(|d| d.reason.is_none()));
}

#[test]
fn missing_days_are_not_bridged_or_zero_filled() {
    let ctx = context();
    let frames = vec![
        frame(&ctx, DAY_NS, 2),
        frame(&ctx, 3 * DAY_NS, 6),
        frame(&ctx, 4 * DAY_NS, 8),
    ];
    let output = returns(&ctx, &frames).unwrap();
    assert_eq!(output.days.len(), 3);
    for day in &output.days[..2] {
        assert_eq!(day.value, None);
        assert_eq!(day.reason, Some(ReportCurrencyReturnFailureV1::SnapshotGap));
    }
    assert_eq!(output.days[2].value, Some(0.0));
}

#[test]
fn daily_sampling_needs_only_midnight_valuations_not_an_intraday_grid() {
    // Seven milliseconds does not divide a day and is not a promised snapshot
    // grid. Flat native accounts do not produce those timer observations.
    let mut s = settings();
    s.snapshot_interval_ms = 7;
    let mut session = session();
    session.period_end_ns = count(2 * DAY_NS);
    let ctx = SpotCashContext::new(&s, session, &[instrument()]).unwrap();
    let mut frames = vec![
        frame(&ctx, DAY_NS, 2),
        frame(&ctx, DAY_NS + DAY_NS / 2, 4),
        frame(&ctx, 2 * DAY_NS, 6),
    ];
    frames[1].prices.clear();
    let output = returns(&ctx, &frames).unwrap();
    assert_eq!(output.days[0].value, Some(0.0));
    assert!(output.days[0].reason.is_none());
    frames.remove(1);
    let output = returns(&ctx, &frames).unwrap();
    assert_eq!(output.days[0].value, Some(0.0));
    assert!(output.days[0].reason.is_none());
}

#[test]
fn external_flows_unknown_or_nonzero_never_produce_returns() {
    let ctx = day_context();
    let f = vec![frame(&ctx, DAY_NS, 2), frame(&ctx, 2 * DAY_NS, 4)];
    let mut evidence = flows(&ctx, &f);
    evidence.external_flow_count = None;
    assert!(ctx.daily_returns(&f, &evidence).is_err());
    evidence.external_flow_count = Some(count(1));
    assert!(ctx.daily_returns(&f, &evidence).is_err());
    evidence.external_flow_count = Some(count(0));
    evidence.session_id = Id::new();
    assert!(ctx.daily_returns(&f, &evidence).is_err());
}

#[test]
fn sessions_order_and_snapshot_identity_cannot_be_spliced() {
    let ctx = day_context();
    let first = frame(&ctx, DAY_NS, 2);
    let mut second = frame(&ctx, 2 * DAY_NS, 4);
    second.snapshot.binding.session_id = Id::new();
    assert!(returns(&ctx, &[first.clone(), second]).is_err());
    assert!(returns(&ctx, &[first.clone(), first.clone()]).is_err());
    let mut second = frame(&ctx, 2 * DAY_NS, 4);
    second.snapshot.binding.event_id = first.snapshot.binding.event_id;
    assert!(returns(&ctx, &[first.clone(), second]).is_err());
    let second = frame(&ctx, 2 * DAY_NS, 4);
    assert!(returns(&ctx, &[second, first]).is_err());
}

#[test]
fn partial_utc_days_are_excluded_without_moving_the_clock() {
    let mut session = session();
    session.period_start_ns = count(DAY_NS + 1);
    session.period_end_ns = count(3 * DAY_NS - 1);
    let ctx = SpotCashContext::new(&settings(), session, &[instrument()]).unwrap();
    let f = vec![
        frame(&ctx, DAY_NS + 1, 2),
        frame(&ctx, 2 * DAY_NS, 4),
        frame(&ctx, 3 * DAY_NS - 1, 6),
    ];
    assert!(returns(&ctx, &f).unwrap().days.is_empty());
}

#[test]
fn explicit_native_null_is_required_and_open_candles_are_rejected() {
    let ctx = context();
    let mut f = frame(&ctx, DAY_NS, 2);
    let mut wire = serde_json::to_value(&f.snapshot).unwrap();
    wire.as_object_mut().unwrap().remove("native_base_currency");
    assert!(serde_json::from_value::<NativeSpotCashSnapshotV1>(wire).is_err());
    f.prices[0].bar_open_ns = f.prices[0].event_ns;
    fails(&ctx, &f, ReportValuationFailureV1::InvalidPriceSource);
}

#[test]
fn price_replay_order_is_session_local_even_when_the_frozen_dataset_is_shared() {
    let first = context();
    let first_price = frame(&first, DAY_NS, 2).prices.remove(0);
    let mut second_session = first.session.clone();
    second_session.session_id = Id::new();
    let second = SpotCashContext::new(&settings(), second_session, &[instrument()]).unwrap();
    let mut second_frame = frame(&second, DAY_NS, 2);
    second_frame.prices = vec![first_price];
    fails(
        &second,
        &second_frame,
        ReportValuationFailureV1::InvalidPriceSource,
    );
}

#[test]
fn different_prices_or_snapshot_cannot_reuse_a_global_sequence() {
    let ctx = day_context();
    let first = frame(&ctx, DAY_NS, 2);
    for sequence in [1, 2] {
        let mut second = frame(&ctx, 2 * DAY_NS, 4);
        second.prices[0].observed_sequence = count(sequence);
        second.prices[0].price = money("60");
        assert!(returns(&ctx, &[first.clone(), second]).is_err());
    }
}

#[test]
fn replay_clocks_cannot_run_backwards_or_rewrite_one_source_row() {
    let ctx = day_context();
    let first = frame(&ctx, DAY_NS, 2);
    let mut second = frame(&ctx, 2 * DAY_NS, 4);
    second.prices[0].observed_sequence = count(0);
    assert!(returns(&ctx, &[first.clone(), second]).is_err());
    let mut second = frame(&ctx, 2 * DAY_NS, 4);
    second.prices[0].source_row_key = first.prices[0].source_row_key.clone();
    assert!(returns(&ctx, &[first, second]).is_err());
}

#[test]
fn an_unchanged_known_prior_price_can_be_reused_within_its_frozen_maximum_age() {
    let mut s = settings();
    s.multi_currency_spot_cash
        .as_mut()
        .unwrap()
        .maximum_price_age_ns = count(DAY_NS);
    let mut session = session();
    session.period_end_ns = count(2 * DAY_NS);
    let ctx = SpotCashContext::new(&s, session, &[instrument()]).unwrap();
    let first = frame(&ctx, DAY_NS, 2);
    let mut second = frame(&ctx, 2 * DAY_NS, 4);
    second.prices = first.prices.clone();
    let result = returns(&ctx, &[first, second]).unwrap();
    assert_eq!(result.days[0].value, Some(0.0));
    assert!(result.days[0].reason.is_none());
}

#[test]
fn single_valuation_also_rejects_inconsistent_multi_price_order_evidence() {
    let mut s = settings();
    s.multi_currency_spot_cash
        .as_mut()
        .unwrap()
        .allowed_instrument_ids
        .push("ETH-USDC.HYPERLIQUID".into());
    let btc = instrument();
    let mut eth = btc.clone();
    eth.instrument_id = "ETH-USDC.HYPERLIQUID".into();
    eth.base_currency = "ETH".into();
    let ctx = SpotCashContext::new(&s, session(), &[btc, eth]).unwrap();
    let mut f = frame(&ctx, DAY_NS, 4);
    f.snapshot.balances.push(NativeSpotCashBalanceV1 {
        currency: "ETH".into(),
        total: money("1"),
        free: money("1"),
        locked: money("0"),
    });
    let mut eth_price = f.prices[0].clone();
    eth_price.instrument_id = "ETH-USDC.HYPERLIQUID".into();
    eth_price.source_row_key = "eth-row".into();
    f.prices.push(eth_price);
    fails(&ctx, &f, ReportValuationFailureV1::ReplaySequenceConflict);
    f.prices[1].observed_sequence = count(2);
    f.prices[1].source_row_key = f.prices[0].source_row_key.clone();
    fails(&ctx, &f, ReportValuationFailureV1::PriceSourceConflict);
    f.prices[1].source_row_key = "eth-row".into();
    f.prices[0].available_ns = count(DAY_NS - 1);
    f.prices[0].event_ns = count(DAY_NS - 1);
    fails(&ctx, &f, ReportValuationFailureV1::ReplayClockConflict);
}

#[test]
fn reports_keep_the_actual_policy_frozen_with_the_simulation_settings() {
    let mut s = settings();
    let ctx = SpotCashContext::new(&s, session(), &[instrument()]).unwrap();
    let frozen = serde_json::to_value(s.multi_currency_spot_cash.as_ref().unwrap()).unwrap();
    s.multi_currency_spot_cash
        .as_mut()
        .unwrap()
        .maximum_price_age_ns = count(1);
    s.multi_currency_spot_cash
        .as_mut()
        .unwrap()
        .allowed_instrument_ids
        .clear();
    let frames: Vec<_> = (1..=4).map(|d| frame(&ctx, d * DAY_NS, d * 2)).collect();
    assert_eq!(
        serde_json::to_value(ctx.value(&frames[0]).policy).unwrap(),
        frozen
    );
    assert_eq!(
        serde_json::to_value(returns(&ctx, &frames).unwrap().policy).unwrap(),
        frozen
    );
    let wire = serde_json::to_value(ctx.value(&frames[0])).unwrap();
    assert!(wire["snapshot"].get("policy_artifact_id").is_none());
    assert!(
        wire["snapshot"]
            .get("native_evidence_artifact_id")
            .is_none()
    );
    assert!(wire["snapshot"].get("native_run_id").is_none());
}

#[test]
fn all_intraday_frames_keep_runtime_source_and_order_validation() {
    let ctx = day_context();
    let frames = vec![
        frame(&ctx, DAY_NS, 2),
        frame(&ctx, DAY_NS + DAY_NS / 2, 4),
        frame(&ctx, 2 * DAY_NS, 6),
    ];
    assert!(returns(&ctx, &frames).is_ok());
    let mutations: [fn(&mut NativeSpotValuationFrameV1); 8] = [
        |f| f.snapshot.binding.native_instance_id = native_id(1003),
        |f| f.snapshot.binding.dataset_revision_id = Id::new(),
        |f| f.snapshot.binding.snapshot_sequence = count(2),
        |f| f.prices[0].session_id = Id::new(),
        |f| f.prices[0].native_instance_id = native_id(1003),
        |f| f.prices[0].dataset_revision_id = Id::new(),
        |f| f.prices[0].observed_sequence = count(4),
        |f| f.prices[0].available_ns = count(2 * DAY_NS),
    ];
    for mutate in mutations {
        let mut changed = frames.clone();
        mutate(&mut changed[1]);
        assert!(returns(&ctx, &changed).is_err());
    }
}

#[test]
fn same_clock_publications_use_the_first_true_utc_boundary() {
    let ctx = day_context();
    let first = frame(&ctx, DAY_NS, 2);
    let mut later = frame(&ctx, DAY_NS, 4);
    later.prices = first.prices.clone();
    later.snapshot.balances[0].total = money("80");
    later.snapshot.balances[0].free = money("60");
    let result = returns(&ctx, &[first, later, frame(&ctx, 2 * DAY_NS, 6)]).unwrap();
    assert_eq!(result.days[0].value, Some(0.0));
    assert_eq!(
        result.days[0]
            .opening_snapshot
            .as_ref()
            .unwrap()
            .snapshot_sequence,
        count(2)
    );
}

#[test]
fn decision_build_never_creates_or_replaces_a_published_midnight_boundary() {
    let ctx = day_context();
    let first = frame(&ctx, DAY_NS, 2);
    let mut decision = frame(&ctx, 2 * DAY_NS, 4);
    decision.snapshot.binding.origin = NativeSpotSnapshotOriginV1::DecisionBuild;
    let missing = returns(&ctx, &[first.clone(), decision.clone()]).unwrap();
    assert_eq!(
        missing.days[0].reason,
        Some(ReportCurrencyReturnFailureV1::SnapshotGap)
    );
    let mut published = frame(&ctx, 2 * DAY_NS, 6);
    published.prices = decision.prices.clone();
    decision.snapshot.balances[0].total = money("80");
    decision.snapshot.balances[0].free = money("60");
    let actual = returns(&ctx, &[first, decision, published]).unwrap();
    assert_eq!(actual.days[0].value, Some(0.0));
    assert_eq!(
        actual.days[0]
            .closing_snapshot
            .as_ref()
            .unwrap()
            .snapshot_sequence,
        count(6)
    );
}

#[test]
fn unavailable_midnight_equity_is_not_zero_or_carried_forward() {
    let ctx = day_context();
    for boundary in [0, 1] {
        let mut frames = vec![frame(&ctx, DAY_NS, 2), frame(&ctx, 2 * DAY_NS, 4)];
        frames[boundary].prices.clear();
        let result = returns(&ctx, &frames).unwrap();
        assert_eq!(result.days[0].value, None);
        assert_eq!(
            result.days[0].reason,
            Some(ReportCurrencyReturnFailureV1::ValuationUnavailable)
        );
    }
}

#[test]
fn cash_only_midnight_snapshots_need_no_nonexistent_intraday_observations() {
    let mut s = settings();
    s.snapshot_interval_ms = 1;
    let ctx = SpotCashContext::new(&s, session(), &[instrument()]).unwrap();
    let mut frames: Vec<_> = (1..=4).map(|d| frame(&ctx, d * DAY_NS, d * 2)).collect();
    for f in &mut frames {
        f.snapshot.balances.truncate(1);
        f.prices.clear();
    }
    let result = returns(&ctx, &frames).unwrap();
    assert_eq!(result.days.len(), 3);
    assert!(
        result
            .days
            .iter()
            .all(|day| day.value == Some(0.0) && day.reason.is_none())
    );
}

#[test]
fn flow_receipt_must_match_runtime_window_snapshot_endpoints_and_count() {
    let ctx = day_context();
    let frames = vec![frame(&ctx, DAY_NS, 2), frame(&ctx, 2 * DAY_NS, 4)];
    let evidence = flows(&ctx, &frames);
    assert!(ctx.daily_returns(&frames, &evidence).is_ok());
    let mutations: [fn(&mut NativeSpotCashFlowEvidenceV1); 11] = [
        |e| e.native_instance_id = native_id(1003),
        |e| e.session_id = Id::new(),
        |e| e.dataset_revision_id = Id::new(),
        |e| e.account_id = "OTHER-001".into(),
        |e| e.venue = "OTHER".into(),
        |e| e.period_start_ns = count(DAY_NS - 1),
        |e| e.period_end_ns = count(2 * DAY_NS + 1),
        |e| e.opening_snapshot.event_id = native_id(1004),
        |e| e.closing_snapshot.snapshot_sequence = count(5),
        |e| e.observed_snapshot_count = count(1),
        |e| e.external_flow_count = None,
    ];
    for mutate in mutations {
        let mut changed = evidence.clone();
        mutate(&mut changed);
        assert!(ctx.daily_returns(&frames, &changed).is_err());
    }
    assert!(ctx.daily_returns(&frames[..1], &evidence).is_err());
    assert!(ctx.daily_returns(&[], &evidence).is_err());
    let mut wire = serde_json::to_value(&evidence).unwrap();
    wire["complete"] = serde_json::json!(true);
    assert!(serde_json::from_value::<NativeSpotCashFlowEvidenceV1>(wire).is_err());
    let mut wire = serde_json::to_value(&evidence).unwrap();
    wire.as_object_mut().unwrap().remove("external_flow_count");
    assert!(serde_json::from_value::<NativeSpotCashFlowEvidenceV1>(wire).is_err());
}

#[test]
fn receipt_and_frames_must_cover_both_actual_session_endpoints() {
    let ctx = day_context();
    for (start, end) in [(DAY_NS + 1, 2 * DAY_NS), (DAY_NS, 2 * DAY_NS - 1)] {
        let frames = vec![frame(&ctx, start, 2), frame(&ctx, end, 4)];
        // Even a count/endpoint-consistent projection cannot claim the missing
        // start or end of the observer session was covered by these frames.
        assert!(returns(&ctx, &frames).is_err());
    }
}

#[test]
fn full_utc_days_inside_partial_session_are_kept_without_boundary_synthesis() {
    let mut session = session();
    session.period_start_ns = count(DAY_NS + 1);
    session.period_end_ns = count(4 * DAY_NS - 1);
    let ctx = SpotCashContext::new(&settings(), session, &[instrument()]).unwrap();
    let frames = vec![
        frame(&ctx, DAY_NS + 1, 2),
        frame(&ctx, 2 * DAY_NS, 4),
        frame(&ctx, 3 * DAY_NS, 6),
        frame(&ctx, 4 * DAY_NS - 1, 8),
    ];
    let output = returns(&ctx, &frames).unwrap();
    assert_eq!(output.days.len(), 1);
    assert_eq!(output.days[0].day_start_ns, count(2 * DAY_NS));
    assert_eq!(output.days[0].day_end_ns, count(3 * DAY_NS));
    let missing = vec![frames[0].clone(), frames[2].clone(), frames[3].clone()];
    let output = returns(&ctx, &missing).unwrap();
    assert_eq!(
        output.days[0].reason,
        Some(ReportCurrencyReturnFailureV1::SnapshotGap)
    );
    assert!(output.days[0].opening_snapshot.is_none());
}
