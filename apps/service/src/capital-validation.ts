import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { Context } from '@earendil-works/chord';
import { defineDoc, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonicalJsonDigest } from '../../research/src/native-json.ts';
import { cancelNativeRemoteTasks, hasNativeStepRecovery, retainNativeRemoteUncertainty, withNativeRemoteTracking, withNativeStep } from '../../research/src/native-execution.ts';
import { validateCapitalDocument } from './capital-exit.ts';
import type { ServiceConfig } from './config.ts';
import { scienceTarget } from './config.ts';
import { Application, json, Problem } from './facts.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';

type Input = { key: string; kind: string; value: JsonObject[string]; target: NativeTarget; dataDir: string };
type Checkpoint = { phase: 'validate' } | { phase: 'wait'; wakeAt: number };

/** Pure native validation receipts share the original official task; reading a
 * capital view repeatedly cannot create a new invocation for the same bytes. */
export const CapitalValidationRequests = defineDoc<{ records: { [key: string]: JsonObject } }>({
  kind: 'qz.capital-validations', version: 1, scope: 'session', initial: () => ({ records: {} }),
});

export const CapitalDocumentValidation = defineTask<Input, Checkpoint, null>({
  name: 'qz.capital-document-validation', version: 1, initial: () => ({ phase: 'validate' }),
  phases: {
    async validate(task, runtime, ctx) {
      try {
        await withNativeRemoteTracking(task.id, runtime, ctx, () => withNativeStep('capital-contract', () =>
          validateCapitalDocument({ jobBinary: task.input.target, dataDir: task.input.dataDir }, task.input.kind, task.input.value, ctx)));
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), ctx);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, task.id, runtime, ctx);
        if (runtime.signal.aborted || ctx.abortSignal?.aborted) throw error;
        // Protected calls retain their original invocation during transport
        // uncertainty. A confirmed native rejection remains a failed task.
        if (await hasNativeStepRecovery(task.id, runtime, ctx)
          && error instanceof Error && /nautilus_(execution_unknown|cancellation_unknown)/.test(error.message)) {
          await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'wait', wakeAt: runtime.now() + 1000 } }), ctx);
          return;
        }
        const code = error instanceof Error ? error.message : 'native_capital_exit_contract_invalid';
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: code } } }), ctx);
      }
    },
    async wait(task, runtime, ctx) {
      await runtime.sleep(task.state.checkpoint.wakeAt ?? runtime.now(), ctx);
      await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'validate' } }), ctx);
    },
  },
  async abort(_task, runtime, ctx) {
    const protectedExecution = await hasNativeStepRecovery(runtime.taskId, runtime, ctx);
    let settled = await cancelNativeRemoteTasks(runtime.taskId, runtime, ctx);
    while (protectedExecution && !settled) {
      await runtime.sleep(runtime.now() + 1000, ctx);
      settled = await cancelNativeRemoteTasks(runtime.taskId, runtime, ctx);
    }
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx);
  },
});

export async function validateCapitalDocumentDurable(host: ResearchHost, config: ServiceConfig, kind: string, value: unknown, ctx: Context = BACKGROUND_CONTEXT): Promise<void> {
  const target = scienceTarget(config);
  // A credential reference is a host-private lookup, not a new scientific
  // purpose. Retain the first complete target in the original task input.
  const targetIdentity = typeof target === 'string' ? target : { ...target, credentialRef: null };
  const original = json({ kind, value, target: targetIdentity });
  const key = canonicalJsonDigest(original);
  const taskId = await host.harness.commit(async tx => {
    const records = (await tx.doc(CapitalValidationRequests)).records;
    const prior = records[key]; if (prior) return Number(prior.taskId);
    assertBusinessAdmission(await tx.doc(Application));
    const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
    const taskId = await tx.createTask(CapitalDocumentValidation, { key, kind, value: original.value!, target, dataDir: config.dataDir },
      { conversationId: conversation.id, ownership: { kind: 'conversation' } });
    records[key] = { original, taskId, conversationId: conversation.id };
    return taskId;
  }, ctx);
  const settled = await host.harness.waitForTask(taskId as TaskId<null>, ctx);
  const outcome = settled.state.outcome;
  if (outcome.status === 'completed') return;
  if (outcome.status === 'aborted') throw new Problem('capital_validation_cancelled', 503);
  const code = outcome.error?.message ?? 'native_capital_exit_contract_invalid';
  // The task retains the original native diagnostic. HTTP consumers receive a
  // public code instead of exposing arbitrary remote stderr as a Problem.
  if (code === 'native_capital_exit_contract_invalid') throw new Problem(code, 422);
  throw new Problem('capital_validation_failed', 503);
}
