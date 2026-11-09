import assert from 'node:assert/strict';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { createRegistry, defineExtension, defineTask, Harness } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { NativeExecutionRequestV2, NativeJson, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { NativeSteps, withNativeRemoteTracking } from '../../research/src/native-execution.ts';
import { openDurableStorage } from '../../research/src/durable-storage.ts';
import { canonicalJsonDigest } from '../../research/src/native-json.ts';
import { computePmScience } from '../src/pm-science.ts';
import { pmAllocationMethod } from '../src/pm-method.ts';
import { object } from '../src/facts.ts';

const context = BACKGROUND_CONTEXT;

/** Controlled HTTP replies exercise only the real PM/Durable identity contract.
 * This fixture does not run a scientific solver or establish native recovery. */
function protocolSource(): JsonObject {
  const prefix = { event_start_ns: '0', event_end_ns: '5000000000', decision_cutoff_ns: '5000000000', bar_types: ['TEST.SIM-1-SECOND-LAST-EXTERNAL'] };
  const request = { train_end_ns: '1000000000', validation_end_ns: '5000000000', forecast: { selection: prefix, parameters: {} } };
  const members = [1, 2].map(index => ({ model: '(module)', factorVersionId: `test-factor-${index}`, sourceTrialId: `test-trial-${index}`, weight: '0.5' }));
  const fitted = members.map(member => ({ factor_version_id: member.factorVersionId, source_evidence_ref: `evidence:${member.factorVersionId}`, weight: member.weight }));
  return {
    protocolTestNotScientificEvidence: true,
    catalogPrefix: { method: 'NAUTILUS_SELECTED_PREFIX_SHA256_V2', content_sha256: 'a'.repeat(64), selection: prefix },
    request,
    members,
    fit: { fit_end_ns: request.train_end_ns, members: fitted },
    fitRequest: { fit_end_ns: request.train_end_ns, members: fitted.map(member => ({ ...member, report: { request } })) },
    policy: { targetTtlNs: '1000000000' },
    simulationRequest: {
      selection: { ...prefix, event_start_ns: request.train_end_ns },
      settings: { base_currency: 'USD', fee_rates: [{ instrument_id: 'TEST.SIM', taker: '0' }] },
      target_points: ['2000000000', '3000000000'].map(asof_ns => ({ asof_ns, valid_until_ns: String(BigInt(asof_ns) + 1000000000n), targets: [], cash_weight: '1' })),
    },
  };
}

test('PM protocol: interruption after the frozen remote reply reuses original preparation identities; saved legacy UUIDs remain original', { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-pm-preparation-recovery-'));
  const source = protocolSource();
  const implementationDigest = `sha256:${'b'.repeat(64)}`;
  const instanceId = 'controlled-pm-worker'; const profileId = 'controlled-protocol-profile';
  const posted: NativeExecutionRequestV2[] = [];
  const originalReplies = new Map<string, NativeJson>();
  const commands = ['catalog-prefix-digest', 'freeze-ensemble-calibrations', 'capital-curve', 'study-portfolio', 'simulate'];
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    const send = (body: unknown) => response.end(JSON.stringify(body));
    if (request.url === '/v1/capabilities') { send({ schemaVersion: 1, implementationDigest, commands }); return; }
    if (request.url === '/v2/capabilities') { send({ schemaVersion: 2, instanceId, implementationDigest, commands,
      recovery: { profileId, verifiedCommands: commands, checkpointIntervalMs: 60_000 } }); return; }
    if (request.method !== 'POST' || request.url !== '/v2/execute') { response.statusCode = 500; send({ error: 'unexpected_controlled_request' }); return; }
    let text = ''; for await (const bytes of request) text += bytes;
    const call = JSON.parse(text) as NativeExecutionRequestV2; posted.push(call);
    let result: NativeJson;
    if (call.command === 'catalog-prefix-digest') result = source.catalogPrefix!;
    else if (call.command === 'freeze-ensemble-calibrations') result = { members: (object(call.input).members as JsonObject[])
      .map(member => ({ factor_version_id: member.factor_version_id!, calibration: { protocolTest: true } })) };
    else if (call.command === 'capital-curve') result = { termination: 'PLANNED_ONLY', generation: { planned_capitals: ['1000'] } };
    else if (call.command === 'study-portfolio') result = { simulation_request: { original: call.input }, simulation: { protocolTest: true } };
    else if (call.command === 'simulate') result = { protocolTest: true };
    else throw new Error(`unexpected_controlled_command:${call.command}`);
    assert.ok(!originalReplies.has(call.requestId), 'a completed original invocation must never be POSTed again');
    originalReplies.set(call.requestId, result);
    send({ schemaVersion: 2, instanceId, requestId: call.requestId, taskId: posted.length, status: 'COMPLETED', result,
      recovery: { profileId, phase: 'complete', generation: 1, checkpointAt: new Date().toISOString(), blockedReason: null } });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'controlled-pm', url: `http://127.0.0.1:${address.port}`, protocolVersion: 2, instanceId, recoveryProfileId: profileId };
  let cut = true; let entered!: () => void;
  const interrupted = new Promise<void>(resolve => { entered = resolve; });
  let preparationBeforeCommit: JsonObject | undefined;
  const Compute = defineTask<{ legacyPreparation?: JsonObject }, { phase: 'compute' }, JsonObject>({
    name: 'test.pm-original-preparation', version: 1, initial: () => ({ phase: 'compute' }),
    phases: { async compute(task, runtime, ctx) {
      if (task.input.legacyPreparation) await runtime.memo('preparation', task.input.legacyPreparation, ctx);
      try {
        const result = await withNativeRemoteTracking(task.id, runtime, ctx, () => computePmScience({ source, capital: { amount: '1000', currency: 'USD' },
          currency: 'USD', catalog: '/controlled/immutable-catalog', binary: target, dataDir: directory, cycleId: 'original-cycle', allocationMethod: pmAllocationMethod() }, ctx, {
          read: key => runtime.memo<JsonObject>(key, ctx),
          write: async (key, value) => {
            if (key === 'preparation' && cut) { preparationBeforeCommit = structuredClone(value); entered(); throw new Error('controlled_preparation_commit_cut'); }
            await runtime.memo(key, value, ctx);
          },
        }));
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: result as unknown as JsonObject } }), ctx);
      } catch (error) {
        if (runtime.signal.aborted) throw error;
        if (error instanceof Error && error.message === 'controlled_preparation_commit_cut') { await runtime.sleep(runtime.now() + 120_000, ctx); return; }
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: String(error) } } }), ctx);
      }
    } },
    async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });
  const registry = createRegistry(); registry.install(defineExtension({ name: 'test.pm-preparation-recovery', tasks: [Compute] }));
  const open = async () => {
    const harness = await Harness.open(await openDurableStorage(join(directory, 'original.sqlite')), { registry, models: createModels() }, context);
    harness.resume(); return harness;
  };
  let harness = await open();
  const admit = async (legacyPreparation?: JsonObject) => harness.commit(async tx => {
    const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
    const taskId = await tx.createTask(Compute, legacyPreparation ? { legacyPreparation } : {}, { conversationId: conversation.id, ownership: { kind: 'conversation' } });
    return { taskId, conversationId: conversation.id };
  }, context);
  try {
    const original = await admit(); await interrupted;
    const freeze = posted.find(call => call.command === 'freeze-ensemble-calibrations'); assert.ok(freeze);
    const before = await harness.snapshot(NativeSteps, original.conversationId, String(original.taskId), context);
    assert.ok(Object.values(before!.steps).some(step => object(step.request).requestId === freeze.requestId && object(step.receipt).status === 'COMPLETED'));
    assert.ok(preparationBeforeCommit);
    await harness.close(context); cut = false; harness = await open();
    const task = await harness.waitForTask(original.taskId as TaskId<JsonObject>, context);
    assert.equal(task.state.outcome.status, 'completed', JSON.stringify(task.state.outcome));
    if (task.state.outcome.status !== 'completed') return;
    const result = task.state.outcome.result;
    const evidence = object(result.evidence);
    assert.deepEqual(evidence.freezeRequest, freeze.input, 'every object ID and frozen request byte survives the preparation commit gap');
    assert.equal(posted.filter(call => call.command === 'freeze-ensemble-calibrations').length, 1);
    const after = await harness.snapshot(NativeSteps, original.conversationId, String(original.taskId), context);
    assert.equal(canonicalJsonDigest(Object.values(after!.steps).find(step => object(step.request).requestId === freeze.requestId)!.request!), canonicalJsonDigest(freeze as unknown as JsonObject));
    for (const binding of evidence.bindings as JsonObject[]) for (const field of ['reportArtifactId', 'modelArtifactId', 'calibrationArtifactId']) assert.match(String(binding[field]), /^[0-9a-f]{8}-[0-9a-f]{4}-5[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/);

    // A saved preparation from before the deterministic UUID change remains
    // authoritative. This is a separate original Task with its own invocations.
    const objectIds = Object.keys(object(object(preparationBeforeCommit.value).objects));
    const legacyIds = new Map(objectIds.map((id, index) => [id, `00000000-0000-7000-8000-${String(index + 1).padStart(12, '0')}`]));
    function legacy(value: NativeJson): NativeJson {
      if (typeof value === 'string') return legacyIds.get(value) ?? value;
      if (Array.isArray(value)) return value.map(legacy);
      if (value && typeof value === 'object') return Object.fromEntries(Object.entries(value).map(([key, item]) => [legacyIds.get(key) ?? key, legacy(item)]));
      return value;
    }
    const savedLegacy = legacy(preparationBeforeCommit) as JsonObject;
    const legacyTask = await admit(savedLegacy);
    const settledLegacy = await harness.waitForTask(legacyTask.taskId as TaskId<JsonObject>, context);
    assert.equal(settledLegacy.state.outcome.status, 'completed', JSON.stringify(settledLegacy.state.outcome));
    if (settledLegacy.state.outcome.status === 'completed') assert.deepEqual(object(settledLegacy.state.outcome.result.evidence).bindings, object(savedLegacy.value).bindings);
    assert.equal(posted.filter(call => call.command === 'freeze-ensemble-calibrations').length, 1, 'saved legacy preparation never refits or reassigns its UUIDs');
  } finally {
    await harness.close(context); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});
