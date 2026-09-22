from pathlib import Path


def replace(path, old, new):
    p = Path(path)
    s = p.read_text()
    assert s.count(old) == 1, (path, old)
    p.write_text(s.replace(old, new))


p = Path('apps/job/tests/support/polymarket.rs')
s = p.read_text()
s += '''
/// Independently frozen fixture source declaration, not inferred from simulation output.
pub fn settlement_groups(
    event: u64,
    available: u64,
    payouts: [&str; 2],
) -> Vec<contracts::settlement::NativeSettlementGroupV1> {
    vec![contracts::settlement::NativeSettlementGroupV1 {
        condition_id: "fixture-event".into(),
        source_reference: "SYNTHETIC_NATIVE_REGRESSION".into(),
        outcomes: IDS.iter().zip(payouts).map(|(id, price)| {
            contracts::settlement::NativeSettlementOutcomeV1 {
                instrument_id: (*id).into(),
                close_price: price.parse().unwrap(),
                ts_event: contracts::DbCounter::new(event).unwrap(),
                ts_init: contracts::DbCounter::new(available).unwrap(),
            }
        }).collect(),
    }]
}
'''
p.write_text(s)
replace('apps/job/tests/polymarket.rs',
    '    target.valid_until_ns = request.selection.event_end_ns;',
    '''    // Target trading authority ends at expiry; existing positions may wait
    // longer for source-observed resolution without authorizing another trade.
    target.valid_until_ns = market::count(expiration);''')
replace('apps/job/tests/polymarket.rs',
    '''    if let Some(payouts) = resolved {
        prediction::settle(''',
    '''    if let Some(payouts) = resolved {
        request.settlements = prediction::settlement_groups(expiration, (19 + delay_minutes) * STEP, payouts);
        prediction::settle(''')
