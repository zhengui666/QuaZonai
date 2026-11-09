import { createHash } from 'node:crypto';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc, defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { CapacityReport, ResearchSpecification } from '../../../packages/contracts/src/index.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical } from '../../research/src/documents.ts';
import { cancelNativeRemoteTasks, withNativeRemoteTracking } from '../../research/src/native-execution.ts';
import { capacityPlan, computeCapacityPlan, unavailableCapacity } from './capacity.ts';
import type { CapacityPlan } from './capacity.ts';
import type { ServiceConfig } from './config.ts';
import { scienceTarget } from './config.ts';
import { json, Problem } from './facts.ts';
import type { PortfolioFact } from './facts.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';
import { Application } from './facts.ts';

const context = BACKGROUND_CONTEXT;
type Assumptions = NonNullable<ResearchSpecification['capital']>;
type Kind = 'FACTOR' | 'PORTFOLIO';
type Request = { requestId: string; assumptions?: Assumptions };
type Record = { kind: Kind; id: string; requestId: string; original: JsonObject; taskId: number; createdAt: string; result: CapacityReport | null };
export const CapacityRequests = defineDoc<{ records: { [key: string]: JsonObject } }>({
  kind: 'qz.capacity-requests', version: 1, scope: 'session', initial: () => ({ records: {} }),
});
const record = (value: JsonObject): Record => value as unknown as Record;
const keyOf = (kind: Kind, id: string, requestId: string) => canonical([kind, id, requestId]);

/** Legacy callers have no request ID. Exact repeated bodies deliberately address
 * the same calculation; an explicit new requestId is required to calculate anew. */
export function capacityRequest(kind: Kind, id: string, assumptions: Assumptions | undefined, requestId?: string): Request {
  if (requestId !== undefined && (typeof requestId !== 'string' || !requestId.trim() || requestId.length > 256)) throw new Problem('capacity_request_id_invalid');
  return { requestId: requestId ?? `capacity:${createHash('sha256').update(canonical(json({ kind, id, assumptions: assumptions ?? null }))).digest('hex')}`, assumptions };
}

export function createCapacity(config: ServiceConfig) {
  let host: ResearchHost | undefined;
  const CapacityTask = defineTask<{ key: string; plan: CapacityPlan; target: NativeTarget; dataDir: string }, { phase: 'compute' }, JsonObject>({
    name: 'qz.capacity', version: 1, initial: () => ({ phase: 'compute' }),
    phases: { async compute(task, runtime, ctx) {
      try {
        const result = await withNativeRemoteTracking(task.id, runtime, ctx, () => computeCapacityPlan(task.input.plan, task.input.target, task.input.dataDir, ctx));
        await runtime.commit(async tx => {
          record((await tx.doc(CapacityRequests)).records[task.input.key]!).result = result;
          return { status: 'terminal', outcome: { status: 'completed', result: json(result) } };
        }, ctx);
      } catch (error) {
        if (runtime.signal.aborted || ctx.abortSignal?.aborted) throw error;
        // Diagnostics remain in the original official Task; HTTP applies its
        // existing public-error policy, never converting a failure into success.
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: error instanceof Error ? error.message : 'capacity_failed' } } }), ctx);
      }
    } },
    async abort(_task, runtime, ctx) {
      while (!await cancelNativeRemoteTasks(runtime.taskId, runtime, ctx)) await runtime.sleep(runtime.now() + 1000, ctx);
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
  });
  const extension = defineExtension({ name: 'qz.capacity', tasks: [CapacityTask] });
  function connected() { if (!host) throw new Problem('capacity_host_unavailable', 503); return host; }
  async function start(kind: Kind, id: string, input: Request, portfolio?: PortfolioFact): Promise<CapacityReport> {
    const current = connected(); const key = keyOf(kind, id, input.requestId);
    const original = json({ kind, id, assumptions: input.assumptions ?? null });
    const existing = (await current.harness.snapshot(CapacityRequests, context))?.records[key];
    if (existing && canonical(record(existing).original) !== canonical(original)) throw new Problem('request_id_conflict', 409);
    // Freeze all scientific source facts before admission; executing a queued
    // request never rereads a later factor or portfolio revision.
    const plan = existing ? undefined : await capacityPlan(current, kind, id, input.assumptions, portfolio);
    const taskId = await current.harness.commit(async tx => {
      const facts = await tx.doc(CapacityRequests); const previous = facts.records[key];
      if (previous) {
        if (canonical(record(previous).original) !== canonical(original)) throw new Problem('request_id_conflict', 409);
        return record(previous).taskId;
      }
      assertBusinessAdmission(await tx.doc(Application));
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const taskId = await tx.createTask(CapacityTask, { key, plan: json(plan) as unknown as CapacityPlan, target: scienceTarget(config), dataDir: config.dataDir }, { conversationId: conversation.id, ownership: { kind: 'conversation' } });
      facts.records[key] = json({ kind, id, requestId: input.requestId, original, taskId, createdAt: new Date().toISOString(), result: null });
      return taskId;
    }, context);
    const settled = await current.harness.waitForTask(taskId as TaskId<JsonObject>, context);
    if (settled.state.outcome.status !== 'completed') throw new Error(settled.state.outcome.status === 'failed' ? settled.state.outcome.error?.message ?? 'capacity_failed' : 'capacity_cancelled');
    return structuredClone(settled.state.outcome.result) as unknown as CapacityReport;
  }
  return { extension, connect(value: ResearchHost) { host = value; },
    startFactor: (id: string, input: Request) => start('FACTOR', id, input),
    startPortfolio: (portfolio: PortfolioFact, input: Request) => start('PORTFOLIO', portfolio.view.id, input, portfolio),
    async read(kind: Kind, id: string, requestId?: string): Promise<CapacityReport> {
      const values = Object.values((await connected().harness.snapshot(CapacityRequests, context))?.records ?? {}).map(record)
        .filter(row => row.kind === kind && row.id === id && (!requestId || row.requestId === requestId));
      const result = values.reverse().find(row => row.result !== null)?.result;
      return result ? structuredClone(result) : unavailableCapacity('', values.length ? 'capacity_pending' : 'capacity_not_assessed', id);
    },
  };
}
