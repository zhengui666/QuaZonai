import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';
import type { ListPage, ResearchListItem } from '../../../packages/contracts/src/lists.ts';
import Research from './research';
import { listQuery } from './list-values';

const visibleText = (html: string) => html.replace(/<[^>]*>/g, '');
function renderList(items: ResearchListItem[], query = '') {
  const params = new URLSearchParams(query);
  const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  client.setQueryData<ListPage<ResearchListItem>>(['/research', 'list', listQuery(params)], {
    items, total: items.length, page: 1, pageSize: 20, facets: {},
  });
  try {
    return renderToStaticMarkup(createElement(QueryClientProvider, { client }, createElement(Research, {
      route: { page: 'research', creating: false, params },
    })));
  } finally { client.clear(); }
}
const study: ResearchListItem = {
  id: 312987, title: '成交量研究', status: 'COMPUTED', trials: 3, model: 'private-provider/private-model',
  datasetId: 'internal-dataset-identity', datasetName: '冻结市场数据', scope: 'single-factor', modality: 'BAR',
  criterion: { status: 'not_met', metric: 'pearson_ic', value: -0.0123456789, target: 0.05, partition: 'HELDOUT' },
};

describe('research table presentation', () => {
  it('distinguishes an empty collection from an empty search result', () => {
    expect(visibleText(renderList([]))).toContain('尚无研究');
    const filtered = visibleText(renderList([], 'q=不存在的研究'));
    expect(filtered).toContain('没有匹配的研究');
    expect(filtered).not.toContain('尚无研究');
  });
  it('shows execution, criterion and exact independent value without exposing technical identities or long copy', () => {
    const fullName = '研究名称'.repeat(25) + '\n完整问题只在详情显示';
    const html = renderList([{ ...study, title: fullName }]);
    const text = visibleText(html);
    expect(text).toContain('已计算');
    expect(text).toContain('未达到标准');
    expect(text).toContain('-0.0123456789');
    expect(text).toContain('独立评价指标');
    expect(text).toContain('冻结市场数据');
    expect(text).not.toContain(String(study.id));
    expect(html).not.toContain(study.model);
    expect(html).not.toContain(study.datasetId);
    expect(html).not.toContain(fullName);
    expect(html).not.toContain('完整问题只在详情显示');
    expect(html).toContain(`#/research/${study.id}`);
    expect(html).not.toContain('study-document');
  });

  it('keeps an unavailable measurement distinct from an actual zero and retains its annualization', () => {
    const html = renderList([
      { ...study, criterion: { ...study.criterion!, status: 'insufficient', value: null } },
      { ...study, id: 312988, title: '365 天研究', criterion: { ...study.criterion!, status: 'computed', metric: 'Sharpe Ratio (365 days)', value: 0, target: null } },
      { ...study, id: 312989, title: '历史研究', scope: null, datasetName: null, criterion: null },
    ]);
    const row = (id: number) => visibleText(html.match(new RegExp(`<tr[^>]*data-row-key="${id}"[^>]*>([\\s\\S]*?)</tr>`))?.[1] ?? '');
    expect(row(312987)).toContain('证据不足');
    expect(row(312987)).toContain('不可用');
    expect(row(312988)).toContain('365 天年化');
    expect(row(312988)).toContain('已计算（未设阈值）');
    expect(row(312988)).toContain('0');
    expect(row(312988)).not.toContain('不可用');
    expect(row(312989)).toContain('未评价');
    expect(row(312989)).toContain('未记录');
  });
});
