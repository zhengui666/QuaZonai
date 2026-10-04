import { readFileSync } from 'node:fs';
import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { App, type DrawerProps } from 'antd';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api, displayTime, type Schema } from './api';
import { Briefs, BriefDetail } from './briefs';
import { Cycles, CycleSelection } from './cycles';
import { Runs, RunDetail } from './runs';
import { ResearchOverview } from './research-overview';

// These render the real query-backed views and controls. Drawer portals alone
// are inlined for SSR; browser navigation, focus and SSE delivery are separate checks.
vi.mock('antd', async () => {
  const actual = await vi.importActual<typeof import('antd')>('antd');
  return { ...actual, Drawer: ({ open, title, children }: DrawerProps) => open
    ? createElement('section', { 'aria-label': typeof title === 'string' ? title : undefined }, children) : null };
});
const writes = () => { throw new Error('Research observation must not submit business commands'); };
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
    expect(html).not.toMatch(/<form\b|type="submit"|新建 Brief 草稿|编写第一个 Brief|保存 Brief 草稿|以此创建新版本|查看 \/ 编辑|确认冻结 Brief|启动新 Cycle|确认启动 Cycle|请求取消运行|确认请求取消/);
    for (const method of ['POST', 'PUT', 'PATCH', 'DELETE'] as const) expect(api[method]).not.toHaveBeenCalled();
    return html;
  } finally { client.clear(); }
}

const close = () => {};
const time = '2026-09-01T00:00:00Z';
const empty = { schema_version: 1, items: [], next_cursor: null };
const original: Schema['BriefCreate'] = JSON.parse(readFileSync(new URL('../../../tests/contracts/research-brief.json', import.meta.url), 'utf8'));
const brief: Schema['BriefView'] = { id: 'original-brief', project_id: 'project', content: original.content, bindings: original.bindings,
  supersedes_id: 'previous-brief', version: 2, revision: '3', state: 'FROZEN', created_at: time, updated_at: time, frozen_at: time };
const context: Schema['FrozenBriefV1'] = { schema_version: 1, brief, execution_context: { schema_version: 1,
  runtime_id: 'original-runtime', runtime_revision: '17', discovery_input_set_id: 'original-discovery',
  validation_input_set_id: 'original-validation', sealed_input_set_id: 'original-sealed' } };
const cycle: Schema['CycleViewV1'] = { schema_version: 1, id: 'original-cycle', project_id: 'project', brief_id: brief.id, ordinal: 1,
  revision: '2', state: 'CANCELLED', trigger: 'OPERATOR', created_at: time, started_at: time, ended_at: time,
  initial_run_id: 'original-run', budget: brief.content.budget, used_experiments: 2, reserved_experiments: 0, reserved_cpu_seconds: '0',
  outcome: 'INCONCLUSIVE', next_action: 'Review original cancellation evidence', available_actions: ['VIEW_RUNS', 'VIEW_SELECTION'] };
const run: Schema['RunSnapshotV1'] = { schema_version: 1, id: 'original-run', project_id: 'project', cycle_id: cycle.id,
  revision: '4', kind: 'AGENT_RESEARCH', state: 'RUNNING', current_attempt_no: 1, active_attempt_id: 'original-attempt',
  queued_at: time, started_at: time, deadline_at: '2026-09-02T00:00:00Z', input_set_id: 'original-input', last_event_seq: '7' };
const selection: Schema['CycleSelectionV1'] = { schema_version: 1, cycle_id: cycle.id, project_id: 'project', created_at: time,
  research_run_id: run.id, policy_id: 'original-policy', status: 'INCONCLUSIVE', trial_count: '1', eligible_count: '0', selected_count: '0', unfinished_count: '0',
  rule: { schema_version: 1, candidate_count: 1, comparable_scope: 'FAMILY_LINEAGE', comparison_input_set_id: 'original-comparison',
    direction: 'MAXIMIZE', evaluation_kind: 'WALK_FORWARD', execution_assumptions_id: 'original-assumption', family_id: 'original-family', frequency: 'UTC_DAY',
    method_id: 'original-method', method_version: '0.63.0', metric_code: 'ORIGINAL_RETURN', metric_scope: 'portfolio',
    missing_required_metric: 'INCONCLUSIVE', root_lineage_id: 'original-lineage', tie_break: 'EXPERIMENT_ID_ASC', unit: 'RETURN' } };
const trial: Schema['CycleSelectionTrialV1'] = { schema_version: 1, cycle_id: cycle.id, source_cycle_id: cycle.id,
  experiment_id: 'original-experiment', execution_run_id: run.id, execution_state: 'CANCELLED', selected: false,
  unfinished: false, rank: null, reason: 'EXECUTION_CANCELLED', selection_metric: null };
