import type { ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { App } from 'antd';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api, type Schema } from './api';
import { DataInputs, InputDetails } from './data-inputs';

// Real data observation components render against query-cache fixtures. Browser
// navigation, transport validation and download cancellation have separate tests.
const writes = () => { throw new Error('Data observation must not submit business commands'); };
beforeEach(() => {
  vi.spyOn(api, 'POST').mockImplementation(writes);
  vi.spyOn(api, 'PUT').mockImplementation(writes);
  vi.spyOn(api, 'PATCH').mockImplementation(writes);
  vi.spyOn(api, 'DELETE').mockImplementation(writes);
});
afterEach(() => vi.restoreAllMocks());
type Entry = [readonly unknown[], unknown];
function render(node: ReactNode, entries: Entry[] = []) {
  const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, gcTime: Infinity, retry: false } } });
  for (const [key, value] of entries) client.setQueryData(key, value);
  try {
    const html = renderToStaticMarkup(<App><QueryClientProvider client={client}>{node}</QueryClientProvider></App>);
    expect(html).not.toMatch(/<form\b|type="submit"|新建冻结输入|创建并冻结项目输入|请求数据质量验证|单独请求 DATA_VALIDATE|运行详情与取消/);
    for (const method of ['POST', 'PUT', 'PATCH', 'DELETE'] as const) expect(api[method]).not.toHaveBeenCalled();
    return html;
  } finally { client.clear(); }
}
const time = '2026-01-01T00:00:00.000001Z';
const empty = { schema_version: 1, items: [], next_cursor: null };
const source: Schema['DataSourceView'] = { id: 'original-source', name: 'Original catalog', runtime_id: 'original-runtime',
  revision: '9007199254740993', enabled: true, native_catalog_ref: 'original/catalog', provider_kind: 'NAUTILUS_CATALOG', created_at: time, updated_at: time };
const dataset: Schema['DatasetView'] = { id: 'original-dataset', source_id: source.id, data_use_grant_id: 'original-grant',
  universe_version_id: 'original-universe', native_snapshot_ref: 'original-snapshot', storage_version: '9007199254740993',
  schema_version: '1', data_kind: 'BAR', partition: 'DISCOVERY', origin: 'FIXTURE', pit_status: 'UNVERIFIED', revision_policy: 'UNKNOWN',
  row_count: '9007199254740993', available_through: time, event_start: '2025-01-01T00:00:00Z', event_end: '2025-12-31T00:00:00Z', timezone: 'UTC',
  quality_artifact_id: 'original-quality', native_metadata_artifact_id: 'original-metadata', created_at: time, checked_at: time,
  license_state: 'ACTIVE', source_enabled: true, runtime_enabled: true };
const input: Schema['InputSetView'] = { header: { id: 'original-input', project_id: 'project', purpose: 'DISCOVERY', revision: '1',
  decision_cutoff: time, frozen_at: time, created_at: time }, items: [{ id: 'original-item', ordinal: 0,
    item: { kind: 'DATASET', dataset_revision_id: dataset.id, role: 'DISCOVERY' }, origin: 'FIXTURE', pit_status: 'UNVERIFIED' }] };
const run: Schema['RunSnapshotV1'] = { schema_version: 1, id: 'original-run', project_id: 'project', input_set_id: input.header.id,
  kind: 'DATA_VALIDATE', state: 'RUNNING', revision: '1', current_attempt_no: 1, active_attempt_id: 'original-attempt',
  queued_at: time, started_at: time, deadline_at: '2026-01-01T01:00:00Z', last_event_seq: '2' };
const details: Entry[] = [
  [['data', 'input', 'project', input.header.id], input],
  [['data', 'revision', dataset.id], dataset],
  [['data', 'source', source.id], source],
  [['data', 'validation-runs', 'project', input.header.id, undefined], { ...empty, items: [run] }],
];

describe('read-only frozen inputs and validation evidence', () => {
  it('retains empty histories, refresh and pagination without creation shortcuts', () => {
    const html = render(<DataInputs projectId="project" />, [[['data', 'inputs', 'project', undefined], empty]]);
    for (const value of ['此项目尚无冻结输入', '刷新冻结输入', '上一页', '下一页', 'CLI/Skill']) expect(html).toContain(value);
  });

  it('retains all purposes and original microsecond cutoffs in the history', () => {
    const html = render(<DataInputs projectId="project" />, [[['data', 'inputs', 'project', undefined], {
      ...empty, next_cursor: 'next-page', items: [input.header, { ...input.header, id: 'sealed-input', purpose: 'SEALED' }],
    }]]);
    for (const value of [input.header.id, 'sealed-input', 'SEALED', time]) expect(html).toContain(value);
  });

  it('retains exact source provenance and read-only navigation to the matching run', () => {
    const html = render(<InputDetails id={input.header.id} project="project" />, details);
    for (const value of ['原始冻结输入详情', source.id, source.runtime_id, source.revision, dataset.native_snapshot_ref,
      dataset.data_use_grant_id, dataset.universe_version_id, dataset.quality_artifact_id, dataset.native_metadata_artifact_id!, time,
      'PIT: UNVERIFIED', '该版本未构成合格真实 PIT 证据', run.id, '运行详情', '查看产物']) expect(html).toContain(value);
  });

  it('shows unresolved metadata honestly and does not substitute another source', () => {
    const html = render(<InputDetails id={input.header.id} project="project" />, details.filter(([key]) => key[1] !== 'source'));
    expect(html).toContain(`数据版本：${dataset.id} · 元数据尚未载入`);
    expect(html).not.toContain(dataset.native_snapshot_ref);
  });

  it('keeps Sealed artifact inputs metadata-only and excludes other runs', () => {
    const sealed: Schema['InputSetView'] = { header: { ...input.header, id: 'sealed-input', purpose: 'SEALED' }, items: [{
      id: 'sealed-item', ordinal: 0, item: { kind: 'ARTIFACT', artifact_id: 'sealed-artifact', role: 'REPORT' }, origin: 'FIXTURE',
    }] };
    const html = render(<InputDetails id={sealed.header.id} project="project" />, [
      [['data', 'input', 'project', sealed.header.id], sealed],
      [['data', 'validation-runs', 'project', sealed.header.id, undefined], { ...empty, items: [run, { ...run, id: 'other-kind', input_set_id: sealed.header.id, kind: 'ALPHA_EVALUATE' }] }],
    ]);
    expect(html).toContain('sealed-artifact');
    expect(html).toContain('这里只显示冻结身份，不读取内容');
    expect(html).toContain('这一项目运行页没有匹配的数据验证');
    expect(html).not.toContain(run.id);
    expect(html).not.toContain('other-kind');
  });
});