p = Path('apps/job/tests/polymarket.rs')
s = p.read_text()
s += '''
#[test]
fn unregistered_changed_and_missing_settlements_cannot_change_a_frozen_replay() {
    let (catalog, request) = simulation("0", Some(["1.0000", "0.0000"]), 0);
    for mutation in 0..4 {
        let mut changed = request.clone();
        match mutation {
            0 => changed.settlements.clear(),
            1 => {
                for outcome in &mut changed.settlements[0].outcomes {
                    outcome.close_price = "0.5".parse().unwrap();
                }
            }
            2 => {
                for outcome in &mut changed.settlements[0].outcomes {
                    outcome.ts_init = market::count(outcome.ts_init.get() - 1);
                }
            }
            _ => {
                for outcome in &mut changed.settlements[0].outcomes {
                    outcome.ts_event = market::count(outcome.ts_event.get() - 1);
                }
            }
        }
        let error = simulate(catalog.path(), &changed).unwrap_err();
        assert!(error.contains("POLYMARKET_SETTLEMENT_SOURCE_MISMATCH"), "{mutation}: {error}");
    }
    let (missing, mut frozen) = simulation("0", None, 0);
    frozen.settlements = request.settlements;
    assert!(simulate(missing.path(), &frozen).unwrap_err()
        .contains("POLYMARKET_SETTLEMENT_SOURCE_MISMATCH"));
}

#[test]
fn native_settlement_rejects_incoherent_complete_condition_payouts() {
    for payouts in [["1.0000", "1.0000"], ["0.8000", "0.8000"], ["0.0000", "0.0000"]] {
        let (catalog, request) = simulation("0", Some(payouts), 0);
        let error = simulate(catalog.path(), &request).unwrap_err();
        assert!(error.contains("polymarket_incoherent_payout_vector"), "{error}");
    }
}

#[test]
fn one_traded_outcome_still_requires_the_complete_source_payout_vector() {
    let (catalog, mut request) = simulation("0", Some(["1.0000", "0.0000"]), 0);
    request.selection.bar_types.truncate(1);
    request.settings.fee_rates.truncate(1);
    request.target_points[0].targets.truncate(1);
    request.target_points[0].cash_weight = "0.8".parse().unwrap();
    let result = simulate(catalog.path(), &request).unwrap();
    assert!((pnl(&result) - 300.0).abs() < 0.0001);
    request.settlements[0].outcomes[1].close_price = "0.5".parse().unwrap();
    assert!(simulate(catalog.path(), &request).is_err());
    request.settlements[0].outcomes.truncate(1);
    assert!(simulate(catalog.path(), &request).is_err());
}

#[test]
fn close_on_the_exclusive_holding_boundary_is_not_early_cash() {
    let (catalog, mut request) = simulation("0", Some(["1.0000", "0.0000"]), 0);
    request.selection.event_end_ns = market::count(19 * STEP);
    request.selection.decision_cutoff_ns = request.selection.event_end_ns;
    assert!(simulate(catalog.path(), &request).unwrap_err().contains("POLYMARKET_PENDING_RESOLUTION"));
}

#[test]
fn adding_a_close_to_an_existing_bar_catalog_does_not_authorize_it() {
    use nautilus_model::{data::InstrumentClose, enums::InstrumentCloseType, types::Price};
    use nautilus_persistence::backend::catalog::ParquetDataCatalog;
    let (root, request) = simulation("0", Some(["1.0000", "0.0000"]), 0);
    let catalog = ParquetDataCatalog::from_uri(root.path().to_str().unwrap(), None, None, None, None).unwrap();
    let extra = InstrumentClose::new(IDS[0].parse().unwrap(), Price::from("0.5000"),
        InstrumentCloseType::ContractExpired, (18 * STEP).into(), (20 * STEP).into());
    catalog.write_to_parquet(&[extra], None, None, None).unwrap();
    assert!(simulate(root.path(), &request).is_err());
}

#[test]
fn portfolio_build_rejects_expired_and_cross_expiry_targets_before_publishing_weights() {
    let (_unused, original, model) = market::portfolio();
    let cutoff = original.selection.decision_cutoff_ns.get();
    let until = cutoff + u64::from(original.mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000;
    for (expiration, accepted) in [(cutoff - 1, false), (cutoff, false), (until - 1, false), (until, true), (until + 1, true)] {
        let mut request = original.clone();
        let catalog = tempfile::tempdir().unwrap();
        prediction::write_catalog(catalog.path(), 20, "0", expiration, true);
        prediction::settings(&mut request.execution_settings, "0", expiration);
        request.mandate.base_currency = "pUSD".into();
        request.current_weights.base_currency = "pUSD".into();
        request.selection.bar_types = IDS.iter().map(|id| format!("{id}-1-MINUTE-LAST-EXTERNAL")).collect();
        for ((asset, weight), id) in request.assets.iter_mut().zip(&mut request.current_weights.weights).zip(IDS) {
            asset.instrument_id = id.into();
            asset.currency = "pUSD".into();
            weight.instrument_id = id.into();
            weight.currency = "pUSD".into();
        }
        let result = job::portfolio::build(catalog.path(), &request, |id| {
            if id == request.mandate.constraints.transaction_costs_ref {
                Ok(serde_json::to_vec(&request.execution_settings)?)
            } else if id == request.current_weights_artifact_id {
                Ok(serde_json::to_vec(&request.current_weights)?)
            } else {
                Ok(model.clone())
            }
        });
        if accepted {
            assert!(result.is_ok(), "expiry={expiration}: {result:?}");
        } else {
            let error = result.unwrap_err().to_string();
            assert!(error.contains("polymarket_target_lifetime"), "expiry={expiration}: {error}");
        }
    }
}
'''
p.write_text(s)

