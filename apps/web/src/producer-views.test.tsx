import { createElement, type ReactNode } from 'react';
import { renderToStaticMarkup } from 'react-dom/server';
import { App, type DrawerProps } from 'antd';
import { QueryClient, QueryClientProvider } from '@tanstack/react-query';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { api, type Schema } from './api';
import { VersionDetail } from './alphas';
import { Mandates, MandateDetail } from './portfolio';
import { Candidates, CandidateDetail } from './portfolio-candidates';
import { ApprovalRevocations, ReleaseApprovals, ReleaseDecisionHistory, ReleaseDetail } from './delivery';
import { PortfolioBuild } from './portfolio-build';
import { PortfolioStudy } from './portfolio-study';
import { ReleaseCreate } from './release-create';
import { isForecastAlphaVersion, isForecastCandidate, isForecastCandidateDetail, isForecastMandate, isForecastRelease } from './producer-views';

// The real views, tables, queries and buttons render here. Only the portal shell
// is inlined: Ant Design Drawers intentionally render no contents during SSR.
// Browser click, keyboard, Close/Back and download behavior remain browser checks.
vi.mock('antd', async () => {
  const actual = await vi.importActual<typeof import('antd')>('antd');
  return { ...actual, Drawer: ({ open, title, children }: DrawerProps) => open
    ? createElement('section', { 'aria-label': typeof title === 'string' ? title : undefined }, children) : null };
});
const writes = () => { throw new Error('Read-only views must not submit business commands'); };
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
    expect(html).not.toMatch(/<form\b|type="submit"|请求封存评估|新建组合配置|请求组合构建|请求组合 Study|冻结目标包|审批此目标包|人工拒绝与重新考虑|登记 Offer|撤销审批/);
    for (const method of ['POST', 'PUT', 'PATCH', 'DELETE'] as const) expect(api[method]).not.toHaveBeenCalled();
    return html;
  } finally { client.clear(); }
}
const close = () => {};
const time = '2026-09-01T00:00:00Z';
const empty = { schema_version: 1, items: [], next_cursor: null };
const forecastAlpha: Schema['AlphaVersionView'] = {
  id: 'forecast-alpha', alpha_id: 'alpha', project_id: 'project', version: '1', experiment_id: 'experiment', root_lineage_id: 'lineage',
  code_artifact_id: 'code', model_artifact_id: 'model', runtime_image_ref: 'frozen-image', signal_contract_version: '1',
  signal_kind: 'SCORE', forecast_unit: 'UNITLESS_SCORE', horizon_kind: 'VARIABLE_INTERVAL', horizon_value: null,
  calibration_id: null, origin: null, created_at: time,
};
const strategyAlpha: Schema['StrategyAlphaVersionV1'] = {
  schema_version: 1, output_kind: 'TARGET_WEIGHT', id: 'strategy-alpha', alpha_id: 'alpha', project_id: 'project', version: '2',
  experiment_id: 'experiment', root_lineage_id: 'lineage', created_at: time,
  policy: { schema_version: 1, output_kind: 'TARGET_WEIGHT',
    source: { experiment_id: 'experiment', evaluation_run_id: 'source-run', accepted_attempt_id: 'source-attempt', report_artifact_id: 'source-report' },
    code_artifact_id: 'code', model_artifact_id: 'model', parameter_artifact_id: 'parameters', dataset_revision_id: 'dataset',
    feature_artifact_ids: ['feature'], feature_schema: [], instrument_id: 'BTCUSDT.BINANCE', base_currency: 'USDT', model_abi: 'wasm',
    initialization: { source_fold_index: 2, first_ordinal: 10, first_event_ns: '1788220800000000000', first_decision_ns: '1788220801000000000' },
    target_ttl_ns: '60000000000', runtime_image_ref: 'frozen-image' },
};
const constraints: Schema['PortfolioConstraintsV1'] = {
  schema_version: 1, long_only: true, min_asset_weight: '0', max_asset_weight: '1', min_cash_weight: '0', max_cash_weight: '1',
  min_net_exposure: '0', max_net_exposure: '1', max_gross_exposure: '1', max_turnover_per_rebalance: '1',
  asset_overrides: [], group_bounds: [], transaction_costs_ref: 'costs',
};
const strategyMandate: Schema['StrategyMandateViewV1'] = {
  schema_version: 1, id: 'strategy-mandate', project_id: 'project', version: 2, created_at: time,
  content: { schema_version: 1, allocation_method: 'FIXED_TARGET_WEIGHTS', base_currency: 'USDT', capital_assumption: '1000',
    constraints, execution_assumptions_id: 'costs', exposure_tolerance: '0.0001', max_input_age_seconds: 60, target_ttl_seconds: 60,
    universe_version_id: 'universe' },
};
const forecastMandate: Schema['MandateViewV1'] = {
  id: 'forecast-mandate', project_id: 'project', version: 1, created_at: time,
  content: { base_currency: 'USDT', capital_assumption: '1000', constraints, execution_assumptions_id: 'costs', exposure_tolerance: '0.0001',
    universe_version_id: 'universe', objective: 'MIN_RISK', risk_measure: 'VARIANCE', required_evaluation_policy_id: 'policy',
    rebalance_schedule: { schema_version: 1, kind: 'FIXED_INTERVAL', interval_seconds: 60, calendar_ref: null, session_offset_seconds: null,
      max_input_age_seconds: 60, target_ttl_seconds: 60, timezone: 'UTC' },
    alpha_ensemble: { schema_version: 1, adapter_kind: 'FIXED_WEIGHTED_FORECAST', upstream_class: 'ndarray::ArrayBase::dot', upstream_version: '0.17.1', parameters: {} },
    covariance_estimator: { schema_version: 1, adapter_kind: 'SAMPLE_COVARIANCE', upstream_class: 'ndarray_stats::CorrelationExt::cov', upstream_version: '0.7.0', parameters: { ddof: 1 } },
    optimizer: { schema_version: 1, adapter_kind: 'CLARABEL_QP', upstream_class: 'clarabel::solver::DefaultSolver', upstream_version: '0.11.1',
      parameters: { schema_version: 1, accept_inaccurate: false, max_iterations: 100, risk_aversion: '1', solver_tolerance: '0.0001' } },
  },
};
const forecastCandidate: Schema['CandidateViewV1'] = {
  id: 'forecast-candidate', project_id: 'project', mandate_id: forecastMandate.id, input_set_id: 'input', run_id: 'legacy-run',
  decision_asof: time, created_at: time, execution_status: 'SUCCEEDED', evidence_status: 'VALID', origin: 'REAL',
  solver_status: 'OPTIMAL', current_weights_source: 'NONE', diagnostics_artifact_id: 'diagnostics', target_artifact_id: 'target', cash_weight: null,
};
const forecastDetail: Schema['CandidateDetailV1'] = { header: forecastCandidate,
  members: [{ alpha_version_id: forecastAlpha.id, qualification_id: 'qualification', ensemble_weight: '1', forecast_unit: 'UNITLESS_SCORE', coverage_fraction: '1', calibration_id: null }],
  targets: [{ instrument_id: 'BTCUSDT.BINANCE', currency: 'USDT', target_weight: '0.25', asof: time, valid_until: time }],
};
const strategyCandidate: Schema['StrategyPortfolioCandidateV1'] = {
  schema_version: 1, source_kind: 'STRATEGY_ALPHA', id: 'strategy-candidate', project_id: 'project', mandate_id: strategyMandate.id,
  input_set_id: 'input', run_id: 'run', accepted_attempt_id: 'attempt', report_artifact_id: 'report', allocation_method: 'FIXED_TARGET_WEIGHTS',
  purpose: { purpose: 'HISTORICAL_REPLAY' }, cash_weight: '0.75', decision_asof: time, created_at: time,
  input_provenance: { dataset_revision_id: 'dataset', market_data_origin: 'FIXTURE', pit_status: 'UNVERIFIED', revision_policy: 'AS_KNOWN_THEN', feature_artifact_origins: { feature: 'FIXTURE' } },
  members: [{ alpha_version_id: strategyAlpha.id, ensemble_weight: '1' }], targets: [{ instrument_id: 'BTCUSDT.BINANCE', currency: 'USDT', weight: '0.25' }],
};
const currentCandidate: Schema['StrategyPortfolioCandidateV1'] = { ...strategyCandidate, id: 'current-candidate',
  purpose: { purpose: 'CURRENT_DECISION',
    account_start: { downstream_id: 'downstream', trader_id: 'TRADER-001', account_id: 'PAPER-001', base_currency: 'USDT', starting_capital: '1000', execution_assumptions_id: 'costs' },
    member_inputs: [{ alpha_version_id: strategyAlpha.id, feature_artifact_ids: ['current-feature'] }],
  },
};
const evaluation: Schema['EvaluationView'] = {
  id: 'evaluation', project_id: 'project', subject_candidate_id: forecastCandidate.id, subject_alpha_version_id: null,
  input_set_id: 'input', policy_id: 'policy', run_id: 'run', evaluation_kind: 'FORWARD', execution_status: 'SUCCEEDED', evidence_status: 'VALID', decision: 'PASS',
  report_artifact_id: 'evaluation-report', method_versions_artifact_id: 'methods', origin: 'REAL', concluded_at: time, checked_at: time,
  valid_until: time, unexpired_at_read: true,
};
const forecastRelease: Schema['ReleaseViewV1'] = {
  id: 'forecast-release', project_id: 'project', candidate_id: forecastCandidate.id, mandate_id: forecastMandate.id, evaluation_id: evaluation.id,
  package_artifact_id: 'forecast-package', package_schema_version: '1', market_capability_version: '1', environment: 'REAL',
  asof: time, valid_from: time, valid_until: time, created_at: time,
};
const strategyRelease: Schema['StrategyReleaseViewV1'] = {
  schema_version: 1, id: 'strategy-release', project_id: 'project', candidate_id: currentCandidate.id, mandate_id: strategyMandate.id,
  package_artifact_id: 'strategy-package', package_schema_version: '2', market_capability_version: '1', source_kind: 'NATIVE_TARGET_DECISION',
  execution_environment: 'PAPER', source: { run_id: 'run', accepted_attempt_id: 'attempt', report_artifact_id: 'report', alpha_version_ids: [strategyAlpha.id], input_provenance: strategyCandidate.input_provenance },
  asof: time, valid_from: time, valid_until: time, created_at: time,
};

