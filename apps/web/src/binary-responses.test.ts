import { describe, expect, it } from 'vitest';
import { validateSuccessfulResponse } from './api';
import { responseKind } from './generated/responses.cjs';

const path = '/api/v2/artifacts/{id}/content';
const bytes = new Uint8Array([0, 255, 1, 128, 123, 125, 10]);

describe('native response body validation', () => {
  it('leaves declared binary content untouched for the caller', async () => {
    const response = new Response(bytes, { headers: { 'Content-Type': 'application/octet-stream' } });
    expect(await validateSuccessfulResponse(response, path, 'GET')).toBe(response);
    expect(response.bodyUsed).toBe(false);
    expect(new Uint8Array(await response.arrayBuffer())).toEqual(bytes);
  });
  it.each([
    [200, 'application/json'], [201, 'application/octet-stream'], [200, 'text/plain'],
  ])('rejects undeclared download status/media %s %s', async (status, contentType) => {
    const response = new Response('{}', { status, headers: { 'Content-Type': contentType } });
    await expect(validateSuccessfulResponse(response, path, 'GET'))
      .rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it('does not allow binary media on JSON-only operations', async () => {
    expect(responseKind('/api/v2/projects', 'GET', 200, 'application/octet-stream')).toBeUndefined();
    const response = new Response(JSON.stringify({ schema_version: 1, items: [], next_cursor: null }), {
      headers: { 'Content-Type': 'application/octet-stream' },
    });
    await expect(validateSuccessfulResponse(response, '/api/v2/projects', 'GET'))
      .rejects.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
});
