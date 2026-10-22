import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, mkdir, writeFile, rename, cp, readdir } from 'node:fs/promises';
import { createHash } from 'node:crypto';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { PmMandateStart, PmMandateView } from '../../../packages/contracts/src/pm.ts';
import { assessCapacityCurve } from '../../../packages/contracts/src/decision.ts';
import research from '../../research/plugins/research.ts';
import factors from '../../research/plugins/factor-wasm.ts';
import fittedEnsemble from '../../research/plugins/fitted-ensemble.ts';
import { canonical } from '../../research/src/documents.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { pmRecord, pmState } from '../src/pm-facts.ts';
import { evaluatePmIndependent } from '../src/pm-science.ts';
import { pmAllocationMethod } from '../src/pm-method.ts';
import { publishDatasetRevision } from '../src/dataset-revision-publish.ts';
import { fauxAssistantMessage, fauxToolCall, setup } from './support.ts';

// Real HTTP + official SQLite/Durable. The research provider and job executable
// from support.ts are explicitly protocol fixtures, not investment evidence.
// Native learning, capital replay and real forward outcomes have separate gates.
async function pmFixture() {
  const state = await setup();
  const dataset = state.options.config.datasets[0]!;
  dataset.data.request = { protocol_version: 1, train_end_ns: '100', validation_end_ns: '200', forecast: { schema_version: 1,
    selection: { schema_version: 1, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'],
      event_start_ns: '0', event_end_ns: '300', decision_cutoff_ns: '300', maximum_rows: 100 }, parameters: {} } };
  await state.restart();
  // This registered fixture can only produce a constant-score protocol result.
  // A PM consumer must not promote it into account profitability or capacity.
  state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
  return state;
}
function mandateInput(state: Awaited<ReturnType<typeof pmFixture>>, requestId: string): PmMandateStart {
  return { requestId, goal: 'Compare complete net daily return paths, losses, costs and feasible capital; retain unresolved tradeoffs.',
    datasetId: 'controlled', funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000.000000000000000001', currency: 'USD' } },
    model: state.model, maxTrials: 1, operatingAuthority: 'RESEARCH_ONLY' };
}
async function start(state: Awaited<ReturnType<typeof pmFixture>>, input: PmMandateStart) {
  const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers, payload: input });
  assert.equal(response.statusCode, 202, response.body);
  const view = response.json<PmMandateView>();
  assert.ok(view.mandate.id, 'Admission must identify the durable business mandate.');
  return view;
}
async function read(state: Awaited<ReturnType<typeof pmFixture>>, id: string) {
  const response = await state.service.app.inject({ url: `/api/v3/mandates/${encodeURIComponent(id)}`, headers: state.headers });
  assert.equal(response.statusCode, 200, response.body);
  return response.json<PmMandateView>();
}

test('PM HTTP: an unresolved business tradeoff survives original-request replay and restart without an invented preference', { timeout: 30_000 }, async () => {
  const state = await pmFixture();
  try {
    const input = mandateInput(state, 'pm-ambiguous-intent');
    const first = await start(state, input);
    assert.equal(first.mandate.ownerGoal, input.goal);
    assert.equal(first.mandate.selection.mode, 'FRONTIER_ONLY');
    assert.equal(first.activePolicy.selectionRule.mode, 'FRONTIER_ONLY');
    assert.deepEqual(first.mandate.capitalSource, input.funding);
    assert.equal(first.mandate.operatingAuthority, 'RESEARCH_ONLY');
    assert.equal(first.mandate.selection.utilityModelRef, null);

    const replay = await start(state, input);
    assert.equal(replay.mandate.id, first.mandate.id);
    assert.deepEqual(replay.mandate, first.mandate);
    assert.deepEqual(replay.activePolicy, first.activePolicy);

    const conflict = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers,
      payload: { ...input, businessPreference: 'NET_RETURN' } });
    assert.equal(conflict.statusCode, 409, 'A lost HTTP response must not allow the same request identity to silently change the investment preference.');

    await state.restart();
    const restored = await read(state, first.mandate.id);
    assert.deepEqual(restored.mandate, first.mandate);
    assert.deepEqual(restored.activePolicy, first.activePolicy);
    assert.ok(restored.history.every(decision => decision.targetCommandId === null), 'A research-only mandate cannot emit an external target.');
    assert.ok(restored.capacityEstimates.every(estimate => estimate.level !== 'ECONOMIC' || estimate.status !== 'AVAILABLE'),
      'The protocol fixture supplies no identified market-impact or economic-capacity evidence.');
    assert.ok(restored.outcomes.every(outcome => outcome.realizedMetrics.length === 0),
      'A protocol trial cannot create realized account profit.');
  } finally { await state.cleanup(); }
});

test('PM HTTP: an explicitly accepted return-first profile freezes precise metric identities instead of numeric tuning knobs', { timeout: 30_000 }, async () => {
  const state = await pmFixture();
  try {
    const input = { ...mandateInput(state, 'pm-explicit-preference'), businessPreference: 'NET_RETURN' as const };
    const view = await start(state, input);
    const rule = view.mandate.selection;
    assert.equal(rule.mode, 'LEXICOGRAPHIC');
    assert.equal(rule.establishedBy, 'ACCEPTED_PROFILE');
    assert.equal(rule.objectives[0]?.metricKey, 'account.daily_geometric_net_return');
    assert.equal(rule.objectives[0]?.direction, 'MAXIMIZE');
    assert.ok(rule.objectives.some(objective => objective.metricKey === 'account.maximum_drawdown' && objective.direction === 'MINIMIZE'));
    assert.ok(rule.objectives.every(objective => typeof objective.metricDefinitionId === 'string' && objective.metricDefinitionId.length > 0));
    assert.deepEqual(view.activePolicy.selectionRule, rule);
    assert.equal(rule.utilityModelRef, null, 'The accepted ordering does not authorize a fabricated weighted utility.');
    const before = structuredClone(rule);
    await state.restart();
    assert.deepEqual((await read(state, view.mandate.id)).mandate.selection, before);
  } finally { await state.cleanup(); }
});

