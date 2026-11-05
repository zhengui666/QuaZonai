import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it } from 'vitest';
import type { ReviewReport } from '../../../packages/contracts/src/review.ts';
import { ReviewReportPanel, reviewFilename, reviewMarkdown, reviewReason } from './review-report';
import { metricDefinition } from './metric-glossary';
import { createReviewPolicy, scoreReview } from '../../../packages/contracts/src/review-scoring.ts';

const report: ReviewReport = {
  schemaVersion: 1, id: 'report-1', target: { kind: 'FACTOR', id: 'factor-1', version: 'frozen-version-2', sourceEvidenceRefs: ['validation-evidence'], evaluationStartNs: '9007199254740995' },
  policyId: 'policy-1', policy: { schemaVersion: 1, id: 'policy-1', targetKind: 'FACTOR', frozenAtNs: '9007199254740994', calibration: { rootCount: 30, endNs: '8000000000000000', evidenceRefs: ['reference-cohort'] }, requiredChecks: ['pit'], rejectChecks: ['future_leakage'], metricRules: [], dimensionWeights: { DATA: 20, STATISTICS: 25, ROBUSTNESS: 20, EXECUTION: 15, INCREMENTAL: 15, MECHANISM: 5 }, passScore: 80, observeScore: 65, dimensionFloor: 60 },
  decision: 'REJECT', totalScore: 94.125, qualityCoefficient: 0, reasonCodes: ['future_leakage'], hardFailures: ['标签在决策后才可得'], missingEvidence: [],
  dimensions: [{ id: 'STATISTICS', label: '样本外统计证据', score: 95, weight: 25, status: 'COMPLETE', reasons: [] }],
  metrics: [{ key: 'rank_ic', value: 0.125, status: 'AVAILABLE', observationCount: 256, method: 'signed-spearman-v1', scope: 'POOLED_TIME_SERIES', unit: 'ratio', frequency: 'BAR', evidenceRef: 'factor-analysis' }],
  checks: [{ id: 'future_leakage', status: 'FAIL', reason: '模型读入未来标签', evidenceRefs: ['time-audit'] }],
  analyses: [{ kind: 'factor_diagnostics', status: 'COMPLETE', reason: null, evidenceRef: 'factor-analysis', metrics: ['rank_ic'] }],
  weights: [{ level: 'PORTFOLIO', status: 'PROPOSED', members: [{ id: 'portfolio-1', weight: '0.900719925474099501' }, { id: 'portfolio-2', weight: '0.099280074525900499' }], sourceReportIds: ['source-report-1', 'source-report-2'], candidateId: 'new-candidate', requiresStudy: true, reason: null }],
  narrative: '高分不能抵消时间泄漏。<script>alert(1)</script>', createdAt: '2026-11-02T10:11:12Z',
};

