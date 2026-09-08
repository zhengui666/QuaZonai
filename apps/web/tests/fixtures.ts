// SYNTHETIC UI contract fixtures. These are not research, qualification, runtime,
// database isolation, trading, or production-authentication acceptance evidence.
import { expect } from '@playwright/test';
import type { Page, Route } from '@playwright/test';
import type { Schema } from '../src/api';

export const id = (tail: number) => `01990000-0000-7000-8000-${tail.toString().padStart(12, '0')}`;
export const project: Schema['ProjectView'] = {
  id: id(1), root_lineage_id: id(1), name: '合成界面测试研究', description: '仅用于界面合同测试，不是真实研究。',
  state: 'DRAFT', current_brief_id: null, current_automation_policy_id: null, created_by: 'OPERATOR',
  archived_at: null, created_at: '2026-09-08T00:00:00Z', updated_at: '2026-09-08T00:00:00Z', revision: '9007199254740993',
};
export const run: Schema['RunSnapshotV1'] = {
  schema_version: 1, id: id(3), project_id: id(1), cycle_id: null, kind: 'IMPORT', input_set_id: id(4),
  state: 'QUEUED', current_attempt_no: 0, active_attempt_id: null, last_event_seq: '1',
  deadline_at: '2030-09-08T01:00:00Z', cancellation_requested_at: null, terminal_reason_code: null,
  queued_at: '2026-09-08T00:00:00Z', started_at: null, finished_at: null, revision: '9007199254740993',
};
export const session: Schema['BrowserSession'] = {
  schema_version: 1, authenticated_at: '2026-09-08T00:00:00Z', expires_at: '2030-09-09T00:00:00Z',
  trusted_device_id: null, recent_authentication_required: false,
};
export function problem(code: string, status: number, detail = '合成合同测试错误。') {
  return { type: `urn:quazonai:problem:${code.toLowerCase()}`, title: code, status, code, detail,
    request_id: id(99), retryable: false, field_errors: [], safe_next_actions: [],
    ...(code === 'REVISION_CONFLICT' ? { current_revision: '9007199254740994' } : {}),
  };
}
function object(value: unknown): Record<string, unknown> {
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('Expected object request body');
  return value as Record<string, unknown>;
}
export async function reply(route: Route, json: unknown, status = 200) {
  await route.fulfill({ status, json, contentType: status >= 400 ? 'application/problem+json' : 'application/json', headers: { 'Cache-Control': 'no-store' } });
}
export type Captured = { path: string; method: string; key: string | null; body: unknown };
export async function fixture(page: Page, options: { authenticated?: boolean; loseFirstCreate?: boolean; conflict?: boolean; requireVerify?: boolean } = {}) {
  const state = { authenticated: options.authenticated ?? true, verified: !options.requireVerify,
    projects: [project], run: { ...run }, commands: [] as Captured[], eventHeaders: [] as (string | null)[],
  };
  await page.route('**/api/**', async route => {
    const request = route.request(); const path = new URL(request.url()).pathname; const method = request.method();
    if (!['GET', 'HEAD'].includes(method)) state.commands.push({ path, method, key: await request.headerValue('Idempotency-Key'), body: request.postDataJSON() as unknown });
    if (path === '/api/v2/bootstrap/status') return reply(route, { schema_version: 1, initialized: true, setup_allowed: false });
    if (path === '/api/v2/auth/login') {
      state.authenticated = true; return reply(route, session);
    }
    if (!state.authenticated) return reply(route, problem('AUTH_REQUIRED', 401), 401);
    if (path === '/api/v2/auth/session') return reply(route, session);
    if (path === '/api/v2/auth/logout') { state.authenticated = false; return route.fulfill({ status: 204 }); }
    if (path === '/api/v2/auth/verify') { state.verified = true; return reply(route, session); }
    if (path === '/api/v2/auth/devices') return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/v2/projects' && method === 'GET') return reply(route, { schema_version: 1, items: state.projects, next_cursor: null });
    if (path === '/api/v2/projects' && method === 'POST') {
      const body = object(request.postDataJSON());
      expect(body).toMatchObject({ schema_version: 1, fork_from_project_id: null });
      if (typeof body.name !== 'string' || typeof body.description !== 'string') throw new Error('Invalid project request');
      const calls = state.commands.filter(item => item.path === path && item.method === method);
      const created: Schema['ProjectView'] = { ...project, id: id(2), root_lineage_id: id(2), name: body.name, description: body.description, revision: '1' };
      state.projects = [created, project];
      if (options.loseFirstCreate && calls.length === 1) return route.abort('connectionfailed');
      return reply(route, { schema_version: 1, replayed: calls.length > 1, resource: created }, 201);
    }
    if (path === `/api/v2/projects/${id(1)}` && method === 'PATCH') {
      const body = object(request.postDataJSON());
      expect(body.expected_revision).toBe(project.revision);
      if (options.conflict) return reply(route, problem('REVISION_CONFLICT', 409), 409);
      return reply(route, { schema_version: 1, replayed: false, resource: project });
    }
    if (path === `/api/v2/projects/${id(1)}`) return reply(route, project);
    if (path === `/api/v2/projects/${id(1)}/briefs`) return reply(route, { schema_version: 1, items: [], next_cursor: null });
    if (path === '/api/v2/runs') return reply(route, { schema_version: 1, items: [state.run], next_cursor: null });
    if (path === `/api/v2/runs/${id(3)}`) return reply(route, state.run);
    if (path === `/api/v2/runs/${id(3)}/cancel`) {
      const body = object(request.postDataJSON());
      expect(body.expected_revision).toBe(run.revision);
      if (!state.verified) return reply(route, problem('RECENT_AUTH_REQUIRED', 403), 403);
      state.run = { ...state.run, state: 'CANCEL_REQUESTED', cancellation_requested_at: '2026-09-08T00:01:00Z', revision: '9007199254740994' };
      return reply(route, { schema_version: 1, replayed: false, resource: state.run }, 202);
    }
    if (path === `/api/v2/runs/${id(3)}/events`) {
      state.eventHeaders.push(await request.headerValue('last-event-id'));
      return route.fulfill({ status: 200, contentType: 'text/event-stream', body: ': synthetic keepalive\n\n' });
    }
    return reply(route, problem('NOT_FOUND', 404), 404);
  });
  return state;
}
export async function navigate(page: Page, title: string) {
  const open = page.getByRole('button', { name: '打开主导航' });
  const item = page.getByRole('menuitem', { name: title, exact: true });
  // Authentication is asynchronous. An immediate isVisible before the console
  // mounts would skip opening mobile navigation and then wait for a hidden item.
  await expect(open.or(item).first()).toBeVisible();
  if (!(await item.isVisible())) await open.click();
  await item.click();
  await expect(page.getByRole('dialog', { name: '主导航', exact: true })).not.toBeVisible();
}
