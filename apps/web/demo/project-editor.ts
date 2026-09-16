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
const validKey = (key?: string): key is string => !!key && key.length <= 200 && key.trim() === key && /^[\x20-\x7e]+$/.test(key);
const validState = ajv.compile({ $ref: 'native#/components/schemas/RunState' });
const validId = ajv.compile({ $ref: 'native#/components/schemas/Id' });
const validLimit = ajv.compile(document.paths['/api/v2/projects'].get.parameters.find(parameter => parameter.name === 'limit')!.schema);
const empty = { status: 200, value: { schema_version: 1, items: [], next_cursor: null } };

// Local presentation state only; never reaches a native project or issues authority.
export function projectEditor() {
  const original = structuredClone(records.get(`/api/v2/projects/${id(1)}`)!.value) as Schema['ProjectView'];
  const projects = new Map([[original.id, original]]);
  const originalBrief = structuredClone(records.get(`/api/v2/briefs/${id(10)}`)!.value) as Schema['BriefView'];
  const briefs = new Map([[originalBrief.id, originalBrief]]);
  const receipts = new Map<string, { body: unknown; resource: Schema['ProjectView'] | Schema['BriefView']; status: number }>();
  return (method: string, path: string, body?: unknown, key?: string, query = new URLSearchParams()) => {
    const denied = demoResponse(method === 'GET' ? 'PATCH' : method, path);
    const invalid = { status: 422, value: { ...denied.value as Schema['Problem'], status: 422, code: 'VALIDATION_ERROR', detail: '请求或幂等键不符合原生合同。' } };
    const parts = path.split('/');
    const project = parts[3] === 'projects' ? projects.get(parts[4] ?? '') : undefined;
    const brief = parts[3] === 'briefs' ? briefs.get(parts[4] ?? '') : undefined;
    const briefPage = project && parts.length === 6 && parts[5] === 'briefs';
    if (method === 'GET') {
      const selected = query.get('project_id');
      const nestedPage = project && project.id !== original.id && parts.length === 6 && ['briefs', 'cycles', 'execution-assumptions', 'portfolio-mandates', 'portfolio-candidates', 'releases', 'handoffs', 'automation-policies', 'forward', 'forward-observations', 'forward-weight-snapshots', 'wakes'].includes(parts[5]!);
      const globalPage = ['/api/v2/alphas', '/api/v2/artifacts', '/api/v2/evaluation-policies', '/api/v2/experiments', '/api/v2/input-sets', '/api/v2/runs'].includes(path);
      const limit = Number(query.get('limit') ?? '50'); const cursor = query.get('cursor');
      if (globalPage && path !== '/api/v2/runs' && selected === null) return invalid;
      if (path === '/api/v2/projects' || briefPage || nestedPage || globalPage) {
        const allowed = ['limit', 'cursor', ...(globalPage ? ['project_id', ...(path === '/api/v2/runs' ? ['state'] : [])] : [])];
        if ((selected !== null && !validId(selected)) || !/^\d+$/.test(query.get('limit') ?? '50') || !validLimit(limit) || (cursor !== null && !validId(cursor)) || (query.has('state') && !validState(query.get('state'))) || [...query.keys()].some(name => !allowed.includes(name) || query.getAll(name).length !== 1)) return invalid;
      }
      const paginate = <T extends { id: string }>(rows: T[]) => {
        const ascending = path === '/api/v2/runs';
        const items = rows.filter(item => !cursor || (ascending ? item.id > cursor : item.id < cursor))
          .sort((a, b) => ascending ? a.id.localeCompare(b.id) : b.id.localeCompare(a.id));
        return { status: 200, value: { schema_version: 1, items: items.slice(0, limit), next_cursor: items.length > limit ? items[limit - 1]!.id : null } };
      };
      if (path === '/api/v2/projects') return paginate([...projects.values()]);
      if (briefPage) return paginate([...briefs.values()].filter(item => item.project_id === project.id));
      if (globalPage) {
        if (path !== '/api/v2/runs' && !projects.has(selected!)) return demoResponse('GET', `/api/v2/projects/${selected}`);
        const rows = (records.get(path)?.value as { items: { id: string; project_id: string; state?: string }[] } | undefined)?.items ?? [];
        return paginate(rows.filter(item => (!selected || item.project_id === selected) && (!query.has('state') || item.state === query.get('state'))));
      }
      if (brief && parts.length === 5) return { status: 200, value: brief };
      if (project && parts.length === 5) return { status: 200, value: project };
      if (nestedPage) return empty;
      return undefined;
    }
    const creatingBrief = method === 'POST' && parts[3] === 'projects' && parts.length === 6 && parts[5] === 'briefs';
    if (creatingBrief || (method === 'PATCH' && parts[3] === 'briefs' && parts.length === 5)) {
      if (!validId(parts[4])) return invalid;
      const request = creatingBrief ? validBriefCreate(body) ? body : undefined : validBriefUpdate(body) ? body : undefined;
      if (!request || !validKey(key) || bindingListError(request.bindings)) return invalid;
      let content: Schema['BriefContentV1'];
      try { content = briefContent(request.content); } catch { return invalid; }
      if ([content.hypothesis, content.economic_rationale].some(text => !text.trim() || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/u.test(text))) return invalid;
      const encoded = [path, content, request.bindings, 'expected_revision' in request ? request.expected_revision : request.supersedes_id ?? null];
      const receiptKey = `brief:${method}:${key}`;
      const previous = receipts.get(receiptKey);
      if (previous) return isDeepStrictEqual(previous.body, encoded)
        ? { status: previous.status, value: { schema_version: 1, resource: previous.resource, replayed: true } }
        : { status: 409, value: { ...denied.value as Schema['Problem'], status: 409, code: 'IDEMPOTENCY_CONFLICT', detail: '此幂等键已用于不同请求，不能重用。' } };
      if (creatingBrief ? !project : !brief) return demoResponse('GET', path);
      const projectId = creatingBrief ? project!.id : brief!.project_id;
      // This offline scene owns only the original project's declared references.
      if (projectId !== originalBrief.project_id) return denied;
      for (const field of ['universe_version_id', 'evaluation_policy_id', 'execution_assumptions_id', 'base_currency', 'benchmark_ref'] as const) {
        if (content[field] !== originalBrief.content[field]) return invalid;
      }
      if (request.bindings.some(binding => !originalBrief.bindings.some(original => original.dataset_revision_id === binding.dataset_revision_id && original.role === binding.role)
        || (binding.role === 'SEALED' ? binding.access_policy === 'RESEARCH_READ' : binding.access_policy === 'EVALUATOR_ONLY'))) return invalid;
      if ('expected_revision' in request) {
        if (request.expected_revision !== brief!.revision) return { status: 409, value: { ...denied.value as Schema['Problem'], status: 409, code: 'REVISION_CONFLICT', current_revision: brief!.revision, detail: 'Brief 草稿已修改，请重新读取。' } };
        if (brief!.state !== 'DRAFT') return denied;
      } else if (request.supersedes_id != null && briefs.get(request.supersedes_id)?.project_id !== projectId) return invalid;
      if (receipts.size >= 256) return denied;
      const now = new Date().toISOString();
      const resource: Schema['BriefView'] = creatingBrief ? {
        id: id(2000 + briefs.size), project_id: projectId, version: briefs.size + 1, revision: '1', state: 'DRAFT',
        content, bindings: structuredClone(request.bindings), supersedes_id: (request as Schema['BriefCreate']).supersedes_id ?? null,
        frozen_at: null, created_at: now, updated_at: now,
      } : { ...brief!, content, bindings: structuredClone(request.bindings), revision: String(BigInt(brief!.revision) + 1n), updated_at: now };
      briefs.set(resource.id, resource);
      const status = creatingBrief ? 201 : 200;
      receipts.set(receiptKey, { body: structuredClone(encoded), resource, status });
      return { status, value: { schema_version: 1, resource, replayed: false } };
    }
    const creating = method === 'POST' && path === '/api/v2/projects';
    if (!creating && !(method === 'PATCH' && project && parts.length === 5)) return undefined;
    const request = creating ? validCreate(body) ? body : undefined : validUpdate(body) ? body : undefined;
    if (!request || !request.name.trim() || /[\u0000-\u001f\u007f-\u009f]/u.test(request.name) || /[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f-\u009f]/u.test(request.description) || !validKey(key)) return invalid;
    const encoded = 'expected_revision' in request
      ? [path, request.schema_version, request.expected_revision, request.name, request.description, request.state]
      : [path, request.schema_version, request.name, request.description, request.fork_from_project_id ?? null];
    const receiptKey = `${method}:${key}`;
    const previous = receipts.get(receiptKey);
    if (previous) return isDeepStrictEqual(previous.body, encoded)
      ? { status: previous.status, value: { schema_version: 1, resource: previous.resource, replayed: true } }
      : { status: 409, value: { ...denied.value as Schema['Problem'], status: 409, code: 'IDEMPOTENCY_CONFLICT', detail: '此幂等键已用于不同请求，不能重用。' } };
    if (!('expected_revision' in request)) {
      if (request.fork_from_project_id != null) return denied;
    } else {
      if (!projectStateOptions(project!, briefs.get(project!.current_brief_id ?? '')).some(option => option.value === request.state)) return denied;
      if (request.expected_revision !== project!.revision) return { status: 409, value: {
        ...denied.value as Schema['Problem'], status: 409, code: 'REVISION_CONFLICT', current_revision: project!.revision,
        detail: '合成项目已修改，请重新读取。',
      } };
    }
    if (receipts.size >= 256) return denied;
    const now = new Date().toISOString(); const newId = id(1000 + projects.size);
    const resource: Schema['ProjectView'] = creating ? {
      id: newId, root_lineage_id: newId, name: request.name, description: request.description, state: 'DRAFT',
      current_brief_id: null, current_automation_policy_id: null, created_by: 'OPERATOR', archived_at: null,
      created_at: now, updated_at: now, revision: '1',
    } : { ...project!, name: request.name, description: request.description, state: (request as Schema['ProjectUpdate']).state, archived_at: (request as Schema['ProjectUpdate']).state === 'ARCHIVED' ? project!.archived_at ?? now : null, revision: String(BigInt(project!.revision) + 1n), updated_at: now };
    projects.set(resource.id, resource);
    const status = creating ? 201 : 200;
    receipts.set(receiptKey, { body: encoded, resource, status });
    return { status, value: { schema_version: 1, resource, replayed: false } };
  };
}
