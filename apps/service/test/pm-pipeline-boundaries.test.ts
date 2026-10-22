import assert from 'node:assert/strict';
import { test } from 'node:test';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import type { DatasetConfig } from '../src/config.ts';
import { assertPmDataAvailable, asOfDatasets, captureDataset } from '../src/pm-datasets.ts';
import { evaluatePmQualification, pmQualificationProtocol } from '../src/pm-qualification.ts';
import { pmMetricDefinitions, selectionRule } from '../src/pm-registry.ts';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { computePmDecision } from '../src/pm-native.ts';
import type { PmRecord } from '../src/pm-facts.ts';
import { iterationRequest } from '../../research/plugins/native-wasm.ts';
import { configDefaults } from '../src/config.ts';

function metrics(values: number[], role: Pm.EvidenceRole = 'INDEPENDENT'): Pm.MetricObservation[] {
  return pmMetricDefinitions().map((definition, index) => ({ definition, status: 'AVAILABLE', value: values[index]!, currency: null,
    reasonCodes: [], role, interval: { startNs: '100', endExclusiveNs: '200' }, asOfNs: '300', availableAtNs: '301', origin: 'DERIVED',
    methodRef: 'frozen-method', evidenceRefs: ['original-native-artifact'], sampleCount: '2', expectedCount: '2', uncertainty: null, raw: null }));
}

test('independent promotion: registered owner ordering can contradict a better return without reselection', () => {
  const owner = selectionRule('DOWNSIDE_FIRST'); const protocol = pmQualificationProtocol(owner);
  const result = evaluatePmQualification(protocol, owner, metrics([0.2, 0.1, 0]), metrics([0, 0, 0]), '302');
  assert.equal(result.outcome, 'NOT_READY');
  assert.ok(result.reasonCodes.includes('frozen_configuration_not_preferred_to_matched_baseline'));
  assert.deepEqual(result.improved, ['qz.metric.daily_geometric_net_return/1']);
  assert.deepEqual(result.worsened, ['qz.metric.maximum_drawdown/1']);
});

test('independent promotion: strict preference only grants shadow research, never statistical or target qualification', () => {
  const owner = selectionRule('NET_RETURN'); const protocol = pmQualificationProtocol(owner);
  const result = evaluatePmQualification(protocol, owner, metrics([0.01, 0.1, 0.5]), metrics([0, 0, 0]), '302');
  assert.equal(result.outcome, 'READY');
  assert.equal(protocol.maximumPromotion, 'SHADOW'); assert.equal(protocol.statisticalSignificanceClaim, false); assert.equal(protocol.liveTargetAuthorization, false);
  assert.equal(evaluatePmQualification(protocol, owner, metrics([0, 0, 0]), metrics([0, 0, 0]), '302').outcome, 'NOT_READY');
});

test('independent promotion: partial, selection-role, future, and mismatched evidence never pass', () => {
  const owner = selectionRule('NET_RETURN'); const protocol = pmQualificationProtocol(owner);
  const baseline = metrics([0, 0, 0]);
  const partial = metrics([0.2, 0, 0]); partial[0]!.status = 'INSUFFICIENT'; partial[0]!.value = null; partial[0]!.reasonCodes = ['incomplete_days'];
  assert.equal(evaluatePmQualification(protocol, owner, partial, baseline, '302').outcome, 'NEEDS_EVIDENCE');
  assert.equal(evaluatePmQualification(protocol, owner, metrics([0.2, 0, 0], 'SELECTION'), baseline, '302').outcome, 'NEEDS_EVIDENCE');
  assert.equal(evaluatePmQualification(protocol, owner, metrics([0.2, 0, 0]), baseline, '300').outcome, 'NEEDS_EVIDENCE');
  const shifted = metrics([0.2, 0, 0]); shifted[0]!.interval.endExclusiveNs = '201';
  assert.equal(evaluatePmQualification(protocol, owner, shifted, baseline, '302').outcome, 'NEEDS_EVIDENCE');
  assert.equal(evaluatePmQualification({ ...protocol, maximumPromotion: 'TARGET_OPERATION' }, owner, metrics([0.2, 0, 0]), baseline, '302').outcome, 'NEEDS_EVIDENCE');
});

test('independent promotion: unresolved owner frontier never invents utility weights', () => {
  const owner = selectionRule('FRONTIER');
  assert.equal(evaluatePmQualification(pmQualificationProtocol(owner), owner, metrics([0.1, 0.1, 0.5]), metrics([0, 0, 0]), '302').outcome, 'NEEDS_EVIDENCE');
});

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

test('PM owner boundary: unsupported no-borrow and loss promises preserve capability gaps and do not spend unexecuted scientific attempts', async () => {
  // Deliberately only a rejected-input protocol. No fitted performance or valid
  // native model is supplied, and the nonexistent binary must never execute.
  const data = { catalog: '/unavailable-protocol-catalog', request: { train_end_ns: '100', validation_end_ns: '200', forecast: {
    selection: { event_start_ns: '0', event_end_ns: '300', decision_cutoff_ns: '300', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] }, parameters: {} } } };
  const leverage: Pm.OwnerBoundary = { id: 'original-owner-boundary', kind: 'LEVERAGE', value: { maximumGrossNavRatio: '1', borrowingAllowed: false }, sourceStatement: 'No leverage.', effectiveFromNs: '1' };
  const loss: Pm.OwnerBoundary = { ...leverage, kind: 'LOSS', value: { metricKey: 'account.maximum_drawdown', metricDefinitionId: 'original-drawdown', limit: 0.02, unit: 'LOSS_RATIO', horizonRef: 'future' } };
  for (const [boundary, settings] of [[leverage, { account_kind: 'MARGIN', leverage: '1' }], [leverage, { account_kind: 'CASH', leverage: '2' }], [loss, { account_kind: 'CASH', leverage: '1' }]] as const) {
    const cycle = { id: 'original-cycle', strategyVersionId: 'original-strategy', datasetRevisionIds: ['original-dataset'], runtime: { conversationId: 1, taskId: 2 },
      basisRevision: 'original-basis', asOfNs: '300', availableCutoffNs: '300', accountBasis: null, dependencyRevisionRefs: [] } as unknown as Pm.DecisionCycle;
    const record = { mandate: { id: 'original-mandate', allowedUniverseRef: 'original-universe', capitalSource: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, ownerBoundaries: [boundary] },
      policy: { id: 'original-policy' }, gaps: {}, artifacts: { [cycle.id]: { maximumTrials: 2, research: { input: { data } },
        configured: { data, revision: { id: 'original-dataset', availableThroughNs: '300' }, strategy: {} },
        nativeSource: { request: iterationRequest(data, true), fit: { members: [{ factor_version_id: 'protocol-only' }] }, simulationRequest: { settings } } } } } as unknown as PmRecord;
    const result = await computePmDecision(record, cycle, configDefaults({ jobBinary: '/must-not-execute' }), BACKGROUND_CONTEXT);
    assert.equal(result.decision.action, 'NEEDS_EVIDENCE'); assert.equal(result.scienceCompleted, false);
    assert.equal(result.gaps[0]!.remedy, 'UNSUPPORTED'); assert.equal(result.decision.targetCommandId, null);
    assert.equal(result.trialSettlements.length, 2);
    assert.ok(result.trialSettlements.every(item => item.state === 'REJECTED' && !item.countedAttempt && item.evidenceRefs.length === 1));
    assert.equal(result.comparison.candidates.length, 0);
  }
});
