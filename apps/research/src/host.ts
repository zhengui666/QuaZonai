import researchAssessment from '../plugins/research-assessment.ts';
import nativeStrategyBacktest from '../plugins/native-strategy-backtest.ts';
import { assertResearchRunSettled } from './research-configuration.ts';
import researchTools, { nativeResearchExtension } from '../plugins/research.ts';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { clampThinkingLevel, getSupportedThinkingLevels } from '@earendil-works/pi-ai/models';
import type { Models } from '@earendil-works/pi-ai/models';
import type { ModelThinkingLevel } from '@earendil-works/pi-ai';
import { configure, createRegistry, defineTask, GenerationTask, Harness, ProviderDoc } from '@earendil-works/pi-durable';
import type { ConversationId, ConversationWatch, Cursor, Extension, HarnessOptions, Storage, SubmissionId, TaskId, Tx } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { pathToFileURL } from 'node:url';
import { resolve, dirname } from 'node:path';
import { mkdir } from 'node:fs/promises';
import { canonicalJsonDigest } from './native-json.ts';
import { canonical, Requests, Study } from './documents.ts';
import { ReviewLedger } from './review-documents.ts';
import { bindReviewSourceEvidence, ReviewSourceValidationTask, reviewSourceCaptureIdentity } from './review-source.ts';
import type { CapturedReviewSource } from './review-source.ts';
import { defineExtension } from '@earendil-works/pi-durable';
import { LiteratureDoc } from './literature-document.ts';
import { researchInput } from './input.ts';
import { researchInstructions } from '../../../packages/contracts/src/research.ts';
import type { ResearchInput } from './documents.ts';
import { configuredModels, subscriptionSpeed, supportsServiceTier, ModelTiers } from './models.ts';
import { ModelCredentials } from './credentials.ts';
import { researchSettings } from './settings.ts';
import { assertNativeRemoteTaskSettled, reconcileNativeRemoteTasks } from './native-remote-receipts.ts';
import { CampaignResearchDoc } from './pm-campaign.ts';
import { settleCampaignTrial } from './pm-ledger.ts';

import { NativeExecutionTask, withNativeExecutionHost } from './native-execution.ts';
import { ResearchIntakeTask } from './intake.ts';
import { isRecoveryRequestId, modelErrorCode, recoveryInput, recoveryRequestId, submissionView } from './recovery.ts';
const context = BACKGROUND_CONTEXT;

