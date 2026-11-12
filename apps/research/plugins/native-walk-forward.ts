import { registeredNativeStrategyRequest } from '../src/research-configuration.ts';
import { arr, map, obj, opt, unknown } from '@earendil-works/pi-schema';
import { defineDoc, defineExtension, defineTool, type JsonObject, type TaskId } from '@earendil-works/pi-durable';
import { nativeTargetForData } from '../src/native-execution.ts';
import { NativeStrategyBacktestTask } from './native-strategy-backtest.ts';
import { nativeWalkForwardResult } from '../src/native-walk-forward-result.ts';
import { object, studyInput } from '../src/trials.ts';
import { assertNativeRemoteTaskSettled, reconcileNativeRemoteTasks } from '../src/native-remote-receipts.ts';

export const NativeWalkForward = defineDoc<{ taskIds: TaskId<JsonObject>[] }>({
  kind: 'qz.native-walk-forward', version: 1, scope: 'conversation', history: 'latest', fork: 'initial',
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, initial: () => ({ taskIds: [] }),
});
const WalkForwardChild = defineDoc<{ taskId: TaskId<JsonObject> | null }>({
  kind: 'qz.native-walk-forward-child', version: 1, scope: 'task',
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, initial: () => ({ taskId: null }),
});
export const nativeWalkForwardTool = defineTool({
  name: 'native_strategy_walk_forward',
  description: 'Run actual chronological train-selection/test evaluation using the registered upstream Nautilus strategy and data. Optional strategies contains upstream strategy objects only; all candidates retain the same registered venue, instrument, data and event/availability window. split is solow TimeSeriesSplit {n_splits,test_size?,gap,max_train_size?}; counts are unique availability-time batches, not invented independent observations. objective {native_key,currency?,direction:MAXIMIZE|MINIMIZE} refers to an unambiguous official training statistic. Every candidate training run and chosen test run starts a fresh official account/strategy/indicators inside its own window. No prior-window warmup. Never select using test results. This is real walk-forward evidence, not comprehensive overfitting protection, CPCV, parameter-neighbourhood or cost validation. Run logic review before exposing these results when asked for two-lane assessment; then use the assessment report. No live orders or trading gate.',
  parameters: obj({ strategies: opt(arr(map(unknown()))), split: map(unknown()), objective: map(unknown()) }), replay: 'safe', executionMode: 'sequential',
  async execute(args, api, context) {
    const input = await studyInput(api, context);
    const source = registeredNativeStrategyRequest(input);
    if (typeof input.data.catalog !== 'string' || !input.data.catalog) throw new Error('catalog_required');
    const parameters = args as { strategies?: JsonObject[]; split: JsonObject; objective: JsonObject };
    const strategies = parameters.strategies ?? [object(source.strategy, 'native_strategy_config')];
    if (!strategies.length) throw new Error('native_walk_forward_candidates_required');
    const request: JsonObject = { schema_version: 1, candidates: strategies.map(strategy => ({ ...source, strategy })), split: parameters.split, objective: parameters.objective };
    const previous = await api.snapshot(NativeWalkForward, api.conversationId, context);
    await reconcileNativeRemoteTasks(api, (previous?.taskIds ?? []).map(id => ({ id, conversationId: api.conversationId })), context);
    const id = await api.commit(async tx => {
      const child = await tx.doc(WalkForwardChild, api.taskId); if (child.taskId) return child.taskId;
      const doc = await tx.doc(NativeWalkForward, api.conversationId);
      for (const previousId of doc.taskIds) {
        const previous = await tx.task(previousId);
        if (previous && previous.state.status !== 'terminal') throw new Error('native_walk_forward_execution_unsettled');
        if (previous) await assertNativeRemoteTaskSettled(tx, api.conversationId, previousId);
      }
      child.taskId = await tx.createTask(NativeStrategyBacktestTask, { request, sourceRequest: source, walkForward: true, catalog: input.data.catalog as string, target: nativeTargetForData(input.data) }, { ownership: { kind: 'task', taskId: api.taskId } });
      doc.taskIds.push(child.taskId); return child.taskId;
    }, context);
    const task = await api.waitForTask(id, context);
    if (task.state.outcome.status !== 'completed') throw new Error('native_walk_forward_not_completed');
    const result = nativeWalkForwardResult(task.state.outcome.result, request);
    return { output: [{ type: 'text', text: JSON.stringify({ taskId: id, ...result }) }] };
  },
});

export default defineExtension({ name: 'qz.native-walk-forward', tools: [nativeWalkForwardTool] });
