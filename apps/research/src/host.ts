import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { clampThinkingLevel, getSupportedThinkingLevels } from '@earendil-works/pi-ai/models';
import type { Models } from '@earendil-works/pi-ai/models';
import type { ModelThinkingLevel } from '@earendil-works/pi-ai';
import { configure, createRegistry, GenerationTask, Harness, ProviderDoc } from '@earendil-works/pi-durable';
import type { ConversationId, ConversationWatch, Cursor, Extension, HarnessOptions, Storage, SubmissionId, TaskId, Tx } from '@earendil-works/pi-durable';
import { acquireDurableOwner, assertOriginalDurableDatabase, openDurableStorage } from './durable-storage.ts';
import type { DurableOwner } from './durable-storage.ts';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { canonical, Requests, Study } from './documents.ts';
import { ReviewLedger } from './review-documents.ts';
import { freezeReviewPolicies } from './review-workflow.ts';
import { reviewExtensionWithUsage } from '../plugins/review.ts';
import { ReviewFactorAccountTask } from './review-factor-account.ts';
import { bindReviewSourceEvidence, ReviewSourceValidationTask } from './review-source.ts';
import type { CapturedReviewSource } from './review-source.ts';
import { ReviewNativeTask } from './review-native-task.ts';
import { IndependentReviewTask } from './review-runtime.ts';
import { defineExtension } from '@earendil-works/pi-durable';
import { LiteratureDoc } from './literature-document.ts';
import { researchInput } from './input.ts';
import { researchInstructions } from '../../../packages/contracts/src/research.ts';
import type { ResearchInput } from './documents.ts';
import { configuredModels, subscriptionSpeed, supportsServiceTier } from './models.ts';
import { ModelCredentials } from './credentials.ts';
import { researchSettings } from './settings.ts';
import { assertNativeRemoteTaskSettled, reconcileNativeRemoteTasks } from './native-remote-receipts.ts';
import { Continuation, researchContinuation } from './continuation.ts';
import { CampaignResearchDoc } from './pm-campaign.ts';
import { settleCampaignTrial } from './pm-ledger.ts';

import { isRecoveryRequestId, modelErrorCode, recoveryInput, recoveryRequestId, submissionView } from './recovery.ts';
const context = BACKGROUND_CONTEXT;

