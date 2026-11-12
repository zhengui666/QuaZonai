import { selectedSettings as selectedSimulationSettings } from '../src/strategy.ts';
import { assertNativeRemoteTaskSettled, reconcileNativeRemoteTasks } from '../src/native-remote-receipts.ts';
import { indicatorStrategyResult } from '../src/indicator-result.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { int, map, obj, opt, str, unknown } from '@earendil-works/pi-schema';
import { defineDoc, defineExtension, defineTask, defineTool, type JsonObject, type TaskId, type Tx, type ConversationId } from '@earendil-works/pi-durable';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { executeNativeValue, nativeTargetForData, withNativeRemoteTracking, cancelUnsettledNativeExecutions, retainNativeRemoteUncertainty, nativeUncertainTaskId, reconcileNativeExecution } from '../src/native-execution.ts';
import type { NativeScanCheckpoint, NativeScanContext } from '../src/native-execution.ts';
import { object, studyInput } from '../src/trials.ts';
import { canonical, Study, type ResearchInput } from '../src/documents.ts';
import { nativeResearchRequest, isTickResearchInput } from '../../../packages/contracts/src/tick-input.ts';
import { compareDecimal } from '../../../packages/contracts/src/pm-validation.ts';
import { checkExposure } from './native-exposure.ts';

/** Conversation input facts only; lifecycle/replay remain official Pi Tasks. */
export const IndicatorConfiguration = defineDoc<{ request: JsonObject | null; validationPrefix: boolean; evaluationTaskId: TaskId<JsonObject> | null; previousTaskIds?: TaskId<JsonObject>[]; configuredByTaskId?: number }>({
  kind: 'qz.indicator-strategy-configuration', version: 1, scope: 'conversation', history: 'latest', fork: 'initial',
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127,
  initial: () => ({ request: null, validationPrefix: false, evaluationTaskId: null }),
});
/** Native Task/receipt facts are authoritative, including failed calls with unresolved remote receipts. */
export async function assertIndicatorRunSettled(tx: Tx, conversationId: ConversationId): Promise<void> {
  const configuration = await tx.doc(IndicatorConfiguration, conversationId);
  if (!configuration.evaluationTaskId) return;
  const task = await tx.task(configuration.evaluationTaskId);
  if (!task || task.state.status !== 'terminal') throw Object.assign(new Error('indicator_strategy_execution_unsettled'), { statusCode: 409 });
  await assertNativeRemoteTaskSettled(tx, conversationId, configuration.evaluationTaskId);
}
type IndicatorParameters = { signal: JsonObject; decision_start_ns: string; long_weight: string; target_ttl_seconds: number; observations?: JsonObject; bar_type?: string };

