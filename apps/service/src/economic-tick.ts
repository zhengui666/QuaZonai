import type { FastifyInstance } from 'fastify';
import { randomUUID } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc, defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import { API } from '../../../packages/contracts/src/index.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { validateNativeTickEconomicRequest, validateNativeTickEconomicResult, type NativeTickEconomicRequest, type NativeTickEconomicResult, type TickEconomicReplayStart, type TickEconomicReplayView } from '../../../packages/contracts/src/economic-tick.ts';
import { nativeResearchInput } from '../../../packages/contracts/src/tick-input.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical, Requests, Study } from '../../research/src/documents.ts';
import { strategyMembers } from '../../research/src/strategy.ts';
import { compileWasm, iterationRequest } from '../../research/plugins/native-wasm.ts';
import { replayTickEconomic } from '../../research/plugins/tick-economic.ts';
import { NativeRemoteUncertainty, remoteNativeExecutionReceipt, retainNativeRemoteUncertainty, validateRemoteTarget, withNativeRemoteTracking } from '../../research/src/native-execution.ts';
import { executionDiagnostic } from '../../research/src/execution-errors.ts';
import type { ServiceConfig } from './config.ts';
import { boundScienceTarget } from './science-binding.ts';
import { factorReference } from './factors.ts';
import { Application, json, object, Problem, text } from './facts.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';

const context = BACKGROUND_CONTEXT;
// No research tool reads this owner-only document or receives its daily outcomes.
export const TickEconomicReplays = defineDoc<{ runs: Record<string, JsonObject> }>({
  kind: 'qz.tick-economic-replays', version: 1, scope: 'session', initial: () => ({ runs: {} }),
});
type ReplayFact = { original: TickEconomicReplayStart; view: TickEconomicReplayView };
type ReplayInput = { id: string; dataDir: string; catalog: string; target: NativeTarget; model: string; request: NativeTickEconomicRequest };
function fact(value: JsonObject): ReplayFact { return value as unknown as ReplayFact; }
function binding(data: JsonObject) { return data.sealed ? object(data.sealed, 'sealed_data') : data; }

export const TickEconomicReplay = defineTask<ReplayInput, { phase: 'compute' }, null>({
  name: 'qz.tick-economic-replay', version: 1, initial: () => ({ phase: 'compute' }),
  phases: { async compute(task, runtime, ctx) {
    let directory: string | undefined;
    try {
      await runtime.commit(async tx => {
        fact((await tx.doc(TickEconomicReplays)).runs[task.input.id]!).view.status = 'RUNNING';
        return undefined;
      }, ctx);
      const references = (await runtime.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), ctx))?.requests ?? [];
      let result: NativeTickEconomicResult;
      if (references.length) {
        // A restart may recover an original receipt, never POST a replacement run.
        if (references.length !== 1 || references[0]!.taskId !== task.id || canonical(references[0]!.target!) !== canonical(task.input.target)) throw new Error('tick_economic_remote_reference_invalid');
        const original = references[0]!;
        if (original.status === 'REJECTED') throw new Error(text(original.error, 'native_admission_error'));
        const receipt = await remoteNativeExecutionReceipt(validateRemoteTarget(original.target), text(original.requestId, 'native_request_id'), ctx.abortSignal ?? runtime.signal);
        if (receipt.status !== 'COMPLETED') throw new Error('tick_economic_remote_execution_unsettled');
        result = validateNativeTickEconomicResult(receipt.result);
        if (canonical(result.request) !== canonical(task.input.request)) throw new Error('tick_economic_result_request_mismatch');
      } else {
        directory = await mkdtemp(join(task.input.dataDir, 'tick-economic-'));
        const model = join(directory, 'model.wasm');
        await writeFile(model, compileWasm(task.input.model, 'retained-factor.wat', 'replay-tick-economic'));
        result = await withNativeRemoteTracking(task.id, runtime, ctx, () => replayTickEconomic(task.input.target,
          task.input.request, { catalog: task.input.catalog, model }, directory!, ctx));
      }
      await runtime.commit(async tx => {
        const stored = fact((await tx.doc(TickEconomicReplays)).runs[task.input.id]!);
        stored.view.status = 'COMPUTED'; stored.view.result = result; stored.view.error = null;
        const native = await tx.doc(NativeRemoteUncertainty, task.conversationId, String(task.id), null);
        for (const reference of native.requests) { reference.status = 'COMPLETED'; reference.error = null; }
        return { status: 'terminal', outcome: { status: 'completed', result: null } };
      }, ctx);
    } catch (error) {
      await retainNativeRemoteUncertainty(error, task.id, runtime, ctx);
      if (runtime.signal.aborted) throw error;
      await runtime.commit(async tx => {
        const stored = fact((await tx.doc(TickEconomicReplays)).runs[task.input.id]!);
        stored.view.status = 'FAILED'; stored.view.error = executionDiagnostic(error, 'tick_economic_replay_failed');
        return { status: 'terminal', outcome: { status: 'failed', error: { message: stored.view.error } } };
      }, ctx);
    } finally { if (directory) await rm(directory, { recursive: true, force: true }); }
  } },
  async abort(task, runtime, ctx) {
    await runtime.commit(async tx => {
      fact((await tx.doc(TickEconomicReplays)).runs[task.input.id]!).view.status = 'STOPPED';
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, ctx);
  },
});
export const tickEconomicExtension = defineExtension({ name: 'qz.tick-economic-replay', tasks: [TickEconomicReplay] });

