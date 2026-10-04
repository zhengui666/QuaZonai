import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { App, type DrawerProps } from 'antd';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api, displayTime, type Schema } from './api';
import { ExecutionAssumptions, ExecutionAssumptionDetail } from './execution-assumptions';
import { EvaluationPolicies, EvaluationPolicyDetail } from './evaluation-policies';
import { AutomationPolicies, AutomationPolicyDetail } from './automation-policies';

// Keep real tables, queries and actions. Only Drawer portals are inlined because
// Ant Design hides their children during SSR. Click/keyboard/Back behavior and
// query transport remain native browser checks, not claims of these render tests.
vi.mock('antd', async () => {
  const actual = await vi.importActual<typeof import('antd')>('antd');
  return { ...actual, Drawer: ({ open, title, children }: DrawerProps) => open
    ? createElement('section', { 'aria-label': typeof title === 'string' ? title : undefined }, children) : null };
});
const writes = () => { throw new Error('Policy observation tabs must not submit business commands'); };
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
    expect(html).not.toMatch(/<form\b|<input\b|<textarea\b|type="submit"|新建执行假设|保存执行假设|新建评估政策|保存不可变评估政策|冻结自动化政策|确认冻结政策|重试同一政策|撤销政策|确认追加撤销/);
    for (const method of ['POST', 'PUT', 'PATCH', 'DELETE'] as const) expect(api[method]).not.toHaveBeenCalled();
    return html;
  } finally { client.clear(); }
}

const close = () => {};
const time = '2026-09-01T00:00:00Z';
const empty = { schema_version: 1, items: [], next_cursor: null };
const assumption: Schema['ExecutionAssumptionsViewV1'] = {
  id: 'assumption', project_id: 'project', input_set_id: 'original-input', dataset_revision_id: 'original-dataset', runtime_id: 'runtime',
  capability_snapshot_artifact_id: 'original-capabilities', fee_schedule_artifact_id: 'original-fees', engine_image_ref: 'native-image',
  venue_capability_ref: 'original-venue', calendar_version: 'original-calendar', settlement_rule_ref: 'original-settlement',
  cost_assumption_status: 'CONSERVATIVE_ASSUMPTION', created_at: time,
  bar_liquidity: { schema_version: 1, report_artifact_id: 'original-bar-report', maximum_age_seconds: 60, participation_limit: '0.01' },
  bar_liquidity_valid_until: '2026-09-02T00:00:00Z',
  rolling_liquidity: { schema_version: 1, maximum_age_seconds: 300, participation_limit: '0.02' },
  rolling_liquidity_artifact_id: 'original-rolling-policy',
  settings: { schema_version: 1, account_kind: 'CASH', base_currency: 'USD', starting_capital: '12345.67', exposure_tolerance: '0.001',
    leverage: '1', snapshot_interval_ms: 1000, fee_rates: [],
    fee_model: { schema_version: 1, adapter_kind: 'NAUTILUS_MAKER_TAKER', upstream_class: 'nautilus_execution::models::fee::MakerTakerFeeModel', upstream_version: '0.63.0', parameters: {} },
    fill_model: { schema_version: 1, adapter_kind: 'NAUTILUS_DEFAULT_FILL', upstream_class: 'nautilus_execution::models::fill::DefaultFillModel', upstream_version: '0.63.0', parameters: { prob_fill_on_limit: '1', prob_slippage: '0', random_seed: '7' } },
    latency_model: { schema_version: 1, adapter_kind: 'NAUTILUS_STATIC_LATENCY', upstream_class: 'nautilus_execution::models::latency::StaticLatencyModel', upstream_version: '0.63.0', parameters: { base_latency_ns: '1', insert_latency_ns: '2', update_latency_ns: '3', cancel_latency_ns: '4' } },
  },
};
const metric: Schema['MetricRequirementV1'] = { schema_version: 1, metric_code: 'ORIGINAL_RETURN', scope: 'portfolio', comparator: 'GE',
  threshold_low: '0.03', threshold_high: null, minimum_observations: '30', method_allowlist: ['original-method'], required: true };
