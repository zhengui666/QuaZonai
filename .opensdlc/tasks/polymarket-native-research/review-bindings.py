from pathlib import Path


def replace(path, old, new):
    p = Path(path)
    s = p.read_text()
    assert s.count(old) == 1, (path, old)
    p.write_text(s.replace(old, new))


# Visible complete conditions are selected from the original immutable registration,
# never reconstructed from a new close-file scan or user-provided outcome label.
replace('crates/domain/src/prediction.rs',
    '/// Bind complete payouts to the original quality window and selected native identities.',
    '''pub fn visible_settlements(
    groups: &[contracts::settlement::NativeSettlementGroupV1],
    instruments: &[String],
    cutoff: contracts::DbCounter,
) -> Vec<contracts::settlement::NativeSettlementGroupV1> {
    scoped_settlements(groups, instruments).into_iter()
        .filter(|g| g.outcomes.iter().all(|o| o.ts_init <= cutoff)).collect()
}

/// Bind complete payouts to the original quality window and selected native identities.''')
replace('crates/store/src/data_validation.rs',
    '''            selection: NativeDatasetSelectionV1 {
                settlements: Vec::new(),''',
    '''            selection: NativeDatasetSelectionV1 {
                settlements: domain::prediction::visible_settlements(
                    &quality.settlements, &quality.instrument_ids, selection.decision_cutoff_ns),''')
replace('crates/store/src/lifecycle/portfolio/simulation.rs',
    '''        let native = NativeSimulationRequestV1 {
            settlements: Vec::new(),''',
    '''        let native = NativeSimulationRequestV1 {
            settlements: dataset.selection.settlements.clone(),''')
replace('crates/store/src/lifecycle/portfolio/study.rs',
    '''        request: Box::new(NativePortfolioStudyRequestV1 {
            settlements: Vec::new(),''',
    '''        request: Box::new(NativePortfolioStudyRequestV1 {
            settlements: dataset.selection.settlements.clone(),''')

# Both preparation quality and publication binding carry the frozen lifecycle
# evidence separately from BAR selections; forecasts remain unchanged.
for file in ('apps/job/src/managed.rs', 'crates/domain/src/execution/output.rs'):
    p = Path(file)
    s = p.read_text()
    old = '''            settlements: Vec::new(),
            dataset_revision_id: *dataset_revision_id,
            selection: request.source_selection.clone(),'''
    assert s.count(old) == 1, file
    s = s.replace(old, '''            settlements: request.settlements.clone(),
            dataset_revision_id: *dataset_revision_id,
            selection: request.source_selection.clone(),''')
    for variant in ('SimulateCandidate', 'SimulatePortfolioSequence'):
        old = f'''        NativeTaskParametersV1::{variant} {{
            dataset_revision_id,
            source_selection,
            ..
        }}''' if variant == 'SimulateCandidate' else f'''        | NativeTaskParametersV1::{variant} {{
            dataset_revision_id,
            source_selection,
            ..
        }}'''
        new = old.replace('            source_selection,\n', '            source_selection,\n            request,\n')
        assert s.count(old) == 1, (file, variant)
        s = s.replace(old, new)
    old = '''            settlements: Vec::new(),
            dataset_revision_id: *dataset_revision_id,
            selection: source_selection.clone(),'''
    assert s.count(old) == 1, file
    s = s.replace(old, '''            settlements: request.settlements.clone(),
            dataset_revision_id: *dataset_revision_id,
            selection: source_selection.clone(),''')
    p.write_text(s)

replace('apps/job/src/managed.rs',
    '''            let mut first = u64::MAX;''',
    '''            crate::prediction::catalog_closes(
                &input.join("catalogs").join(selected.dataset_revision_id.to_string()),
                &data, &selected.selection, &selected.settlements,
            )?;
            let mut first = u64::MAX;''')
replace('apps/job/src/managed.rs',
    '''            datasets.push(NativeDatasetQualityV1 {
                settlements: Vec::new(),''',
    '''            datasets.push(NativeDatasetQualityV1 {
                settlements: selected.settlements,''')
replace('crates/domain/src/execution/output.rs',
    '''                    actual.dataset_revision_id != expected.dataset_revision_id
                        || !same_selection(&actual.selection, &expected.selection)''',
    '''                    actual.dataset_revision_id != expected.dataset_revision_id
                        || actual.settlements != expected.settlements
                        || !same_selection(&actual.selection, &expected.selection)''')
