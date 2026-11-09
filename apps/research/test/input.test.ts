import assert from 'node:assert/strict';
import { test } from 'node:test';
import { researchInput } from '../src/input.ts';

const input = { goal: 'Keep exact research inputs', model: { provider: 'test', modelId: 'test' }, data: {}, policy: {} };

test('retired request allowances are rejected while real scientific fuel and row contracts remain intact', () => {
  assert.throws(() => researchInput({ ...input, maxTrials: 1 }), /research_input_fields_unsupported/);
  for (const key of ['max_trials', 'maximumScientificCalls', 'maximumModelTurns', 'modelSpend']) {
    assert.throws(() => researchInput({ ...input, policy: { [key]: 1 } }), /research_policy_fields_unsupported/);
  }
  const scientific = { ...input, data: { parameters: { total_fuel: '100' }, selection: { maximum_rows: '20' } } };
  assert.deepEqual(researchInput(scientific), scientific);
});

test('extension-owned JSON numbers must remain finite and exact across nested objects and arrays', () => {
  for (const value of [9007199254740992, -9007199254740992, Infinity, -Infinity, NaN]) {
    for (const field of ['data', 'policy'] as const) {
      assert.throws(() => researchInput({ ...input, [field]: { plugin: [{ nested: value }] } }), /invalid_json_number/);
    }
  }
  const original = { ...input, data: { plugin: [Number.MAX_SAFE_INTEGER, -Number.MAX_SAFE_INTEGER, 0, 0.25, null, '9007199254740993'] }, policy: { } };
  assert.deepEqual(researchInput(original), original);
  assert.notEqual(researchInput(original), original);
  for (const text of ['9007199254740992', '9007199254740993', '1e999']) {
    assert.throws(() => researchInput(JSON.parse(`{"goal":"original","model":{"provider":"test","modelId":"test"},"data":{"value":${text}},"policy":{}}`)), /invalid_json_number/);
  }
});
