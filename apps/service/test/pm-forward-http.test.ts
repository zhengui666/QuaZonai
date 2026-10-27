import assert from 'node:assert/strict';
import { test } from 'node:test';
import { spawn } from 'node:child_process';
import { access, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { PmMandateView } from '../../../packages/contracts/src/pm.ts';
import { compareDecimal, pmCanonical } from '../../../packages/contracts/src/pm-validation.ts';
import research from '../../research/plugins/research.ts';
import factors from '../../research/plugins/factor-wasm.ts';
import fittedEnsemble from '../../research/plugins/fitted-ensemble.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { Application, json, object, observation } from '../src/facts.ts';
import type { ObservationFact } from '../src/facts.ts';
import { accountKey } from '../src/capital-exit.ts';
import { publishDatasetRevision } from '../src/dataset-revision-publish.ts';
import { paperReceiver } from './pm-paper-receiver.ts';
import { pmRecord, pmState } from '../src/pm-facts.ts';
import { observedSpotFixture } from './pm-observed-fixture.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup } from './support.ts';

const available = !!process.env.QZ_PM_OBSERVED_FIXTURE_DIR && !!process.env.QZ_JOB_BINARY;
const child = process.env.QZ_PM_FORWARD_HTTP_CHILD === '1';
const paper = process.env.QZ_PM_FORWARD_HTTP_MODE === 'PAPER';
// The measured full-environment RO run took 321.50 seconds and began current
// computation around second 212, close to its former 220-second phase limit.
// Use the existing PAPER technical bounds for both modes. This is a finite
// test watchdog, not a source freshness extension or a throughput guarantee.
const options = { timeout: 420_000, skip: !available ? 'Requires the full cash-heavy hourly-label native source and matching job; no scientific fixture is synthesized.' : false };
const call = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function result(transcript: { messages: readonly unknown[] }, name: string, offset = -1): JsonObject {
  const entry = (transcript.messages.filter(value => (value as { role?: string; toolName?: string }).role === 'toolResult'
    && (value as { toolName?: string }).toolName === name) as { isError?: boolean; content: { type: string; text?: string }[] }[]).at(offset);
  assert.ok(entry); const text = entry.content.filter(value => value.type === 'text').map(value => value.text).join('');
  assert.ok(!entry.isError, `${name}: ${text}`); return object(JSON.parse(text));
}
function script(state: Awaited<ReturnType<typeof setup>>, source: JsonObject) {
  let hypothesisId = ''; const members = source.members as JsonObject[];
  state.faux.setResponses([
    call('strategy_intake', { name: 'Causal signals against original spot inventory', archetype: 'PREDICTIVE_ALPHA',
      economicMechanism: 'Compare trained causal price forecasts in the actual native shared spot account against retaining original quantities.',
      falsificationConditions: ['The frozen method fails the original matched account preference or independent protocol.'], requiredCapabilities: [],
      intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['fees', 'selection_bias'], ambiguities: [], ownerBoundaries: [] }),
    call('research_hypothesis', { economicFamilyId: 'controlled:causal-price', mechanism: 'Original causal price features predict the original future-return label.',
      prediction: 'The original fitted combination may improve matched account evidence.', falsification: 'Independent matched account evidence does not improve.',
      requiredDataCapabilities: ['BAR'], incrementalQuestion: 'Does the original joint model improve evidence beyond retaining actual inventory?' }),
    transcript => { hypothesisId = String(result(transcript, 'research_hypothesis').id); return call('factor_evaluate', { hypothesisId, hypothesis: 'Original close feature', model: members[0]!.model! }); },
    () => call('factor_evaluate', { hypothesisId, hypothesis: 'Original inverse-price feature', model: members[1]!.model! }),
    transcript => call('strategy_ensemble_fit', { hypothesisId, hypothesis: 'Fit only original mature TRAIN labels.',
      sourceTrialIds: [result(transcript, 'factor_evaluate', -2).trialId!, result(transcript, 'factor_evaluate').trialId!] }),
    transcript => call('strategy_ensemble_evaluate', { hypothesisId, hypothesis: 'Native joint validation of the learned original combination.',
      fitTrialId: result(transcript, 'strategy_ensemble_fit').trialId!, instrumentId: String(object(source.policy).instrumentId) }),
    transcript => { result(transcript, 'strategy_ensemble_evaluate'); return call('research_finish', { reason: 'Original planned factor, fit and joint trials are complete.' }); },
  ]);
}
async function exists(path: string) { try { await access(path); return true; } catch { return false; } }

if (!child) { for (const mode of ['RESEARCH_ONLY', 'PAPER']) test(`PM forward ${mode} HTTP recovery runs in an isolated archival software clock process`, options, async () => {
  console.log(JSON.stringify({ scope: 'ORIGINAL_FORWARD_HTTP_RUNTIME', mode, node: process.version, execPath: process.execPath }));
  await access(join(process.env.QZ_PM_OBSERVED_FIXTURE_DIR!, 'science', 'cash-heavy-hour', 'source.json'));
  const output = await new Promise<{ code: number | null; log: string }>((resolve, reject) => {
    const environment: NodeJS.ProcessEnv = { ...process.env, QZ_PM_FORWARD_HTTP_CHILD: '1', QZ_PM_FORWARD_HTTP_MODE: mode };
    // Node marks its test workers; this is an independent runner with its own
    // isolated software clock, not a recursive worker of the parent suite.
    delete environment.NODE_TEST_CONTEXT;
    const processChild = spawn(process.execPath, ['--experimental-strip-types', '--test', fileURLToPath(import.meta.url)], {
      env: environment, stdio: ['ignore', 'pipe', 'pipe'],
    });
    let log = ''; processChild.stdout.on('data', value => { log += value; }); processChild.stderr.on('data', value => { log += value; });
    // PAPER includes at most one additional original one-minute source wait.
    // Both modes share a finite process watchdog; financial deadlines remain
    // the original source facts and are independently asserted below.
    const timer = setTimeout(() => { processChild.kill('SIGTERM'); reject(new Error(`forward_http_child_timeout\n${log.slice(-12000)}`)); }, 410_000);
    processChild.on('error', error => { clearTimeout(timer); reject(error); });
    processChild.on('close', code => { clearTimeout(timer); resolve({ code, log }); });
  });
  assert.equal(output.code, 0, output.log);
  assert.match(output.log, /ARCHIVAL_SOFTWARE_CONTROLLED_CLOCK_FORWARD_RECOVERY_PASS/);
  if (mode === 'PAPER') assert.match(output.log, /ORIGINAL_PAPER_TARGET_RECEIPT_RECOVERY_PASS/);
  for (const line of output.log.split('\n')) if (line.includes('THIS_CONTROLLED_FRESH_SOURCE_WINDOW_NOT_A_PRODUCTION_THROUGHPUT_GUARANTEE')) console.log(line);
}); }
else test('actual learned ACCOUNT method survives independent evaluation and original current-task recovery without another trial or sealed read', options, async t => {
  const f = await observedSpotFixture(join(process.env.QZ_PM_OBSERVED_FIXTURE_DIR!, 'science', 'cash-heavy-hour'));
  const current = object(f.source.current_initial_state); const realNow = Date.now.bind(Date);
  const offset = realNow() - Number(BigInt(String(current.open_orders_available_ns)) / 1_000_000n) - 1000;
  // This child process alone uses an archival software execution clock. It
  // advances naturally; no native event, availability, TTL or timer is changed.
  t.mock.method(Date, 'now', () => realNow() - offset);
  const state = await setup(); let mandateId: string | null = null; let completed = false;
  let remote: Awaited<ReturnType<typeof paperReceiver>> | null = null;
  const directory = state.directory; const logPath = join(directory, 'native-calls.log');
  const pausePath = join(directory, 'pause-after-forward-receipt'); const reachedPath = join(directory, 'original-receipt-saved');
  const builtPath = join(directory, 'native-build-returned'); const binary = join(directory, 'controlled-process-boundary.mjs');
  const actualBinary = process.env.QZ_JOB_BINARY!;
  // The executable always invokes the original job unchanged. Only the next
  // read-only prefix verification waits, after its genuine build result has
  // reached the official memo. Closing the service exercises real cancellation.
  await writeFile(binary, `#!${process.execPath}\nimport {spawnSync} from 'node:child_process';\nimport {appendFileSync,existsSync,writeFileSync} from 'node:fs';\nconst args=process.argv.slice(2);\nif(args[0]==='catalog-prefix-digest'&&existsSync(${JSON.stringify(builtPath)})&&existsSync(${JSON.stringify(pausePath)})){writeFileSync(${JSON.stringify(reachedPath)},'saved');while(existsSync(${JSON.stringify(pausePath)}))await new Promise(r=>setTimeout(r,20));}\nappendFileSync(${JSON.stringify(logPath)},args[0]+'\\n');\nconst started=process.hrtime.bigint();\nconst result=spawnSync(${JSON.stringify(actualBinary)},args,{stdio:'inherit'});\nif(args[0]==='build-observed-spot'&&result.status===0)writeFileSync(${JSON.stringify(builtPath)},JSON.stringify({elapsedNs:(process.hrtime.bigint()-started).toString()}));\nprocess.exit(result.status??1);\n`, { mode: 0o700 });
  await writeFile(pausePath, 'pause original next verification');
  const originalEnvironmentBinary = process.env.QZ_JOB_BINARY; process.env.QZ_JOB_BINARY = binary;
  try {
    const source = object(f.source.fittedSource); const dataset = state.options.config.datasets[0]!;
    dataset.currency = String(f.settings.base_currency); dataset.data = { ...object(source.data), sealed: object(source.heldoutData) };
    dataset.strategy = object(source.policy) as unknown as NonNullable<typeof dataset.strategy>;
    {
      // The model and immutable source are registered before research results.
      // TEST source history remains UNVERIFIED and cannot confer live authority.
      remote = await paperReceiver({ source: f.source, directory, connection: f.connection,
        originalSnapshot: object(object(f.application.observations[f.connection.id]).snapshot) });
      dataset.revisionManifest = join(directory, 'paper-source', 'current.json');
      const published = await publishDatasetRevision(dataset, { kind: 'ARCHIVAL_SOFTWARE_PAPER_SOURCE',
        originalExecutionModel: remote.registration, originalSource: f.source.contentRef!,
        receiptPolicy: 'ORIGINAL_NATIVE_PER_ROW_AVAILABILITY_AT_ACTUAL_CUTOFF_NOT_HISTORICAL_PIT' });
      dataset.data = published.configured.data;
    }
    state.options.config.jobBinary = binary; state.options.config.connections = [f.connection]; state.options.extensions = [research, factors, fittedEnsemble];
    await state.restart();
    const original = object(f.application.observations[f.connection.id]);
    const archival: ObservationFact = { input: { requestId: 'original-archival-source', portfolioId: 'original-archival-source', mode: 'paper', connectionId: f.connection.id },
      connection: f.connection, taskId: 0, target: {}, snapshot: json(original.snapshot), snapshotReceivedAtNs: String(original.snapshotReceivedAtNs),
      view: { id: f.connection.id, name: 'ARCHIVAL_SOFTWARE_CONTROLLED_CLOCK', portfolioId: 'original-archival-source', mode: 'paper', status: 'STOPPED',
        currency: dataset.currency, equity: null, cash: null, positions: [], curve: [], updatedAt: null, error: null, receipt: null } };
    await state.service.research.harness.commit(async tx => { const app = await tx.doc(Application); app.observations[f.connection.id] = json(archival);
      app.accountControls = { [accountKey(f.connection)]: json(Object.values(f.application.accountControls)[0]!) }; }, context);
    script(state, source);
    const admitted = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers, payload: {
      requestId: 'actual-cash-heavy-current', goal: 'Research predictive alpha from causal factors and compare matched net spot account evidence. No leverage.',
      datasetId: dataset.id, funding: { kind: 'ACCOUNT', accountBindingId: f.connection.id }, model: state.model, maxTrials: 12,
      businessPreference: 'NET_RETURN', operatingAuthority: paper ? 'AUTHORIZED_TARGETS' : 'RESEARCH_ONLY' } });
    assert.equal(admitted.statusCode, 202, admitted.body); mandateId = admitted.json<PmMandateView>().mandate.id;
    const read = async () => { const response = await state.service.app.inject({ url: `/api/v3/mandates/${encodeURIComponent(mandateId!)}`, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body); return response.json<PmMandateView>(); };
    const record = async () => pmRecord(pmState(await state.service.pm.snapshot()), mandateId!);
    const diagnose = async (stage: string, value: Awaited<ReturnType<typeof record>>) => {
      // Failure-only bounded diagnostics: no raw native tapes, full policy,
      // ledger, account balances or model payloads are dumped into CI output.
      const cycles = Object.values(value.cycles).slice(-12);
      const taskIds = [...new Set([value.monitorTaskId, ...cycles.map(cycle => cycle.runtime.taskId)])].filter((id): id is number => typeof id === 'number');
      const tasks = await Promise.all(taskIds.map(async id => {
        try {
          const task = await state.service.research.harness.getTask(id as TaskId, context);
          const storedState = object(json(task?.state ?? {}));
          return { id, status: storedState.status ?? null, phase: storedState.checkpoint ? object(storedState.checkpoint).phase ?? null : null,
            outcomeStatus: storedState.outcome ? object(storedState.outcome).status ?? null : null };
        } catch { return { id, readFailure: true }; }
      }));
      const basis = (input: unknown) => {
        if (!input) return null;
        const row = object(input);
        return Object.fromEntries(['accountBindingId', 'accountSourceId', 'observationId', 'sequence', 'observedAtNs', 'availableAtNs',
          'controlRevision', 'positionsEvidenceRef', 'outstandingCommandsRef'].map(key => [key, row[key] ?? null]));
      };
      const pending = value.artifacts['latest-account-pending']; const capture = pending?.capture ? object(pending.capture) : null;
      const capturedArtifact = capture?.artifact ? object(capture.artifact) : null;
      const app = await state.service.research.harness.snapshot(Application, context);
      const observed = app?.observations[f.connection.id] ? observation(app.observations[f.connection.id]) : null;
      const snapshot = observed?.snapshot ? object(observed.snapshot) : null;
      return JSON.stringify({ stage, fixtureFlow: { currentPublished, sourceEpoch, sourceResumed, sourceWaitMs, expectedFreshPriceNs },
        maintenance: await state.service.maintenance.status(),
        runtime: { node: process.version, atNs: (BigInt(Date.now()) * 1_000_000n).toString(), realNowMs: realNow() },
        currentDecisionId: value.currentDecisionId, latestCycleId: value.latestCycleId, basisRevision: value.basisRevision,
        policy: { id: value.policy.id, operatingRuleId: object(object(value.policy.validDomain).operatingRule ?? {}).id ?? null,
          allocationMethodId: object(object(value.policy.validDomain).jointAllocationMethod ?? {}).id ?? null },
        counts: Object.fromEntries(['campaigns', 'cycles', 'decisions', 'events', 'gaps'].map(key => [key, Object.keys(value[key as keyof typeof value] as object).length])),
        cycles: cycles.map(cycle => ({ id: cycle.id, state: cycle.state, taskId: cycle.runtime.taskId, asOfNs: cycle.asOfNs,
          availableCutoffNs: cycle.availableCutoffNs, outputDecisionId: cycle.outputDecisionId, reasonCodes: cycle.reasonCodes.slice(-8),
          matchesCurrentBasis: cycle.basisRevision === value.basisRevision, matchesCurrentPolicy: cycle.policyRevisionId === value.policy.id,
          operatingForward: value.artifacts[cycle.id]?.operatingForward ?? null,
          forwardSeedDecisionId: value.artifacts[cycle.id]?.forwardSeedDecisionId ?? null, accountBasis: basis(cycle.accountBasis) })), tasks,
        forwardSlots: Object.entries(value.artifacts).filter(([key]) => key.startsWith('forward-slot:')).slice(-12).map(([key, slot]) =>
          ({ key, cycleId: slot.cycleId, seedDecisionId: slot.seedDecisionId, slot: slot.slot, policyRevisionId: slot.policyRevisionId })),
        decisions: Object.values(value.decisions).slice(-8).map(decision => {
          const explanation = object(decision.explanationFacts); const frozen = value.artifacts[`independent-input:${decision.id}`];
          return { id: decision.id, cycleId: decision.cycleId, action: decision.action, status: decision.status, reasonCodes: decision.reasonCodes.slice(-8),
            policyRevisionId: decision.policyRevisionId, matchesCurrentPolicy: decision.policyRevisionId === value.policy.id,
            frozenIndependentPolicyMatchesCurrent: !!frozen?.policy && pmCanonical(frozen.policy) === pmCanonical(value.policy),
            forwardSeedEvidenceRef: explanation.forwardSeedEvidenceRef ?? null, independentEvidenceRef: explanation.independentEvidenceRef ?? null,
            economicQualification: object(explanation.economicQualification ?? {}).outcome ?? null,
            forwardAnchorNs: explanation.forwardAnchorNs ?? null, forwardTtlSeconds: explanation.forwardTtlSeconds ?? null,
            strategyControl: value.artifacts[`strategy-control:${decision.strategyVersionId}`]?.action ?? null };
        }),
        latestAccountPending: { eventKind: pending?.eventKind ?? null, basis: basis(capture?.basis),
          snapshotReceivedAtNs: capturedArtifact?.snapshotReceivedAtNs ?? null, capitalControlReceivedAtNs: capturedArtifact?.capitalControlReceivedAtNs ?? null,
          capabilitiesReceivedAtNs: capturedArtifact?.capitalCapabilities ? object(capturedArtifact.capitalCapabilities).receivedAtNs ?? null : null },
        originalLatestAccount: { snapshotReceivedAtNs: observed?.snapshotReceivedAtNs ?? null,
          ...Object.fromEntries(['accountSourceId', 'observationId', 'sequence', 'nativeClockNs', 'controlRevision'].map(key => [key, snapshot?.[key] ?? null])),
          transportError: observed?.view.error ?? null },
        events: Object.values(value.events).slice(-12).map(event => ({ id: event.id, kind: event.kind, availableAtNs: event.availableAtNs, sourceRevisionRef: event.sourceRevisionRef })),
        gaps: Object.values(value.gaps).slice(-16).map(gap => ({ capability: gap.requiredCapability, reasonCode: gap.reasonCode, state: gap.state })),
        errors: Object.values(value.artifacts).filter(item => item.stage).slice(-8).map(item => ({
          stage: typeof item.stage === 'string' ? item.stage.slice(0, 160) : null,
          reason: typeof item.reason === 'string' ? item.reason.slice(0, 500) : typeof item.error === 'string' ? item.error.slice(0, 500) : null })),
        nativeCalls: await exists(logPath) ? (await readFile(logPath, 'utf8')).trim().split('\n').slice(-16) : [] });
    };
    let currentPublished = false; let sourceEpoch: string | null = null; let sourceResumed = false;
    let expectedFreshPriceNs: string | null = null;
    let sourceWaitMs = 0;
    // Share the existing PAPER receipt watchdog: the measured RO solve began
    // at approximately 212 seconds. PAPER may also wait one original source
    // interval; neither mode changes the native 65-second price-age policy.
    let deadline = realNow() + 290_000;
    while (!(await exists(reachedPath)) && realNow() < deadline) {
      const stored = await record(); const view = await read();
      if (!currentPublished && view.cycles.some(cycle => cycle.state === 'COMPUTING')) {
        // Publish the next actual native frame through the real source HTTP
        // interface, retaining receiver time. No native balances are edited.
        remote!.publishCurrent(current);
        // Original read-only source refresh fetches account, capabilities and
        // capital state. No invented prior portfolio, target or fill exists.
        const refreshed = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${encodeURIComponent(f.connection.id)}/refresh`, headers: state.headers });
        assert.equal(refreshed.statusCode, 200, refreshed.body);
        const observed = observation((await state.service.research.harness.snapshot(Application, context))!.observations[f.connection.id]);
        assert.equal(observed.snapshot!.sequence, '2', JSON.stringify(observed.view));
        const capital = await state.service.app.inject({ url: `/api/v3/observations/${encodeURIComponent(f.connection.id)}/capital-exits`, headers: state.headers });
        assert.equal(capital.statusCode, 200, capital.body); assert.equal(compareDecimal(capital.json().reserved, '0'), 0);
        currentPublished = true;
        if (paper) {
          // Drain the already admitted historical selection/independent work.
          // A new current solve must not depend on which second that lengthy
          // research happens to finish within the original candle interval.
          const draining = await state.service.app.inject({ method: 'POST', url: '/api/v3/maintenance/enter', headers: state.headers,
            payload: { requestId: 'paper-wait-original-next-source', reason: 'Drain admitted research before the next original market receipt' } });
          assert.equal(draining.statusCode, 202, draining.body); sourceEpoch = draining.json().receipt.epoch;
        }
      }
      if (view.cycles.some(cycle => cycle.state === 'FAILED')) assert.fail(await diagnose('original_cycle_failed', stored));
      const settled = view.cycles.filter(cycle => cycle.outputDecisionId);
      if (settled.length && !view.cycles.some(cycle => ['PENDING', 'COMPUTING', 'EVALUATING'].includes(cycle.state))) {
        if (!Object.values(stored.decisions).some(decision => object(object(decision.explanationFacts).economicQualification ?? {}).outcome === 'READY'))
          assert.fail(await diagnose('original_native_method_did_not_qualify', stored));
      }
      if (paper && sourceEpoch && !sourceResumed && Object.values(stored.decisions).some(decision =>
        object(object(decision.explanationFacts).economicQualification ?? {}).outcome === 'READY')) {
        const maintenance = await state.service.maintenance.status();
        if (maintenance.state === 'QUIESCED') {
          assert.equal(maintenance.epoch, sourceEpoch); assert.ok(maintenance.parkedTaskIds.includes(stored.monitorTaskId!));
          assert.equal((await readFile(logPath, 'utf8')).split('\n').filter(value => value === 'build-observed-spot').length, 0);
          const scientificLedger = json(stored.derivedTrials), scientificBudget = json(stored.budgetMovements);
          const scientificExposure = json((await state.service.research.harness.snapshot(Exposures, context))?.windows ?? []);
          const metadata = object(JSON.parse(await readFile(join(String(f.source.catalog), 'qz-spot-cash-source.json'), 'utf8')));
          const waitingAtNs = BigInt(Date.now()) * 1_000_000n;
          const next = (metadata.responses as JsonObject[]).find(row => BigInt(String(row.received_ns)) > waitingAtNs);
          assert.ok(next, 'The original immutable source must contain a later genuine receipt; no clock or market row is synthesized.');
          const receiptNs = BigInt(String(next.received_ns));
          assert.ok(receiptNs - waitingAtNs <= 61_000_000_000n, 'Wait only for the original next one-minute source receipt.');
          const originalBar = object((next.response as JsonObject[]).at(-1));
          expectedFreshPriceNs = ((BigInt(String(originalBar.T)) + 1n) * 1_000_000n).toString();
          const sourceWaitStartedAt = realNow();
          while (BigInt(Date.now()) * 1_000_000n < receiptNs) await new Promise(resolve => setTimeout(resolve, 50));
          // Waiting for the original market receipt is separate from polling
          // native progress. Keep that evidenced wait bounded to one interval;
          // the parent process watchdog and all financial deadlines still apply.
          sourceWaitMs = Math.min(61_000, realNow() - sourceWaitStartedAt);
          deadline += sourceWaitMs;
          // No new account frame/proof is published and no evidence deadline
          // is renewed. Resume the original parked monitor at the actual clock.
          const resumedAtNs = (BigInt(Date.now()) * 1_000_000n).toString();
          const resumed: { statusCode: number; body: string } = await state.service.app.inject({ method: 'POST', url: '/api/v3/maintenance/resume', headers: state.headers,
            payload: { requestId: 'paper-original-source-arrived', epoch: sourceEpoch } });
          assert.equal(resumed.statusCode, 202, resumed.body); sourceResumed = true;
          const after = await record();
          assert.deepEqual(json(after.derivedTrials), scientificLedger); assert.deepEqual(json(after.budgetMovements), scientificBudget);
          assert.deepEqual(json((await state.service.research.harness.snapshot(Exposures, context))?.windows ?? []), scientificExposure);
          await writeFile(join(directory, 'paper-current-source-timing.json'), JSON.stringify({ schemaVersion: 1,
            scope: 'ARCHIVAL_SOFTWARE_CONTROLLED_CLOCK', originalMaintenanceEpoch: sourceEpoch, waitingAtNs: waitingAtNs.toString(),
            originalReceiptNs: receiptNs.toString(), originalPriceEventNs: expectedFreshPriceNs, resumedAtNs,
            originalMaximumPriceAgeNs: object(f.settings.multi_currency_spot_cash).maximum_price_age_ns }), { mode: 0o600 });
        }
      }
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    let stored = await record();
    if (!await exists(reachedPath)) assert.fail(await diagnose('original_current_receipt_not_reached', stored));
    const cycle = Object.values(stored.cycles).find(value => value.state === 'COMPUTING' && stored.artifacts[value.id]?.operatingForward === true)!;
    assert.ok(cycle); const originalTaskId = cycle.runtime.taskId!;
    const seed = Object.values(stored.decisions).find(value => typeof object(value.explanationFacts).forwardSeedEvidenceRef === 'string')!;
    assert.equal(object(object(seed.explanationFacts).economicQualification).outcome, 'READY');
    const campaign = Object.values(stored.campaigns)[0]!;
    const originalBasis = object(stored.artifacts[`campaign-account-start:${campaign.id}`]!.basis);
    assert.deepEqual(stored.cycles[seed.cycleId]!.accountBasis, originalBasis);
    assert.deepEqual(object(object(stored.artifacts[`independent-input:${seed.id}`]!.capture).basis), originalBasis);
    assert.equal(originalBasis.sequence, '1'); assert.equal(cycle.accountBasis!.sequence, '2');
    const evidence = async (id: string) => {
      const response = await state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(id)}`, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body); return response.json<{ payload: JsonObject }>().payload;
    };
    const assertHistoricalEvidence = async () => {
      // Expand the large historic reports after current delivery/recovery,
      // not while the test intentionally pauses a live freshness deadline.
      const seedEvidence = await evidence(String(object(seed.explanationFacts).forwardSeedEvidenceRef));
      const selectedEvidence = await evidence(seed.jointReplayEvidenceRef!);
      const independentEvidence = await evidence(String(object(seed.explanationFacts).independentEvidenceRef));
      const independentStudy = object((object(independentEvidence.evidence).studies as JsonObject[])[0]);
      for (const request of [object(object(selectedEvidence.comparison).request), object(object(independentStudy.paired).request)]) {
        assert.deepEqual(object(request.initial_state).snapshot, f.original.snapshot,
          'Historical selection and independent evidence retain their original campaign starting account despite current-source refresh.');
      }
      for (const request of [object(seedEvidence.study), object(object(object(independentStudy.paired).request).study)]) {
        assert.equal(object(object(request.mandate).constraints).cash_funding, 'PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1',
          'Both selection and actual native independent request must use the original funding method.');
        assert.equal(object(object(object(request.mandate).optimizer).parameters).cash_numerical_output, 'VALIDATED_FEASIBLE_SEGMENT_V1',
          'Selection and independent evaluation must register the same numerical output protocol before observing their outcomes.');
      }
    };
    const trials = json(stored.derivedTrials), movements = json(stored.budgetMovements);
    const exposures = json((await state.service.research.harness.snapshot(Exposures, context))?.windows ?? []);
    assert.ok(Array.isArray(exposures) && exposures.some(value => object(value).kind === 'FINAL'));
    await state.service.close(); await rm(pausePath); await state.restart();
    const restoredDeadline = realNow() + 45_000;
    while (realNow() < restoredDeadline) {
      stored = await record(); const value = stored.cycles[cycle.id]!;
      if (value.outputDecisionId) break;
      assert.notEqual(value.state, 'FAILED', JSON.stringify(Object.values(stored.artifacts).filter(value => value.stage)));
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    const restored = stored.cycles[cycle.id]!; assert.equal(restored.runtime.taskId, originalTaskId);
    assert.ok(restored.outputDecisionId); const decision = stored.decisions[restored.outputDecisionId]!;
    assert.ok(decision.solverEvidenceRef, JSON.stringify(decision));
    const comparison = stored.comparisons[decision.comparisonRef]!;
    assert.equal(comparison.evaluationContext, 'FORWARD_MODEL_ESTIMATE'); assert.ok(comparison.modeledComparison?.hold);
    assert.equal(comparison.modeledComparison!.forecastUnit, 'RETURN_PER_HORIZON');
    const originalNative = await state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(decision.solverEvidenceRef!)}`, headers: state.headers });
    assert.equal(originalNative.statusCode, 200, originalNative.body); const artifact = originalNative.json<{ payload: JsonObject }>().payload;
    assert.deepEqual(object(object(object(artifact.result).request).initial_state).snapshot, current.snapshot);
    assert.deepEqual(object(artifact.result).instrument_definitions, [remote!.definition], 'Native current rules must be the actual pre-research registered instrument, not result-derived source evidence.');
    assert.ok((object(object(object(artifact.result).build).input).assets as JsonObject[]).every(asset =>
      asset.transaction_cost_rate === object(remote!.registration.costs).commissionRate));
    assert.equal(object(object(artifact.result).request).selection && object(object(object(artifact.result).request).selection).decision_cutoff_ns, cycle.asOfNs);
    if (paper) {
      assert.ok(sourceResumed && expectedFreshPriceNs);
      assert.ok((object(artifact.result).trade_estimates as JsonObject[]).every(trade => BigInt(String(trade.price_event_ns)) >= BigInt(expectedFreshPriceNs!)),
        'The current solve must use the newly available original source mark after maintenance resumes.');
    }
    assert.deepEqual(json(stored.derivedTrials), trials); assert.deepEqual(json(stored.budgetMovements), movements);
    assert.deepEqual(json((await state.service.research.harness.snapshot(Exposures, context))?.windows ?? []), exposures);
    assert.equal((await readFile(logPath, 'utf8')).split('\n').filter(value => value === 'build-observed-spot').length, 1, 'Recovery must consume the original durable native receipt, not solve again.');
    const task = await state.service.research.harness.getTask(originalTaskId as TaskId, context);
    assert.equal(task!.state.status, 'terminal');
    assert.equal(object(object(object(artifact.result).request).mandate).constraints
      && object(object(object(object(artifact.result).request).mandate).constraints).cash_funding,
    'PRESERVE_MIN_CASH_AFTER_LINEAR_TURNOVER_V1');
    assert.equal(object(object(object(object(object(artifact.result).request).mandate).optimizer).parameters).cash_numerical_output,
      'VALIDATED_FEASIBLE_SEGMENT_V1', 'Current execution must retain the numerical protocol of the original independently evaluated method.');
    if (paper) {
      assert.ok(remote);
      const offeredDeadline = realNow() + 15_000;
      while (realNow() < offeredDeadline) {
        stored = await record();
        if (remote.posts && Object.values(stored.commands).some(command => command.decisionId === decision.id && command.state === 'UNCERTAIN')) break;
        await new Promise(resolve => setTimeout(resolve, 25));
      }
      const currentDecision = stored.decisions[decision.id]!;
      const assessment = stored.assessments[stored.strategies[currentDecision.strategyVersionId]!.pipelineAssessmentId]!;
      assert.equal(assessment.stage, 'PAPER', JSON.stringify({ currentDecision, assessment }));
      assert.equal(assessment.outcome, 'READY', JSON.stringify({ currentDecision, assessment }));
      const qualification = object(stored.artifacts[String(object(currentDecision.explanationFacts).executionQualificationEvidenceRef)]);
      const originalPriceDeadline = (qualification.deadlines as JsonObject[]).find(value => String(value.sourceKind).startsWith('ORIGINAL_MARK_PRICE_AND_INPUT_AGE:'))!;
      assert.ok(originalPriceDeadline);
      const price = (object(artifact.result).trade_estimates as JsonObject[])[0]!;
      assert.equal(BigInt(String(originalPriceDeadline.validUntilNs)), BigInt(String(price.price_event_ns))
        + BigInt(String(object(f.settings.multi_currency_spot_cash).maximum_price_age_ns)));
      assert.ok(BigInt(assessment.evaluatedAtNs) < BigInt(String(originalPriceDeadline.validUntilNs)), 'Positive delivery is only claimed within this original fresh-price window.');
      const measured = object(JSON.parse(await readFile(builtPath, 'utf8')));
      assert.ok(BigInt(String(measured.elapsedNs)) > 0n);
      console.log(JSON.stringify({ scope: 'THIS_CONTROLLED_FRESH_SOURCE_WINDOW_NOT_A_PRODUCTION_THROUGHPUT_GUARANTEE',
        nativeElapsedNs: measured.elapsedNs, cycleAsOfNs: cycle.asOfNs, evaluatedAtNs: assessment.evaluatedAtNs,
        originalPriceEventNs: price.price_event_ns, originalPriceValidUntilNs: originalPriceDeadline.validUntilNs }));
      assert.equal(object(object(stored.policy.validDomain).qualificationProtocol).maximumPromotion, 'SHADOW', 'Paper readiness never promotes the original method to live.');
      assert.ok(currentDecision.targetCommandId, JSON.stringify(currentDecision));
      const command = stored.commands[currentDecision.targetCommandId]!;
      assert.equal(command.state, 'UNCERTAIN'); assert.equal(remote.posts, 1); assert.equal(remote.requests.length, 1);
      const targetTaskId = Number(stored.artifacts[`target-task:${command.id}`]!.taskId) as TaskId;
      const originalDeadline = command.validUntilNs; const accepted = remote.receipts[0]!;
      assert.equal(originalDeadline, qualification.validUntilNs, 'Delivery retains the minimum original source deadline.');
      assert.equal(accepted.id, command.idempotencyKey);
      assert.ok(currentDecision.assetTargets.length > 0);
      assert.deepEqual(object(accepted.target).targets, object(object(artifact.result).target).targets);
      assert.equal(object(accepted.target).cash_weight, object(object(artifact.result).target).cash_weight);
      assert.deepEqual(object(accepted.allocation), stored.artifacts[`target-allocation:${command.id}`]);
      assert.equal(object(accepted.allocation).snapshotSequence, '2');
      assert.equal(object(accepted.allocation).nativeClockNs, object(current.snapshot).ts_event);
      const draining = await state.service.app.inject({ method: 'POST', url: '/api/v3/maintenance/enter', headers: state.headers,
        payload: { requestId: 'paper-original-uncertain-receipt', reason: 'Drain original paper target without cancellation or resubmission' } });
      assert.equal(draining.statusCode, 202, draining.body);
      assert.equal((await state.service.maintenance.status()).state, 'DRAINING', 'A lost POST response remains an external responsibility.');
      await state.service.close(); await state.restart(); remote.revealReceipts();
      const receiptDeadline = realNow() + 15_000;
      while (realNow() < receiptDeadline) {
        stored = await record(); if (stored.commands[command.id]!.state === 'ACKNOWLEDGED') break;
        await new Promise(resolve => setTimeout(resolve, 50));
      }
      const recovered = stored.commands[command.id]!; assert.equal(recovered.state, 'ACKNOWLEDGED');
      assert.equal(recovered.validUntilNs, originalDeadline, 'Recovery never renews source/target expiry.');
      assert.equal(stored.artifacts[`target-task:${command.id}`]!.taskId, targetTaskId);
      assert.deepEqual(stored.artifacts[recovered.externalReceiptRef!], accepted);
      assert.equal(remote.posts, 1, 'The original uncertainty checkpoint permits only GET after local restart.'); assert.ok(remote.gets >= 1);
      assert.deepEqual(json(stored.derivedTrials), trials); assert.deepEqual(json(stored.budgetMovements), movements);
      assert.deepEqual(json((await state.service.research.harness.snapshot(Exposures, context))?.windows ?? []), exposures);
      const targetTask = await state.service.research.harness.getTask(targetTaskId, context); assert.equal(targetTask!.state.status, 'terminal');
      const maintenanceDeadline = realNow() + 12_000;
      while (realNow() < maintenanceDeadline && (await state.service.maintenance.status()).state !== 'QUIESCED') await new Promise(resolve => setTimeout(resolve, 50));
      assert.equal((await state.service.maintenance.status()).state, 'QUIESCED');
      await assertHistoricalEvidence();
      console.log('ORIGINAL_PAPER_TARGET_RECEIPT_RECOVERY_PASS');
      console.log('ARCHIVAL_SOFTWARE_CONTROLLED_CLOCK_FORWARD_RECOVERY_PASS');
      completed = true;
      return;
    }
    assert.equal(decision.targetCommandId, null, 'Research-only authority never authorizes a target.');
    const counters = (value: typeof stored) => Object.fromEntries(['events', 'cycles', 'decisions', 'comparisons', 'evidence', 'outcomes', 'attributions']
      .map(key => [key, Object.keys(value[key as keyof typeof value] as object).length]));
    const beforeHeartbeat = counters(stored);
    for (let batch = 0; batch < 3; batch++) {
      const expectedSequence = String(102 + batch * 100);
      // Controlled transport heartbeats republish the SAME original native
      // frame; no balances, market event or native availability is synthesized.
      for (let index = 0; index < 100; index++) {
        const sequence = String(3 + batch * 100 + index);
        remote!.heartbeat(sequence);
        const response = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${encodeURIComponent(f.connection.id)}/refresh`, headers: state.headers });
        assert.equal(response.statusCode, 200, response.body);
      }
      const sampledDeadline = realNow() + 12_000;
      while (realNow() < sampledDeadline) {
        stored = await record();
        const pending = stored.artifacts['latest-account-pending'];
        if (pending && object(object(pending.capture).basis).sequence === expectedSequence) break;
        await new Promise(resolve => setTimeout(resolve, 50));
      }
      assert.equal(object(object(stored.artifacts['latest-account-pending']!.capture).basis).sequence, expectedSequence);
      assert.deepEqual(counters(stored), beforeHeartbeat, 'Same registered slot coalesces actual source heartbeats before appending business history.');
      assert.equal(stored.currentDecisionId, null, 'New account receipt still invalidates the original current basis immediately.');
      assert.equal(stored.decisions[decision.id]!.status, 'SUPERSEDED');
    }
    assert.equal((await readFile(logPath, 'utf8')).split('\n').filter(value => value === 'build-observed-spot').length, 1);
    assert.equal(remote!.posts, 0, 'A fully implemented paper receiver does not override research-only owner authority.');
    assert.deepEqual(json(stored.derivedTrials), trials); assert.deepEqual(json(stored.budgetMovements), movements);
    {
      // The original operating slot is already consumed. A real source outage
      // and recovery must preserve that claim, rather than manufacture a new
      // current solve or relabel the old account/independent evidence as fresh.
      const manifestPath = dataset.revisionManifest!;
      const originalManifest = await readFile(manifestPath, 'utf8');
      const originalCycles = json(stored.cycles), originalDecisions = json(stored.decisions);
      const originalDatasets = structuredClone((await read()).datasetRevisions);
      const originalPendingBasis = json(object(object(stored.artifacts['latest-account-pending']!.capture).basis));
      const originalAccount = observation((await state.service.research.harness.snapshot(Application, context))!.observations[f.connection.id]);
      const originalSnapshot = json(originalAccount.snapshot!), originalReceivedAtNs = originalAccount.snapshotReceivedAtNs;
      const originalSlots = json(Object.fromEntries(Object.entries(stored.artifacts).filter(([key]) => key.startsWith('forward-slot:'))));
      const recoveryIds = (value: typeof stored) => Object.values(value.events).filter(event => event.kind === 'SOURCE_RECOVERED').map(event => event.id);
      assert.deepEqual(recoveryIds(stored), []);
      const waitForSource = async (predicate: (value: typeof stored) => boolean) => {
        const limit = realNow() + 12_000;
        while (realNow() < limit) { stored = await record(); if (predicate(stored)) return; await new Promise(resolve => setTimeout(resolve, 50)); }
        assert.ok(predicate(stored), JSON.stringify({ health: stored.artifacts['source-health'], gaps: Object.values(stored.gaps), events: Object.values(stored.events) }));
      };
      try {
        await writeFile(manifestPath, '{unreadable source manifest');
        await waitForSource(value => value.artifacts['source-health']?.state === 'FAILED'
          && Object.values(value.gaps).some(gap => gap.requiredCapability === 'source_revision' && gap.state === 'OPEN'));
      } finally {
        // Restore the exact original manifest, not a republished revision or a
        // new availability clock. All native source bytes remain untouched.
        await writeFile(manifestPath, originalManifest);
      }
      const failureId = String(stored.artifacts['source-health']!.failureEventId);
      await waitForSource(value => recoveryIds(value).length === 1 && Object.values(value.gaps).some(gap =>
        gap.requiredCapability === 'source_recovery_operating' && gap.state !== 'RESOLVED'
        && gap.reasonCode === 'source_recovered_waiting_for_next_registered_operating_slot'));
      const recoveredId = recoveryIds(stored)[0]!; const recovered = stored.events[recoveredId]!;
      assert.equal(recovered.supersedesEventId, failureId);
      const recoveryEvidence = stored.evidence[recovered.sourceRevisionRef]!;
      const recoveryArtifact = stored.artifacts[recoveryEvidence.objectKey]!;
      assert.equal(recoveryArtifact.outcome, 'WAITING_EVIDENCE');
      assert.equal(recoveryArtifact.datasetRevisionId, originalDatasets[0]!.id);
      assert.equal(recoveryArtifact.contentRef, originalDatasets[0]!.contentRef);
      assert.equal(recoveryArtifact.reasonCode, 'source_recovered_waiting_for_next_registered_operating_slot');
      assert.ok(BigInt(recovered.availableAtNs) >= BigInt(stored.events[failureId]!.availableAtNs));
      for (let poll = 0; poll < 3; poll++) await read();
      await state.restart();
      // Let the original persisted Monitor perform another actual source GET
      // and manifest verification after reopen; no synthetic task is invoked.
      await new Promise(resolve => setTimeout(resolve, 5500));
      stored = await record();
      assert.deepEqual(recoveryIds(stored), [recoveredId], 'One failure episode has one immutable recovery even after polling and SQLite reopen.');
      assert.deepEqual(stored.artifacts[recoveryEvidence.objectKey], recoveryArtifact);
      assert.ok(Object.values(stored.gaps).filter(gap => gap.requiredCapability === 'source_revision').every(gap => gap.state === 'RESOLVED'));
      assert.deepEqual((await read()).datasetRevisions, originalDatasets);
      assert.deepEqual(json(stored.cycles), originalCycles); assert.deepEqual(json(stored.decisions), originalDecisions);
      assert.deepEqual(json(Object.fromEntries(Object.entries(stored.artifacts).filter(([key]) => key.startsWith('forward-slot:')))), originalSlots);
      assert.equal(stored.currentDecisionId, null);
      assert.deepEqual(json(object(object(stored.artifacts['latest-account-pending']!.capture).basis)), originalPendingBasis);
      const afterAccount = observation((await state.service.research.harness.snapshot(Application, context))!.observations[f.connection.id]);
      assert.deepEqual(json(afterAccount.snapshot!), originalSnapshot); assert.equal(afterAccount.snapshotReceivedAtNs, originalReceivedAtNs);
      assert.deepEqual(json(stored.derivedTrials), trials); assert.deepEqual(json(stored.budgetMovements), movements);
      assert.deepEqual(json((await state.service.research.harness.snapshot(Exposures, context))?.windows ?? []), exposures);
      assert.equal((await readFile(logPath, 'utf8')).split('\n').filter(value => value === 'build-observed-spot').length, 1);
      assert.equal(remote!.posts, 0);
    }
    const entering = await state.service.app.inject({ method: 'POST', url: '/api/v3/maintenance/enter', headers: state.headers,
      payload: { requestId: 'pause-original-forward-monitor', reason: 'Verify original monitor maintenance after recovery' } });
    assert.equal(entering.statusCode, 202, entering.body);
    const maintenanceDeadline = realNow() + 12_000; let quiesced = false;
    while (realNow() < maintenanceDeadline) {
      const maintenance = await state.service.maintenance.status();
      if (maintenance.state === 'QUIESCED') { assert.ok(maintenance.parkedTaskIds.includes(stored.monitorTaskId!)); quiesced = true; break; }
      await new Promise(resolve => setTimeout(resolve, 50));
    }
    assert.ok(quiesced, 'The original monitor parks without erasing the original operating task or starting another solve.');
    await assertHistoricalEvidence();
    console.log('ARCHIVAL_SOFTWARE_CONTROLLED_CLOCK_FORWARD_RECOVERY_PASS');
    completed = true;
  } finally {
    process.env.QZ_JOB_BINARY = originalEnvironmentBinary; await rm(pausePath, { force: true });
    if (completed) await state.cleanup();
    else {
      if (mandateId) try { await writeFile(join(directory, 'failed-pm-record.json'), JSON.stringify(pmRecord(pmState(await state.service.pm.snapshot()), mandateId)), { mode: 0o600 }); } catch { /* The original closed SQLite remains authoritative. */ }
      await state.service.close(); console.error(`Original failed HTTP/native evidence retained at ${directory}`);
    }
    await remote?.close(); await f.cleanup();
  }
});
