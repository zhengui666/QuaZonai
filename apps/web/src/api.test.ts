import { afterEach, describe, expect, it, vi } from 'vitest';
import { ApiFailure, Intent, dataOf, isCounter, isDecimal, makeClient, responseFailure, retryAt } from './api';
import { validateResponse } from './generated/responses.cjs';

afterEach(() => vi.unstubAllGlobals());
const problem = {
  type: 'urn:quazonai:problem:revision-conflict', title: 'REVISION_CONFLICT', status: 409,
  code: 'REVISION_CONFLICT', detail: '请重新载入。', request_id: '01990000-0000-7000-8000-000000000001',
  retryable: false, current_revision: '9007199254740993', field_errors: [], safe_next_actions: ['RELOAD'],
};
describe('same-origin strict API client', () => {
  it('never turns an invalid success object into an empty page', async () => {
    const client = makeClient('https://example.test', async () => Response.json({}));
    await expect(client.GET('/api/v2/projects')).rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it('accepts an explicit empty page without inventing data', async () => {
    const fetcher = vi.fn<typeof fetch>(async request => {
      expect(request).toBeInstanceOf(Request);
      if (!(request instanceof Request)) throw new Error('Expected a Request');
      expect(request.credentials).toBe('same-origin');
      expect(request.cache).toBe('no-store');
      expect(request.redirect).toBe('error');
      return Response.json({ schema_version: 1, items: [], next_cursor: null });
    });
    const result = dataOf(await makeClient('https://example.test', fetcher).GET('/api/v2/projects'));
    expect(result.items).toEqual([]);
    expect(fetcher).toHaveBeenCalledTimes(1);
  });
  it('keeps revision conflicts precise above Number.MAX_SAFE_INTEGER', async () => {
    const failure = await responseFailure(Response.json(problem, { status: 409 }));
    expect(failure).toBeInstanceOf(ApiFailure);
    expect(failure.problem?.current_revision).toBe('9007199254740993');
    expect(failure.code).toBe('REVISION_CONFLICT');
  });
  it('does not display proxy HTML or fabricated raw errors', async () => {
    const failure = await responseFailure(new Response('<h1>private proxy details</h1>', { status: 502 }));
    expect(failure.code).toBe('HTTP_CONTRACT_ERROR');
    expect(failure.message).not.toContain('private proxy');
  });
  it('rejects an offline mutation before network dispatch, with no queue', async () => {
    vi.stubGlobal('navigator', { onLine: false });
    const fetcher = vi.fn<typeof fetch>();
    await expect(makeClient('https://example.test', fetcher).POST('/api/v2/auth/logout')).rejects.toMatchObject({ code: 'OFFLINE' });
    expect(fetcher).not.toHaveBeenCalled();
    vi.stubGlobal('navigator', { onLine: true });
    expect(fetcher).not.toHaveBeenCalled();
  });
  it('distinguishes unknown network outcomes from rejected commands', async () => {
    const client = makeClient('https://example.test', async () => { throw new TypeError('secret-bearing transport message'); });
    await expect(client.POST('/api/v2/auth/logout')).rejects.toMatchObject({ code: 'NETWORK_UNKNOWN' });
  });
  it('requires exact bootstrap and session response schemas', () => {
    expect(validateResponse('/api/v2/bootstrap/status', 'GET', 200, { schema_version: 1, initialized: false, setup_allowed: true })).toBe(true);
    expect(validateResponse('/api/v2/bootstrap/status', 'GET', 200, { schema_version: 2, initialized: false, setup_allowed: true })).toBe(false);
    expect(validateResponse('/api/v2/bootstrap/status', 'GET', 200, { schema_version: 1, initialized: false })).toBe(false);
    expect(validateResponse('/api/v2/auth/logout', 'POST', 204, undefined)).toBe(true);
    expect(validateResponse('/api/v2/auth/logout', 'POST', 200, {})).toBe(false);
  });
});
describe('canonical scalar and command identity', () => {
  it('reuses the original idempotency key for the exact same retry', () => {
    const intent = new Intent(); const request = { name: 'Research', schema_version: 1 };
    const first = intent.headers('POST', '/api/v2/projects', request);
    expect(intent.headers('POST', '/api/v2/projects', { ...request })).toEqual(first);
    expect(intent.headers('POST', '/api/v2/projects', { ...request, name: 'Different' })).not.toEqual(first);
    intent.clear();
    expect(intent.headers('POST', '/api/v2/projects', request)).not.toEqual(first);
  });
  it.each(['0', '1', '9007199254740993', '9223372036854775807'])('accepts canonical database counter %s as a string', value => {
    expect(isCounter(value)).toBe(true);
  });
  it.each(['-1', '01', '1.0', '1e3', ' 1', '1\n', '9223372036854775808', ''])('rejects noncanonical or overflowing counter %j', value => {
    expect(isCounter(value)).toBe(false);
  });
  it('requires positive counters when requested', () => { expect(isCounter('0', true)).toBe(false); });
  it('does not coerce decimal money through floating point', () => {
    expect(isDecimal('12345678901234567890.123456789012345678')).toBe(true);
    expect(isDecimal('1e-8')).toBe(false);
    expect(isDecimal('0.1234567890123456789')).toBe(false);
  });
  it('parses Retry-After without allowing a past or invalid deadline', () => {
    expect(retryAt('30', 1000)).toBe(31000);
    expect(retryAt('nonsense', 1000)).toBe(0);
    expect(retryAt('Thu, 01 Jan 1970 00:00:02 GMT', 1000)).toBe(2000);
    expect(retryAt(null, 1000)).toBe(0);
  });
});