const project: Schema['ProjectView'] = { id: 'project', root_lineage_id: 'lineage', name: 'Original research', description: '',
  state: 'ACTIVE', revision: '1', created_by: 'OPERATOR', created_at: time, updated_at: time, current_brief_id: brief.id };

describe('read-only research process', () => {
  it('preserves empty record and next-step guidance without an authoring shortcut', () => {
    const html = render(<Briefs projectId="project" projectState="ACTIVE" />, [[['briefs', 'project', undefined], empty]]);
    expect(html).toContain('尚无 Brief');
    expect(html).toContain('外部 Agent');
    expect(html).toContain('刷新 Brief');
    expect(render(<Cycles projectId="project" />, [[['cycles', 'project', undefined], empty]])).toContain('暂无研究周期');
  });

  it.each(['ACTIVE', 'ARCHIVED'] as const)('keeps draft and frozen Briefs readable in %s research', state => {
    const draft = { ...brief, id: 'draft', state: 'DRAFT' as const, version: 3, frozen_at: null };
    const list = render(<Briefs projectId="project" projectState={state} currentBriefId={brief.id} />,
      [[['briefs', 'project', undefined], { ...empty, items: [brief, draft], next_cursor: 'next' }]]);
    for (const value of ['当前版本', '查看草稿', '查看冻结版本', '下一页', brief.content.hypothesis]) expect(list).toContain(value);
    const detail = render(<BriefDetail brief={draft} close={close} />);
    expect(detail).toContain('原完整 Brief');
    expect(detail).toContain('previous-brief');
    expect(detail).not.toContain('原冻结执行上下文');
  });

  it('preserves frozen content, budgets, binding access and original execution inputs', () => {
    const detail = render(<BriefDetail brief={brief} close={close} />, [[['frozen-brief', brief.id], context]]);
    for (const value of [brief.content.hypothesis, brief.content.economic_rationale, 'max_experiments', 'EVALUATOR_ONLY',
      'original-runtime / 17', 'original-discovery', 'original-validation', 'original-sealed']) expect(detail).toContain(value);
  });

  it('retains cycle outcome, run navigation and cancelled trial evidence without inventing a metric', () => {
    const list = render(<Cycles projectId="project" />, [[['cycles', 'project', undefined], { ...empty, items: [cycle] }]]);
    for (const value of ['已取消', 'INCONCLUSIVE', cycle.next_action!, '查看准备运行', '查看试验选择']) expect(list).toContain(value);
    const detail = render(<CycleSelection id={cycle.id} close={close} />, [[['cycle-selection', cycle.id], selection],
      [['cycle-selection-trials', cycle.id, undefined], { ...empty, items: [trial] }]]);
    for (const value of ['original-policy', 'original-assumption', trial.experiment_id, 'CANCELLED', '缺值：EXECUTION_CANCELLED', '不参与排名']) expect(detail).toContain(value);
  });

  it.each(['RUNNING', 'CANCEL_REQUESTED', 'CANCELLED'] as const)('retains %s run state and event observation without cancellation controls', state => {
    const snapshot = { ...run, state, cancellation_requested_at: state === 'RUNNING' ? null : time,
      finished_at: state === 'CANCELLED' ? time : null, terminal_reason_code: state === 'CANCELLED' ? 'ORIGINAL_CANCEL_CONFIRMED' : null };
    const list = render(<Runs projectId="project" />, [[['runs', 'project', undefined, undefined], { ...empty, items: [snapshot] }]]);
    expect(list).toContain('按运行状态筛选');
    expect(list).toContain('刷新运行');
    const detail = render(<RunDetail id={run.id} close={close} />, [[['run', run.id], snapshot]]);
    for (const value of [run.id, cycle.id, 'original-attempt', '实时事件', '重载快照并连接', '取消请求时间']) expect(detail).toContain(value);
    if (state !== 'RUNNING') expect(detail).toContain(displayTime(time));
    if (state === 'CANCELLED') {
      expect(detail).toContain('ORIGINAL_CANCEL_CONFIRMED');
      expect(detail).toContain('运行已终止');
    }
  });

  it('keeps the overview anchored to the actual current Brief and read-only navigation', () => {
    const html = render(<ResearchOverview project={project} current navigate={close} />, [[['briefs', 'project', 'current', brief.id], brief],
      [['cycles', 'project', undefined], empty]]);
    for (const value of [brief.content.hypothesis, '查看 Brief', '查看执行上下文', 'CLI / Skill']) expect(html).toContain(value);
    expect(html).not.toContain('编辑草稿');
    expect(html).not.toContain('明确确认启动');
  });
});