replace('crates/domain/src/execution/output/study.rs',
    '''        if replay.selection != selected''',
    '''        if replay.settlements != request.settlements
            || replay.selection != selected''')

# At Runtime admission, compare the submitted frozen evidence to this registered
# snapshot. Even a coherent replacement vector under the same version is rejected.
replace('apps/runtime/src/materialize.rs',
    '''        selection_scope(catalog, selection)?;''',
    '''        selection_scope(catalog, selection)?;
        let supplied = match &parameters {
            NativeTaskParametersV1::ValidateData { selections, .. } => selections.iter()
                .find(|s| s.dataset_revision_id == revision).map(|s| s.settlements.as_slice()),
            NativeTaskParametersV1::SimulatePortfolio { request, .. }
            | NativeTaskParametersV1::SimulateCandidate { request, .. }
            | NativeTaskParametersV1::SimulatePortfolioSequence { request, .. } => Some(request.settlements.as_slice()),
            NativeTaskParametersV1::StudyPortfolio { request, .. } => Some(request.settlements.as_slice()),
            _ => None,
        };
        if let Some(supplied) = supplied {
            let ids = selection.bar_types.iter().map(|name| {
                name.parse::<nautilus_model::data::BarType>()
                    .map(|kind| kind.instrument_id().to_string())
                    .map_err(|_| Failure::Invalid("catalog_bar_type_scope"))
            }).collect::<Result<Vec<_>>>()?;
            let registered = domain::prediction::visible_settlements(
                &catalog.metadata.quality.datasets[0].settlements, &ids, selection.decision_cutoff_ns);
            if supplied != registered.as_slice() {
                return Err(Failure::Invalid("catalog_settlement_binding"));
            }
            domain::prediction::settlement_scope(supplied, &ids, selection)
                .map_err(|_| Failure::Invalid("catalog_settlement_binding"))?;
        }
        if let NativeTaskParametersV1::BuildPortfolio { request, .. } = &parameters {
            let until = request.selection.decision_cutoff_ns.get()
                .checked_add(u64::from(request.mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000)
                .ok_or(Failure::Invalid("catalog_target_lifetime"))?;
            domain::prediction::target_window(
                &catalog.metadata.universe.instrument_definitions,
                &request.assets.iter().map(|a| a.instrument_id.clone()).collect::<Vec<_>>(),
                request.selection.decision_cutoff_ns.get(), until,
            ).map_err(|_| Failure::Invalid("catalog_target_lifetime"))?;
        }''')

# The same original definition check guards native admission and publication.
# Release source revalidation already calls publication::eligibility.
replace('crates/store/src/lifecycle/portfolio.rs',
    '''    domain::execution::portfolio_build_request(&native)?;''',
    '''    domain::execution::portfolio_build_request(&native)?;
    let until_ns = native.selection.decision_cutoff_ns.get()
        .checked_add(u64::from(native.mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000)
        .ok_or(StoreError::Integrity)?;
    domain::prediction::target_window(
        &dataset.metadata.universe.instrument_definitions,
        &native.assets.iter().map(|a| a.instrument_id.clone()).collect::<Vec<_>>(),
        native.selection.decision_cutoff_ns.get(), until_ns,
    )?;''')
replace('crates/store/src/lifecycle/portfolio/publication.rs',
    '''        if binding.selection.selection != frozen.selection {
            return Err(StoreError::Integrity);
        }''',
    '''        if binding.selection.selection != frozen.selection {
            return Err(StoreError::Integrity);
        }
        let original_until = frozen.selection.decision_cutoff_ns.get()
            .checked_add(u64::from(frozen.mandate.rebalance_schedule.target_ttl_seconds) * 1_000_000_000)
            .ok_or(StoreError::Integrity)?;
        domain::prediction::target_window(
            &binding.metadata.universe.instrument_definitions,
            &frozen.assets.iter().map(|a| a.instrument_id.clone()).collect::<Vec<_>>(),
            frozen.selection.decision_cutoff_ns.get(), original_until,
        )?;''')

print('Frozen registration, Runtime admission, Job quality and publication now share the authored binding.')
