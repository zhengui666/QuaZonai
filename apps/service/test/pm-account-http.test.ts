import assert from 'node:assert/strict';
import { test } from 'node:test';
import { join } from 'node:path';
import { writeFile } from 'node:fs/promises';
import { isDeepStrictEqual } from 'node:util';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { PmMandateView } from '../../../packages/contracts/src/pm.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import research from '../../research/plugins/research.ts';
import factors from '../../research/plugins/factor-wasm.ts';
import fittedEnsemble from '../../research/plugins/fitted-ensemble.ts';
import { Application, json, object } from '../src/facts.ts';
import type { ObservationFact } from '../src/facts.ts';
import { acceptSnapshot } from '../src/downstream.ts';
import { captureAccount } from '../src/pm-account.ts';
import { toNativeObservedSpotStart } from '../src/pm-account-native.ts';
import { pmRecord, pmState } from '../src/pm-facts.ts';
import { observedSpotFixture } from './pm-observed-fixture.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup } from './support.ts';

const nativeOptions = { timeout: 240_000, skip: !process.env.QZ_PM_OBSERVED_FIXTURE_DIR || !process.env.QZ_JOB_BINARY
  ? 'Requires the native multi-day observed-spot source and matching job; this is not a protocol-only scientific check.' : false };
const call = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function result(transcript: { messages: readonly unknown[] }, name: string, offset = -1): JsonObject {
  const message = (transcript.messages.filter(value => (value as { role?: string; toolName?: string }).role === 'toolResult'
    && (value as { toolName?: string }).toolName === name) as { isError?: boolean; content: { type: string; text?: string }[] }[]).at(offset);
  assert.ok(message, `Original ${name} result required.`);
  const text = message.content.filter(value => value.type === 'text').map(value => value.text).join('');
  assert.ok(!message.isError, `${name}: ${text}`); return object(JSON.parse(text));
}
function script(state: Awaited<ReturnType<typeof setup>>, source: JsonObject) {
  const members = source.members as JsonObject[]; let hypothesisId = '';
  state.faux.setResponses([
    call('strategy_intake', { name: 'Original spot inventory and causal signals', archetype: 'PREDICTIVE_ALPHA',
      economicMechanism: 'Compare calibrated causal bar predictions against retaining original spot inventory in a shared account.',
      falsificationConditions: ['Net joint account path is not preferred to the matched original inventory HOLD.'], requiredCapabilities: [],
      intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['fees', 'selection_bias'], ambiguities: [],
      ownerBoundaries: [{ kind: 'LEVERAGE', sourceStatement: 'No leverage.', normalizedValue: { maximumGrossNavRatio: '1', borrowingAllowed: false } }] }),
    call('research_hypothesis', { economicFamilyId: 'native-test:spot-price-prediction', mechanism: 'Causal price factors predict the registered future return.',
      prediction: 'A frozen fitted ensemble may improve matched net account outcomes.', falsification: 'Shared-account evidence does not improve the registered objective.',
      requiredDataCapabilities: ['BAR'], incrementalQuestion: 'Do calibrated signals improve the account beyond retaining original quantities?' }),
    transcript => { hypothesisId = String(result(transcript, 'research_hypothesis').id);
      return call('factor_evaluate', { hypothesisId, hypothesis: 'Original causal close factor', model: members[0]!.model! }); },
    () => call('factor_evaluate', { hypothesisId, hypothesis: 'Original second causal factor', model: members[1]!.model! }),
    transcript => call('strategy_ensemble_fit', { hypothesisId, hypothesis: 'Fit using only original matured TRAIN observations.',
      sourceTrialIds: [result(transcript, 'factor_evaluate', -2).trialId!, result(transcript, 'factor_evaluate').trialId!] }),
    transcript => call('strategy_ensemble_evaluate', { hypothesisId, hypothesis: 'Evaluate the frozen learned combination in native validation.',
      fitTrialId: result(transcript, 'strategy_ensemble_fit').trialId!, instrumentId: String(object(source.policy).instrumentId) }),
    transcript => { result(transcript, 'strategy_ensemble_evaluate');
      return call('research_finish', { reason: 'All original factor, training and joint validation attempts are recorded for account comparison.' }); },
  ]);
}

