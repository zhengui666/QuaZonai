import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { DatasetConfig } from '../src/config.ts';
import { assertPmDataAvailable, asOfDatasets, captureDataset } from '../src/pm-datasets.ts';

test('PM PIT: native usable-availability cutoff is checked independently of manifest publication and event range', () => {
  const data = { request: { forecast: { selection: { event_start_ns: '10', event_end_ns: '1000', decision_cutoff_ns: '100' } } } };
  assert.equal(assertPmDataAvailable(data, '100'), '100', 'A future event range is not evidence that future rows were consumed.');
  assert.throws(() => assertPmDataAvailable(data, '99'), /dataset_native_availability_after_decision_cutoff/);
  const configured = { id: 'source', name: 'source', source: 'original', assetClass: 'SPOT', currency: 'USD', ready: true, start: null, end: null, data } as DatasetConfig;
  assert.throws(() => captureDataset(configured, { datasets: {}, mandates: {}, commands: {} }, '99'), /dataset_native_availability_after_decision_cutoff/);
  const state = { datasets: {}, mandates: {}, commands: {} };
  const registered = captureDataset(configured, state, '100');
  assert.equal(asOfDatasets(state, [registered.revision.id], '100')[0]!.revision.availableThroughNs, '100');
  registered.revision.availableThroughNs = '101';
  assert.throws(() => asOfDatasets(state, [registered.revision.id], '100'), /dataset_native_availability_after_decision_cutoff/);
});
