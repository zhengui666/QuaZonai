//! Select recorded native and report-currency evidence without inventing missing marks.
use super::{bad, equity_curve::portfolio_equity_curve, simulation, spot_cash_report};
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
    let report = result
        .spot_cash_report
        .as_ref()
        .filter(|_| currency.is_none());
    let (statistics, returns_status, returns_reason) = if let Some(report) = report {
        let (status, reason) = spot_cash_report::returns_state(report);
        (&report.statistics, status, reason)
    } else {
        (
            &result.statistics,
            result.returns_status,
            result.returns_reason.clone(),
        )
    };
    let native = statistics
        .iter()
        .find(|s| s.group == group && s.native_key == key && s.currency.as_deref() == currency);
    let (value, status, reason_code) = if currency.is_none() && returns_status != MetricStatus::Ok {
        (None, returns_status, returns_reason)
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
        } else if report.is_some() {
            "REPORT_CURRENCY_DAILY_RETURNS"
        } else {
            "UTC_DAILY_PORTFOLIO_RETURNS"
        }
        .into(),
        method_id: if report.is_some() {
            format!("qz.spot_cash_report/{method}")
        } else {
            method.into()
        },
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
            spot_cash: summary.spot_cash,
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

/// The same native metrics and gap-preserving equity selection for every replay view.
pub(super) fn native_simulation_summary(
    request: &NativeSimulationRequestV1,
    result: &NativeSimulationResultV1,
) -> Result<NativeSimulationSummaryV1, DomainError> {
    let series = portfolio_equity_curve(request, result, &EquityCurveQuery::default())?;
    let n = series.points.len();
    let indices = if result.spot_cash_report.is_some() {
        super::equity_curve::report_preview_indices(&series.points, MAX_EXPERIMENT_PREVIEW_POINTS)
    } else {
        let selected = n.min(MAX_EXPERIMENT_PREVIEW_POINTS);
        (0..selected)
            .map(|i| {
                if selected <= 1 {
                    0
                } else {
                    i * (n - 1) / (selected - 1)
                }
            })
            .collect()
    };
    let selected = indices.len();
    let points = indices
        .into_iter()
        .map(|index| series.points[index].clone())
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
        .filter(|v| !v.is_null() && result.spot_cash_report.is_none())
        .map(|v| {
            simulation::money(v, currency)?
                .to_plain_string()
                .parse()
                .map_err(|_| bad("experiment_summary.commissions"))
        })
        .transpose()?;
    let period = result
        .spot_cash_report
        .as_ref()
        .map(|report| u64::from(report.daily_returns.annualization_days))
        .unwrap_or_else(|| {
            crate::prediction::portfolio_annualization_days(&request.settings.fee_model) as u64
        });
    let spot_cash = result
        .spot_cash_report
        .as_ref()
        .map(|report| cash_summary(report, result, account))
        .transpose()?;
    Ok(NativeSimulationSummaryV1 {
        spot_cash,
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
        daily_return_count: count(
            result
                .spot_cash_report
                .as_ref()
                .map_or(result.returns.len(), |r| r.daily_returns.days.len()),
        )?,
        return_frequency: "UTC_DAY".into(),
        native_account_pnl: if result.spot_cash_report.is_some() {
            ExperimentSummaryStatisticV1 {
                native_key: "PnL (total)".into(),
                value: None,
                status: MetricStatus::InsufficientData,
                reason_code: Some(
                    "MULTI_CURRENCY_NATIVE_PNL_IS_NOT_REPORT_VALUATION_CHANGE".into(),
                ),
                unit: currency.clone(),
                scope: "NATIVE_PNL_RETAINED_BY_CURRENCY".into(),
                method_id: "nautilus-analysis.PortfolioAnalyzer.total_pnl".into(),
                annualization_days: None,
            }
        } else {
            statistic(
                result,
                "PnL (total)",
                Some(currency),
                "nautilus-analysis.PortfolioAnalyzer.total_pnl",
                currency,
                None,
            )
        },
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
        commissions_reason: if result.spot_cash_report.is_some() {
            Some("MULTI_CURRENCY_COMMISSIONS_RETAINED_BY_CURRENCY".into())
        } else {
            commissions
                .is_none()
                .then(|| "NATIVE_BASE_CURRENCY_COMMISSIONS_UNAVAILABLE".into())
        },
        commissions,
        equity_preview: ExperimentEquityPreviewV1 {
            source_point_count: series.source_point_count,
            distinct_point_count: series.window_point_count,
            source_resolution: series.resolution,
            sampled: series.sampled || selected < n,
            sampling_method: if result.spot_cash_report.is_some() {
                "DISPLAY_ONLY_LAST_SEQUENCE_PER_CLOCK_PRESERVE_GAPS_THEN_UNIFORM_PROJECTED_INDEX"
            } else {
                "DISPLAY_ONLY_FIRST_LAST_UNIFORM_PROJECTED_INDEX"
            }
            .into(),
            points,
        },
    })
}

fn cash_summary(
    report: &contracts::spot_cash_report::NativeSpotCashReportV1,
    result: &NativeSimulationResultV1,
    account: &serde_json::Value,
) -> Result<contracts::spot_cash_report::NativeSpotCashSummaryV1, DomainError> {
    use contracts::{
        spot_cash::{NativeSpotCashMoneyV1, ReportCurrencyValuationOutcomeV1},
        spot_cash_report::{NativeSpotCashObservationKindV1, NativeSpotCashSummaryV1},
    };
    let mut commissions_by_currency = Vec::new();
    let commissions_reason = match account.get("commissions") {
        None | Some(serde_json::Value::Null) => Some("NATIVE_COMMISSIONS_UNAVAILABLE".into()),
        Some(value) => {
            for (currency, value) in value
                .as_object()
                .ok_or_else(|| bad("experiment_summary.commissions"))?
            {
                crate::control::text(currency, 1, 40, false)?;
                let amount = spot_cash_report::money(value, currency)?;
                commissions_by_currency.push(NativeSpotCashMoneyV1 {
                    currency: currency.clone(),
                    amount,
                });
            }
            None
        }
    };
    let values: Vec<_> = report
        .observations
        .iter()
        .filter_map(|observation| match &observation.record {
            NativeSpotCashObservationKindV1::Snapshot { valuation, .. } => Some(&valuation.outcome),
            _ => None,
        })
        .collect();
    let valuation_change = match (values.first(), values.last()) {
        (
            Some(ReportCurrencyValuationOutcomeV1::Complete { total: first, .. }),
            Some(ReportCurrencyValuationOutcomeV1::Complete { total: last, .. }),
        ) => Some(
            (last.as_decimal() - first.as_decimal())
                .to_plain_string()
                .parse()
                .map_err(|_| bad("experiment_summary.valuation_range"))?,
        ),
        _ => None,
    };
    Ok(NativeSpotCashSummaryV1 {
        report_currency: report.daily_returns.report_currency.clone(),
        valuation_basis: "NATIVE_CASH_BALANCE_TOTALS_AT_OBSERVED_DIRECT_SPOT_CLOSES".into(),
        valuation_change_reason: valuation_change
            .is_none()
            .then(|| "REPORT_CURRENCY_ENDPOINT_VALUATION_UNAVAILABLE".into()),
        valuation_change,
        commissions_by_currency,
        commissions_reason,
        native_pnl_by_currency: result
            .statistics
            .iter()
            .filter(|s| s.group == NativeStatisticGroup::Pnl)
            .cloned()
            .collect(),
        fee_acceptance: report.fee_acceptance,
        source_evidence: report.source_evidence.clone(),
    })
}
