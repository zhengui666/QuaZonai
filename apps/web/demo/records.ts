// Fixed SYNTHETIC presentation records. No database, account, jobs or delivery.
import type { Schema } from '../src/api';
import briefInput from '../../../tests/contracts/research-brief.json';

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
    input_set_id: id(21), policy_id: id(90), run_id: run.id, evaluation_kind: 'WALK_FORWARD',
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

// No fake qualification, approval, Claim or account is issued by this preview.
for (const suffix of ['cycles', 'portfolio-mandates', 'portfolio-candidates', 'releases', 'handoffs', 'automation-policies']) {
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
