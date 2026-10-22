import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, mkdtemp, rm } from 'node:fs/promises';
import { dirname, join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject } from '@earendil-works/pi-durable';
import { computePmAccountScience, computePmScience, evaluatePmIndependent } from '../src/pm-science.ts';
import { observedSpotFixture } from './pm-observed-fixture.ts';
import { pmAllocationMethod } from '../src/pm-method.ts';
import { object } from '../src/facts.ts';
import { pmMetricDefinitions } from '../src/pm-registry.ts';
import { projectNativeValidationMetrics } from '../../../packages/contracts/src/decision.ts';

const fixture = process.env.QZ_PM_NATIVE_FIXTURE;
// check-core explicitly requires the retained Rust-generated fixture. Running
// this file alone without native assets is a skipped native gate, never a pass.
test('PM native: original training panels drive frozen coefficients, fresh per-capital optimization and a matched cash account', { skip: !fixture, timeout: 180_000 }, async () => {
  const source = JSON.parse(await readFile(fixture!, 'utf8')) as JsonObject;
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-pm-native-test-'));
  const input = { source, capital: { amount: '3000', currency: 'USD' }, currency: 'USD', catalog: String(source.catalog ?? join(dirname(fixture!), 'catalog')),
    binary: process.env.QZ_JOB_BINARY!, dataDir: directory, maximumTrials: 4, cycleId: 'native-test-cycle', allocationMethod: pmAllocationMethod() };
  try {
    const retained = new Map<string, JsonObject>();
    const checkpoint = { read: async (key: string) => retained.get(key), write: async (key: string, value: JsonObject) => { retained.set(key, value); } };
    const result = await computePmScience(input, BACKGROUND_CONTEXT, checkpoint);
    assert.equal(result.consumedTrials, result.points.length + 1, 'Every real capital experiment and the cash baseline consumes the shared allowance.');
    assert.ok(result.consumedTrials <= input.maximumTrials);
    assert.ok(result.points.length >= 2, 'The native planner must generate more than the owner-capital endpoint when original lot rules identify a lower point.');
    assert.equal(object(object(result.evidence.planned).resources).attempted_simulations, 0, 'A plan is never counted as an observed replay.');
    const completed = result.points.filter(point => point.status === 'COMPLETED');
    assert.ok(completed.length > 0, JSON.stringify(result.points.map(point => ({ capital: point.capital, reason: point.reason }))));
    const cashSelection = object(result.cash.simulationRequest.selection);
    for (const point of completed) {
      assert.equal(object(point.simulationRequest!.settings).starting_capital, '3000');
      assert.deepEqual(point.simulationRequest!.selection, cashSelection, 'Every comparison must retain the exact initial state and observation interval.');
      assert.ok(Array.isArray(point.allocation!.frames) && point.allocation!.frames.length >= 2, 'Each capital point must run causal allocation at multiple cutoffs in one account.');
      assert.ok(typeof point.simulation!.events === 'string');
    }
    const interval = { startNs: String(cashSelection.event_start_ns), endExclusiveNs: String(cashSelection.event_end_ns) };
    const metadata = { role: 'SELECTION' as const, interval, asOfNs: interval.endExclusiveNs, availableAtNs: interval.endExclusiveNs,
      evidenceRefs: ['native-test-evidence'], methodRef: 'native-test-method', currency: 'USD', calendarId: 'UTC_COMPLETE_DAYS' };
    for (const simulation of [result.cash.simulation, ...completed.map(point => point.simulation!)]) {
      const metrics = projectNativeValidationMetrics({ portfolio: { partition: 'VALIDATION', statistics: simulation.statistics!, returns_status: simulation.returns_status ?? null } }, pmMetricDefinitions(), metadata);
      assert.ok(metrics.length >= 3 && metrics.every(metric => metric.status === 'AVAILABLE'), JSON.stringify(metrics));
    }
    const bindings = result.evidence.bindings as JsonObject[]; const objects = object(result.evidence.objects);
    assert.equal(bindings.length, 2);
    for (const binding of bindings) for (const field of ['reportArtifactId', 'modelArtifactId', 'calibrationArtifactId']) {
      const id = String(binding[field]); assert.match(id, /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);
      assert.ok(objects[id], `${field} must resolve to the retained actual payload.`);
    }
    assert.equal(object(result.evidence.method).parameterOrigin, 'REGISTERED_METHOD');
    assert.ok(result.limitations.includes('HISTORICAL_SELECTION_NOT_INDEPENDENT_EVALUATION'));
    assert.ok(result.limitations.includes('NO_FEASIBILITY_INTERPOLATION_BETWEEN_TESTED_POINTS'));
    const recovered = await computePmScience(input, BACKGROUND_CONTEXT, { read: checkpoint.read,
      write: async () => { throw new Error('recovered_original_scientific_receipt_was_not_reused'); } });
    assert.deepEqual(recovered.evidence.bindings, result.evidence.bindings, 'Recovery preserves the actual frozen report/calibration object identities.');
    assert.deepEqual(recovered.points, result.points);
    const independent = await evaluatePmIndependent({ ...input, selectedCapital: '3000', heldoutData: object(source.heldoutData) }, BACKGROUND_CONTEXT);
    assert.equal(independent.consumedTrials, 2, 'Frozen portfolio and its matched independent cash baseline are two actual experiments.');
    assert.equal(independent.baseline.kind, 'CASH');
    assert.equal(independent.baseline.status, 'COMPLETED');
    assert.deepEqual(independent.baseline.simulationRequest.selection, independent.point.simulationRequest?.selection);
    assert.equal(independent.evidence.role, 'INDEPENDENT');
    assert.equal(independent.point.status, 'COMPLETED', independent.point.reason ?? 'Independent native account did not complete.');
    assert.equal(independent.point.capital, '3000');
    assert.ok(!Object.hasOwn(independent.evidence, 'planned'), 'Independent assessment cannot rerun capital exploration or select a new winner.');
    const independentStudy = object((independent.evidence.studies as JsonObject[])[0]!.request);
    const anchor = BigInt(String(object((object(source.simulationRequest).target_points as JsonObject[])[0]).asof_ns));
    const ttl = BigInt(String(object(source.policy).targetTtlNs));
    for (const cutoff of independentStudy.manual_cutoffs_ns as string[]) assert.equal((BigInt(cutoff) - anchor) % ttl, 0n,
      'The independent period retains the original frozen schedule phase.');
    const outside = structuredClone(source); const outsideRequest = object(outside.simulationRequest); const selected = object(outsideRequest.selection);
    selected.event_end_ns = (BigInt(String(object(outside.request).validation_end_ns)) + 1n).toString();
    await assert.rejects(computePmScience({ ...input, source: outside }, BACKGROUND_CONTEXT), /pm_selection_outside_registered_validation/);
    await assert.rejects(computePmScience({ ...input, allocationMethod: { ...pmAllocationMethod(), objective: 'MIN_RISK' } }, BACKGROUND_CONTEXT), /pm_unregistered_allocation_method/);
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('PM observed science: native training, derived inventory NAV and paired HOLD/reoptimization remain bound across recovery and independent evaluation',
  { skip: !process.env.QZ_PM_OBSERVED_FIXTURE_DIR || !process.env.QZ_JOB_BINARY, timeout: 240_000 }, async () => {
    const f = await observedSpotFixture(join(process.env.QZ_PM_OBSERVED_FIXTURE_DIR!, 'science'));
    try {
      const source = object(f.source.fittedSource, 'actual_multiday_spot_fit_source');
      const input = { source, capture: f.common.capture, datasetRevision: f.common.datasetRevision, currency: String(f.settings.base_currency),
        catalog: f.common.catalog, dataDir: f.common.dataDir, binary: f.common.binary, maximumTrials: 4,
        cycleId: 'observed-native-science-cycle', allocationMethod: pmAllocationMethod() };
      const retained = new Map<string, JsonObject>();
      const checkpoint = { read: async (key: string) => retained.get(key), write: async (key: string, value: JsonObject) => { retained.set(key, value); } };
      assert.equal(input.capture.basis.equity, null, 'An absent native single-currency equity remains absent.');
      const original = structuredClone(input.capture);
      const result = await computePmAccountScience(input, BACKGROUND_CONTEXT, checkpoint);
      assert.deepEqual(input.capture, original, 'Derived inventory valuation must not rewrite the actual observation.');
      assert.equal(result.capital.currency, input.currency);
      assert.equal(result.valuation.original.basis, 'DERIVED_REPORT_CURRENCY_INVENTORY_VALUE');
      assert.equal(result.consumedTrials, result.points.length * 2);
      assert.ok(result.consumedTrials <= input.maximumTrials);
      assert.ok(result.points.length >= 2, 'Different capital points each require actual optimization and both native account runs.');
      assert.equal(object(object(result.evidence.planned).resources).attempted_simulations, 0);
      assert.equal(object(result.evidence.datasetBridge).kind, 'PM_DATASET_REVISION_BRIDGE');
      assert.equal(object(result.evidence.datasetBridge).pmDatasetRevisionId, input.datasetRevision.id);
      const completed = result.points.filter(point => point.status === 'COMPLETED');
      assert.ok(completed.length > 0, JSON.stringify(result.points.map(point => ({ capital: point.capital, reason: point.reason }))));
      for (const point of result.points) {
        assert.equal(point.hold.status, 'COMPLETED', point.hold.reason ?? '');
        assert.equal(point.hold.result!.orders, '0');
        assert.equal(point.hold.result!.consumed_target_points, '0');
        assert.deepEqual(point.hold.simulationRequest.target_points, []);
        const holdSelection = object(point.hold.simulationRequest.selection);
        assert.ok(BigInt(String(holdSelection.event_start_ns)) >= BigInt(input.capture.basis.availableAtNs));
        if (point.simulationRequest) assert.deepEqual(point.simulationRequest.selection, holdSelection);
        if (point.allocation) {
          const first = object((point.allocation.frames as JsonObject[])[0]!.input);
          assert.ok((first.assets as JsonObject[]).some(asset => Number(asset.current_weight) > 0), 'First optimizer sees actual inventory, not fresh cash.');
        }
      }
      for (const simulation of [completed[0]!.simulation!, completed[0]!.hold.result!]) {
        const report = object(simulation.spot_cash_report);
        assert.equal(object(report.daily_returns).report_currency, input.currency);
        const selected = object(completed[0]!.hold.simulationRequest.selection);
        const projected = projectNativeValidationMetrics({ portfolio: { partition: 'VALIDATION', statistics: report.statistics!, returns_status: report.returns_status ?? null } },
          pmMetricDefinitions(), { role: 'SELECTION', interval: { startNs: String(selected.event_start_ns), endExclusiveNs: String(selected.event_end_ns) },
            asOfNs: String(selected.event_end_ns), availableAtNs: String(selected.event_end_ns), evidenceRefs: ['native-observed-software-test'],
            methodRef: 'native-observed-software-test', currency: input.currency, calendarId: 'UTC_COMPLETE_DAYS' });
        assert.ok(projected.length >= 3 && projected.every(metric => metric.status === 'AVAILABLE'), JSON.stringify(projected));
      }
      const recovered = await computePmAccountScience(input, BACKGROUND_CONTEXT, { read: checkpoint.read,
        write: async () => { throw new Error('observed_native_receipt_not_reused'); } });
      assert.deepEqual(recovered, result);
      const independent = await evaluatePmIndependent({ ...input, capital: result.capital, selectedCapital: completed[0]!.capital,
        heldoutData: object(source.heldoutData) }, BACKGROUND_CONTEXT);
      assert.equal(independent.consumedTrials, 2);
      assert.equal(independent.point.status, 'COMPLETED', independent.point.reason ?? '');
      assert.equal(independent.baseline.kind, 'HOLD');
      assert.equal(independent.baseline.status, 'COMPLETED', independent.baseline.reason ?? '');
      assert.equal(independent.baseline.simulation!.orders, '0');
      assert.deepEqual(independent.baseline.simulationRequest.selection, independent.point.simulationRequest?.selection);
      assert.equal(Object.hasOwn(independent.evidence, 'planned'), false, 'Independent evaluation cannot launch another capital search.');
      assert.equal(independent.point.capital, completed[0]!.capital);
    } finally { await f.cleanup(); }
  });