test('PM ACCOUNT native HTTP: archived original receipts drive learned HOLD/rebalance evidence, derived NAV, owner audit and durable recovery', nativeOptions, async t => {
  const f = await observedSpotFixture(join(process.env.QZ_PM_OBSERVED_FIXTURE_DIR!, 'science'));
  const state = await setup();
  let mandateId: string | undefined;
  const diagnostic = async () => {
    if (!mandateId) return { stage: 'before_mandate_admission' };
    const record = pmRecord(pmState(await state.service.pm.snapshot()), mandateId);
    const campaigns = [];
    const task = async (taskId: number) => {
      const value = await state.service.research.harness.getTask(taskId as TaskId, context);
      return !value ? { taskId, missing: true } : { taskId, kind: value.kind, state: value.state.status === 'terminal'
        ? { status: value.state.status, outcome: value.state.outcome.status,
          error: value.state.outcome.status === 'failed' || value.state.outcome.status === 'faulted' ? value.state.outcome.error : null }
        : value.state };
    };
    for (const campaign of Object.values(record.campaigns)) {
      if (campaign.runtime.conversationId === record.conversationId) continue;
      const source = await state.service.research.state(campaign.runtime.conversationId);
      const status = await state.service.research.diagnostics(campaign.runtime.conversationId);
      const trials = source.research?.trials.map(trial => ({ taskId: trial.taskId, status: trial.status, error: trial.error, errorDetail: trial.errorDetail })) ?? [];
      const final = source.research?.final;
      campaigns.push({ campaign, status, final: final ? { taskId: final.taskId, trialId: final.trialId, status: final.status, error: final.error, reason: final.reason } : null, trials,
        taskStates: await Promise.all([...trials.flatMap(trial => typeof trial.taskId === 'number' ? [trial.taskId] : []),
          ...(status.generation ? [status.generation.taskId] : [])].map(task)) });
    }
    return { scope: 'OWNER_ONLY_CONTROLLED_NATIVE_TEST_DIAGNOSTIC', campaigns, cycles: Object.values(record.cycles), comparisons: Object.values(record.comparisons),
      errors: Object.values(record.artifacts).filter(item => item.stage), gaps: Object.values(record.gaps) };
  };
  try {
    const source = object(f.source.fittedSource, 'original_multiday_spot_source');
    const dataset = state.options.config.datasets[0]!;
    dataset.currency = String(f.settings.base_currency); dataset.data = { ...object(source.data), sealed: object(source.heldoutData) };
    dataset.strategy = object(source.policy) as unknown as NonNullable<typeof dataset.strategy>; state.options.config.jobBinary = process.env.QZ_JOB_BINARY!;
    state.options.extensions = [research, factors, fittedEnsemble];
    state.options.config.connections = [f.connection]; await state.restart();

    // Explicit archived software fixture: the original native account snapshot
    // and its historical receiver clock were already captured by the producer.
    // This is not a live /account receipt today, broker provenance or PIT proof.
    const original = object(f.application.observations[f.connection.id]);
    const archival: ObservationFact = { input: { requestId: 'archived-native-observation', portfolioId: 'archived-native-origin', mode: 'paper', connectionId: f.connection.id },
      connection: f.connection, taskId: 0, target: {}, snapshot: json(original.snapshot), snapshotReceivedAtNs: String(original.snapshotReceivedAtNs),
      view: { id: f.connection.id, name: 'Archived native software source', portfolioId: 'archived-native-origin', mode: 'paper', status: 'STOPPED',
        currency: dataset.currency, equity: null, cash: null, positions: [], curve: [], updatedAt: null, error: null, receipt: null } };
    const newlyReceived = { ...archival, snapshot: null, snapshotReceivedAtNs: undefined };
    acceptSnapshot(newlyReceived, json(archival.snapshot));
    const controls = Object.fromEntries(Object.entries(f.application.accountControls).map(([key, value]) => [key, json(value)]));
    const lateApplication = { observations: { [f.connection.id]: json(newlyReceived) }, accountControls: controls };
    const late = captureAccount(lateApplication, f.connection.id);
    assert.ok(BigInt(late.basis.availableAtNs) > BigInt(String(object(source.request).validation_end_ns)));
    assert.throws(() => toNativeObservedSpotStart(late, { selectionStartNs: String(object(object(source.simulationRequest).selection).event_start_ns) }),
      /snapshot_not_available_before_replay/, 'Receiving an old snapshot today cannot make it available to an earlier validation decision.');
    await state.service.research.harness.commit(async tx => {
      const application = await tx.doc(Application); application.observations[f.connection.id] = json(archival);
      application.accountControls = controls;
    }, context);
    // This archive has no live portfolio/target transport to refresh. A failed
    // refresh must retain its actual old receipt, not invent a current one.
    await assert.rejects(state.service.work.refreshObservation(f.connection.id));
    const retainedArchive = (await state.service.work.getObservation(f.connection.id));
    assert.equal(retainedArchive.snapshotReceivedAtNs, archival.snapshotReceivedAtNs);
    assert.deepEqual(retainedArchive.snapshot, archival.snapshot);
    script(state, source);
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers, payload: {
      requestId: 'original-observed-native-account', goal: 'Research predictive alpha from causal bar factors; compare complete net daily account outcomes with retaining original spot quantities. No leverage.',
      datasetId: dataset.id, funding: { kind: 'ACCOUNT', accountBindingId: f.connection.id }, model: state.model,
      maxTrials: 12, businessPreference: 'NET_RETURN', operatingAuthority: 'RESEARCH_ONLY' } });
    assert.equal(response.statusCode, 202, response.body); let view = response.json<PmMandateView>(); const id = view.mandate.id; mandateId = id;
    const read = async () => { const response = await state.service.app.inject({ url: `/api/v3/mandates/${encodeURIComponent(id)}`, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body); return response.json<PmMandateView>(); };
    const deadline = Date.now() + 150_000;
    while (Date.now() < deadline) {
      view = await read();
      if (view.currentDecision?.accountBasis?.valuation && view.comparisons.some(comparison => comparison.candidates.some(candidate => candidate.kind === 'HOLD_CURRENT')
        && comparison.candidates.some(candidate => candidate.kind === 'REBALANCE' && candidate.status === 'AVAILABLE'))) break;
      const record = pmRecord(pmState(await state.service.pm.snapshot()), id);
      for (const campaign of view.campaigns.filter(item => item.state === 'SEARCHING' && item.runtime.conversationId > 0)) {
        const research = await state.service.research.state(campaign.runtime.conversationId);
        const failed = research.research?.trials.filter(trial => trial.status === 'FAILED') ?? [];
        assert.equal(failed.length, 0, JSON.stringify(failed.map(trial => ({ taskId: trial.taskId, error: trial.error, detail: trial.errorDetail }))));
        assert.notEqual(research.submission?.status, 'unanswered', JSON.stringify(await diagnostic()));
      }
      assert.ok(!view.cycles.some(cycle => cycle.state === 'FAILED'), JSON.stringify({ gaps: view.gaps, errors: Object.values(record.artifacts).filter(item => item.stage) }));
      const evaluated = view.cycles.find(cycle => cycle.state === 'CONCLUDED' && cycle.triggerEventIds.some(eventId => record.events[eventId]?.kind === 'RESEARCH_EVIDENCE'));
      if (evaluated) assert.ok(view.history.some(decision => decision.cycleId === evaluated.id && decision.accountBasis?.valuation),
        JSON.stringify({ cycle: evaluated, campaigns: view.campaigns, gaps: view.gaps, decisions: view.history }));
      await new Promise(resolve => setTimeout(resolve, 100));
    }
    let record = pmRecord(pmState(await state.service.pm.snapshot()), id);
    assert.ok(view.currentDecision?.accountBasis?.valuation, JSON.stringify({ campaigns: view.campaigns, gaps: view.gaps, decisions: view.history,
      errors: Object.values(record.artifacts).filter(item => item.stage) }));
    const decision = view.currentDecision; assert.ok(['HOLD', 'REBALANCE', 'NEEDS_EVIDENCE'].includes(decision.action));
    assert.equal(decision.accountBasis!.equity, null); assert.equal(decision.targetCommandId, null);
    const nav = decision.accountBasis!.valuation!.reportValue;
    if (decision.action === 'REBALANCE') assert.deepEqual(decision.capital!.equity, nav);
    else { assert.equal(decision.capital, null); assert.deepEqual(decision.assetTargets, []); assert.deepEqual(decision.strategyAllocations, []); }
    assert.equal(decision.accountBasis!.valuation!.origin, 'DERIVED'); assert.equal(decision.accountBasis!.observedAtNs, f.native.ts_event);
    assert.equal(decision.accountBasis!.availableAtNs, f.common.capture.basis.availableAtNs);
    assert.ok(view.researchTrials.some(trial => trial.purpose === 'ENSEMBLE_FIT' && trial.state === 'COMPUTED'));
    assert.ok(view.researchTrials.some(trial => trial.purpose === 'JOINT_REPLAY' && trial.state === 'COMPUTED'));
    assert.ok(view.researchTrials.some(trial => trial.purpose === 'ROBUSTNESS' && trial.countedAttempt));
    assert.ok(view.capacityEstimates.some(capacity => capacity.curve.length >= 2));
    assert.ok(view.researchTrials.filter(trial => trial.countedAttempt).length <= 12);
    const comparison = view.comparisons.find(item => item.id === decision.comparisonRef)!;
    const leverage = view.mandate.ownerBoundaries.filter(boundary => boundary.kind === 'LEVERAGE');
    assert.equal(leverage.length, 1); assert.equal(leverage[0]!.sourceStatement, 'No leverage.');
    assert.deepEqual(leverage[0]!.value, { maximumGrossNavRatio: '1', borrowingAllowed: false });
    assert.deepEqual(object(view.activePolicy.validDomain).ownerBoundaries, view.mandate.ownerBoundaries);
    assert.equal(comparison.evaluationContext, 'HISTORICAL_JOINT_REPLAY'); assert.deepEqual(comparison.scope.capital, nav);
    assert.ok(BigInt(comparison.scope.interval.startNs) >= BigInt(decision.accountBasis!.availableAtNs));
    assert.ok(comparison.candidates.some(candidate => candidate.kind === 'HOLD_CURRENT' && candidate.status === 'AVAILABLE'));
    assert.ok(comparison.candidates.some(candidate => candidate.kind === 'REBALANCE' && candidate.status === 'AVAILABLE'));
    assert.ok(view.budgetMovements.some(movement => movement.reasonCode === 'same_observed_state_hold_replay'));
    assert.ok(comparison.candidates.every(candidate => candidate.status === 'AVAILABLE'));
    const objectiveValues = comparison.candidates.map(candidate => ({ id: candidate.id, kind: candidate.kind, values: view.activePolicy.selectionRule.objectives.map(objective => {
      const metrics = [...candidate.netOutcome, ...candidate.downside].filter(metric => metric.definition.id === objective.metricDefinitionId);
      assert.equal(metrics.length, 1); assert.equal(metrics[0]!.status, 'AVAILABLE');
      assert.equal(typeof metrics[0]!.value, 'number'); assert.ok(Number.isFinite(metrics[0]!.value));
      return { definitionId: objective.metricDefinitionId, direction: objective.direction, value: metrics[0]!.value as number };
    }) }));
    if (decision.action === 'NEEDS_EVIDENCE') {
      assert.equal(comparison.conclusion, 'UNRESOLVED_TRADEOFF'); assert.equal(comparison.selectedCandidateId, null);
      assert.ok(decision.reasonCodes.includes('investment_tradeoff_unresolved'));
      assert.equal(view.activePolicy.selectionRule.mode, 'LEXICOGRAPHIC');
      let tied = objectiveValues;
      for (let index = 0; index < view.activePolicy.selectionRule.objectives.length; index++) {
        const values = tied.map(candidate => candidate.values[index]!.value);
        const best = view.activePolicy.selectionRule.objectives[index]!.direction === 'MAXIMIZE' ? Math.max(...values) : Math.min(...values);
        tied = tied.filter(candidate => candidate.values[index]!.value === best);
      }
      assert.ok(tied.length > 1, JSON.stringify({ objectiveValues, tied }));
      assert.equal(decision.jointReplayEvidenceRef, null, 'A genuine tie cannot fabricate a selected configuration.');
    }
    // Read complete native bodies through the owner API, including compressed
    // Durable evidence. Their actual outcomes are independent of selecting a winner.
    let baselineSimulation: JsonObject | null = null;
    const pairedHoldSimulations: JsonObject[] = [];
    for (const candidate of comparison.candidates) {
      const endpoint = `/api/v3/pm-evidence/${encodeURIComponent(candidate.configurationEvidenceRef)}`;
      const response = await state.service.app.inject({ url: endpoint, headers: state.headers }); assert.equal(response.statusCode, 200, response.body);
      const body = response.json<{ reference: { role: string; visibility: string }; payload: JsonObject }>();
      assert.equal(body.reference.role, 'SELECTION'); assert.equal(body.reference.visibility, 'OWNER_ONLY');
      assert.equal(body.payload.status, 'COMPLETED');
      const selection = object(object(body.payload.simulationRequest).selection);
      assert.equal(selection.event_start_ns, comparison.scope.interval.startNs); assert.equal(selection.event_end_ns, comparison.scope.interval.endExclusiveNs);
      const simulation = object(body.payload.simulation);
      const report = object(simulation.spot_cash_report);
      assert.equal(object(report.daily_returns).report_currency, nav.currency);
      const accounts = object(simulation.canonical_result).accounts as JsonObject[];
      assert.equal(accounts.length, 1); assert.equal(object(accounts[0]!.Cash).allow_borrowing, false);
      const frames = (report.observations as JsonObject[]).filter(observation => observation.frame != null).map(observation => object(object(observation.frame).snapshot));
      assert.ok(frames.length > 0);
      for (const frame of frames) {
        assert.equal(frame.account_kind, 'CASH'); assert.equal(frame.balances_complete, true);
        for (const balance of frame.balances as JsonObject[]) for (const field of ['total', 'free', 'locked']) assert.ok(compareDecimal(String(balance[field]), '0') >= 0);
      }
      // This oracle reads original native rows directly. It must not reuse the
      // production projector, which could otherwise manufacture a false tie.
      const nativeValue = (key: string) => {
        const rows = (report.statistics as JsonObject[]).filter(row => row.native_key === key);
        assert.equal(rows.length, 1, key); const row = rows[0]!;
        assert.ok(row.status === undefined || row.status === 'AVAILABLE', key);
        assert.equal(typeof row.value, 'number', key); assert.ok(Number.isFinite(row.value), key); return row.value as number;
      };
      const observed = nativeValue('Calendar Observed Return Days');
      assert.ok(observed > 0); assert.equal(observed, nativeValue('Calendar Expected Days')); assert.equal(nativeValue('Calendar Coverage Ratio'), 1);
      const expected = new Map([
        ['qz.metric.daily_geometric_net_return/1', nativeValue('Calendar Geometric Daily Return')],
        ['qz.metric.maximum_drawdown/1', -nativeValue('Max Drawdown')],
        ['qz.metric.negative_day_fraction/1', nativeValue('Calendar Negative Days') / observed],
      ]);
      // JSON does not retain the sign of zero; numeric equality is exact and
      // accepts +0/-0 without introducing a tolerance for nonzero observations.
      for (const metric of objectiveValues.find(row => row.id === candidate.id)!.values) assert.ok(metric.value === expected.get(metric.definitionId), metric.definitionId);
      if (candidate.kind === 'HOLD_CURRENT') {
        baselineSimulation = simulation; assert.equal(simulation.orders, '0'); assert.deepEqual(object(body.payload.simulationRequest).target_points, []);
      } else {
        const paired = object(body.payload.comparison); const sent = object(paired.request); const initial = object(sent.initial_state);
        assert.deepEqual(initial.snapshot, f.native);
        assert.equal(initial.snapshot_evidence_ref, decision.accountBasis!.positionsEvidenceRef);
        assert.equal(initial.available_ns, decision.accountBasis!.availableAtNs);
        assert.deepEqual(object(paired.original).request, sent);
        const study = object(sent.study); const settings = object(study.execution_settings);
        assert.equal(settings.account_kind, 'CASH'); assert.ok(compareDecimal(String(settings.leverage), '1') <= 0);
        const constraints = object(object(study.mandate).constraints);
        assert.equal(constraints.long_only, true);
        for (const field of ['max_gross_exposure', 'max_net_exposure', 'max_asset_weight']) assert.ok(compareDecimal(String(constraints[field]), '1') <= 0);
        const hold = object(paired.hold); assert.equal(hold.status, 'COMPLETED'); pairedHoldSimulations.push(object(hold.result));
        assert.deepEqual(object(object(paired.rebalance).result).simulation, simulation);
      }
      assert.equal((await state.service.app.inject({ url: endpoint })).statusCode, 401);
    }
    assert.ok(baselineSimulation && pairedHoldSimulations.length);
    assert.ok(pairedHoldSimulations.some(hold => isDeepStrictEqual(hold, baselineSimulation)),
      'The baseline is an actual paired native HOLD leg; separate executions retain distinct native run identities.');
    for (const evidenceId of [decision.accountBasis!.positionsEvidenceRef, decision.accountBasis!.valuation!.valuationEvidenceRef]) {
      const endpoint = `/api/v3/pm-evidence/${encodeURIComponent(evidenceId)}`;
      const response = await state.service.app.inject({ url: endpoint, headers: state.headers }); assert.equal(response.statusCode, 200, response.body);
      const body = response.json<{ reference: { id: string; visibility: string }; payload: JsonObject }>();
      assert.equal(body.reference.id, evidenceId); assert.equal(body.reference.visibility, 'OWNER_ONLY');
      assert.equal((await state.service.app.inject({ url: endpoint })).statusCode, 401);
    }
    const independent = record.artifacts[`independent-task:${decision.id}`];
    if (decision.action === 'REBALANCE') {
      assert.equal(typeof independent?.taskId, 'number', 'Selected rebalances require the original owner-side independent task.');
      const settled = await state.service.research.harness.waitForTask(independent!.taskId as TaskId, context);
      assert.equal(settled.state.outcome.status, 'completed', JSON.stringify(settled.state));
      const finished = await read(); const frozen = finished.history.find(item => item.id === decision.id)!;
      assert.deepEqual(frozen.capital, decision.capital); assert.deepEqual(frozen.assetTargets, decision.assetTargets);
      assert.equal(frozen.comparisonRef, decision.comparisonRef);
      const independentEvidenceRef = object(frozen.explanationFacts).independentEvidenceRef;
      assert.equal(typeof independentEvidenceRef, 'string');
      const endpoint = `/api/v3/pm-evidence/${encodeURIComponent(String(independentEvidenceRef))}`;
      const response = await state.service.app.inject({ url: endpoint, headers: state.headers }); assert.equal(response.statusCode, 200, response.body);
      const body = response.json<{ reference: { role: string; visibility: string }; payload: JsonObject }>();
      assert.equal(body.reference.role, 'INDEPENDENT'); assert.equal(body.reference.visibility, 'OWNER_ONLY');
      assert.equal(body.payload.consumedTrials, 2);
      const baseline = object(body.payload.baseline), point = object(body.payload.point);
      assert.equal(baseline.kind, 'HOLD'); assert.equal(baseline.status, 'COMPLETED'); assert.equal(point.status, 'COMPLETED');
      assert.equal(object(baseline.simulation).orders, '0');
      assert.deepEqual(object(baseline.simulationRequest).selection, object(point.simulationRequest).selection);
      assert.equal((await state.service.app.inject({ url: endpoint })).statusCode, 401);
      assert.ok(finished.assessments.some(assessment => assessment.evidenceRefs.includes(String(independentEvidenceRef))),
        'Qualification or economic rejection must retain the actual independent evidence.');
    } else assert.equal(independent, undefined, 'HOLD or an unresolved comparison cannot fabricate an independently qualified alpha strategy.');
    view = await read(); record = pmRecord(pmState(await state.service.pm.snapshot()), id);
    const before = structuredClone(view.history.find(item => item.id === decision.id));
    const counted = view.researchTrials.filter(trial => trial.countedAttempt).map(trial => trial.id).sort();
    const originalSource = record.artifacts[record.evidence[decision.accountBasis!.positionsEvidenceRef]!.objectKey]!;
    assert.deepEqual(object(object(originalSource.snapshot).nativeObservation).snapshot, f.native);
    await state.restart(); const restored = await read();
    assert.deepEqual(restored.history.find(item => item.id === decision.id), before);
    assert.deepEqual(restored.researchTrials.filter(trial => trial.countedAttempt).map(trial => trial.id).sort(), counted);
    assert.ok(restored.history.every(item => item.targetCommandId === null));
    t.diagnostic(JSON.stringify({ action: decision.action, rawEquity: decision.accountBasis!.equity, derivedNav: nav,
      snapshotAvailableNs: decision.accountBasis!.availableAtNs, comparisonStartNs: comparison.scope.interval.startNs,
      countedAttempts: counted.length, conclusion: comparison.conclusion, objectiveValues,
      scope: 'CONTROLLED_ARCHIVED_NATIVE_ACCOUNT_SOFTWARE_COUNTERFACTUAL_NOT_BROKER_OR_LIVE_PIT_EVIDENCE' }));
  } catch (error) {
    const details = await diagnostic();
    await writeFile('/tmp/qz-pm-account-http-diagnostic.json', JSON.stringify(details, null, 2));
    t.diagnostic(JSON.stringify(details));
    throw error;
  } finally { await state.cleanup(); await f.cleanup(); }
});
