import assert from 'node:assert/strict';
import { test } from 'node:test';
import {
  normalizeParameterScan, parameterScanPlan, parameterScanPoint, parameterScanStart,
  type ParameterAxis, type ParameterDomain,
} from '../../../packages/contracts/src/parameter-scans.ts';

const defaults = { fast_period: '5', slow_period: '20', threshold: '0', investedWeight: '0.5', targetTtlNs: '9007199254740993' };
function plan(axes: ParameterAxis[]) {
  return parameterScanPlan({ name: 'Native parameter comparison', source: { kind: 'factor', factorId: 'r1.t2.m0' }, mode: 'STRATEGY', axes,
    objective: { metricId: 'account.sharpe252', partition: 'VALIDATION', direction: 'maximize' } });
}
const values = (key: ParameterAxis['key'], entries: string[]): ParameterAxis => ({ key, domain: { kind: 'values', values: entries } });

test('scan grid counts valid period combinations while retaining skipped points and exact values', () => {
  const input = plan([
    values('fast_period', ['5', '10', '20']), values('slow_period', ['10', '20', '40']),
    values('threshold', ['-0.5', '0', '0.5']), values('investedWeight', ['0.25', '0.50']),
  ]);
  const grid = normalizeParameterScan(input, defaults);
  assert.equal(grid.total, '54'); assert.equal(grid.eligible, '36'); assert.equal(grid.skipped, '18');
  const points = Array.from({ length: 54 }, (_, i) => parameterScanPoint(grid, String(i)));
  assert.equal(points.filter(row => !row.skipReason).length, 36);
  assert.equal(new Set(points.map(row => JSON.stringify(row.parameters))).size, 54);
  assert.equal(points[0]!.parameters.targetTtlNs, '9007199254740993');
  assert.equal(points[53]!.parameters.investedWeight, '0.5');
  assert.throws(() => parameterScanPoint(grid, '54'), /parameter_scan_ordinal_invalid/);
});

test('linear ranges use exact steps and stop before a nonaligned endpoint', () => {
  const grid = normalizeParameterScan(plan([{ key: 'threshold', domain: { kind: 'range', start: '0.1', end: '0.35', step: '0.1' } }]), defaults);
  assert.equal(grid.total, '3');
  assert.deepEqual(['0', '1', '2'].map(i => parameterScanPoint(grid, i).parameters.threshold), ['0.1', '0.2', '0.3']);
  const deduplicated = normalizeParameterScan(plan([values('investedWeight', ['0.10', '0.1', '0.2'])]), defaults);
  assert.equal(deduplicated.total, '2'); assert.equal(deduplicated.axes[0]!.duplicates, '1');
  assert.throws(() => plan([{ key: 'threshold', domain: { kind: 'range', start: '1', end: '2', step: '0' } }]), /parameter_scan_range_invalid/);
  assert.throws(() => plan([{ key: 'threshold', domain: { kind: 'range', start: '9007199254740992', end: '9007199254740996', step: '1' } }]), /parameter_scan_float_step_indistinguishable/);
  assert.throws(() => plan([{ key: 'threshold', domain: { kind: 'range', start: '9007199254740991', end: '9007199254740995', step: '1' } }]), /parameter_scan_float_step_indistinguishable/);
  assert.throws(() => plan([{ key: 'threshold', domain: { kind: 'range', start: '9007199254740993', end: '9007199254740997', step: '2' } }]), /parameter_scan_float_step_indistinguishable/);
});

test('existing finite native thresholds retain their value when JavaScript renders an exponent', () => {
  for (const threshold of ['1e-7', '1e+21', '-1e-7']) {
    const grid = normalizeParameterScan(plan([values('fast_period', ['5'])]), { ...defaults, threshold });
    const point = parameterScanPoint(grid, '0');
    assert.equal(Number(point.parameters.threshold), Number(threshold));
    assert.doesNotMatch(point.parameters.threshold!, /e/i);
  }
  const grid = normalizeParameterScan(plan([{ key: 'threshold', domain: { kind: 'range', start: '1e-7', end: '3e-7', step: '1e-7' } }]), defaults);
  assert.equal(grid.total, '3');
  assert.deepEqual(['0', '1', '2'].map(i => parameterScanPoint(grid, i).parameters.threshold), ['0.0000001', '0.0000002', '0.0000003']);
});

test('compact period range counts agree with direct enumeration across different strides and discrete domains', () => {
  const fastDomains: ParameterDomain[] = [{ kind: 'values', values: ['1', '5', '9', '2'] }];
  const slowDomains: ParameterDomain[] = [{ kind: 'values', values: ['2', '7', '12', '5'] }];
  for (const start of ['1', '3', '8']) for (const step of ['1', '2', '4']) fastDomains.push({ kind: 'range', start, end: '12', step });
  for (const start of ['2', '4', '9']) for (const step of ['1', '3', '5']) slowDomains.push({ kind: 'range', start, end: '14', step });
  for (const fast of fastDomains) for (const slow of slowDomains) {
    const grid = normalizeParameterScan(plan([{ key: 'slow_period', domain: slow }, values('threshold', ['0', '0.5']), { key: 'fast_period', domain: fast }]), defaults);
    const direct = Array.from({ length: Number(grid.total) }, (_, i) => parameterScanPoint(grid, String(i))).filter(point => !point.skipReason).length;
    assert.equal(grid.eligible, String(direct), JSON.stringify({ fast, slow }));
  }
});

test('billions of period values remain compact with counts beyond JavaScript integer precision', () => {
  const grid = normalizeParameterScan(plan([
    { key: 'fast_period', domain: { kind: 'range', start: '1', end: '4000000000', step: '1' } },
    { key: 'slow_period', domain: { kind: 'range', start: '2', end: '4000000000', step: '1' } },
  ]), defaults);
  const n = 4000000000n;
  assert.equal(grid.total, (n * (n - 1n)).toString());
  assert.equal(grid.eligible, (n * (n - 1n) / 2n).toString());
  assert.equal(grid.axes.length, 2);
  assert.equal(grid.axes[0]!.domain.kind, 'range');
  assert.equal(parameterScanPoint(grid, (BigInt(grid.total) - 1n).toString()).parameters.fast_period, '4000000000');
});

test('scan inputs reject unsupported axes, independent partitions and forged objectives before execution', () => {
  const input = plan([values('fast_period', ['5'])]);
  assert.equal(input.execution.concurrency, 1);
  assert.throws(() => parameterScanPlan({ ...input, objective: { ...input.objective, partition: 'HELDOUT' } }), /parameter_scan_objective_unsupported/);
  assert.throws(() => parameterScanPlan({ ...input, mode: 'FACTOR' }), /parameter_scan_objective_unsupported/);
  assert.throws(() => parameterScanPlan({ ...input, axes: [values('fast_period', ['5']), values('fast_period', ['10'])] }), /parameter_scan_duplicate_axis/);
  assert.throws(() => parameterScanPlan({ ...input, axes: [{ key: 'total_fuel', domain: { kind: 'values', values: ['1'] } }] }), /parameter_scan_axis_unsupported/);
  assert.throws(() => parameterScanStart(input), /parameter_scan_text_required/);
  assert.equal(parameterScanStart({ ...input, requestId: 'owner-command' }).requestId, 'owner-command');
});
