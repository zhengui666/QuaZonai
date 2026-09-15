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
  expect(demoResponse('GET', '/api/v2/data/revisions', 'SEALED')).toMatchObject({ status: 200, value: {
    items: [{ partition: 'SEALED', origin: 'FIXTURE', row_count: '0', license_state: 'EXPIRED', source_enabled: false, runtime_enabled: false }],
  } });
  expect(demoResponse('GET', '/api/v2/portfolio-candidates/01990000-0000-7000-8000-000000000203')).toMatchObject({
    status: 200, value: { header: { origin: 'FIXTURE', execution_status: 'FAILED', target_artifact_id: null, cash_weight: null }, members: [], targets: [] },
  });
});
