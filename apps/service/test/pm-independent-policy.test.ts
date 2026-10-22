import assert from 'node:assert/strict';
import { test } from 'node:test';
import { access, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import research from '../../research/plugins/research.ts';
import factors from '../../research/plugins/factor-wasm.ts';
import fittedEnsemble from '../../research/plugins/fitted-ensemble.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { object } from '../src/facts.ts';
import { pmRecord, pmState } from '../src/pm-facts.ts';
import { evaluatePmQualification } from '../src/pm-qualification.ts';
import { context, fauxAssistantMessage, fauxToolCall, setup } from './support.ts';

const directory = process.env.QZ_PM_NATIVE_FIXTURE_DIR;
const binary = process.env.QZ_JOB_BINARY;
const call = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
function result(transcript: { messages: readonly unknown[] }, tool: string, index = -1): JsonObject {
  const message = (transcript.messages.filter(value => (value as { role?: string; toolName?: string }).role === 'toolResult'
    && (value as { toolName?: string }).toolName === tool) as { isError?: boolean; content: { type: string; text?: string }[] }[]).at(index);
  assert.ok(message); const text = message.content.filter(item => item.type === 'text').map(item => item.text).join('');
  assert.ok(!message.isError, `${tool}: ${text}`); return object(JSON.parse(text));
}
async function exists(path: string) { try { await access(path); return true; } catch { return false; } }

test('PM independent recovery keeps its admitted policy after a legitimate concurrent owner clarification', {
  timeout: 240_000, skip: !directory || !binary ? 'Requires the actual multi-day native source and original executable; no independent result is synthesized.' : false,
}, async t => {
  const source = object(JSON.parse(await readFile(join(directory!, 'source.json'), 'utf8')));
  const state = await setup(); const pause = join(state.directory, 'pause-independent-return');
  const completed = join(state.directory, 'actual-independent-cash-completed'); const reached = join(state.directory, 'independent-checkpoint-saved');
  const log = join(state.directory, 'original-native-invocations.log'); const wrapper = join(state.directory, 'original-native-boundary.mjs');
  const validationEnd = String(object(source.request).validation_end_ns);
  // The wrapper passes the original request unchanged to the real native job.
  // It pauses only the later read-only prefix check, after both genuine native
  // independent legs have reached the official task checkpoint.
  await writeFile(wrapper, `#!${process.execPath}\nimport {spawnSync} from 'node:child_process';\nimport {appendFileSync,existsSync,writeFileSync} from 'node:fs';\nconst args=process.argv.slice(2);\nif(args[0]==='catalog-prefix-digest'&&existsSync(${JSON.stringify(completed)})&&existsSync(${JSON.stringify(pause)})){writeFileSync(${JSON.stringify(reached)},'original-receipts-persisted');while(existsSync(${JSON.stringify(pause)}))await new Promise(r=>setTimeout(r,20));}\nconst chunks=[];for await(const chunk of process.stdin)chunks.push(chunk);const input=Buffer.concat(chunks);\nappendFileSync(${JSON.stringify(log)},args[0]+'\\n');\nconst child=spawnSync(${JSON.stringify(binary)},args,{input,stdio:['pipe','inherit','inherit']});\nif(args[0]==='simulate'&&child.status===0&&BigInt(JSON.parse(input).selection.event_start_ns)>=BigInt(${JSON.stringify(validationEnd)}))writeFileSync(${JSON.stringify(completed)},'original-cash-result');\nprocess.exit(child.status??1);\n`, { mode: 0o700 });
  await writeFile(pause, 'pause original task after its native receipts');
  const previousBinary = process.env.QZ_JOB_BINARY; process.env.QZ_JOB_BINARY = wrapper;
  try {
    const dataset = state.options.config.datasets[0]!;
    dataset.data = { ...object(source.data), sealed: object(source.heldoutData) };
    dataset.strategy = object(source.policy) as unknown as NonNullable<typeof dataset.strategy>;
    state.options.config.jobBinary = wrapper; state.options.extensions = [research, factors, fittedEnsemble]; await state.restart();
    let hypothesisId = ''; const members = source.members as JsonObject[];
    state.faux.setResponses([
      call('strategy_intake', { name: 'Original causal ensemble', archetype: 'PREDICTIVE_ALPHA',
        economicMechanism: 'Train causal price features and compare the actual shared native account with cash.',
        falsificationConditions: ['Original matched independent account evidence fails.'], requiredCapabilities: [],
        intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['fees', 'selection_bias'], ambiguities: [], ownerBoundaries: [] }),
      call('research_hypothesis', { economicFamilyId: 'test:original-policy', mechanism: 'Original causal price features predict future returns.',
        prediction: 'The joint account may improve matched account evidence.', falsification: 'Independent matched evidence does not improve.',
        requiredDataCapabilities: ['BAR'], incrementalQuestion: 'Does the original joint model improve net account evidence?' }),
      transcript => { hypothesisId = String(result(transcript, 'research_hypothesis').id); return call('factor_evaluate', { hypothesisId, hypothesis: 'Original close feature', model: members[0]!.model! }); },
      () => call('factor_evaluate', { hypothesisId, hypothesis: 'Original inverse-price feature', model: members[1]!.model! }),
      transcript => call('strategy_ensemble_fit', { hypothesisId, hypothesis: 'Original TRAIN fit.', sourceTrialIds: [result(transcript, 'factor_evaluate', -2).trialId!, result(transcript, 'factor_evaluate').trialId!] }),
      transcript => call('strategy_ensemble_evaluate', { hypothesisId, hypothesis: 'Original joint account validation.', fitTrialId: result(transcript, 'strategy_ensemble_fit').trialId!, allocations: 'ALL_EQUAL' }),
      transcript => { result(transcript, 'strategy_ensemble_evaluate'); return call('research_finish', { reason: 'Original planned factor, fit and joint validation evidence completed.' }); },
    ]);
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers,
      payload: { requestId: 'independent-policy-recovery', goal: 'Research causal predictive alpha and complete daily account outcomes.', datasetId: dataset.id,
        funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, model: state.model, businessPreference: 'NET_RETURN', maxTrials: 12 } });
    assert.equal(response.statusCode, 202, response.body); const initial = response.json<Pm.PmMandateView>();
    const id = initial.mandate.id; const deadline = Date.now() + 180_000;
    while (!await exists(reached) && Date.now() < deadline) await new Promise(resolve => setTimeout(resolve, 40));
    let record = pmRecord(pmState(await state.service.pm.snapshot()), id);
    assert.ok(await exists(reached), JSON.stringify({ campaigns: record.campaigns, cycles: record.cycles, errors: Object.values(record.artifacts).filter(value => value.stage) }));
    const decision = record.decisions[record.currentDecisionId!]!; assert.equal(decision.action, 'REBALANCE');
    const taskId = record.artifacts[`independent-task:${decision.id}`]!.taskId as number;
    const frozen = structuredClone(record.artifacts[`independent-input:${decision.id}`]!);
    const policy = object(frozen.policy) as unknown as Pm.DecisionPolicyRevision;
    assert.equal(policy.id, decision.policyRevisionId);
    const originalTrials = Object.values(record.derivedTrials).filter(trial => trial.runtime.taskId === taskId);
    assert.equal(originalTrials.length, 2); assert.ok(originalTrials.every(trial => trial.state === 'ADMITTED' && trial.countedAttempt));
    const counted = Object.values(record.derivedTrials).filter(trial => trial.countedAttempt).length;
    const exposures = structuredClone((await state.service.research.harness.snapshot(Exposures, context))!.windows);
    assert.equal(exposures.filter(window => window.kind === 'FINAL').length, 1);
    const invocations = (await readFile(log, 'utf8')).split('\n').filter(name => name !== 'catalog-prefix-digest');
    const clarification = await state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(id)}/intent-clarifications`, headers: state.headers,
      payload: { requestId: 'concurrent-owner-clarification', expectedRevision: record.mandate.revision, statement: 'No leverage.',
        boundaries: [{ kind: 'LEVERAGE', sourceStatement: 'No leverage.', value: { maximumGrossNavRatio: '1', borrowingAllowed: false } }], resolvedAmbiguities: [] } });
    assert.equal(clarification.statusCode, 202, clarification.body); const changed = clarification.json<Pm.PmMandateView>();
    assert.notEqual(changed.activePolicy.id, policy.id); assert.equal(changed.campaigns.at(-1)!.state, 'PAUSED');
    assert.equal(changed.history.find(item => item.id === decision.id)!.status, 'SUPERSEDED');
    await state.service.close(); await rm(pause); await state.restart();
    const resumed = await state.service.research.harness.waitForTask(taskId as TaskId, context);
    assert.equal(resumed.state.status, 'terminal'); assert.equal(resumed.state.status === 'terminal' && resumed.state.outcome.status, 'completed', JSON.stringify(resumed.state));
    record = pmRecord(pmState(await state.service.pm.snapshot()), id);
    const finished = record.decisions[decision.id]!;
    const evidenceId = String(object(finished.explanationFacts).independentEvidenceRef);
    const evidence = await state.service.app.inject({ url: `/api/v3/pm-evidence/${encodeURIComponent(evidenceId)}`, headers: state.headers });
    assert.equal(evidence.statusCode, 200, evidence.body.slice(0, 200)); const payload = object(evidence.json().payload);
    const metrics = payload.metrics as unknown as Pm.MetricObservation[]; const baseline = payload.baselineMetrics as unknown as Pm.MetricObservation[];
    assert.ok(metrics.length > 0); assert.deepEqual([...new Set([...metrics, ...baseline].map(metric => metric.methodRef))], [policy.id],
      'Native P1 evidence cannot be attributed to the newer owner policy P2.');
    assert.deepEqual(payload.qualification, evaluatePmQualification(object(object(policy.validDomain).qualificationProtocol), policy.selectionRule, metrics, baseline,
      String(evidence.json().reference.availableAtNs)));
    assert.deepEqual(record.artifacts[`independent-input:${decision.id}`], frozen);
    assert.equal(record.policy.id, changed.activePolicy.id); assert.equal(record.currentDecisionId, null);
    assert.equal(record.cycles[decision.cycleId]!.state, 'SUPERSEDED'); assert.equal(finished.status, 'SUPERSEDED');
    assert.equal(Object.values(record.derivedTrials).filter(trial => trial.countedAttempt).length, counted);
    assert.deepEqual(Object.values(record.derivedTrials).filter(trial => trial.runtime.taskId === taskId).map(trial => trial.id), originalTrials.map(trial => trial.id));
    assert.deepEqual((await state.service.research.harness.snapshot(Exposures, context))!.windows, exposures);
    assert.deepEqual((await readFile(log, 'utf8')).split('\n').filter(name => name !== 'catalog-prefix-digest'), invocations, 'Original native scientific receipts are reused; only provenance verification can run on recovery.');
    t.diagnostic(JSON.stringify({ originalPolicyId: policy.id, currentPolicyId: record.policy.id, independentTaskId: taskId, countedAttempts: counted, originalIndependentTrialIds: originalTrials.map(trial => trial.id) }));
  } finally {
    if (previousBinary === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previousBinary;
    await rm(pause, { force: true }); await state.cleanup();
  }
});
