import assert from 'node:assert/strict';
import { test } from 'node:test';
import { researchInput } from '../src/input.ts';

const input = { goal: 'Keep exact research inputs', model: { provider: 'test', modelId: 'test' }, data: {}, policy: {} };

test('extension-owned JSON numbers must remain finite and exact across nested objects and arrays', () => {
  for (const value of [9007199254740992, -9007199254740992, Infinity, -Infinity, NaN]) {
    for (const field of ['data', 'policy'] as const) {
      assert.throws(() => researchInput({ ...input, [field]: { plugin: [{ nested: value }] } }), /invalid_json_number/);
    }
  }
  const original = { ...input, data: { plugin: [Number.MAX_SAFE_INTEGER, -Number.MAX_SAFE_INTEGER, 0, 0.25, null, '9007199254740993'] }, policy: { max_trials: 0 } };
  assert.deepEqual(researchInput(original), original);
  assert.notEqual(researchInput(original), original);
  for (const text of ['9007199254740992', '9007199254740993', '1e999']) {
    assert.throws(() => researchInput(JSON.parse(`{"goal":"original","model":{"provider":"test","modelId":"test"},"data":{"value":${text}},"policy":{}}`)), /invalid_json_number/);
  }
});