/** Preserve the registered input; derive only the selected scope and its original fee declarations. */
export function configuredIndicatorRequest(input: ResearchInput, parameters: IndicatorParameters) {
  if (input.campaign || isTickResearchInput(input.data) || input.data.catalog_scan_request !== undefined) throw new Error('indicator_strategy_bar_dataset_required');
  if (typeof input.data.catalog !== 'string' || !input.data.catalog) throw new Error('catalog_required');
  const batchTemplate = input.data.indicator_strategy_batch_request === undefined ? null : object(input.data.indicator_strategy_batch_request, 'indicator_strategy_batch_request');
  if (batchTemplate && input.data.indicator_strategy_request !== undefined) throw new Error('indicator_strategy_binding_ambiguous');
  let template = input.data.indicator_strategy_request === undefined ? null : object(input.data.indicator_strategy_request, 'indicator_strategy_request');
  if (batchTemplate) {
    if (!Array.isArray(batchTemplate.requests) || !batchTemplate.requests.length) throw new Error('indicator_strategy_batch_requests_required');
    template = object(batchTemplate.requests[0], 'indicator_strategy_batch_request');
    for (const value of batchTemplate.requests) {
      const request = object(value, 'indicator_strategy_batch_request');
      if (canonical(request.selection ?? null) !== canonical(template.selection ?? null) || canonical(request.settings ?? null) !== canonical(template.settings ?? null)) throw new Error('indicator_strategy_batch_scope_mismatch');
    }
  }
  if (template && input.data.sealed) throw new Error('indicator_strategy_template_must_be_historical');
  let selection: JsonObject; let settings: JsonObject;
  if (template) {
    selection = structuredClone(object(template.selection, 'indicator_strategy_selection'));
    settings = structuredClone(object(template.settings, 'indicator_strategy_settings'));
  } else {
    const registered = nativeResearchRequest(input.data);
    selection = structuredClone(object(object(registered.forecast, 'indicator_strategy_bar_forecast').selection, 'indicator_strategy_selection'));
    const end = registered.validation_end_ns;
    if (typeof end !== 'string' || !/^(0|[1-9][0-9]*)$/.test(end)) throw new Error('indicator_strategy_validation_end_required');
    if (typeof selection.event_end_ns !== 'string' || BigInt(end) > BigInt(selection.event_end_ns)) throw new Error('indicator_strategy_selection_invalid');
    selection.event_end_ns = end;
    if (typeof selection.decision_cutoff_ns !== 'string') throw new Error('indicator_strategy_cutoff_required');
    selection.decision_cutoff_ns = BigInt(selection.decision_cutoff_ns) < BigInt(end) ? selection.decision_cutoff_ns : end;
    settings = structuredClone(object(object(input.policy.strategy, 'indicator_strategy_registered_settings_required').simulationSettings, 'indicator_strategy_settings'));
  }
  const registeredSeriesCount = Array.isArray(selection.bar_types) ? selection.bar_types.length : 0;
  if (parameters.bar_type !== undefined) {
    if (!Array.isArray(selection.bar_types) || !selection.bar_types.includes(parameters.bar_type)) throw new Error('indicator_strategy_bar_type_outside_registered_scope');
    selection.bar_types = [parameters.bar_type];
  }
  if (!Array.isArray(selection.bar_types) || selection.bar_types.length !== 1) throw new Error('indicator_strategy_single_series_required');
  // Native explicit replay requires exactly one original fee declaration per selected instrument.
  // Preserve the registered input; filter only this run's settings using the existing adapter.
  if (registeredSeriesCount > 1 || settings.fee_rates !== undefined) {
    const barType = selection.bar_types[0];
    if (typeof barType !== 'string') throw new Error('indicator_strategy_bar_type_invalid');
    const instrumentId = barType.split('-').slice(0, -4).join('-');
    if (!instrumentId) throw new Error('indicator_strategy_bar_type_invalid');
    settings = selectedSimulationSettings(settings, [instrumentId]);
  }
  const counter = (value: unknown) => {
    if (typeof value !== 'string' || !/^(0|[1-9][0-9]*)$/.test(value) || BigInt(value) > 9223372036854775807n) throw new Error('indicator_strategy_clock_invalid');
    return BigInt(value);
  };
  const start = counter(selection.event_start_ns), end = counter(selection.event_end_ns), cutoff = counter(selection.decision_cutoff_ns), decision = counter(parameters.decision_start_ns);
  // Native historical templates may receive BARs after their event-time window.
  // Iterative datasets still cannot decide at/after the sealed prefix boundary.
  if (start >= end || decision < start || decision > cutoff || !template && decision >= end) throw new Error('indicator_strategy_decision_outside_registered_scope');
  if (compareDecimal(parameters.long_weight, '0') < 0 || compareDecimal(parameters.long_weight, '1') > 0) throw new Error('indicator_strategy_long_weight_invalid');
  if (!Number.isSafeInteger(parameters.target_ttl_seconds) || parameters.target_ttl_seconds < 1 || parameters.target_ttl_seconds > 4294967295) throw new Error('indicator_strategy_ttl_invalid');
  const request: JsonObject = { schema_version: 1, selection, settings, signal: parameters.signal,
    decision_start_ns: parameters.decision_start_ns, long_weight: parameters.long_weight, target_ttl_seconds: parameters.target_ttl_seconds,
    ...(parameters.observations === undefined ? {} : { observations: parameters.observations }) };
  return { request, validationPrefix: !template };
}

