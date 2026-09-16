// Fixed SYNTHETIC presentation records. No database, account, jobs or delivery.
import type { Schema } from '../src/api';
import briefInput from '../../../tests/contracts/research-brief.json';
import policyInput from '../../../tests/contracts/research-policy.json';
import runtimeCapabilities from '../../../tests/contracts/runtime-capabilities.fixture.json';

export const id = (n: number) => `01990000-0000-7000-8000-${String(n).padStart(12, '0')}`;
const at = '2026-09-15T00:00:10Z';
const frozenAt = '2026-09-15T00:00:00Z';
const checkedAt = '2026-09-16T00:00:00Z';
const nativeImage = runtimeCapabilities.image_refs.find(image => image.job_kind === 'ALPHA_EVALUATE')!.image_ref;
const page = (items: unknown[]) => ({ schema_version: 1, items, next_cursor: null });
export const records = new Map<string, { contract: string; value: unknown }>();
function record(path: string, contract: string, value: unknown) { records.set(path, { contract, value }); }

const project: Schema['ProjectView'] = {
  id: id(1), root_lineage_id: id(1), name: 'SYNTHETIC · 双 Alpha 研究示例',
  description: '固定合成界面记录；没有执行研究、模型推理或交付。', state: 'DRAFT',
  current_brief_id: id(10), current_automation_policy_id: null, created_by: 'OPERATOR',
  archived_at: null, created_at: frozenAt, updated_at: at, revision: '1',
};
const brief: Schema['BriefView'] = {
  id: id(10), project_id: project.id, version: 1, revision: '1', state: 'FROZEN',
  content: { ...briefInput.content, hypothesis: 'SYNTHETIC：比较两种合成信号。',
    economic_rationale: '仅解释界面与记录关联，不代表经济有效性。' } as Schema['BriefContentV1'],
  bindings: [...briefInput.bindings as Schema['BriefBindingV1'][],
    { dataset_revision_id: id(901), role: 'VALIDATION', access_policy: 'RESEARCH_READ' }],
  supersedes_id: null, frozen_at: frozenAt, created_at: frozenAt, updated_at: frozenAt,
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
const inputSet = {
  id: id(21), project_id: project.id, purpose: 'DISCOVERY', decision_cutoff: frozenAt, frozen_at: frozenAt, revision: '1', created_at: frozenAt,
} satisfies Schema['InputSetSummary'];
const inputs = [inputSet,
  { ...inputSet, id: policyInput.comparison_input_set_id, purpose: 'VALIDATION' },
  { ...inputSet, id: id(900), purpose: 'SEALED' },
] satisfies Schema['InputSetSummary'][];
record('/api/v2/input-sets', '/api/v2/input-sets', page(inputs));
for (const [index, input] of inputs.entries()) record(`/api/v2/input-sets/${input.id}`, '/api/v2/input-sets/{id}', {
  header: input, items: [{ id: id(index === 0 ? 22 : 901 + index), ordinal: 0,
    item: { kind: 'DATASET', dataset_revision_id: brief.bindings.find(binding => binding.role === input.purpose)!.dataset_revision_id, role: input.purpose },
    origin: 'FIXTURE', pit_status: 'UNVERIFIED' }],
} satisfies Schema['InputSetView']);
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
    calibration_id: null, runtime_image_ref: nativeImage, origin: 'FIXTURE', created_at: at,
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
  ...policyFields, id: brief.content.evaluation_policy_id, project_id: project.id, version: 1, created_at: frozenAt,
  question: 'SYNTHETIC · 仅合成研究，不授予真实资格', require_real_data: false,
  split_policy: { ...policyFields.split_policy, label_horizon_observations: brief.content.horizon_value, purge_observations: briefInput.content.horizon_value,
    sealed_revision_id: brief.bindings.find(binding => binding.role === 'SEALED')!.dataset_revision_id },
  metric_requirements: policyFields.metric_requirements.map(item => ({ ...item, metric_code: 'PEARSON_IC', scope: 'asset:0/fold:0', method_allowlist: ['ndarray-stats.pearson_correlation'] })),
  sealed_metric_requirements: policyFields.sealed_metric_requirements!.map(item => ({ ...item, metric_code: 'PEARSON_IC', scope: 'asset:0', method_allowlist: ['ndarray-stats.pearson_correlation'] })),
  required_capabilities: [], portfolio_metric_requirements: null, portfolio_study_plan: null,
  selection_rule: { ...selection, metric_code: 'PEARSON_IC', metric_scope: 'asset:0/fold:0', method_id: 'ndarray-stats.pearson_correlation',
    method_version: '0.7.0', unit: 'CORRELATION', frequency: `1-MINUTE-LAST-EXTERNAL;horizon=${brief.content.horizon_value}`, schema_version: 1, comparison_input_set_id, execution_assumptions_id: brief.content.execution_assumptions_id,
    comparable_scope: 'FAMILY_LINEAGE', root_lineage_id: project.root_lineage_id, family_id: id(205), tie_break: 'EXPERIMENT_ID_ASC', missing_required_metric: 'INCONCLUSIVE' },
};
const cycle: Schema['CycleViewV1'] = {
  schema_version: 1, id: id(300), project_id: project.id, brief_id: brief.id, ordinal: 1,
  trigger: 'OPERATOR', state: 'COMPLETED', outcome: 'NO_SUPPORTED_CANDIDATE',
  next_action: 'SYNTHETIC · 补齐真实证据后才可重新研究；本预览没有执行模型任务。',
  budget: brief.content.budget, used_experiments: 2, reserved_experiments: 0, reserved_cpu_seconds: '0',
  initial_run_id: null, researcher_profile: null, reviewer_profile: null,
  available_actions: ['VIEW_BRIEF', 'VIEW_SELECTION'], revision: '1', created_at: at, started_at: at, ended_at: at,
};
const researchRun: Schema['RunSnapshotV1'] = {
  ...run, id: id(301), cycle_id: cycle.id, kind: 'AGENT_RESEARCH', terminal_reason_code: 'SYNTHETIC_PRESENTATION_ONLY',
};
record(`/api/v2/runs/${researchRun.id}`, '/api/v2/runs/{id}', researchRun);
for (const item of [run, researchRun]) record(`/api/v2/runs/${item.id}/rebalance`, '/api/v2/runs/{id}/rebalance', {
  schema_version: 1, rebalance: null,
} satisfies Schema['RunRebalanceViewV1']);
const selectionSnapshot: Schema['CycleSelectionV1'] = {
  schema_version: 1, cycle_id: cycle.id, project_id: project.id, research_run_id: researchRun.id,
  policy_id: policy.id, rule: policy.selection_rule, status: 'COMPLETE', trial_count: '2',
  eligible_count: '0', selected_count: '0', unfinished_count: '0', created_at: at,
};
const trials: Schema['CycleSelectionTrialV1'][] = alphas.map((alpha, n) => ({
  schema_version: 1, cycle_id: cycle.id, source_cycle_id: cycle.id, experiment_id: id(50 + n),
  alpha_version_id: alpha.active_version_id, review_alpha_version_id: null, evaluation_id: id(80 + n),
  execution_run_id: run.id, execution_state: 'SUCCEEDED', compile_run_id: null, discovery_run_id: null,
  validation_run_id: null, rank: null, selected: false, selection_metric: null, reason: 'INVALID_EVIDENCE', unfinished: false,
}));
record(`/api/v2/projects/${project.id}/cycles`, '/api/v2/projects/{id}/cycles', page([cycle]));
record(`/api/v2/cycles/${cycle.id}/selection`, '/api/v2/cycles/{id}/selection', selectionSnapshot);
record(`/api/v2/cycles/${cycle.id}/selection/trials`, '/api/v2/cycles/{id}/selection/trials', page(trials));

