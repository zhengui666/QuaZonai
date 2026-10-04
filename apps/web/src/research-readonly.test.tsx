import { readFileSync } from 'node:fs';
import type { ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { App } from 'antd';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api, type Schema } from './api';
import { Projects, ProjectDetail } from './projects';
import { AgentEvaluations, ReportDetail } from './agent-evaluations';
import type { AgentReport } from './agent-evaluation-data';

// Actual read views and populated caches. Browser navigation, retries and PWA
// protection are covered by the native and contract-browser tests separately.
const writes = () => { throw new Error('Research observations cannot submit business writes'); };
beforeEach(() => {
  vi.spyOn(api, 'POST').mockImplementation(writes);
  vi.spyOn(api, 'PATCH').mockImplementation(writes);
  vi.spyOn(api, 'PUT').mockImplementation(writes);
  vi.spyOn(api, 'DELETE').mockImplementation(writes);
});
afterEach(() => vi.restoreAllMocks());
type Entry = [readonly unknown[], unknown];
function render(node: ReactNode, entries: Entry[]) {
  const client = new QueryClient({ defaultOptions: { queries: { staleTime: Infinity, gcTime: Infinity, retry: false } } });
  for (const [key, value] of entries) client.setQueryData(key, value);
  try {
    const html = renderToStaticMarkup(<App><QueryClientProvider client={client}>{node}</QueryClientProvider></App>);
    expect(html).not.toMatch(/<form\b|type="submit"|type="file"|新建研究|创建第一个研究|修改项目状态|保存项目|上传报告|选择 JSON 报告|原样重试上传请求/);
    for (const method of ['POST', 'PATCH', 'PUT', 'DELETE'] as const) expect(api[method]).not.toHaveBeenCalled();
    return html;
  } finally { client.clear(); }
}
const now = '2026-09-01T00:00:00Z';
const empty = { schema_version: 1, items: [], next_cursor: null };
const project: Schema['ProjectView'] = { id: 'project', root_lineage_id: 'lineage', name: 'Original research',
  description: 'Original frozen goal', state: 'DRAFT', revision: '7', current_brief_id: null, created_by: 'OPERATOR', created_at: now, updated_at: now };
const artifact: Schema['ArtifactView'] = { id: 'report', project_id: project.id, kind: 'REPORT', media_type: 'application/json',
  schema_name: 'qz.agent_evaluation_report', schema_version: '1', access_class: 'RESEARCH', byte_count: '1234', origin: 'FIXTURE', created_by: 'OPERATOR', created_at: now };
const report: AgentReport = JSON.parse(readFileSync(new URL('../../../tests/fixtures/agent-evaluation/unrun-v1.json', import.meta.url), 'utf8'));

describe('Research and Agent reports are observation-only', () => {
  it('keeps the empty research view useful without a create shortcut', () => {
    const html = render(<Projects />, [[['projects', undefined], empty]]);
    expect(html).toContain('尚无研究项目'); expect(html).toContain('CLI/Skill'); expect(html).toContain('刷新');
  });
  it('keeps original project names, timestamps, search and navigation without edit controls', () => {
    const html = render(<Projects onNavigate={() => {}} />, [[['projects', undefined], { ...empty, items: [project] }]]);
    for (const value of [project.name, project.description!, project.updated_at, '搜索本页研究项目', '进入研究', '组合记录']) expect(html).toContain(value);
    expect(html).not.toContain('>编辑<');
  });
  it('keeps the project revision, observed state and research sections in the original detail', () => {
    const html = render(<ProjectDetail id={project.id} />, [[['project', project.id], project], [['cycles', project.id, undefined], empty]]);
    for (const value of [project.name, project.description!, '项目记录与修订', '草稿', '研究 Brief', '冻结输入', '运行记录', 'Agent 评估']) expect(html).toContain(value);
  });
  it('keeps report navigation and excludes non-report artifacts without offering uploads', () => {
    const html = render(<AgentEvaluations projectId={project.id} />, [[['agent-evaluation-artifacts', project.id, undefined], {
      ...empty, items: [artifact, { ...artifact, id: 'model-artifact', kind: 'MODEL' }], next_cursor: 'next-page',
    }]]);
    expect(html).toContain(artifact.id); expect(html).toContain('1234 bytes'); expect(html).toContain('下一页');
    expect(html).not.toContain('model-artifact');
  });
  it('preserves protocol-only and unobserved-model evidence instead of implying a model run', () => {
    const html = render(<ReportDetail id={artifact.id} />, [[['agent-evaluation', artifact.id], report]]);
    for (const value of ['PROTOCOL_ONLY：协议测试，不是实际模型评估', 'UNRUN: 2', 'gpt-6-luna / max', '未知（未观察到）', '未知用量与费用不会补零']) expect(html).toContain(value);
    expect(html).not.toContain('LIVE：');
  });
});
