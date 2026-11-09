import { createHash } from 'node:crypto';
import { mkdir, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc, defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import { atomicWriteFile } from '../../research/src/durable-files.ts';
import { canonical } from '../../research/src/documents.ts';
import { Application, json, Problem } from './facts.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';

const context = BACKGROUND_CONTEXT;
type Captured = { identity: string; privateInput: JsonObject };
type LoadCheckpoint = { phase: 'load' } | { phase: 'publish' } | { phase: 'wait'; resume: 'load' | 'publish'; wakeAt: number; reason: string };
function blockedInput(error: unknown): boolean {
  const seen = new Set<Error>(); let current = error;
  while (current instanceof Error && !seen.has(current)) {
    seen.add(current);
    const status = (current as Error & { status?: number; statusCode?: number }).status ?? (current as Error & { statusCode?: number }).statusCode;
    if (/^data_source_(?:plugin_version_unavailable|input_corrupt)/.test(current.message)
      || ['ENOENT', 'EIO', 'ENOSPC', 'EROFS', 'EMFILE', 'ENFILE', 'ECONNRESET', 'ECONNREFUSED', 'ETIMEDOUT', 'EAI_AGAIN', 'EHOSTUNREACH', 'ENETUNREACH', 'UND_ERR_CONNECT_TIMEOUT', 'UND_ERR_HEADERS_TIMEOUT', 'UND_ERR_BODY_TIMEOUT', 'UND_ERR_SOCKET'].includes(String((current as NodeJS.ErrnoException).code))
      || status === 408 || status === 429 || typeof status === 'number' && status >= 500 && status <= 599
      || /^(?:fetch failed|HTTP (?:408|429|5\d\d)\b)/i.test(current.message)) return true;
    current = current.cause;
  }
  return false;
}
interface Operations {
  capture(name: string): Promise<Captured>;
  load(name: string, input: JsonObject, signal: AbortSignal): Promise<JsonObject>;
  publish(name: string, input: JsonObject, result: JsonObject): Promise<JsonObject>;
  failed(name: string, input: JsonObject): Promise<void>;
}
export const DataSourceLoads = defineDoc<{ records: { [key: string]: JsonObject } }>({
  kind: 'qz.data-source-loads', version: 1, scope: 'session', initial: () => ({ records: {} }),
});

/** Official Durable owns execution. Password-bearing plugin configuration is
 * retained only in private mode-0600 files, never in a model/task document. */
export function createDataSourceTasks(dataDir: string, operations: Operations) {
  let host: ResearchHost | undefined;
  const Refresh = defineTask<{ key: string; source: string; identity: string; privateFile: string }, LoadCheckpoint, JsonObject>({
    name: 'qz.data-source.refresh', version: 1, initial: () => ({ phase: 'load' }),
    phases: {
      async load(task, runtime, ctx) {
        let captured: JsonObject | undefined;
        try {
          captured = JSON.parse(await readFile(task.input.privateFile, 'utf8')) as JsonObject;
          if (createHash('sha256').update(canonical(captured)).digest('hex') !== task.input.identity) throw new Error('data_source_input_corrupt');
          if (await runtime.memo<JsonObject>('loaded', ctx)) { await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'publish' } }), ctx); return; }
          const loaded = await operations.load(task.input.source, captured, runtime.signal);
          ctx.abortSignal?.throwIfAborted(); runtime.signal.throwIfAborted();
          await runtime.memo('loaded', { ...loaded, completedAt: new Date(runtime.now()).toISOString() }, ctx);
          await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'publish' } }), ctx);
        } catch (error) {
          if (runtime.signal.aborted || ctx.abortSignal?.aborted) throw error;
          if (blockedInput(error)) {
            await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'wait', resume: 'load', wakeAt: runtime.now() + 5000, reason: 'data_source_original_input_unavailable' } }), ctx);
            return;
          }
          if (captured) await operations.failed(task.input.source, captured);
          await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: 'data_source_refresh_failed' } } }), ctx);
        }
      },
      async publish(task, runtime, ctx) {
        try {
          const captured = JSON.parse(await readFile(task.input.privateFile, 'utf8')) as JsonObject;
          if (createHash('sha256').update(canonical(captured)).digest('hex') !== task.input.identity) throw new Error('data_source_input_corrupt');
          const loaded = await runtime.memo<JsonObject>('loaded', ctx);
          if (!loaded) throw new Error('data_source_result_missing');
          let result: JsonObject;
          try { result = await operations.publish(task.input.source, captured, loaded); }
          catch (error) {
            if (error instanceof Problem && error.message === 'data_source_refresh_configuration_changed') result = { state: 'superseded', source: task.input.source, completedAt: loaded.completedAt!, datasetIds: (loaded.datasets as JsonObject[]).map(row => String(row.id)) };
            else throw error;
          }
          await runtime.commit(async tx => {
            const row = (await tx.doc(DataSourceLoads)).records[task.input.key]!;
            row.result = result;
            return { status: 'terminal', outcome: { status: 'completed', result } };
          }, ctx);
        } catch (error) {
          if (runtime.signal.aborted || ctx.abortSignal?.aborted) throw error;
          if (!blockedInput(error) && !(error instanceof Error && error.message === 'data_source_save_failed')) throw error;
          await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'wait', resume: 'publish', wakeAt: runtime.now() + 5000, reason: 'data_source_publication_unavailable' } }), ctx);
        }
      },
      async wait(task, runtime, ctx) {
        await runtime.sleep(task.state.checkpoint.wakeAt ?? runtime.now(), ctx);
        await runtime.commit(() => ({ status: 'running', checkpoint: { phase: task.state.checkpoint.resume ?? 'load' } }), ctx);
      },
    },
    async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });
  const extension = defineExtension({ name: 'qz.data-source.refresh', tasks: [Refresh] });
  function connected() { if (!host) throw new Problem('data_source_host_unavailable', 503); return host; }
  return {
    extension,
    connect(value: ResearchHost) { host = value; },
    async refresh(source: string, requestId: string): Promise<JsonObject> {
      if (!requestId || requestId.length > 256) throw new Problem('data_source_request_id_invalid');
      const key = canonical([source, requestId]); const current = connected();
      // Original identity wins even if owner configuration has since changed.
      const existing = (await current.harness.snapshot(DataSourceLoads, context))?.records[key];
      let captured: Captured | undefined; let privateFile: string | undefined;
      if (!existing) {
        captured = await operations.capture(source);
        const directory = join(dataDir, '.data-source-loads'); await mkdir(directory, { recursive: true });
        privateFile = join(directory, `${captured.identity}.json`);
        // Content-addressed private input is immutable; replay cannot replace
        // credentials/config for a previously admitted operation.
        try {
          const previous = await readFile(privateFile, 'utf8');
          if (canonical(JSON.parse(previous)) !== canonical(captured.privateInput)) throw new Error('data_source_input_conflict');
        } catch (error) {
          if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error;
          await atomicWriteFile(privateFile, JSON.stringify(captured.privateInput), 0o600);
        }
      }
      const taskId = await current.harness.commit(async tx => {
        const records = (await tx.doc(DataSourceLoads)).records;
        if (records[key]) return Number(records[key]!.taskId);
        assertBusinessAdmission(await tx.doc(Application));
        const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
        const taskId = await tx.createTask(Refresh, { key, source, identity: captured!.identity, privateFile: privateFile! }, { conversationId: conversation.id, ownership: { kind: 'conversation' } });
        records[key] = json({ source, requestId, identity: captured!.identity, taskId, result: null });
        return taskId;
      }, context);
      const settled = await current.harness.waitForTask(taskId as TaskId<JsonObject>, context);
      if (settled.state.outcome.status !== 'completed') throw new Problem(settled.state.outcome.status === 'aborted' ? 'data_source_refresh_cancelled' : 'data_source_refresh_failed', settled.state.outcome.status === 'aborted' ? 409 : 400);
      return structuredClone(settled.state.outcome.result);
    },
    async cancel(source: string) {
      if (!host) return;
      const records = (await host.harness.snapshot(DataSourceLoads, context))?.records ?? {};
      for (const value of Object.values(records)) if (value.source === source && value.result === null) await host.harness.abortTask(Number(value.taskId) as TaskId, context);
    },
  };
}