const nativeFixtureDirectory = process.env.QZ_PM_NATIVE_FIXTURE_DIR;
const nativeJob = process.env.QZ_JOB_BINARY;
test('PM independent origin: matching request metadata cannot substitute another native market history',
  { skip: !nativeFixtureDirectory || !nativeJob ? 'Requires the two original native catalogs.' : false }, async () => {
    const original = JSON.parse(await readFile(join(nativeFixtureDirectory!, 'source.json'), 'utf8'));
    const other = JSON.parse(await readFile(join(nativeFixtureDirectory!, 'source-next.json'), 'utf8'));
    assert.equal(typeof other.counterexampleCatalog, 'string');
    assert.ok(original.catalogPrefix, 'The native source must retain its fit-time original prefix proof.');
    await assert.rejects(evaluatePmIndependent({ source: original, heldoutData: { ...original.heldoutData, catalog: other.counterexampleCatalog },
      selectedCapital: '3000', capital: { amount: '3000', currency: 'USD' }, currency: 'USD', catalog: other.counterexampleCatalog,
      binary: nativeJob!, dataDir: nativeFixtureDirectory!, cycleId: 'independent-original-prefix-counterexample', allocationMethod: pmAllocationMethod() },
    BACKGROUND_CONTEXT), /pm_original_catalog_prefix_changed/,
    'A sealed catalog must establish the exact frozen historical prefix before it can supply independent evidence.');
  });
function nativeToolResult(transcript: { messages: readonly unknown[] }, toolName: string, offset = -1): JsonObject {
  const messages = transcript.messages.filter(message => {
    const item = message as { role?: string; toolName?: string };
    return item.role === 'toolResult' && item.toolName === toolName;
  }) as { isError?: boolean; content: { type: string; text?: string }[] }[];
  const message = messages.at(offset);
  assert.ok(message, `Original ${toolName} receipt must exist.`);
  const text = message.content.filter(item => item.type === 'text').map(item => item.text).join('');
  assert.ok(!message.isError, `${toolName} failed: ${text}`);
  return JSON.parse(text) as JsonObject;
}
const call = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });

function scriptNativeResearch(state: Awaited<ReturnType<typeof setup>>, source: { members: { model: string }[] }) {
  let hypothesisId = '';
  state.faux.setResponses([
    call('strategy_intake', { name: 'Two causal price factors', archetype: 'PREDICTIVE_ALPHA',
      economicMechanism: 'Calibrate causal price features on TRAIN, then compare their shared-account net validation path with cash.',
      falsificationConditions: ['Matched validation account does not improve the frozen return-first objective.'],
      requiredCapabilities: [], intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['costs', 'selection_bias'],
      ambiguities: [], ownerBoundaries: [] }),
    call('research_hypothesis', { economicFamilyId: 'native-fixture:price-prediction', mechanism: 'Causal price features predict the registered future return.',
      prediction: 'The joint account can improve net validation returns relative to cash.', falsification: 'Matched net validation does not improve the objective.',
      requiredDataCapabilities: ['BAR'], incrementalQuestion: 'Do original calibrated signals add account-level evidence?' }),
    transcript => {
      hypothesisId = String(nativeToolResult(transcript, 'research_hypothesis').id);
      return call('factor_evaluate', { hypothesisId, hypothesis: 'Original close feature', model: source.members[0].model });
    },
    () => call('factor_evaluate', { hypothesisId, hypothesis: 'Original inverse-price feature', model: source.members[1].model }),
    transcript => call('strategy_ensemble_fit', { hypothesisId, hypothesis: 'Learn signal coefficients within the original TRAIN protocol.',
      sourceTrialIds: [nativeToolResult(transcript, 'factor_evaluate', -2).trialId!, nativeToolResult(transcript, 'factor_evaluate').trialId!] }),
    transcript => call('strategy_ensemble_evaluate', { hypothesisId, hypothesis: 'Replay the frozen ensemble in one original validation account.',
      fitTrialId: nativeToolResult(transcript, 'strategy_ensemble_fit').trialId!, allocations: 'ALL_EQUAL' }),
    transcript => {
      nativeToolResult(transcript, 'strategy_ensemble_evaluate');
      return call('research_finish', { reason: 'The planned factor, fitting and joint validation operations have completed; compare the disclosed evidence.' });
    },
  ]);
}

