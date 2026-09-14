use contracts::{imports::*, Id};

fn manifest() -> HistoricalManifestV1 {
    serde_json::from_value(serde_json::json!({
        "schema_version": 1, "source_installation_id": Id::new(),
        "source_schema_version": "0029_portfolio_candidate_exposure",
        "exported_at": "2026-09-15T00:00:00Z", "exclusions": [],
        "records": [{ "identity": {"kind":"RESEARCH", "source_table":"research_charters", "source_id":"0f626e80-e949-4f2f-9e1e-824e77aa9921"},
            "label":"Original research", "source_state":"ARCHIVED", "created_at":"2026-09-01T00:00:00Z",
            "finished_at":null, "relations":[], "object_ref":null, "object_bytes":null
        }]
    })).unwrap()
}

#[test]
fn historical_uuid_and_relations_are_preserved_without_current_authority() {
    let mut value = manifest();
    assert!(domain::imports::inspect_manifest(&value)
        .unwrap()
        .is_empty());
    let source = value.records[0].identity.clone();
    let mut run = value.records[0].clone();
    run.identity.kind = HistoricalKindV1::Research;
    run.identity.source_table = "research_programs".into();
    // The same old UUID in two tables is two distinct original identities.
    run.relations.push(HistoricalRelationV1 {
        field: "charter_id".into(),
        target: source,
    });
    value.records.push(run);
    assert!(domain::imports::inspect_manifest(&value)
        .unwrap()
        .is_empty());
    value.records.remove(0);
    assert_eq!(
        domain::imports::inspect_manifest(&value).unwrap()[0].code,
        HistoricalIssueCodeV1::MissingRelation
    );
}

#[test]
fn malformed_trace_reports_duplicates_precision_and_secret_table_without_echoing_content() {
    let mut value = manifest();
    value.records.push(value.records[0].clone());
    value.records[0].created_at = "2026-09-01T00:00:00.000000001Z".parse().unwrap();
    let issues = domain::imports::inspect_manifest(&value).unwrap();
    assert!(issues
        .iter()
        .any(|v| v.code == HistoricalIssueCodeV1::DuplicateIdentity));
    assert!(issues
        .iter()
        .any(|v| v.code == HistoricalIssueCodeV1::InvalidTime));
    value.records.truncate(1);
    value.records[0].identity.source_table = "credential_sets".into();
    assert!(domain::imports::inspect_manifest(&value)
        .unwrap()
        .iter()
        .any(|v| v.code == HistoricalIssueCodeV1::UnsupportedTable));
    value.source_schema_version = "unknown".into();
    assert!(domain::imports::inspect_manifest(&value).is_err());
}

#[test]
fn existing_wrong_parent_and_missing_required_parent_are_not_valid_relations() {
    let mut value = manifest();
    let mut program = value.records[0].clone();
    program.identity.source_table = "research_programs".into();
    value.records.push(program);
    assert!(domain::imports::inspect_manifest(&value)
        .unwrap()
        .iter()
        .any(|i| i.code == HistoricalIssueCodeV1::RequiredRelation));
    let wrong = value.records[1].identity.clone();
    value.records[1].relations.push(HistoricalRelationV1 {
        field: "charter_id".into(),
        target: wrong,
    });
    assert!(domain::imports::inspect_manifest(&value)
        .unwrap()
        .iter()
        .any(|i| i.code == HistoricalIssueCodeV1::InvalidRelation));
}