export const configureIndicatorStrategyTool = defineTool({
  name: 'native_indicator_strategy_configure',
  description: 'Prepare a historical native indicator strategy in this same conversation, then call native_indicator_strategy_evaluate. Reuse the registered catalog and native settings; derive a selected BAR and its original instrument fees without modifying the registered input. A conventional BAR dataset uses only its TRAIN/VALIDATION prefix, never sealed data. A historical batch supplies its common registered selection/settings; explicit configuration creates a new single run without modifying the original batch. Optional bar_type selects one member of the registered BAR universe and retains only that instrument’s original fee declaration for this run. Supply the native signal object (currently EMA_CROSS, RSI or MACD), decision_start_ns, exact decimal long_weight and target_ttl_seconds. Optional observations uses native ATR/Stochastics/Donchian periods. Parameters are stored as original JSON; the native engine validates its own schema on execution. No code generation, new dataset, live order or approval. Missing registered settings require configuration, not invented defaults.',
  parameters: obj({ bar_type: opt(str()), signal: map(unknown()), decision_start_ns: str(), long_weight: str(), target_ttl_seconds: int({ minimum: 1, maximum: 4294967295 }), observations: opt(map(unknown())) }),
  replay: 'safe', executionMode: 'sequential',
  async execute(args, api, context) {
    const input = await studyInput(api, context);
    const configured = configuredIndicatorRequest(input, args as IndicatorParameters);
    const previous = await api.snapshot(IndicatorConfiguration, api.conversationId, context);
    if (previous?.evaluationTaskId) await reconcileNativeRemoteTasks(api, [{ id: previous.evaluationTaskId, conversationId: api.conversationId }], context);
    await api.commit(async tx => {
      const study = await tx.doc(Study, api.conversationId);
      if (study.final || study.trials.length) throw new Error('indicator_strategy_requires_uncomputed_research');
      const stored = await tx.doc(IndicatorConfiguration, api.conversationId);
      if (stored.configuredByTaskId === api.taskId) return;
      if (stored.evaluationTaskId) {
        await assertIndicatorRunSettled(tx, api.conversationId);
        stored.previousTaskIds = [...(stored.previousTaskIds ?? []), stored.evaluationTaskId];
        stored.evaluationTaskId = null;
      }
      stored.configuredByTaskId = api.taskId;
      stored.request = configured.request; stored.validationPrefix = configured.validationPrefix;
    }, context);
    return { output: [{ type: 'text', text: JSON.stringify({ status: 'CONFIGURED_NOT_EXECUTED', ...configured, nativeValidation: 'ON_EXECUTION' }) }] };
  },
});

/** Execute the owner's explicit binding, not an inferred conversion from a Wasm study. */
export const indicatorStrategyTool = defineTool({
  name: 'native_indicator_strategy_evaluate',
  description: 'Complete one historical backtest or an explicit same-selection/settings data.indicator_strategy_batch_request (one Pi task/receipt, fresh strategies over native reset, all-or-nothing completion). Backtest of the configured request in this conversation or explicit data.indicator_strategy_request bound to this study, using its registered catalog and Nautilus instance. Requires one BAR series, native EMA_CROSS/RSI/MACD parameters, simulation settings, long_weight, decision_start_ns and target_ttl_seconds. Optional observations returns native ATR, cumulative-since-selection VWAP (not daily/session VWAP), Stochastics and Donchian values with null during their own warmup. Performance covers the whole selection including pre-decision cash periods, not just the trading period. Same-availability BARs update indicators as one batch; only the last BAR can decide, and a batch with unsettled native orders is skipped (counts returned), never queued using future prices. No Wasm, covariance/optimizer, automatic approval, paper account or live orders. Cannot convert an already computed legacy Wasm study or choose an unbound period/data range.',
  parameters: obj({}), replay: 'safe', executionMode: 'sequential',
  async execute(_args, api, context) {
    const input = await studyInput(api, context);
    const configured = await api.snapshot(IndicatorConfiguration, api.conversationId, context);
    const hasBatchTemplate = input.data.indicator_strategy_batch_request !== undefined;
    if (hasBatchTemplate && input.data.indicator_strategy_request !== undefined) throw new Error('indicator_strategy_binding_ambiguous');
    const batch = hasBatchTemplate && !configured?.request;
    const request = object(batch ? input.data.indicator_strategy_batch_request : configured?.request ?? input.data.indicator_strategy_request, 'indicator_strategy_request');
    if (typeof input.data.catalog !== 'string') throw new Error('catalog_required');
    // An indicator study is explicitly historical, not a back door into sealed evaluation.
    if (input.campaign || input.data.sealed && !configured?.validationPrefix) throw new Error('indicator_strategy_requires_explicit_historical_binding');
    if (configured?.validationPrefix) await checkExposure(api, context, input.data, false);
    const id = await api.commit(async tx => {
      const study = await tx.doc(Study, api.conversationId);
      if (study.final && study.final.taskId !== api.taskId) throw new Error('research_already_final');
      const binding = await tx.doc(IndicatorChild, api.taskId);
      if (binding.taskId) return binding.taskId;
      const configuration = await tx.doc(IndicatorConfiguration, api.conversationId);
      if (configuration.evaluationTaskId) { binding.taskId = configuration.evaluationTaskId; return binding.taskId; }
      if (configuration.request && canonical(configuration.request) !== canonical(request)) throw new Error('indicator_strategy_configuration_changed');
      binding.taskId = await tx.createTask(IndicatorStrategyTask, {
        request, ...(batch ? { batch: true } : {}), catalog: input.data.catalog as string, target: nativeTargetForData(input.data),
      }, { ownership: { kind: 'task', taskId: api.taskId } });
      configuration.evaluationTaskId = binding.taskId;
      return binding.taskId;
    }, context);
    const settled = await api.waitForTask(id, context);
    if (settled.state.outcome.status !== 'completed') {
      const outcome = settled.state.outcome;
      throw new Error('error' in outcome && outcome.error ? outcome.error.message : 'indicator_strategy_evaluation_aborted');
    }
    const result = validatedIndicatorResult(settled.state.outcome.result, request, batch);
    await api.commit(async tx => {
      const study = await tx.doc(Study, api.conversationId);
      if (study.final && study.final.taskId !== api.taskId) throw new Error('research_already_final');
      study.final = { taskId: api.taskId, trialId: null, comparedTrials: [], status: 'COMPUTED',
        reason: batch ? 'explicit_native_indicator_strategy_batch_completed' : 'explicit_native_indicator_strategy_completed',
        scientificClaim: 'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION', result };
    }, context);
    return { output: [{ type: 'text', text: JSON.stringify(result) }], control: { terminate: true } };
  },
});

