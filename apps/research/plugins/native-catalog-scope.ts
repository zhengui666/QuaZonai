import { defineDoc, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId, ToolExecutionApi } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { withoutAbortSignal } from '@earendil-works/chord/context';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { mkdtemp, realpath, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { JsonReader } from '../src/native-json.ts';
import { object } from '../src/trials.ts';
import { NativeEvidencePart, retainNativeEvidence } from './native-evidence.ts';
import { classifyExecutionError, executionDiagnostic, ExecutionFailure } from '../src/execution-errors.ts';

export const CatalogScopes = defineDoc<{ roots: JsonObject[] }>({
  kind: 'qz.catalog-scopes', version: 1, scope: 'session', initial: () => ({ roots: [] }),
});

async function readScope(catalog: string, context: Context): Promise<string[]> {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-catalog-scope-'));
  const env = new NodeExecutionEnv({ cwd: directory });
  try {
    const parser = new JsonReader(); let stderr = '';
    const result = getOrThrow(await env.exec([
      'bash', '-c', 'exec "$1" catalog-factor-scope --catalog "$2"',
      'qz-catalog-scope', process.env.QZ_JOB_BINARY ?? 'job', catalog,
    ], { inheritEnv: false, env: { PATH: process.env.PATH ?? '/usr/bin:/bin', TMPDIR: directory },
      onOutput: (text, _context, info) => { if (info.stream === 'stdout') parser.write(text); else stderr += text; },
    }, context));
    if (result.exitCode !== 0) throw new Error(stderr.trim() || `native_catalog_scope_exit_${result.exitCode}`);
    const barTypes = parser.finish();
    if (!Array.isArray(barTypes) || !barTypes.length || !barTypes.every((type) => typeof type === 'string')) throw new Error('native_catalog_scope_invalid');
    return barTypes;
  } finally {
    await env.cleanup(withoutAbortSignal(context));
    await rm(directory, { recursive: true, force: true });
  }
}

export const CatalogScopeTask = defineTask<{ catalog: string }, { phase: 'read' }, JsonObject>({
  name: 'qz.catalog.scope', version: 1, initial: () => ({ phase: 'read' }),
  phases: { async read(task, runtime, context) {
    try {
      const barTypes = await readScope(task.input.catalog, context);
      const native = await retainNativeEvidence({ barTypes }, async (index, data) => {
        await runtime.commit(async (tx) => {
          (await tx.doc(NativeEvidencePart, runtime.conversationId, `${task.id}:${index}`, null)).data = data;
          return undefined;
        }, context);
      });
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: {
        conversationId: runtime.conversationId, taskId: task.id, native, count: String(barTypes.length),
      } } }), context);
    } catch (error) {
      if (runtime.signal.aborted) throw error;
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: {
        message: executionDiagnostic(error, 'catalog_scope_failed'), detail: { ...classifyExecutionError(error, 'scientific_execution_failed', { operation: 'catalog-factor-scope' }) },
      } } }), context);
    }
  } },
  async abort(_task, runtime, context) {
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
  },
});

const Child = defineDoc<{ taskId: TaskId<JsonObject> | null }>({
  kind: 'qz.catalog-scope-child', version: 1, scope: 'task', initial: () => ({ taskId: null }),
});

export async function catalogScope(api: ToolExecutionApi, context: Context, data: JsonObject): Promise<JsonObject> {
  if (typeof data.catalog !== 'string') throw new Error('catalog_required');
  const catalog = await realpath(data.catalog);
  // Reuse only an immutable admitted root's identity set, never its freshness.
  // The kernel also checks this set before computing any outside-scope model.
  const cached = (await api.snapshot(CatalogScopes, context))?.roots.find((root) => root.catalog === catalog);
  if (cached) return { ...data, catalog_scope: cached.reference! };
  const id = await api.commit(async (tx) => {
    const binding = await tx.doc(Child, api.taskId);
    binding.taskId ??= await tx.createTask(CatalogScopeTask, { catalog: data.catalog as string }, { ownership: { kind: 'task', taskId: api.taskId } });
    return binding.taskId;
  }, context);
  const settled = await api.waitForTask(id, context);
  if (settled.state.outcome.status !== 'completed') {
    const outcome = settled.state.outcome;
    const error = 'error' in outcome ? outcome.error : undefined;
    const detail = error?.detail;
    throw new ExecutionFailure(error?.message ?? outcome.status,
      detail && typeof detail === 'object' && !Array.isArray(detail) ? detail : { operation: 'catalog-factor-scope' });
  }
  const result = object(settled.state.outcome.result, 'catalog_scope');
  const reference = { conversationId: result.conversationId!, taskId: result.taskId!, count: result.count! };
  await api.commit(async (tx) => {
    const scopes = await tx.doc(CatalogScopes);
    if (!scopes.roots.some((root) => root.catalog === catalog)) scopes.roots.push({ catalog, reference });
  }, context);
  return { ...data, catalog_scope: reference };
}
