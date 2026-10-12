//! Project original shared-account scientific results, not stored candidate views.
use super::experiment_summary::native_simulation_summary;
use crate::DomainError;
use contracts::strategy_portfolio::*;

pub fn strategy_portfolio_summary(
    report: &NativeStrategyCompositionResultV1,
) -> Result<StrategyPortfolioSummaryOutcomeV1, DomainError> {
    super::super::strategy::result(&report.request, report)?;
    Ok(match &report.outcome {
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
    })
}
