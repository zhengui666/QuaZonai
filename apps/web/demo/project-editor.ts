import { isDeepStrictEqual } from 'node:util';
import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import document from '../../../contracts/generated/api-v2.openapi.json';
import type { Schema } from '../src/api';
import { briefContent } from '../src/brief-fields';
import { bindingListError } from '../src/authoring-constraints';
import { projectStateOptions } from '../src/authoring-options';
import { demoResponse, id, records } from './records';

const ajv = new Ajv2020({ strict: false, inlineRefs: false });
addFormats(ajv); ajv.addSchema(document, 'native');
const validUpdate = ajv.compile<Schema['ProjectUpdate']>({ $ref: 'native#/components/schemas/ProjectUpdate' });
const validCreate = ajv.compile<Schema['ProjectCreate']>({ $ref: 'native#/components/schemas/ProjectCreate' });
const validBriefCreate = ajv.compile<Schema['BriefCreate']>({ $ref: 'native#/components/schemas/BriefCreate' });
const validBriefUpdate = ajv.compile<Schema['BriefUpdate']>({ $ref: 'native#/components/schemas/BriefUpdate' });
const validBriefFreeze = ajv.compile<Schema['BriefFreezeV1']>({ $ref: 'native#/components/schemas/BriefFreezeV1' });
const validCycleStart = ajv.compile<Schema['CycleStartV1']>({ $ref: 'native#/components/schemas/CycleStartV1' });
const validKey = (key?: string): key is string => !!key && key.length <= 200 && key.trim() === key && /^[\x20-\x7e]+$/.test(key);
const validState = ajv.compile({ $ref: 'native#/components/schemas/RunState' });
const validId = ajv.compile({ $ref: 'native#/components/schemas/Id' });
const validLimit = ajv.compile(document.paths['/api/v2/projects'].get.parameters.find(parameter => parameter.name === 'limit')!.schema);

const stamp = '2026-09-17T00:00:00Z';
const demoRuntime: Schema['RuntimeView'] = {
  id: id(2980), revision: '1', protocol_version: 1, credential_configured: false, ca_configured: false,
  last_capability_snapshot_artifact_id: null, created_at: stamp, updated_at: stamp,
  configuration: {
    name: 'SYNTHETIC · Demo Runtime', endpoint: 'https://synthetic.invalid', tls_policy: 'SYSTEM_CA',
    enabled: true, development_http: false,
    allowed_capabilities: ['DATA_VALIDATE', 'ALPHA_EVALUATE', 'PORTFOLIO_BUILD', 'PORTFOLIO_SIMULATE'],
  },
};
const demoProfiles: Schema['CodexProfileViewV1'][] = [2981, 2982].map((tail, index) => ({
  id: id(tail), name: index === 0 ? 'SYNTHETIC · Demo Researcher' : 'SYNTHETIC · Demo Reviewer',
  home_binding: `synthetic-demo-${tail}`, profile_origin: 'OPERATOR_MOUNT', connection_mode: 'SYSTEM', custom_base_url: null,
  credential_configured: false, model_settings: {
    schema_version: 1, use_default_model_settings: true, saved_model: null, saved_reasoning_effort: null, saved_fast_mode: false,
  },
  revision: '1', created_at: stamp, updated_at: stamp,
}));

type FlowReceipt = { body: unknown; resource: unknown; status: number };

