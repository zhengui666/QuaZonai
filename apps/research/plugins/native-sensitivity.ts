import { arr, map, obj, opt, str, unknown } from '@earendil-works/pi-schema';
import { defineDoc, defineTool, type JsonObject, type TaskId } from '@earendil-works/pi-durable';
import { registeredNativeStrategyRequest } from '../src/research-configuration.ts';
import { canonical, Study } from '../src/documents.ts';
import { object, studyInput } from '../src/trials.ts';
import { nativeTargetForData } from '../src/native-execution.ts';
import { NativeStrategyBacktestTask, nativeStrategyResult } from './native-strategy-backtest.ts';
import { assertNativeRemoteTaskSettled, reconcileNativeRemoteTasks } from '../src/native-remote-receipts.ts';

export type SensitivityRole = 'BASELINE' | 'PARAMETER_NEIGHBOUR' | 'COST';
export const NativeSensitivity = defineDoc<{ runs: { toolTaskId: number; sourceRequest: JsonObject; neighbourStrategies: JsonObject[]; costs: JsonObject[]; tasks: { role: SensitivityRole; taskId: TaskId<JsonObject> }[] }[] }>({
  kind: 'qz.native-sensitivity', version: 1, scope: 'conversation', history: 'latest', fork: 'initial',
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, initial: () => ({ runs: [] }),
});
export const nativeSensitivityTool = defineTool({
  name: 'native_strategy_sensitivity',
  description: 'Actually run a baseline plus explicitly chosen parameter neighbours and/or maker/taker fee assumptions with the registered official Nautilus strategy, account settings and data. Each case is a real independent native backtest. Only strategy parameters or the specified two fees change. This uses the registered historical window; it is sensitivity evidence, not a new independent OOS test or a live order. Declare the same neighbourStrategies/costs in the pre-execution logic review validationMethod when requesting a two-lane rating. Preserve failed attempts; read their original Task IDs rather than claiming missing results succeeded.',
  parameters: obj({ neighbourStrategies: opt(arr(map(unknown()))), costs: opt(arr(obj({ maker_fee: str(), taker_fee: str() }))) }), replay: 'safe', executionMode: 'sequential',
  async execute(args, api, context) {
    const input = await studyInput(api, context); const source = registeredNativeStrategyRequest(input);
    if (typeof input.data.catalog !== 'string' || !input.data.catalog) throw new Error('catalog_required');
    const neighbourStrategies = (args.neighbourStrategies ?? []) as JsonObject[]; const costs = (args.costs ?? []) as JsonObject[];
    if (!neighbourStrategies.length && !costs.length) throw new Error('native_sensitivity_cases_required');
    const prior = await api.snapshot(NativeSensitivity, api.conversationId, context);
    await reconcileNativeRemoteTasks(api, (prior?.runs ?? []).flatMap(run => run.tasks.map(({ taskId: id }) => ({ id, conversationId: api.conversationId }))), context);
    const run = await api.commit(async tx => {
      const doc = await tx.doc(NativeSensitivity, api.conversationId);
      const currentInput = (await tx.doc(Study, api.conversationId)).input;
      if (!currentInput || canonical(registeredNativeStrategyRequest(currentInput)) !== canonical(source)) throw new Error('native_sensitivity_configuration_changed');
      const original = doc.runs.find(run => run.toolTaskId === api.taskId); if (original) return JSON.parse(JSON.stringify(original)) as typeof original;
      for (const previous of doc.runs) for (const { taskId } of previous.tasks) {
        const task = await tx.task(taskId);
        if (task && task.state.status !== 'terminal') throw new Error('native_sensitivity_execution_unsettled');
        if (task) await assertNativeRemoteTaskSettled(tx, api.conversationId, taskId);
      }
      const requests: { role: SensitivityRole; request: JsonObject }[] = [ { role: 'BASELINE', request: source },
        ...neighbourStrategies.map(strategy => ({ role: 'PARAMETER_NEIGHBOUR' as const, request: { ...source, strategy } })),
        ...costs.map(cost => ({ role: 'COST' as const, request: { ...source, venue: { ...object(source.venue, 'native_strategy_venue'), maker_fee: cost.maker_fee!, taker_fee: cost.taker_fee! } } })),
      ];
      const tasks: { role: SensitivityRole; taskId: TaskId<JsonObject> }[] = [];
      for (const { role, request } of requests) tasks.push({ role, taskId: await tx.createTask(NativeStrategyBacktestTask, { request, sourceRequest: source, catalog: input.data.catalog as string, target: nativeTargetForData(input.data) }, { ownership: { kind: 'task', taskId: api.taskId } }) });
      const run = { toolTaskId: api.taskId as number, sourceRequest: source, neighbourStrategies, costs, tasks }; doc.runs.push(run); return run;
    }, context);
    const results = await Promise.all(run.tasks.map(async ({ role, taskId }) => {
      const task = await api.waitForTask(taskId, context);
      if (task.state.outcome.status !== 'completed') return { role, taskId, status: task.state.outcome.status, error: task.state.outcome.status === 'failed' ? task.state.outcome.error.message : null, result: null };
      return { role, taskId, status: 'completed', error: null, result: nativeStrategyResult(task.state.outcome.result, object(object(task.input, 'sensitivity_input').request, 'sensitivity_request')) };
    }));
    return { output: [{ type: 'text', text: JSON.stringify({ sourceRequest: run.sourceRequest, scope: 'FULL_REGISTERED_WINDOW_SENSITIVITY_NOT_NEW_OOS', toolTaskId: run.toolTaskId, results }) }] };
  },
});
