import { defineDoc, type JsonObject, type TaskId, type Tx, type ConversationId } from '@earendil-works/pi-durable';
import { assertNativeRemoteTaskSettled } from './native-remote-receipts.ts';

/** Conversation input facts only; lifecycle/replay remain official Pi Tasks. */
export const ResearchConfiguration = defineDoc<{ request: JsonObject | null; validationPrefix: boolean; evaluationTaskId: TaskId<JsonObject> | null; previousTaskIds?: TaskId<JsonObject>[]; configuredByTaskId?: number }>({
  kind: 'qz.indicator-strategy-configuration', version: 1, scope: 'conversation', history: 'latest', fork: 'initial',
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127,
  initial: () => ({ request: null, validationPrefix: false, evaluationTaskId: null }),
});
/** Native Task/receipt facts are authoritative, including failed calls with unresolved remote receipts. */
export async function assertResearchRunSettled(tx: Tx, conversationId: ConversationId): Promise<void> {
  const configuration = await tx.doc(ResearchConfiguration, conversationId);
  if (!configuration.evaluationTaskId) return;
  const task = await tx.task(configuration.evaluationTaskId);
  if (!task || task.state.status !== 'terminal') throw Object.assign(new Error('native_strategy_execution_unsettled'), { statusCode: 409 });
  await assertNativeRemoteTaskSettled(tx, conversationId, configuration.evaluationTaskId);
}
