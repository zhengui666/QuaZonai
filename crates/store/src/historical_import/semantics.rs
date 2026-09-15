//! Audited 0029 polymorphic references. These are history checks, not active authority.
use super::*;

pub(super) async fn check(
    tx: &mut Transaction<'_, Postgres>,
    staged: &BTreeMap<String, StagedProjection>,
    unverified: &mut Vec<String>,
) -> Result<u64, StoreError> {
    let exposure = [
        ("PROGRAM", "research_programs"),
        ("BRANCH", "research_branches"),
        ("MISSION", "research_missions"),
        ("ALPHA_MODEL", "alpha_models"),
        ("ALPHA_QUALIFICATION", "alpha_qualifications"),
        ("PORTFOLIO_CANDIDATE", "portfolio_candidates"),
    ];
    let downstream = [
        ("ALPHA", "alpha_qualifications"),
        ("PORTFOLIO", "portfolio_candidates"),
    ];
    let mut checked = 0;
    for (name, targets) in [
        ("evidence_exposures", exposure.as_slice()),
        ("degradation_observations", downstream.as_slice()),
        ("research_wake_events", downstream.as_slice()),
    ] {
        let Some(from) = staged.get(&format!("public.{name}")) else {
            continue;
        };
        if !from.columns.iter().any(|c| c == "subject_type")
            || !from.columns.iter().any(|c| c == "subject_id")
        {
            return Err(StoreError::Integrity);
        }
        let kinds: Vec<String> = sqlx::query_scalar(&format!(
            "SELECT DISTINCT subject_type FROM pg_temp.{}",
            from.table
        ))
        .fetch_all(&mut **tx)
        .await?;
        for kind in kinds {
            let target = targets
                .iter()
                .find(|(k, _)| *k == kind)
                .map(|(_, t)| *t)
                .ok_or(StoreError::Invalid("historical_subject_type"))?;
            let Some(to) = staged.get(&format!("public.{target}")) else {
                unverified.push(format!("POLYMORPHIC:{name}:{kind}->{target}"));
                continue;
            };
            if !to.columns.iter().any(|c| c == "id") {
                return Err(StoreError::Integrity);
            }
            let broken:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM pg_temp.{} s WHERE s.subject_type=$1 AND NOT EXISTS(SELECT 1 FROM pg_temp.{} t WHERE t.id=s.subject_id))",from.table,to.table)).bind(&kind).fetch_one(&mut **tx).await?;
            if broken {
                return Err(StoreError::Invalid("historical_subject_reference"));
            }
            checked += 1;
        }
    }
    // Job/event/preflight type namespaces were open-ended in the old application.
    // Preserve their identities, but never infer a target table from a string.
    for name in ["jobs", "events", "preflight_receipts"] {
        if let Some(table) = staged.get(&format!("public.{name}")) {
            let present: bool = sqlx::query_scalar(&format!(
                "SELECT EXISTS(SELECT 1 FROM pg_temp.{})",
                table.table
            ))
            .fetch_one(&mut **tx)
            .await?;
            if present {
                unverified.push(format!("POLYMORPHIC_UNRESOLVED:{name}"));
            }
        }
    }
    for(name,predicate)in [
        ("evidence_exposures","level NOT BETWEEN 1 AND 3"),
        ("disclosures","NOT ((audience='CODEX' AND level=1) OR (audience='OPERATOR' AND level=2) OR (audience='POSTMORTEM' AND level=3)) OR length(classification_code)=0 OR NOT ((classification_code='QUALIFIED' AND reason_code IS NULL) OR (classification_code<>'QUALIFIED' AND reason_code IS NOT NULL AND length(reason_code)>0))"),
    ] {
        if let Some(table)=staged.get(&format!("public.{name}")) {
            let broken:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM pg_temp.{} WHERE {predicate})",table.table)).fetch_one(&mut **tx).await?;
            if broken{return Err(StoreError::Invalid("historical_disclosure_semantics"));}
        }
    }
    Ok(checked)
}