export async function openResearch(options: {
  database: string;
  extensions?: readonly Extension[];
  modules?: readonly string[];
  providerModules?: readonly string[];
  models?: Models;
  storage?: Storage;
  /** Installed recovery may only reopen the existing original ledger. */
  requireOriginalDatabase?: boolean;
  settings?: HarnessOptions['settings'];
  thinkingLevel?: ModelThinkingLevel;
  /** Service-owned parent admission, applied to original native recovery too. */
  authorizeResume?: (tx: Tx, id: ConversationId) => Promise<void>;
}) {
  const settings = researchSettings(options.settings);
  const requestedThinking = options.thinkingLevel ?? 'medium';
  if (!['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'].includes(requestedThinking)) throw new Error('invalid_thinking_level');
  let credentials: ModelCredentials | undefined;
  let owner: DurableOwner | undefined;
  let models: Models;
  let speed!: ReturnType<typeof subscriptionSpeed>;
  let harness!: Harness;
  let storage!: Storage;
  const registry = createRegistry();
  try {
    // Own the original storage before opening credentials, provider modules or
    // a Harness. A second owner must never make external calls before failing.
    owner = await acquireDurableOwner(options.database);
    if (options.requireOriginalDatabase) assertOriginalDurableDatabase(owner.path);
    storage = options.storage ?? await openDurableStorage(owner.path, { owner });
    credentials = options.models ? undefined : await ModelCredentials.open(`${resolve(options.database)}.auth`);
    models = options.models ?? await configuredModels(options.providerModules, credentials);
    speed = subscriptionSpeed(models);
    models = speed.models;
    for (const extension of options.extensions ?? []) registry.install(extension);
    for (const path of options.modules ?? []) {
      const module = await import(pathToFileURL(resolve(path)).href);
      registry.install(module.default as Extension);
    }
    registry.install(reviewExtensionWithUsage(() => harness));
    registry.install(defineExtension({ name: 'qz.review-runtime', tasks: [IndependentReviewTask, ReviewNativeTask, ReviewSourceValidationTask, ReviewFactorAccountTask] }));
    registry.install(researchContinuation(() => harness));
    harness = await Harness.open(storage, { models, registry, settings: { ...settings, extensions: settings.extensions ?? registry.snapshot().installed().filter(extension => extension.name !== 'qz.review') } }, context);
  } catch (error) {
    try { await storage?.close(context); }
    finally { try { await credentials?.close(); } finally { owner?.close(); } }
    throw error;
  }
  let accepting = true;
  const admissions = new Set<Promise<unknown>>();
  const watchers = new Set<ConversationWatch>();
  // Serialize owner commands only; official Durable still owns all execution.
  const ownerChanges = new Map<number, Promise<unknown>>();
  async function ownerChange<T>(id: number, operation: () => Promise<T>): Promise<T> {
    const prior = ownerChanges.get(id) ?? Promise.resolve();
    const next = prior.catch(() => {}).then(operation); ownerChanges.set(id, next);
    try { return await next; } finally { if (ownerChanges.get(id) === next) ownerChanges.delete(id); }
  }

  async function conversation(id: number) {
    if (!Number.isSafeInteger(id) || id < 0) throw new Error('invalid_research_id');
    const value = await harness.conversation(id as ConversationId, context);
    if (!value) throw new Error('research_not_found');
    return value;
  }

  async function rememberTier(id: ConversationId, value: unknown) {
    const tier = (value as ResearchInput).serviceTier;
    if (tier === undefined) return;
    const provider = await harness.snapshot(ProviderDoc, id, context);
    if (!provider) throw new Error('provider_session_required');
    speed.tiers.set(provider.sessionId, tier);
  }

  // Repair recorded admission→submit gaps only, never repeat a native input.
  // A later final/owner stop takes precedence over a pending recovery intent.
  try {
    const requests = await harness.snapshot(Requests, context);
    await Promise.all(Object.values(requests?.requests ?? {}).map(request => rememberTier(request.conversationId, request.input)));
    // Repair a persisted owner-stop intent before submitting any new work.
    for (const id of new Set(Object.values(requests?.requests ?? {}).map(request => request.conversationId))) {
      const campaign = await harness.snapshot(CampaignResearchDoc, id, context);
      if (campaign?.state === 'CLOSING' && campaign.closingReason === 'owner_stop') await settleOwnerStop(id);
    }
    for (const [key, request] of Object.entries(requests?.requests ?? {})) {
      const requestId = key.slice('request:'.length);
      const missing = await harness.commit(async (tx) => {
        if (await tx.submissionByRequest(request.conversationId, requestId)) return false;
        return (await tx.doc(Study, request.conversationId)).final === null;
      }, context);
      if (!missing) continue;
      if (request.recovery) {
        if ((await latestSubmission(request.conversationId))?.id === request.recovery.previousSubmissionId) await resume(request.conversationId, request.recovery.previousSubmissionId);
      } else await (await conversation(request.conversationId)).submit({ type: 'input', content: researchInstructions(researchInput(request.input)), requestId }, context);
    }
    harness.resume();
  } catch (error) {
    try { await harness.close(context); }
    finally { try { await credentials?.close(); } finally { owner?.close(); } }
    throw error;
  }

  async function start(requestId: string, value: ResearchInput, originalAdmission = false, sourceEvidence?: CapturedReviewSource) {
    if (typeof requestId !== 'string' || !requestId) throw new Error('request_id_required');
    const input = researchInput(value);
    const id = await harness.commit(async (tx) => {
      const requests = await tx.doc(Requests);
      const key = `request:${requestId}`;
      const existing = requests.requests[key];
      if (existing) {
        if (existing.recovery) throw new Error('request_id_conflict');
        if (canonical(existing.input) !== canonical(input)) throw new Error('request_id_conflict');
        if (sourceEvidence && (!existing.sourceEvidence || canonical(existing.sourceEvidence) !== canonical(JSON.parse(JSON.stringify(sourceEvidence))))) throw new Error('review_source_identity_conflict');
        return existing.conversationId;
      }
      if (isRecoveryRequestId(requestId)) {
        // Only the service's persisted original admission may repair this old gap;
        // even that cannot clone an actual legacy native continuation receipt.
        const priorConversation = Number(requestId.split(':')[1]);
        if (!originalAdmission || Number.isSafeInteger(priorConversation) && await tx.submissionByRequest(priorConversation as ConversationId, requestId)) throw new Error('request_id_conflict');
      }
      const model = models.getModel(input.model.provider, input.model.modelId);
      if (!model) throw new Error('model_unavailable');
      if (input.thinkingLevel !== undefined && !getSupportedThinkingLevels(model).includes(input.thinkingLevel)) throw new Error('thinking_level_unsupported');
      if (input.serviceTier !== undefined && !supportsServiceTier(model)) throw new Error('service_tier_unsupported');
      const thinkingLevel = input.thinkingLevel ?? clampThinkingLevel(model, requestedThinking);
      const created = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      await configure(tx, created.id, { model: input.model, thinkingLevel, instructions: researchInstructions(input) });
      (await tx.doc(Study, created.id)).input = freezeReviewPolicies(input);
      if (sourceEvidence) await bindReviewSourceEvidence(tx, created.id, input.data, sourceEvidence);
      if (input.campaign) {
        const campaign = await tx.doc(CampaignResearchDoc, created.id);
        campaign.binding = input.campaign;
        // Versioned technical allowance, not a market or scientific threshold.
        campaign.orchestration = { policyVersion: 'qz.research.resources/1', generationTaskIds: [] };
      }
      requests.requests[key] = { input, conversationId: created.id, ...(sourceEvidence ? { sourceEvidence: JSON.parse(JSON.stringify(sourceEvidence)) } : {}) };
      return created.id;
    }, context);
    await rememberTier(id, input);
    // Admission and owner stop share the same original Conversation boundary.
    // A final-state read alone cannot fence a later native submit after abort.
    return ownerChange(id, async () => {
      if ((await harness.snapshot(Study, id, context))?.deletedAt) throw Object.assign(new Error('research_deleted'), { statusCode: 409 });
      const previous = await harness.commit((tx) => tx.submissionByRequest(id, requestId), context);
      if (previous) return { conversationId: id, submissionId: previous.id };
      if ((await harness.snapshot(Study, id, context))?.final) throw new Error('research_already_final');
      const submission = await (await conversation(id)).submit({ type: 'input', content: researchInstructions(input), requestId }, context);
      return { conversationId: id, submissionId: submission.id };
    });
  }
  async function submissionFor(id: number) {
    const requests = await harness.snapshot(Requests, context);
    const binding = Object.entries(requests?.requests ?? {}).find(([, request]) => request.conversationId === id && !request.recovery);
    if (!binding) return null;
    return await harness.commit((tx) => tx.submissionByRequest(id as ConversationId, binding[0].slice('request:'.length)), context) ?? null;
  }

  async function latestSubmission(id: number) {
    let cursor: Cursor | undefined;
    do {
      const page = await storage.scanSubmissions({ conversationId: id as ConversationId, order: 'descending' }, 100, cursor, context);
      const input = page.items.find((item) => item.type === 'input');
      if (input) return input;
      cursor = page.next;
    } while (cursor);
    return null;
  }
  async function settleOwnerStop(id: ConversationId) {
    await (await conversation(id)).abort(context, { background: true });
    await harness.commit(async tx => {
      const campaign = await tx.doc(CampaignResearchDoc, id);
      if (!campaign.binding) {
        if (campaign.closingReason === 'owner_stop') campaign.state = 'CONCLUDED';
        return;
      }
      const study = await tx.doc(Study, id);
      for (const trial of study.trials.filter(item => item.status === 'STARTED')) {
        const task = await tx.task(trial.taskId as TaskId);
        if (!task || task.state.status !== 'terminal') throw new Error('owner_stop_task_not_settled');
        trial.status = 'FAILED'; trial.error = 'owner_stop';
        trial.errorDetail = { operation: 'research_trial', stage: 'owner_stop', taskOutcome: task.state.outcome.status };
        await settleCampaignTrial(tx, { conversationId: id, taskId: task.id }, null, 'owner_stop', null);
      }
      if (study.final?.status === 'STARTED') {
        study.final.status = 'FAILED'; study.final.error = 'owner_stop';
        study.final.errorDetail = { operation: 'independent_evaluation', stage: 'owner_stop' };
      }
      campaign.state = 'CONCLUDED';
    }, context);
  }
  async function state(id: number) {
    await conversation(id);
    return { research: await harness.snapshot(Study, id as ConversationId, context),
      campaignResearch: await harness.snapshot(CampaignResearchDoc, id as ConversationId, context),
      literature: await harness.snapshot(LiteratureDoc, id as ConversationId, context),
      reviews: await harness.snapshot(ReviewLedger, id as ConversationId, context),
      submission: await latestSubmission(id), originalSubmission: await submissionFor(id) };
  }
  async function diagnostics(id: number) {
    const current = await state(id);
    if (!current.research?.input) throw new Error('research_not_found');
    const page = await harness.commit((tx) => tx.scanTasks({ conversationId: id as ConversationId, kind: GenerationTask.definition.name, order: 'descending' }, 1), context);
    const generation = page.items[0];
    const outcome = generation?.state.status === 'terminal' ? generation.state.outcome : undefined;
    return { conversationId: id, originalSubmission: submissionView(current.originalSubmission), submission: submissionView(current.submission),
      generation: generation ? { taskId: generation.id as number, status: generation.state.status,
        outcome: outcome?.status ?? null, error: outcome && (outcome.status === 'failed' || outcome.status === 'faulted') ? modelErrorCode(outcome.error.message) : null } : null };
  }
  async function resume(id: number, previousId: number) {
    if (!Number.isSafeInteger(previousId) || previousId < 1) throw new Error('invalid_submission_id');
    const handle = await conversation(id);
    if ((await harness.snapshot(Study, handle.id, context))?.deletedAt) throw Object.assign(new Error('research_deleted'), { statusCode: 409 });
    const requestId = recoveryRequestId(id, previousId);
    const existing = await harness.commit(async tx => {
      await options.authorizeResume?.(tx, handle.id);
      return tx.submissionByRequest(handle.id, requestId);
    }, context);
    if (existing) {
      if (existing.type !== 'input' || existing.id <= previousId || existing.conversationId !== handle.id) throw new Error('recovery_request_conflict');
      return { conversationId: id, submissionId: existing.id, previousSubmissionId: previousId };
    }
    const current = await state(id);
    const mode = await harness.commit(async (tx) => {
      await options.authorizeResume?.(tx, handle.id);
      const study = await tx.doc(Study, handle.id);
      const requests = await tx.doc(Requests); const key = `request:${requestId}`;
      const intent = requests.requests[key];
      if (!study.input) throw new Error('research_not_found');
      if (intent && (!intent.recovery || intent.conversationId !== handle.id || intent.recovery.previousSubmissionId !== previousId || canonical(intent.input) !== canonical(study.input))) throw new Error('recovery_request_conflict');
      const ownerAborted = current.submission?.status === 'unanswered' && current.submission.reason === 'aborted';
      const stopped = study.final?.status === 'STOPPED' && study.final.trialId === null && study.final.result === null
        && (current.submission?.status === 'done' || ownerAborted && study.final.reason === 'owner_stop');
      if (study.final && (intent || !stopped)) throw new Error('research_already_final');
      if (current.submission?.id !== previousId) throw new Error('research_not_recoverable_conflict');
      const failed = current.submission.status === 'unanswered' && current.submission.reason === 'model_error';
      const interruptedStop = intent?.recovery?.mode === 'stopped' && (current.submission.status === 'done' || ownerAborted && study.previousStops?.some(stop => stop.previousSubmissionId === previousId && stop.final.reason === 'owner_stop'));
      if (!stopped && !failed && !interruptedStop) throw new Error('research_not_recoverable_conflict');
      const mode = intent?.recovery?.mode ?? (stopped ? 'stopped' : 'model_error');
      if (!intent) {
        const campaign = await tx.doc(CampaignResearchDoc, handle.id);
        if (study.final) { study.previousStops = [...(study.previousStops ?? []), { previousSubmissionId: previousId, final: study.final }]; study.final = null; }
        if (campaign.closureTaskId !== null) campaign.previousClosures = [...(campaign.previousClosures ?? []), { taskId: campaign.closureTaskId, reason: campaign.closingReason }];
        campaign.state = 'SEARCHING'; campaign.closureTaskId = null; campaign.closingReason = null;
        delete (await tx.doc(Continuation, handle.id)).evidenceSignature;
        requests.requests[key] = { input: study.input, conversationId: handle.id, recovery: { previousSubmissionId: previousId, mode } };
      }
      return mode;
    }, context);
    const next = await handle.submit({ type: 'input', content: recoveryInput(previousId, mode), requestId, whenBusy: 'reject' }, context);
    return { conversationId: id, submissionId: next.id, previousSubmissionId: previousId };
  }
  let closed: Promise<void> | undefined;
  return {
    harness, models, registry, withOwnerChange: ownerChange,
    async start(requestId: string, value: ResearchInput, originalAdmission = false, sourceEvidence?: CapturedReviewSource) {
      if (!accepting) throw new Error('research_closing');
      const admission = start(requestId, value, originalAdmission, sourceEvidence);
      admissions.add(admission);
      try { return await admission; } finally { admissions.delete(admission); }
    },
    state, diagnostics,
    // Share the native owner-command boundary with start/resume/stop. Deletion
    // never aborts a scientific task or discards its original ledger.
    async remove(id: number, validate: (tx: Tx) => Promise<void>, factorId?: string) {
      return ownerChange(id, async () => {
        const handle = await conversation(id);
        const tasks = await harness.commit(async tx => {
          const records = []; let cursor: Cursor | undefined;
          do { const page = await tx.scanTasks({ conversationId: handle.id }, 100, cursor); records.push(...page.items); cursor = page.next; } while (cursor);
          return records;
        }, context);
        await reconcileNativeRemoteTasks(harness, tasks, context);
        const current = await state(id);
        return harness.commit(async tx => {
          const study = await tx.doc(Study, handle.id);
          if (!study.input) throw new Error('research_not_found');
          const prior = factorId ? study.deletedFactors?.[factorId] : study.deletedAt;
          if (prior) return prior;
          if (study.deletedAt) throw new Error('research_not_found');
          if (['queued', 'placed'].includes(current.submission?.status ?? '')) throw Object.assign(new Error('research_delete_tasks_active'), { statusCode: 409 });
          let cursor: Cursor | undefined;
          do {
            const page = await tx.scanTasks({ conversationId: handle.id }, 100, cursor);
            if (page.items.some(task => task.state.status !== 'terminal')) throw Object.assign(new Error('research_delete_tasks_active'), { statusCode: 409 });
            for (const task of page.items) await assertNativeRemoteTaskSettled(tx, task.conversationId, task.id);
            cursor = page.next;
          } while (cursor);
          await validate(tx);
          const deletedAt = new Date().toISOString();
          if (factorId) study.deletedFactors = { ...study.deletedFactors, [factorId]: deletedAt };
          else study.deletedAt = deletedAt;
          return deletedAt;
        }, context);
      });
    },
    async resume(id: number, previousId: number) {
      if (!accepting) throw new Error('research_closing');
      const admission = ownerChange(id, () => resume(id, previousId)); admissions.add(admission);
      try { return await admission; } finally { admissions.delete(admission); }
    },
    async list(cursor?: Cursor) {
      return harness.commit((tx) => tx.scanConversations({ order: 'descending' }, 100, cursor), context);
    },
    /** Collection projections do not load literature, campaign records or task receipts. */
    async listState(id: number) {
      const [research, reviews, submission] = await Promise.all([
        harness.snapshot(Study, id as ConversationId, context),
        harness.snapshot(ReviewLedger, id as ConversationId, context),
        latestSubmission(id),
      ]);
      return { research, reviews, submission };
    },
    async view(id: number) {
      const watch = await (await conversation(id)).watch(context);
      try {
        return { ...watch.value, research: await harness.snapshot(Study, watch.value.conversation.id, context),
          campaignResearch: await harness.snapshot(CampaignResearchDoc, watch.value.conversation.id, context),
          literature: await harness.snapshot(LiteratureDoc, watch.value.conversation.id, context),
          reviews: await harness.snapshot(ReviewLedger, watch.value.conversation.id, context), submission: await submissionFor(id) };
      } finally { await watch.stop(); }
    },
    async watch(id: number): Promise<ConversationWatch> {
      if (!accepting) throw new Error('research_closing');
      const watch = await (await conversation(id)).watch(context);
      if (!accepting) { await watch.stop(); throw new Error('research_closing'); }
      watchers.add(watch);
      void watch.closed.then(() => watchers.delete(watch));
      return watch;
    },
    async stop(id: number) {
      return ownerChange(id, async () => {
        const study = await conversation(id);
        if ((await harness.snapshot(Study, study.id, context))?.deletedAt) throw Object.assign(new Error('research_deleted'), { statusCode: 409 });
        await harness.commit(async (tx) => {
          const facts = await tx.doc(Study, study.id);
          facts.final ??= { trialId: null, comparedTrials: facts.trials.map((trial) => trial.taskId!), status: 'STOPPED', reason: 'owner_stop', result: null };
          const campaign = await tx.doc(CampaignResearchDoc, study.id);
          if ((campaign.binding || campaign.closureTaskId !== null) && campaign.state !== 'CONCLUDED') { campaign.state = 'CLOSING'; campaign.closingReason = 'owner_stop'; }
        }, context);
        await settleOwnerStop(study.id);
        return this.view(id);
      });
    },
    async tasks(cursor?: Cursor) {
      return { ...await harness.inspect(context), records: await harness.commit((tx) => tx.scanTasks({ order: 'descending' }, 100, cursor), context) };
    },
    async submission(id: number) {
      if (!Number.isSafeInteger(id) || id < 0) throw new Error('invalid_submission_id');
      const submission = await harness.submission(id as SubmissionId, context);
      if (!submission) throw new Error('submission_not_found');
      return submission.status(context);
    },
    submissionFor,
    async usage() { return harness.usage(context); },
    async availableModels(providerId?: string) {
      try { return (await models.getAvailable(providerId)).map((model) => ({ provider: model.provider, modelId: model.id, name: model.name, reasoning: model.reasoning, thinkingLevels: [...getSupportedThinkingLevels(model)], effectiveThinkingLevel: clampThinkingLevel(model, requestedThinking), serviceTiers: supportsServiceTier(model) ? ['default' as const, 'priority' as const] : [] })); }
      catch { throw new Error('provider_status_unavailable'); }
    },
    close() {
      if (!closed) {
        accepting = false;
        closed = (async () => {
          await Promise.allSettled([...admissions]);
          await Promise.all([...watchers].map((watch) => watch.stop()));
          try { await harness.close(context); } // Suspend native work, never imply cancellation.
          finally { try { await credentials?.close(); } finally { owner?.close(); } }
        })();
      }
      return closed;
    },
  };
}

export type ResearchHost = Awaited<ReturnType<typeof openResearch>>;
