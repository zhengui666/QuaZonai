import assert from 'node:assert/strict';
import { createHash, randomBytes } from 'node:crypto';
import { test } from 'node:test';
import { awaitWithContext } from '@earendil-works/chord/context';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { EvidenceRef, PmMandateView, PortfolioDecision } from '../../../packages/contracts/src/pm.ts';
import { object } from '../src/facts.ts';
import { digest, PmDocument, pmRecord, pmState } from '../src/pm-facts.ts';
import { pmArtifactDigest, pmScienceCheckpoint, readPmArtifact, retainPmArtifacts } from '../src/pm-evidence.ts';
import { context, setup } from './support.ts';
import { pmTargetPayload } from '../src/pm-target.ts';

// The original token-at-a-time implementation is an independent identity oracle.
function originalArtifactDigest(value: JsonObject): string {
  const hash = createHash('sha256');
  function visit(item: JsonObject[string] | undefined): void {
    if (item === null || typeof item !== 'object') { hash.update(JSON.stringify(item) ?? 'null'); return; }
    if (Array.isArray(item)) {
      hash.update('['); item.forEach((entry, index) => { if (index) hash.update(','); visit(entry); }); hash.update(']'); return;
    }
    hash.update('{'); let first = true;
    for (const key of Object.keys(item).filter(key => item[key] !== undefined).sort()) {
      if (!first) hash.update(','); first = false; hash.update(JSON.stringify(key)); hash.update(':'); visit(item[key]);
    }
    hash.update('}');
  }
  visit(value); return hash.digest('hex');
}

test('streamed PM artifact digest preserves canonical JSON identity', () => {
  const values: JsonObject[] = [{ z: '0.000000000000000001', a: ['原值', null, { b: 2, a: -0 }] },
    { c: '\n\\"', a: { '\u0000': 'x', '\uFFFF': 'y' }, b: [false, 1e-20, 1e21] }];
  for (const value of values) assert.equal(pmArtifactDigest(value), digest(value));
});

test('buffered PM digest keeps original bytes for large nested tapes and complete Unicode tokens', () => {
  const shared = { amount: '3000.000000000000000001', clock: '1791849600000000001', text: '原生🐋\ud800' };
  const value: JsonObject = { schema_version: 1, '2': 'two', '10': 'ten',
    rows: Array.from({ length: 10_000 }, (_, index) => ({ index, original: { snapshot: { balances: [shared, shared],
      events: [{ exact: '0.000000000000000001', side: index % 2 ? 'BUY' : 'SELL' }] } } })),
    boundary: 'x'.repeat(65_529) + '🐋', largeToken: '中文🐋'.repeat(32_768), nonfinite: [NaN, Infinity, -Infinity] };
  assert.equal(pmArtifactDigest(value), originalArtifactDigest(value));
  assert.equal(pmArtifactDigest(value), digest(value));
});

test('buffered PM digest retains legacy sparse, missing and non-JSON value handling', () => {
  const sparse = new Array(4); sparse[2] = undefined; sparse[3] = 7;
  const value = { omitted: undefined, sparse, symbol: Symbol('value'), function: () => 1,
    nested: { nonfinite: NaN, date: new Date(0) } } as unknown as JsonObject;
  assert.equal(pmArtifactDigest(value), originalArtifactDigest(value));
});

test('buffered PM digest propagates original late getter and BigInt errors and refuses cycles', () => {
  const failure = new Error('original-late-hash-getter-failure');
  const fixtures = [
    () => ({ a: 'x'.repeat(128 * 1024), z: 1n }),
    () => { let calls = 0; return { a: 'x'.repeat(128 * 1024), get z() { if (++calls === 2) throw failure; return true; } }; },
  ];
  for (const fixture of fixtures) {
    let originalError: unknown;
    try { originalArtifactDigest(fixture() as unknown as JsonObject); } catch (error) { originalError = error; }
    assert.ok(originalError instanceof Error);
    assert.throws(() => pmArtifactDigest(fixture() as unknown as JsonObject), error => {
      assert.ok(error instanceof Error); assert.equal(error.constructor, originalError.constructor);
      assert.equal(error.message, originalError.message); return true;
    });
  }
  const circular: JsonObject = {}; circular.self = circular;
  assert.throws(() => pmArtifactDigest(circular), RangeError);
});

