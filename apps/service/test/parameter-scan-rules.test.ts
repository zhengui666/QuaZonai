import assert from 'node:assert/strict';
import { test } from 'node:test';
import { normalizeParameterScan, parameterScanPlan, parameterScanPoint, parameterScanStart, type ParameterAxis } from '../../../packages/contracts/src/parameter-scans.ts';
function plan(axes: ParameterAxis[]) { return parameterScanPlan({ name: 'Native configuration grid', source: { kind: 'research', researchId: 1 }, mode: 'STRATEGY', axes, objective: { metricId: 'An upstream statistic', partition: 'FULL_SELECTION', direction: 'minimize' } }); }
test('numeric paths and exact decimal grids do not impose proprietary indicator constraints', () => {
  const grid = normalizeParameterScan(plan([{ key: 'signal.fast_period', domain: { kind: 'values', values: ['5', '20'] } }, { key: 'signal.slow_period', domain: { kind: 'values', values: ['3', '40'] } }, { key: 'long_weight', domain: { kind: 'values', values: ['0', '0.100000000000000001'] } }]), { 'settings.latency_model.parameters.insert_latency_ns': '9007199254740993' });
  assert.equal(grid.total, '8'); assert.equal(grid.eligible, '8'); assert.equal(grid.skipped, '0');
  assert.equal(parameterScanPoint(grid, '1').parameters.long_weight, '0.100000000000000001');
  assert.equal(parameterScanPoint(grid, '0').parameters['settings.latency_model.parameters.insert_latency_ns'], '9007199254740993');
  assert.equal(parameterScanPoint(grid, '0').skipReason, null, 'Nautilus, not the grid, validates strategy combinations');
});
test('large grids stay compact and exact, zero and negative native values remain expressible', () => {
  const grid = normalizeParameterScan(plan([{ key: 'custom.value', domain: { kind: 'range', start: '-9007199254740993', end: '9007199254740993', step: '1' } }]), {});
  assert.equal(normalizeParameterScan(plan([{ key: 'exact', domain: { kind: 'values', values: ['9.007199254740993e15'] } }]), {}).axes[0]!.domain.kind, 'values');
  assert.equal(parameterScanPoint(normalizeParameterScan(plan([{ key: 'exact', domain: { kind: 'values', values: ['9.007199254740993e15'] } }]), {}), '0').parameters.exact, '9007199254740993');
  assert.equal(grid.total, '18014398509481987'); assert.equal(parameterScanPoint(grid, '9007199254740993').parameters['custom.value'], '0');
  const decimals = normalizeParameterScan(plan([{ key: 'native.threshold', domain: { kind: 'values', values: ['0.10', '0.1', '-1e-7'] } }]), {});
  assert.equal(decimals.total, '2'); assert.equal(parameterScanPoint(decimals, '1').parameters['native.threshold'], '-0.0000001');
});
test('independent scan concurrency and objective direction survive parsing; batching and unsafe inputs do not', () => {
  const input = plan([{ key: 'signal.period', domain: { kind: 'values', values: ['2'] } }]);
  const parsed = parameterScanStart({ ...input, requestId: 'owner-command', execution: { concurrency: 4 } });
  assert.deepEqual(parsed.execution, { concurrency: 4 }); assert.equal(parsed.objective.direction, 'minimize');
  assert.throws(() => parameterScanPlan({ ...input, source: { kind: 'factor', factorId: 'old' } }), /source_unsupported/);
  assert.throws(() => parameterScanPlan({ ...input, axes: [{ key: '__proto__.polluted', domain: { kind: 'values', values: ['1'] } }] }), /axis_unsupported/);
  assert.deepEqual(parameterScanPlan({ ...input, execution: undefined }).execution, { concurrency: 1 });
  for (const concurrency of [0, -1, 1.5, Number.MAX_SAFE_INTEGER + 1, '4']) {
    assert.throws(() => parameterScanPlan({ ...input, execution: { concurrency } }), /parameter_scan_concurrency_invalid: execution.concurrency/);
  }
  for (const batchSize of [0, 3]) {
    assert.throws(() => parameterScanPlan({ ...input, execution: { concurrency: 4, batchSize } }), /parameter_scan_field_unsupported: execution.batchSize/);
    assert.throws(() => parameterScanStart({ ...input, requestId: 'owner-command', execution: { concurrency: 4, batchSize } }), /parameter_scan_field_unsupported: execution.batchSize/);
  }
  assert.throws(() => parameterScanPlan({ ...input, objective: { ...input.objective, partition: 'VALIDATION' } }), /objective_unsupported/);
  assert.throws(() => parameterScanPlan({ ...input, axes: [...input.axes, ...input.axes] }), /duplicate_axis/);
});

import { assertDistinctNativeNumbers } from '../../../packages/contracts/src/parameter-scans.ts';
test('JSON-number grid validation follows IEEE-754 distinguishability without trimming exact string grids', () => {
  const axis = (start: string, end: string, step: string) => normalizeParameterScan(plan([{ key: 'native.f64', domain: { kind: 'range', start, end, step } }]), {}).axes[0]!;
  assert.throws(() => assertDistinctNativeNumbers(axis('30', '30.00000000000000001', '0.000000000000000001')), /indistinguishable/);
  assert.throws(() => assertDistinctNativeNumbers(axis('9007199254740993', '9007199254740997', '2')), /indistinguishable/, 'ties-to-even can collapse exact ULP steps');
  assert.doesNotThrow(() => assertDistinctNativeNumbers(axis('0', '9007199254740992', '1')), 'exactly representable huge grid must stay accepted');
  assert.doesNotThrow(() => assertDistinctNativeNumbers(axis('9007199254740992', '9007199254740996', '2')));
  for (const [start, end, step] of [['-2', '2', '0.1'], ['-0.000000000000000001', '0.000000000000000001', '0.0000000000000000001'], ['1', '1.000000000000002', '0.0000000000000001'], ['-1.000000000000002', '-1', '0.0000000000000002'], ['9007199254740991', '9007199254740995', '1']]) {
    const value = axis(start!, end!, step!); const numbers = Array.from({ length: Number(value.count) }, (_, i) => Number(parameterScanPoint({ axes: [value], fixed: {}, total: value.count, eligible: value.count, skipped: '0' }, String(i)).parameters['native.f64']));
    const distinct = new Set(numbers).size === numbers.length;
    if (distinct) assert.doesNotThrow(() => assertDistinctNativeNumbers(value)); else assert.throws(() => assertDistinctNativeNumbers(value), /indistinguishable/);
  }
});
