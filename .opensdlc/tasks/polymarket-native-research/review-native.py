from pathlib import Path


def replace(path, old, new):
    p = Path(path)
    s = p.read_text()
    assert s.count(old) == 1, (path, old)
    p.write_text(s.replace(old, new))


p = Path('apps/job/src/prediction.rs')
s = p.read_text()
start = s.index('/// Closing records remain in the original native catalog.')
end = s.index('\npub(crate) fn order_notional(', start)
s = s[:start] + '''/// Verify the frozen source inventory before giving any close to the native engine.
/// The complete condition is checked even when only one sibling is traded. A close
/// added to a BAR directory cannot acquire authority merely by being on disk.
pub(crate) fn catalog_closes(
    root: &Path,
    market: &NativeMarketData,
    selection: &NativeBarSelectionV1,
    groups: &[contracts::settlement::NativeSettlementGroupV1],
) -> Result<Vec<InstrumentClose>> {
    let binaries = market.series.iter().filter_map(|s| match &s.instrument {
        InstrumentAny::BinaryOption(b) => Some((b.id, b)),
        _ => None,
    }).collect::<BTreeMap<_, _>>();
    let selected = binaries.keys().map(ToString::to_string).collect::<Vec<_>>();
    domain::prediction::settlement_scope(groups, &selected, selection)?;
    if binaries.is_empty() { return Ok(Vec::new()); }
    let mut expected = BTreeMap::new();
    for group in groups {
        for outcome in &group.outcomes {
            let id = outcome.instrument_id.parse::<nautilus_model::identifiers::InstrumentId>()?;
            ensure!(expected.insert(id, outcome).is_none(), "DUPLICATE_POLYMARKET_SETTLEMENT");
        }
    }
    let ids = binaries.keys().chain(expected.keys()).copied().collect::<BTreeSet<_>>();
    let mut catalog = ParquetDataCatalog::from_uri(
        root.to_str().ok_or_else(|| anyhow::anyhow!("CATALOG_PATH_ENCODING"))?,
        None, Some(4096), None, None,
    )?;
    // All already-available closes for these identities must be registered. Only
    // their later selection for replay uses the requested half-open holding window.
    let query = catalog.query::<InstrumentClose>(
        Some(ids.iter().map(ToString::to_string).collect()), None,
        Some(UnixNanos::from(selection.decision_cutoff_ns.get())), None, None, true,
    )?;
    let mut seen = BTreeSet::new();
    let mut closes = Vec::new();
    for record in query.take(ids.len() + 1) {
        let Data::InstrumentClose(close) = record? else {
            anyhow::bail!("CATALOG_NATIVE_TYPE_MISMATCH");
        };
        let original = expected.get(&close.instrument_id)
            .ok_or_else(|| anyhow::anyhow!("POLYMARKET_SETTLEMENT_SOURCE_MISMATCH"))?;
        let price: contracts::DecimalValue = close.close_price.to_string().parse()
            .map_err(anyhow::Error::msg)?;
        ensure!(
            close.close_type == InstrumentCloseType::ContractExpired
                && close.ts_init >= close.ts_event
                && close.ts_event.as_u64() == original.ts_event.get()
                && close.ts_init.as_u64() == original.ts_init.get()
                && price == original.close_price,
            "POLYMARKET_SETTLEMENT_SOURCE_MISMATCH"
        );
        ensure!(seen.insert(close.instrument_id), "DUPLICATE_POLYMARKET_SETTLEMENT");
        if let Some(binary) = binaries.get(&close.instrument_id) {
            ensure!(
                close.ts_event >= binary.activation_ns
                    && close.close_price.precision == binary.price_precision,
                "POLYMARKET_CLOSE_CONTRACT_INVALID"
            );
            if close.ts_init.as_u64() >= selection.event_start_ns.get()
                && close.ts_init.as_u64() < selection.event_end_ns.get() {
                closes.push(close);
            }
        }
    }
    ensure!(seen.len() == expected.len(), "POLYMARKET_SETTLEMENT_SOURCE_MISMATCH");
    // Keep original arrival ordering, including ties; no synthetic timestamp is added.
    closes.sort_by_key(|c| c.ts_init);
    Ok(closes)
}

pub(crate) fn close_events(
    root: &Path,
    market: &NativeMarketData,
    selection: &NativeBarSelectionV1,
    groups: &[contracts::settlement::NativeSettlementGroupV1],
) -> Result<Vec<InstrumentClose>> {
    let closes = catalog_closes(root, market, selection, groups)?;
    for series in &market.series {
        if let InstrumentAny::BinaryOption(binary) = &series.instrument {
            if binary.expiration_ns.as_u64() < selection.event_end_ns.get() {
                ensure!(closes.iter().any(|c| c.instrument_id == binary.id),
                    "POLYMARKET_PENDING_RESOLUTION");
            }
        }
    }
    Ok(closes)
}

/// Native instruments remain the authority for the target's usable trading window.
pub(crate) fn target_window(
    instruments: &[InstrumentAny],
    asof_ns: u64,
    until_ns: u64,
) -> Result<()> {
    let definitions = instruments.iter().map(serde_json::to_value).collect::<Result<Vec<_>, _>>()?;
    let ids = instruments.iter().map(|i| i.id().to_string()).collect::<Vec<_>>();
    domain::prediction::target_window(&definitions, &ids, asof_ns, until_ns)?;
    Ok(())
}
''' + s[end:]
p.write_text(s)

