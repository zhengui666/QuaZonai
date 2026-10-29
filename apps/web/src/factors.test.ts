import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { describe, expect, it, vi } from 'vitest';
import Factors from './factors';

const state = vi.hoisted(() => ({ datasets: [{ id: 'dataset-internal-identity', name: '历史逐笔数据' }] }));
vi.mock('./data-hooks', () => ({ useResource: (path: string) => ({
  data: path === '/datasets' ? state.datasets : path === '/factors' ? [factor] : path === '/factors/factor-id' ? factor : undefined,
  isPending: false, refetch: vi.fn(),
}) }));
vi.mock('./capacity', () => ({ FactorCapacity: () => null }));
vi.mock('./delete-record', () => ({ DeleteRecord: () => null }));
const factor = {
  id: 'factor-id', name: '原子因子', hypothesis: '研究假设', modelSource: '',
  datasetId: 'dataset-internal-identity', market: ['hidden-market-symbol'],
  researchId: 1, trialId: 2, memberIndex: 0, evidence: 'independent',
  combinable: false, metrics: [], status: 'COMPUTED',
};

for (const id of [undefined, 'factor-id']) describe(id ? 'factor detail data' : 'factor library data', () => {
  const render = () => renderToStaticMarkup(createElement(Factors, { route: { page: 'factors', creating: false, id, params: new URLSearchParams() } }));
  it('shows the registered dataset name without internal data identity or markets', () => {
    state.datasets = [{ id: 'dataset-internal-identity', name: '历史逐笔数据' }];
    const markup = render();
    expect(markup).toContain('历史逐笔数据');
    expect(markup).not.toContain('dataset-internal-identity');
    expect(markup).not.toContain('hidden-market-symbol');
    expect(markup).not.toContain('数据 / 市场');
  });
  it('keeps a missing name explicit rather than displaying the dataset id', () => {
    state.datasets = [];
    const markup = render();
    expect(markup).toContain('数据集名称未记录');
    expect(markup).not.toContain('dataset-internal-identity');
    expect(markup).not.toContain('hidden-market-symbol');
  });
});
