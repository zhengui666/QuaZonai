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
  expect(edit('PATCH', path, { ...body, name: 12 }, 'bad')).toBeUndefined();
  expect(edit('POST', '/api/v2/handoffs', body, 'claim')).toBeUndefined();
  expect(projectEditor()('GET', path)).toMatchObject({ value: { revision: '1' } });
});
