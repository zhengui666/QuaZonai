import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { configuredModels } from './models.ts';
import type { Models } from '@earendil-works/pi-ai/models';
import { configure, createRegistry, Harness } from '@earendil-works/pi-durable';
import type { ConversationId, Cursor, Extension, Storage } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { pathToFileURL } from 'node:url';
import { resolve } from 'node:path';
import { mkdir } from 'node:fs/promises';
import { dirname } from 'node:path';
import { canonical, Requests, Study } from './documents.ts';
import { researchInput } from './input.ts';
import type { ResearchInput } from './documents.ts';

const context = BACKGROUND_CONTEXT;

export async function openResearch(options: {
  database: string;
  extensions?: readonly Extension[];
  modules?: readonly string[];
  providerModules?: readonly string[];
  models?: Models;
  storage?: Storage;
}) {
  const models = options.models ?? await configuredModels(options.providerModules);
  const registry = createRegistry();
  for (const extension of options.extensions ?? []) registry.install(extension);
  for (const path of options.modules ?? []) {
    const module = await import(pathToFileURL(resolve(path)).href);
    const extension = module.default as Extension;
    registry.install(extension);
  }
  await mkdir(dirname(resolve(options.database)), { recursive: true });
  const storage = options.storage ?? await openNodeSqliteStorage(options.database);
  const harness = await Harness.open(storage, { models, registry }, context);
  harness.resume();

  async function conversation(id: number) {
    if (!Number.isSafeInteger(id) || id < 0) throw new Error('invalid_research_id');
    const value = await harness.conversation(id as ConversationId, context);
    if (!value) throw new Error('research_not_found');
    return value;
  }

  return {
    harness, models, registry,
    async start(requestId: string, value: ResearchInput) {
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
        if (!models.getModel(input.model.provider, input.model.modelId)) throw new Error('model_unavailable');
        const created = await tx.createConversation({ ownership: { kind: 'ownerless' } });
        await configure(tx, created.id, { model: input.model, instructions: input.goal });
        const study = await tx.doc(Study, created.id);
        study.input = input;
        requests.requests[key] = { input, conversationId: created.id };
        return created.id;
      }, context);
      const study = await conversation(id);
      // A crash between creation and submission is repaired by replaying this same request.
      const submission = await study.submit({ type: 'input', content: input.goal, requestId }, context);
      return { conversationId: study.id, submissionId: submission.id };
    },
    async list(cursor?: Cursor) {
      return harness.commit((tx) => tx.scanConversations({ order: 'descending' }, 100, cursor), context);
    },
    async view(id: number) {
      const watch = await (await conversation(id)).watch(context);
      try {
        return { ...watch.value, research: await harness.snapshot(Study, watch.value.conversation.id, context) };
      } finally { await watch.stop(); }
    },
    async watch(id: number) { return (await conversation(id)).watch(context); },
    async stop(id: number) {
      await (await conversation(id)).abort(context, { background: true });
      return this.view(id);
    },
    async tasks() {
      return { ...await harness.inspect(context), records: await harness.commit((tx) => tx.scanTasks({ order: 'descending' }, 100), context) };
    },
    async usage() { return harness.usage(context); },
    async availableModels() {
      return (await models.getAvailable()).map((model) => ({ provider: model.provider, modelId: model.id, name: model.name, reasoning: model.reasoning }));
    },
    async close() { await harness.close(context); },
  };
}

export type ResearchHost = Awaited<ReturnType<typeof openResearch>>;
