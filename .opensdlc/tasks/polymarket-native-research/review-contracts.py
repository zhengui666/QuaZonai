from pathlib import Path
import re


def replace(path, old, new):
    p = Path(path)
    s = p.read_text()
    assert s.count(old) == 1, (path, old)
    p.write_text(s.replace(old, new))


path = Path('crates/contracts/src/settlement.rs')
assert not path.exists()
path.write_text('''//! Bounded source observations for native ContractExpired records, not a payout engine.
//! These are frozen dataset evidence, never model features or trading instructions.
use crate::{DbCounter, DecimalValue};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSettlementOutcomeV1 {
    #[schema(min_length = 1, max_length = 200)]
    pub instrument_id: String,
    pub close_price: DecimalValue,
    pub ts_event: DbCounter,
    pub ts_init: DbCounter,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, ToSchema)]
#[serde(deny_unknown_fields)]
pub struct NativeSettlementGroupV1 {
    #[schema(min_length = 1, max_length = 120)]
    pub condition_id: String,
    #[schema(min_length = 1, max_length = 2000)]
    pub source_reference: String,
    /// The complete two-outcome condition, even when only one token is traded.
    #[schema(min_items = 2, max_items = 2)]
    pub outcomes: Vec<NativeSettlementOutcomeV1>,
}
''')
replace('crates/contracts/src/lib.rs', 'pub mod science;', 'pub mod science;\npub mod settlement;')

# Add an optional wire field. An absent/empty array grants no settlement evidence;
# existing non-prediction documents retain their exact serialized representation.
for file, kind in [
    ('crates/contracts/src/execution.rs', 'NativeDatasetSelectionV1'),
    ('crates/contracts/src/execution.rs', 'NativeDatasetQualityV1'),
    ('crates/contracts/src/science.rs', 'NativeSimulationRequestV1'),
    ('crates/contracts/src/science/portfolio.rs', 'NativePortfolioStudyRequestV1'),
]:
    replace(file, f'pub struct {kind} {{', f'''pub struct {kind} {{
    /// Complete original condition payouts; not inferred from a last bar or expiry.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    #[schema(max_items = 256)]
    pub settlements: Vec<crate::settlement::NativeSettlementGroupV1>,''')

# Mechanical initialization of existing Rust constructors, not replacement data.
# Production origins are wired explicitly in review-bindings.py; historical fixtures
# with no settlement observations preserve the empty/unknown state.
types = ('NativeDatasetSelectionV1', 'NativeDatasetQualityV1', 'NativeSimulationRequestV1', 'NativePortfolioStudyRequestV1')
for root in ('apps', 'crates', 'tests'):
    for p in Path(root).rglob('*.rs'):
        s = p.read_text()
        original = s
        for kind in types:
            pattern = rf'(?<!struct )\b{kind} \{{\n(?P<indent>[ \t]+)(?=[A-Za-z_][A-Za-z_0-9]*\s*:)'
            s = re.sub(pattern, lambda m: m.group(0) + 'settlements: Vec::new(),\n' + m.group('indent'), s)
        if s != original:
            print('Initialize additive settlement binding:', p)
            p.write_text(s)