const evaluation: Schema['EvaluationPolicyView'] = {
  id: 'evaluation-policy', project_id: 'project', version: 2, question: 'Original frozen research question', created_at: time,
  maximum_missing_fraction: '0.01', maximum_sealed_uses_per_lineage: 1, minimum_observations: 30, require_real_data: true,
  required_capabilities: ['original-capability'], validity_seconds: '3600', metric_requirements: [metric], sealed_metric_requirements: [metric],
  portfolio_metric_requirements: null, portfolio_study_plan: null,
  selection_rule: { schema_version: 1, candidate_count: 1, comparable_scope: 'FAMILY_LINEAGE', comparison_input_set_id: 'original-comparison',
    direction: 'MAXIMIZE', evaluation_kind: 'WALK_FORWARD', execution_assumptions_id: 'assumption', family_id: 'original-family', frequency: 'UTC_DAY',
    method_id: 'original-method', method_version: '0.63.0', metric_code: 'ORIGINAL_RETURN', metric_scope: 'portfolio',
    missing_required_metric: 'INCONCLUSIVE', root_lineage_id: 'original-lineage', tie_break: 'EXPERIMENT_ID_ASC', unit: 'RETURN' },
  split_policy: { schema_version: 1, kind: 'WALK_FORWARD', train_size: '60', test_size: '30', step_size: '30', purge_observations: '1',
    embargo_observations: '1', interval_validation_required: true, sealed_revision_id: 'original-sealed' },
};
const automation: Schema['AutomationPolicyViewV1'] = {
  id: 'automation-policy', project_id: 'project', created_at: time, authorized_at: time,
  content: { mode: 'AUTO_PAPER', mandate_id: 'original-mandate', downstream_id: 'original-downstream', enabled_for_new_rebalances: false,
    valid_until: '2026-09-02T00:00:00Z', max_feedback_age_seconds: '60', max_rebalances_per_day: 2, minimum_paper_elapsed_seconds: '3600',
    required_paper_observations: 3, promotion_metric_requirements: [metric], degradation_metric_requirements: [metric] },
};
const revocation: Schema['PolicyRevocationViewV1'] = { id: 'original-revocation', automation_policy_id: automation.id,
  created_at: time, effective_at: '2026-09-01T12:00:00Z', reason: 'Original scheduled revocation reason' };

describe('read-only portfolio and delivery policy tabs', () => {
  it('retains empty histories and pagination without create or freeze shortcuts', () => {
    const assumptions = render(<ExecutionAssumptions project="project" />, [[['execution-assumptions', 'project', undefined], empty]]);
    expect(assumptions).toContain('暂无执行假设');
    expect(assumptions).toContain('刷新执行假设');
    expect(assumptions).toContain('下一页');
    const evaluations = render(<EvaluationPolicies project="project" />, [[['evaluation-policies', 'project', undefined], empty]]);
    expect(evaluations).toContain('暂无评估政策');
    expect(evaluations).toContain('刷新政策');
    const automations = render(<AutomationPolicies project="project" />, [[['automation-policies', 'project', undefined], empty]]);
    expect(automations).toContain('原政策');
    expect(automations).toContain('上一页');
  });

  it('preserves assumption rows, detail navigation and original native-model evidence', () => {
    const list = render(<ExecutionAssumptions project="project" />, [[['execution-assumptions', 'project', undefined], { ...empty, items: [assumption], next_cursor: 'next' }]]);
    expect(list).toContain('查看假设 assumption');
    expect(list).toContain('12345.67 USD');
    const detail = render(<ExecutionAssumptionDetail id={assumption.id} close={close} />, [[['execution-assumption', assumption.id], assumption]]);
    for (const value of ['不可变执行假设', 'original-input', 'original-dataset', 'original-capabilities', 'original-fees', 'native-image',
      'original-bar-report', 'original-rolling-policy', 'CONSERVATIVE_ASSUMPTION', 'NAUTILUS_DEFAULT_FILL', '12345.67', displayTime(assumption.bar_liquidity_valid_until)]) expect(detail).toContain(value);
  });

  it('preserves evaluation eligibility limits and the complete frozen policy', () => {
    const list = render(<EvaluationPolicies project="project" />, [[['evaluation-policies', 'project', undefined], { ...empty, items: [evaluation] }]]);
    expect(list).toContain('政策 v2');
    expect(list).toContain(evaluation.question);
    expect(list).toContain('未定义，不能授予组合 PASS');
    const detail = render(<EvaluationPolicyDetail id={evaluation.id} project="project" close={close} />, [[['evaluation-policy', 'project', evaluation.id], evaluation]]);
    for (const value of ['原完整评估政策', 'original-comparison', 'original-sealed', 'original-lineage', 'INCONCLUSIVE', 'ORIGINAL_RETURN']) expect(detail).toContain(value);
  });

  it('retains the automation policy and original revocations after removing revocation authoring', () => {
    const list = render(<AutomationPolicies project="project" />, [[['automation-policies', 'project', undefined], { ...empty, items: [automation] }]]);
    expect(list).toContain(automation.id);
    expect(list).toContain('AUTO_PAPER');
    const detail = render(<AutomationPolicyDetail policy={automation} />, [[['policy-revocations', automation.id], [revocation]]]);
    for (const value of ['原自动化政策', '原政策撤销历史', 'original-mandate', 'original-downstream', 'ORIGINAL_RETURN', revocation.id,
      revocation.reason, displayTime(revocation.effective_at)]) expect(detail).toContain(value);
  });

  it('keeps an empty revocation history read-only without fabricating a receipt', () => {
    const detail = render(<AutomationPolicyDetail policy={automation} />, [[['policy-revocations', automation.id], []]]);
    expect(detail).toContain('原政策撤销历史');
    expect(detail).not.toContain(revocation.id);
    expect(detail).not.toContain('原撤销已追加');
  });
});
