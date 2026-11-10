import type { Context } from '@earendil-works/chord';
import { withoutAbortSignal } from '@earendil-works/chord/context';
import { defineExtension, defineTask, type JsonObject, type TaskId } from '@earendil-works/pi-durable';
import { NodeExecutionEnv } from '@earendil-works/pi-durable/env/node';
import { getOrThrow } from '@earendil-works/pi-durable/env';
import { createHash } from 'node:crypto';
import { mkdir, readFile, stat, writeFile } from 'node:fs/promises';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { canonical } from '../../research/src/documents.ts';

type SourceKind = 'http' | 'hf' | 'hub' | 'snapshot';
export type SourceAcquisitionInput = {
  kind: SourceKind; frozenPlan: JsonObject; workdir: string; output: string;
  cacheDir?: string; termsFile?: string;
};
/** Files are frozen before admission; tasks hold references, not repeated plan panels. */
export type PreparedSourceAcquisition = {
  kind: SourceKind; frozenPlanPath: string; frozenPlanDigest: string;
  workdir: string; output: string; toolPath: string; cacheDir?: string;
  termsFile?: string; termsDigest?: string;
};
export type SourceAcquisitionResult = { manifestPath: string; planId: string };
type Descriptor = { schemaVersion: 1; kind: SourceKind; planId: string; concurrency: number; minIntervalMs: number; units: { id: string; index: number }[] };
type Progress = { planId: string; unitCount: number; concurrency: number; minIntervalMs: number; nextIndex: number };
type Checkpoint = { phase: 'prepare' } | ({ phase: 'schedule' } & Progress) | ({ phase: 'publish' } & Progress)
  | ({ phase: 'joined'; children: number[] } & Progress);
type UnitInput = PreparedSourceAcquisition & { planId: string; index: number };
type UnitResult = { planId: string; index: number; receiptPath: string };
const digest = (bytes: string | Buffer) => createHash('sha256').update(bytes).digest('hex');

async function frozenFile(path: string, bytes: string | Buffer): Promise<void> {
  try { await writeFile(path, bytes, { flag: 'wx', mode: 0o600 }); }
  catch (error) { if ((error as NodeJS.ErrnoException).code !== 'EEXIST') throw error; }
  if (!(await readFile(path)).equals(Buffer.from(bytes))) throw new Error('source_frozen_artifact_conflict');
}

export async function prepareSourceAcquisition(input: SourceAcquisitionInput): Promise<PreparedSourceAcquisition> {
  if (!['http', 'hf', 'hub', 'snapshot'].includes(input.kind)) throw new Error('source_kind_invalid');
  if (input.kind === 'http' && !input.termsFile) throw new Error('source_terms_required');
  if (input.kind !== 'http' && !input.cacheDir) throw new Error('source_cache_required');
  const workdir = resolve(input.workdir); await mkdir(workdir, { recursive: true });
  const bytes = canonical(input.frozenPlan);
  const frozenPlanDigest = digest(bytes);
  const frozenPlanPath = join(workdir, `plan-${frozenPlanDigest}.json`);
  await frozenFile(frozenPlanPath, bytes);
  const prepared: PreparedSourceAcquisition = {
    kind: input.kind, frozenPlanPath, frozenPlanDigest, workdir, output: resolve(input.output),
    toolPath: resolve(process.env.QZ_SOURCE_UNIT_TOOL ?? fileURLToPath(new URL('../../../runtimes/data/source_units.py', import.meta.url))),
    ...(input.cacheDir ? { cacheDir: resolve(input.cacheDir) } : {}),
  };
  if (input.termsFile) {
    const terms = await readFile(resolve(input.termsFile));
    prepared.termsDigest = digest(terms);
    prepared.termsFile = join(workdir, `terms-${prepared.termsDigest}.bin`);
    await frozenFile(prepared.termsFile, terms);
  }
  return prepared;
}

async function verifyFrozen(input: PreparedSourceAcquisition, terms = false): Promise<void> {
  if (digest(await readFile(input.frozenPlanPath)) !== input.frozenPlanDigest) throw new Error('source_frozen_plan_changed');
  if (terms && input.termsFile && digest(await readFile(input.termsFile)) !== input.termsDigest) throw new Error('source_frozen_terms_changed');
}

async function sourceTool(input: PreparedSourceAcquisition, command: 'plan' | 'execute' | 'publish', context: Context, index?: number): Promise<unknown> {
  try {
    await verifyFrozen(input, command === 'publish');
    const args = ['python3', input.toolPath, command, '--kind', input.kind, '--plan', input.frozenPlanPath];
    if (command !== 'plan') args.push('--workdir', input.workdir);
    if (command === 'execute') args.push('--unit-index', String(index));
    if (input.cacheDir && command !== 'plan') args.push('--cache-dir', input.cacheDir);
    if (command === 'publish') {
      args.push('--output', input.output);
      if (input.termsFile) args.push('--terms-file', input.termsFile);
    }
    const environment = new NodeExecutionEnv({ cwd: input.workdir });
    const output: string[] = [];
    try {
      const result = getOrThrow(await environment.exec(args, {
        onOutput(chunk, _context, info) { if (info.stream === 'stdout' && command === 'plan') output.push(chunk); },
      }, context));
      if (result.exitCode !== 0) throw new Error('source_process_failed');
      // Unit and publication payloads remain files. Only the descriptor is parsed.
      return command === 'plan' ? JSON.parse(output.join('')) : undefined;
    } finally { await environment.cleanup(withoutAbortSignal(context)); }
  } catch (error) {
    if (context.abortSignal?.aborted) throw error;
    // Provider exceptions may contain signed URLs, credentials or local paths.
    // Durable task diagnostics retain only the public stage code.
    throw new Error(`source_${command}_failed`);
  }
}