describe('producer envelopes remain distinct read-only records', () => {
  it('narrows native shapes without inventing legacy fields or replacing missing values', () => {
    expect(isForecastAlphaVersion(forecastAlpha)).toBe(true); expect(isForecastAlphaVersion(strategyAlpha)).toBe(false);
    expect(isForecastMandate(forecastMandate)).toBe(true); expect(isForecastMandate(strategyMandate)).toBe(false);
    expect(isForecastCandidate(forecastCandidate)).toBe(true); expect(isForecastCandidate(strategyCandidate)).toBe(false);
    expect(isForecastCandidateDetail(forecastDetail)).toBe(true); expect(isForecastCandidateDetail(strategyCandidate)).toBe(false);
    expect(isForecastRelease(forecastRelease)).toBe(true); expect(isForecastRelease(strategyRelease)).toBe(false);
    expect(forecastAlpha.calibration_id).toBeNull(); expect(forecastCandidate.cash_weight).toBeNull();
    expect(strategyCandidate).not.toHaveProperty('execution_status'); expect(strategyRelease).not.toHaveProperty('evaluation_id');
  });
  it.each([forecastAlpha, strategyAlpha])('shows original Alpha $id without evaluation commands', version => {
    const html = render(<VersionDetail alpha="alpha" project="project" number={version.version} expectedId={version.id} />, [
      [['alpha-version', 'alpha', version.version, 'project', version.id], version],
      [['alpha-evaluations', version.id, 'project', undefined], empty],
    ]);
    expect(html).toContain(version.id); expect(html).toContain('frozen-image');
    if (isForecastAlphaVersion(version)) {
      expect(html).toContain('UNITLESS_SCORE'); expect(html).toContain('未登记校准'); expect(html).toContain('查看原资格历史');
    } else { expect(html).toContain('TARGET_WEIGHT'); expect(html).toContain('source-report'); expect(html).not.toContain('UNITLESS_SCORE'); }
  });
  it('keeps both mandate kinds in the list and preserves complete configuration details', () => {
    const list = render(<Mandates project="project" />, [[['mandates', 'project', undefined], { ...empty, items: [forecastMandate, strategyMandate] }]]);
    expect(list).toContain('刷新配置'); expect(list).toContain('MIN_RISK'); expect(list).toContain('FIXED_TARGET_WEIGHTS');
    for (const mandate of [forecastMandate, strategyMandate]) {
      const html = render(<MandateDetail id={mandate.id} project="project" close={close} />, [[['mandate', mandate.id, 'project'], mandate]]);
      expect(html).toContain(mandate.id); expect(html).toContain('服务器保存的完整配置');
    }
  });
  it('retains forecast targets and qualifications alongside distinct native target weights', () => {
    const list = render(<Candidates project="project" />, [[['portfolio-candidates', 'project', undefined], { ...empty, items: [forecastCandidate, strategyCandidate] }]]);
    expect(list).toContain('刷新候选'); expect(list).toContain('SUCCEEDED'); expect(list).toContain('原生记录未提供');
    const legacy = render(<CandidateDetail id={forecastCandidate.id} project="project" close={close} />, [
      [['portfolio-candidate', 'project', forecastCandidate.id], forecastDetail],
      [['candidate-evaluations', 'project', forecastCandidate.id, undefined], { ...empty, items: [{ ...evaluation, evaluation_kind: 'PORTFOLIO' }] }],
    ]);
    expect(legacy).toContain('qualification'); expect(legacy).toContain('0.25'); expect(legacy).toContain('未生成'); expect(legacy).toContain('已发表的候选研究评估');
    for (const candidate of [strategyCandidate, currentCandidate]) {
      const native = render(<CandidateDetail id={candidate.id} project="project" close={close} />, [[['portfolio-candidate', 'project', candidate.id], candidate]]);
      expect(native).toContain('0.25'); expect(native).toContain('0.75'); expect(native).toContain(candidate.purpose.purpose);
      expect(native).toContain('UNVERIFIED'); expect(native).not.toContain('qualification'); expect(native).not.toContain('预测单位');
    }
  });
  it.each([forecastRelease, strategyRelease])('keeps source and download access for $id without approval or offer commands', release => {
    const html = render(<ReleaseDetail id={release.id} project="project" close={close} />, [[['release', 'project', release.id], release]]);
    expect(html).toContain(release.package_artifact_id); expect(html).toContain('下载原始目标包');
    if (isForecastRelease(release)) expect(html).toContain(evaluation.id);
    else { expect(html).toContain('NATIVE_TARGET_DECISION'); expect(html).toContain('PAPER'); expect(html).toContain('FIXTURE'); expect(html).not.toContain('原审批历史'); }
  });
  it('keeps original authorization history readable after removing its write dialogs', () => {
    const approval: Schema['ApprovalViewV1'] = { id: 'approval', project_id: 'project', candidate_id: forecastCandidate.id, release_id: forecastRelease.id,
      authority_kind: 'OPERATOR', downstream_id: 'downstream', environment: 'PAPER', evidence_set_id: 'evidence', created_at: time, granted_at: time, valid_until: time };
    const approvals = render(<ReleaseApprovals release={forecastRelease} />, [[['release-approvals', forecastRelease.id, undefined], { ...empty, items: [approval] }]]);
    expect(approvals).toContain('刷新审批历史'); expect(approvals).toContain('OPERATOR');
    const decision: Schema['ReleaseDecisionViewV1'] = { id: 'decision', project_id: 'project', candidate_id: forecastCandidate.id, release_id: forecastRelease.id,
      downstream_id: 'downstream', environment: 'PAPER', decision: 'REJECT', ordinal: 1, reason_code: 'REVIEW', reason: 'Original review reason',
      decided_by: 'OPERATOR', created_at: time, decided_at: time };
    const decisions = render(<ReleaseDecisionHistory release={forecastRelease} />, [[['release-decisions', forecastRelease.id, undefined], { ...empty, items: [decision] }]]);
    expect(decisions).toContain('刷新决定历史'); expect(decisions).toContain('Original review reason');
    const revocations = render(<ApprovalRevocations id="approval" />, [[['approval-revocations', 'approval', undefined], { ...empty,
      items: [{ id: 'revocation', approval_id: 'approval', effective_at: time, created_at: time, reason_code: 'RESEARCH_CHANGED', reason: 'Original revocation reason' } satisfies Schema['ApprovalRevocationViewV1']] }]]);
    expect(revocations).toContain('Original revocation reason');
  });
  it('turns historical Build, Study and Freeze component entrypoints into original read views', () => {
    const build = render(<PortfolioBuild mandate={forecastMandate} close={close} />, [[['mandate', forecastMandate.id, 'project'], forecastMandate]]);
    expect(build).toContain(forecastMandate.id);
    const study = render(<PortfolioStudy candidate={forecastCandidate} close={close} />, [
      [['portfolio-candidate', 'project', forecastCandidate.id], forecastDetail], [['candidate-evaluations', 'project', forecastCandidate.id, undefined], empty],
    ]);
    expect(study).toContain(forecastCandidate.id);
    const freeze = render(<ReleaseCreate project="project" candidate={forecastCandidate.id} evaluation={evaluation} close={close} />, [
      [['evaluation', evaluation.id, { id: forecastCandidate.id, project: 'project' }, undefined], evaluation],
      [['evaluation-metrics', evaluation.id, undefined], empty],
    ]);
    expect(freeze).toContain(evaluation.report_artifact_id);
  });
});
