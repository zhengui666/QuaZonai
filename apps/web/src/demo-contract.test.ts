import { expect, test } from 'vitest';
import { demoResponse, records } from '../demo/records';
import { validateProblem, validateResponse } from './generated/responses.cjs';

test('synthetic preview preserves native response contracts and denies every write', () => {
  for (const [path, { contract, value }] of records) {
    expect(validateResponse(contract, 'get', 200, value, 'application/json'), contract).toBe(true);
    expect(demoResponse('GET', path)).toEqual({ status: 200, value });
    for (const method of ['POST', 'PATCH', 'PUT', 'DELETE']) {
      const denied = demoResponse(method, path);
      expect(denied.status).toBe(403);
      expect(validateProblem(denied.value)).toBe(true);
    }
  }
  expect(demoResponse('POST', '/api/v2/handoffs/arbitrary/claim').status).toBe(403);
  expect(demoResponse('GET', '/api/v2/unknown').status).toBe(404);
});