// Local presentation state only; never reaches a native project, account, Runtime or delivery target.
export function projectEditor() {
  const original = structuredClone(records.get(`/api/v2/projects/${id(1)}`)!.value) as Schema['ProjectView'];
  const projects = new Map([[original.id, original]]);
  const originalBrief = structuredClone(records.get(`/api/v2/briefs/${id(10)}`)!.value) as Schema['BriefView'];
  const briefs = new Map([[originalBrief.id, originalBrief]]);
  // The original frozen context remains historical, including its now-disabled Runtime.
  const contexts = new Map<string, Schema['BriefExecutionContextV1']>();
  const historicalCycles = structuredClone(records.get(`/api/v2/projects/${original.id}/cycles`)!.value) as { items: Schema['CycleViewV1'][] };
  const cycles = new Map(historicalCycles.items.map(item => [item.id, item]));
  const historicalRuntimes = structuredClone(records.get('/api/v2/integrations/runtimes')!.value) as { items: Schema['RuntimeView'][] };
  const dynamicRuns = new Map<string, Schema['RunSnapshotV1']>();
  const receipts = new Map<string, { body: unknown; resource: Schema['ProjectView'] | Schema['BriefView']; status: number }>();
  const flowReceipts = new Map<string, FlowReceipt>();
  const baseRun = structuredClone(records.get(`/api/v2/runs/${id(20)}`)!.value) as Schema['RunSnapshotV1'];
  const inputPage = structuredClone(records.get('/api/v2/input-sets')!.value) as { items: Schema['InputSetSummary'][] };
  const originalInputs = new Map(inputPage.items.map(item => [item.purpose, item.id]));
  const denied = (method: string, path: string) => demoResponse(method === 'GET' ? 'PATCH' : method, path);
  const invalid = (method: string, path: string) => ({ status: 422, value: { ...denied(method, path).value as Schema['Problem'], status: 422, code: 'VALIDATION_ERROR', detail: '请求或幂等键不符合原生合同。' } });
  const conflict = (method: string, path: string, detail: string) => ({ status: 409, value: { ...denied(method, path).value as Schema['Problem'], status: 409, code: 'REVISION_CONFLICT', detail } });
  const increment = (value: string) => String(BigInt(value) + 1n);
  const replay = (map: Map<string, FlowReceipt>, receiptKey: string, encoded: unknown, method: string, path: string) => {
    const previous = map.get(receiptKey);
    if (!previous) return undefined;
    return isDeepStrictEqual(previous.body, encoded)
      ? { status: previous.status, value: { schema_version: 1, resource: previous.resource, replayed: true } }
      : { status: 409, value: { ...denied(method, path).value as Schema['Problem'], status: 409, code: 'IDEMPOTENCY_CONFLICT', detail: '此幂等键已用于不同请求，不能重用。' } };
  };

  return (method: string, path: string, body?: unknown, key?: string, query = new URLSearchParams()) => {
    const parts = path.split('/');
    const project = parts[3] === 'projects' ? projects.get(parts[4] ?? '') : undefined;
    const brief = parts[3] === 'briefs' ? briefs.get(parts[4] ?? '') : undefined;
    const briefPage = project && parts.length === 6 && parts[5] === 'briefs';

    if (method === 'GET') {
      const runtimePage = path === '/api/v2/integrations/runtimes';
      const profilePage = path === '/api/v2/settings/codex';
      if (path === `/api/v2/integrations/runtimes/${demoRuntime.id}`) return { status: 200, value: demoRuntime };
      if (path === `/api/v2/integrations/runtimes/${demoRuntime.id}/readiness`) return { status: 200, value: {
        schema_version: 1, runtime_id: demoRuntime.id, integration_revision: demoRuntime.revision,
        state: 'NOT_CHECKED', available_job_kinds: [], latest_observation: null,
      } satisfies Schema['RuntimeReadinessV1'] };
      const profile = demoProfiles.find(item => path === `/api/v2/settings/codex/${item.id}`);
      if (profile) return { status: 200, value: profile };
      if (parts[3] === 'cycles' && parts.length === 5) {
        const cycle = cycles.get(parts[4] ?? '');
        if (cycle) return { status: 200, value: cycle };
      }
      if (parts[3] === 'runs' && parts.length === 5) {
        const run = dynamicRuns.get(parts[4] ?? '');
        if (run) return { status: 200, value: run };
      }
      if (parts[3] === 'runs' && parts.length === 6 && parts[5] === 'rebalance' && dynamicRuns.has(parts[4] ?? '')) {
        return { status: 200, value: { schema_version: 1, rebalance: null } satisfies Schema['RunRebalanceViewV1'] };
      }
      if (brief && parts.length === 6 && parts[5] === 'execution-context') {
        const context = contexts.get(brief.id);
        if (context) return { status: 200, value: { schema_version: 1, brief, execution_context: context } satisfies Schema['FrozenBriefV1'] };
      }
      const selected = query.get('project_id');
      const nestedPage = project && parts.length === 6 && ['briefs', 'cycles', 'execution-assumptions', 'portfolio-mandates', 'portfolio-candidates', 'releases', 'handoffs', 'automation-policies', 'forward', 'forward-observations', 'forward-weight-snapshots', 'wakes'].includes(parts[5]!);
      const globalPage = ['/api/v2/alphas', '/api/v2/artifacts', '/api/v2/evaluation-policies', '/api/v2/experiments', '/api/v2/input-sets', '/api/v2/runs'].includes(path);
      const limit = Number(query.get('limit') ?? '50'); const cursor = query.get('cursor');
      if (globalPage && path !== '/api/v2/runs' && selected === null) return invalid(method, path);
      if (path === '/api/v2/projects' || briefPage || nestedPage || globalPage || runtimePage || profilePage) {
        const allowed = ['limit', 'cursor', ...(globalPage ? ['project_id', ...(path === '/api/v2/runs' ? ['state'] : [])] : [])];
        if ((selected !== null && !validId(selected)) || !/^\d+$/.test(query.get('limit') ?? '50') || !validLimit(limit) || (cursor !== null && !validId(cursor)) || (query.has('state') && !validState(query.get('state'))) || [...query.keys()].some(name => !allowed.includes(name) || query.getAll(name).length !== 1)) return invalid(method, path);
      }
      const paginate = <T extends { id: string }>(rows: T[]) => {
        const ascending = path === '/api/v2/runs';
        const items = rows.filter(item => !cursor || (ascending ? item.id > cursor : item.id < cursor))
          .sort((a, b) => ascending ? a.id.localeCompare(b.id) : b.id.localeCompare(a.id));
        return { status: 200, value: { schema_version: 1, items: items.slice(0, limit), next_cursor: items.length > limit ? items[limit - 1]!.id : null } };
      };
      if (runtimePage) return paginate([...historicalRuntimes.items, demoRuntime]);
      if (profilePage) return paginate(demoProfiles);
      if (path === '/api/v2/projects') return paginate([...projects.values()]);
      if (briefPage) return paginate([...briefs.values()].filter(item => item.project_id === project.id));
      if (globalPage) {
        if (path !== '/api/v2/runs' && !projects.has(selected!)) return demoResponse('GET', `/api/v2/projects/${selected}`);
        const stored = (records.get(path)?.value as { items: { id: string; project_id: string; state?: string }[] } | undefined)?.items ?? [];
        const rows = path === '/api/v2/runs' ? [...stored, ...dynamicRuns.values()] : stored;
        return paginate(rows.filter(item => (!selected || item.project_id === selected) && (!query.has('state') || item.state === query.get('state'))));
      }
      if (brief && parts.length === 5) return { status: 200, value: brief };
      if (project && parts.length === 5) return { status: 200, value: project };
      if (nestedPage) {
        const stored = project.id === original.id ? (records.get(path)?.value as { items: { id: string }[] } | undefined)?.items ?? [] : [];
        const rows = parts[5] === 'cycles' ? [...cycles.values()].filter(item => item.project_id === project.id) : stored;
        return paginate(rows);
      }
      return undefined;
    }

    if (method === 'POST' && parts[3] === 'briefs' && parts.length === 6 && parts[5] === 'freeze') {
      if (!validId(parts[4]) || !validBriefFreeze(body) || !validKey(key)) return invalid(method, path);
      const encoded = [path, body]; const receiptKey = `freeze:${key}`;
      // A lost acknowledgement remains replayable after freezing or archiving, as in Store.
      const previous = replay(flowReceipts, receiptKey, encoded, method, path); if (previous) return previous;
      if (!brief || brief.state !== 'DRAFT') return invalid(method, path);
      if (body.expected_revision !== brief.revision) return conflict(method, path, 'Brief 草稿已修改，请重新读取。');
      const project = projects.get(brief.project_id);
      if (!project || project.state === 'ARCHIVED' || body.execution_context.runtime_id !== demoRuntime.id || body.execution_context.runtime_revision !== demoRuntime.revision) return denied(method, path);
      const purposes = ['DISCOVERY', 'VALIDATION', 'SEALED'] as const;
      const expected = purposes.map(purpose => originalInputs.get(purpose));
      const actual = [body.execution_context.discovery_input_set_id, body.execution_context.validation_input_set_id, body.execution_context.sealed_input_set_id];
      if (actual.some((value, index) => value !== expected[index])) return invalid(method, path);
      for (const [index, purpose] of purposes.entries()) {
        const input = records.get(`/api/v2/input-sets/${actual[index]}`)?.value as Schema['InputSetView'] | undefined;
        const bound = new Set(brief.bindings.filter(binding => binding.role === purpose).map(binding => binding.dataset_revision_id));
        if (!input || input.header.project_id !== brief.project_id || input.header.purpose !== purpose || bound.size === 0
          || input.items.some(member => member.item.kind !== 'DATASET' || member.item.role !== purpose)) return invalid(method, path);
        const datasets = new Set(input.items.flatMap(member => member.item.kind === 'DATASET' ? [member.item.dataset_revision_id] : []));
        if (!isDeepStrictEqual(bound, datasets)) return invalid(method, path);
      }
      const now = new Date().toISOString();
      const frozen: Schema['BriefView'] = { ...brief, state: 'FROZEN', frozen_at: now, updated_at: now };
      briefs.set(frozen.id, frozen); contexts.set(frozen.id, structuredClone(body.execution_context));
      const current: Schema['ProjectView'] = { ...project, current_brief_id: frozen.id, revision: increment(project.revision), updated_at: now };
      projects.set(current.id, current);
      const resource: Schema['FrozenBriefV1'] = { schema_version: 1, brief: frozen, execution_context: body.execution_context };
      flowReceipts.set(receiptKey, { body: structuredClone(encoded), resource, status: 200 });
      return { status: 200, value: { schema_version: 1, resource, replayed: false } };
    }

    if (method === 'POST' && parts[3] === 'projects' && parts.length === 6 && parts[5] === 'cycles') {
      if (!validId(parts[4]) || !validCycleStart(body) || !validKey(key)) return invalid(method, path);
      const encoded = [path, body]; const receiptKey = `cycle:${key}`;
      // Replaying history must not depend on today's project state or create another Cycle.
      const previous = replay(flowReceipts, receiptKey, encoded, method, path); if (previous) return previous;
      if (!project || project.state !== 'ACTIVE' || body.expected_revision !== project.revision) return invalid(method, path);
      const selectedBrief = briefs.get(body.brief_id);
      if (!selectedBrief || selectedBrief.project_id !== project.id || selectedBrief.state !== 'FROZEN' || !contexts.has(selectedBrief.id)) return denied(method, path);
      const allowedProfiles = new Set(demoProfiles.map(item => item.id));
      if (!allowedProfiles.has(body.researcher_profile.profile_id) || !allowedProfiles.has(body.reviewer_profile.profile_id)
        || body.researcher_profile.expected_revision !== '1' || body.reviewer_profile.expected_revision !== '1') return invalid(method, path);
      const ordinal = Math.max(0, ...[...cycles.values()].filter(item => item.project_id === project.id).map(item => item.ordinal)) + 1;
      const cycleId = id(3200 + cycles.size); const runId = id(3300 + dynamicRuns.size); const now = new Date().toISOString();
      const cycle: Schema['CycleViewV1'] = {
        schema_version: 1, id: cycleId, project_id: project.id, brief_id: selectedBrief.id, ordinal,
        trigger: 'OPERATOR', state: 'COMPLETED', outcome: 'NO_SUPPORTED_CANDIDATE',
        next_action: 'SYNTHETIC · 本周期未执行实验、没有合格候选；双 Alpha、组合与目标包仅为独立历史展示。',
        budget: selectedBrief.content.budget, used_experiments: 0, reserved_experiments: 0, reserved_cpu_seconds: '0',
        initial_run_id: runId, researcher_profile: body.researcher_profile, reviewer_profile: body.reviewer_profile,
        available_actions: ['VIEW_BRIEF', 'VIEW_RUNS'], revision: '1', created_at: now, started_at: now, ended_at: now,
      };
      const run: Schema['RunSnapshotV1'] = {
        ...baseRun, id: runId, project_id: project.id, cycle_id: cycle.id, kind: 'AGENT_RESEARCH', state: 'SUCCEEDED',
        current_attempt_no: 1, active_attempt_id: id(3400 + dynamicRuns.size), terminal_reason_code: 'SYNTHETIC_PRESENTATION_ONLY',
        deadline_at: new Date(Date.parse(now) + selectedBrief.content.budget.max_wall_seconds * 1000).toISOString(),
        queued_at: now, started_at: now, finished_at: now, revision: '1',
      };
      cycles.set(cycle.id, cycle); dynamicRuns.set(run.id, run);
      const resource: Schema['CycleStartedV1'] = { schema_version: 1, cycle, run };
      flowReceipts.set(receiptKey, { body: structuredClone(encoded), resource, status: 202 });
      return { status: 202, value: { schema_version: 1, resource, replayed: false } };
    }

    const creatingBrief = method === 'POST' && parts[3] === 'projects' && parts.length === 6 && parts[5] === 'briefs';
    if (creatingBrief || (method === 'PATCH' && parts[3] === 'briefs' && parts.length === 5)) {
      if (!validId(parts[4])) return invalid(method, path);
      const request = creatingBrief ? validBriefCreate(body) ? body : undefined : validBriefUpdate(body) ? body : undefined;
      if (!request || !validKey(key) || bindingListError(request.bindings)) return invalid(method, path);
      let content: Schema['BriefContentV1'];
      try { content = briefContent(request.content); } catch { return invalid(method, path); }
      if ([content.hypothesis, content.economic_rationale].some(text => !text.trim() || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/u.test(text))) return invalid(method, path);
      const encoded = [path, content, request.bindings, 'expected_revision' in request ? request.expected_revision : request.supersedes_id ?? null];
      const receiptKey = `brief:${method}:${key}`;
      const previous = receipts.get(receiptKey);
      if (previous) return isDeepStrictEqual(previous.body, encoded)
        ? { status: previous.status, value: { schema_version: 1, resource: previous.resource, replayed: true } }
        : { status: 409, value: { ...denied(method, path).value as Schema['Problem'], status: 409, code: 'IDEMPOTENCY_CONFLICT', detail: '此幂等键已用于不同请求，不能重用。' } };
      if (creatingBrief ? !project : !brief) return demoResponse('GET', path);
      const projectId = creatingBrief ? project!.id : brief!.project_id;
      if (projectId !== originalBrief.project_id || projects.get(projectId)?.state === 'ARCHIVED') return denied(method, path);
      for (const field of ['universe_version_id', 'evaluation_policy_id', 'execution_assumptions_id', 'base_currency', 'benchmark_ref'] as const) {
        if (content[field] !== originalBrief.content[field]) return invalid(method, path);
      }
      if (request.bindings.some(binding => !originalBrief.bindings.some(original => original.dataset_revision_id === binding.dataset_revision_id && original.role === binding.role)
        || (binding.role === 'SEALED' ? binding.access_policy === 'RESEARCH_READ' : binding.access_policy === 'EVALUATOR_ONLY'))) return invalid(method, path);
      if ('expected_revision' in request) {
        if (request.expected_revision !== brief!.revision) return { status: 409, value: { ...denied(method, path).value as Schema['Problem'], status: 409, code: 'REVISION_CONFLICT', current_revision: brief!.revision, detail: 'Brief 草稿已修改，请重新读取。' } };
        if (brief!.state !== 'DRAFT') return denied(method, path);
      } else if (request.supersedes_id != null && briefs.get(request.supersedes_id)?.project_id !== projectId) return invalid(method, path);
      if (receipts.size >= 256) return denied(method, path);
      const now = new Date().toISOString();
      const resource: Schema['BriefView'] = creatingBrief ? {
        id: id(2000 + briefs.size), project_id: projectId, version: briefs.size + 1, revision: '1', state: 'DRAFT',
        content, bindings: structuredClone(request.bindings), supersedes_id: (request as Schema['BriefCreate']).supersedes_id ?? null,
        frozen_at: null, created_at: now, updated_at: now,
      } : { ...brief!, content, bindings: structuredClone(request.bindings), revision: increment(brief!.revision), updated_at: now };
      briefs.set(resource.id, resource);
      const status = creatingBrief ? 201 : 200;
      receipts.set(receiptKey, { body: structuredClone(encoded), resource, status });
      return { status, value: { schema_version: 1, resource, replayed: false } };
    }

    const creating = method === 'POST' && path === '/api/v2/projects';
    if (!creating && !(method === 'PATCH' && project && parts.length === 5)) return undefined;
    const request = creating ? validCreate(body) ? body : undefined : validUpdate(body) ? body : undefined;
    if (!request || !request.name.trim() || /[\u0000-\u001f\u007f-\u009f]/u.test(request.name) || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/u.test(request.description) || !validKey(key)) return invalid(method, path);
    const encoded = 'expected_revision' in request
      ? [path, request.schema_version, request.expected_revision, request.name, request.description, request.state]
      : [path, request.schema_version, request.name, request.description, request.fork_from_project_id ?? null];
    const receiptKey = `${method}:${key}`;
    const previous = receipts.get(receiptKey);
    if (previous) return isDeepStrictEqual(previous.body, encoded)
      ? { status: previous.status, value: { schema_version: 1, resource: previous.resource, replayed: true } }
      : { status: 409, value: { ...denied(method, path).value as Schema['Problem'], status: 409, code: 'IDEMPOTENCY_CONFLICT', detail: '此幂等键已用于不同请求，不能重用。' } };
    if (!('expected_revision' in request)) {
      if (request.fork_from_project_id != null) return denied(method, path);
    } else {
      if (request.expected_revision !== project!.revision) return { status: 409, value: {
        ...denied(method, path).value as Schema['Problem'], status: 409, code: 'REVISION_CONFLICT', current_revision: project!.revision,
        detail: '合成项目已修改，请重新读取。',
      } };
      if (!projectStateOptions(project!, briefs.get(project!.current_brief_id ?? '')).some(option => option.value === request.state)) return denied(method, path);
    }
    if (receipts.size >= 256) return denied(method, path);
    const now = new Date().toISOString(); const newId = id(1000 + projects.size);
    const resource: Schema['ProjectView'] = creating ? {
      id: newId, root_lineage_id: newId, name: request.name, description: request.description, state: 'DRAFT',
      current_brief_id: null, current_automation_policy_id: null, created_by: 'OPERATOR', archived_at: null,
      created_at: now, updated_at: now, revision: '1',
    } : { ...project!, name: request.name, description: request.description, state: (request as Schema['ProjectUpdate']).state, archived_at: (request as Schema['ProjectUpdate']).state === 'ARCHIVED' ? project!.archived_at ?? now : null, revision: increment(project!.revision), updated_at: now };
    projects.set(resource.id, resource);
    const status = creating ? 201 : 200;
    receipts.set(receiptKey, { body: encoded, resource, status });
    return { status, value: { schema_version: 1, resource, replayed: false } };
  };
}
