from pathlib import Path


def replace(path, old, new):
    p = Path(path)
    s = p.read_text()
    assert s.count(old) == 1, (path, old)
    p.write_text(s.replace(old, new))


replace('DESIGN.md',
    '组合、独立研究和目标证据继续使用现有冻结输入与 Clarabel，不新增回测内核。',
    '''组合、独立研究和目标证据继续使用现有冻结输入与 Clarabel，不新增回测内核。
Polymarket 的正式日收益统计使用 Nautilus 的 365 日年化配置；原有股票／外汇
路径维持 252 日配置。日均收益不年化，指标保留实际 period、原生方法与缺值原因。
Canonical 原始报告保持上游原样；正式 portfolio statistics 不使用其逐持仓兜底收益。''')
replace('DESIGN.md',
    'source_metadata、instruments 及分别存放的 trades/quotes/deltas/bars。',
    '''source_metadata、instruments 及分别存放的 trades/quotes/deltas/bars/closes。
写入按原生 instrument_id 分区，BAR 按完整 BarType 分区，不能混写不同资产的
Parquet 元数据；同时间戳的盘口更新保持原始顺序。''')
replace('crates/domain/src/execution/output/simulation.rs',
    '    if iso_currency::Currency::from_code(currency).is_none() {',
    '    if !contracts::research_currency::supported(currency) {')
replace('crates/domain/src/prediction.rs',
    '''pub fn uses_native_fee(model: &NativeModelRefV1) -> bool {
    matches!(model, NativeModelRefV1::NautilusPolymarket { .. })
}''',
    '''pub fn uses_native_fee(model: &NativeModelRefV1) -> bool {
    matches!(model, NativeModelRefV1::NautilusPolymarket { .. })
}

/// UTC daily portfolio returns: continuous prediction markets use a calendar year.
/// Existing non-prediction studies retain their frozen 252-day convention.
pub fn portfolio_annualization_days(model: &NativeModelRefV1) -> usize {
    if uses_native_fee(model) { 365 } else { 252 }
}''')
replace('apps/job/src/simulation.rs',
    'use nautilus_analysis::analyzer::PortfolioAnalyzer;',
    '''use nautilus_analysis::{
    analyzer::{PortfolioAnalyzer, Statistic},
    statistics::{returns_volatility::ReturnsVolatility, sharpe_ratio::SharpeRatio, sortino_ratio::SortinoRatio},
};''')
replace('apps/job/src/simulation.rs',
    'fn portfolio_return_analysis(engine: &BacktestEngine) -> Result<PortfolioAnalyzer> {',
    '''fn portfolio_return_analysis(
    engine: &BacktestEngine,
    settings: &NativeSimulationSettingsV1,
) -> Result<PortfolioAnalyzer> {''')
replace('apps/job/src/simulation.rs',
    '''    let mut analyzer = PortfolioAnalyzer::default();
    analyzer.set_portfolio_returns_from_snapshots(&account_ids, &snapshots);''',
    '''    let mut analyzer = PortfolioAnalyzer::default();
    let period = domain::prediction::portfolio_annualization_days(&settings.fee_model);
    if domain::prediction::uses_native_fee(&settings.fee_model) {
        use std::sync::Arc;
        let replacements: [(Statistic, Statistic); 3] = [
            (Arc::new(ReturnsVolatility::new(None)), Arc::new(ReturnsVolatility::new(Some(period)))),
            (Arc::new(SharpeRatio::new(None)), Arc::new(SharpeRatio::new(Some(period)))),
            (Arc::new(SortinoRatio::new(None)), Arc::new(SortinoRatio::new(Some(period)))),
        ];
        for (original, replacement) in replacements {
            analyzer.deregister_statistic(&original);
            analyzer.register_statistic(replacement);
        }
    }
    analyzer.set_portfolio_returns_from_snapshots(&account_ids, &snapshots);''')
replace('apps/job/src/simulation.rs',
    'let return_analysis = portfolio_return_analysis(&engine)?;',
    'let return_analysis = portfolio_return_analysis(&engine, &request.settings)?;')
replace('crates/domain/src/execution/output/simulation.rs',
    '    let mut records = Vec::with_capacity(3);',
    '''    let period = crate::prediction::portfolio_annualization_days(&request.settings.fee_model);
    let volatility_key = format!("Returns Volatility ({period} days)");
    let sharpe_key = format!("Sharpe Ratio ({period} days)");
    let mut records = Vec::with_capacity(3);''')
replace('crates/domain/src/execution/output/simulation.rs',
    '            "Returns Volatility (252 days)",',
    '            volatility_key.as_str(),')
replace('crates/domain/src/execution/output/simulation.rs',
    '            "Sharpe Ratio (252 days)",',
    '            sharpe_key.as_str(),')
p = Path('crates/domain/src/execution/output/simulation.rs')
s = p.read_text()
assert s.count('Some(252.0)') == 2
p.write_text(s.replace('Some(252.0)', 'Some(period as f64)'))

replace('apps/job/tests/polymarket.rs',
    '    assert_eq!(result.frames.len(), 3);',
    '''    assert_eq!(result.frames.len(), 3);
    let simulation_request = result.simulation_request.as_ref().unwrap();
    let simulation = result.simulation.as_ref().unwrap();
    let (metrics, _) = domain::execution::portfolio_simulation_metrics(
        contracts::Id::new(), contracts::Id::new(), simulation_request, simulation,
    ).unwrap();
    assert_eq!(metrics[0].annualization_factor, None);
    assert_eq!(metrics[1].annualization_factor, Some(365.0));
    assert_eq!(metrics[2].annualization_factor, Some(365.0));
    let original = simulation.statistics.iter().find(|s|
        s.group == NativeStatisticGroup::Returns && s.native_key == "Returns Volatility (365 days)"
    ).unwrap();
    assert_eq!(metrics[1].value, original.value);
    assert!(!simulation.statistics.iter().any(|s|
        s.group == NativeStatisticGroup::Returns && s.native_key.ends_with("(252 days)")
    ));''')
