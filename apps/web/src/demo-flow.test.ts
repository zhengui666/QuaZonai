// These tests exercise only the in-memory synthetic preview, never native research.
import { expect, test, vi } from 'vitest';
import { projectEditor } from '../demo/project-editor';
import { demoResponse, id, records } from '../demo/records';
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
  const runtime = (edit('GET', '/api/v2/integrations/runtimes')!.value as { items: Schema['RuntimeView'][] }).items.find(item => item.configuration.enabled)!;
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
  const beforeProject = edit('GET', projectPath)!.value as Schema['ProjectView'];
  const first = edit('POST', freezePath, freeze, 'freeze')!;
  expect(first.status).toBe(200);
  expect(validateResponse('/api/v2/briefs/{id}/freeze', 'post', 200, first.value, 'application/json')).toBe(true);
  const frozen = (first.value as { resource: Schema['FrozenBriefV1'] }).resource;
  expect(frozen.brief.revision).toBe(String(BigInt(draft.revision) + 1n));
  expect(edit('GET', `/api/v2/briefs/${draft.id}`)?.value).toEqual(frozen.brief);
  expect(edit('GET', projectPath)).toMatchObject({ value: {
    current_brief_id: draft.id, revision: String(BigInt(beforeProject.revision) + 1n),
  } });
  return { edit, projectPath, freeze, freezePath, frozen };
}

function setProjectState(edit: ReturnType<typeof projectEditor>, projectPath: string, state: Schema['ProjectState']) {
  const project = edit('GET', projectPath)!.value as Schema['ProjectView'];
  const changed = edit('PATCH', projectPath, {
    schema_version: 1, expected_revision: project.revision, name: project.name, description: project.description, state,
  }, `state-${state}-${project.revision}`)!;
  expect(changed.status).toBe(200);
  return (changed.value as { resource: Schema['ProjectView'] }).resource;
}

function startRequest(edit: ReturnType<typeof projectEditor>, project: Schema['ProjectView'], briefId: string): Schema['CycleStartV1'] {
  const profiles = (edit('GET', '/api/v2/settings/codex')!.value as { items: Schema['CodexProfileViewV1'][] }).items;
  const researcher = profiles.find(item => item.name === 'SYNTHETIC · Demo Researcher')!;
  const reviewer = profiles.find(item => item.name === 'SYNTHETIC · Demo Reviewer')!;
  return {
    schema_version: 1, brief_id: briefId, expected_revision: project.revision,
    researcher_profile: { profile_id: researcher.id, expected_revision: researcher.revision },
    reviewer_profile: { profile_id: reviewer.id, expected_revision: reviewer.revision },
  };
}