export async function openResearch(options: {
  database: string;
  deferResume?: boolean;
  extensions?: readonly Extension[];
  modules?: readonly string[];
  providerModules?: readonly string[];
  models?: Models;
  storage?: Storage;
  settings?: HarnessOptions['settings'];
  thinkingLevel?: ModelThinkingLevel;
  /** Service-owned parent admission, applied to original native recovery too. */
  authorizeResume?: (tx: Tx, id: ConversationId) => Promise<void>;
  authorizeAdmission?: (tx: Tx, input: ResearchInput, metadata?: import('@earendil-works/pi-durable').JsonObject) => Promise<void>;
}) {
  const settings = researchSettings(typeof options.settings === 'function' ? options.settings() : options.settings);
  const requestedThinking = options.thinkingLevel ?? 'medium';
  if (!['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'].includes(requestedThinking)) throw new Error('invalid_thinking_level');
  const credentials = options.models ? undefined : await ModelCredentials.open(`${resolve(options.database)}.auth`);
  let models: Models;
  let speed!: ReturnType<typeof subscriptionSpeed>;
  let harness!: Harness;
  let storage!: Storage;
  const registry = createRegistry();
  const OwnerStopTask = defineTask<{ conversationId: ConversationId }, { phase: 'stop' }, null>({
    name: 'qz.research.owner-stop', version: 1, initial: () => ({ phase: 'stop' }),
    phases: { async stop(task, runtime, taskContext) {
      await settleOwnerStop(task.input.conversationId, taskContext);
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), taskContext);
    } },
    abort: (_task, runtime, taskContext) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), taskContext),
  });
  try {
    models = options.models ?? await configuredModels(options.providerModules, credentials);
    speed = subscriptionSpeed(models, async id => (await harness.snapshot(ModelTiers, context))?.tiers[id]);
    models = speed.models;
    for (const extension of options.extensions ?? []) registry.install(extension);
    for (const path of options.modules ?? []) {
      const module = await import(pathToFileURL(resolve(path)).href);
      registry.install(module.default as Extension);
    }
    registry.install(researchAssessment);
    registry.install(nativeStrategyBacktest);
    registry.install(nativeResearchExtension(registry.snapshot().installed().find(extension => extension.name === 'qz.research') ?? researchTools));
    registry.install(defineExtension({ name: 'qz.research-runtime', tasks: [OwnerStopTask, NativeExecutionTask, ResearchIntakeTask, ReviewSourceValidationTask] }));
    await mkdir(dirname(resolve(options.database)), { recursive: true });
    storage = options.storage ?? await openNodeSqliteStorage(options.database);
    harness = await Harness.open(storage, { models, registry, settings: { ...settings, extensions: settings.extensions ?? registry.snapshot().installed().filter(extension => !['qz.review', 'qz.research-native'].includes(extension.name)) } }, context);
  } catch (error) { await credentials?.close(); throw error; }
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

  // The official scheduler resumes intake phases and admitted work.
  if (!options.deferResume) harness.resume();
  const controller = await harness.root(context);

  async function start(requestId: string, value: ResearchInput, _originalAdmission = false, sourceEvidence?: CapturedReviewSource, metadata?: import('@earendil-works/pi-durable').JsonObject) {
    if (typeof requestId !== 'string' || !requestId) throw new Error('request_id_required');
    if (isRecoveryRequestId(requestId)) throw new Error('request_id_conflict');
    const input = structuredClone(researchInput(value));
    const inputDigest = canonicalJsonDigest(input);
    const taskId = await controller.commit(async tx => {
      const requests = await tx.doc(Requests); const key = `request:${requestId}`;
      const existing = requests.requests[key];
      if (existing) {
        if (existing.conversationId !== null && (await tx.doc(Study, existing.conversationId)).deletedAt) throw Object.assign(new Error('research_deleted'), { statusCode: 409 });
        if (existing.recovery || existing.inputDigest !== inputDigest || !existing.taskId) throw new Error('request_id_conflict');
        if (sourceEvidence && (!existing.sourceEvidence || canonical(reviewSourceCaptureIdentity(existing.sourceEvidence as unknown as CapturedReviewSource)) !== canonical(reviewSourceCaptureIdentity(sourceEvidence)))) throw new Error('review_source_identity_conflict');
        if (metadata && canonical(existing.metadata ?? null) !== canonical(metadata)) throw new Error('request_id_conflict');
        return existing.taskId;
      }
      await options.authorizeAdmission?.(tx, input, metadata);
      if (input.campaign) throw Object.assign(new Error('custom_research_campaign_retired'), { statusCode: 410 });
      if (['reviewPolicy', 'reviewPolicies', 'reviewedFactorSources'].some(key => input.policy[key] !== undefined)) throw Object.assign(new Error('legacy_research_scoring_retired'), { statusCode: 410 });
    const model = models.getModel(input.model.provider, input.model.modelId);
    if (!model) throw new Error('model_unavailable');
    if (input.thinkingLevel !== undefined && !getSupportedThinkingLevels(model).includes(input.thinkingLevel)) throw new Error('thinking_level_unsupported');
    if (input.serviceTier !== undefined && !supportsServiceTier(model)) throw new Error('service_tier_unsupported');
    const thinkingLevel = input.thinkingLevel ?? clampThinkingLevel(model, requestedThinking);
      const source = sourceEvidence ? JSON.parse(JSON.stringify(sourceEvidence)) : undefined;
      const taskId = await tx.createTask(ResearchIntakeTask, { requestId, input, thinkingLevel, toolPolicy: 'NAUTILUS_ONLY', ...(source ? { sourceEvidence: source } : {}) }, { ownership: { kind: 'conversation' } });
      requests.requests[key] = { input, inputDigest, conversationId: null, submissionId: null, taskId, ...(source ? { sourceEvidence: source } : {}), ...(metadata ? { metadata } : {}) };
      return taskId;
    }, context);
    const task = await harness.waitForTask(taskId, context);
    if (task.state.outcome.status !== 'completed') throw new Error('error' in task.state.outcome ? task.state.outcome.error?.message ?? 'research_intake_failed' : 'research_intake_failed');
    return task.state.outcome.result;
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
      const input = page.items.find((item) => item.type !== 'write');
      if (input) return input;
      cursor = page.next;
    } while (cursor);
    return null;
  }
  async function settleOwnerStopFacts(tx: Tx, id: ConversationId) {
    const campaign = await tx.doc(CampaignResearchDoc, id);
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
  }
  async function settleOwnerStop(id: ConversationId, stopContext = context) {
    await (await conversation(id)).abort(stopContext, { background: true });
    await harness.commit(tx => settleOwnerStopFacts(tx, id), stopContext);
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
  async function resume(id: number, previousId: number, instruction?: string) {
    if (instruction !== undefined && (typeof instruction !== 'string' || !instruction.trim())) throw new Error('invalid_research_resume');
    if (!Number.isSafeInteger(previousId) || previousId < 1) throw new Error('invalid_submission_id');
    const handle = await conversation(id);
    if ((await harness.snapshot(Study, handle.id, context))?.deletedAt) throw Object.assign(new Error('research_deleted'), { statusCode: 409 });
    const requestId = recoveryRequestId(id, previousId);
    const existing = await harness.commit(async tx => {
      await options.authorizeResume?.(tx, handle.id);
      const submission = await tx.submissionByRequest(handle.id, requestId);
      if (submission && (await tx.doc(Requests)).requests[`request:${requestId}`]?.recovery?.instruction !== instruction) throw new Error('recovery_request_conflict');
      return submission;
    }, context);
    if (existing) {
      if (!['input', 'continue'].includes(existing.type) || existing.id <= previousId || existing.conversationId !== handle.id) throw new Error('recovery_request_conflict');
      return { conversationId: id, submissionId: existing.id, previousSubmissionId: previousId };
    }
    const current = await state(id);
    const nativeTasks = async (tx: Tx) => {
      const records = []; let cursor: Cursor | undefined;
      do { const page = await tx.scanTasks({ conversationId: handle.id }, 100, cursor);
        records.push(...page.items.filter(task => task.kind === 'qz.native-strategy.backtest' || task.kind === NativeExecutionTask.definition.name)); cursor = page.next;
      } while (cursor);
      return records;
    };
    await reconcileNativeRemoteTasks(harness, await harness.commit(nativeTasks, context), context);
    const nextId = await harness.commit(async (tx) => {
      await options.authorizeResume?.(tx, handle.id);
      const study = await tx.doc(Study, handle.id);
      const requests = await tx.doc(Requests); const key = `request:${requestId}`;
      const intent = requests.requests[key];
      if (!study.input) throw new Error('research_not_found');
      if (intent && (!intent.recovery || intent.conversationId !== handle.id || intent.recovery.previousSubmissionId !== previousId || intent.recovery.instruction !== instruction || canonical(intent.input) !== canonical(study.input))) throw new Error('recovery_request_conflict');
      const failed = current.submission?.status === 'unanswered' && current.submission.reason === 'model_error';
      const ownerAborted = current.submission?.status === 'unanswered' && current.submission.reason === 'aborted';
      const nativeContinuation = study.toolPolicy === 'NAUTILUS_ONLY' && !study.input.campaign && !study.trials.length
        && (current.submission?.status === 'done' && instruction !== undefined || ownerAborted || failed && !!study.final)
        && (!study.final || study.final.scientificClaim === 'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION');
      if (nativeContinuation) await assertResearchRunSettled(tx, handle.id);
      const stopped = study.final?.status === 'STOPPED' && study.final.trialId === null && study.final.result === null
        && (current.submission?.status === 'done' || ownerAborted && study.final.reason === 'owner_stop');
      if (study.final && (intent || !stopped && !nativeContinuation)) throw new Error('research_already_final');
      if (current.submission?.id !== previousId) throw new Error('research_not_recoverable_conflict');
      const interruptedStop = intent?.recovery?.mode === 'stopped' && (current.submission.status === 'done' || ownerAborted && study.previousStops?.some(stop => stop.previousSubmissionId === previousId && stop.final.reason === 'owner_stop'));
      if (!stopped && !failed && !interruptedStop && !nativeContinuation) throw new Error('research_not_recoverable_conflict');
      if (study.toolPolicy === 'NAUTILUS_ONLY' && (stopped || nativeContinuation)) for (const task of await nativeTasks(tx)) {
        if (task.state.status !== 'terminal') throw Object.assign(new Error('native_strategy_execution_unsettled'), { statusCode: 409 });
        await assertNativeRemoteTaskSettled(tx, handle.id, task.id);
      }
      if (stopped && study.final?.reason === 'owner_stop' || nativeContinuation && ownerAborted) await settleOwnerStopFacts(tx, handle.id);
      const mode = intent?.recovery?.mode ?? (nativeContinuation ? 'native' : stopped ? 'stopped' : 'model_error');
      const submissionId = await handle.submit(tx, mode === 'model_error' && instruction === undefined ? { type: 'continue', requestId } : { type: 'input', content: recoveryInput(previousId, mode) + (instruction === undefined ? '' : `\n\nOwner instruction:\n${instruction}`), requestId, whenBusy: 'reject' });
      if (!intent) {
        delete (await tx.doc(CampaignResearchDoc, handle.id)).ownerStopTaskId;
        if (study.final && mode === 'native') { study.previousNativeResults = [...(study.previousNativeResults ?? []), { previousSubmissionId: previousId, final: study.final }]; study.final = null; }
        if (study.final) { study.previousStops = [...(study.previousStops ?? []), { previousSubmissionId: previousId, final: study.final }]; study.final = null; }
        requests.requests[key] = { input: study.input, conversationId: handle.id, recovery: { previousSubmissionId: previousId, mode, ...(instruction !== undefined ? { instruction } : {}) } };
      }
      return submissionId;
    }, context);
    return { conversationId: id, submissionId: nextId, previousSubmissionId: previousId };
  }
  let closed: Promise<void> | undefined;
  return {
    harness, models, registry, withOwnerChange: ownerChange,
    withNativeHost<T>(operation: () => T): T { return withNativeExecutionHost(harness, controller.id, operation); },
    async start(requestId: string, value: ResearchInput, originalAdmission = false, sourceEvidence?: CapturedReviewSource, metadata?: import('@earendil-works/pi-durable').JsonObject) {
      if (!accepting) throw new Error('research_closing');
      const admission = start(requestId, value, originalAdmission, sourceEvidence, metadata);
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
    async resume(id: number, previousId: number, instruction?: string) {
      if (!accepting) throw new Error('research_closing');
      const admission = ownerChange(id, () => resume(id, previousId, instruction)); admissions.add(admission);
      try { return await admission; } finally { admissions.delete(admission); }
    },
    async list(cursor?: Cursor) {
      const page = await harness.commit((tx) => tx.scanConversations({ order: 'descending' }, 100, cursor), context);
      return { ...page, items: page.items.filter(item => item.id !== controller.id) };
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
        const stopTaskId = await harness.commit(async (tx) => {
          const facts = await tx.doc(Study, study.id);
          facts.final ??= { trialId: null, comparedTrials: facts.trials.map((trial) => trial.taskId!), status: 'STOPPED', reason: 'owner_stop', result: null };
          const campaign = await tx.doc(CampaignResearchDoc, study.id);
          if (campaign.state !== 'CONCLUDED') { campaign.state = 'CLOSING'; campaign.closingReason = 'owner_stop'; }
          campaign.ownerStopTaskId ??= await tx.createTask(OwnerStopTask, { conversationId: study.id }, { conversationId: controller.id, ownership: { kind: 'conversation' } });
          return campaign.ownerStopTaskId;
        }, context);
        const stopped = await harness.waitForTask(stopTaskId, context);
        if (stopped.state.outcome.status !== 'completed') throw new Error('research_owner_stop_failed');
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
          finally { await credentials?.close(); }
        })();
      }
      return closed;
    },
  };
}

export type ResearchHost = Awaited<ReturnType<typeof openResearch>>;