# Direct upstream catalog round-trips catch input ordering bugs without depending
# on a fake SDK or representing identical reception times as observed nanoseconds.
p = Path('apps/job/src/bin/polymarket-history.rs')
s = p.read_text()
needle = '    #[test]\n    fn schema_and_timestamp_overflow_are_rejected()'
assert s.count(needle) == 1
addition = '''    #[test]
    fn equal_reception_times_preserve_original_arrival_order_in_native_partitions() {
        use nautilus_model::{data::{BookOrder, Data}, enums::{BookAction, OrderSide, RecordFlag}};
        let mut input = archive();
        let id = input.instruments[0].id();
        for row in &mut input.trades { row.ts_init = 50_u64.into(); }
        for event in [20_u64, 10_u64] {
            input.quotes.push(QuoteTick::new(id, Price::from("0.4000"), Price::from("0.4500"),
                Quantity::from("1.000000"), Quantity::from("2.000000"), event.into(), 50_u64.into()));
        }
        input.deltas = vec![
            OrderBookDelta::new(id, BookAction::Add,
                BookOrder::new(OrderSide::Buy, Price::from("0.4000"), Quantity::from("1.000000"), 1),
                0, 10, 20_u64.into(), 50_u64.into()),
            OrderBookDelta::new(id, BookAction::Update,
                BookOrder::new(OrderSide::Buy, Price::from("0.4000"), Quantity::from("2.000000"), 1),
                RecordFlag::F_LAST as u8, 11, 10_u64.into(), 50_u64.into()),
        ];
        let trades = input.trades.iter().copied().map(Data::Trade).collect::<Vec<_>>();
        let quotes = input.quotes.iter().copied().map(Data::Quote).collect::<Vec<_>>();
        let deltas = input.deltas.iter().copied().map(Data::Delta).collect::<Vec<_>>();
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("arrival-order");
        import(input, &output).unwrap();
        let root = output.join("catalog");
        let mut catalog = ParquetDataCatalog::from_uri(root.to_str().unwrap(), None, None, None, None).unwrap();
        macro_rules! rows {
            ($kind:ty) => {
                catalog.query::<$kind>(Some(vec![id.to_string()]), None, None, None, None, true)
                    .unwrap().collect::<Result<Vec<_>, _>>().unwrap()
            };
        }
        assert_eq!(rows!(TradeTick), trades);
        assert_eq!(rows!(QuoteTick), quotes);
        assert_eq!(rows!(OrderBookDelta), deltas);
    }

    #[test]
    fn contradictory_sibling_payouts_are_rejected_before_catalog_publication() {
        for prices in [["1.0000", "1.0000"], ["0.8000", "0.8000"], ["0.0000", "0.0000"]] {
            let mut input = archive();
            let other = InstrumentId::from_str("test-condition-987654321098765432109876543210.POLYMARKET").unwrap();
            let mut definition = serde_json::to_value(&input.instruments[0]).unwrap();
            definition["BinaryOption"]["id"] = other.to_string().into();
            definition["BinaryOption"]["raw_symbol"] = "987654321098765432109876543210".into();
            input.instruments.push(serde_json::from_value(definition).unwrap());
            input.closes = input.instruments.iter().zip(prices).map(|(instrument, price)| {
                InstrumentClose::new(instrument.id(), Price::from(price), InstrumentCloseType::ContractExpired,
                    1000_u64.into(), 1200_u64.into())
            }).collect();
            let directory = tempfile::tempdir().unwrap();
            let output = directory.path().join("incoherent");
            assert!(import(input, &output).is_err());
            assert!(!output.exists());
        }
    }

'''
p.write_text(s.replace(needle, addition + needle))

# The Runtime source-authority check uses its real SQLite journal. It does not
# claim to execute an OCI engine; native engine replay is tested above and in CI.
replace('apps/runtime/tests/catalog_scope.rs',
    'mod catalog_fixture;',
    'mod catalog_fixture;\n#[path = "../../job/tests/support/polymarket.rs"]\nmod prediction;')
