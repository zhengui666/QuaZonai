import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { TaskRecord } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import type { PortfolioFact } from './facts.ts';

type Task = TaskRecord<unknown, unknown, unknown> | undefined;
/** Technical execution state is a projection of the official task record. */
export function executionStatus(task: Task, completed: string, queued = 'PENDING'): string {
  if (!task) return completed;
  if (task.state.status !== 'terminal') return task.state.status === 'pending' ? queued : 'RUNNING';
  const outcome = task.state.outcome;
  return outcome.status === 'completed' ? completed : outcome.status === 'aborted' ? 'STOPPED' : 'FAILED';
}
export async function portfolioRuntime(host: ResearchHost, fact: PortfolioFact): Promise<PortfolioFact> {
  const task = fact.view.taskId === null ? undefined : await host.harness.getTask(fact.view.taskId as import('@earendil-works/pi-durable').TaskId, BACKGROUND_CONTEXT);
  return { ...fact, view: { ...fact.view, status: executionStatus(task, fact.view.status === 'REVIEW_BLOCKED' ? 'REVIEW_BLOCKED' : 'COMPUTED') } };
}
