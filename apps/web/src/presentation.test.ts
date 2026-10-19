import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import { EvidenceMetrics, Facts, metricLabel, Metrics } from './ui';
import { factMetricKey, metricDefinition } from './metric-glossary';
import { shortTitle, shouldCollapse } from './titles';
import { RecordTitle, TextDisclosure } from './disclosure';

describe('Chinese metric presentation', () => {
  it('translates native names without rewriting their values or annualization period', () => {
    expect(metricLabel({ key: 'HELDOUT.portfolio.Sharpe Ratio (365 days)', label: 'raw native key', value: 1.2345 })).toBe('夏普比率（365 天年化）');
    expect(metricLabel({ key: 'HELDOUT.pearson_ic', label: 'Pearson IC', value: null })).toBe('皮尔逊信息系数（IC）');
    expect(metricDefinition('uniqueBars').label).toBe('独立行情柱数');
    expect(metricDefinition('reusedBarObservations').label).toBe('重复使用行情柱次数');
    const headline = renderToStaticMarkup(createElement(EvidenceMetrics, { metrics: [{ key: 'HELDOUT.pearson_ic', label: 'Pearson IC', value: null, partition: 'HELDOUT' }] }));
    expect(headline).toContain('独立样本信息系数（IC）');
  });
  it('covers every default Nautilus and calendar statistic without guessing an annualization period', () => {
    for (const key of ['PnL (total)', 'PnL% (total)', 'Max Winner', 'Avg Winner', 'Min Winner', 'Max Loser', 'Avg Loser', 'Min Loser', 'Expectancy', 'Win Rate', 'Returns Skewness', 'Returns Kurtosis', 'Average (Return)', 'Average Loss (Return)', 'Average Win (Return)', 'Tail Ratio', 'Profit Factor', 'Risk Return Ratio', 'Long Ratio', 'Max Drawdown', 'Calendar Expected Days', 'Calendar Observed Return Days', 'Calendar Coverage Ratio', 'Calendar Geometric Daily Return', 'Calendar Minimum Daily Return', 'Calendar Maximum Daily Return', 'Calendar Maximum Absolute Deviation', 'Calendar Negative Days', 'Calendar Zero Days', 'counts.observations', 'counts.included', 'counts.excluded', 'counts.indicator_warmup', 'counts.incomplete_labels', 'counts.cross_boundary_labels', 'counts.prediction_outside_partition']) {
      expect(metricDefinition(`HELDOUT.portfolio.${key}`).label, key).not.toBe('未识别指标');
      expect(metricDefinition(key).explanation.length, key).toBeGreaterThan(12);
    }
    for (const period of ['252', '365']) for (const metric of ['Sharpe Ratio', 'Sortino Ratio', 'Returns Volatility']) expect(metricDefinition(`${metric} (${period} days)`).label).toContain(`${period} 天年化`);
    expect(metricDefinition('Sharpe Ratio').label).toBe('未识别指标');
    expect(metricDefinition('a_new_key').label).toBe('未识别指标');
  });
  it('does not explain request identities as money or individual exit amounts as account totals', () => {
    const markup = renderToStaticMarkup(createElement(Facts, { items: [['请求', 'original-request-id'], ['请求金额', '100.000000000000000001']] }));
    expect(markup).not.toContain('aria-label="解释：请求"');
    expect(markup).toContain('aria-label="解释：请求金额"');
    expect(markup).toContain('original-request-id');
    expect(markup).toContain('100.000000000000000001');
    expect(factMetricKey('账户总预留')).toBe('reserved');
    expect(factMetricKey('保留预留')).toBe('exitReserved');
    expect(factMetricKey('剩余可交易资金')).toBe('remainingManagedCapital');
  });
  it('keeps unknown keys in a technical disclosure, not the headline', () => {
    const markup = renderToStaticMarkup(createElement(Metrics, { metrics: [{ key: 'unfamiliar.scientific_field', label: 'unfamiliar.scientific_field', value: '9007199254740995.000000000000000001' }] }));
    expect(markup).toContain('未识别指标');
    expect(markup).toContain('原始字段');
    expect(markup).toContain('9007199254740995.000000000000000001');
    expect(markup).toContain('解释：未识别指标');
    expect(markup).not.toContain(' open=');
  });
});

describe('short titles and original-text disclosures', () => {
  it('prefers explicit names and derives a bounded first meaningful sentence otherwise', () => {
    expect(shortTitle('明确名称', '很长的假设。下一句')).toBe('明确名称');
    expect(shortTitle(null, '\n# \n\n# 成交量可以解释收益吗？继续讨论')).toBe('成交量可以解释收益吗');
    expect(shortTitle(undefined, 'First meaningful sentence. Original assumptions remain.')).toBe('First meaningful sentence');
    expect(shortTitle('  名称\n与空格  ')).toBe('名称 与空格');
    expect(shortTitle(undefined, '', '原子因子')).toBe('原子因子');
    expect(Array.from(shortTitle('像素'.repeat(40))).length).toBe(42);
    expect(shortTitle('长'.repeat(50))).toBe('长'.repeat(41) + '…');
  });
  it('collapses long descriptions and retains full, unmodified text', () => {
    const original = '  完整的原始假设\n' + '不可改写的内容与模型来源。'.repeat(20) + '  ';
    expect(shouldCollapse(original)).toBe(true);
    expect(shouldCollapse('短假设')).toBe(false);
    const description = renderToStaticMarkup(createElement(TextDisclosure, { label: '研究假设', value: original }));
    expect(description).toContain('<details'); expect(description).not.toContain(' open='); expect(description).toContain(original);
    const title = renderToStaticMarkup(createElement(RecordTitle, { name: original }));
    expect(title).toContain('完整名称与描述'); expect(title).toContain(original); expect(title).not.toContain(' open=');
  });
});
