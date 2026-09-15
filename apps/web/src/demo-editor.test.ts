import { expect, test } from 'vitest';
import { projectEditor } from '../demo/project-editor';
import { id } from '../demo/records';
import { validateResponse } from './generated/responses.cjs';

test('synthetic metadata editing retains receipts and cannot activate or deliver', () => {
  const edit = projectEditor(); const path = `/api/v2/projects/${id(1)}`;
  const body = { schema_version: 1, expected_revision: '1', name: 'SYNTHETIC edited', description: 'Temporary demo', state: 'DRAFT' };
  const saved = edit('PATCH', path, body, 'one')!;
  expect(saved.status).toBe(200);
  expect(validateResponse('/api/v2/projects/{id}', 'patch', 200, saved.value, 'application/json')).toBe(true);
  expect(edit('GET', path)).toMatchObject({ value: { name: body.name, revision: '2' } });
  expect(edit('PATCH', path, { ...body, expected_revision: '2', name: 'Second' }, 'two')?.status).toBe(200);
  expect(edit('PATCH', path, body, 'one')).toMatchObject({ value: { replayed: true, resource: { revision: '2' } } });
  expect(edit('GET', path)).toMatchObject({ value: { revision: '3', name: 'Second' } });
  expect(edit('PATCH', path, body, 'three')?.status).toBe(409);
  expect(edit('PATCH', path, { ...body, state: 'ACTIVE' }, 'four')?.status).toBe(403);
  expect(edit('PATCH', path, { ...body, name: 12 }, 'bad')?.status).toBe(422);
  expect(edit('POST', '/api/v2/handoffs', body, 'claim')).toBeUndefined();
  expect(projectEditor()('GET', path)).toMatchObject({ value: { revision: '1' } });
});

test('demo accepts native header bounds and semantically equal request order', () => {
  const edit = projectEditor(); const path = `/api/v2/projects/${id(1)}`;
  const body = { schema_version: 1, expected_revision: '1', name: 'Synthetic', description: '中'.repeat(8000), state: 'DRAFT' };
  const key = 'x'.repeat(200);
  expect(edit('PATCH', path, body, key)?.status).toBe(200);
  expect(edit('PATCH', path, Object.fromEntries(Object.entries(body).reverse()), key)).toMatchObject({ value: { replayed: true } });
  expect(edit('PATCH', path, { ...body, name: 'Different' }, key)).toMatchObject({ status: 409, value: { code: 'IDEMPOTENCY_CONFLICT' } });
  expect(edit('PATCH', path, { ...body, state: 'ACTIVE' }, key)).toMatchObject({ status: 409, value: { code: 'IDEMPOTENCY_CONFLICT' } });
  expect(edit('PATCH', path, { ...body, expected_revision: '2', state: 'ACTIVE' }, 'fresh')?.status).toBe(403);
  expect(edit('GET', path)).toMatchObject({ value: { revision: '2', state: 'DRAFT' } });
  for (const key of ['', 'x'.repeat(201), ' a', 'a ', 'a\tb', '中文', 'a\x7f']) expect(edit('PATCH', path, body, key)?.status).toBe(422);
  expect(projectEditor()('PATCH', path, body, 'a b')?.status).toBe(200);
});

test('temporary projects preserve creation receipts, pagination and empty research boundaries', () => {
  const edit = projectEditor(); const path = '/api/v2/projects';
  const body = { schema_version: 1, name: 'SYNTHETIC new', description: 'Temporary', fork_from_project_id: null };
  const created = edit('POST', path, body, 'create')!;
  expect(created.status).toBe(201);
  expect(validateResponse(path, 'post', 201, created.value, 'application/json')).toBe(true);
  const value = created.value as { resource: { id: string; root_lineage_id: string; revision: string } };
  const project = value.resource.id;
  expect(value.resource).toMatchObject({ root_lineage_id: project, revision: '1', state: 'DRAFT', current_brief_id: null, current_automation_policy_id: null });
  expect(project).not.toBe(id(1));
  expect(edit('POST', path, Object.fromEntries(Object.entries(body).reverse()), 'create')).toMatchObject({ status: 201, value: { replayed: true, resource: value.resource } });
  expect(edit('POST', path, { schema_version: 1, name: body.name, description: body.description }, 'create')).toMatchObject({ status: 201, value: { replayed: true } });
  expect(edit('POST', path, { ...body, name: 'Changed' }, 'create')).toMatchObject({ status: 409, value: { code: 'IDEMPOTENCY_CONFLICT' } });
  expect(edit('POST', path, { ...body, fork_from_project_id: id(1) }, 'fork')?.status).toBe(403);
  expect(edit('POST', path, { ...body, name: ' ' }, 'blank')?.status).toBe(422);
  const first = edit('GET', path, undefined, undefined, new URLSearchParams('limit=1'))!;
  expect(validateResponse(path, 'get', 200, first.value, 'application/json')).toBe(true);
  expect(first).toMatchObject({ value: { items: [{ id: project }], next_cursor: project } });
  expect(edit('GET', path, undefined, undefined, new URLSearchParams({ limit: '1', cursor: project }))).toMatchObject({ value: { items: [{ id: id(1) }], next_cursor: null } });
  for (const query of ['limit=0', 'limit=101', 'cursor=bad', 'limit=1&limit=2', 'unknown=1']) expect(edit('GET', path, undefined, undefined, new URLSearchParams(query))?.status).toBe(422);
  for (const suffix of ['briefs', 'cycles', 'releases', 'forward', 'wakes']) expect(edit('GET', `${path}/${project}/${suffix}`)).toMatchObject({ value: { items: [], next_cursor: null } });
  expect(edit('GET', '/api/v2/alphas', undefined, undefined, new URLSearchParams({ project_id: project }))).toMatchObject({ value: { items: [] } });
  const update = { schema_version: 1, expected_revision: '1', name: 'Edited', description: 'Still temporary', state: 'DRAFT' };
  expect(edit('PATCH', `${path}/${project}`, update, 'create')).toMatchObject({ status: 200, value: { resource: { revision: '2' } } });
  expect(edit('POST', path, body, 'create')).toMatchObject({ value: { resource: { name: body.name, revision: '1' } } });
  expect(edit('PATCH', `${path}/${id(1)}`, update, 'create')).toMatchObject({ status: 409, value: { code: 'IDEMPOTENCY_CONFLICT' } });
  expect(edit('PATCH', `${path}/${project}`, { ...update, expected_revision: '2', state: 'ACTIVE' }, 'activate')?.status).toBe(403);
  expect(projectEditor()('GET', `${path}/${project}`)).toBeUndefined();
});
