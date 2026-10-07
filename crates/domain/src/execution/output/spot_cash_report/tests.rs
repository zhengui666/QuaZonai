//! Controlled transport/consumer tests, not native execution or fee qualification.
use super::super::{
    equity_curve::portfolio_equity_curve, experiment_summary::native_simulation_summary,
};
use super::*;
use contracts::{
    DbCounter, Id, SchemaV1,
    equity_curve::EquityCurveQuery,
    portfolio::{AllocationTargetV1, NativeModelRefV1},
    science::*,
    spot_fees::*,
};
use serde_json::json;
#[path = "../../../../../../tests/support/execution_models.rs"]
mod models;
const DAY: u64 = 86_400_000_000_000;
const INSTRUMENT: &str = "BTC-USDC.HYPERLIQUID";
const BAR: &str = "BTC-USDC.HYPERLIQUID-1-MINUTE-LAST-EXTERNAL";
fn count(value: u64) -> DbCounter {
    DbCounter::new(value).unwrap()
}
fn amount(value: &str) -> DecimalValue {
    value.parse().unwrap()
}
fn id(value: usize) -> NativeSpotRuntimeIdV1 {
    format!("00000000-0000-4000-8000-{value:012x}")
        .parse()
        .unwrap()
}
fn snapshot(
    session: &NativeSpotCashSessionV1,
    context: &SpotCashContext,
    sequence: u64,
    clock: u64,
    usdc: &str,
    btc: Option<&str>,
    prices: &[NativeSpotPriceV1],
    origin: NativeSpotSnapshotOriginV1,
) -> NativeSpotCashObservationV1 {
    let mut balances = vec![NativeSpotCashBalanceV1 {
        currency: "USDC".into(),
        total: amount(usdc),
        free: amount(usdc),
        locked: amount("0"),
    }];
    if let Some(btc) = btc {
        balances.push(NativeSpotCashBalanceV1 {
            currency: "BTC".into(),
            total: amount(btc),
            free: amount(btc),
            locked: amount("0"),
        });
    }
    let binding = NativeSpotSnapshotBindingV1 {
        native_instance_id: session.native_instance_id,
        event_id: id(sequence as usize),
        dataset_revision_id: session.dataset_revision_id,
        snapshot_sequence: count(sequence),
        session_id: session.session_id,
        account_id: session.account_id.clone(),
        venue: session.venue.clone(),
        asof_ns: count(clock),
        origin,
    };
    let native_balances: Vec<_> = balances.iter().map(|b| json!({
        "currency":b.currency, "total":format!("{} {}", b.total.as_decimal().to_plain_string(), b.currency),
        "free":format!("{} {}", b.free.as_decimal().to_plain_string(), b.currency), "locked":format!("{} {}", b.locked.as_decimal().to_plain_string(), b.currency)
    })).collect();
    let native = json!({
        "event_id":binding.event_id.to_string(), "ts_event":clock, "ts_init":clock,
        "account_id":session.account_id, "account_type":"CASH", "base_currency":null,
        "balances":native_balances, "margins":[],
        "total_equity":["999999 USDC","999999 BTC"],
        "unrealized_pnls":["11111 USDC"], "realized_pnls":["22222 USDC"],
        "base_currency_equity":null,"is_stale":false,"stale_instruments":[],
        "stale_currencies":[],"unpriced_instruments":[]
    });
    let frame = NativeSpotValuationFrameV1 {
        snapshot: NativeSpotCashSnapshotV1 {
            binding,
            account_kind: NativeAccountKind::Cash,
            native_base_currency: None,
            balances_complete: true,
            balances,
        },
        prices: prices.to_vec(),
    };
    NativeSpotCashObservationV1 {
        sequence: count(sequence),
        native_clock_ns: count(clock),
        record: NativeSpotCashObservationKindV1::Snapshot {
            origin,
            native,
            valuation: context.value(&frame),
            frame,
        },
    }
}
fn fixture(gap: bool) -> (NativeSimulationRequestV1, NativeSimulationResultV1) {
    let session = NativeSpotCashSessionV1 {
        session_id: Id::new(),
        native_instance_id: id(1000),
        dataset_revision_id: Id::new(),
        account_id: "HYPERLIQUID-001".into(),
        venue: "HYPERLIQUID".into(),
        period_start_ns: count(DAY),
        period_end_ns: count(3 * DAY),
    };
    let fee_schedule = FrozenSpotFeeScheduleV1 {
        schema_version: SchemaV1,
        source: FrozenSpotFeeSourceV1 {
            status: SpotFeeEvidenceStatusV1::PublicRateScenarioUnverifiedApplicability,
            source_ref: "controlled:public-rate-scenario".into(),
            source_uri: "https://example.org/controlled".into(),
            observed_at_ns: count(DAY),
            assumptions: "Controlled transport fixture; no fee applicability claim".into(),
        },
        valid_from_ns: count(DAY),
        valid_until_ns: count(3 * DAY + 1),
        rules: [SpotFeeOrderSideV1::Buy, SpotFeeOrderSideV1::Sell]
            .into_iter()
            .flat_map(|side| {
                [SpotFeeLiquidityV1::Maker, SpotFeeLiquidityV1::Taker]
                    .into_iter()
                    .map(move |liquidity| FrozenSpotFeeRuleV1 {
                        instrument_id: INSTRUMENT.into(),
                        order_side: side,
                        liquidity,
                        rate: amount("0.001"),
                        basis: SpotFeeBasisV1::QuoteNotional,
                        fee_currency: SpotFeeCurrencyV1::Quote,
                        currency_code: "USDC".into(),
                        currency_precision: 6,
                        rounding: SpotFeeRoundingV1::HalfEvenCurrencyPrecision,
                    })
            })
            .collect(),
    };
    let settings = NativeSimulationSettingsV1 {
        multi_currency_spot_cash: Some(NativeSpotCashPolicyV1 {
            schema_version: SchemaV1,
            mode: NativeSpotCashModeV1::MultiCurrencyCash,
            report_currency: "USDC".into(),
            price_method: NativeSpotPriceMethodV1::ClosedBarClose,
            allowed_instrument_ids: vec![INSTRUMENT.into()],
            returns_policy: ReportCurrencyDailyPolicyV1::FreshSimulationNoExternalFlows,
            daily_sampling: NativeSpotDailySamplingV1::NativeUtcMidnightBoundaries,
            maximum_price_age_ns: count(DAY),
        }),
        schema_version: SchemaV1,
        base_currency: "USDC".into(),
        starting_capital: amount("1000"),
        account_kind: NativeAccountKind::Cash,
        leverage: amount("1"),
        fee_model: NativeModelRefV1::FrozenSpotFeeScenario {
            schema_version: SchemaV1,
            native_version: "0.63.0".into(),
            parameters: FrozenSpotFeeScenarioParametersV1 {
                acceptance: SpotCashFeeAcceptanceV1::PublicRateScenarioUnverifiedApplicability,
                schedule: fee_schedule.clone(),
            },
        },
        fill_model: models::fill(),
        latency_model: models::latency(1),
        snapshot_interval_ms: 86_400_000,
        exposure_tolerance: amount("0.000001"),
        fee_rates: vec![NativeFeeRateV1 {
            instrument_id: INSTRUMENT.into(),
            maker: amount("0.001"),
            taker: amount("0.001"),
        }],
    };
    let instrument = NativeSpotInstrumentV1 {
        instrument_id: INSTRUMENT.into(),
        venue: "HYPERLIQUID".into(),
        native_kind: "CurrencyPair".into(),
        base_currency: "BTC".into(),
        quote_currency: "USDC".into(),
        is_inverse: false,
        has_expiration: false,
        multiplier: amount("1"),
    };
    let context = SpotCashContext::new(&settings, session.clone(), &[instrument.clone()]).unwrap();
    let mut observations = vec![snapshot(
        &session,
        &context,
        1,
        DAY,
        "1000",
        None,
        &[],
        NativeSpotSnapshotOriginV1::NativePublication,
    )];
    for (clock, price, publication) in [
        (DAY + 1, "100", false),
        (2 * DAY, "110", true),
        (3 * DAY, "120", true),
    ] {
        let seq = observations.len() as u64 + 1;
        let key = format!("controlled:{clock}");
        let native = json!({"bar_type":BAR,"open":price,"high":price,"low":price,"close":price,
            "volume":"1000","ts_event":clock,"ts_init":clock});
        observations.push(NativeSpotCashObservationV1 {
            sequence: count(seq),
            native_clock_ns: count(clock),
            record: NativeSpotCashObservationKindV1::Bar {
                event_time: NativeSpotBarEventTimeV1::CloseExclusive,
                native,
                source_row_key: key.clone(),
                bar_open_ns: count(clock - 1),
                bar_close_ns: count(clock),
            },
        });
        if gap && clock == 2 * DAY {
            continue;
        }
        let prices = vec![NativeSpotPriceV1 {
            session_id: session.session_id,
            native_instance_id: session.native_instance_id,
            dataset_revision_id: session.dataset_revision_id,
            source_row_key: key,
            instrument_id: INSTRUMENT.into(),
            method: NativeSpotPriceMethodV1::ClosedBarClose,
            observed_sequence: count(seq),
            bar_open_ns: count(clock - 1),
            event_ns: count(clock),
            available_ns: count(clock),
            price: amount(price),
        }];
        observations.push(snapshot(
            &session,
            &context,
            seq + 1,
            clock,
            "100",
            Some("9"),
            &prices,
            if publication {
                NativeSpotSnapshotOriginV1::NativePublication
            } else {
                NativeSpotSnapshotOriginV1::DecisionBuild
            },
        ));
    }
    let frames: Vec<_> = observations
        .iter()
        .filter_map(|o| match &o.record {
            NativeSpotCashObservationKindV1::Snapshot { frame, .. } => Some(frame.clone()),
            _ => None,
        })
        .collect();
    let flow_evidence = NativeSpotCashFlowEvidenceV1 {
        native_run_id: id(1001),
        native_instance_id: session.native_instance_id,
        session_id: session.session_id,
        dataset_revision_id: session.dataset_revision_id,
        account_id: session.account_id.clone(),
        venue: session.venue.clone(),
        period_start_ns: session.period_start_ns,
        period_end_ns: session.period_end_ns,
        opening_snapshot: frames.first().unwrap().snapshot.binding.clone(),
        closing_snapshot: frames.last().unwrap().snapshot.binding.clone(),
        observed_snapshot_count: count(frames.len() as u64),
        external_flow_count: Some(count(0)),
    };
    let daily_returns = context.daily_returns(&frames, &flow_evidence).unwrap();
    let summary = BTreeMap::from([
        ("venues.total".into(), "1".into()),
        ("orders.open".into(), "0".into()),
        ("orders.inflight".into(), "0".into()),
    ]);
    let statistics = if gap {
        Vec::new()
    } else {
        [
            "Average (Return)",
            "Returns Volatility (365 days)",
            "Sharpe Ratio (365 days)",
            "Max Drawdown",
        ]
        .into_iter()
        .map(|key| NativeStatisticV1 {
            group: NativeStatisticGroup::Returns,
            native_key: key.into(),
            currency: None,
            value: Some(0.01),
            reason_code: None,
        })
        .collect()
    };
    let report = NativeSpotCashReportV1 {
        schema_version: SchemaV1,
        session: session.clone(),
        native_run_receipt: json!({"instance_id":session.native_instance_id,"run_id":id(1001),
            "run_started":1,"run_finished":2,"backtest_start":DAY,"backtest_end":3*DAY,
            "iterations":3,"total_events":0,"total_orders":0,"total_positions":0,"summary":summary}),
        instruments: vec![instrument],
        observations,
        flow_evidence,
        daily_returns,
        statistics,
        fee_schedule,
        source_evidence: None,
        fee_acceptance: SpotCashFeeAcceptanceV1::PublicRateScenarioUnverifiedApplicability,
    };
    let canonical_snapshots: Vec<_> = report
        .observations
        .iter()
        .filter_map(|o| match &o.record {
            NativeSpotCashObservationKindV1::Snapshot {
                origin: NativeSpotSnapshotOriginV1::NativePublication,
                native,
                ..
            } => {
                let mut raw = native.clone();
                raw["event_id"] = json!(format!("event-{}", o.sequence.get()));
                raw["ts_event"] = json!(o.native_clock_ns.get().to_string());
                raw["ts_init"] = json!(o.native_clock_ns.get().to_string());
                Some(raw)
            }
            _ => None,
        })
        .collect();
    let result = NativeSimulationResultV1 {
        spot_cash_report: Some(report),
        schema_version: SchemaV1,
        native_version: "0.63.0".into(),
        iterations: count(3),
        events: count(0),
        orders: count(0),
        positions: count(0),
        consumed_target_points: count(1),
        summary: summary.clone(),
        statistics: vec![NativeStatisticV1 {
            group: NativeStatisticGroup::Pnl,
            native_key: "PnL (total)".into(),
            currency: Some("BTC".into()),
            value: Some(42.0),
            reason_code: None,
        }],
        returns_kind: NativeReturnsKind::PortfolioDaily,
        returns_status: MetricStatus::InsufficientData,
        returns_reason: Some("PORTFOLIO_DAILY_RETURNS_UNAVAILABLE".into()),
        returns: vec![],
        canonical_result: json!({"schema":"nautilus-backtest-result/v1","summary":summary,
            "run":{"backtest_start_ns":DAY.to_string(),"backtest_end_ns":(3*DAY).to_string(),"outcome":"completed",
                "iterations":"3","total_events":"0","total_orders":"0","total_positions":"0"},
            "accounts":[{"Cash":{"allow_borrowing":false,"base":{"id":session.account_id,"account_type":"CASH","base_currency":null,
                "balances_starting":{"USDC":"1000 USDC"},"commissions":{"BTC":"0.01 BTC","USDC":"0.25 USDC"}}}}],
            "portfolio_snapshots":canonical_snapshots}),
    };
    let request = NativeSimulationRequestV1 {
        schema_version: SchemaV1,
        settlements: vec![],
        settings,
        selection: NativeBarSelectionV1 {
            schema_version: SchemaV1,
            bar_types: vec![BAR.into()],
            event_start_ns: count(DAY),
            event_end_ns: count(3 * DAY + 1),
            decision_cutoff_ns: count(3 * DAY + 1),
            maximum_rows: 16,
        },
        target_points: vec![NativeTargetPointV1 {
            schema_version: SchemaV1,
            asof_ns: count(DAY + 1),
            valid_until_ns: count(DAY + 100),
            targets: vec![AllocationTargetV1 {
                instrument_id: INSTRUMENT.into(),
                currency: "USDC".into(),
                weight: amount("0.9"),
            }],
            cash_weight: amount("0.1"),
        }],
    };
    (request, result)
}
fn report_mut(result: &mut NativeSimulationResultV1) -> &mut NativeSpotCashReportV1 {
    result.spot_cash_report.as_mut().unwrap()
}
#[test]
fn complete_report_binds_all_observations_and_preserves_native_canonical() {
    let (request, result) = fixture(false);
    let original = result.canonical_result.clone();
    binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).unwrap();
    assert_eq!(result.canonical_result, original);
}
#[test]
fn canonical_ring_tail_accepts_truncation_but_rejects_changed_or_invented_rows() {
    let (request, mut result) = fixture(false);
    result.canonical_result["portfolio_snapshots"]
        .as_array_mut()
        .unwrap()
        .remove(0);
    binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).unwrap();
    result.canonical_result["portfolio_snapshots"][0]["balances"][0]["total"] = json!("999 USDC");
    assert!(binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).is_err());
}
#[test]
fn independently_consistent_foreign_dataset_still_requires_frozen_outer_identity() {
    let (_, result) = fixture(false);
    dataset(
        &result,
        result
            .spot_cash_report
            .as_ref()
            .unwrap()
            .session
            .dataset_revision_id,
    )
    .unwrap();
    assert!(dataset(&result, Id::new()).is_err());
}
#[test]
fn receipt_and_observer_tape_cannot_cross_sessions_runs_or_sequences() {
    let (request, original) = fixture(false);
    for case in 0..8 {
        let mut result = original.clone();
        let report = report_mut(&mut result);
        match case {
            0 => report.native_run_receipt["instance_id"] = json!(id(3333)),
            1 => report.native_run_receipt["run_id"] = json!(id(3333)),
            2 => report.native_run_receipt["total_orders"] = json!(1),
            3 => report.observations[2].sequence = count(55),
            4 => report.observations[2].native_clock_ns = count(0),
            5 => report.session.session_id = Id::new(),
            6 => report.flow_evidence.external_flow_count = None,
            _ => report.flow_evidence.external_flow_count = Some(count(1)),
        }
        assert!(
            binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn snapshot_null_complete_balances_and_real_event_id_are_required() {
    let (request, original) = fixture(false);
    for case in 0..7 {
        let mut result = original.clone();
        let report = report_mut(&mut result);
        let NativeSpotCashObservationKindV1::Snapshot { native, frame, .. } =
            &mut report.observations[2].record
        else {
            panic!()
        };
        match case {
            0 => {
                native.as_object_mut().unwrap().remove("base_currency");
            }
            1 => native["base_currency"] = json!("USDC"),
            2 => native["event_id"] = json!(id(8888)),
            3 => {
                native["balances"].as_array_mut().unwrap().pop();
            }
            4 => native["balances"][0]["free"] = json!("99 USDC"),
            5 => frame.snapshot.binding.origin = NativeSpotSnapshotOriginV1::NativePublication,
            _ => frame.snapshot.balances_complete = false,
        }
        assert!(
            binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn unchanged_native_snapshot_cannot_bind_an_invented_report_total_or_mark() {
    let (request, original) = fixture(false);
    for case in 0..5 {
        let mut result = original.clone();
        let report = report_mut(&mut result);
        let NativeSpotCashObservationKindV1::Snapshot {
            frame, valuation, ..
        } = &mut report.observations[2].record
        else {
            panic!()
        };
        match case {
            0 => {
                if let ReportCurrencyValuationOutcomeV1::Complete { total, .. } =
                    &mut valuation.outcome
                {
                    *total = amount("12345");
                }
            }
            1 => frame.prices[0].price = amount("1"),
            2 => frame.prices[0].available_ns = count(3 * DAY),
            3 => frame.prices[0].source_row_key = "different-row".into(),
            _ => frame.prices[0].observed_sequence = count(999),
        }
        assert!(
            binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).is_err(),
            "case {case}"
        );
    }
}
#[test]
fn a_gap_is_preserved_and_cannot_be_hidden_by_a_finite_statistic() {
    let (request, mut result) = fixture(true);
    binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).unwrap();
    assert_eq!(
        returns_state(result.spot_cash_report.as_ref().unwrap()).0,
        MetricStatus::Failed
    );
    report_mut(&mut result).statistics.push(NativeStatisticV1 {
        group: NativeStatisticGroup::Returns,
        native_key: "Average (Return)".into(),
        currency: None,
        value: Some(0.0),
        reason_code: None,
    });
    assert!(binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).is_err());
}
#[test]
fn frozen_fee_scenario_cannot_silently_change_or_claim_verified_applicability() {
    let (request, original) = fixture(false);
    for case in 0..3 {
        let mut result = original.clone();
        let report = report_mut(&mut result);
        match case {
            0 => report.fee_schedule.rules[0].rate = amount("0"),
            1 => report.fee_schedule.source.status = SpotFeeEvidenceStatusV1::DataBacked,
            _ => report.fee_schedule.valid_until_ns = count(2 * DAY),
        }
        assert!(binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).is_err());
    }
}
#[test]
fn graph_metrics_and_summary_select_the_same_separate_report_basis() {
    let (request, result) = fixture(false);
    let series = portfolio_equity_curve(&request, &result, &EquityCurveQuery::default()).unwrap();
    assert_eq!(series.points.first().unwrap().value, Some(amount("1000")));
    assert_eq!(series.points.last().unwrap().value, Some(amount("1180")));
    assert_eq!(series.source_point_count.get(), 4);
    let summary = native_simulation_summary(&request, &result).unwrap();
    assert_eq!(summary.daily_return_count.get(), 2);
    assert!(summary.native_account_pnl.value.is_none());
    assert!(summary.commissions.is_none());
    assert_eq!(summary.sharpe_ratio.scope, "REPORT_CURRENCY_DAILY_RETURNS");
    let spot = summary.spot_cash.unwrap();
    assert_eq!(spot.valuation_change, Some(amount("180")));
    assert_eq!(spot.commissions_by_currency.len(), 2);
    assert_eq!(spot.commissions_by_currency[0].currency, "BTC");
    assert_eq!(spot.commissions_by_currency[0].amount, amount("0.01"));
    assert_eq!(
        spot.native_pnl_by_currency[0].currency.as_deref(),
        Some("BTC")
    );
    let (metrics, _) = simulation::metrics(Id::new(), Id::new(), &request, &result).unwrap();
    assert!(metrics
        .iter()
        .all(|m| m.scope == "portfolio.report_currency_daily" && m.observation_count.get() == 2));
}
#[test]
fn new_mode_and_legacy_result_cannot_be_mixed_and_old_wire_omits_new_fields() {
    let (mut request, mut result) = fixture(false);
    request.settings.multi_currency_spot_cash = None;
    request.settings.fee_model = models::fee();
    assert!(simulation::binding(&request, &result).is_err());
    result.spot_cash_report = None;
    let encoded = serde_json::to_value(&result).unwrap();
    assert!(encoded.get("spot_cash_report").is_none());
    let roundtrip: NativeSimulationResultV1 = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(serde_json::to_value(roundtrip).unwrap(), encoded);
}

fn refresh_derived(request: &NativeSimulationRequestV1, result: &mut NativeSimulationResultV1) {
    let report = report_mut(result);
    let context = SpotCashContext::new(
        &request.settings,
        report.session.clone(),
        &report.instruments,
    )
    .unwrap();
    let mut latest = BTreeMap::new();
    let mut frames = Vec::new();
    for observation in &mut report.observations {
        match &mut observation.record {
            NativeSpotCashObservationKindV1::Bar {
                event_time: _,
                native,
                source_row_key,
                bar_open_ns,
                bar_close_ns,
            } => {
                let instrument_id = report.instruments[0].instrument_id.clone();
                latest.insert(
                    instrument_id.clone(),
                    NativeSpotPriceV1 {
                        session_id: report.session.session_id,
                        native_instance_id: report.session.native_instance_id,
                        dataset_revision_id: report.session.dataset_revision_id,
                        source_row_key: source_row_key.clone(),
                        instrument_id,
                        method: NativeSpotPriceMethodV1::ClosedBarClose,
                        observed_sequence: observation.sequence,
                        bar_open_ns: *bar_open_ns,
                        event_ns: *bar_close_ns,
                        available_ns: observation.native_clock_ns,
                        price: decimal(&native["close"]).unwrap(),
                    },
                );
            }
            NativeSpotCashObservationKindV1::Snapshot {
                frame, valuation, ..
            } => {
                frame.prices = latest.values().cloned().collect();
                *valuation = context.value(frame);
                frames.push(frame.clone());
            }
        }
    }
    report.daily_returns = context
        .daily_returns(&frames, &report.flow_evidence)
        .unwrap();
    if returns_state(report).0 != MetricStatus::Ok {
        report.statistics.clear();
    }
}
#[test]
fn distinct_stablecoins_require_their_observed_direct_price_and_never_parity() {
    let (request, result) = fixture(false);
    let request: NativeSimulationRequestV1 = serde_json::from_str(
        &serde_json::to_string(&request)
            .unwrap()
            .replace("BTC", "USDT"),
    )
    .unwrap();
    let mut result: NativeSimulationResultV1 = serde_json::from_str(
        &serde_json::to_string(&result)
            .unwrap()
            .replace("BTC", "USDT"),
    )
    .unwrap();
    for observation in &mut report_mut(&mut result).observations {
        if let NativeSpotCashObservationKindV1::Bar { native, .. } = &mut observation.record {
            for field in ["open", "high", "low", "close"] {
                native[field] = json!("0.96");
            }
        }
    }
    refresh_derived(&request, &mut result);
    binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).unwrap();
    let series = portfolio_equity_curve(&request, &result, &EquityCurveQuery::default()).unwrap();
    assert_eq!(series.points.last().unwrap().value, Some(amount("108.64")));
    assert_ne!(series.points.last().unwrap().value, Some(amount("109")));
}
#[test]
fn an_actual_stale_observation_has_no_partial_or_carried_chart_total() {
    let (mut request, mut result) = fixture(false);
    request
        .settings
        .multi_currency_spot_cash
        .as_mut()
        .unwrap()
        .maximum_price_age_ns = count(1);
    let report = report_mut(&mut result);
    let observation = &mut report.observations[2];
    observation.native_clock_ns = count(DAY + 5);
    let NativeSpotCashObservationKindV1::Snapshot { native, frame, .. } = &mut observation.record
    else {
        panic!()
    };
    native["ts_event"] = json!(DAY + 5);
    native["ts_init"] = json!(DAY + 5);
    frame.snapshot.binding.asof_ns = count(DAY + 5);
    refresh_derived(&request, &mut result);
    binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).unwrap();
    let series = portfolio_equity_curve(&request, &result, &EquityCurveQuery::default()).unwrap();
    let stale = series
        .points
        .iter()
        .find(|p| p.timestamp_ns == count(DAY + 5))
        .unwrap();
    assert!(stale.value.is_none());
    assert_eq!(
        stale.reason_code.as_deref(),
        Some("REPORT_CURRENCY_VALUATION_STALE_PRICE")
    );
}
#[test]
fn fresh_capital_cannot_be_replaced_by_a_consistent_revalued_opening_inventory() {
    let (request, mut result) = fixture(false);
    let report = report_mut(&mut result);
    let NativeSpotCashObservationKindV1::Snapshot { native, frame, .. } =
        &mut report.observations[0].record
    else {
        panic!()
    };
    native["balances"][0]["total"] = json!("900 USDC");
    native["balances"][0]["free"] = json!("900 USDC");
    frame.snapshot.balances[0].total = amount("900");
    frame.snapshot.balances[0].free = amount("900");
    result.canonical_result["portfolio_snapshots"][0]["balances"][0]["total"] = json!("900 USDC");
    result.canonical_result["portfolio_snapshots"][0]["balances"][0]["free"] = json!("900 USDC");
    refresh_derived(&request, &mut result);
    assert!(binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).is_err());
}
#[test]
fn borrowing_or_missing_native_null_is_not_an_opt_in_cash_account() {
    let (request, original) = fixture(false);
    for case in 0..3 {
        let mut result = original.clone();
        match case {
            0 => result.canonical_result["accounts"][0]["Cash"]["allow_borrowing"] = json!(true),
            1 => {
                result.canonical_result["accounts"][0]["Cash"]["base"]
                    .as_object_mut()
                    .unwrap()
                    .remove("base_currency");
            }
            _ => {
                result.canonical_result["accounts"][0]["Cash"]["base"]["base_currency"] =
                    json!("USDC")
            }
        }
        assert!(simulation::binding(&request, &result).is_err());
    }
}
#[test]
fn typed_tape_rejects_unknown_fields_and_missing_explicit_snapshot_null() {
    let (_, result) = fixture(false);
    let report = result.spot_cash_report.unwrap();
    let mut observation = serde_json::to_value(&report.observations[0]).unwrap();
    observation["extra"] = json!(1);
    assert!(serde_json::from_value::<NativeSpotCashObservationV1>(observation).is_err());
    let mut observation = serde_json::to_value(&report.observations[0]).unwrap();
    observation["frame"]["snapshot"]
        .as_object_mut()
        .unwrap()
        .remove("native_base_currency");
    assert!(serde_json::from_value::<NativeSpotCashObservationV1>(observation).is_err());
}

