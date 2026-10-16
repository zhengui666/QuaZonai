import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { clampThinkingLevel, getSupportedThinkingLevels } from '@earendil-works/pi-ai/models';
import type { Models } from '@earendil-works/pi-ai/models';
import type { ModelThinkingLevel } from '@earendil-works/pi-ai';
import { configure, createRegistry, Harness } from '@earendil-works/pi-durable';
import type { ConversationId, ConversationWatch, Cursor, Extension, HarnessOptions, Storage, SubmissionId } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { pathToFileURL } from 'node:url';
import { resolve, dirname } from 'node:path';
import { mkdir } from 'node:fs/promises';
import { canonical, Requests, Study } from './documents.ts';
import { researchInput } from './input.ts';
import type { ResearchInput } from './documents.ts';
import { configuredModels } from './models.ts';
import { ModelCredentials } from './credentials.ts';
import { researchSettings } from './settings.ts';
import { researchContinuation } from './continuation.ts';

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
  let harness!: Harness;
  const registry = createRegistry();
  try {
    models = options.models ?? await configuredModels(options.providerModules, credentials);
    for (const extension of options.extensions ?? []) registry.install(extension);
    for (const path of options.modules ?? []) {
      const module = await import(pathToFileURL(resolve(path)).href);
      registry.install(module.default as Extension);
    }
    registry.install(researchContinuation(() => harness));
    await mkdir(dirname(resolve(options.database)), { recursive: true });
    const storage = options.storage ?? await openNodeSqliteStorage(options.database);
    harness = await Harness.open(storage, { models, registry, settings }, context);
  } catch (error) { await credentials?.close(); throw error; }
  let accepting = true;
  const admissions = new Set<Promise<unknown>>();
  const watchers = new Set<ConversationWatch>();

  async function conversation(id: number) {
    if (!Number.isSafeInteger(id) || id < 0) throw new Error('invalid_research_id');
    const value = await harness.conversation(id as ConversationId, context);
    if (!value) throw new Error('research_not_found');
    return value;
  }

  // Repair only the original admission→submit gap. Existing native submissions
  // (including unanswered) are never resubmitted; stopped/final studies stay put.
  try {
    const requests = await harness.snapshot(Requests, context);
    for (const [key, request] of Object.entries(requests?.requests ?? {})) {
      const requestId = key.slice('request:'.length);
      const missing = await harness.commit(async (tx) => {
        if (await tx.submissionByRequest(request.conversationId, requestId)) return false;
        return (await tx.doc(Study, request.conversationId)).final === null;
      }, context);
      if (missing) await (await conversation(request.conversationId)).submit({ type: 'input', content: researchInput(request.input).goal, requestId }, context);
    }
    harness.resume();
  } catch (error) { await harness.close(context); await credentials?.close(); throw error; }

  async function start(requestId: string, value: ResearchInput) {
    if (typeof requestId !== 'string' || !requestId) throw new Error('request_id_required');
    const input = researchInput(value);
    const id = await harness.commit(async (tx) => {
      const requests = await tx.doc(Requests);
      const key = `request:${requestId}`;
      const existing = requests.requests[key];
      if (existing) {
        if (canonical(existing.input) !== canonical(input)) throw new Error('request_id_conflict');
        return existing.conversationId;
      }
      const model = models.getModel(input.model.provider, input.model.modelId);
      if (!model) throw new Error('model_unavailable');
      if (input.thinkingLevel !== undefined && !getSupportedThinkingLevels(model).includes(input.thinkingLevel)) throw new Error('thinking_level_unsupported');
      const thinkingLevel = input.thinkingLevel ?? clampThinkingLevel(model, requestedThinking);
      const created = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      await configure(tx, created.id, { model: input.model, thinkingLevel, instructions: input.goal });
      (await tx.doc(Study, created.id)).input = input;
      requests.requests[key] = { input, conversationId: created.id };
      return created.id;
    }, context);
    const previous = await harness.commit((tx) => tx.submissionByRequest(id, requestId), context);
    if (previous) return { conversationId: id, submissionId: previous.id };
    if ((await harness.snapshot(Study, id, context))?.final) throw new Error('research_already_final');
    const submission = await (await conversation(id)).submit({ type: 'input', content: input.goal, requestId }, context);
    return { conversationId: id, submissionId: submission.id };
  }
  async function submissionFor(id: number) {
    const requests = await harness.snapshot(Requests, context);
    const binding = Object.entries(requests?.requests ?? {}).find(([, request]) => request.conversationId === id);
    if (!binding) return null;
    return await harness.commit((tx) => tx.submissionByRequest(id as ConversationId, binding[0].slice('request:'.length)), context) ?? null;
  }

  let closed: Promise<void> | undefined;
  return {
    harness, models, registry,
    async start(requestId: string, value: ResearchInput) {
      if (!accepting) throw new Error('research_closing');
      const admission = start(requestId, value);
      admissions.add(admission);
      try { return await admission; } finally { admissions.delete(admission); }
    },
    async list(cursor?: Cursor) {
      return harness.commit((tx) => tx.scanConversations({ order: 'descending' }, 100, cursor), context);
    },
    async view(id: number) {
      const watch = await (await conversation(id)).watch(context);
      try {
        return { ...watch.value, research: await harness.snapshot(Study, watch.value.conversation.id, context), submission: await submissionFor(id) };
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
      const study = await conversation(id);
      await harness.commit(async (tx) => {
        const facts = await tx.doc(Study, study.id);
        facts.final ??= { trialId: null, comparedTrials: facts.trials.map((trial) => trial.taskId!), status: 'STOPPED', reason: 'owner_stop', result: null };
      }, context);
      await study.abort(context, { background: true });
      return this.view(id);
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
      try { return (await models.getAvailable(providerId)).map((model) => ({ provider: model.provider, modelId: model.id, name: model.name, reasoning: model.reasoning, thinkingLevels: [...getSupportedThinkingLevels(model)], effectiveThinkingLevel: clampThinkingLevel(model, requestedThinking) })); }
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
