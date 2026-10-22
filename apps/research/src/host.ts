import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { clampThinkingLevel, getSupportedThinkingLevels } from '@earendil-works/pi-ai/models';
import type { Models } from '@earendil-works/pi-ai/models';
import type { ModelThinkingLevel } from '@earendil-works/pi-ai';
import { configure, createRegistry, GenerationTask, Harness, ProviderDoc } from '@earendil-works/pi-durable';
import type { ConversationId, ConversationWatch, Cursor, Extension, HarnessOptions, Storage, SubmissionId } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { pathToFileURL } from 'node:url';
import { resolve, dirname } from 'node:path';
import { mkdir } from 'node:fs/promises';
import { canonical, Requests, Study } from './documents.ts';
import { LiteratureDoc } from './literature-document.ts';
import { researchInput } from './input.ts';
import { researchInstructions } from '../../../packages/contracts/src/research.ts';
import type { ResearchInput } from './documents.ts';
import { configuredModels, subscriptionSpeed, supportsServiceTier } from './models.ts';
import { ModelCredentials } from './credentials.ts';
import { researchSettings } from './settings.ts';
import { researchContinuation } from './continuation.ts';

import { isRecoveryRequestId, modelErrorCode, recoveryInput, recoveryRequestId, submissionView } from './recovery.ts';
const context = BACKGROUND_CONTEXT;

export async function openResearch(options: {
  database: string;
  extensions?: readonly Extension[];
  modules?: readonly string[];
  providerModules?: readonly string[];
  models?: Models;
  storage?: Storage;
  settings?: HarnessOptions['settings'];
  thinkingLevel?: ModelThinkingLevel;
}) {
  const settings = researchSettings(options.settings);
  const requestedThinking = options.thinkingLevel ?? 'medium';
  if (!['off', 'minimal', 'low', 'medium', 'high', 'xhigh', 'max'].includes(requestedThinking)) throw new Error('invalid_thinking_level');
  const credentials = options.models ? undefined : await ModelCredentials.open(`${resolve(options.database)}.auth`);
  let models: Models;
  let speed!: ReturnType<typeof subscriptionSpeed>;
  let harness!: Harness;
  let storage!: Storage;
  const registry = createRegistry();
  try {
    models = options.models ?? await configuredModels(options.providerModules, credentials);
    speed = subscriptionSpeed(models);
    models = speed.models;
    for (const extension of options.extensions ?? []) registry.install(extension);
    for (const path of options.modules ?? []) {
      const module = await import(pathToFileURL(resolve(path)).href);
      registry.install(module.default as Extension);
    }
    registry.install(researchContinuation(() => harness));
    await mkdir(dirname(resolve(options.database)), { recursive: true });
    storage = options.storage ?? await openNodeSqliteStorage(options.database);
    harness = await Harness.open(storage, { models, registry, settings }, context);
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
  } catch (error) { await harness.close(context); await credentials?.close(); throw error; }

  async function start(requestId: string, value: ResearchInput, originalAdmission = false) {
    if (typeof requestId !== 'string' || !requestId) throw new Error('request_id_required');
    const input = researchInput(value);
    const id = await harness.commit(async (tx) => {
      const requests = await tx.doc(Requests);
      const key = `request:${requestId}`;
      const existing = requests.requests[key];
      if (existing) {
        if (existing.recovery) throw new Error('request_id_conflict');
        if (canonical(existing.input) !== canonical(input)) throw new Error('request_id_conflict');
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
      (await tx.doc(Study, created.id)).input = input;
      requests.requests[key] = { input, conversationId: created.id };
      return created.id;
    }, context);
    await rememberTier(id, input);
    const previous = await harness.commit((tx) => tx.submissionByRequest(id, requestId), context);
    if (previous) return { conversationId: id, submissionId: previous.id };
    if ((await harness.snapshot(Study, id, context))?.final) throw new Error('research_already_final');
    const submission = await (await conversation(id)).submit({ type: 'input', content: researchInstructions(input), requestId }, context);
    return { conversationId: id, submissionId: submission.id };
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
  async function state(id: number) {
    await conversation(id);
    return { research: await harness.snapshot(Study, id as ConversationId, context),
      literature: await harness.snapshot(LiteratureDoc, id as ConversationId, context),
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
    const requestId = recoveryRequestId(id, previousId);
    const existing = await harness.commit((tx) => tx.submissionByRequest(handle.id, requestId), context);
    if (existing) {
      if (existing.type !== 'input' || existing.id <= previousId || existing.conversationId !== handle.id) throw new Error('recovery_request_conflict');
      return { conversationId: id, submissionId: existing.id, previousSubmissionId: previousId };
    }
    const current = await state(id);
    const mode = await harness.commit(async (tx) => {
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
      const maximum = study.input.policy.max_trials;
      if (typeof maximum !== 'number' || study.trials.filter((trial) => trial.status !== 'REJECTED').length >= maximum) throw new Error('trial_budget_exhausted');
      const mode = intent?.recovery?.mode ?? (stopped ? 'stopped' : 'model_error');
      if (!intent) {
        if (study.final) { study.previousStops = [...(study.previousStops ?? []), { previousSubmissionId: previousId, final: study.final }]; study.final = null; }
        requests.requests[key] = { input: study.input, conversationId: handle.id, recovery: { previousSubmissionId: previousId, mode } };
      }
      return mode;
    }, context);
    const next = await handle.submit({ type: 'input', content: recoveryInput(previousId, mode), requestId, whenBusy: 'reject' }, context);
    return { conversationId: id, submissionId: next.id, previousSubmissionId: previousId };
  }
  let closed: Promise<void> | undefined;
  return {
    harness, models, registry,
    async start(requestId: string, value: ResearchInput, originalAdmission = false) {
      if (!accepting) throw new Error('research_closing');
      const admission = start(requestId, value, originalAdmission);
      admissions.add(admission);
      try { return await admission; } finally { admissions.delete(admission); }
    },
    state, diagnostics,
    async resume(id: number, previousId: number) {
      if (!accepting) throw new Error('research_closing');
      const admission = ownerChange(id, () => resume(id, previousId)); admissions.add(admission);
      try { return await admission; } finally { admissions.delete(admission); }
    },
    async list(cursor?: Cursor) {
      return harness.commit((tx) => tx.scanConversations({ order: 'descending' }, 100, cursor), context);
    },
    async view(id: number) {
      const watch = await (await conversation(id)).watch(context);
      try {
        return { ...watch.value, research: await harness.snapshot(Study, watch.value.conversation.id, context),
          literature: await harness.snapshot(LiteratureDoc, watch.value.conversation.id, context), submission: await submissionFor(id) };
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
        await harness.commit(async (tx) => {
          const facts = await tx.doc(Study, study.id);
          facts.final ??= { trialId: null, comparedTrials: facts.trials.map((trial) => trial.taskId!), status: 'STOPPED', reason: 'owner_stop', result: null };
        }, context);
        await study.abort(context, { background: true });
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
