import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject } from '@earendil-works/pi-durable';
import { iterationRequest } from '../plugins/native-wasm.ts';
import { researchSettings } from '../src/settings.ts';

test('research adds no stream deadline, including retired configured deadlines', () => {
  assert.equal(researchSettings().stream?.timeoutMs, undefined);
  assert.equal(researchSettings({ stream: { timeoutMs: 1 } }).stream?.timeoutMs, undefined);
});

test('native preparation preserves absent/null fuel and explicit scientific budgets', () => {
  for (const parameters of [{}, { total_fuel: null }, { total_fuel: '23' }] as JsonObject[]) {
    const data = { request: { train_end_ns: '10', validation_end_ns: '20', forecast: { selection: { event_start_ns: '0', event_end_ns: '30' }, parameters } } };
    const original = structuredClone(data);
    const request = iterationRequest(data);
    assert.deepEqual((request.forecast as { parameters: unknown }).parameters, parameters);
    assert.deepEqual(data, original);
  }
});