replace('apps/job/src/simulation.rs',
    '    let closes = crate::prediction::close_events(root, &market, &request.selection)?;',
    '    let closes = crate::prediction::close_events(root, &market, &request.selection, &request.settlements)?;')
replace('apps/job/src/simulation.rs',
    '''    let mut total = point.cash_weight.as_decimal().clone();''',
    '''    crate::prediction::target_window(instruments, point.asof_ns.get(), point.valid_until_ns.get())?;
    let mut total = point.cash_weight.as_decimal().clone();''')
replace('apps/job/src/portfolio.rs',
    '    crate::simulation::execution_market(&market, settings)?;',
    '''    crate::simulation::execution_market(&market, settings)?;
    let until = selection.decision_cutoff_ns.get()
        .checked_add(u64::from(mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000)
        .ok_or_else(|| anyhow::anyhow!("POLYMARKET_TARGET_TIME_RANGE"))?;
    crate::prediction::target_window(
        &market.series.iter().map(|s| s.instrument.clone()).collect::<Vec<_>>(),
        selection.decision_cutoff_ns.get(), until,
    )?;''')
replace('apps/job/src/study.rs',
    '''    let mut replay = NativeSimulationRequestV1 {
        settlements: Vec::new(),''',
    '''    let mut replay = NativeSimulationRequestV1 {
        settlements: request.settlements.clone(),''')

p = Path('apps/job/src/bin/polymarket-history.rs')
s = p.read_text()
assert s.count('(r.instrument_id, r.ts_init, r.ts_event)') == 4
assert s.count('(r.bar_type, r.ts_init, r.ts_event)') == 1
s = s.replace('(r.instrument_id, r.ts_init, r.ts_event)', '(r.instrument_id, r.ts_init)')
s = s.replace('(r.bar_type, r.ts_init, r.ts_event)', '(r.bar_type, r.ts_init)')
needle = '    for bar in &archive.bars {'
assert s.count(needle) == 1
s = s.replace(needle, '''    // A partial archive stays unqualified, but observed sibling payouts must
    // never contradict each other. This checks source conservation, not cash replay.
    let mut conditions = std::collections::BTreeMap::<String, Vec<contracts::settlement::NativeSettlementOutcomeV1>>::new();
    for close in &archive.closes {
        let identity = close.instrument_id.to_string();
        let (condition, _) = identity.rsplit_once('-').context("POLYMARKET_CLOSE_IDENTITY")?;
        conditions.entry(condition.into()).or_default().push(contracts::settlement::NativeSettlementOutcomeV1 {
            instrument_id: identity,
            close_price: close.close_price.to_string().parse().map_err(anyhow::Error::msg)?,
            ts_event: contracts::DbCounter::new(close.ts_event.as_u64()).map_err(anyhow::Error::msg)?,
            ts_init: contracts::DbCounter::new(close.ts_init.as_u64()).map_err(anyhow::Error::msg)?,
        });
    }
    for (condition_id, outcomes) in conditions {
        if outcomes.len() > 1 {
            domain::prediction::settlements(&[contracts::settlement::NativeSettlementGroupV1 {
                condition_id, source_reference: archive.source_reference.clone(), outcomes,
            }])?;
        }
    }
''' + needle)
p.write_text(s)

print('Authored native inventory and lifetime checks applied; execution remains Nautilus-owned.')
