import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import type { PortfolioView } from '../../../packages/contracts/src/index.ts';
import type { ReviewAllocationView } from '../../../packages/contracts/src/review.ts';
import { createReviewPolicy, scoreReview } from '../../../packages/contracts/src/review-scoring.ts';
import { allocationInput, portfolioAllocationEligible, type AllocationFields } from './review-allocation-input';
import { AllocationResult, PortfolioAllocations } from './review-allocations';

vi.mock('./data-hooks', () => ({ useResource: () => ({ data: [], isPending: false, refetch: vi.fn() }) }));
vi.mock('@tanstack/react-query', () => ({ useQueryClient: () => ({ setQueryData: vi.fn(), invalidateQueries: vi.fn() }) }));
vi.mock('./commands', async importOriginal => ({ ...await importOriginal<typeof import('./commands')>(), useCommand: () => ({ busy: false, submit: vi.fn() }) }));

function portfolio(id: string): PortfolioView {
  const review = scoreReview({ id: `report-${id}`, target: { kind: 'PORTFOLIO', id, version: 'frozen-version', sourceEvidenceRefs: ['source'], evaluationStartNs: '100' }, policy: createReviewPolicy('PORTFOLIO', 'policy', '1'), metrics: [], checks: [], createdAt: '2026-11-02T10:00:00Z' });
  // UI fixtures cover admission affordances; scientific approval is revalidated by the service.
  return { id, name: `组合 ${id}`, status: 'COMPUTED', currency: 'USD', datasetId: 'dataset', capital: '1000', researchIds: [1], metrics: [], curve: [], targets: [], error: null, createdAt: '2026-11-02T10:00:00Z', taskId: 1, review: { ...review, decision: 'PASS', totalScore: 90 } };
}
const sources = [portfolio('one'), portfolio('two')];
const fields: AllocationFields = {
  name: '风险分散配置', portfolioIds: ['one', 'two'], memberCaps: { one: '0.700000000000000001', two: '0.6' }, currentWeights: { one: '0', two: '0' },
  clusters: [{ id: '共同风险组', members: ['one', 'two'], maximumWeight: '1' }], maximumTurnover: '1', maximumScientificCalls: '24',
};

describe('multi-portfolio allocation inputs', () => {
  it('submits only selected source identities and explicit constraints, keeping decimal precision', () => {
    const input = allocationInput({ ...fields, metrics: [{ value: 99 }], score: 100 } as AllocationFields, sources);
    expect(Object.keys(input).sort()).toEqual(['name', 'portfolioIds', 'memberCaps', 'clusterCaps', 'currentWeights', 'maximumTurnover', 'maximumScientificCalls'].sort());
    expect(input.memberCaps[0]).toEqual({ id: 'one', maximumWeight: '0.700000000000000001' });
    expect(input.maximumScientificCalls).toBe(24);
    expect(input).not.toHaveProperty('reviewPolicy');
    expect(input).not.toHaveProperty('metrics');
    expect(input).not.toHaveProperty('score');
    expect(allocationInput({ ...fields, independentDatasetId: 'fresh-independent-window' }, sources).independentDatasetId).toBe('fresh-independent-window');
  });
  it('requires passing current sources and a compatible common account basis', () => {
    expect(portfolioAllocationEligible({ ...sources[0]!, review: undefined })).toBe(false);
    expect(portfolioAllocationEligible({ ...sources[0]!, review: { ...sources[0]!.review!, decision: 'INCONCLUSIVE' } })).toBe(false);
    expect(() => allocationInput({ ...fields, portfolioIds: ['one'] }, sources)).toThrow('至少两个');
    expect(() => allocationInput(fields, [sources[0]!, { ...sources[1]!, currency: 'EUR' }])).toThrow('相同币种');
    expect(() => allocationInput(fields, [sources[0]!, { ...sources[1]!, review: undefined }])).toThrow('缺少当前通过');
  });
  it('rejects absent constraints, oversubscribed current weights and fabricated cluster membership', () => {
    expect(() => allocationInput({ ...fields, memberCaps: {} }, sources)).toThrow('精确小数');
    expect(() => allocationInput({ ...fields, currentWeights: {} }, sources)).toThrow('精确小数');
    expect(() => allocationInput({ ...fields, currentWeights: { one: '0.500000000000000001', two: '0.5' } }, sources)).toThrow('不能超过 1');
    expect(() => allocationInput({ ...fields, clusters: [] }, sources)).toThrow('至少一个相关簇');
    expect(() => allocationInput({ ...fields, clusters: [{ id: 'group', members: ['one', 'unknown'], maximumWeight: '1' }] }, sources)).toThrow('所选组合');
    for (const maximumScientificCalls of ['', '1.2', '0', '4097']) expect(() => allocationInput({ ...fields, maximumScientificCalls }, sources)).toThrow('科学调用预算');
    expect(() => allocationInput({ ...fields, maximumTurnover: '' }, sources)).toThrow('精确小数');
  });
});

describe('multi-portfolio allocation presentation', () => {
  it('shows the missing-evidence prerequisite and disables creation without two passing portfolios', () => {
    const markup = renderToStaticMarkup(createElement(PortfolioAllocations, { portfolios: [{ ...sources[0]!, review: undefined }] }));
    expect(markup).toContain('当前有 0 个');
    expect(markup).toContain('请先补齐各组合的评审证据');
    expect(markup).toContain('disabled=""');
    expect(markup).toContain('尚无多组合资金配置记录');
  });
  it('preserves candidate weights and async status without claiming overall approval', () => {
    const value: ReviewAllocationView = { id: 'allocation', requestId: 'original-request', name: '候选配置', status: 'RUNNING', portfolioIds: ['one', 'two'], taskId: 42, consumedScientificCalls: 7, weights: { level: 'PORTFOLIO', status: 'PROPOSED', members: [{ id: 'one', weight: '0.700000000000000001' }], sourceReportIds: ['source-report'], candidateId: 'new-version', requiresStudy: true, reason: null }, review: null, error: null, createdAt: '2026-11-02T10:00:00Z' };
    const markup = renderToStaticMarkup(createElement(AllocationResult, { value }));
    expect(markup).toContain('0.700000000000000001');
    expect(markup).toContain('new-version');
    expect(markup).toContain('source-report');
    expect(markup).toContain('尚无评审报告');
    expect(markup).not.toContain('通过评审');
    expect(markup).toContain('/portfolios/one');
  });
});
