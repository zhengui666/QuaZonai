import { retainNativeRemoteUncertainty, withNativeRemoteTracking } from '../src/native-execution.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { nativeTargetForData, executeNativeValue } from '../src/native-execution.ts';
import type { ResearchRuntime } from '../src/research-runtime.ts';
import { defineDoc, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId, TaskRuntime } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { mkdtemp, realpath, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { object } from '../src/trials.ts';
import { NativeEvidencePart, readNativeEvidence, retainNativeEvidence } from './native-evidence.ts';
import { classifyExecutionError, executionDiagnostic, ExecutionFailure } from '../src/execution-errors.ts';
import { canonical } from '../src/documents.ts';

export const CatalogScopes = defineDoc<{ roots: JsonObject[] }>({
  kind: 'qz.catalog-scopes', version: 1, scope: 'session', initial: () => ({ roots: [] }),
});

async function readScope(catalog: string, context: Context, binary: NativeTarget): Promise<string[]> {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-catalog-scope-'));
  try {
    const barTypes = await executeNativeValue(binary, 'catalog-factor-scope', {}, ['--catalog', catalog], directory, context);
    if (!Array.isArray(barTypes) || !barTypes.length || !barTypes.every((type) => typeof type === 'string')) throw new Error('native_catalog_scope_invalid');
    return barTypes;
  } finally {
    await rm(directory, { recursive: true, force: true });
  }
}

export const CatalogScopeTask = defineTask<{ catalog: string; binary?: NativeTarget }, { phase: 'read' }, JsonObject>({
  name: 'qz.catalog.scope', version: 1, initial: () => ({ phase: 'read' }),
  phases: { async read(task, runtime, context) {
    return withNativeRemoteTracking(task.id, runtime, context, async () => {
    try {
      const barTypes = await readScope(task.input.catalog, context, task.input.binary ?? nativeTargetForData({}));
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
      await retainNativeRemoteUncertainty(error, task.id, runtime, context);
      if (runtime.signal.aborted) throw error;
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: {
        message: executionDiagnostic(error, 'catalog_scope_failed'), detail: { ...classifyExecutionError(error, 'scientific_execution_failed', { operation: 'catalog-factor-scope' }) },
      } } }), context);
    }
    });
  } },
  async abort(_task, runtime, context) {
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
  },
});

const Child = defineDoc<{ taskId: TaskId<JsonObject> | null }>({
  kind: 'qz.catalog-scope-child', version: 1, scope: 'task', initial: () => ({ taskId: null }),
});

export async function readCatalogScope(runtime: Pick<TaskRuntime<unknown, unknown, unknown, object>, 'getTask' | 'snapshot' | 'snapshotAsOf'>, reference: JsonObject, context: Context): Promise<JsonObject> {
  const original = await runtime.getTask(reference.taskId as TaskId<JsonObject>, context);
  if (original?.kind !== 'qz.catalog.scope' || original.state.status !== 'terminal' || original.state.outcome.status !== 'completed') throw new Error('native_catalog_scope_missing');
  return readNativeEvidence(runtime, original.conversationId, original.id, object(original.state.outcome.result.native, 'scope_native'), context);
}

export async function catalogScope(api: ResearchRuntime, context: Context, data: JsonObject): Promise<JsonObject> {
  if (typeof data.catalog !== 'string') throw new Error('catalog_required');
  const catalog = await realpath(data.catalog);
  const binary = nativeTargetForData(data);
  const sameBinding = (root: JsonObject) => typeof binary === 'string' ? root.nautilusTarget === undefined
    : root.nautilusTarget !== undefined && canonical(root.nautilusTarget) === canonical(binary);
  // Reuse only an immutable admitted root's identity set, never its freshness.
  // The kernel also checks this set before computing any outside-scope model.
  const cached = (await api.snapshot(CatalogScopes, context))?.roots.find((root) => root.catalog === catalog && sameBinding(root));
  if (cached) return { ...data, catalog_scope: cached.reference! };
  const id = await api.commit(async (tx) => {
    const binding = await tx.doc(Child, api.taskId);
    binding.taskId ??= await tx.createTask(CatalogScopeTask, { catalog: data.catalog as string, binary }, { ownership: { kind: 'task', taskId: api.taskId } });
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
    if (!scopes.roots.some((root) => root.catalog === catalog && sameBinding(root))) scopes.roots.push({ catalog, reference, ...(typeof binary !== 'string' ? { nautilusTarget: binary } : {}) });
  }, context);
  return { ...data, catalog_scope: reference };
}