p = Path('crates/domain/src/prediction.rs')
s = p.read_text()
needle = '\n#[cfg(test)]\nmod tests {'
assert s.count(needle) == 1
addition = '''
/// Validate source coherence only. Nautilus still performs every cash movement.
pub fn settlements(
    groups: &[contracts::settlement::NativeSettlementGroupV1],
) -> Result<(), DomainError> {
    use std::collections::BTreeSet;
    let bad = || DomainError::Invalid("polymarket_settlement_source");
    if groups.len() > 256 { return Err(bad()); }
    let mut conditions = BTreeSet::new();
    let mut instruments = BTreeSet::new();
    for group in groups {
        crate::control::text(&group.condition_id, 1, 120, false)?;
        crate::control::text(&group.source_reference, 1, 2000, false)?;
        if !conditions.insert(&group.condition_id) || group.outcomes.len() != 2 {
            return Err(bad());
        }
        let prefix = format!("{}-", group.condition_id);
        let mut total = BigDecimal::from(0);
        for outcome in &group.outcomes {
            crate::control::text(&outcome.instrument_id, 1, 200, false)?;
            let token = outcome.instrument_id.strip_prefix(&prefix)
                .and_then(|s| s.strip_suffix(".POLYMARKET")).ok_or_else(bad)?;
            if token.is_empty() || !token.bytes().all(|b| b.is_ascii_digit())
                || !instruments.insert(&outcome.instrument_id)
                || !outcome.close_price.is_fraction()
                || outcome.ts_event > outcome.ts_init
            {
                return Err(bad());
            }
            total += outcome.close_price.as_decimal();
        }
        if total != BigDecimal::from(1) {
            return Err(DomainError::Invalid("polymarket_incoherent_payout_vector"));
        }
    }
    Ok(())
}

/// Select whole conditions, never remove a sibling merely because it is not traded.
pub fn scoped_settlements(
    groups: &[contracts::settlement::NativeSettlementGroupV1],
    instruments: &[String],
) -> Vec<contracts::settlement::NativeSettlementGroupV1> {
    groups.iter().filter(|g| g.outcomes.iter().any(|o| instruments.contains(&o.instrument_id)))
        .cloned().collect()
}

/// Bind complete payouts to the original quality window and selected native identities.
pub fn settlement_scope(
    groups: &[contracts::settlement::NativeSettlementGroupV1],
    instruments: &[String],
    selection: &contracts::science::NativeBarSelectionV1,
) -> Result<(), DomainError> {
    settlements(groups)?;
    if groups != scoped_settlements(groups, instruments)
        || groups.iter().flat_map(|g| &g.outcomes).any(|o|
            o.ts_init > selection.decision_cutoff_ns)
    {
        return Err(DomainError::Invalid("polymarket_settlement_scope"));
    }
    Ok(())
}

/// Fixed TTLs are rejected, never silently clipped to conceal an expired target.
pub fn target_window(
    definitions: &[Value],
    ids: &[String],
    asof_ns: u64,
    until_ns: u64,
) -> Result<(), DomainError> {
    if asof_ns >= until_ns { return Err(DomainError::Invalid("polymarket_target_lifetime")); }
    for id in ids {
        let mut found = false;
        for definition in definitions {
            let (class, payload) = crate::catalogs::instrument_definition(definition)?;
            if payload.get("id").and_then(Value::as_str) != Some(id.as_str()) { continue; }
            if found { return Err(DomainError::Invalid("polymarket_target_identity")); }
            found = true;
            if class == "BinaryOption" {
                let (activation, expiration) = instrument(payload)?;
                if asof_ns < activation || asof_ns >= expiration || until_ns > expiration {
                    return Err(DomainError::Invalid("polymarket_target_lifetime"));
                }
            }
        }
        if !found { return Err(DomainError::Invalid("polymarket_target_identity")); }
    }
    Ok(())
}
'''
p.write_text(s.replace(needle, addition + needle))

# Quality metadata is the immutable registration authority, not a second catalog.
replace('crates/domain/src/catalogs.rs',
    '    bar_notionals(quality)?;',
    '''    bar_notionals(quality)?;
    crate::prediction::settlement_scope(&quality.settlements, &quality.instrument_ids, &quality.selection)?;
    if value.partition == contracts::research::DataPartition::Sealed && !quality.settlements.is_empty() {
        return Err(bad("quality.sealed_settlement_values"));
    }
    for group in &quality.settlements {
        for outcome in &group.outcomes {
            if chrono::DateTime::from_timestamp_nanos(outcome.ts_init.get() as i64) > value.available_through {
                return Err(bad("quality.settlement_availability"));
            }
            // The untraded sibling may have no BAR series. Its native definition is
            // still required so one source cannot splice unrelated condition IDs.
            let mut found = false;
            for definition in &value.universe.instrument_definitions {
                let (class, payload) = instrument_definition(definition)?;
                if payload["id"].as_str() != Some(&outcome.instrument_id) { continue; }
                if found || class != "BinaryOption" { return Err(bad("quality.settlement_instrument")); }
                let (activation, _) = crate::prediction::instrument(payload)?;
                if payload["info"]["condition_id"].as_str() != Some(&group.condition_id)
                    || outcome.ts_event.get() < activation {
                    return Err(bad("quality.settlement_instrument"));
                }
                found = true;
            }
            if !found { return Err(bad("quality.settlement_instrument")); }
        }
    }''')

replace('crates/domain/src/execution/output.rs',
    '        crate::catalogs::bar_notionals(item)?;',
    '''        crate::catalogs::bar_notionals(item)?;
        crate::prediction::settlement_scope(&item.settlements, &item.instrument_ids, &item.selection)?;
        if item.settlements.iter().flat_map(|g| &g.outcomes).any(|o| o.ts_init.get() > checked) {
            return Err(bad("native_output.settlement_time"));
        }''')
print('Authored source contracts and pure rules applied; native compilation still required.')