test('synthetic freeze retries return the original receipt before frozen or archived guards', () => {
  const { edit, projectPath, freeze, freezePath, frozen } = frozenFixture();
  const reordered = { execution_context: Object.fromEntries(Object.entries(freeze.execution_context).reverse()),
    expected_revision: freeze.expected_revision, schema_version: 1 };
  const stale = edit('POST', freezePath, freeze, 'fresh-old-revision')!;
  expect(stale).toMatchObject({ status: 409, value: { code: 'REVISION_CONFLICT', current_revision: frozen.brief.revision } });
  expect(validateResponse('/api/v2/briefs/{id}/freeze', 'post', 409, stale.value, 'application/problem+json')).toBe(true);
  for (const archived of [false, true]) {
    if (archived) setProjectState(edit, projectPath, 'ARCHIVED');
    const before = structuredClone(edit('GET', projectPath));
    const replay = edit('POST', freezePath, reordered, 'freeze')!;
    expect(replay).toEqual({ status: 200, value: { schema_version: 1, resource: frozen, replayed: true } });
    expect(validateResponse('/api/v2/briefs/{id}/freeze', 'post', 200, replay.value, 'application/json')).toBe(true);
    expect(edit('GET', projectPath)).toEqual(before);
    expect(edit('GET', `/api/v2/briefs/${frozen.brief.id}/execution-context`)?.value).toEqual(frozen);
    expect(edit('POST', freezePath, { ...freeze, expected_revision: frozen.brief.revision }, `fresh-${archived}`)?.status).not.toBe(200);
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
  const start = startRequest(edit, active, frozen.brief.id);
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

test('empty bindings are rejected at draft creation and update without changing existing records', () => {
  const { edit, projectPath, frozen } = frozenFixture();
  const collection = `${projectPath}/briefs`;
  const create = { schema_version: 1, content: frozen.brief.content, bindings: frozen.brief.bindings, supersedes_id: frozen.brief.id };
  const created = edit('POST', collection, create, 'valid-draft')!;
  expect(created.status).toBe(201);
  const draft = (created.value as { resource: Schema['BriefView'] }).resource;
  const path = `/api/v2/briefs/${draft.id}`;
  const beforeProject = structuredClone(edit('GET', projectPath));
  const beforeBriefs = structuredClone(edit('GET', collection));
  for (const [method, route, contract, request] of [
    ['POST', collection, '/api/v2/projects/{id}/briefs', { ...create, bindings: [] }],
    ['PATCH', path, '/api/v2/briefs/{id}', { schema_version: 1, expected_revision: draft.revision, content: draft.content, bindings: [] }],
  ] as const) {
    const rejected = edit(method, route, request, `empty-${method}`)!;
    expect(rejected).toMatchObject({ status: 422, value: { code: 'VALIDATION_ERROR' } });
    expect(validateResponse(contract, method.toLowerCase(), 422, rejected.value, 'application/problem+json')).toBe(true);
    expect(edit('GET', projectPath)).toEqual(beforeProject);
    expect(edit('GET', collection)).toEqual(beforeBriefs);
    expect(edit('GET', path)?.value).toEqual(draft);
  }
});

test.each(['DISCOVERY', 'VALIDATION', 'SEALED'] as const)('Demo refuses a freeze with missing %s bindings without changing the project or draft', missing => {
  const { edit, projectPath, freeze, frozen } = frozenFixture();
  const created = edit('POST', `${projectPath}/briefs`, {
    schema_version: 1, content: frozen.brief.content, supersedes_id: frozen.brief.id,
    bindings: frozen.brief.bindings.filter(binding => binding.role !== missing),
  }, 'incomplete-draft')!;
  expect(created.status).toBe(201);
  const draft = (created.value as { resource: Schema['BriefView'] }).resource;
  const path = `/api/v2/briefs/${draft.id}`;
  const before = structuredClone(edit('GET', projectPath));
  const rejected = edit('POST', `${path}/freeze`, { ...freeze, expected_revision: draft.revision }, 'incomplete-freeze')!;
  expect(rejected).toMatchObject({ status: 422, value: { code: 'VALIDATION_ERROR' } });
  expect(validateResponse('/api/v2/briefs/{id}/freeze', 'post', 422, rejected.value, 'application/problem+json')).toBe(true);
  expect(edit('GET', projectPath)).toEqual(before);
  expect(edit('GET', path)?.value).toEqual(draft);
  expect(edit('GET', `${path}/execution-context`) ?? demoResponse('GET', `${path}/execution-context`)).toMatchObject({ status: 404 });
  const repaired = edit('PATCH', path, {
    schema_version: 1, expected_revision: draft.revision, content: draft.content, bindings: [...frozen.brief.bindings].reverse(),
  }, 'repair-bindings')!;
  expect(repaired.status).toBe(200);
  const revision = (repaired.value as { resource: Schema['BriefView'] }).resource.revision;
  // Rejected commands did not publish a receipt; an explicitly corrected request can succeed.
  expect(edit('POST', `${path}/freeze`, { ...freeze, expected_revision: revision }, 'incomplete-freeze')).toMatchObject({
    status: 200, value: { replayed: false, resource: { brief: { id: draft.id, revision: String(BigInt(revision) + 1n), state: 'FROZEN' } } },
  });
});

test('new Demo Cycles preserve unique ordinals and record actions without granting candidates or changing history', () => {
  const { edit, projectPath, frozen } = frozenFixture();
  const history = structuredClone([...records]);
  const historicalCycles = (edit('GET', `${projectPath}/cycles`)!.value as { items: Schema['CycleViewV1'][] }).items;
  expect(historicalCycles).toHaveLength(1);
  const active = setProjectState(edit, projectPath, 'ACTIVE');
  const start = startRequest(edit, active, frozen.brief.id);
  for (const offset of [1, 2]) {
    const response = edit('POST', `${projectPath}/cycles`, start, `new-${offset}`)!;
    expect(response.status).toBe(202);
    expect(validateResponse('/api/v2/projects/{id}/cycles', 'post', 202, response.value, 'application/json')).toBe(true);
    const created = (response.value as { resource: Schema['CycleStartedV1'] }).resource;
    expect(created.cycle).toMatchObject({ brief_id: frozen.brief.id, outcome: 'NO_SUPPORTED_CANDIDATE', used_experiments: 0,
      reserved_experiments: 0, available_actions: ['VIEW_BRIEF', 'VIEW_RUNS'] });
    expect(created.cycle.ordinal).toBe(Math.max(...historicalCycles.map(item => item.ordinal)) + offset);
    expect(created.cycle.next_action).toContain('仅为独立历史展示');
    expect(created.run.cycle_id).toBe(created.cycle.id);
    expect(created.run.input_set_id).toBe(frozen.execution_context.discovery_input_set_id);
    expect(created.run.terminal_reason_code).toBe('SYNTHETIC_PRESENTATION_ONLY');
    expect(Date.parse(created.run.deadline_at) - Date.parse(created.run.queued_at)).toBe(frozen.brief.content.budget.max_wall_seconds * 1000);
    expect(Date.parse(created.run.finished_at!)).toBeLessThanOrEqual(Date.parse(created.run.deadline_at));
    for (const [path, contract, value] of [
      [`/api/v2/cycles/${created.cycle.id}`, '/api/v2/cycles/{id}', created.cycle],
      [`/api/v2/runs/${created.run.id}`, '/api/v2/runs/{id}', created.run],
    ] as const) {
      const read = edit('GET', path)!;
      expect(read).toEqual({ status: 200, value });
      expect(validateResponse(contract, 'get', 200, read.value, 'application/json')).toBe(true);
    }
    const missingPath = `/api/v2/cycles/${created.cycle.id}/selection`;
    expect(edit('GET', missingPath) ?? demoResponse('GET', missingPath)).toMatchObject({ status: 404 });
    const beforeCycles = structuredClone(edit('GET', `${projectPath}/cycles`));
    const beforeRuns = structuredClone(edit('GET', '/api/v2/runs'));
    expect(edit('POST', `${projectPath}/cycles`, start, `new-${offset}`)).toMatchObject({
      status: 202, value: { resource: created, replayed: true },
    });
    expect(edit('GET', `${projectPath}/cycles`)).toEqual(beforeCycles);
    expect(edit('GET', '/api/v2/runs')).toEqual(beforeRuns);
  }
  const rows = (edit('GET', `${projectPath}/cycles`)!.value as { items: Schema['CycleViewV1'][] }).items;
  expect(rows).toHaveLength(3);
  expect(new Set(rows.map(item => item.ordinal)).size).toBe(3);
  expect(new Set(rows.map(item => item.id)).size).toBe(3);
  expect(edit('GET', `/api/v2/cycles/${historicalCycles[0]!.id}`)?.value).toEqual(historicalCycles[0]);
  expect([...records]).toEqual(history);
});

test('historical frozen contexts remain unchanged and do not inherit the new Demo Runtime', () => {
  const { edit, projectPath, freeze } = frozenFixture();
  const path = `/api/v2/briefs/${id(10)}/execution-context`;
  const original = structuredClone(records.get(path)!.value) as Schema['FrozenBriefV1'];
  const response = edit('GET', path) ?? demoResponse('GET', path);
  expect(response.value).toEqual(original);
  expect(validateResponse('/api/v2/briefs/{id}/execution-context', 'get', 200, response.value, 'application/json')).toBe(true);
  expect(original.execution_context.runtime_id).not.toBe(freeze.execution_context.runtime_id);
  const active = setProjectState(edit, projectPath, 'ACTIVE');
  const before = structuredClone(edit('GET', `${projectPath}/cycles`));
  expect(edit('POST', `${projectPath}/cycles`, startRequest(edit, active, original.brief.id), 'old-context')?.status).toBe(403);
  expect(edit('GET', `${projectPath}/cycles`)).toEqual(before);
  expect((edit('GET', path) ?? demoResponse('GET', path)).value).toEqual(original);
});

test('synthetic resource lists retain historical Runtimes, paginate, and never fabricate readiness', () => {
  const edit = projectEditor();
  for (const path of ['/api/v2/integrations/runtimes', '/api/v2/settings/codex']) {
    const all = edit('GET', path)!;
    expect(validateResponse(path, 'get', 200, all.value, 'application/json')).toBe(true);
    const items = (all.value as { items: { id: string }[] }).items;
    expect(items).toHaveLength(2);
    expect(edit('GET', path, undefined, undefined, new URLSearchParams({ limit: '1' }))).toMatchObject({ value: {
      items: [items[0]], next_cursor: items[0]!.id,
    } });
    expect(edit('GET', path, undefined, undefined, new URLSearchParams({ limit: '1', cursor: items[0]!.id }))).toMatchObject({ value: {
      items: [items[1]], next_cursor: null,
    } });
    for (const query of ['limit=0', 'limit=101', 'cursor=bad', 'limit=1&limit=2', 'unknown=1']) {
      expect(edit('GET', path, undefined, undefined, new URLSearchParams(query))?.status).toBe(422);
    }
  }
  const runtimes = (edit('GET', '/api/v2/integrations/runtimes')!.value as { items: Schema['RuntimeView'][] }).items;
  for (const runtime of runtimes) {
    const path = `/api/v2/integrations/runtimes/${runtime.id}`;
    expect((edit('GET', path) ?? demoResponse('GET', path)).value).toEqual(runtime);
    const readiness = edit('GET', `${path}/readiness`) ?? demoResponse('GET', `${path}/readiness`);
    expect(validateResponse('/api/v2/integrations/runtimes/{id}/readiness', 'get', 200, readiness.value, 'application/json')).toBe(true);
    expect(readiness.value).toMatchObject({ runtime_id: runtime.id, integration_revision: runtime.revision,
      state: runtime.configuration.enabled ? 'NOT_CHECKED' : 'DISABLED', available_job_kinds: [] });
    if (runtime.configuration.enabled) expect(readiness.value).toMatchObject({ latest_observation: null });
  }
});

test.each(['ACTIVE', 'PAUSED'] as const)('stale Cycle starts report the latest project revision before its %s admission state', state => {
  const { edit, projectPath, frozen } = frozenFixture();
  const active = setProjectState(edit, projectPath, 'ACTIVE');
  const stale = startRequest(edit, active, frozen.brief.id);
  const changed = edit('PATCH', projectPath, {
    schema_version: 1, expected_revision: active.revision, name: `${active.name} · other tab`, description: active.description, state,
  }, 'other-tab')!;
  expect(changed.status).toBe(200);
  let current = (changed.value as { resource: Schema['ProjectView'] }).resource;
  const beforeProject = structuredClone(edit('GET', projectPath));
  const beforeCycles = structuredClone(edit('GET', `${projectPath}/cycles`));
  const beforeRuns = structuredClone(edit('GET', '/api/v2/runs'));
  const rejected = edit('POST', `${projectPath}/cycles`, stale, 'stale-start')!;
  expect(rejected).toMatchObject({ status: 409, value: { code: 'REVISION_CONFLICT', current_revision: current.revision, retryable: false } });
  expect(validateResponse('/api/v2/projects/{id}/cycles', 'post', 409, rejected.value, 'application/problem+json')).toBe(true);
  if (state === 'PAUSED') {
    const closed = edit('POST', `${projectPath}/cycles`, startRequest(edit, current, frozen.brief.id), 'paused-start')!;
    expect(closed).toMatchObject({ status: 409, value: { code: 'DOMAIN_CONFLICT' } });
    expect(validateResponse('/api/v2/projects/{id}/cycles', 'post', 409, closed.value, 'application/problem+json')).toBe(true);
  }
  expect(edit('GET', projectPath)).toEqual(beforeProject);
  expect(edit('GET', `${projectPath}/cycles`)).toEqual(beforeCycles);
  expect(edit('GET', '/api/v2/runs')).toEqual(beforeRuns);
  if (state === 'PAUSED') current = setProjectState(edit, projectPath, 'ACTIVE');
  const refreshed = edit('POST', `${projectPath}/cycles`, startRequest(edit, current, frozen.brief.id), 'stale-start')!;
  expect(refreshed).toMatchObject({ status: 202, value: { replayed: false, resource: { cycle: { brief_id: frozen.brief.id } } } });
});

test('daily Cycle quotas count the project across keys and Briefs, replay old receipts and reset only at UTC midnight', () => {
  vi.useFakeTimers({ toFake: ['Date'] });
  try {
    vi.setSystemTime(new Date('2026-09-18T07:59:59.999+08:00'));
    const { edit, projectPath, frozen, freeze } = frozenFixture();
    const active = setProjectState(edit, projectPath, 'ACTIVE');
    const request = startRequest(edit, active, frozen.brief.id);
    expect(frozen.brief.content.budget.max_cycles_per_day).toBe(2);
    const path = `${projectPath}/cycles`;
    const first = edit('POST', path, request, 'first')!;
    expect(first.status).toBe(202);
    const second = edit('POST', path, request, 'second')!;
    expect(second.status).toBe(202);
    const originalReceipt = (first.value as { resource: Schema['CycleStartedV1'] }).resource;
    const beforeCycles = structuredClone(edit('GET', path));
    const beforeRuns = structuredClone(edit('GET', '/api/v2/runs'));
    for (const key of ['third', 'another-key']) {
      const rejected = edit('POST', path, request, key)!;
      expect(rejected).toMatchObject({ status: 429, value: { code: 'BUDGET_EXHAUSTED', retryable: false,
        field_errors: [{ field: 'budget', code: 'BUDGET_EXHAUSTED' }] } });
      expect(validateResponse('/api/v2/projects/{id}/cycles', 'post', 429, rejected.value, 'application/problem+json')).toBe(true);
    }
    const draftResponse = edit('POST', `${projectPath}/briefs`, {
      schema_version: 1, content: frozen.brief.content, bindings: frozen.brief.bindings, supersedes_id: frozen.brief.id,
    }, 'quota-brief')!;
    expect(draftResponse.status).toBe(201);
    const draft = (draftResponse.value as { resource: Schema['BriefView'] }).resource;
    expect(edit('POST', `/api/v2/briefs/${draft.id}/freeze`, { ...freeze, expected_revision: draft.revision }, 'quota-freeze')?.status).toBe(200);
    setProjectState(edit, projectPath, 'PAUSED');
    const resumed = setProjectState(edit, projectPath, 'ACTIVE');
    const changedRequest = startRequest(edit, resumed, draft.id);
    const beforeProject = structuredClone(edit('GET', projectPath));
    expect(edit('POST', path, changedRequest, 'new-brief')).toMatchObject({ status: 429, value: { code: 'BUDGET_EXHAUSTED' } });
    expect(edit('POST', path, request, 'first')).toEqual({ status: 202, value: { schema_version: 1, resource: originalReceipt, replayed: true } });
    expect(edit('GET', projectPath)).toEqual(beforeProject);
    expect(edit('GET', path)).toEqual(beforeCycles);
    expect(edit('GET', '/api/v2/runs')).toEqual(beforeRuns);

    vi.setSystemTime(new Date('2026-09-18T08:00:00+08:00'));
    const nextDay = edit('POST', path, changedRequest, 'new-brief')!;
    expect(nextDay.status).toBe(202);
    expect(validateResponse('/api/v2/projects/{id}/cycles', 'post', 202, nextDay.value, 'application/json')).toBe(true);
    const next = (nextDay.value as { resource: Schema['CycleStartedV1'] }).resource;
    expect(next.cycle.created_at).toBe('2026-09-18T00:00:00.000Z');
    expect(next.cycle.ordinal).toBe((second.value as { resource: Schema['CycleStartedV1'] }).resource.cycle.ordinal + 1);
    const after = structuredClone(edit('GET', path));
    expect((after!.value as { items: Schema['CycleViewV1'][] }).items).toHaveLength(4);
    expect(edit('POST', path, request, 'first')).toMatchObject({ status: 202, value: { replayed: true, resource: originalReceipt } });
    expect(edit('GET', path)).toEqual(after);
  } finally {
    vi.useRealTimers();
  }
});

test('synthetic Codex roles return unobserved metadata and never fabricate native results', () => {
  const edit = projectEditor();
  const profiles = (edit('GET', '/api/v2/settings/codex')!.value as { items: Schema['CodexProfileViewV1'][] }).items;
  for (const profile of profiles) {
    const query = new URLSearchParams({ profile_id: profile.id });
    const before = edit('GET', '/api/v2/codex/models', undefined, undefined, query)!;
    expect(before).toEqual({ status: 200, value: {
      schema_version: 1, profile_id: profile.id, profile_revision: profile.revision,
      state: 'NEVER_PROBED', observation: null,
    } });
    expect(validateResponse('/api/v2/codex/models', 'get', 200, before.value, 'application/json')).toBe(true);
    const probe = edit('POST', '/api/v2/codex/probe', {
      schema_version: 1, profile_id: profile.id, expected_revision: profile.revision,
    }, `probe-${profile.id}`) ?? demoResponse('POST', '/api/v2/codex/probe');
    expect(probe.status).toBe(403);
    expect(edit('GET', '/api/v2/codex/models', undefined, undefined, query)).toEqual(before);
  }
  for (const query of ['', 'profile_id=bad', `profile_id=${id(2981)}&profile_id=${id(2982)}`, `profile_id=${id(2981)}&unknown=1`]) {
    expect(edit('GET', '/api/v2/codex/models', undefined, undefined, new URLSearchParams(query))?.status).toBe(422);
  }
  expect(edit('GET', '/api/v2/codex/models', undefined, undefined, new URLSearchParams({ profile_id: id(9000) }))?.status).toBe(404);
});