function descriptor(value: unknown, kind: SourceKind): Descriptor {
  const result = value as Descriptor;
  if (!result || result.schemaVersion !== 1 || result.kind !== kind || !/^[a-f0-9]{64}$/.test(result.planId)
    || result.concurrency !== (kind === 'http' ? 1 : 4) || result.minIntervalMs !== (kind === 'http' ? 350 : 0)
    || !Array.isArray(result.units) || !result.units.length
    || result.units.some((unit, index) => unit.index !== index || unit.id !== `${result.planId}-${index}`)) throw new Error('source_descriptor_invalid');
  return result;
}

export const SourceAcquisitionUnitTask = defineTask<UnitInput, { phase: 'execute' }, UnitResult>({
  name: 'qz.source-acquisition.unit', version: 1, initial: () => ({ phase: 'execute' }),
  phases: { async execute(task, runtime, context) {
    try {
      await sourceTool(task.input, 'execute', context, task.input.index);
      const receiptPath = join(task.input.workdir, 'units', String(task.input.index).padStart(8, '0'), 'receipt.json');
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: {
        planId: task.input.planId, index: task.input.index, receiptPath,
      } } }), context);
    } catch (error) {
      if (runtime.signal.aborted) throw error;
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: {
        message: error instanceof Error ? error.message : 'source_unit_failed',
      } } }), context);
    }
  } },
  abort: (_task, runtime, context) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context),
});

export const SourceAcquisitionTask = defineTask<PreparedSourceAcquisition, Checkpoint, SourceAcquisitionResult>({
  name: 'qz.source-acquisition', version: 1, initial: () => ({ phase: 'prepare' }),
  phases: {
    async prepare(task, runtime, context) {
      const plan = descriptor(await sourceTool(task.input, 'plan', context), task.input.kind);
      await runtime.commit(() => ({ status: 'running', checkpoint: {
        phase: 'schedule', planId: plan.planId, unitCount: plan.units.length,
        concurrency: plan.concurrency, minIntervalMs: plan.minIntervalMs, nextIndex: 0,
      } }), context);
    },
    async schedule(task, runtime, context) {
      const checkpoint = task.state.checkpoint;
      if (checkpoint.nextIndex >= checkpoint.unitCount) {
        await runtime.commit(() => ({ status: 'running', checkpoint: { ...checkpoint, phase: 'publish' } }), context);
        return;
      }
      // The descriptor chooses a source-specific batch; official ownership,
      // waiting, join policy and timers provide all durable scheduling state.
      await runtime.commit(async tx => {
        const end = Math.min(checkpoint.unitCount, checkpoint.nextIndex + checkpoint.concurrency);
        const children: TaskId<UnitResult>[] = [];
        for (let index = checkpoint.nextIndex; index < end; index++) {
          children.push(await tx.createTask(SourceAcquisitionUnitTask, { ...task.input, planId: checkpoint.planId, index },
            { ownership: { kind: 'task', taskId: task.id } }));
        }
        return { status: 'waiting', on: children, policy: 'allSettled', checkpoint: {
          ...checkpoint, phase: 'joined', nextIndex: end, children: children as number[],
        } };
      }, context);
    },
    async joined(task, runtime, context) {
      const checkpoint = task.state.checkpoint;
      const outcomes = await runtime.outcomes(checkpoint.children as TaskId<UnitResult>[], context);
      const failed = outcomes.find(outcome => outcome.status !== 'completed');
      if (failed) {
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: {
          message: failed.status === 'failed' || failed.status === 'faulted' ? failed.error.message : 'source_unit_aborted',
        } } }), context);
      } else if (checkpoint.nextIndex === checkpoint.unitCount) {
        await runtime.commit(() => ({ status: 'running', checkpoint: { ...checkpoint, phase: 'publish' } }), context);
      } else if (checkpoint.minIntervalMs > 0) {
        await runtime.sleepUntil(runtime.now() + checkpoint.minIntervalMs, { ...checkpoint, phase: 'schedule' }, context);
      } else {
        await runtime.commit(() => ({ status: 'running', checkpoint: { ...checkpoint, phase: 'schedule' } }), context);
      }
    },
    async publish(task, runtime, context) {
      const checkpoint = task.state.checkpoint;
      await sourceTool(task.input, 'publish', context);
      const manifestPath = join(task.input.output, {
        http: 'acquisition.json', hf: 'selection.json', hub: 'market-data.json', snapshot: 'snapshot.json',
      }[task.input.kind]);
      if (!(await stat(manifestPath)).isFile()) throw new Error('source_publication_missing');
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: {
        manifestPath, planId: checkpoint.planId,
      } } }), context);
    },
  },
  abort: (_task, runtime, context) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context),
});

export const sourceAcquisitionExtension = defineExtension({ name: 'qz.source-acquisition', tasks: [SourceAcquisitionTask, SourceAcquisitionUnitTask] });