replace('apps/runtime/tests/catalog_scope.rs',
    '''async fn accepts(
    kind: u8,
    metadata: RuntimeCatalogMetadataV1,
    selection: NativeBarSelectionV1,
) -> bool {
    let root = tempfile::tempdir().unwrap();''',
    '''async fn accepts(
    kind: u8,
    metadata: RuntimeCatalogMetadataV1,
    selection: NativeBarSelectionV1,
) -> bool {
    accepts_with_settlements(kind, metadata, selection, Vec::new()).await
}

async fn accepts_with_settlements(
    kind: u8,
    metadata: RuntimeCatalogMetadataV1,
    selection: NativeBarSelectionV1,
    settlements: Vec<contracts::settlement::NativeSettlementGroupV1>,
) -> bool {
    let root = tempfile::tempdir().unwrap();''')
replace('apps/runtime/tests/catalog_scope.rs',
    '    let parameters = if matches!(kind, 6 | 7) {',
    '    let mut parameters = if matches!(kind, 6 | 7) {')
replace('apps/runtime/tests/catalog_scope.rs',
    '''    let parameter = Id::new();
    let encoded = serde_json::to_vec(&parameters).unwrap();''',
    '''    match &mut parameters {
        NativeTaskParametersV1::ValidateData { selections, .. } => {
            for selected in selections { selected.settlements = settlements.clone(); }
        }
        NativeTaskParametersV1::SimulatePortfolio { request, .. }
        | NativeTaskParametersV1::SimulateCandidate { request, .. }
        | NativeTaskParametersV1::SimulatePortfolioSequence { request, .. } => request.settlements = settlements,
        NativeTaskParametersV1::StudyPortfolio { request, .. } => request.settlements = settlements,
        _ => {},
    }
    let parameter = Id::new();
    let encoded = serde_json::to_vec(&parameters).unwrap();''')
p = Path('apps/runtime/tests/catalog_scope.rs')
s = p.read_text()
s += '''
#[tokio::test]
async fn frozen_settlement_vectors_cannot_be_replaced_under_the_same_registered_snapshot() {
    let mut metadata = catalog_fixture::metadata();
    let id = prediction::IDS[0];
    metadata.universe.membership[0].instrument_id = id.into();
    metadata.universe.instrument_definitions = prediction::instruments("0", 200_000_000_000)
        .iter().map(|instrument| serde_json::to_value(instrument).unwrap()).collect();
    let quality = &mut metadata.quality.datasets[0];
    quality.instrument_ids = vec![id.into()];
    quality.selection.bar_types = vec![format!("{id}-1-MINUTE-LAST-EXTERNAL")];
    quality.settlements = prediction::settlement_groups(200_000_000_000, 230_000_000_000, ["1", "0"]);
    let selected = quality.selection.clone();
    let frozen = quality.settlements.clone();
    domain::catalogs::metadata(&metadata, now()).unwrap();
    assert!(accepts_with_settlements(0, metadata.clone(), selected.clone(), frozen.clone()).await);
    assert!(!accepts(0, metadata.clone(), selected.clone()).await);
    for mutation in 0..3 {
        let mut replaced = frozen.clone();
        match mutation {
            0 => { for o in &mut replaced[0].outcomes { o.close_price = "0.5".parse().unwrap(); } }
            1 => { for o in &mut replaced[0].outcomes { o.ts_init = count(229_000_000_000); } }
            _ => { replaced[0].outcomes.pop(); }
        }
        assert!(!accepts_with_settlements(0, metadata.clone(), selected.clone(), replaced).await);
    }
    let mut restarted = metadata.clone();
    for outcome in &mut restarted.quality.datasets[0].settlements[0].outcomes {
        outcome.close_price = "0.5".parse().unwrap();
    }
    domain::catalogs::metadata(&restarted, now()).unwrap();
    assert!(!accepts_with_settlements(0, restarted, selected.clone(), frozen).await);
    // No outcome vector is introduced into the Alpha forecast request.
    assert!(accepts(1, metadata, selected).await);
}
'''
p.write_text(s)

