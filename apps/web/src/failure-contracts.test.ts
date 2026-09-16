import { afterEach, describe, expect, it, vi } from 'vitest';
import { AUTH_CHANGED, makeClient } from './api';

const id = '01990000-0000-7000-8000-000000000001';
const path = '/api/v2/artifacts/{id}/content' as const;
const problem = { type: 'urn:quazonai:problem:budget-exhausted', title: 'BUDGET_EXHAUSTED',
  status: 429, code: 'BUDGET_EXHAUSTED', detail: '本次运行的产物预算已经耗尽。',
  request_id: id, retryable: false, field_errors: [], safe_next_actions: [] };
function client(body: unknown, status = 429, media = 'application/problem+json') {
  return makeClient('https://example.test', async () => Response.json(body, {
    status, headers: { 'Content-Type': media },
  }));
}
const download = (api: ReturnType<typeof client>) => api.GET(path, { params: { path: { id } }, parseAs: 'blob' });
afterEach(() => vi.unstubAllGlobals());

describe('native error status, media and Problem schema', () => {
  it('preserves a declared valid Problem instead of treating it as a Blob', async () => {
    await expect(download(client(problem))).rejects.toMatchObject({ code: 'BUDGET_EXHAUSTED', status: 429 });
  });
  it.each(['application/octet-stream', 'application/json', 'text/plain'])('rejects Problem JSON with wrong media %s', async media => {
    await expect(download(client(problem, 429, media))).rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it.each([
    { request_id: 'not-a-uuid' }, { request_id: '01990000-0000-4000-8000-000000000001' },
    { current_revision: '0' }, { current_revision: 1 }, { retryable: 'false' },
    { field_errors: [{ field: 'x', code: 'y' }] }, { status: 428 }, { request_id: null },
  ])('rejects an invalid native Problem field %j', override => {
    return expect(download(client({ ...problem, ...override }))).rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it('rejects a well-shaped but undeclared status', async () => {
    await expect(download(client({ ...problem, status: 418 }, 418))).rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it('does not emit authentication changes from malformed upstream data', async () => {
    const events = new EventTarget(); const listener = vi.fn(); events.addEventListener(AUTH_CHANGED, listener);
    vi.stubGlobal('window', events);
    await expect(download(client({ ...problem, status: 401, code: 'AUTH_REQUIRED', request_id: 'invalid' }, 401)))
      .rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
    expect(listener).not.toHaveBeenCalled();
    await expect(download(client({ ...problem, status: 401, code: 'AUTH_REQUIRED' }, 401)))
      .rejects.toMatchObject({ code: 'AUTH_REQUIRED' });
    expect(listener).toHaveBeenCalledTimes(1);
  });
});
