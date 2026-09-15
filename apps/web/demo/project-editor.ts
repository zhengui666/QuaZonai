import Ajv2020 from 'ajv/dist/2020.js';
import addFormats from 'ajv-formats';
import document from '../../../contracts/generated/api-v2.openapi.json';
import type { Schema } from '../src/api';
import { demoResponse, id, records } from './records';

const ajv = new Ajv2020({ strict: false, inlineRefs: false });
addFormats(ajv); ajv.addSchema(document, 'native');
const valid = ajv.compile<Schema['ProjectUpdate']>({ $ref: 'native#/components/schemas/ProjectUpdate' });

// Local presentation state only; never activates a project or issues authority.
export function projectEditor() {
  let project = structuredClone(records.get(`/api/v2/projects/${id(1)}`)!.value) as Schema['ProjectView'];
  const receipts = new Map<string, { body: string; resource: Schema['ProjectView'] }>();
  return (method: string, path: string, body?: unknown, key?: string) => {
    if (method === 'GET' && path === `/api/v2/projects/${project.id}`) return { status: 200, value: project };
    if (method === 'GET' && path === '/api/v2/projects') return { status: 200, value: { schema_version: 1, items: [project], next_cursor: null } };
    if (method !== 'PATCH' || path !== `/api/v2/projects/${project.id}`) return undefined;
    const denied = demoResponse('PATCH', path);
    if (!valid(body) || !key || key.length > 200 || key.trim() !== key || !/^[\x20-\x7e]+$/.test(key)) return { status: 422, value: { ...denied.value as Schema['Problem'], status: 422, code: 'VALIDATION_ERROR', detail: '请求或幂等键不符合原生合同。' } };
    if (body.state !== project.state) return denied;
    const encoded = JSON.stringify([body.schema_version, body.expected_revision, body.name, body.description, body.state]);
    const previous = receipts.get(key);
    if (previous) return previous.body === encoded
      ? { status: 200, value: { schema_version: 1, resource: previous.resource, replayed: true } } : { status: 409, value: { ...denied.value as Schema['Problem'], status: 409, code: 'IDEMPOTENCY_CONFLICT', detail: '此幂等键已用于不同请求，不能重用。' } };
    if (body.expected_revision !== project.revision) return { status: 409, value: {
      ...denied.value as Schema['Problem'], status: 409, code: 'REVISION_CONFLICT', current_revision: project.revision,
      detail: '合成项目已修改，请重新读取。',
    } };
    if (receipts.size >= 256) return denied;
    project = { ...project, name: body.name, description: body.description, revision: String(BigInt(project.revision) + 1n), updated_at: new Date().toISOString() };
    receipts.set(key, { body: encoded, resource: project });
    return { status: 200, value: { schema_version: 1, resource: project, replayed: false } };
  };
}
