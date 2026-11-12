import assert from 'node:assert/strict';
import { validateReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';

/** The original caller's fields remain exact; only the documented admission
 * addition is accepted. The entire frozen input is still compared on replay. */
export function assertFrozenInput(actual: unknown, original: unknown): void {
  const value = structuredClone(actual) as { policy: Record<string, unknown> };
  const expected = original as { policy: Record<string, unknown> };
  if (expected.policy.reviewPolicies === undefined && value.policy.reviewPolicies !== undefined) {
    const policies = value.policy.reviewPolicies as Record<string, unknown>;
    assert.equal(validateReviewPolicy(policies.FACTOR).targetKind, 'FACTOR');
    assert.equal(validateReviewPolicy(policies.PORTFOLIO).targetKind, 'PORTFOLIO');
    delete value.policy.reviewPolicies;
  }
  assert.deepEqual(value, original);
}

/** Retain a pre-native-tool-policy admission in the real Durable store.
 * Compatibility-only setup: no native results, scores or task outcomes are invented. */
export async function retainLegacyResearchAdmission(
  host: import('../src/host.ts').ResearchHost, requestId: string, value: import('../src/documents.ts').ResearchInput,
  sourceEvidence?: import('../src/review-source.ts').CapturedReviewSource,
  metadata?: import('@earendil-works/pi-durable').JsonObject,
  authorize?: Parameters<typeof import('../src/host.ts').openResearch>[0]['authorizeAdmission'],
) {
  const { researchInput } = await import('../src/input.ts');
  const { Requests } = await import('../src/documents.ts');
  const { ResearchIntakeTask } = await import('../src/intake.ts');
  const { canonicalJsonDigest } = await import('../src/native-json.ts');
  const { isRecoveryRequestId } = await import('../src/recovery.ts');
  const { BACKGROUND_CONTEXT: context } = await import('@earendil-works/chord/context');
  if (!requestId || isRecoveryRequestId(requestId) || (await host.harness.snapshot(Requests, context))?.requests[`request:${requestId}`]) return;
  const input = structuredClone(researchInput(value));
  const inputDigest = canonicalJsonDigest(input);
  const model = (await host.availableModels(input.model.provider)).find(model => model.modelId === input.model.modelId);
  if (!model) throw new Error('model_unavailable');
  if (input.thinkingLevel !== undefined && !model.thinkingLevels.includes(input.thinkingLevel)) throw new Error('thinking_level_unsupported');
  if (input.serviceTier !== undefined && !model.serviceTiers.includes(input.serviceTier)) throw new Error('service_tier_unsupported');
  await (await host.harness.root(context)).commit(async tx => {
    const requests = await tx.doc(Requests);
    if (requests.requests[`request:${requestId}`]) return;
    await authorize?.(tx, input, metadata);
    const source = sourceEvidence ? JSON.parse(JSON.stringify(sourceEvidence)) : undefined;
    const taskId = await tx.createTask(ResearchIntakeTask, { requestId, input, thinkingLevel: input.thinkingLevel ?? model.effectiveThinkingLevel,
      ...(source ? { sourceEvidence: source } : {}) }, { ownership: { kind: 'conversation' } });
    requests.requests[`request:${requestId}`] = { input, inputDigest, conversationId: null, submissionId: null, taskId,
      ...(source ? { sourceEvidence: source } : {}), ...(metadata ? { metadata } : {}) };
  }, context);
}

/** Compatibility tests open persisted Intake/Task work, not a new legacy producer.
 * New-admission tests import the production host instead. */
export async function openRetainedResearch(options: Parameters<typeof import('../src/host.ts').openResearch>[0]) {
  const { openResearch } = await import('../src/host.ts');
  const host = await openResearch(options);
  const originalStart = host.start;
  let closed = false;
  const start: typeof host.start = async (requestId, value, originalAdmission, sourceEvidence, metadata) => {
    if (closed) return originalStart(requestId, value, originalAdmission, sourceEvidence, metadata);
    await retainLegacyResearchAdmission(host, requestId, value, sourceEvidence, metadata, options.authorizeAdmission);
    return originalStart(requestId, value, originalAdmission, sourceEvidence, metadata);
  };
  return { ...host, start, close() { closed = true; return host.close(); } };
}