describe('evidence-bound quantitative review report', () => {
  it('keeps a high-scoring rejection, exact weights and candidate provenance visible', () => {
    const markup = renderToStaticMarkup(createElement(ReviewReportPanel, { report }));
    expect(markup).toContain('不予放行');
    expect(markup).not.toContain('通过评审');
    expect(markup).toContain('94.125');
    expect(markup).toContain('标签在决策后才可得');
    expect(markup).toContain('0.900719925474099501');
    expect(markup).toContain('frozen-version-2');
    expect(markup).toContain('9007199254740995');
    expect(markup).toContain('source-report-1');
    expect(markup).toContain('new-candidate');
    expect(markup).toContain('候选建议，须完成新候选共享账户验证');
    expect(markup).toContain('&lt;script&gt;');
    expect(markup).not.toContain('<script>');
    expect(markup).toContain('导出 JSON');
  });
  it('never displays missing scores as zero or makes missing evidence look complete', () => {
    const incomplete = structuredClone(report);
    incomplete.decision = 'INCONCLUSIVE'; incomplete.totalScore = null; incomplete.hardFailures = [];
    incomplete.dimensions[0]!.score = null; incomplete.dimensions[0]!.status = 'INCOMPLETE';
    incomplete.missingEvidence = ['完整试验账本缺失']; incomplete.weights = [];
    const markup = renderToStaticMarkup(createElement(ReviewReportPanel, { report: incomplete }));
    expect(markup).toContain('证据不足，暂不放行');
    expect(markup).toContain('完整试验账本缺失');
    expect(markup).toContain('不完整');
    expect(markup).toContain('不可用');
    expect(markup).not.toContain('多组合资金权重');
    expect(reviewMarkdown(incomplete)).toContain('总分（0–100）：不可用');
    expect(renderToStaticMarkup(createElement(ReviewReportPanel))).toContain('尚无评审报告');
  });
  it('exports the frozen findings and numeric values without executing model prose', () => {
    const exported = reviewMarkdown(report);
    expect(exported).toContain('结论：不予放行');
    expect(exported).toContain('标签在决策后才可得');
    expect(exported).toContain('0\\.900719925474099501');
    expect(exported).toContain('frozen-version-2');
    expect(exported).toContain('factor-analysis');
    expect(exported).toContain('&lt;script&gt;');
    expect(exported).not.toContain('<script>');
    expect(reviewFilename({ ...report, id: '../../untrusted/identity\n' }, 'json')).toBe('quazonai-review-.._.._untrusted_identity_.json');
  });
  it('explains missing references and selection diagnostics without claiming future profit probabilities', () => {
    expect(reviewReason('reference_calibration_missing')).toContain('不能用于自动放行');
    expect(reviewReason('factor.rank_ic_lower:insufficient_or_invalid')).toBe('秩信息系数区间下界：样本不足或指标值无效');
    expect(reviewReason('independent_window:missing_check')).toBe('独立评估窗口：缺少检查结果');
    expect(reviewReason('unrecognized:raw-source')).toBe('unrecognized:raw-source');
    expect(metricDefinition('selection.dsr').explanation).toContain('不是未来盈利概率');
    expect(metricDefinition('pbo').explanation).toContain('诊断搜索过程');
    expect(metricDefinition('quantile_spread').explanation).toContain('不是可交易的组合净收益');
  });
  it('explains a real 100-point rejection against an unfavorable reference cohort and exports absolute floors', () => {
    const policy = createReviewPolicy('FACTOR', 'bad-reference-policy', '20');
    policy.calibration = { rootCount: 30, endNs: '10', evidenceRefs: ['frozen-poor-reference-cohort'] };
    for (const rule of policy.metricRules) {
      if (rule.key === 'selection.dsr') { rule.low = 0.1; rule.high = 0.2; }
      else if (rule.minimumTarget) { rule.low = -0.2; rule.high = -0.1; }
    }
    const badReferenceReject = scoreReview({ id: 'bad-reference-reject', target: { kind: 'FACTOR', id: 'factor', version: 'frozen', sourceEvidenceRefs: ['native-evidence', 'verified-source'], evaluationStartNs: '100' }, policy, createdAt: '2026-11-02T10:00:00Z',
      metrics: policy.metricRules.map(rule => ({ key: rule.key, value: rule.high, status: 'AVAILABLE', observationCount: 30, method: rule.methods[0]!, scope: rule.scope, unit: rule.unit, frequency: rule.frequency, evidenceRef: 'native-evidence' })),
      checks: policy.requiredChecks.map(id => ({ id, status: 'PASS', reason: 'checked', evidenceRefs: ['verified-source'] })),
    });
    expect(badReferenceReject.totalScore).toBe(100); expect(badReferenceReject.decision).toBe('REJECT');
    const markup = renderToStaticMarkup(createElement(ReviewReportPanel, { report: badReferenceReject }));
    expect(markup).toContain('不予放行'); expect(markup).toContain('未达到冻结的绝对准入底线');
    expect(markup).toContain('≥ 0.95'); expect(markup).toContain('&gt; 0');
    expect(markup).toContain('高相对得分不能抵消未达底线');
    const markdown = reviewMarkdown(badReferenceReject);
    expect(markdown).toContain('总分（0–100）：100'); expect(markdown).toContain('结论：不予放行');
    expect(markdown).toContain('≥ 0\\.95'); expect(markdown).toContain('&gt; 0');
    expect(markdown).toContain('未达到冻结的绝对准入底线');
  });
});