const assumptions: Schema['ExecutionAssumptionsViewV1'] = {
  id: brief.content.execution_assumptions_id, project_id: project.id, input_set_id: id(21), dataset_revision_id: brief.bindings[0]!.dataset_revision_id,
  runtime_id: id(220), capability_snapshot_artifact_id: id(221), fee_schedule_artifact_id: id(201), engine_image_ref: nativeImage,
  venue_capability_ref: 'EXAMPLE', calendar_version: 'fixture-v1', settlement_rule_ref: 'fixture-only', cost_assumption_status: 'CONSERVATIVE_ASSUMPTION',
  bar_liquidity: null, bar_liquidity_valid_until: null, rolling_liquidity: null, rolling_liquidity_artifact_id: null, created_at: frozenAt,
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
record('/api/v2/runs', '/api/v2/runs', page([run, researchRun, rejectedRun]));
record(`/api/v2/runs/${rejectedRun.id}`, '/api/v2/runs/{id}', rejectedRun);
record(`/api/v2/runs/${rejectedRun.id}/rebalance`, '/api/v2/runs/{id}/rebalance', { schema_version: 1, rebalance: null } satisfies Schema['RunRebalanceViewV1']);
record(`/api/v2/projects/${project.id}/portfolio-mandates`, '/api/v2/projects/{id}/portfolio-mandates', page([mandate]));
record(`/api/v2/portfolio-mandates/${mandate.id}`, '/api/v2/portfolio-mandates/{id}', mandate);
record(`/api/v2/projects/${project.id}/portfolio-candidates`, '/api/v2/projects/{id}/portfolio-candidates', page([candidate]));
record(`/api/v2/portfolio-candidates/${candidate.id}`, '/api/v2/portfolio-candidates/{id}', { header: candidate, members: [], targets: [] } satisfies Schema['CandidateDetailV1']);
record(`/api/v2/portfolio-candidates/${candidate.id}/evaluations`, '/api/v2/portfolio-candidates/{id}/evaluations', page([]));

const runtime: Schema['RuntimeView'] = {
  id: id(220), revision: '2', protocol_version: 1, credential_configured: false, ca_configured: false,
  last_capability_snapshot_artifact_id: null, created_at: '2026-01-01T00:00:00Z', updated_at: checkedAt,
  configuration: { name: 'SYNTHETIC · 未连接 Runtime', endpoint: 'https://synthetic.invalid', tls_policy: 'SYSTEM_CA',
    enabled: false, development_http: false, allowed_capabilities: ['DATA_VALIDATE'] },
};
const source: Schema['DataSourceView'] = {
  id: id(230), name: 'SYNTHETIC · 演示目录', runtime_id: runtime.id, native_catalog_ref: 'synthetic/catalog',
  provider_kind: 'NAUTILUS_CATALOG', enabled: false, revision: '2', created_at: '2026-01-01T00:00:00Z', updated_at: checkedAt,
};
const grant: Schema['DataGrantView'] = {
  id: id(231), source_id: source.id, version: '1', license_reference: 'SYNTHETIC · 已过期的演示许可',
  evidence_artifact_id: id(232), allowed_uses: 'RESEARCH', valid_from: '2026-01-01T00:00:00Z', valid_until: '2026-09-15T00:05:00Z',
  created_at: '2026-01-01T00:00:00Z', license_state: 'EXPIRED', checked_at: checkedAt,
};
const universe: Schema['UniverseView'] = {
  id: brief.content.universe_version_id, name: 'SYNTHETIC · 演示投资域', registration_state: 'LEGACY_UNVERIFIED',
  membership_artifact_id: id(233), instrument_definitions_artifact_id: id(234), calendar_ref: 'fixture-calendar',
  calendar_version: 'fixture-v1', selection_asof: frozenAt, has_historical_membership: false,
  coverage_start: '2026-01-01T00:00:00Z', coverage_end: at, created_at: '2026-01-01T00:00:00Z',
};
const datasets: Schema['DatasetView'][] = brief.bindings.map((binding, n) => ({
  id: binding.dataset_revision_id, source_id: source.id, data_use_grant_id: grant.id, native_snapshot_ref: `synthetic/snapshot-${n}`,
  storage_version: '1', universe_version_id: universe.id, schema_version: '1', data_kind: 'BAR', partition: binding.role,
  event_start: `2026-0${binding.role === 'DISCOVERY' ? 1 : binding.role === 'VALIDATION' ? 2 : 3}-01T00:00:00Z`,
  event_end: `2026-0${binding.role === 'DISCOVERY' ? 1 : binding.role === 'VALIDATION' ? 2 : 3}-02T00:00:00Z`,
  available_through: frozenAt, row_count: '200', timezone: 'UTC',
  quality_artifact_id: id(250 + n), pit_status: 'UNVERIFIED', revision_policy: 'UNKNOWN', origin: 'FIXTURE', created_at: frozenAt,
  native_metadata_artifact_id: id(240 + n), registration_observed_at: frozenAt,
  source_enabled: false, runtime_enabled: false, license_state: 'EXPIRED', checked_at: checkedAt,
}));
record('/api/v2/integrations/runtimes', '/api/v2/integrations/runtimes', page([runtime]));
record(`/api/v2/integrations/runtimes/${runtime.id}`, '/api/v2/integrations/runtimes/{id}', runtime);
record(`/api/v2/briefs/${brief.id}/execution-context`, '/api/v2/briefs/{id}/execution-context', {
  schema_version: 1, brief, execution_context: { schema_version: 1,
    runtime_id: runtime.id, runtime_revision: '1',
    discovery_input_set_id: inputSet.id, validation_input_set_id: policyInput.comparison_input_set_id, sealed_input_set_id: id(900),
  },
} satisfies Schema['FrozenBriefV1']);
record(`/api/v2/integrations/runtimes/${runtime.id}/readiness`, '/api/v2/integrations/runtimes/{id}/readiness', {
  schema_version: 1, runtime_id: runtime.id, integration_revision: runtime.revision, state: 'DISABLED', available_job_kinds: [],
  latest_observation: { id: id(222), runtime_id: runtime.id, integration_revision: '1', snapshot_artifact_id: id(221),
    observed_at: '2026-09-14T23:59:30Z', valid_until: '2026-09-15T00:00:30Z',
    outcome: { status: 'AVAILABLE', capabilities: {
      ...runtimeCapabilities as Schema['RuntimeCapabilitiesV1'], checked_at: '2026-09-14T23:59:30Z',
      engine_versions: { ...runtimeCapabilities.engine_versions, 'solow-cv': '0.7.3', 'ndarray-stats': '0.7.0', linregress: '0.5.4', nautilus: '0.63.0', 'simulation-models': '1' },
      job_kinds: [...runtimeCapabilities.job_kinds as Schema['RunKind'][], 'PORTFOLIO_SIMULATE'],
      image_refs: [...runtimeCapabilities.image_refs as Schema['RuntimeImageV1'][], { job_kind: 'PORTFOLIO_SIMULATE', image_ref: nativeImage }],
      venues: [{ venue: 'EXAMPLE', instrument_classes: ['Equity'], data_kinds: ['BAR'], expiry_and_settlement: false }],
      artifact_schemas: [...runtimeCapabilities.artifact_schemas, ...['qz.alpha_validation', 'qz.alpha_sealed', 'qz.wasm_model', 'qz.model_compilation', 'qz.native_forecast', 'qz.native_simulation', 'qz.data_quality'].map(name => ({ name, version: '1' }))],
    } },
  },
} satisfies Schema['RuntimeReadinessV1']);
record('/api/v2/data/sources', '/api/v2/data/sources', page([source]));
record(`/api/v2/data/sources/${source.id}`, '/api/v2/data/sources/{id}', source);
record(`/api/v2/data/sources/${source.id}/grants`, '/api/v2/data/sources/{id}/grants', page([grant]));
record(`/api/v2/data/grants/${grant.id}/revocations`, '/api/v2/data/grants/{id}/revocations', page([]));
record('/api/v2/data/universes', '/api/v2/data/universes', page([universe]));
record(`/api/v2/data/universes/${universe.id}`, '/api/v2/data/universes/{id}', universe);
record('/api/v2/data/revisions', '/api/v2/data/revisions', page(datasets));
for (const dataset of datasets) record(`/api/v2/data/revisions/${dataset.id}`, '/api/v2/data/revisions/{id}', dataset);

// Hypothetical, expired DEMO history: these presentation records never enter a
// database or qualify a real Alpha. The production approval paths remain absent.
const expired = '2026-09-15T00:05:00Z';
const observed = '2026-09-16T00:00:00Z';
const demoAlphaRuns: Schema['RunSnapshotV1'][] = [];
const demoAlphas: Schema['AlphaView'][] = alphas.map((alpha, n) => ({ ...alpha,
  id: id(400 + n), name: `SYNTHETIC · 历史展示 ${n + 1}`, active_version_id: id(410 + n),
}));
for (const [n, alpha] of demoAlphas.entries()) {
  const evaluationRun: Schema['RunSnapshotV1'] = { ...run, id: id(450 + n), kind: 'ALPHA_EVALUATE', terminal_reason_code: 'SYNTHETIC_PRESENTATION_ONLY' };
  demoAlphaRuns.push(evaluationRun);
  record(`/api/v2/runs/${evaluationRun.id}`, '/api/v2/runs/{id}', evaluationRun);
  record(`/api/v2/runs/${evaluationRun.id}/rebalance`, '/api/v2/runs/{id}/rebalance', { schema_version: 1, rebalance: null } satisfies Schema['RunRebalanceViewV1']);
  const original = records.get(`/api/v2/alphas/${alphas[n]!.id}/versions/1`)!.value as Schema['AlphaVersionView'];
  const version: Schema['AlphaVersionView'] = { ...original, id: alpha.active_version_id!, alpha_id: alpha.id,
    experiment_id: id(440 + n), code_artifact_id: id(460 + n), model_artifact_id: id(470 + n),
    signal_kind: 'EXPECTED_RETURN', forecast_unit: 'RETURN_PER_HORIZON',
  };
  const evaluation: Schema['EvaluationView'] = {
    id: id(420 + n), project_id: project.id, subject_alpha_version_id: version.id, subject_candidate_id: null,
    input_set_id: id(21), policy_id: policy.id, run_id: evaluationRun.id, evaluation_kind: 'WALK_FORWARD',
    execution_status: 'SUCCEEDED', evidence_status: 'VALID', decision: 'PASS', origin: 'FIXTURE',
    report_artifact_id: id(480 + n), method_versions_artifact_id: id(480 + n), concluded_at: at,
    valid_until: expired, checked_at: observed, unexpired_at_read: false,
  };
  const qualification: Schema['QualificationView'] = { id: id(430 + n), alpha_version_id: version.id,
    policy_id: policy.id, qualifying_evaluation_id: id(490 + n), granted_at: at, valid_until: expired,
    created_at: at, checked_at: observed, grant_window_open: false, revocation: null,
  };
  record(`/api/v2/alphas/${alpha.id}/versions`, '/api/v2/alphas/{id}/versions', page([version]));
  record(`/api/v2/alphas/${alpha.id}/versions/1`, '/api/v2/alphas/{id}/versions/{version}', version);
  record(`/api/v2/alpha-versions/${version.id}/evaluations`, '/api/v2/alpha-versions/{id}/evaluations', page([evaluation]));
  record(`/api/v2/evaluations/${evaluation.id}`, '/api/v2/evaluations/{id}', evaluation);
  record(`/api/v2/evaluations/${evaluation.id}/metrics`, '/api/v2/evaluations/{id}/metrics', page([]));
  record(`/api/v2/alpha-versions/${version.id}/qualifications`, '/api/v2/alpha-versions/{id}/qualifications', page([qualification]));
}
record('/api/v2/alphas', '/api/v2/alphas', page([...alphas, ...demoAlphas]));
const demoCandidate: Schema['CandidateViewV1'] = { ...candidate, id: id(500), run_id: id(501),
  execution_status: 'SUCCEEDED', solver_status: 'OPTIMAL', evidence_status: 'VALID', reason_code: 'SYNTHETIC_PRESENTATION_ONLY',
  forecast_artifact_id: id(502), covariance_artifact_id: id(503), diagnostics_artifact_id: id(504),
  target_artifact_id: id(505), allocation_evaluation_id: null, cash_weight: '0.2',
};
const demoEvaluation: Schema['EvaluationView'] = {
  id: id(506), project_id: project.id, subject_alpha_version_id: null, subject_candidate_id: demoCandidate.id,
  input_set_id: id(21), policy_id: policy.id, run_id: id(512), evaluation_kind: 'PORTFOLIO', execution_status: 'SUCCEEDED',
  evidence_status: 'VALID', decision: 'PASS', report_artifact_id: id(507), method_versions_artifact_id: id(507),
  origin: 'FIXTURE', concluded_at: at, valid_until: expired, checked_at: observed, unexpired_at_read: false,
};
export const demoPackage: Schema['TargetPackageV1'] = {
  release_id: id(510), package_schema_version: '1', environment_origin: 'DEMO', project_id: project.id,
  candidate_id: demoCandidate.id, mandate_id: mandate.id, qualification_refs: [id(430), id(431)], evaluation_refs: [demoEvaluation.id],
  input_revision_refs: datasets.map(item => item.id), engine_versions: { presentation: 'SYNTHETIC_NOT_EXECUTED' },
  asof: at, valid_from: at, valid_until: expired, base_currency: 'USD', capital_assumption: '1000', current_weights_source: 'NONE',
  targets: [{ instrument_id: 'SYNTHETIC.EXAMPLE', target_weight: '0.8', currency: 'USD' }], cash_weight: '0.2',
  constraints_summary: mandate.content.constraints, exposure_tolerance: mandate.content.exposure_tolerance,
  cost_assumption_ref: assumptions.id, compatible_market_capabilities: ['SYNTHETIC_ONLY'],
  limitations: ['SYNTHETIC / FIXTURE：假设历史仅用于界面演示；没有执行优化、评估或授予真实资格。', '已过期 DEMO；不能审批、登记 Offer 或领取。'],
  provenance_artifact_refs: [id(507)],
};
const demoRelease: Schema['ReleaseViewV1'] = { id: demoPackage.release_id, project_id: project.id, candidate_id: demoCandidate.id,
  mandate_id: mandate.id, evaluation_id: demoEvaluation.id, package_artifact_id: id(511), package_schema_version: '1',
  market_capability_version: 'SYNTHETIC_ONLY', environment: 'DEMO', asof: at, valid_from: at, valid_until: expired, created_at: at,
};
export const packageBytes = JSON.stringify(demoPackage, null, 2) + '\n';
const demoRun: Schema['RunSnapshotV1'] = { ...run, id: id(501), kind: 'PORTFOLIO_BUILD', terminal_reason_code: 'SYNTHETIC_PRESENTATION_ONLY' };
const simulationRun: Schema['RunSnapshotV1'] = { ...demoRun, id: id(512), kind: 'PORTFOLIO_SIMULATE' };
record(`/api/v2/runs/${simulationRun.id}`, '/api/v2/runs/{id}', simulationRun);
record(`/api/v2/runs/${simulationRun.id}/rebalance`, '/api/v2/runs/{id}/rebalance', { schema_version: 1, rebalance: null } satisfies Schema['RunRebalanceViewV1']);
record('/api/v2/runs', '/api/v2/runs', page([run, researchRun, rejectedRun, ...demoAlphaRuns, demoRun, simulationRun]));
record(`/api/v2/runs/${demoRun.id}`, '/api/v2/runs/{id}', demoRun);
record(`/api/v2/runs/${demoRun.id}/rebalance`, '/api/v2/runs/{id}/rebalance', { schema_version: 1, rebalance: null } satisfies Schema['RunRebalanceViewV1']);
record(`/api/v2/artifacts/${demoRelease.package_artifact_id}`, '/api/v2/artifacts/{id}', {
  id: demoRelease.package_artifact_id, project_id: project.id, producer_run_id: null, producer_attempt_id: null,
  kind: 'PACKAGE', media_type: 'application/json', schema_name: 'qz.target_package', schema_version: '1', byte_count: String(Buffer.byteLength(packageBytes)),
  access_class: 'DELIVERY', origin: 'FIXTURE', created_by: 'IMPORT', created_at: at,
} satisfies Schema['ArtifactView']);
record(`/api/v2/portfolio-candidates/${demoCandidate.id}`, '/api/v2/portfolio-candidates/{id}', { header: demoCandidate,
  members: demoAlphas.map((alpha, n) => ({ alpha_version_id: alpha.active_version_id!, qualification_id: id(430 + n), ensemble_weight: '0.5', calibration_id: null, forecast_unit: 'RETURN_PER_HORIZON', coverage_fraction: '1' })),
  targets: demoPackage.targets.map(item => ({ ...item, asof: at, valid_until: expired })),
} satisfies Schema['CandidateDetailV1']);
record(`/api/v2/projects/${project.id}/portfolio-candidates`, '/api/v2/projects/{id}/portfolio-candidates', page([candidate, demoCandidate]));
record(`/api/v2/portfolio-candidates/${demoCandidate.id}/evaluations`, '/api/v2/portfolio-candidates/{id}/evaluations', page([demoEvaluation]));
record(`/api/v2/evaluations/${demoEvaluation.id}`, '/api/v2/evaluations/{id}', demoEvaluation);
record(`/api/v2/evaluations/${demoEvaluation.id}/metrics`, '/api/v2/evaluations/{id}/metrics', page([]));
record(`/api/v2/projects/${project.id}/releases`, '/api/v2/projects/{id}/releases', page([demoRelease]));
record(`/api/v2/releases/${demoRelease.id}`, '/api/v2/releases/{id}', demoRelease);
record(`/api/v2/releases/${demoRelease.id}/approvals`, '/api/v2/releases/{id}/approvals', page([]));
record(`/api/v2/releases/${demoRelease.id}/decisions`, '/api/v2/releases/{id}/decisions', page([]));

record('/api/v2/artifacts', '/api/v2/artifacts', page([...records.values()].filter(item => item.contract === '/api/v2/artifacts/{id}').map(item => item.value)));

// No approval, Claim or account is issued by this preview.
for (const suffix of ['handoffs', 'automation-policies', 'forward', 'forward-observations', 'wakes']) {
  record(`/api/v2/projects/${project.id}/${suffix}`, `/api/v2/projects/{id}/${suffix}`, page([]));
}
for (const path of ['/api/v2/auth/devices', '/api/v2/settings/codex', '/api/v2/integrations/downstreams', '/api/v2/migrations/reports']) {
  record(path, path, page([]));
}
record('/api/v2/codex/homes', '/api/v2/codex/homes', []);

export function demoResponse(method: string, pathname: string, partition: string | null = null) {
  if (method === 'GET' && pathname === `/api/v2/artifacts/${id(511)}/content`) return { status: 200, value: packageBytes, binary: true };
  const item = method === 'GET' ? records.get(pathname) : undefined;
  if (item && pathname === '/api/v2/data/revisions' && partition) {
    return { status: 200, value: page(datasets.filter(dataset => dataset.partition === partition)) };
  }
  if (item) return { status: 200, value: item.value };
  const status = method === 'GET' ? 404 : 403;
  return { status, value: {
    type: 'urn:quazonai:problem:demo', title: 'SYNTHETIC 界面预览', status,
    code: status === 403 ? 'FORBIDDEN' : 'NOT_FOUND', request_id: id(999), retryable: false,
    detail: status === 403 ? '预览不执行写入、认证、计算或交付。请勿输入真实凭据。' : '此预览场景尚未提供该记录。',
    field_errors: [], safe_next_actions: [],
  } satisfies Schema['Problem'] };
}
