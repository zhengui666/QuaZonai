import { describe, expect, it } from 'vitest';
import { makeClient } from './api';
import { responseKind } from './generated/responses.cjs';

const id = '01990000-0000-7000-8000-000000000001';
const path = '/api/v2/artifacts/{id}/content' as const;
const bytes = new Uint8Array([0, 255, 1, 128, 123, 125, 10]);
const binary = () => new Response(bytes.slice(), { headers: { 'Content-Type': 'application/octet-stream' } });

describe('declared binary responses retain their native body and parseAs behavior', () => {
  it('returns exact bytes through the native arrayBuffer parser', async () => {
    const result = await makeClient('https://example.test', async () => binary()).GET(path, {
      params: { path: { id } }, parseAs: 'arrayBuffer',
    });
    if (!(result.data instanceof ArrayBuffer)) throw new Error('ArrayBuffer response missing');
    expect(new Uint8Array(result.data)).toEqual(bytes);
  });
  it('returns a native Blob without parsing its content as JSON', async () => {
    const result = await makeClient('https://example.test', async () => binary()).GET(path, {
      params: { path: { id } }, parseAs: 'blob',
    });
    if (!(result.data instanceof Blob)) throw new Error('Blob response missing');
    expect(new Uint8Array(await result.data.arrayBuffer())).toEqual(bytes);
  });
  it('leaves a stream unconsumed until its caller reads it', async () => {
    const response = binary();
    const result = await makeClient('https://example.test', async () => response).GET(path, {
      params: { path: { id } }, parseAs: 'stream',
    });
    expect(response.bodyUsed).toBe(false);
    if (!(result.data instanceof ReadableStream)) throw new Error('ReadableStream response missing');
    expect(new Uint8Array(await new Response(result.data).arrayBuffer())).toEqual(bytes);
  });
  it.each([
    [200, 'application/json'], [201, 'application/octet-stream'], [200, 'text/plain'],
  ])('rejects undeclared status/media %s %s', async (status, contentType) => {
    const client = makeClient('https://example.test', async () => new Response('{}', { status, headers: { 'Content-Type': contentType } }));
    await expect(client.GET(path, { params: { path: { id } }, parseAs: 'blob' })).rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it('does not make arbitrary JSON endpoints permissive when parseAs is blob', async () => {
    const client = makeClient('https://example.test', async () => Response.json({}));
    await expect(client.GET('/api/v2/projects', { parseAs: 'blob' })).rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
    expect(responseKind('/api/v2/projects', 'GET', 200, 'application/octet-stream')).toBeUndefined();
  });
  it('rejects valid-looking JSON delivered using an undeclared media type', async () => {
    const body = JSON.stringify({ schema_version: 1, items: [], next_cursor: null });
    const client = makeClient('https://example.test', async () => new Response(body, { headers: { 'Content-Type': 'application/octet-stream' } }));
    await expect(client.GET('/api/v2/projects')).rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it('still treats a download failure as a Problem, not a successful Blob', async () => {
    const problem = { type: 'urn:quazonai:problem:budget-exhausted', title: 'BUDGET_EXHAUSTED',
      status: 429, code: 'BUDGET_EXHAUSTED', detail: '本次运行的产物预算已经耗尽。',
      request_id: id, retryable: false, field_errors: [], safe_next_actions: [] };
    const client = makeClient('https://example.test', async () => Response.json(problem, { status: 429, headers: { 'Content-Type': 'application/problem+json' } }));
    await expect(client.GET(path, { params: { path: { id } }, parseAs: 'blob' })).rejects.toMatchObject({
      code: 'BUDGET_EXHAUSTED', status: 429, problem: { retryable: false },
    });
  });
});