#[test]
fn same_clock_display_selects_last_real_sequence_but_daily_boundary_keeps_first_publication() {
    let (request, mut result) = fixture(false);
    let report = report_mut(&mut result);
    let context = SpotCashContext::new(
        &request.settings,
        report.session.clone(),
        &report.instruments,
    )
    .unwrap();
    let NativeSpotCashObservationKindV1::Snapshot { frame, .. } = &report.observations[4].record
    else {
        panic!()
    };
    let decision = snapshot(
        &report.session,
        &context,
        777,
        2 * DAY,
        "100",
        Some("8.99"),
        &frame.prices,
        NativeSpotSnapshotOriginV1::DecisionBuild,
    );
    report.observations.insert(5, decision);
    for (index, observation) in report.observations.iter_mut().enumerate() {
        observation.sequence = count(index as u64 + 1);
        if let NativeSpotCashObservationKindV1::Snapshot { frame, .. } = &mut observation.record {
            frame.snapshot.binding.snapshot_sequence = observation.sequence;
        }
    }
    let frames: Vec<_> = report
        .observations
        .iter()
        .filter_map(|o| match &o.record {
            NativeSpotCashObservationKindV1::Snapshot { frame, .. } => Some(frame),
            _ => None,
        })
        .collect();
    report.flow_evidence.observed_snapshot_count = count(frames.len() as u64);
    report.flow_evidence.closing_snapshot = frames.last().unwrap().snapshot.binding.clone();
    refresh_derived(&request, &mut result);
    binding(&request, &result, result.spot_cash_report.as_ref().unwrap()).unwrap();
    let series = portfolio_equity_curve(&request, &result, &EquityCurveQuery::default()).unwrap();
    assert!(!series.sampled);
    assert_eq!(
        series.sampled,
        (series.points.len() as u64) < series.window_point_count.get()
    );
    assert_eq!(series.source_point_count.get(), 5);
    assert_eq!(
        series
            .points
            .iter()
            .find(|p| p.timestamp_ns == count(2 * DAY))
            .unwrap()
            .value,
        Some(amount("1088.9"))
    );
    let report = result.spot_cash_report.as_ref().unwrap();
    assert_eq!(report.daily_returns.days[0].value, Some(0.09));
    assert_eq!(
        report.daily_returns.days[1]
            .opening_snapshot
            .as_ref()
            .unwrap()
            .origin,
        NativeSpotSnapshotOriginV1::NativePublication
    );
}

