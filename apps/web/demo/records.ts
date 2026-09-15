// Fixed SYNTHETIC presentation records. No database, account, jobs or delivery.
import type { Schema } from '../src/api';
import briefInput from '../../../tests/contracts/research-brief.json';
import policyInput from '../../../tests/contracts/research-policy.json';

export const id = (n: number) => `01990000-0000-7000-8000-${String(n).padStart(12, '0')}`;
const at = '2026-09-15T00:00:00Z';
const page = (items: unknown[]) => ({ schema_version: 1, items, next_cursor: null });
export const records = new Map<string, { contract: string; value: unknown }>();
function record(path: string, contract: string, value: unknown) { records.set(path, { contract, value }); }

const project: Schema['ProjectView'] = {
  id: id(1), root_lineage_id: id(1), name: 'SYNTHETIC · 双 Alpha 研究示例',
  description: '固定合成界面记录；没有执行研究、模型推理或交付。', state: 'DRAFT',
  current_brief_id: id(10), current_automation_policy_id: null, created_by: 'OPERATOR',
  archived_at: null, created_at: at, updated_at: at, revision: '1',
};
const brief: Schema['BriefView'] = {
  id: id(10), project_id: project.id, version: 1, revision: '1', state: 'FROZEN',
  content: { ...briefInput.content, hypothesis: 'SYNTHETIC：比较两种合成信号。',
    economic_rationale: '仅解释界面与记录关联，不代表经济有效性。' } as Schema['BriefContentV1'],
  bindings: briefInput.bindings as Schema['BriefBindingV1'][],
  supersedes_id: null, frozen_at: at, created_at: at, updated_at: at,
};
const run: Schema['RunSnapshotV1'] = {
  schema_version: 1, id: id(20), project_id: project.id, cycle_id: null, kind: 'IMPORT',
  input_set_id: id(21), state: 'SUCCEEDED', current_attempt_no: 1, active_attempt_id: null,
  last_event_seq: '0', deadline_at: at, cancellation_requested_at: null, terminal_reason_code: null,
  queued_at: at, started_at: at, finished_at: at, revision: '1',
};
record('/api/v2/bootstrap/status', '/api/v2/bootstrap/status', { schema_version: 1, initialized: true, setup_allowed: false });
record('/api/v2/auth/session', '/api/v2/auth/session', {
  schema_version: 1, authenticated_at: at, expires_at: '2099-01-01T00:00:00Z',
  trusted_device_id: null, recent_authentication_required: true,
} satisfies Schema['BrowserSession']);
record('/api/v2/projects', '/api/v2/projects', page([project]));
record(`/api/v2/projects/${project.id}`, '/api/v2/projects/{id}', project);
record(`/api/v2/projects/${project.id}/briefs`, '/api/v2/projects/{id}/briefs', page([brief]));
record(`/api/v2/briefs/${brief.id}`, '/api/v2/briefs/{id}', brief);
record('/api/v2/runs', '/api/v2/runs', page([run]));
record(`/api/v2/runs/${run.id}`, '/api/v2/runs/{id}', run);

const alphas: Schema['AlphaView'][] = [0, 1].map(n => ({
  id: id(30 + n), project_id: project.id, name: `SYNTHETIC · 信号 ${n + 1}`,
  lifecycle: 'RESEARCH', active_version_id: id(40 + n), active_version: '1',
  revision: '1', created_at: at, updated_at: at,
}));
record('/api/v2/alphas', '/api/v2/alphas', page(alphas));
for (const [n, alpha] of alphas.entries()) {
  const version: Schema['AlphaVersionView'] = {
    id: id(40 + n), project_id: project.id, alpha_id: alpha.id, version: '1',
    experiment_id: id(50 + n), root_lineage_id: project.root_lineage_id,
    code_artifact_id: id(60 + n), model_artifact_id: id(70 + n), signal_contract_version: '1',
    signal_kind: 'SCORE', horizon_kind: 'FIXED_BARS', horizon_value: '1', forecast_unit: 'UNITLESS_SCORE',
    calibration_id: null, runtime_image_ref: 'synthetic.invalid/example:fixture', origin: 'FIXTURE', created_at: at,
  };
  const evaluation: Schema['EvaluationView'] = {
    id: id(80 + n), project_id: project.id, subject_alpha_version_id: version.id, subject_candidate_id: null,
    input_set_id: id(21), policy_id: brief.content.evaluation_policy_id, run_id: run.id, evaluation_kind: 'WALK_FORWARD',
    execution_status: 'SUCCEEDED', evidence_status: 'INCOMPLETE', decision: 'INCONCLUSIVE',
    report_artifact_id: id(100 + n), method_versions_artifact_id: id(100 + n), origin: 'FIXTURE',
    concluded_at: at, valid_until: null, checked_at: at, unexpired_at_read: false,
  };
  record(`/api/v2/alphas/${alpha.id}/versions`, '/api/v2/alphas/{id}/versions', page([version]));
  record(`/api/v2/alphas/${alpha.id}/versions/1`, '/api/v2/alphas/{id}/versions/{version}', version);
  record(`/api/v2/alpha-versions/${version.id}/evaluations`, '/api/v2/alpha-versions/{id}/evaluations', page([evaluation]));
  record(`/api/v2/evaluations/${evaluation.id}`, '/api/v2/evaluations/{id}', evaluation);
  record(`/api/v2/evaluations/${evaluation.id}/metrics`, '/api/v2/evaluations/{id}/metrics', page([]));
  record(`/api/v2/alpha-versions/${version.id}/qualifications`, '/api/v2/alpha-versions/{id}/qualifications', page([]));
}