p = Path('crates/domain/tests/prediction_source.rs')
assert not p.exists()
p.write_text('''//! Source and target contracts only. Native simulation owns trading and payout.
use contracts::{settlement::*, DbCounter};
use domain::prediction;
use serde_json::{json, Value};

fn group() -> NativeSettlementGroupV1 {
    NativeSettlementGroupV1 {
        condition_id: "condition".into(), source_reference: "SYNTHETIC_SOURCE_TEST".into(),
        outcomes: [("condition-1.POLYMARKET", "1"), ("condition-2.POLYMARKET", "0")].into_iter()
            .map(|(id, value)| NativeSettlementOutcomeV1 {
                instrument_id: id.into(), close_price: value.parse().unwrap(),
                ts_event: DbCounter::new(100).unwrap(), ts_init: DbCounter::new(120).unwrap(),
            }).collect(),
    }
}

#[test]
fn complete_payout_vectors_preserve_collateral_and_identity() {
    for prices in [["1", "0"], ["0", "1"], ["0.5", "0.5"], ["0.8", "0.2"]] {
        let mut source = group();
        for (outcome, value) in source.outcomes.iter_mut().zip(prices) { outcome.close_price = value.parse().unwrap(); }
        prediction::settlements(&[source]).unwrap();
    }
    for mutation in 0..8 {
        let mut source = group();
        match mutation {
            0 => source.outcomes[1].close_price = "1".parse().unwrap(),
            1 => { for o in &mut source.outcomes { o.close_price = "0.8".parse().unwrap(); } }
            2 => { source.outcomes.pop(); }
            3 => source.outcomes[1].instrument_id = source.outcomes[0].instrument_id.clone(),
            4 => source.outcomes[1].instrument_id = "foreign-2.POLYMARKET".into(),
            5 => source.outcomes[0].ts_init = DbCounter::new(99).unwrap(),
            6 => source.source_reference.clear(),
            _ => source.outcomes[0].close_price = "-0.1".parse().unwrap(),
        }
        assert!(prediction::settlements(&[source]).is_err(), "mutation {mutation}");
    }
}

fn definition(token: &str, expiry: u64) -> Value {
    json!({"BinaryOption": {"id":format!("condition-{token}.POLYMARKET"), "raw_symbol":token,
        "currency":"pUSD", "activation_ns":10, "expiration_ns":expiry,
        "info":{"condition_id":"condition", "token_id":token}}})
}

#[test]
fn earliest_contract_expiry_bounds_all_target_publication_paths() {
    let definitions = vec![definition("1", 100), definition("2", 150)];
    let ids = vec!["condition-1.POLYMARKET".into(), "condition-2.POLYMARKET".into()];
    prediction::target_window(&definitions, &ids, 10, 100).unwrap();
    prediction::target_window(&definitions, &ids, 99, 100).unwrap();
    for (asof, until) in [(9, 100), (90, 101), (100, 110), (101, 120), (90, 90)] {
        assert!(prediction::target_window(&definitions, &ids, asof, until).is_err());
    }
    assert!(prediction::target_window(&definitions, &["foreign.POLYMARKET".into()], 10, 90).is_err());
}

#[test]
fn a_single_traded_token_keeps_its_sibling_and_future_evidence_remains_unavailable() {
    let groups = vec![group()];
    let ids = vec![groups[0].outcomes[0].instrument_id.clone()];
    let selected = prediction::scoped_settlements(&groups, &ids);
    assert_eq!(selected[0].outcomes.len(), 2);
    assert!(prediction::visible_settlements(&groups, &ids, DbCounter::new(119).unwrap()).is_empty());
    assert_eq!(prediction::visible_settlements(&groups, &ids, DbCounter::new(120).unwrap()), groups);
}
''')
print('Authored native, domain and real-journal regressions applied; no execution success is implied.')