test('PM HTTP intake: an unsupported market-making mandate cannot quietly become a directional-alpha experiment', { timeout: 25_000 }, async () => {
  const state = await pmFixture();
  try {
    state.options.extensions = [research, factors];
    await state.restart();
    const goal = 'Build two-sided market making without directional exposure.';
    state.faux.setResponses([
      call('strategy_intake', { name: 'Two-sided market making', archetype: 'MICROSTRUCTURE',
        economicMechanism: 'Quote both sides and manage inventory.', falsificationConditions: ['Execution costs exceed spread capture.'],
        requiredCapabilities: ['ORDER_BOOK', 'QUEUE_POSITION'], intendedHoldingHorizonRef: 'INTRADAY_INVENTORY',
        knownFailureModes: ['adverse_selection'], ambiguities: [], ownerBoundaries: [{ kind: 'LEVERAGE', sourceStatement: 'without directional exposure' }] }),
      call('factor_evaluate', { hypothesis: 'This unsupported substitution must be refused before science.',
        model: '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))' }),
      call('research_finish', { reason: 'The admitted strategy requires unsupported market-making data and execution capabilities.' }),
    ]);
    const admitted = await start(state, { ...mandateInput(state, 'unsupported-market-making'), goal, maxTrials: 6, businessPreference: 'NET_RETURN' });
    let view = admitted;
    const deadline = Date.now() + 20_000;
    while (Date.now() < deadline) {
      view = await read(state, admitted.mandate.id);
      if (view.gaps.some(item => item.requiredCapability === 'strategy_archetype:MICROSTRUCTURE')) break;
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    assert.equal(view.mandate.ownerGoal, goal);
    assert.ok(view.gaps.some(item => item.requiredCapability === 'strategy_archetype:MICROSTRUCTURE'), JSON.stringify(view));
    assert.equal(view.researchTrials.filter(item => item.countedAttempt).length, 0,
      'A prohibited substitution cannot consume a predictive-alpha experiment and later call it market-making evidence.');
    assert.ok(view.history.every(item => item.action === 'NEEDS_EVIDENCE' && item.targetCommandId === null));
    const campaign = view.campaigns.find(item => item.runtime.conversationId > 0)!;
    const original = await state.service.research.state(campaign.runtime.conversationId);
    assert.equal(original.campaignResearch!.intake!.strategy.archetype, 'MICROSTRUCTURE');
    assert.deepEqual(original.campaignResearch!.intake!.ownerBoundaryStatements,
      [{ kind: 'LEVERAGE', sourceStatement: 'without directional exposure' }]);
  } finally { await state.cleanup(); }
});

test('PM HTTP source: an automatic revision invalidates the old basis and cannot reset an exhausted shared research budget', { timeout: 40_000 }, async () => {
  const state = await pmFixture();
  try {
    const dataset = state.options.config.datasets[0]!;
    const root = join(state.directory, 'protocol-source'); await mkdir(root);
    const manifestPath = join(root, 'revision.json'); dataset.revisionManifest = manifestPath;
    const canonical = (value: unknown): string => Array.isArray(value) ? `[${value.map(canonical).join(',')}]`
      : value !== null && typeof value === 'object' ? `{${Object.entries(value).sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)
        .map(([key, item]) => `${JSON.stringify(key)}:${canonical(item)}`).join(',')}}` : JSON.stringify(value);
    const hash = (value: string) => createHash('sha256').update(value).digest('hex');
    async function publish(revision: string, end: string) {
      const bytes = `PROTOCOL FIXTURE ONLY: ${revision}; this is not market data.`;
      await mkdir(join(root, revision)); await writeFile(join(root, revision, 'protocol.txt'), bytes);
      const body = { schemaVersion: 1, datasetId: dataset.id, sourceRevision: revision,
        availableAtNs: (BigInt(Date.now()) * 1_000_000n).toString(), revisionKind: 'APPEND',
        data: { catalog: revision, request: { schema_version: 1, train_end_ns: '100', validation_end_ns: '200',
          forecast: { schema_version: 1, selection: { schema_version: 1, bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'],
            event_start_ns: '0', event_end_ns: end, decision_cutoff_ns: end, maximum_rows: 100 }, parameters: {} } } },
        files: [{ path: `${revision}/protocol.txt`, bytes: String(Buffer.byteLength(bytes)), sha256: hash(bytes) }] };
      await writeFile(`${manifestPath}.next`, JSON.stringify({ ...body, contentDigest: hash(canonical(body)) }));
      await rename(`${manifestPath}.next`, manifestPath);
    }
    await publish('original', '300');
    state.options.extensions = [research, factors]; await state.restart();
    state.faux.setResponses([
      call('strategy_intake', { name: 'Protocol source lifecycle', archetype: 'PREDICTIVE_ALPHA',
        economicMechanism: 'Research predictive alpha from registered bar factors.', falsificationConditions: ['No usable evidence.'], requiredCapabilities: [],
        intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['missing_data'], ambiguities: [], ownerBoundaries: [] }),
      call('research_hypothesis', { economicFamilyId: 'protocol:source-lifecycle', mechanism: 'Test registered factor evidence availability.',
        prediction: 'An actual admitted adapter attempt produces a receipt.', falsification: 'The adapter cannot produce evidence.',
        requiredDataCapabilities: ['BAR'], incrementalQuestion: 'Can the registered source produce any factor evidence?' }),
      transcript => call('factor_evaluate', { hypothesisId: String(nativeToolResult(transcript, 'research_hypothesis').id),
        hypothesis: 'The unavailable native adapter must consume its admitted attempt.',
        model: '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))' }),
      call('research_finish', { reason: 'The single admitted adapter attempt failed; no economic conclusion is available.' }),
    ]);
    const admitted = await start(state, { ...mandateInput(state, 'automatic-source-budget'), goal: 'Research predictive alpha from bar factors within the original one-attempt budget.' });
    let view = admitted;
    const firstDeadline = Date.now() + 15_000;
    while (Date.now() < firstDeadline) {
      view = await read(state, admitted.mandate.id);
      if (view.currentDecision && view.researchTrials.some(trial => trial.countedAttempt)) break;
      await new Promise(resolve => setTimeout(resolve, 40));
    }
    assert.ok(view.currentDecision, JSON.stringify(view));
    assert.equal(view.researchTrials.filter(trial => trial.countedAttempt).length, 1);
    assert.equal(view.currentDecision.action, 'NEEDS_EVIDENCE');
    const original = structuredClone(view.currentDecision);
    const originalDataset = view.datasetRevisions[0]!.id;
    const originalStrategy = view.strategies[0]!.id;
    await publish('new-source', '400');
    const deadline = Date.now() + 20_000;
    while (Date.now() < deadline) {
      view = await read(state, admitted.mandate.id);
      if (view.campaigns.length === 2 && view.campaigns.at(-1)!.state === 'PAUSED') break;
      await new Promise(resolve => setTimeout(resolve, 40));
    }
    assert.equal(view.campaigns.length, 2, 'The official monitor must admit the source change without a refresh HTTP request.');
    assert.equal(view.campaigns.at(-1)!.state, 'PAUSED');
    assert.equal(view.campaigns.at(-1)!.conclusionReason, 'shared_trial_budget_exhausted');
    assert.equal(view.researchTrials.filter(trial => trial.countedAttempt).length, 1, 'A new dataset identity cannot grant a new trial allowance.');
    assert.equal(view.history.find(decision => decision.id === original.id)!.status, 'SUPERSEDED');
    assert.notEqual(view.currentDecision?.id, original.id);
    const nextDataset = view.campaigns.at(-1)!.datasetRevisionIds[0]!;
    assert.notEqual(nextDataset, originalDataset);
    assert.ok(view.datasetRevisions.some(revision => revision.id === nextDataset));
    assert.ok(view.strategies.some(strategy => strategy.id !== originalStrategy && strategy.modelRefs.length === 0));
    const stateSnapshot = pmState(await state.service.pm.snapshot());
    assert.equal(stateSnapshot.datasets[originalDataset]!.revision.sourceRevision, 'original');
    assert.equal(await readFile(join(root, 'original', 'protocol.txt'), 'utf8'), 'PROTOCOL FIXTURE ONLY: original; this is not market data.');
    assert.ok(view.history.every(decision => decision.targetCommandId === null));
  } finally { await state.cleanup(); }
});

test('PM native HTTP: learned signals produce a matched owner-NAV decision with original costs, budgets and replayable evidence',
  { timeout: 240_000, skip: !nativeFixtureDirectory || !nativeJob ? 'Requires the generated native multi-day catalog and matching real job binary.' : false }, async t => {
    const source = JSON.parse(await readFile(join(nativeFixtureDirectory!, 'source.json'), 'utf8'));
    assert.equal(typeof source.catalog, 'string', 'The native generator must identify the actual immutable catalog used by its report.');
    const state = await setup();
    const originalBinary = process.env.QZ_JOB_BINARY;
    try {
      const dataset = state.options.config.datasets[0]!;
      assert.equal(source.data.catalog, source.catalog);
      assert.ok(source.heldoutData && source.policy.simulationSettings,
        'The fixture must export its original clipped data, sealed extension and actually replayed policy.');
      dataset.data = { ...source.data, sealed: source.heldoutData };
      dataset.strategy = source.policy;
      dataset.revisionManifest = join(state.directory, 'immutable-native-source', 'current.json');
      const published = await publishDatasetRevision(dataset, { scope: 'TEST_NATIVE_HISTORY_RECOVERY' });
      dataset.data = published.configured.data;
      const originalManifest = await readFile(dataset.revisionManifest, 'utf8');
      const nativeLog = join(state.directory, 'actual-native-commands.log');
      const nativeWrapper = join(state.directory, 'original-native-job.mjs');
      await writeFile(nativeWrapper, `#!${process.execPath}\nimport {spawnSync} from 'node:child_process';\nimport {appendFileSync} from 'node:fs';\nconst args=process.argv.slice(2);appendFileSync(${JSON.stringify(nativeLog)},args[0]+'\\n');\nconst chunks=[];for await(const chunk of process.stdin)chunks.push(chunk);const child=spawnSync(${JSON.stringify(nativeJob)},args,{input:Buffer.concat(chunks),stdio:['pipe','inherit','inherit']});process.exit(child.status??1);\n`, { mode: 0o700 });
      process.env.QZ_JOB_BINARY = nativeWrapper; state.options.config.jobBinary = nativeWrapper;
      state.options.extensions = [research, factors, fittedEnsemble];
      await state.restart();
      scriptNativeResearch(state, source);
      const admitted = await start(state, { ...mandateInput(state, 'pm-native-supported-path'),
        goal: 'Research predictive alpha from causal bar features and compare complete daily net account outcomes with cash.',
        funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } },
        businessPreference: 'NET_RETURN', maxTrials: 12 });
      let view = admitted;
      const deadline = Date.now() + 100_000;
      while (Date.now() < deadline) {
        view = await read(state, admitted.mandate.id);
        const campaign = view.campaigns.find(item => item.state === 'SEARCHING' && item.runtime.conversationId > 0);
        if (campaign) {
          const actual = await state.service.research.state(campaign.runtime.conversationId);
          const failures = actual.research?.trials.filter(trial => trial.status === 'FAILED') ?? [];
          assert.equal(failures.length, 0, JSON.stringify(failures.map(trial => ({ taskId: trial.taskId, error: trial.error, detail: trial.errorDetail }))));
        }
        if (view.lastCycle?.runtime.taskId) {
          await state.service.research.harness.waitForTask(view.lastCycle.runtime.taskId as TaskId, BACKGROUND_CONTEXT);
          view = await read(state, admitted.mandate.id);
          break;
        }
        await new Promise(resolve => setTimeout(resolve, 50));
      }
      let record = pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id);
      assert.ok(view.currentDecision, JSON.stringify({ campaigns: view.campaigns, gaps: view.gaps,
        errors: Object.values(record.artifacts).filter(item => item.stage) }));
      const selectedDecision = structuredClone(view.currentDecision);
      if (selectedDecision.action === 'REBALANCE') {
        const independentTaskId = record.artifacts[`independent-task:${selectedDecision.id}`]?.taskId;
        assert.equal(typeof independentTaskId, 'number', 'The owner-side independent task must be admitted with the original selected decision.');
        const final = await state.service.research.harness.waitForTask(independentTaskId as TaskId, BACKGROUND_CONTEXT);
        assert.equal(final.state.status, 'terminal');
        assert.equal(final.state.status === 'terminal' && final.state.outcome.status, 'completed', JSON.stringify(final.state));
        view = await read(state, admitted.mandate.id);
        record = pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id);
        const finalDecision = view.currentDecision!;
        assert.equal(finalDecision.id, selectedDecision.id);
        assert.equal(finalDecision.comparisonRef, selectedDecision.comparisonRef);
        assert.deepEqual(finalDecision.capital, selectedDecision.capital, 'Sealed results cannot select a more favorable capital configuration.');
        assert.deepEqual(finalDecision.assetTargets, selectedDecision.assetTargets);
        const independentEvidenceId = (finalDecision.explanationFacts as JsonObject).independentEvidenceRef;
        assert.equal(typeof independentEvidenceId, 'string');
        const endpoint = `/api/v3/pm-evidence/${encodeURIComponent(String(independentEvidenceId))}`;
        const evidence = await state.service.app.inject({ url: endpoint, headers: state.headers });
        assert.equal(evidence.statusCode, 200, evidence.body);
        const original = evidence.json<{ reference: { id: string; visibility: string; role: string }; payload: JsonObject }>();
        assert.equal(original.reference.id, independentEvidenceId);
        assert.equal(original.reference.visibility, 'OWNER_ONLY');
        assert.equal(original.reference.role, 'INDEPENDENT');
        assert.equal(original.payload.consumedTrials, 2, 'Frozen portfolio and matched baseline each consume an actual independent attempt.');
        assert.equal((await state.service.app.inject({ url: endpoint })).statusCode, 401);
        const exposure = await state.service.research.harness.snapshot(Exposures, BACKGROUND_CONTEXT);
        const finals = exposure!.windows.filter(item => item.kind === 'FINAL');
        assert.equal(finals.length, 1, 'Only the frozen PM portfolio may consume the original sealed interval.');
        assert.equal(finals[0]!.taskId, independentTaskId);
        assert.ok(view.ensembles.some(ensemble => ensemble.id === finalDecision.ensembleRevisionId && ensemble.qualification === 'INDEPENDENTLY_EVALUATED'));
        const protocol = (view.activePolicy.validDomain as JsonObject).qualificationProtocol as JsonObject;
        assert.equal(protocol.maximumPromotion, 'SHADOW');
        assert.equal(protocol.statisticalSignificanceClaim, false);
        assert.equal(protocol.liveTargetAuthorization, false);
        assert.deepEqual(protocol.selection, admitted.activePolicy.selectionRule);
        assert.ok(view.assessments.some(assessment => assessment.stage === 'SHADOW' && assessment.outcome === 'READY'
          && assessment.reasonCodes.includes('frozen_configuration_survived_registered_economic_falsification')),
        'This synthetic upward-price fixture must pass only the pre-registered shadow comparison, without claiming statistical significance or live qualification.');
        assert.ok(!view.assessments.some(assessment => assessment.stage === 'TARGET_OPERATION' && assessment.outcome === 'READY'));
      }
      const decision = view.currentDecision!;
      assert.ok(['REBALANCE', 'CASH'].includes(decision.action), JSON.stringify({ decision, comparisons: view.comparisons }));
      assert.equal(decision.targetCommandId, null, 'Historical selection does not authorize a current external target.');
      assert.ok(view.ensembles.some(ensemble => ensemble.members.length === 2 && (decision.action === 'CASH'
        ? ensemble.qualification === 'CANDIDATE' : ['SELECTED', 'INDEPENDENTLY_EVALUATED'].includes(ensemble.qualification))));
      for (const ensemble of view.ensembles) for (const member of ensemble.members) {
        assert.equal(typeof member.calibrationModelId, 'string', 'A learned member must identify its actual frozen native calibration.');
        assert.ok(ensemble.trainingEvidenceRefs.includes(member.calibrationModelId!));
        const calibration = await state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(member.calibrationModelId!)}`, headers: state.headers });
        assert.equal(calibration.statusCode, 200, calibration.body);
        const originalCalibration = calibration.json<{ reference: { id: string; role: string }; payload: JsonObject }>();
        assert.equal(originalCalibration.reference.id, member.calibrationModelId);
        assert.equal(originalCalibration.reference.role, 'TRAINING');
        const reportId = originalCalibration.payload.source_report_artifact_id;
        assert.equal(typeof reportId, 'string');
        assert.ok(ensemble.trainingEvidenceRefs.includes(String(reportId)), 'The calibration must resolve to its original native training panel.');
        const report = await state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(String(reportId))}`, headers: state.headers });
        assert.equal(report.statusCode, 200, report.body);
        const originalReport = report.json<{ reference: { id: string; role: string }; payload: JsonObject }>();
        assert.equal(originalReport.reference.id, reportId);
        assert.equal(originalReport.reference.role, 'TRAINING');
        assert.equal((originalReport.payload.request as JsonObject).train_end_ns, ensemble.fitCutoffNs);
        assert.ok(Array.isArray(originalReport.payload.points) && originalReport.payload.points.length > 0);
      }
      assert.ok(view.capacityEstimates.some(estimate => estimate.curve.length >= 2), 'The actual service must explore more than full deployment plus cash.');
      assert.ok(view.capacityEstimates.every(estimate => estimate.level !== 'ECONOMIC'), 'No fixture identifies live market-impact capacity.');
      assert.ok(view.researchTrials.some(trial => trial.purpose === 'ENSEMBLE_FIT' && trial.state === 'COMPUTED'));
      assert.ok(view.researchTrials.some(trial => trial.purpose === 'JOINT_REPLAY' && trial.state === 'COMPUTED'));
      assert.ok(view.researchTrials.filter(trial => trial.countedAttempt).length <= 12);
      assert.ok(view.comparisons.some(comparison => comparison.candidates.some(candidate => candidate.kind === 'CASH')
        && comparison.candidates.some(candidate => candidate.kind === 'REBALANCE' && candidate.status === 'AVAILABLE')));
      assert.equal(decision.capital!.equity.amount, '3000');
      for (const capacity of view.capacityEstimates) {
        const resource = await state.service.app.inject({ url: `/api/v3/capacity-estimates/${encodeURIComponent(capacity.id)}`, headers: state.headers });
        assert.equal(resource.statusCode, 200, resource.body);
        assert.deepEqual(resource.json(), capacity);
        for (const point of capacity.curve.filter(point => point.status === 'AVAILABLE')) {
          assert.equal(assessCapacityCurve(resource.json(), point.capital).feasible, true, 'The public capacity evidence must preserve actual tested-point feasibility.');
        }
        assert.equal(capacity.supportedCapitalUpperBound, null, 'Successful discrete capital points never establish an economic upper bound.');
        assert.ok(capacity.feasibleRegions.every(region => region.kind === 'TESTED_POINT' && region.lower.amount === region.upper.amount));
      }
      for (const campaign of view.campaigns) {
        const resource = await state.service.app.inject({ url: `/api/v3/campaigns/${encodeURIComponent(campaign.id)}/trials`, headers: state.headers });
        assert.equal(resource.statusCode, 200, resource.body);
        const original = await state.service.research.state(campaign.runtime.conversationId);
        assert.equal(original.research!.final!.status, 'STOPPED', 'The Agent-side campaign must finish selection without consuming the PM portfolio held-out evaluation.');
        assert.deepEqual(resource.json(), view.researchTrials.filter(trial => trial.campaignId === campaign.id));
      }
      const original = structuredClone(decision);
      await state.restart();
      let restored = await read(state, admitted.mandate.id);
      assert.deepEqual(restored.history.find(item => item.id === original.id), original,
        'Restart must disclose the original scientific decision instead of recomputing a more favorable one.');
      const completeRecord = pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id);
      const originalComparison = structuredClone(completeRecord.comparisons[original.comparisonRef]);
      const originalAttempts = structuredClone(completeRecord.derivedTrials);
      const originalMovements = structuredClone(completeRecord.budgetMovements);
      const originalExposures = structuredClone((await state.service.research.harness.snapshot(Exposures, BACKGROUND_CONTEXT))!.windows);
      const commandsBefore = (await readFile(nativeLog, 'utf8')).split('\n');
      const waitForSource = async (predicate: (value: PmMandateView) => boolean) => {
        const stop = Date.now() + 20_000; let value = await read(state, admitted.mandate.id);
        while (!predicate(value) && Date.now() < stop) { await new Promise(resolve => setTimeout(resolve, 50)); value = await read(state, admitted.mandate.id); }
        assert.ok(predicate(value), JSON.stringify({ gaps: value.gaps, cycle: value.lastCycle })); return value;
      };
      await writeFile(dataset.revisionManifest, '{temporary unreadable native source');
      await waitForSource(value => value.currentDecision === null && value.gaps.some(item => item.requiredCapability === 'source_revision' && item.state === 'OPEN'));
      await state.restart();
      await writeFile(dataset.revisionManifest, originalManifest);
      restored = await waitForSource(value => value.currentDecision?.id === original.id);
      const recoveredRecord = pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id);
      const recoveredEvent = Object.values(recoveredRecord.events).find(item => item.kind === 'SOURCE_RECOVERED')!;
      assert.ok(recoveredEvent);
      const recoveredEvidence = await state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(recoveredEvent.sourceRevisionRef)}`, headers: state.headers });
      assert.equal(recoveredEvidence.statusCode, 200, recoveredEvidence.body);
      assert.equal(recoveredEvidence.json().payload.outcome, 'HISTORICAL_REPORT_RESTORED');
      assert.equal(recoveredEvidence.json().payload.restoredDecisionId, original.id);
      assert.equal(recoveredEvidence.json().payload.prefixVerification.method, 'NAUTILUS_SELECTED_PREFIX_SHA256_V1');
      assert.deepEqual(recoveredRecord.comparisons[original.comparisonRef], originalComparison);
      assert.deepEqual(recoveredRecord.derivedTrials, originalAttempts); assert.deepEqual(recoveredRecord.budgetMovements, originalMovements);
      assert.deepEqual((await state.service.research.harness.snapshot(Exposures, BACKGROUND_CONTEXT))!.windows, originalExposures);
      const recoveredCommands = (await readFile(nativeLog, 'utf8')).split('\n');
      assert.deepEqual(recoveredCommands.filter(command => command !== 'catalog-prefix-digest'), commandsBefore.filter(command => command !== 'catalog-prefix-digest'),
        'Historical recovery may verify the original prefix but never repeat native science or consume FINAL again.');
      assert.ok(recoveredCommands.filter(command => command === 'catalog-prefix-digest').length > commandsBefore.filter(command => command === 'catalog-prefix-digest').length);
      const { revision: _originalRevision, status: _originalStatus, reasonCodes: _originalReasons, ...originalFacts } = original;
      const { revision: _recoveredRevision, status: _recoveredStatus, reasonCodes: _recoveredReasons, ...recoveredFacts } = restored.currentDecision!;
      assert.deepEqual(recoveredFacts, originalFacts, 'All original economics, as-of, evidence and expiry survive; recovery does not refresh a historical target.');
      assert.equal(restored.targetCommands.length, 0);
      await state.restart(); restored = await read(state, admitted.mandate.id);
      assert.equal(Object.values(pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id).events).filter(item => item.kind === 'SOURCE_RECOVERED').length, 1);
      const sealedBefore = (await state.service.research.harness.snapshot(Exposures, BACKGROUND_CONTEXT))!;
      assert.equal(sealedBefore.windows.filter(item => item.kind === 'FINAL').length, 1,
        'This intent-revision counterexample requires the actual PM independent sample to have been consumed first.');
      const trialsBefore = structuredClone(restored.researchTrials.filter(item => item.countedAttempt));
      assert.equal(trialsBefore.length, 10, 'Original search, joint capital comparisons and both independent legs must remain charged.');
      const budgetBefore = structuredClone(pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id).budget);
      await writeFile(dataset.revisionManifest, '{second temporary unreadable native source');
      await waitForSource(value => value.currentDecision === null && value.gaps.some(item => item.requiredCapability === 'source_revision' && item.state === 'OPEN'));
      const ownerClarification = await state.service.app.inject({ method: 'POST',
        url: `/api/v3/mandates/${encodeURIComponent(admitted.mandate.id)}/intent-clarifications`, headers: state.headers,
        payload: { requestId: 'native-intent-after-independent-exposure', expectedRevision: restored.mandate.revision,
          statement: 'No leverage.', boundaries: [{ kind: 'LEVERAGE', sourceStatement: 'No leverage.',
            value: { maximumGrossNavRatio: '1', borrowingAllowed: false } }], resolvedAmbiguities: [] } });
      assert.equal(ownerClarification.statusCode, 202, ownerClarification.body);
      const revised = ownerClarification.json<PmMandateView>();
      assert.equal(revised.mandate.ownerGoal, restored.mandate.ownerGoal);
      assert.notEqual(revised.activePolicy.id, restored.activePolicy.id);
      assert.equal(revised.campaigns.at(-1)!.state, 'PAUSED');
      assert.equal(revised.campaigns.at(-1)!.conclusionReason, 'intent_revision_requires_new_unexposed_data');
      assert.equal(revised.history.find(item => item.id === original.id)!.status, 'SUPERSEDED');
      assert.deepEqual(revised.researchTrials.filter(item => item.countedAttempt), trialsBefore);
      assert.deepEqual((await state.service.research.harness.snapshot(Exposures, BACKGROUND_CONTEXT))!.windows, sealedBefore.windows);
      assert.deepEqual(pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id).budget, budgetBefore);
      await writeFile(dataset.revisionManifest, originalManifest);
      const afterPolicyRecovery = await waitForSource(value => value.gaps.filter(item => item.requiredCapability === 'source_revision').every(item => item.state === 'RESOLVED'));
      assert.equal(afterPolicyRecovery.currentDecision, null, 'Readable original bytes cannot revive P1 under the owner\'s new P2.');
      const policyRecoveryRecord = pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id);
      const recoveryEvents = Object.values(policyRecoveryRecord.events).filter(item => item.kind === 'SOURCE_RECOVERED');
      assert.equal(recoveryEvents.length, 2);
      assert.equal(policyRecoveryRecord.artifacts[recoveryEvents.at(-1)!.sourceRevisionRef]!.outcome, 'NEEDS_EVIDENCE');
      assert.deepEqual((await readFile(nativeLog, 'utf8')).split('\n').filter(command => command !== 'catalog-prefix-digest'),
        commandsBefore.filter(command => command !== 'catalog-prefix-digest'));
      // The original source is MARGIN: this authenticated new constraint must
      // not relabel the old study as a CASH/no-borrow proof or reuse its FINAL.
      assert.ok(revised.history.every(item => item.targetCommandId === null));
      await state.restart();
      const revisedRestored = await read(state, admitted.mandate.id);
      assert.equal(revisedRestored.campaigns.at(-1)!.state, 'PAUSED');
      assert.deepEqual(revisedRestored.researchTrials.filter(item => item.countedAttempt), trialsBefore);
      assert.deepEqual((await state.service.research.harness.snapshot(Exposures, BACKGROUND_CONTEXT))!.windows, sealedBefore.windows);
      t.diagnostic(JSON.stringify({ action: original.action, ownerNav: original.capital!.equity.amount,
        committedCapital: original.capital!.committedCapital.amount, testedCapitalPoints: view.capacityEstimates.flatMap(item => item.curve.map(point => point.capital.amount)),
        countedAttempts: view.researchTrials.filter(item => item.countedAttempt).length,
        independentEvidenceRef: (original.explanationFacts as JsonObject).independentEvidenceRef ?? null,
        scope: 'SYNTHETIC_NATIVE_PIPELINE_NOT_MARKET_PERFORMANCE_OR_LIVE_QUALIFICATION' }));
    } finally { process.env.QZ_JOB_BINARY = originalBinary; await state.cleanup(); }
  });

test('PM native source: a disjoint automatic data revision refits original panels and updates the decision without spending sealed data or resetting the budget',
  { timeout: 360_000, skip: !nativeFixtureDirectory || !nativeJob ? 'Requires both generated original native revisions and the matching job binary.' : false }, async t => {
    const sources = await Promise.all(['source.json', 'source-next.json'].map(async name => JSON.parse(await readFile(join(nativeFixtureDirectory!, name), 'utf8'))));
    const firstEnd = BigInt(sources[0].heldoutData.request.forecast.selection.event_end_ns);
    const nextStart = BigInt(sources[1].data.request.forecast.selection.event_start_ns);
    assert.ok(nextStart > firstEnd, 'The next TRAIN/VALIDATION must not reuse any prior sealed interval.');
    assert.deepEqual(sources[0].policy, sources[1].policy, 'This test changes data, not the owner preference or registered strategy method.');
    const state = await setup();
    try {
      const dataset = state.options.config.datasets[0]!;
      const root = join(state.directory, 'native-revision-source'); await mkdir(root);
      const manifestPath = join(root, 'revision.json'); dataset.revisionManifest = manifestPath;
      dataset.strategy = sources[0].policy;
      state.options.config.jobBinary = nativeJob!;
      state.options.extensions = [research, factors, fittedEnsemble];
      async function publish(index: number) {
        const source = sources[index]; const revision = `native-original-${index + 1}`;
        await cp(source.catalog, join(root, revision), { recursive: true, errorOnExist: true });
        const files: { path: string; bytes: string; sha256: string }[] = [];
        async function visit(directory: string) {
          for (const entry of (await readdir(join(root, directory), { withFileTypes: true })).sort((a, b) => a.name.localeCompare(b.name))) {
            const path = `${directory}/${entry.name}`;
            if (entry.isDirectory()) await visit(path);
            else { const bytes = await readFile(join(root, path)); files.push({ path, bytes: String(bytes.length), sha256: createHash('sha256').update(bytes).digest('hex') }); }
          }
        }
        await visit(revision);
        assert.ok(files.some(file => file.path.endsWith('.parquet')), 'The manifest must bind real native market files.');
        const body = { schemaVersion: 1, datasetId: dataset.id, sourceRevision: revision,
          availableAtNs: (BigInt(Date.now()) * 1_000_000n).toString(), revisionKind: 'APPEND',
          data: { ...source.data, catalog: revision, sealed: { ...source.heldoutData, catalog: revision } }, files };
        await writeFile(`${manifestPath}.next`, JSON.stringify({ ...body, contentDigest: createHash('sha256').update(canonical(body)).digest('hex') }));
        await rename(`${manifestPath}.next`, manifestPath);
      }
      await publish(0); await state.restart(); scriptNativeResearch(state, sources[0]);
      const admitted = await start(state, { ...mandateInput(state, 'native-two-revision-lifecycle'),
        goal: 'Continuously research causal predictive alpha when new bar data arrives; compare matched daily net account outcomes with cash.',
        funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, businessPreference: 'NET_RETURN', maxTrials: 32 });
      async function completedRevision(expectedCampaigns: number) {
        let view = await read(state, admitted.mandate.id); const waited = new Set<number>(); const deadline = Date.now() + 150_000;
        while (Date.now() < deadline) {
          view = await read(state, admitted.mandate.id);
          const campaign = view.campaigns.at(-1);
          if (view.campaigns.length === expectedCampaigns && campaign && campaign.runtime.conversationId > 0) {
            const study = await state.service.research.state(campaign.runtime.conversationId);
            const failures = study.research?.trials.filter(trial => trial.status === 'FAILED') ?? [];
            assert.equal(failures.length, 0, JSON.stringify(failures.map(trial => ({ taskId: trial.taskId, error: trial.error, detail: trial.errorDetail }))));
            const taskId = view.lastCycle?.runtime.taskId;
            if (taskId && !waited.has(taskId)) { waited.add(taskId); await state.service.research.harness.waitForTask(taskId as TaskId, BACKGROUND_CONTEXT); continue; }
            if (view.currentDecision && ['REBALANCE', 'CASH'].includes(view.currentDecision.action)
              && view.currentDecision.id === view.lastCycle?.outputDecisionId
              && view.lastCycle?.datasetRevisionIds.some(id => campaign.datasetRevisionIds.includes(id))) {
              const record = pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id);
              const independentTaskId = record.artifacts[`independent-task:${view.currentDecision.id}`]?.taskId;
              if (view.currentDecision.action === 'REBALANCE') {
                assert.equal(typeof independentTaskId, 'number');
                const task = await state.service.research.harness.waitForTask(independentTaskId as TaskId, BACKGROUND_CONTEXT);
                const failedState = pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id);
                assert.equal(task.state.status === 'terminal' && task.state.outcome.status, 'completed', JSON.stringify({ state: task.state,
                  errors: Object.entries(failedState.artifacts).filter(([key, item]) => key.startsWith('pm-error:') || key.startsWith('independent-error:') || !!item.stage) }));
              }
              return await read(state, admitted.mandate.id);
            }
          }
          await new Promise(resolve => setTimeout(resolve, 50));
        }
        const record = pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id);
        assert.fail(JSON.stringify({ reason: 'native_revision_never_completed', view, errors: Object.values(record.artifacts).filter(item => item.stage) }));
      }
      async function originalNativeSource(view: PmMandateView): Promise<JsonObject> {
        const record = pmRecord(pmState(await state.service.pm.snapshot()), admitted.mandate.id);
        const binding = record.artifacts[view.lastCycle!.id]!.nativeSource as JsonObject;
        assert.equal(typeof binding.originalEvidenceRef, 'string', 'The selected native source must retain its original child evidence identity.');
        const response = await state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(String(binding.originalEvidenceRef))}`, headers: state.headers });
        assert.equal(response.statusCode, 200, response.body);
        const original = response.json<{ reference: { id: string; visibility: string }; payload: JsonObject }>();
        assert.equal(original.reference.id, binding.originalEvidenceRef);
        assert.equal(original.reference.visibility, 'OWNER_ONLY');
        return original.payload;
      }
      const first = await completedRevision(1);
      assert.equal(first.currentDecision!.action, 'REBALANCE', 'This controlled upward-price native fixture must actually select and independently evaluate a risky candidate.');
      const firstDecision = structuredClone(first.currentDecision!);
      const firstCount = first.researchTrials.filter(trial => trial.countedAttempt).length;
      const firstNative = await originalNativeSource(first);
      assert.deepEqual(firstNative.catalogPrefix, sources[0].catalogPrefix);
      const firstFit = firstNative.fit as JsonObject;
      const firstFinals = (await state.service.research.harness.snapshot(Exposures, BACKGROUND_CONTEXT))!.windows.filter(item => item.kind === 'FINAL');
      assert.equal(firstFinals.length, 1);

      scriptNativeResearch(state, sources[1]); await publish(1);
      // No refresh/recompute request: the official monitor must discover this source.
      const second = await completedRevision(2);
      const nextDecision = second.currentDecision!;
      const nextNative = await originalNativeSource(second);
      assert.deepEqual(nextNative.catalogPrefix, sources[1].catalogPrefix);
      const nextFit = nextNative.fit as JsonObject;
      assert.notEqual(nextDecision.id, firstDecision.id);
      assert.notEqual(nextDecision.strategyVersionId, firstDecision.strategyVersionId);
      assert.notEqual(nextDecision.ensembleRevisionId, firstDecision.ensembleRevisionId);
      assert.equal(second.history.find(decision => decision.id === firstDecision.id)!.status, 'SUPERSEDED');
      assert.equal(nextDecision.targetCommandId, null, 'A new historical source does not authorize a current target.');
      assert.equal(nextFit.fit_end_ns, sources[1].data.request.train_end_ns);
      assert.ok(BigInt(String(nextFit.fit_end_ns)) > firstEnd);
      const coefficients = (fit: JsonObject) => (fit.members as JsonObject[]).flatMap(member => (member.calibrations as JsonObject[])
        .map(calibration => ({ intercept: calibration.intercept, slope: calibration.slope })));
      assert.notDeepEqual(coefficients(nextFit), coefficients(firstFit),
        'New native calibration evidence must come from the changed raw prices, not merely a new revision label or timestamp.');
      const nextFitRequest = nextNative.fitRequest as JsonObject;
      for (const member of nextFitRequest.members as JsonObject[]) {
        const report = member.report as JsonObject;
        const request = report.request as JsonObject;
        assert.equal(request.train_end_ns, sources[1].data.request.train_end_ns);
        assert.equal(((request.forecast as JsonObject).selection as JsonObject).event_start_ns, nextStart.toString());
        const priorRefs = ((firstNative.fitRequest as JsonObject).members as JsonObject[]).map(prior => prior.source_evidence_ref);
        assert.ok(!priorRefs.includes(member.source_evidence_ref), 'The new panel must identify its actual new native task.');
      }
      const finalWindows = (await state.service.research.harness.snapshot(Exposures, BACKGROUND_CONTEXT))!.windows.filter(item => item.kind === 'FINAL');
      assert.deepEqual(finalWindows.filter(item => item.taskId === firstFinals[0]!.taskId), firstFinals,
        'The original sealed exposure remains immutable after discovery and refitting.');
      assert.equal(finalWindows.length, nextDecision.action === 'REBALANCE' ? 2 : 1);
      if (finalWindows.length === 2) assert.ok(BigInt(String(finalWindows[1]!.start)) >= firstEnd);
      const total = second.researchTrials.filter(trial => trial.countedAttempt).length;
      assert.ok(firstCount > 0 && total > firstCount && total <= 32);
      assert.equal(second.searchFamily.countedAttempts, String(total));
      assert.ok(second.researchTrials.filter(trial => trial.campaignId === second.campaigns[1]!.id).some(trial => trial.purpose === 'ENSEMBLE_FIT' && trial.state === 'COMPUTED'));
      assert.ok(second.capacityEstimates.length > first.capacityEstimates.length, 'The new decision must actually re-evaluate capital configurations.');
      for (const capacity of second.capacityEstimates) for (const point of capacity.curve.filter(point => point.status === 'AVAILABLE')) {
        assert.equal(assessCapacityCurve(capacity, point.capital).feasible, true);
      }
      assert.deepEqual(second.activePolicy.selectionRule, first.activePolicy.selectionRule, 'Data updates cannot silently retune the owner objective.');
      t.diagnostic(JSON.stringify({ scope: 'SYNTHETIC_TWO_NATIVE_REVISIONS_NOT_MARKET_QUALIFICATION', actions: [firstDecision.action, nextDecision.action],
        firstAttempts: firstCount, totalAttempts: total, fitCutoffs: [firstFit.fit_end_ns, nextFit.fit_end_ns], independentWindows: finalWindows.length }));
    } finally { await state.cleanup(); }
  });