const { schema_version: _schema, selection, comparison_input_set_id, execution_assumptions_id: _assumptions, ...policyFields } = policyInput as Schema['EvaluationPolicyCreate'];
const policy: Schema['EvaluationPolicyView'] = {
  ...policyFields, id: brief.content.evaluation_policy_id, project_id: project.id, version: 1, created_at: at,
  question: 'SYNTHETIC · 真实数据与独立证据仍是资格前提', portfolio_metric_requirements: null, portfolio_study_plan: null,
  selection_rule: { ...selection, schema_version: 1, comparison_input_set_id, execution_assumptions_id: brief.content.execution_assumptions_id,
    comparable_scope: 'FAMILY_LINEAGE', root_lineage_id: project.root_lineage_id, family_id: id(205), tie_break: 'EXPERIMENT_ID_ASC', missing_required_metric: 'INCONCLUSIVE' },
};
const assumptions: Schema['ExecutionAssumptionsViewV1'] = {
  id: brief.content.execution_assumptions_id, project_id: project.id, input_set_id: id(21), dataset_revision_id: brief.bindings[0]!.dataset_revision_id,
  runtime_id: id(220), capability_snapshot_artifact_id: id(221), fee_schedule_artifact_id: id(201), engine_image_ref: 'synthetic.invalid/example:fixture',
  venue_capability_ref: 'SYNTHETIC', calendar_version: 'fixture-v1', settlement_rule_ref: 'fixture-only', cost_assumption_status: 'CONSERVATIVE_ASSUMPTION',
  bar_liquidity: null, bar_liquidity_valid_until: null, rolling_liquidity: null, rolling_liquidity_artifact_id: null, created_at: at,
  settings: { schema_version: 1, base_currency: 'USD', starting_capital: '1000', account_kind: 'CASH', leverage: '1', snapshot_interval_ms: 1000, exposure_tolerance: '0.000001',
    fee_rates: [{ instrument_id: 'SYNTHETIC.EXAMPLE', maker: '0.001', taker: '0.002' }],
    fee_model: { schema_version: 1, adapter_kind: 'NAUTILUS_MAKER_TAKER', upstream_class: 'nautilus_execution::models::fee::MakerTakerFeeModel', upstream_version: '0.63.0', parameters: {} },
    fill_model: { schema_version: 1, adapter_kind: 'NAUTILUS_DEFAULT_FILL', upstream_class: 'nautilus_execution::models::fill::DefaultFillModel', upstream_version: '0.63.0', parameters: { prob_fill_on_limit: '1', prob_slippage: '0', random_seed: '1' } },
    latency_model: { schema_version: 1, adapter_kind: 'NAUTILUS_STATIC_LATENCY', upstream_class: 'nautilus_execution::models::latency::StaticLatencyModel', upstream_version: '0.63.0', parameters: { base_latency_ns: '1000000', insert_latency_ns: '0', update_latency_ns: '0', cancel_latency_ns: '0' } },
  },
};
record('/api/v2/evaluation-policies', '/api/v2/evaluation-policies', page([policy]));
record(`/api/v2/evaluation-policies/${policy.id}`, '/api/v2/evaluation-policies/{id}', policy);
record(`/api/v2/projects/${project.id}/execution-assumptions`, '/api/v2/projects/{id}/execution-assumptions', page([assumptions]));
record(`/api/v2/execution-assumptions/${assumptions.id}`, '/api/v2/execution-assumptions/{id}', assumptions);

