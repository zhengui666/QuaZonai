//! Select adopted native evidence without fitting, accounting or performance calculations.
use super::{bad, equity_curve::portfolio_equity_curve, simulation};
use crate::DomainError;
use contracts::{
    equity_curve::EquityCurveQuery,
    evidence::MetricStatus,
    experiment_summary::*,
    science::{
        NativeAccountKind, NativeExperimentEvaluationResultV1, NativeSimulationRequestV1,
        NativeSimulationResultV1, NativeStatisticGroup,
    },
    DbCounter, Id, SchemaV1,
};

fn count(value: usize) -> Result<DbCounter, DomainError> {
    DbCounter::new(value as u64).map_err(|_| bad("experiment_summary.count"))
}

fn statistic(
    result: &NativeSimulationResultV1,
    key: &str,
    currency: Option<&str>,
    method: &str,
    unit: &str,
    annualization_days: Option<u16>,
) -> ExperimentSummaryStatisticV1 {
    let group = if currency.is_some() {
        NativeStatisticGroup::Pnl
    } else {
        NativeStatisticGroup::Returns
    };
    let native = result
        .statistics
        .iter()
        .find(|s| s.group == group && s.native_key == key && s.currency.as_deref() == currency);
    let (value, status, reason_code) =
        if currency.is_none() && result.returns_status != MetricStatus::Ok {
            (None, result.returns_status, result.returns_reason.clone())
        } else if let Some(native) = native {
            (
                native.value,
                if native.value.is_some() {
                    MetricStatus::Ok
                } else {
                    MetricStatus::Failed
                },
                native.reason_code.clone(),
            )
        } else {
            (
                None,
                MetricStatus::InsufficientData,
                Some("NATIVE_STATISTIC_UNAVAILABLE".into()),
            )
        };
    ExperimentSummaryStatisticV1 {
        native_key: key.into(),
        value,
        status,
        reason_code,
        unit: unit.into(),
        scope: if currency.is_some() {
            "NATIVE_ACCOUNT_BALANCE_CHANGE_PLUS_ENGINE_UNREALIZED"
        } else {
            "UTC_DAILY_PORTFOLIO_RETURNS"
        }
        .into(),
        method_id: method.into(),
        annualization_days,
    }
}

pub fn experiment_summary(
    experiment_id: Id,
    run_id: Id,
    report_artifact_id: Id,
    report: &NativeExperimentEvaluationResultV1,
) -> Result<ExperimentSummaryV1, DomainError> {
    super::experiment::shape(report)?;
    let mut folds = Vec::with_capacity(report.folds.len());
    for fold in &report.folds {
        let request = &fold.simulation_request;
        let result = &fold.simulation;
        let summary = native_simulation_summary(request, result)?;
        folds.push(ExperimentFoldSummaryV1 {
            fold_index: fold.fold_index,
            native_version: summary.native_version,
            account_id: summary.account_id,
            account_kind: summary.account_kind,
            base_currency: summary.base_currency,
            starting_capital: summary.starting_capital,
            period_start_ns: summary.period_start_ns,
            period_end_ns: summary.period_end_ns,
            decision_count: count(fold.decisions.len())?,
            orders: summary.orders,
            positions: summary.positions,
            daily_return_count: summary.daily_return_count,
            return_frequency: summary.return_frequency,
            native_account_pnl: summary.native_account_pnl,
            sharpe_ratio: summary.sharpe_ratio,
            max_drawdown: summary.max_drawdown,
            commissions: summary.commissions,
            commissions_reason: summary.commissions_reason,
            equity_preview: summary.equity_preview,
        });
    }
    Ok(ExperimentSummaryV1 {
        schema_version: SchemaV1, experiment_id, run_id, report_artifact_id,
        dataset_revision_id: report.dataset_revision_id,
        model_artifact_id: report.model_artifact_id,
        feature_artifact_ids: report.feature_artifact_ids.clone(),
        native_versions: report.native_versions.clone(),
        instrument_id: report.instrument_id.clone(),
        interpretation: "Independent frozen-policy replay with fresh capital per fold; no fitted-model, external-training PIT or Alpha qualification claim. Folds are separate, with no aggregate/average Sharpe or concatenated equity.".into(),
        folds,
    })
}

/// The same native metrics and bounded equity selection for every replay view.
pub(super) fn native_simulation_summary(
    request: &NativeSimulationRequestV1,
    result: &NativeSimulationResultV1,
) -> Result<NativeSimulationSummaryV1, DomainError> {
    let series = portfolio_equity_curve(request, result, &EquityCurveQuery::default())?;
    let n = series.points.len();
    let selected = n.min(MAX_EXPERIMENT_PREVIEW_POINTS);
    let points = (0..selected)
        .map(|i| {
            let index = if selected <= 1 {
                0
            } else {
                i * (n - 1) / (selected - 1)
            };
            series.points[index].clone()
        })
        .collect();
    let variant = match request.settings.account_kind {
        NativeAccountKind::Cash => "Cash",
        NativeAccountKind::Margin => "Margin",
    };
    let account = &result.canonical_result["accounts"][0][variant]["base"];
    let account_id = account["id"]
        .as_str()
        .ok_or_else(|| bad("experiment_summary.account"))?;
    crate::control::text(account_id, 1, 200, false)?;
    let currency = &request.settings.base_currency;
    let commissions = account["commissions"]
        .get(currency)
        .filter(|v| !v.is_null())
        .map(|v| {
            simulation::money(v, currency)?
                .to_plain_string()
                .parse()
                .map_err(|_| bad("experiment_summary.commissions"))
        })
        .transpose()?;
    let period = crate::prediction::portfolio_annualization_days(&request.settings.fee_model);
    Ok(NativeSimulationSummaryV1 {
        native_version: result.native_version.clone(),
        account_id: account_id.into(),
        account_kind: request.settings.account_kind,
        base_currency: currency.clone(),
        starting_capital: request.settings.starting_capital.clone(),
        period_start_ns: series.period_start_ns,
        period_end_ns: series.period_end_ns,
        decision_count: count(request.target_points.len())?,
        orders: result.orders,
        positions: result.positions,
        daily_return_count: count(result.returns.len())?,
        return_frequency: "UTC_DAY".into(),
        native_account_pnl: statistic(
            result,
            "PnL (total)",
            Some(currency),
            "nautilus-analysis.PortfolioAnalyzer.total_pnl",
            currency,
            None,
        ),
        sharpe_ratio: statistic(
            result,
            &format!("Sharpe Ratio ({period} days)"),
            None,
            "nautilus-analysis.SharpeRatio",
            "RATIO",
            Some(period as u16),
        ),
        max_drawdown: statistic(
            result,
            "Max Drawdown",
            None,
            "nautilus-analysis.MaxDrawdown",
            "NEGATIVE_RETURN_FRACTION",
            None,
        ),
        commissions_reason: commissions
            .is_none()
            .then(|| "NATIVE_BASE_CURRENCY_COMMISSIONS_UNAVAILABLE".into()),
        commissions,
        equity_preview: ExperimentEquityPreviewV1 {
            source_point_count: series.source_point_count,
            distinct_point_count: series.window_point_count,
            source_resolution: series.resolution,
            sampled: series.sampled || selected < n,
            sampling_method: "DISPLAY_ONLY_FIRST_LAST_UNIFORM_PROJECTED_INDEX".into(),
            points,
        },
    })
}