test('original PM evidence and scientific checkpoint survive official task restart in separate compressed namespaces', { timeout: 30_000 }, async () => {
  const state = await setup();
  // Storage protocol only: random bytes are deliberately not financial evidence.
  const tape: JsonObject = { scope: 'TEST_ONLY_STORAGE', decimal: '3000.000000000000000001',
    text: '原始证据', bytes: randomBytes(1_200_000).toString('hex') };
  const calibration: JsonObject = { schema_version: 1, actual_test_object_id: 'test-calibration', coefficient: '0.000000000000000001' };
  const originalTarget: JsonObject = { schema_version: 1, asof_ns: '1', valid_until_ns: '100', targets: [], cash_weight: '1' };
  const checkpoint: JsonObject = { fingerprint: 'original-frozen-basis', native: tape, kind: 'original-checkpoint' };
  let wrote = 0; let reached!: () => void; const suspended = new Promise<void>(resolve => { reached = resolve; });
  let release!: () => void; const resumed = new Promise<void>(resolve => { release = resolve; });
  const task = defineTask<{ mandateId: string }, { phase: 'write' | 'resume' }, JsonObject>({
    name: 'test.pm-evidence-recovery', version: 1, initial: () => ({ phase: 'write' }),
    phases: {
      async write(input, runtime, ctx) {
        wrote += 1;
        const science = pmScienceCheckpoint(runtime, 'pm-science', ctx);
        await science.write('capital:3000', checkpoint);
        await science.write('preparation', { ...checkpoint, fingerprint: 'different-preparation-basis' });
        const retained = await retainPmArtifacts(runtime, { 'test-native': tape, 'test-calibration': calibration, 'test-original-target': originalTarget }, ctx, ['test-original-target']);
        assert.equal(retained['test-native']!.kind, 'PM_NATIVE_ARTIFACT_BUNDLE');
        assert.deepEqual(retained['test-original-target'], originalTarget, 'small authoritative delivery input remains inline while scientific tape is compressed');
        assert.ok(Number(object(retained['test-native']!.native).parts) >= 2, 'exercise multiple existing evidence parts');
        await runtime.commit(async tx => {
          const record = pmRecord(pmState(await tx.doc(PmDocument)), input.input.mandateId);
          for (const [id, artifact] of Object.entries({ 'test-native': tape, 'test-calibration': calibration, 'test-original-target': originalTarget })) {
            record.evidence[id] = { id, sourceKind: 'NATIVE_RESULT', role: 'SELECTION', runtime: { conversationId: runtime.conversationId, taskId: runtime.taskId },
              objectKey: id, contentDigest: pmArtifactDigest(artifact), mediaType: 'application/json', sourceRevision: 'test-original', eventInterval: null,
              availableAtNs: '1', clockPrecision: 'NANOSECOND', schemaId: 'test.storage/1', visibility: 'OWNER_ONLY' };
            record.artifacts[id] = retained[id]!;
          }
          return { status: 'running', checkpoint: { phase: 'resume' } };
        }, ctx);
      },
      async resume(_input, runtime, ctx) {
        reached(); await awaitWithContext(resumed, ctx);
        const science = pmScienceCheckpoint(runtime, 'pm-science', ctx);
        assert.deepEqual(await science.read('capital:3000'), checkpoint);
        assert.deepEqual(await science.read('preparation'), { ...checkpoint, fingerprint: 'different-preparation-basis' });
        const saved = await runtime.memo<JsonObject>('pm-science:capital:3000', ctx);
        assert.equal(saved!.kind, 'PM_COMPRESSED_SCIENCE_CHECKPOINT');
        assert.ok(JSON.stringify(saved).length < 1000);
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { restored: true } } }), ctx);
      },
    },
    async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });
  try {
    state.options.extensions.push(defineExtension({ name: 'test.pm-evidence-recovery', tasks: [task] }));
    state.options.config.datasets[0]!.data.request = { train_end_ns: '140', validation_end_ns: '180', forecast: {
      schema_version: 1, selection: { event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] }, parameters: {} } };
    await state.restart();
    const admitted = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers, payload: {
      requestId: 'evidence-storage', goal: 'Protocol storage acceptance only', datasetId: 'controlled', model: state.model, maxTrials: 1,
      funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000.000000000000000001', currency: 'USD' } },
    } });
    assert.equal(admitted.statusCode, 202, admitted.body);
    const mandateId = admitted.json<PmMandateView>().mandate.id;
    const taskId = await state.service.research.harness.commit(async tx => {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      return tx.createTask(task, { mandateId }, { conversationId: conversation.id, ownership: { kind: 'conversation' }, background: true });
    }, context);
    await suspended;
    await state.restart(); release();
    await state.service.research.harness.waitForTask(taskId as TaskId, context);
    const completed = await state.service.research.harness.getTask(taskId, context);
    assert.equal(completed!.state.status, 'terminal'); assert.equal(completed!.state.outcome!.status, 'completed');
    assert.equal(wrote, 1, 'restart resumes original checkpoint rather than recomputing scientific work');
    const record = pmRecord(pmState(await state.service.research.harness.snapshot(PmDocument, context)), mandateId);
    assert.deepEqual(pmTargetPayload(record, { explanationFacts: { nativeTargetEvidenceRef: 'test-original-target' }, basisRevision: 'test-original',
      validUntilNs: '90', assetTargets: [] } as unknown as PortfolioDecision), { ...originalTarget, valid_until_ns: '90' },
    'restart keeps original native target bytes usable by the real delivery consumer without renewing TTL; this is storage, not qualification');
    assert.ok(JSON.stringify(record.artifacts).length < 20_000, 'large native tapes stay outside the main PM document');
    for (const [id, raw] of Object.entries({ 'test-native': tape, 'test-calibration': calibration })) {
      const path = `/api/v3/pm-evidence/${id}`;
      assert.equal((await state.service.app.inject({ url: path })).statusCode, 401);
      const response = await state.service.app.inject({ url: path, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body.slice(0, 300));
      const body = response.json<{ reference: EvidenceRef; payload: JsonObject }>();
      assert.deepEqual(body.payload, raw); assert.equal(body.reference.contentDigest, pmArtifactDigest(raw));
      assert.equal(body.reference.runtime!.taskId, taskId);
    }
    const ref = record.evidence['test-native']!; const marker = record.artifacts['test-native']!;
    await assert.rejects(readPmArtifact(state.service.research.harness, { ...ref, contentDigest: 'wrong-digest' }, marker, context), /digest_mismatch/);
    await assert.rejects(readPmArtifact(state.service.research.harness, ref, { ...marker, taskId: Number(taskId) + 1 }, context), /identity_mismatch/);
    await assert.rejects(readPmArtifact(state.service.research.harness, ref, { ...marker, key: 'test-calibration' }, context), /identity_mismatch/);
  } finally { release(); await state.cleanup(); }
});
