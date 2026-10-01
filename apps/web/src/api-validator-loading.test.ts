import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
const control = vi.hoisted(() => ({ failure: false, pending: undefined as Promise<void> | undefined }));
vi.mock('@quazonai/web/response-contract/lazy', async importOriginal => {
  const actual = await importOriginal<typeof import('@quazonai/web/response-contract/lazy')>();
  return { ...actual, validateResponseAsync: vi.fn(async (...args: Parameters<typeof actual.validateResponseAsync>) => {
    if (control.pending) await control.pending;
    if (control.failure) throw new actual.ResponseValidatorLoadError('fixture-module', new TypeError('download failed'));
    return actual.validateResponseAsync(...args);
  }) };
});
import { authenticationEvents, Intent, makeClient, responseFailure, validateSuccessfulResponse } from './api';
import { validateResponseAsync } from '@quazonai/web/response-contract/lazy';
const page = { schema_version: 1, items: [], next_cursor: null };
const problem = { type: 'urn:quazonai:problem:auth-required', title: 'AUTH_REQUIRED', status: 401,
  code: 'AUTH_REQUIRED', detail: '登录后继续', request_id: '01990000-0000-7000-8000-000000000001', retryable: false, field_errors: [], safe_next_actions: [] };
beforeEach(() => { control.failure = false; control.pending = undefined; vi.mocked(validateResponseAsync).mockClear(); });
afterEach(() => { vi.unstubAllGlobals(); });

describe('lazy response validation at the HTTP boundary', () => {
  it('does not relabel a GET validator failure as a read transport failure', async () => {
    control.failure = true;
    const fetch = vi.fn(async () => Response.json(page)); vi.stubGlobal('fetch', fetch);
    await expect(makeClient('http://localhost').GET('/api/v2/projects')).rejects.toMatchObject({ code: 'RESPONSE_VALIDATOR_UNAVAILABLE' });
    expect(fetch).toHaveBeenCalledTimes(1);
  });
  it('keeps loader failures distinct from malformed data and unknown submission transport', async () => {
    control.failure = true;
    const response = Response.json(page);
    await expect(validateSuccessfulResponse(response, '/api/v2/projects', 'GET')).rejects.toMatchObject({
      code: 'RESPONSE_VALIDATOR_UNAVAILABLE', status: 0,
    });
    expect(response.bodyUsed).toBe(false);
    expect(await response.json()).toEqual(page);
    control.failure = false;
    await expect(validateSuccessfulResponse(Response.json({}), '/api/v2/projects', 'GET')).rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it('preserves the 401 authentication notification when a declared Problem validator fails to load', async () => {
    control.failure = true; const notified = vi.fn(); authenticationEvents.addEventListener('required', notified);
    try {
      const failure = await responseFailure(Response.json(problem, { status: 401, headers: { 'Content-Type': 'application/problem+json' } }), '/api/v2/projects', 'GET');
      expect(failure.code).toBe('RESPONSE_VALIDATOR_UNAVAILABLE'); expect(failure.status).toBe(0); expect(notified).toHaveBeenCalledTimes(1);
    } finally { authenticationEvents.removeEventListener('required', notified); }
  });
  it('does not load for unknown body media, binary streams or no-content responses', async () => {
    const unknown = new Response('private upstream text', { status: 502 });
    expect((await responseFailure(unknown, '/api/v2/projects', 'GET')).code).toBe('HTTP_CONTRACT_ERROR');
    expect(unknown.bodyUsed).toBe(false);
    const empty = new Response(null, { status: 204 });
    expect(await validateSuccessfulResponse(empty, '/api/v2/auth/logout', 'POST')).toBe(empty);
    await expect(validateSuccessfulResponse(new Response('bytes', { headers: { 'Content-Type': 'unknown/media' } }), '/api/v2/projects', 'GET')).rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
    // Empty-response validation is metadata-only in the generated dispatcher.
    expect(vi.mocked(validateResponseAsync)).toHaveBeenCalledTimes(1);
  });
  it('never automatically replays a potentially committed HTTP write, retaining explicit same-intent recovery', async () => {
    control.failure = true;
    const fetch = vi.fn(async (_request: Request) => Response.json({ receipt: 'not yet validated' }, { status: 201 })); vi.stubGlobal('fetch', fetch);
    const client = makeClient('http://localhost'); const intent = new Intent();
    const body = { schema_version: 1 as const, name: 'Research', description: 'Test' };
    const headers = intent.headers('POST', '/api/v2/projects', body);
    await expect(client.POST('/api/v2/projects', { body, params: { header: headers } })).rejects.toMatchObject({ code: 'RESPONSE_VALIDATOR_UNAVAILABLE', status: 0 });
    await Promise.resolve(); expect(fetch).toHaveBeenCalledTimes(1);
    expect(intent.headers('POST', '/api/v2/projects', { ...body })).toEqual(headers);
    await expect(client.POST('/api/v2/projects', { body, params: { header: intent.headers('POST', '/api/v2/projects', body) } })).rejects.toMatchObject({ code: 'RESPONSE_VALIDATOR_UNAVAILABLE' });
    expect(fetch).toHaveBeenCalledTimes(2);
    expect((fetch.mock.calls[0]![0] as Request).headers.get('Idempotency-Key')).toBe((fetch.mock.calls[1]![0] as Request).headers.get('Idempotency-Key'));
  });
  it('rejects an abort while the validator is delayed and does not consume the success body', async () => {
    let release!: () => void; control.pending = new Promise(resolve => { release = resolve; });
    const controller = new AbortController(); const response = Response.json(page);
    const validation = validateSuccessfulResponse(response, '/api/v2/projects', 'GET', controller.signal);
    await vi.waitFor(() => expect(validateResponseAsync).toHaveBeenCalledTimes(1));
    controller.abort(); release(); await expect(validation).rejects.toMatchObject({ name: 'AbortError' }); expect(response.bodyUsed).toBe(false);
  });
});
