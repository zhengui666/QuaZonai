import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';
import type { ListPage } from '../../../packages/contracts/src/lists.ts';
import type { ParameterScanView } from '../../../packages/contracts/src/parameter-scans.ts';
import ParameterScans from './parameter-scans';
import { listQuery } from './list-values';

const scan: ParameterScanView = {
  id: 'scan-identity', requestId: 'original-request', name: 'EMA 参数敏感性', source: { kind: 'factor', factorId: 'factor-identity' },
  sourceView: { source: { kind: 'factor', factorId: 'factor-identity' }, label: '原单因子', researchId: 3, trialId: 4, memberIndexes: [0], datasetId: 'native-data', datasetName: '原始市场数据', modality: 'BAR', currency: 'USD', interval: { startNs: '9007199254740993', endExclusiveNs: '9007199254741993' }, assumptions: [] },
  mode: 'FACTOR', input: { name: 'EMA 参数敏感性', source: { kind: 'factor', factorId: 'factor-identity' }, mode: 'FACTOR', axes: [{ key: 'fast_period', domain: { kind: 'values', values: ['5', '10'] } }], objective: { metricId: 'factor.pearson_ic', partition: 'VALIDATION', direction: 'maximize' }, execution: { concurrency: 1 } },
  grid: { axes: [{ key: 'fast_period', domain: { kind: 'values', values: ['5', '10'] }, count: '2', duplicates: '0' }], fixed: { slow_period: '20' }, total: '9007199254740993', eligible: '9007199254740993', skipped: '0' },
  taskId: 5, createdAt: '2026-11-11T00:00:00Z', updatedAt: '2026-11-11T00:00:05Z', status: 'RUNNING', execution: { concurrency: 1 }, objective: { metricId: 'factor.pearson_ic', partition: 'VALIDATION', direction: 'maximize' },
  counts: { total: '9007199254740993', eligible: '9007199254740993', skipped: '0', completed: '1', failed: '0', stopped: '0', running: '1', pending: '9007199254740991', unknown: '0', comparable: '0' },
  best: null, error: null, scientificQualification: 'UNQUALIFIED_EXPLORATORY',
};
function render(value?: ParameterScanView, query = '', list = false) {
  const params = new URLSearchParams(query); const client = new QueryClient({ defaultOptions: { queries: { retry: false } } });
  if (list) client.setQueryData<ListPage<ParameterScanView>>(['/parameter-scans', 'list', listQuery(params, 'createdAt')], { items: value ? [value] : [], total: value ? 1 : 0, page: 1, pageSize: 20, facets: {} });
  else if (value) client.setQueryData([`/parameter-scans/${value.id}`], value);
  try { return renderToStaticMarkup(createElement(QueryClientProvider, { client }, createElement(ParameterScans, { route: { page: 'parameter-scans', creating: false, ...(!list && value ? { id: value.id } : {}), params } }))); }
  finally { client.clear(); }
}
const text = (html: string) => html.replace(/<[^>]*>/g, '');

describe('parameter scan result presentation', () => {
  it('distinguishes an empty collection from a filtered empty list', () => {
    expect(text(render(undefined, '', true))).toContain('尚无参数扫描');
    expect(text(render(undefined, 'q=未找到', true))).toContain('没有匹配的参数扫描');
  });
  it('retains exact large counts and separates running, comparable and scientific qualification', () => {
    const visible = text(render(scan));
    expect(visible).toContain('9007199254740993');
    expect(visible).toContain('尚无目标指标可用');
    expect(visible).toContain('探索性结果，未获科学资格');
    expect(visible).toContain('关闭页面不会取消扫描');
    expect(visible).toContain('计算完成、指标可比较与独立证据通过分别记录');
  });
  it('displays an actual zero as the best validation value without fabricating one when absent', () => {
    const html = text(render({ ...scan, status: 'COMPLETED', best: { ordinal: '9007199254740993', parameters: { fast_period: '5', slow_period: '20' }, metric: { key: 'VALIDATION.pearson_ic', label: 'IC', partition: 'VALIDATION', status: 'AVAILABLE', value: 0, reason: null } } }));
    expect(html).toContain('试验 #9007199254740993');
    expect(html).not.toContain('尚无目标指标可用');
    expect(html).toContain('· 0');
  });
  it('shows cancellation and remote uncertainty without claiming remote termination', () => {
    const stopping = text(render({ ...scan, status: 'STOPPING' }));
    expect(stopping).toContain('停止请求已记录');
    expect(stopping).toContain('本地任务停止不代表远端已停止');
    const reconciling = text(render({ ...scan, status: 'RECONCILING' }));
    expect(reconciling).toContain('原执行结果尚不确定');
    expect(reconciling).toContain('远端回执待核对');
  });
});