#[test]
fn minimum_observations_uses_checked_report_metrics_without_counting_across_a_gap() {
    let (request, result) = fixture(false);
    let checked_count = |request: &NativeSimulationRequestV1, result: &NativeSimulationResultV1| {
        let (values, _) = simulation::metrics(Id::new(), Id::new(), request, result).unwrap();
        values
            .iter()
            .map(|metric| metric.observation_count.get())
            .min()
            .unwrap_or(0)
    };
    let complete = checked_count(&request, &result);
    assert_eq!(complete, 2);
    assert!(complete >= 2);
    assert!(complete < 3);
    let (request, mut result) = fixture(true);
    assert_eq!(checked_count(&request, &result), 0);
    let report = report_mut(&mut result);
    // A supplied return/count cannot replace the missing actual boundary.
    report.daily_returns.days[0].value = Some(0.1);
    report.daily_returns.days[0].reason = None;
    assert!(simulation::metrics(Id::new(), Id::new(), &request, &result).is_err());
}
#[test]
fn legacy_metrics_keep_the_original_native_observation_count_and_missingness() {
    let (mut request, mut result) = fixture(false);
    request.settings.multi_currency_spot_cash = None;
    request.settings.fee_model = models::fee();
    let report = result.spot_cash_report.take().unwrap();
    result.statistics = report
        .statistics
        .into_iter()
        .map(|mut statistic| {
            statistic.native_key = statistic.native_key.replace("365 days", "252 days");
            statistic
        })
        .collect();
    result.canonical_result["accounts"][0]["Cash"]["base"]["base_currency"] = json!("USDC");
    for snapshot in result.canonical_result["portfolio_snapshots"]
        .as_array_mut()
        .unwrap()
    {
        snapshot["base_currency"] = json!("USDC");
        snapshot["total_equity"] = json!(["1000 USDC"]);
    }
    result.returns_status = MetricStatus::Ok;
    result.returns_reason = None;
    result.returns = vec![
        NativeReturnV1 {
            timestamp_ns: count(DAY),
            value: Some(0.1),
            reason_code: None,
        },
        NativeReturnV1 {
            timestamp_ns: count(2 * DAY),
            value: Some(0.1),
            reason_code: None,
        },
    ];
    let (metrics, _) = simulation::metrics(Id::new(), Id::new(), &request, &result).unwrap();
    assert!(
        metrics
            .iter()
            .all(|metric| metric.observation_count.get() == 2 && metric.scope == "portfolio")
    );
    result.returns[1].value = None;
    result.returns[1].reason_code = Some("NATIVE_RETURN_UNAVAILABLE".into());
    result.returns_status = MetricStatus::Failed;
    result.returns_reason = Some("NATIVE_RETURN_UNAVAILABLE".into());
    let (metrics, _) = simulation::metrics(Id::new(), Id::new(), &request, &result).unwrap();
    assert!(
        metrics
            .iter()
            .all(|metric| metric.observation_count.get() == 2
                && metric.status == MetricStatus::Failed)
    );
}
