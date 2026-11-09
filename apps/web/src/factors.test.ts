import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import type { FactorListItem, ListPage } from '../../../packages/contracts/src/lists.ts';
import Factors from './factors';
import { listQuery } from './list-values';

const state = vi.hoisted(() => ({ datasets: [{ id: 'dataset-internal-identity', name: '历史逐笔数据' }], reads: [] as string[] }));
vi.mock('./data-hooks', () => ({ useResource: (path: string) => {
  state.reads.push(path);
  return { data: path === '/datasets' ? state.datasets : path === '/factors/factor-id' ? factor : undefined, isPending: false, refetch: vi.fn() };
} }));
vi.mock('./capacity', () => ({ FactorCapacity: () => null }));
vi.mock('./delete-record', () => ({ DeleteRecord: () => null }));
const factor = {
  id: 'factor-id', name: '原子因子', hypothesis: '只能在详情阅读的完整研究假设', modelSource: '(module ;; original-factor-source)',
  datasetId: 'dataset-internal-identity', market: ['hidden-market-symbol'],
  researchId: 1, trialId: 2, memberIndex: 0, evidence: 'independent',
  combinable: false, metrics: [], status: 'COMPUTED',
};
const summary: FactorListItem = {
  id: factor.id, name: factor.name, researchId: 1, researchName: '来源研究', trialId: 2, memberIndex: 0,
  datasetId: factor.datasetId, datasetName: '历史逐笔数据', status: 'COMPUTED', evidence: 'independent',
  combinable: false, nativeModality: 'TRADE_TICK', reviewDecision: null, independentIc: null,
};
function render({ id, params = new URLSearchParams(), item = summary }: { id?: string; params?: URLSearchParams; item?: FactorListItem } = {}) {
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  const data: ListPage<FactorListItem> = { items: [item], total: 1, page: 1, pageSize: 20, facets: { research: [{ value: '1', label: '来源研究' }] } };
  client.setQueryData(['/factors', 'list', listQuery(params)], data);
  return renderToStaticMarkup(createElement(QueryClientProvider, { client }, createElement(Factors, { route: { page: 'factors', creating: false, id, params } })));
}
beforeEach(() => { state.datasets = [{ id: 'dataset-internal-identity', name: '历史逐笔数据' }]; state.reads = []; });

for (const id of [undefined, 'factor-id']) describe(id ? 'factor detail data' : 'factor library data', () => {
  it('shows the registered dataset name without internal data identity or markets', () => {
    const markup = render({ id });
    expect(markup).toContain('历史逐笔数据');
    expect(markup).not.toContain('dataset-internal-identity');
    expect(markup).not.toContain('hidden-market-symbol');
    expect(markup).not.toContain('数据 / 市场');
  });
  it('keeps a missing name explicit rather than displaying the dataset id', () => {
    state.datasets = [];
    const markup = render({ id, item: { ...summary, datasetName: null } });
    expect(markup).toContain('数据集名称未记录');
    expect(markup).not.toContain('dataset-internal-identity');
    expect(markup).not.toContain('hidden-market-symbol');
  });
});

describe('factor summary table', () => {
  it('keeps the original identity in navigation and leaves long evidence in detail', () => {
    const markup = render(); const text = markup.replace(/<[^>]+>/g, '');
    expect(markup).toContain('#/factors/factor-id');
    expect(text).not.toContain(factor.id);
    expect(text).not.toContain(factor.hypothesis);
    expect(text).not.toContain(factor.modelSource);
    expect(text).not.toContain('条件容量');
    expect(text).toContain('试验 2 / 分量 1');
    expect(text).toContain('逐笔探索');
    expect(text).toContain('未评审');
    expect(state.reads).toEqual([]);
    const detail = render({ id: factor.id });
    expect(detail).toContain(factor.hypothesis);
    expect(detail).toContain(factor.modelSource);
  });
  it('preserves a measured zero and keeps missing independent IC visibly unavailable', () => {
    expect(render({ item: { ...summary, independentIc: 0 } })).toContain('title="原始值：0">0</span>');
    expect(render()).toContain('title="未提供可用的原单因子独立样本 IC">—</span>');
    expect(render({ item: { ...summary, independentIc: 0.000000001 } })).toContain('>1e-9</span>');
  });
  it('keeps source filtering visible and preserves list parameters on detail navigation', () => {
    const params = new URLSearchParams('research=1&q=原子&page=2&pageSize=50&sort=independentIc&order=desc');
    const markup = render({ params });
    expect(markup).toContain('来源研究：来源研究');
    expect(markup).toContain('aria-label="当前筛选"');
    expect(markup).toContain('href="#/factors/factor-id?research=1');
    const detail = render({ id: factor.id, params });
    expect(detail).toContain('href="#/factors?research=1');
    expect(detail).toContain('page=2&amp;pageSize=50&amp;sort=independentIc&amp;order=desc');
  });
});
