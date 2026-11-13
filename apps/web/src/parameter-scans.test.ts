import { createElement } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { describe, expect, it } from 'vitest';
import type { ListPage } from '../../../packages/contracts/src/lists.ts';
import type { ParameterScanView } from '../../../packages/contracts/src/parameter-scans.ts';
import ParameterScans from './parameter-scans';
import { listQuery } from './list-values';

const scan: ParameterScanView = {
  id: 'scan-identity', requestId: 'original-request', name: 'EMA 参数敏感性', source: { kind: 'research', researchId: 3 },
  sourceView: { source: { kind: 'research', researchId: 3 }, label: '原生请求研究', researchId: 3, datasetId: 'native-data', datasetName: '原始市场数据', modality: 'NATIVE', currency: 'USD', interval: { startNs: '9007199254740993', endExclusiveNs: '9007199254741993' }, assumptions: [] },
  mode: 'STRATEGY', input: { name: 'EMA 参数敏感性', source: { kind: 'research', researchId: 3 }, mode: 'STRATEGY', axes: [{ key: 'strategy.fast_period', domain: { kind: 'values', values: ['5', '10'] } }], objective: { metricId: 'Custom Native Statistic', partition: 'FULL_SELECTION', direction: 'maximize' }, execution: { concurrency: 1 } },
  grid: { axes: [{ key: 'strategy.fast_period', domain: { kind: 'values', values: ['5', '10'] }, count: '2', duplicates: '0' }], fixed: { 'strategy.slow_period': '20' }, total: '9007199254740993', eligible: '9007199254740993', skipped: '0' },
  taskId: 5, createdAt: '2026-11-11T00:00:00Z', updatedAt: '2026-11-11T00:00:05Z', status: 'RUNNING', execution: { concurrency: 1 }, objective: { metricId: 'Custom Native Statistic', partition: 'FULL_SELECTION', direction: 'maximize' },
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
    expect(visible).toContain('不重新计算排名');
    expect(visible).toContain('探索性结果，未获科学资格');
    expect(visible).toContain('关闭页面不会取消扫描');
    expect(visible).toContain('不代表独立证据通过');
  });
  it('does not advertise creation, rankings or retired analysis tabs', () => {
    const visible = text(render(scan));
    for (const label of ['最佳可比', '参数分析', '曲线对比', '新建扫描']) expect(visible).not.toContain(label);
    expect(text(render(scan, '', true))).not.toContain('新建扫描');
    expect(text(render(scan, 'tab=surface'))).toContain('不重新计算排名');
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

it('renders the chosen minimizing direction and preserves the native key', () => {
  const value = { ...scan, objective: { ...scan.objective, metricId: 'Custom Native Statistic', direction: 'minimize' as const } };
  const visible = text(render(value));
  expect(visible).toContain('历史探索记录');
  expect(visible).not.toContain('最小化原生统计值');
  expect(visible).not.toContain('VALIDATION');
});

it('shows independent official strategy task concurrency without retired batching controls', () => {
  const value = { ...scan, execution: { concurrency: 2 } };
  const visible = text(render(value, 'tab=inputs'));
  expect(visible).toContain('并发任务');
  expect(visible).not.toContain('每批最多参数点');
  expect(visible).toContain('strategy.fast_period');
  expect(visible).not.toContain('batchSize');
  expect(visible).not.toContain('signal.fast_period');
});

it('contains no removed API calls or deleted grid imports', async () => {
  const { readFile } = await import('node:fs/promises');
  const source = await readFile(new URL('./parameter-scans.tsx', import.meta.url), 'utf8');
  for (const retired of ['parameterScanPlan', 'parameterScanAxisValue', '/preview', '/surface', '/options', 'ScanBuilder', 'trialOrder', 'ParameterHeatmap']) expect(source).not.toContain(retired);
  const research = await readFile(new URL('./research.tsx', import.meta.url), 'utf8');
  expect(research).not.toContain('/parameter-scans/new');
});
