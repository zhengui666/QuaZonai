// These tests exercise only the in-memory synthetic preview, never native research.
import { expect, test } from 'vitest';
import { projectEditor } from '../demo/project-editor';
import { id } from '../demo/records';
import type { Schema } from './api';
import { validateResponse } from './generated/responses.cjs';

function frozenFixture() {
  const edit = projectEditor();
  const projectPath = `/api/v2/projects/${id(1)}`;
  const original = edit('GET', `/api/v2/briefs/${id(10)}`)!.value as Schema['BriefView'];
  const saved = edit('POST', `${projectPath}/briefs`, {
    schema_version: 1, content: original.content, bindings: original.bindings, supersedes_id: original.id,
  }, 'create')!;
  expect(saved.status).toBe(201);
  const draft = (saved.value as { resource: Schema['BriefView'] }).resource;
  const runtime = (edit('GET', '/api/v2/integrations/runtimes')!.value as { items: Schema['RuntimeView'][] }).items[0]!;
  const inputs = (edit('GET', '/api/v2/input-sets', undefined, undefined,
    new URLSearchParams({ project_id: original.project_id }))!.value as { items: Schema['InputSetSummary'][] }).items;
  const freeze: Schema['BriefFreezeV1'] = {
    schema_version: 1, expected_revision: draft.revision,
    execution_context: {
      schema_version: 1, runtime_id: runtime.id, runtime_revision: runtime.revision,
      discovery_input_set_id: inputs.find(item => item.purpose === 'DISCOVERY')!.id,
      validation_input_set_id: inputs.find(item => item.purpose === 'VALIDATION')!.id,
      sealed_input_set_id: inputs.find(item => item.purpose === 'SEALED')!.id,
    },
  };
  const freezePath = `/api/v2/briefs/${draft.id}/freeze`;
  const first = edit('POST', freezePath, freeze, 'freeze')!;
  expect(first.status).toBe(200);
  expect(validateResponse('/api/v2/briefs/{id}/freeze', 'post', 200, first.value, 'application/json')).toBe(true);
  const frozen = (first.value as { resource: Schema['FrozenBriefV1'] }).resource;
  return { edit, projectPath, freeze, freezePath, frozen };
}

function setProjectState(edit: ReturnType<typeof projectEditor>, projectPath: string, state: Schema['ProjectState']) {
  const project = edit('GET', projectPath)!.value as Schema['ProjectView'];
  const changed = edit('PATCH', projectPath, {
    schema_version: 1, expected_revision: project.revision, name: project.name, description: project.description, state,
  }, `state-${state}`)!;
  expect(changed.status).toBe(200);
  return (changed.value as { resource: Schema['ProjectView'] }).resource;
}

test('synthetic freeze retries return the original receipt before frozen or archived guards', () => {
  const { edit, projectPath, freeze, freezePath, frozen } = frozenFixture();
  const reordered = { execution_context: Object.fromEntries(Object.entries(freeze.execution_context).reverse()),
    expected_revision: freeze.expected_revision, schema_version: 1 };
  for (const archived of [false, true]) {
    if (archived) setProjectState(edit, projectPath, 'ARCHIVED');
    const before = structuredClone(edit('GET', projectPath));
    const replay = edit('POST', freezePath, reordered, 'freeze')!;
    expect(replay).toEqual({ status: 200, value: { schema_version: 1, resource: frozen, replayed: true } });
    expect(validateResponse('/api/v2/briefs/{id}/freeze', 'post', 200, replay.value, 'application/json')).toBe(true);
    expect(edit('GET', projectPath)).toEqual(before);
    expect(edit('GET', `/api/v2/briefs/${frozen.brief.id}/execution-context`)?.value).toEqual(frozen);
    expect(edit('POST', freezePath, freeze, `fresh-${archived}`)?.status).not.toBe(200);
  }
});

test.each(['revision', 'context', 'path'] as const)('synthetic freeze rejects changed %s under the original key', field => {
  const { edit, projectPath, freeze, freezePath } = frozenFixture();
  const before = structuredClone(edit('GET', projectPath));
  const changed = field === 'revision' ? { ...freeze, expected_revision: '99' }
    : field === 'context' ? { ...freeze, execution_context: { ...freeze.execution_context, runtime_id: id(999) } } : freeze;
  const path = field === 'path' ? `/api/v2/briefs/${id(10)}/freeze` : freezePath;
  const rejected = edit('POST', path, changed, 'freeze')!;
  expect(rejected).toMatchObject({ status: 409, value: { code: 'IDEMPOTENCY_CONFLICT' } });
  expect(validateResponse('/api/v2/briefs/{id}/freeze', 'post', 409, rejected.value, 'application/problem+json')).toBe(true);
  expect(edit('GET', projectPath)).toEqual(before);
});

test.each(['PAUSED', 'ARCHIVED'] as const)('synthetic cycle retries survive a later %s project without duplicating work', state => {
  const { edit, projectPath, frozen } = frozenFixture();
  const active = setProjectState(edit, projectPath, 'ACTIVE');
  const profiles = (edit('GET', '/api/v2/settings/codex')!.value as { items: Schema['CodexProfileViewV1'][] }).items;
  const start: Schema['CycleStartV1'] = {
    schema_version: 1, brief_id: frozen.brief.id, expected_revision: active.revision,
    researcher_profile: { profile_id: profiles[0]!.id, expected_revision: profiles[0]!.revision },
    reviewer_profile: { profile_id: profiles[1]!.id, expected_revision: profiles[1]!.revision },
  };
  const cyclePath = `${projectPath}/cycles`;
  const first = edit('POST', cyclePath, start, 'start')!;
  expect(first.status).toBe(202);
  expect(validateResponse('/api/v2/projects/{id}/cycles', 'post', 202, first.value, 'application/json')).toBe(true);
  const started = (first.value as { resource: Schema['CycleStartedV1'] }).resource;
  expect(started.cycle.brief_id).toBe(frozen.brief.id);
  expect(started.run.cycle_id).toBe(started.cycle.id);
  setProjectState(edit, projectPath, state);
  const beforeProject = structuredClone(edit('GET', projectPath));
  const beforeCycles = structuredClone(edit('GET', cyclePath));
  const beforeRuns = structuredClone(edit('GET', '/api/v2/runs'));
  expect(edit('POST', cyclePath, Object.fromEntries(Object.entries(start).reverse()), 'start')).toEqual({
    status: 202, value: { schema_version: 1, resource: started, replayed: true },
  });
  expect(edit('POST', cyclePath, { ...start, expected_revision: '99' }, 'start')).toMatchObject({
    status: 409, value: { code: 'IDEMPOTENCY_CONFLICT' },
  });
  expect(edit('POST', cyclePath, start, 'fresh')?.status).not.toBe(202);
  expect(edit('GET', projectPath)).toEqual(beforeProject);
  expect(edit('GET', cyclePath)).toEqual(beforeCycles);
  expect(edit('GET', '/api/v2/runs')).toEqual(beforeRuns);
});

test('synthetic flow receipts do not bypass request schema or key validation', () => {
  const { edit, projectPath, freeze, freezePath } = frozenFixture();
  const before = structuredClone(edit('GET', projectPath));
  expect(edit('POST', freezePath, {}, 'freeze')?.status).toBe(422);
  for (const key of [undefined, '', ' trailing ', 'x'.repeat(201)]) {
    expect(edit('POST', freezePath, freeze, key)?.status).toBe(422);
  }
  expect(edit('GET', projectPath)).toEqual(before);
});
