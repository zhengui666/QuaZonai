import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { ReviewSession } from '../src/review-documents.ts';
import { IndependentReviewTask, runIndependentReview } from '../src/review-runtime.ts';
import type { IndependentReviewInput } from '../src/review-runtime.ts';
import { researchRuntime } from '../src/research-runtime.ts';
import { createReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';
import type { ReviewReport } from '../../../packages/contracts/src/review.ts';

const context = BACKGROUND_CONTEXT;
const kinds = ['context', 'provenance', 'factor', 'selection', 'stress', 'portfolio'];
const message = (name: string, args: JsonObject) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
const finish = (ref: string) => ({ narrative: 'Protocol-only mechanism review with explicit missing scientific evidence.', findings: [],
  mechanism: ['mechanism', 'availability', 'direction', 'implementation', 'complexity'].map(criterion => ({ criterion, supported: false, evidenceRefs: [ref] })) });

async function fixture() {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-review-agent-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const model = { provider: faux.provider.id, modelId: faux.getModel().id };
  const input: IndependentReviewInput = { target: { kind: 'FACTOR', id: 'fixture:factor', version: 'fixture:version1',
    sourceEvidenceRefs: ['fixture:validation'], evaluationStartNs: '100' },
    policy: createReviewPolicy('FACTOR', 'fixture:policy', '90'), model,
    evidence: Object.fromEntries(kinds.map(kind => [kind, { status: 'INCOMPLETE', reason: 'controlled_protocol_missing_market_evidence', evidenceRefs: ['fixture:validation'] }])),
    checks: [{ id: 'implementation', status: 'PASS', reason: 'controlled_fixture_only', evidenceRefs: ['fixture:validation'] }], metrics: [], createdAt: '2026-11-02T00:00:00.000Z' };
  return { directory, faux, models, model, input, database: join(directory, 'research.sqlite') };
}
async function admit(host: Awaited<ReturnType<typeof openResearch>>, input: IndependentReviewInput) {
  return host.harness.commit(async tx => {
    const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
    return tx.createTask(IndependentReviewTask, input, { conversationId: conversation.id, ownership: { kind: 'conversation' } });
  }, context);
}

// Uses the official Faux Provider and actual Durable storage/tasks. These tests
// establish isolated tool execution and replay, never paid-model review quality.
test('isolated review reads every category, retains missing evidence and reopens the original task without new model calls', { timeout: 30_000 }, async () => {
  const f = await fixture(); let host = await openResearch(f);
  try {
    f.faux.setResponses([...kinds.map(kind => message(`review_${kind}`, {})), message('review_finish', finish('fixture:validation'))]);
    const id = await admit(host, f.input);
    const task = await host.harness.waitForTask(id, context);
    assert.equal(task.state.outcome.status, 'completed');
    if (task.state.outcome.status !== 'completed') return;
    const result = task.state.outcome.result;
    const report = result.report as unknown as ReviewReport;
    assert.equal(report.decision, 'INCONCLUSIVE'); assert.equal(report.qualityCoefficient, 0);
    assert.ok(report.missingEvidence.includes('reference_calibration_missing'));
    const reviewer = Number(result.reviewerConversationId);
    const session = await host.harness.snapshot(ReviewSession, reviewer as ConversationId, context);
    assert.equal(session!.finished, true); assert.deepEqual(session!.reads, kinds); assert.equal(session!.calls, 7);
    assert.equal(session!.narrative, finish('fixture:validation').narrative);
    assert.equal(f.faux.state.callCount, 7);
    await host.close(); host = await openResearch(f);
    const repeated = await host.harness.waitForTask(id, context);
    assert.deepEqual(repeated.state.outcome, task.state.outcome);
    assert.equal(f.faux.state.callCount, 7);
    assert.equal((await host.harness.snapshot(ReviewSession, reviewer as ConversationId, context))!.finished, true);
  } finally { await host.close(); await rm(f.directory, { recursive: true, force: true }); }
});

for (const wrongSource of [false, true]) {
  test(`review finish rejects ${wrongSource ? 'foreign evidence references' : 'skipped mandatory tools'} and cannot release`, { timeout: 30_000 }, async () => {
    const f = await fixture(); const host = await openResearch(f);
    try {
      f.faux.setResponses([...(wrongSource ? kinds.map(kind => message(`review_${kind}`, {})) : []),
        message('review_finish', finish(wrongSource ? 'foreign:heldout' : 'fixture:validation')), fauxAssistantMessage('Cannot complete the evidence review.')]);
      const id = await admit(host, f.input); const task = await host.harness.waitForTask(id, context);
      assert.equal(task.state.outcome.status, 'completed');
      if (task.state.outcome.status !== 'completed') return;
      const report = task.state.outcome.result.report as unknown as ReviewReport;
      assert.equal(report.decision, 'INCONCLUSIVE'); assert.equal(report.qualityCoefficient, 0);
      assert.ok(report.checks.some(check => check.id === 'implementation' && check.status === 'UNKNOWN'));
      const session = await host.harness.snapshot(ReviewSession, Number(task.state.outcome.result.reviewerConversationId) as ConversationId, context);
      assert.equal(session!.finished, false); assert.equal(session!.narrative, null);
    } finally { await host.close(); await rm(f.directory, { recursive: true, force: true }); }
  });
}

const ReplayProbe = defineTask<IndependentReviewInput, { phase: 'review' }, JsonObject>({
  name: 'test.review-replay-probe', version: 1, initial: () => ({ phase: 'review' }),
  async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  phases: { async review(task, runtime, ctx) {
    const api = researchRuntime(runtime);
    const first = await runIndependentReview(api, ctx, task.input);
    const second = await runIndependentReview(api, ctx, task.input);
    let conflict = false;
    try { await runIndependentReview(api, ctx, { ...task.input, target: { ...task.input.target, version: 'fixture:changed-version' } }); }
    catch (error) { conflict = error instanceof Error && error.message === 'review_identity_conflict'; }
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: { firstId: first.id, secondId: second.id, conflict } } }), ctx);
  } },
});

test('the same target/body reuses its review while a changed version cannot reuse that identity', { timeout: 30_000 }, async () => {
  const f = await fixture(); const host = await openResearch({ ...f, extensions: [defineExtension({ name: 'test.review-replay', tasks: [ReplayProbe] })] });
  try {
    f.faux.setResponses([...kinds.map(kind => message(`review_${kind}`, {})), message('review_finish', finish('fixture:validation'))]);
    const id = await host.harness.commit(async tx => {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      return tx.createTask(ReplayProbe, f.input, { conversationId: conversation.id, ownership: { kind: 'conversation' } });
    }, context);
    const task = await host.harness.waitForTask(id as TaskId<JsonObject>, context);
    assert.equal(task.state.outcome.status, 'completed');
    if (task.state.outcome.status !== 'completed') return;
    assert.equal(task.state.outcome.result.firstId, task.state.outcome.result.secondId);
    assert.equal(task.state.outcome.result.conflict, true); assert.equal(f.faux.state.callCount, 7);
  } finally { await host.close(); await rm(f.directory, { recursive: true, force: true }); }
});
