import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import document from '../../../contracts/generated/api-v2.openapi.json';
import type { Schema } from '../src/api';
import { demoResponse, id, records } from './records';

const ajv = new Ajv2020({ strict: false, inlineRefs: false });
addFormats(ajv); ajv.addSchema(document, 'native');
const validUpdate = ajv.compile<Schema['ProjectUpdate']>({ $ref: 'native#/components/schemas/ProjectUpdate' });
const validCreate = ajv.compile<Schema['ProjectCreate']>({ $ref: 'native#/components/schemas/ProjectCreate' });
const validCursor = ajv.compile({ $ref: 'native#/components/schemas/Id' });
const validLimit = ajv.compile(document.paths['/api/v2/projects'].get.parameters.find(parameter => parameter.name === 'limit')!.schema);
const empty = { status: 200, value: { schema_version: 1, items: [], next_cursor: null } };

// Local presentation state only; never activates a project or issues authority.
export function projectEditor() {
  const original = structuredClone(records.get(`/api/v2/projects/${id(1)}`)!.value) as Schema['ProjectView'];
  const projects = new Map([[original.id, original]]);
  const receipts = new Map<string, { body: string; resource: Schema['ProjectView']; status: number }>();
  return (method: string, path: string, body?: unknown, key?: string, query = new URLSearchParams()) => {
    const denied = demoResponse(method === 'GET' ? 'PATCH' : method, path);
    const invalid = { status: 422, value: { ...denied.value as Schema['Problem'], status: 422, code: 'VALIDATION_ERROR', detail: '请求或幂等键不符合原生合同。' } };
    const parts = path.split('/');
    const project = parts[3] === 'projects' ? projects.get(parts[4] ?? '') : undefined;
    if (method === 'GET') {
      if (path === '/api/v2/projects') {
        const limit = Number(query.get('limit') ?? '50'); const cursor = query.get('cursor');
        if (!/^\d+$/.test(query.get('limit') ?? '50') || !validLimit(limit) || (cursor !== null && !validCursor(cursor)) || [...query.keys()].some(name => !['limit', 'cursor'].includes(name) || query.getAll(name).length !== 1)) return invalid;
        const items = [...projects.values()].filter(item => !cursor || item.id < cursor).sort((a, b) => b.id.localeCompare(a.id));
        return { status: 200, value: { schema_version: 1, items: items.slice(0, limit), next_cursor: items.length > limit ? items[limit - 1]!.id : null } };
      }
      if (project && parts.length === 5) return { status: 200, value: project };
      if (project && project.id !== original.id && parts.length === 6 && ['briefs', 'cycles', 'execution-assumptions', 'portfolio-mandates', 'portfolio-candidates', 'releases', 'handoffs', 'automation-policies', 'forward', 'forward-observations', 'wakes'].includes(parts[5]!)) return empty;
      const selected = query.get('project_id');
      if (selected && selected !== original.id && projects.has(selected) && ['/api/v2/alphas', '/api/v2/evaluation-policies', '/api/v2/runs'].includes(path)) return empty;
      return undefined;
    }
    const creating = method === 'POST' && path === '/api/v2/projects';
    if (!creating && !(method === 'PATCH' && project && parts.length === 5)) return undefined;
    const request = creating ? validCreate(body) ? body : undefined : validUpdate(body) ? body : undefined;
    if (!request || !request.name.trim() || !key || key.length > 200 || key.trim() !== key || !/^[\x20-\x7e]+$/.test(key)) return invalid;
    const encoded = 'expected_revision' in request
      ? JSON.stringify([path, request.schema_version, request.expected_revision, request.name, request.description, request.state])
      : JSON.stringify([path, request.schema_version, request.name, request.description, request.fork_from_project_id ?? null]);
    const receiptKey = `${method}:${key}`;
    const previous = receipts.get(receiptKey);
    if (previous) return previous.body === encoded
      ? { status: previous.status, value: { schema_version: 1, resource: previous.resource, replayed: true } }
      : { status: 409, value: { ...denied.value as Schema['Problem'], status: 409, code: 'IDEMPOTENCY_CONFLICT', detail: '此幂等键已用于不同请求，不能重用。' } };
    if (!('expected_revision' in request)) {
      if (request.fork_from_project_id != null) return denied;
    } else {
      if (request.state !== project!.state) return denied;
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
    } : { ...project!, name: request.name, description: request.description, revision: String(BigInt(project!.revision) + 1n), updated_at: now };
    projects.set(resource.id, resource);
    const status = creating ? 201 : 200;
    receipts.set(receiptKey, { body: encoded, resource, status });
    return { status, value: { schema_version: 1, resource, replayed: false } };
  };
}
