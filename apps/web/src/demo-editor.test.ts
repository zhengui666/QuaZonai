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
  for (const key of ['', 'x'.repeat(201), ' a', 'a ', 'a\tb', '中文', 'a\x7f']) expect(edit('PATCH', path, body, key)?.status).toBe(422);
  expect(projectEditor()('PATCH', path, body, 'a b')?.status).toBe(200);
});