const IndicatorChild = defineDoc<{ taskId: TaskId<JsonObject> | null }>({
  kind: 'qz.indicator-strategy-child', checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 127, version: 1, scope: 'task', initial: () => ({ taskId: null }),
});

function validatedIndicatorResult(value: Parameters<typeof indicatorStrategyResult>[0], request: JsonObject, batch: boolean): JsonObject {
  if (!batch) return indicatorStrategyResult(value, request);
  const result = object(value, 'indicator_strategy_batch_result');
  const requests = request.requests;
  if (result.schema_version !== 1 || !Array.isArray(requests) || requests.length === 0
    || !Array.isArray(result.results) || result.results.length !== requests.length
    || result.reuse !== 'NAUTILUS_ENGINE_RESET' || typeof result.native_version !== 'string') throw new Error('indicator_strategy_batch_result_mismatch');
  for (let index = 0; index < requests.length; index++) {
    const row = indicatorStrategyResult(result.results[index]!, object(requests[index], 'indicator_strategy_batch_request'));
    if (object(row.simulation, 'indicator_strategy_simulation').native_version !== result.native_version) throw new Error('indicator_strategy_batch_result_mismatch');
  }
  return result;
}

export const IndicatorStrategyTask = defineTask<{ request: JsonObject; catalog: string; target: NativeTarget; batch?: boolean; scanContext?: NativeScanContext }, NativeScanCheckpoint, JsonObject>({
  name: 'qz.indicator-strategy.evaluate', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, context) {
    const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? process.env.QZ_SCRATCH_DIR ?? '/var/tmp', 'qz-indicators-'));
    try {
      const raw = await withNativeRemoteTracking(task.id, runtime, context, () => executeNativeValue(
        task.input.target, task.input.batch ? 'simulate-indicator-strategy-batch' : 'simulate-indicator-strategy', task.input.request,
        ['--catalog', task.input.catalog], directory, context), task.input.scanContext);
      const result = validatedIndicatorResult(raw, task.input.request, task.input.batch === true);
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result } }), context);
    } catch (error) {
      await retainNativeRemoteUncertainty(error, task.id, runtime, context);
      if (runtime.signal.aborted) throw error;
      const nativeTaskId = nativeUncertainTaskId(error);
      if (nativeTaskId !== null) {
        await runtime.commit(() => ({ status: 'sleeping', until: runtime.now() + 1000, checkpoint: { phase: 'reconcile', nativeTaskId } }), context);
        return;
      }
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: error instanceof Error ? error.message : 'indicator_strategy_evaluation_failed' } } }), context);
    } finally { await rm(directory, { recursive: true, force: true }); }
  }, async reconcile(task, runtime, context) {
    if (task.state.checkpoint.phase !== 'reconcile') throw new Error('native_reconciliation_checkpoint_invalid');
    const settled = await reconcileNativeExecution(task.id, runtime, task.state.checkpoint.nativeTaskId, context);
    await runtime.commit(() => settled ? { status: 'running', checkpoint: { phase: 'compute' } }
      : { status: 'sleeping', until: runtime.now() + 1000, checkpoint: task.state.checkpoint }, context);
  } },
  async abort(task, runtime, context) {
    await cancelUnsettledNativeExecutions(task.id, runtime, context);
    await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context);
  },
});

export default defineExtension({ name: 'qz.indicator-strategy', tasks: [IndicatorStrategyTask], tools: [configureIndicatorStrategyTool, indicatorStrategyTool] });
