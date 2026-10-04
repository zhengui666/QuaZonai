//! Read accepted native facts without replaying members or deriving new metrics.
use super::{bad, experiment_summary::native_simulation_summary};
use crate::DomainError;
use contracts::{strategy_portfolio::*, SchemaV1};

pub fn strategy_portfolio_summary(
    candidate: &StrategyPortfolioCandidateV1,
    report: &NativeStrategyCompositionResultV1,
) -> Result<StrategyPortfolioSummaryV1, DomainError> {
    super::super::strategy::result(&report.request, report)?;
    let request = &report.request;
    // Match the already published projection. These are source/shape checks,
    // not an independent score, qualification, or permission to deliver targets.
    if serde_json::to_value(&candidate.purpose).ok() != serde_json::to_value(&request.purpose).ok()
        || serde_json::to_value(&candidate.input_provenance).ok()
            != serde_json::to_value(&request.input_provenance).ok()
        || candidate.members.len() != request.members.len()
        || candidate
            .members
            .iter()
            .zip(&request.members)
            .any(|(a, b)| {
                a.alpha_version_id != b.alpha_version_id || a.ensemble_weight != b.ensemble_weight
            })
    {
        return Err(bad("strategy_summary.candidate_binding"));
    }
    let target = match &report.outcome {
        StrategyCompositionOutcomeV1::HistoricalReplay {
            simulation_request, ..
        } => simulation_request
            .target_points
            .last()
            .ok_or_else(|| bad("strategy_summary.target"))?,
        StrategyCompositionOutcomeV1::CurrentDecision { target, .. } => target,
    };
    let asof =
        i64::try_from(target.asof_ns.get()).map_err(|_| bad("strategy_summary.target_clock"))?;
    if candidate.decision_asof != chrono::DateTime::from_timestamp_nanos(asof)
        || candidate.cash_weight != target.cash_weight
        || serde_json::to_value(&candidate.targets).ok()
            != serde_json::to_value(&target.targets).ok()
    {
        return Err(bad("strategy_summary.target_binding"));
    }
    let outcome = match &report.outcome {
        StrategyCompositionOutcomeV1::HistoricalReplay {
            simulation_request,
            simulation,
        } => StrategyPortfolioSummaryOutcomeV1::HistoricalReplay {
            simulation: Box::new(native_simulation_summary(simulation_request, simulation)?),
        },
        StrategyCompositionOutcomeV1::CurrentDecision { .. } => {
            StrategyPortfolioSummaryOutcomeV1::CurrentDecision {
                reason_code: "CURRENT_DECISION_TARGET_ONLY_NO_HISTORICAL_SIMULATION".into(),
            }
        }
    };
    Ok(StrategyPortfolioSummaryV1 {
        schema_version: SchemaV1,
        candidate_id: candidate.id,
        project_id: candidate.project_id,
        run_id: candidate.run_id,
        accepted_attempt_id: candidate.accepted_attempt_id,
        report_artifact_id: candidate.report_artifact_id,
        input_provenance: candidate.input_provenance.clone(),
        native_versions: report.native_versions.clone(),
        interpretation: "Accepted native execution only; no scientific qualification, PIT or delivery approval. Historical replay uses one shared-capital account; current decisions contain targets only. Preview points are display-only and must not be used to calculate metrics.".into(),
        outcome,
    })
}
