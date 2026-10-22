import type { Context } from '@earendil-works/chord';
import type { TaskRuntime, ToolExecutionApi, Tx } from '@earendil-works/pi-durable';

/** Common business operations, backed by official task/tool transactions. */
export type ResearchRuntime = Pick<ToolExecutionApi,
  'taskId' | 'conversationId' | 'commit' | 'snapshot' | 'snapshotAsOf' | 'getTask' | 'waitForTask'>;

export function researchRuntime<I, S, R, H extends object>(runtime: TaskRuntime<I, S, R, H>): ResearchRuntime {
  return {
    taskId: runtime.taskId, conversationId: runtime.conversationId,
    snapshot: runtime.snapshot.bind(runtime), snapshotAsOf: runtime.snapshotAsOf.bind(runtime),
    getTask: runtime.getTask.bind(runtime), waitForTask: runtime.waitForTask.bind(runtime),
    async commit<T>(change: (tx: Tx) => T | Promise<T>, context: Context): Promise<T> {
      let result!: T;
      await runtime.commit(async (tx) => { result = await change(tx); return undefined; }, context);
      return result;
    },
  };
}
