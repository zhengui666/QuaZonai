//! Validate trace manifests without reading files, trusting old PASS or minting identities.
use crate::DomainError;
use contracts::imports::*;
use std::collections::BTreeSet;

fn text(value: &str, limit: usize) -> bool {
    !value.is_empty()
        && value.len() <= limit
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

mod source_schema;

pub fn inspect_manifest(
    value: &HistoricalManifestV1,
) -> Result<Vec<HistoricalIssueV1>, DomainError> {
    if value.source_schema_version != "0029_portfolio_candidate_exposure"
        || value.records.len() > 100_000
        || value.exclusions.len() > 256
    {
        return Err(DomainError::Invalid("historical_manifest"));
    }
    if !value
        .exported_at
        .timestamp_subsec_nanos()
        .is_multiple_of(1000)
    {
        return Err(DomainError::Invalid("historical_export_time"));
    }
    let mut identities = BTreeSet::new();
    let mut issues = Vec::new();
    for record in &value.records {
        let mut issue = |field: &str, code| {
            issues.push(HistoricalIssueV1 {
                identity: record.identity.clone(),
                field: field.into(),
                code,
            })
        };
        if record.identity.source_id.is_nil() || record.identity.source_id.is_max() {
            issue("source_id", HistoricalIssueCodeV1::InvalidIdentity);
        }
        if !identities.insert(record.identity.clone()) {
            issue("source_id", HistoricalIssueCodeV1::DuplicateIdentity);
        }
        if !source_schema::table(&record.identity.source_table)
            .is_some_and(|(kind, _)| kind == record.identity.kind)
        {
            issue("source_table", HistoricalIssueCodeV1::UnsupportedTable);
        }
        if !text(&record.label, 240)
            || !text(&record.source_state, 100)
            || record.relations.len() > 256
        {
            issue("record", HistoricalIssueCodeV1::InvalidField);
        }
        if record.created_at > value.exported_at
            || !record
                .created_at
                .timestamp_subsec_nanos()
                .is_multiple_of(1000)
            || record.finished_at.is_some_and(|end| {
                end < record.created_at
                    || end > value.exported_at
                    || !end.timestamp_subsec_nanos().is_multiple_of(1000)
            })
        {
            issue("time", HistoricalIssueCodeV1::InvalidTime);
        }
        if record.object_ref.is_some() != record.object_bytes.is_some()
            || (record.object_ref.is_some() && record.identity.kind != HistoricalKindV1::Artifact)
        {
            issue("object", HistoricalIssueCodeV1::InvalidArtifact);
        }
    }
    for record in &value.records {
        let expected = source_schema::table(&record.identity.source_table)
            .map(|(_, refs)| refs)
            .unwrap_or(&[]);
        for (field, _, required) in expected {
            if *required && !record.relations.iter().any(|r| r.field == *field) {
                issues.push(HistoricalIssueV1 {
                    identity: record.identity.clone(),
                    field: (*field).into(),
                    code: HistoricalIssueCodeV1::RequiredRelation,
                });
            }
        }
        let mut fields = BTreeSet::new();
        for relation in &record.relations {
            let code = if !text(&relation.field, 120) {
                Some(HistoricalIssueCodeV1::InvalidField)
            } else if !fields.insert(&relation.field) {
                Some(HistoricalIssueCodeV1::DuplicateRelation)
            } else if !expected.iter().any(|(field, table, _)| {
                *field == relation.field && *table == relation.target.source_table
            }) {
                Some(HistoricalIssueCodeV1::InvalidRelation)
            } else if !identities.contains(&relation.target) {
                Some(HistoricalIssueCodeV1::MissingRelation)
            } else {
                None
            };
            if let Some(code) = code {
                issues.push(HistoricalIssueV1 {
                    identity: record.identity.clone(),
                    field: relation.field.clone(),
                    code,
                });
            }
        }
    }
    for exclusion in &value.exclusions {
        if !text(&exclusion.source_table, 120) {
            return Err(DomainError::Invalid("historical_exclusion"));
        }
    }
    Ok(issues)
}
