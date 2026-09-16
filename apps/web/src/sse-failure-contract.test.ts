import { describe, expect, it } from 'vitest';
import { responseFailure } from './api';
import { responseKind } from './generated/responses.cjs';

const path = '/api/v2/runs/{id}/events';
const problem = (status: number) => ({
  type: 'urn:quazonai:problem:stream-error', title: 'STREAM_ERROR', status,
  code: 'STREAM_ERROR', detail: 'A well-shaped upstream error is not automatically declared.',
  request_id: '01990000-0000-7000-8000-000000000001', retryable: false,
  field_errors: [], safe_next_actions: [],
});
const response = (status: number, value: unknown = problem(status), contentType = 'application/problem+json') =>
  Response.json(value, { status, headers: { 'Content-Type': contentType } });

describe('manually fetched SSE uses the exact generated operation', () => {
  it.each([418, 500])('rejects a well-formed Problem at undeclared status %s', async status => {
    expect(await responseFailure(response(status), path, 'GET'))
      .toMatchObject({ code: 'HTTP_CONTRACT_ERROR', status, problem: undefined });
  });
  it.each([401, 403, 404, 409, 410, 429])('preserves declared Problem status %s', async status => {
    expect(await responseFailure(response(status), path, 'GET')).toMatchObject({ code: 'STREAM_ERROR', status });
  });
  it('rejects wrong method/path, content type and request identity rather than widening the contract', async () => {
    for (const [endpoint, method] of [[path, 'POST'], ['/not-an-operation', 'GET']]) {
      expect(await responseFailure(response(401), endpoint!, method!)).toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
    }
    expect(await responseFailure(response(401, problem(401), 'application/octet-stream'), path, 'GET'))
      .toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
    expect(await responseFailure(response(401, { ...problem(401), code: 'AUTH_REQUIRED', request_id: 'invalid' }), path, 'GET'))
      .toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it('only classifies the declared success status and exact SSE media', () => {
    expect(responseKind(path, 'GET', 200, 'text/event-stream; charset=utf-8')).toBe('event-stream');
    expect(responseKind(path, 'GET', 201, 'text/event-stream')).toBeUndefined();
    expect(responseKind(path, 'GET', 200, 'text/event-stream-invalid')).toBeUndefined();
  });
});