export function tickEconomicWorkbench(host: ResearchHost, config: ServiceConfig) {
  async function get(id: string) {
    const stored = (await host.harness.snapshot(TickEconomicReplays, context))?.runs[id];
    if (!stored) throw new Problem('tick_economic_replay_not_found', 404);
    return fact(stored).view;
  }
  async function start(value: unknown) {
    const body = object(value);
    if (Object.keys(body).some(key => !['requestId', 'datasetId', 'factorId', 'request'].includes(key))) throw new Problem('tick_economic_unknown_field');
    const input: TickEconomicReplayStart = { requestId: text(body.requestId, 'request_id'), datasetId: text(body.datasetId, 'dataset_id'),
      factorId: text(body.factorId, 'factor_id'), request: validateNativeTickEconomicRequest(body.request) };
    const reference = factorReference(input.factorId);
    return host.harness.commit(async tx => {
      const replays = await tx.doc(TickEconomicReplays);
      const existing = Object.values(replays.runs).map(fact).find(row => row.original.requestId === input.requestId);
      if (existing) {
        if (canonical(existing.original) !== canonical(input)) throw new Problem('request_id_conflict', 409);
        return json(existing.view) as unknown as TickEconomicReplayView;
      }
    const dataset = config.datasets.find(row => row.id === input.datasetId);
    if (!dataset) throw new Problem('dataset_not_found', 404);
    if (!dataset.ready || !dataset.preparation?.nativePrepared) throw new Problem('dataset_not_ready', 409);
      const application = await tx.doc(Application); assertBusinessAdmission(application);
      const study = await tx.doc(Study, reference.researchId as ConversationId);
      if (!study.input || study.deletedAt || study.deletedFactors?.[input.factorId]) throw new Problem('factor_not_found', 404);
      const requests = await tx.doc(Requests);
      const admitted = Object.entries(requests.requests).some(([key, row]) => row.conversationId === reference.researchId
        && object(application.researchRequests[key]?.dataset ?? {}).id === input.datasetId);
      if (!admitted) throw new Problem('tick_economic_factor_dataset_mismatch', 409);
      const trial = study.trials.find(row => row.taskId === reference.trialId);
      if (!trial || trial.status !== 'COMPUTED' || !trial.result) throw new Problem('tick_economic_computed_factor_required', 422);
      const members = strategyMembers(object(trial.proposal));
      if (members.length !== 1 || reference.memberIndex !== 0) throw new Problem('tick_economic_single_factor_required', 422);
      const source = nativeResearchInput(binding(study.input.data));
      const configured = nativeResearchInput(binding(dataset.data));
      if (source?.modality !== 'TRADE_TICK' || configured?.modality !== 'TRADE_TICK' || !source.request.input.representation
        || canonical(source.request) !== canonical(input.request.research) || canonical(configured.request) !== canonical(source.request)) throw new Problem('tick_economic_original_request_mismatch', 409);
      const native = object(trial.result);
      const coverage = object(native.coverage, 'native_coverage');
      if (canonical(native.request!) !== canonical(iterationRequest(study.input.data)) || coverage.modality !== 'TRADE_TICK'
        || coverage.source_revision !== source.request.input.source_revision || typeof coverage.trade_count !== 'string' || !/^[1-9]\d*$/.test(coverage.trade_count)
        || typeof object(native.native_versions, 'native_versions')['nautilus-model'] !== 'string') throw new Problem('tick_economic_native_evidence_mismatch', 409);
      const id = randomUUID();
      const taskId = await tx.createTask(TickEconomicReplay, { id, dataDir: config.dataDir, catalog: text(binding(dataset.data).catalog, 'catalog'),
        model: members[0]!.model, request: input.request, target: boundScienceTarget(config, study.input.data) },
      { conversationId: reference.researchId as ConversationId, ownership: { kind: 'conversation' } });
      const view: TickEconomicReplayView = { id, requestId: input.requestId, datasetId: input.datasetId, factorId: input.factorId, taskId,
        status: 'QUEUED', scientificQualification: 'UNQUALIFIED_EXPLORATORY', result: null, error: null };
      replays.runs[id] = json({ original: input, view });
      return view;
    }, context);
  }
  return { start, get };
}

/** Register after the existing owner authentication and maintenance hooks. */
export function registerTickEconomicRoutes(app: FastifyInstance, work: ReturnType<typeof tickEconomicWorkbench>) {
  app.post(`${API}/tick-economic-replays`, async (request, reply) => reply.code(202).send(await work.start(request.body)));
  app.get<{ Params: { id: string } }>(`${API}/tick-economic-replays/:id`, request => work.get(request.params.id));
}
