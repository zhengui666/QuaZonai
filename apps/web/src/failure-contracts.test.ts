import { describe, expect, it } from 'vitest';
import { responseFailure } from './api';

const id = '01990000-0000-7000-8000-000000000001';
const path = '/api/v2/artifacts/{id}/content' as const;
const problem = { type: 'urn:quazonai:problem:budget-exhausted', title: 'BUDGET_EXHAUSTED',
  status: 429, code: 'BUDGET_EXHAUSTED', detail: '本次运行的产物预算已经耗尽。',
  request_id: id, retryable: false, field_errors: [], safe_next_actions: [] };
function failure(body: unknown, status = 429, media = 'application/problem+json') {
  return responseFailure(Response.json(body, { status, headers: { 'Content-Type': media } }), path, 'GET');
}

describe('native error status, media and Problem schema', () => {
  it('preserves a declared valid Problem instead of treating it as a Blob', async () => {
    await expect(failure(problem)).resolves.toMatchObject({ code: 'BUDGET_EXHAUSTED', status: 429 });
  });
  it.each(['application/octet-stream', 'application/json', 'text/plain'])('rejects Problem JSON with wrong media %s', async media => {
    await expect(failure(problem, 429, media)).resolves.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it.each([
    { request_id: 'not-a-uuid' }, { request_id: '01990000-0000-4000-8000-000000000001' },
    { current_revision: '0' }, { current_revision: 1 }, { retryable: 'false' },
    { field_errors: [{ field: 'x', code: 'y' }] }, { status: 428 }, { request_id: null },
  ])('rejects an invalid native Problem field %j', override => {
    return expect(failure({ ...problem, ...override })).resolves.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it('rejects a well-shaped but undeclared status', async () => {
    await expect(failure({ ...problem, status: 418 }, 418)).resolves.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
  });
  it('preserves valid authorization errors without opening a login flow', async () => {
    await expect(failure({ ...problem, status: 401, code: 'AUTH_REQUIRED', request_id: 'invalid' }, 401))
      .resolves.toMatchObject({ code: 'HTTP_CONTRACT_ERROR' });
    await expect(failure({ ...problem, status: 401, code: 'AUTH_REQUIRED' }, 401))
      .resolves.toMatchObject({ code: 'AUTH_REQUIRED' });
  });
});
