import { expect, test } from 'vitest';
import document from '../../../contracts/generated/api-v2.openapi.json';
import { projectEditor } from '../demo/project-editor';
import { id } from '../demo/records';
import { validateResponse } from './generated/responses.cjs';

test('synthetic metadata editing retains receipts and cannot deliver', () => {
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
  expect(edit('PATCH', path, { ...body, state: 'ACTIVE' }, 'four')?.status).toBe(409);
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
  expect(edit('PATCH', path, { ...body, expected_revision: '2', state: 'ACTIVE' }, 'fresh')?.status).toBe(200);
  expect(edit('GET', path)).toMatchObject({ value: { revision: '3', state: 'ACTIVE' } });
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


test('all temporary project pages validate pagination and text follows native control rules', () => {
  const edit = projectEditor(); const path = '/api/v2/projects';
  const body = { schema_version: 1, name: 'Temporary', description: 'Line 1\nLine 2\tColumn\rReturn' };
  for (const name of ['A\nB', 'A\tB', 'A\rB', 'A\u007fB', 'A\u0085B']) expect(edit('POST', path, { ...body, name }, 'invalid-name')?.status).toBe(422);
  for (const description of ['A\u0000B', 'A\u000bB', 'A\u007fB', 'A\u009fB']) expect(edit('POST', path, { ...body, description }, 'invalid-text')?.status).toBe(422);
  const created = edit('POST', path, body, 'valid-text')!;
  expect(created.status).toBe(201);
  const project = (created.value as { resource: { id: string } }).resource.id;
  const update = { ...body, expected_revision: '1', state: 'DRAFT' };
  expect(edit('PATCH', `${path}/${project}`, { ...update, name: 'A\nB' }, 'invalid-update')?.status).toBe(422);
  expect(edit('PATCH', `${path}/${project}`, { ...update, description: 'A\u0000B' }, 'invalid-update')?.status).toBe(422);
  for (const suffix of ['briefs', 'cycles', 'execution-assumptions', 'portfolio-mandates', 'portfolio-candidates', 'releases', 'handoffs', 'automation-policies', 'forward', 'forward-observations', 'forward-weight-snapshots', 'wakes']) {
    const originalRoute = `${path}/${id(1)}/${suffix}`;
    for (const query of ['limit=0', 'limit=101', 'cursor=bad', 'cursor=', 'limit=1&limit=2', 'unknown=1']) expect(edit('GET', originalRoute, undefined, undefined, new URLSearchParams(query))?.status).toBe(422);
    const originalPage = edit('GET', originalRoute)!;
    expect(validateResponse(`/api/v2/projects/{id}/${suffix}`, 'get', 200, originalPage.value, 'application/json')).toBe(true);
    const originalRows = (originalPage.value as { items: { id: string }[] }).items;
    expect(edit('GET', originalRoute, undefined, undefined, new URLSearchParams('limit=1'))).toMatchObject({ value: { items: originalRows.slice(0, 1), next_cursor: originalRows.length > 1 ? originalRows[0]!.id : null } });
    if (originalRows.length > 1) expect(edit('GET', originalRoute, undefined, undefined, new URLSearchParams({ cursor: originalRows[0]!.id }))).toMatchObject({ value: { items: originalRows.slice(1) } });
    const route = `${path}/${project}/${suffix}`;
    const page = edit('GET', route)!;
    expect(page).toMatchObject({ status: 200, value: { items: [], next_cursor: null } });
    expect(validateResponse(`/api/v2/projects/{id}/${suffix}`, 'get', 200, page.value, 'application/json')).toBe(true);
    for (const query of ['limit=0', 'limit=101', 'cursor=bad', 'cursor=', 'limit=1&limit=2', 'unknown=1']) expect(edit('GET', route, undefined, undefined, new URLSearchParams(query))?.status).toBe(422);
  }
  const globalRoutes = Object.entries(document.paths).filter(([path, operations]) => !path.includes('{') && 'get' in operations && 'parameters' in operations.get && operations.get.parameters.some(parameter => parameter.in === 'query' && parameter.name === 'project_id')).map(([path]) => path);
  expect(globalRoutes).toEqual(expect.arrayContaining(['/api/v2/artifacts', '/api/v2/experiments', '/api/v2/input-sets']));
  for (const route of globalRoutes) {
    for (const query of ['project_id=bad', 'project_id=', `project_id=${id(1)}&project_id=${project}`, `project_id=${id(1)}&limit=0`, 'limit=0']) {
      const rejected = edit('GET', route, undefined, undefined, new URLSearchParams(query))!;
      expect(rejected.status).toBe(422);
      expect(validateResponse(route, 'get', 422, rejected.value, 'application/problem+json')).toBe(true);
    }
    const original = edit('GET', route, undefined, undefined, new URLSearchParams({ project_id: id(1) }))!;
    expect(original.status).toBe(200);
    expect(validateResponse(route, 'get', 200, original.value, 'application/json')).toBe(true);
    const rows = (original.value as { items: { id: string; project_id: string }[] }).items;
    expect(rows.every(item => item.project_id === id(1))).toBe(true);
    const missing = edit('GET', route, undefined, undefined, new URLSearchParams({ project_id: id(99999) }))!;
    expect(missing.status).toBe(route === '/api/v2/runs' ? 200 : 404);
    expect(validateResponse(route, 'get', missing.status, missing.value, missing.status === 200 ? 'application/json' : 'application/problem+json')).toBe(true);
    if (route === '/api/v2/runs') expect(missing).toMatchObject({ value: { items: [] } });
    expect(rows.map(item => item.id)).toEqual(rows.map(item => item.id).sort((a, b) => route === '/api/v2/runs' ? a.localeCompare(b) : b.localeCompare(a)));
    expect(edit('GET', route)?.status).toBe(route === '/api/v2/runs' ? 200 : 422);
    const first = edit('GET', route, undefined, undefined, new URLSearchParams({ project_id: id(1), limit: '1' }))!;
    expect(first).toMatchObject({ value: { items: rows.slice(0, 1), next_cursor: rows.length > 1 ? rows[0]!.id : null } });
    if (rows.length > 1) expect(edit('GET', route, undefined, undefined, new URLSearchParams({ project_id: id(1), cursor: rows[0]!.id }))).toMatchObject({ value: { items: rows.slice(1) } });
    const page = edit('GET', route, undefined, undefined, new URLSearchParams({ project_id: project }))!;
    expect(page).toMatchObject({ status: 200, value: { items: [], next_cursor: null } });
    expect(validateResponse(route, 'get', 200, page.value, 'application/json')).toBe(true);
    for (const query of ['limit=0', 'cursor=bad', 'limit=1&limit=2', 'unknown=1']) expect(edit('GET', route, undefined, undefined, new URLSearchParams(`project_id=${project}&${query}`))?.status).toBe(422);
  }
  expect(edit('GET', '/api/v2/runs', undefined, undefined, new URLSearchParams({ project_id: id(1), state: 'QUEUED' }))).toMatchObject({ value: { items: [] } });
  expect(edit('GET', '/api/v2/runs', undefined, undefined, new URLSearchParams({ project_id: project, state: 'SUCCEEDED' }))?.status).toBe(200);
  expect(edit('GET', '/api/v2/runs', undefined, undefined, new URLSearchParams({ project_id: project, state: 'invalid' }))?.status).toBe(422);
});


test('Brief drafts retain frozen history, references and idempotent revisions', () => {
  const edit = projectEditor(); const path = `/api/v2/projects/${id(1)}/briefs`;
  const frozen = structuredClone(edit('GET', `/api/v2/briefs/${id(10)}`)!.value) as import('./api').Schema['BriefView'];
  const body = { schema_version: 1, content: frozen.content, bindings: frozen.bindings, supersedes_id: frozen.id };
  const saved = edit('POST', path, body, 'brief-create')!;
  expect(saved.status).toBe(201);
  expect(validateResponse('/api/v2/projects/{id}/briefs', 'post', 201, saved.value, 'application/json')).toBe(true);
  const draft = (saved.value as { resource: import('./api').Schema['BriefView'] }).resource;
  expect(draft).toMatchObject({ version: 2, revision: '1', state: 'DRAFT', frozen_at: null, supersedes_id: frozen.id });
  expect(draft.id).not.toBe(frozen.id);
  const reordered = { ...body, content: Object.fromEntries(Object.entries(body.content).reverse()) };
  expect(edit('POST', path, reordered, 'brief-create')).toMatchObject({ value: { replayed: true, resource: draft } });
  const changed = { ...body.content, hypothesis: 'Edited hypothesis' };
  expect(edit('POST', path, { ...body, content: changed }, 'brief-create')?.status).toBe(409);
  const update = { schema_version: 1, content: changed, bindings: body.bindings, expected_revision: '1' };
  for (const [method, route, request] of [['POST', path, body], ['PATCH', `/api/v2/briefs/${draft.id}`, update]] as const) {
    const probe = projectEditor();
    probe('POST', path, body, 'setup');
    const key = `reference-receipt-${method}`;
    expect(probe(method, route, request, key)?.status).toBe(method === 'POST' ? 201 : 200);
    for (const altered of [
      { ...request, content: { ...request.content, evaluation_policy_id: id(999) } },
      { ...request, bindings: request.bindings.map((binding, index) => ({ ...binding, dataset_revision_id: id(997 + index) })) },
    ]) expect(probe(method, route, altered, key)).toMatchObject({ status: 409, value: { code: 'IDEMPOTENCY_CONFLICT' } });
    const malformed = method === 'POST' ? '/api/v2/projects/bad/briefs' : '/api/v2/briefs/bad';
    const missing = method === 'POST' ? `/api/v2/projects/${id(999)}/briefs` : `/api/v2/briefs/${id(999)}`;
    expect(probe(method, malformed, request, 'path')?.status).toBe(422);
    expect(probe(method, missing, request, 'path')?.status).toBe(404);
  }

  expect(edit('PATCH', `/api/v2/briefs/${draft.id}`, update, 'brief-update')).toMatchObject({ status: 200, value: { resource: { revision: '2', content: { hypothesis: changed.hypothesis } } } });
  expect(edit('POST', path, body, 'brief-create')).toMatchObject({ value: { resource: { revision: '1', content: frozen.content } } });
  expect(edit('PATCH', `/api/v2/briefs/${draft.id}`, update, 'stale')?.status).toBe(409);
  expect(edit('PATCH', `/api/v2/briefs/${frozen.id}`, update, 'frozen')?.status).toBe(403);
  expect(edit('GET', `/api/v2/briefs/${frozen.id}`)?.value).toEqual(frozen);
  expect(edit('POST', `/api/v2/briefs/${draft.id}/freeze`, {}, 'freeze')).toBeUndefined();
  expect(edit('GET', `/api/v2/projects/${id(1)}`)).toMatchObject({ value: { current_brief_id: frozen.id, state: 'DRAFT' } });
  const page = edit('GET', path, undefined, undefined, new URLSearchParams('limit=1'))!;
  expect(validateResponse('/api/v2/projects/{id}/briefs', 'get', 200, page.value, 'application/json')).toBe(true);
  expect(page).toMatchObject({ value: { items: [{ id: draft.id }], next_cursor: draft.id } });
  expect(edit('GET', path, undefined, undefined, new URLSearchParams({ cursor: draft.id }))).toMatchObject({ value: { items: [frozen] } });
  expect(edit('GET', path, undefined, undefined, new URLSearchParams('limit=0'))?.status).toBe(422);
  for (const invalid of [
    { ...body, supersedes_id: id(999) },
    { ...body, content: { ...body.content, hypothesis: ' ' } },
    { ...body, content: { ...body.content, hypothesis: 'a\u0000b' } },
    { ...body, content: { ...body.content, evaluation_policy_id: id(999) } },
    { ...body, content: { ...body.content, budget: { ...body.content.budget, max_repair_turns: 100 } } },
    { ...body, bindings: [body.bindings[0], body.bindings[0]] },
    { ...body, bindings: body.bindings.map(binding => ({ ...binding, access_policy: 'RESEARCH_READ' })) },
  ]) expect(edit('POST', path, invalid, 'invalid')?.status).toBe(422);
  const project = edit('POST', '/api/v2/projects', { schema_version: 1, name: 'New', description: '' }, 'project')!;
  const projectId = (project.value as { resource: { id: string } }).resource.id;
  expect(edit('POST', `/api/v2/projects/${projectId}/briefs`, body, 'foreign')?.status).toBe(403);
  expect(projectEditor()('GET', `/api/v2/briefs/${draft.id}`)).toBeUndefined();
});


test('synthetic project transitions retain exact revisions and irreversible archive state', () => {
  const edit = projectEditor(); const path = `/api/v2/projects/${id(1)}`;
  const original = edit('GET', path)!.value as { name: string; description: string };
  let revision = 1;
  for (const state of ['ACTIVE', 'PAUSED', 'ARCHIVED']) {
    const body = { schema_version: 1, expected_revision: String(revision), name: original.name, description: original.description, state };
    const response = edit('PATCH', path, body, state)!;
    expect(response.status).toBe(200);
    expect(validateResponse('/api/v2/projects/{id}', 'patch', 200, response.value, 'application/json')).toBe(true);
    expect(edit('PATCH', path, body, state)).toMatchObject({ value: { replayed: true } });
    expect(edit('GET', path)).toMatchObject({ value: { state, revision: String(++revision) } });
  }
  const archived = edit('GET', path)!.value as { archived_at: string };
  expect(archived.archived_at).not.toBeNull();
  for (const state of ['DRAFT', 'ACTIVE', 'PAUSED']) {
    expect(edit('PATCH', path, { schema_version: 1, expected_revision: '1', name: original.name, description: original.description, state }, `stale-${state}`)).toMatchObject({ status: 409, value: { code: 'REVISION_CONFLICT', current_revision: String(revision) } });
  }
  expect(edit('GET', path)?.value).toEqual(archived);
  expect(edit('PATCH', path, { schema_version: 1, expected_revision: String(revision), name: original.name, description: original.description, state: 'DRAFT' }, 'reopen')?.status).toBe(403);
});


test('archived synthetic projects refuse Brief writes while preserving original receipts', () => {
  const edit = projectEditor(); const projectPath = `/api/v2/projects/${id(1)}`;
  const briefPath = `${projectPath}/briefs`;
  const frozen = edit('GET', `/api/v2/briefs/${id(10)}`)!.value as import('./api').Schema['BriefView'];
  const create = { schema_version: 1, content: frozen.content, bindings: frozen.bindings, supersedes_id: frozen.id };
  const saved = edit('POST', briefPath, create, 'create')!;
  const draft = (saved.value as { resource: import('./api').Schema['BriefView'] }).resource;
  const update = { schema_version: 1, expected_revision: draft.revision, content: frozen.content, bindings: frozen.bindings };
  const updated = edit('PATCH', `/api/v2/briefs/${draft.id}`, update, 'update')!;
  expect(updated.status).toBe(200);
  const project = edit('GET', projectPath)!.value as import('./api').Schema['ProjectView'];
  expect(edit('PATCH', projectPath, { schema_version: 1, expected_revision: project.revision, name: project.name, description: project.description, state: 'ARCHIVED' }, 'archive')?.status).toBe(200);
  expect(edit('POST', briefPath, create, 'new')?.status).toBe(403);
  expect(edit('PATCH', `/api/v2/briefs/${draft.id}`, { ...update, expected_revision: '2' }, 'new')?.status).toBe(403);
  expect(edit('POST', briefPath, create, 'create')).toMatchObject({ value: { replayed: true, resource: draft } });
  expect(edit('PATCH', `/api/v2/briefs/${draft.id}`, update, 'update')).toMatchObject({ value: { replayed: true, resource: (updated.value as { resource: unknown }).resource } });
  expect(edit('GET', `/api/v2/briefs/${draft.id}`)?.value).toEqual((updated.value as { resource: unknown }).resource);
  expect(edit('GET', `/api/v2/briefs/${frozen.id}`)?.value).toEqual(frozen);
});