const mandate: Schema['MandateViewV1'] = {
  id: id(200), project_id: project.id, version: 1, created_at: at,
  content: {
    objective: 'MIN_RISK', risk_measure: 'VARIANCE', base_currency: 'USD', capital_assumption: '1000',
    universe_version_id: brief.content.universe_version_id, required_evaluation_policy_id: brief.content.evaluation_policy_id,
    execution_assumptions_id: brief.content.execution_assumptions_id, exposure_tolerance: '0.000001',
    covariance_estimator: { schema_version: 1, adapter_kind: 'SAMPLE_COVARIANCE', upstream_class: 'ndarray_stats::CorrelationExt::cov', upstream_version: '0.7.0', parameters: { ddof: 1 } },
    alpha_ensemble: { schema_version: 1, adapter_kind: 'FIXED_WEIGHTED_FORECAST', upstream_class: 'ndarray::ArrayBase::dot', upstream_version: '0.17.1', parameters: {} },
    optimizer: { schema_version: 1, adapter_kind: 'CLARABEL_QP', upstream_class: 'clarabel::solver::DefaultSolver', upstream_version: '0.11.1',
      parameters: { schema_version: 1, risk_aversion: '1', max_iterations: 200, solver_tolerance: '0.0000000001', accept_inaccurate: false, cvar_confidence: null, risk_budgeting: null } },
    constraints: { schema_version: 1, long_only: true, min_cash_weight: '0', max_cash_weight: '1', min_asset_weight: '0', max_asset_weight: '1',
      max_gross_exposure: '1', min_net_exposure: '0', max_net_exposure: '1', max_turnover_per_rebalance: '2', group_bounds: [], asset_overrides: [],
      transaction_costs_ref: id(201), max_ex_ante_risk: null, max_participation: null, liquidity_ref: null },
    rebalance_schedule: { schema_version: 1, kind: 'MANUAL', interval_seconds: null, calendar_ref: null, timezone: 'UTC',
      session_offset_seconds: null, max_input_age_seconds: 60, target_ttl_seconds: 300 },
  },
};
const rejectedRun: Schema['RunSnapshotV1'] = { ...run, id: id(202), kind: 'PORTFOLIO_BUILD', state: 'FAILED', terminal_reason_code: 'SYNTHETIC_NO_QUALIFIED_ALPHA' };
const candidate: Schema['CandidateViewV1'] = {
  id: id(203), project_id: project.id, mandate_id: mandate.id, input_set_id: id(21), run_id: rejectedRun.id,
  decision_asof: at, created_at: at, execution_status: 'FAILED', solver_status: 'FAILED', evidence_status: 'INCOMPLETE',
  origin: 'FIXTURE', reason_code: 'SYNTHETIC_NO_QUALIFIED_ALPHA', forecast_artifact_id: null, covariance_artifact_id: null,
  diagnostics_artifact_id: id(204), target_artifact_id: null, allocation_evaluation_id: null, cash_weight: null,
  current_weights_source: 'NONE', current_weights_artifact_id: null,
};
record('/api/v2/runs', '/api/v2/runs', page([run, rejectedRun]));
record(`/api/v2/runs/${rejectedRun.id}`, '/api/v2/runs/{id}', rejectedRun);
record(`/api/v2/projects/${project.id}/portfolio-mandates`, '/api/v2/projects/{id}/portfolio-mandates', page([mandate]));
record(`/api/v2/portfolio-mandates/${mandate.id}`, '/api/v2/portfolio-mandates/{id}', mandate);
record(`/api/v2/projects/${project.id}/portfolio-candidates`, '/api/v2/projects/{id}/portfolio-candidates', page([candidate]));
record(`/api/v2/portfolio-candidates/${candidate.id}`, '/api/v2/portfolio-candidates/{id}', { header: candidate, members: [], targets: [] } satisfies Schema['CandidateDetailV1']);
record(`/api/v2/portfolio-candidates/${candidate.id}/evaluations`, '/api/v2/portfolio-candidates/{id}/evaluations', page([]));

// No fake qualification, approval, Claim or account is issued by this preview.
for (const suffix of ['cycles', 'releases', 'handoffs', 'automation-policies']) {
  record(`/api/v2/projects/${project.id}/${suffix}`, `/api/v2/projects/{id}/${suffix}`, page([]));
}
for (const path of ['/api/v2/auth/devices', '/api/v2/settings/codex', '/api/v2/integrations/runtimes',
  '/api/v2/integrations/downstreams', '/api/v2/data/sources', '/api/v2/data/revisions', '/api/v2/data/universes']) {
  record(path, path, page([]));
}

export function demoResponse(method: string, pathname: string) {
  const item = method === 'GET' ? records.get(pathname) : undefined;
  if (item) return { status: 200, value: item.value };
  const status = method === 'GET' ? 404 : 403;
  return { status, value: {
    type: 'urn:quazonai:problem:demo', title: 'SYNTHETIC 界面预览', status,
    code: status === 403 ? 'FORBIDDEN' : 'NOT_FOUND', request_id: id(999), retryable: false,
    detail: status === 403 ? '预览不执行写入、认证、计算或交付。请勿输入真实凭据。' : '此预览场景尚未提供该记录。',
    field_errors: [], safe_next_actions: [],
  } satisfies Schema['Problem'] };
}
