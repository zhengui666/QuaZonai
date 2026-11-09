import type { FastifyInstance } from 'fastify';
import { randomUUID } from 'node:crypto';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc, defineExtension, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId, TaskRecord } from '@earendil-works/pi-durable';
import { API } from '../../../packages/contracts/src/index.ts';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { validateNativeTickEconomicRequest, validateNativeTickEconomicResult, type NativeTickEconomicRequest, type NativeTickEconomicResult, type TickEconomicReplayDetail, type TickEconomicReplayMember, type TickEconomicReplayMetadata, type TickEconomicReplayProgress, type TickEconomicReplayStart, type TickEconomicReplaySummary, type TickEconomicReplayView } from '../../../packages/contracts/src/economic-tick.ts';
import { nativeResearchInput } from '../../../packages/contracts/src/tick-input.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical, Requests, Study } from '../../research/src/documents.ts';
import { exactWeights, strategyMembers } from '../../research/src/strategy.ts';
import { compileWasm, iterationRequest } from '../../research/plugins/native-wasm.ts';
import { replayTickEconomic } from '../../research/plugins/tick-economic.ts';
import { NativeRemoteUncertainty, waitForRemoteNativeExecutionReceipt, retainNativeRemoteUncertainty, validateRemoteTarget, withNativeRemoteTracking } from '../../research/src/native-execution.ts';
import { executionDiagnostic } from '../../research/src/execution-errors.ts';
import { assertNativeRemoteTaskSettled } from '../../research/src/native-remote-receipts.ts';
import type { ServiceConfig } from './config.ts';
import { boundScienceTarget, withoutExecutionQuotas } from './science-binding.ts';
import { factorReference } from './factors.ts';
import { Application, json, object, Problem, settings, text } from './facts.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';

const context = BACKGROUND_CONTEXT;
// No research tool reads this owner-only document or receives its daily outcomes.
export const TickEconomicReplays = defineDoc<{ runs: Record<string, JsonObject> }>({
  kind: 'qz.tick-economic-replays', version: 1, scope: 'session', initial: () => ({ runs: {} }),
});
type ReplayFact = { original: TickEconomicReplayStart; view: TickEconomicReplayView };
type ReplayInput = { id: string; createdAt?: string; dataDir: string; catalog: string; fundingCatalog?: string; target: NativeTarget; model: string | string[]; request: NativeTickEconomicRequest; bookCores?: number };
type ReplayCheckpoint = { phase: 'compute'; progress?: TickEconomicReplayProgress };
function fact(value: JsonObject): ReplayFact { return value as unknown as ReplayFact; }
function matchingBinding(sourceData: JsonObject, configuredData: JsonObject, request: NativeTickEconomicRequest['research']) {
  const source = nativeResearchInput(sourceData); const configured = nativeResearchInput(configuredData);
  if (source?.modality !== 'TRADE_TICK' || configured?.modality !== 'TRADE_TICK' || !source.request.input.representation
    || canonical(withoutExecutionQuotas(source.request)) !== canonical(withoutExecutionQuotas(request)) || canonical(withoutExecutionQuotas(configured.request)) !== canonical(withoutExecutionQuotas(source.request))) return null;
  return { request: source.request, catalog: text(sourceData.catalog, 'catalog'), fundingCatalog: text(configuredData.catalog, 'catalog') };
}

export const TickEconomicReplay = defineTask<ReplayInput, ReplayCheckpoint, null>({
  name: 'qz.tick-economic-replay', version: 1, initial: input => ({ phase: 'compute',
    ...(input.createdAt ? { progress: { phase: 'QUEUED', updatedAt: input.createdAt } } : {}) }),
  phases: { async compute(task, runtime, ctx) {
    let directory: string | undefined;
    try {
      const report = async (phase: string) => runtime.commit(async tx => {
        fact((await tx.doc(TickEconomicReplays)).runs[task.input.id]!).view.status = 'RUNNING';
        return { status: 'running', checkpoint: { phase: 'compute', progress: { phase, updatedAt: new Date(runtime.now()).toISOString() } } };
      }, ctx);
      const references = (await runtime.snapshot(NativeRemoteUncertainty, task.conversationId, String(task.id), ctx))?.requests ?? [];
      let result: NativeTickEconomicResult;
      if (references.length) {
        await report('RECONCILING');
        // A restart may recover an original receipt, never POST a replacement run.
        if (references.length !== 1 || references[0]!.taskId !== task.id || canonical(references[0]!.target!) !== canonical(task.input.target)) throw new Error('tick_economic_remote_reference_invalid');
        const original = references[0]!;
        if (original.status === 'REJECTED') throw new Error(text(original.error, 'native_admission_error'));
        const receipt = await waitForRemoteNativeExecutionReceipt(validateRemoteTarget(original.target), text(original.requestId, 'native_request_id'), ctx.abortSignal ?? runtime.signal);
        if (['COMPLETED', 'FAILED', 'CANCELLED'].includes(receipt.status)) await runtime.commit(async tx => {
          const native = await tx.doc(NativeRemoteUncertainty, task.conversationId, String(task.id), null);
          const reference = native.requests.find(row => row.requestId === original.requestId && row.taskId === task.id)!;
          reference.status = receipt.status; reference.error = receipt.status === 'FAILED' ? 'nautilus_execution_failed' : null;
          return undefined;
        }, ctx);
        if (receipt.status === 'FAILED') throw new Error(receipt.error);
        if (receipt.status === 'CANCELLED') throw new Error('nautilus_execution_cancelled');
        if (receipt.status !== 'COMPLETED') throw new Error('tick_economic_remote_execution_unsettled');
        result = validateNativeTickEconomicResult(receipt.result);
        if (canonical(withoutExecutionQuotas(result.request)) !== canonical(withoutExecutionQuotas(task.input.request))) throw new Error('tick_economic_result_request_mismatch');
      } else {
        await report('PREPARING_MODELS');
        directory = await mkdtemp(join(task.input.dataDir, 'tick-economic-'));
        const models = Array.isArray(task.input.model) ? task.input.model : [task.input.model];
        const model = await Promise.all(models.map(async (source, index) => {
          const path = join(directory!, `model-${index}.wasm`);
          await writeFile(path, compileWasm(source, 'retained-factor.wat', 'replay-tick-economic'));
          return path;
        }));
        await report('REPLAYING');
        result = await withNativeRemoteTracking(task.id, runtime, ctx, () => replayTickEconomic(task.input.target,
          task.input.request, { catalog: task.input.catalog, fundingCatalog: task.input.fundingCatalog, model, bookCores: task.input.bookCores ?? 1 }, directory!, ctx));
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
      const view = fact((await tx.doc(TickEconomicReplays)).runs[task.input.id]!).view;
      view.status = 'STOPPED'; view.error = null;
      try { await assertNativeRemoteTaskSettled(tx, task.conversationId, task.id); }
      catch (error) {
        if (!(error instanceof Error) || error.message !== 'native_remote_execution_unsettled') throw error;
        view.error = 'tick_economic_local_stop_remote_receipt_unsettled';
      }
      return { status: 'terminal', outcome: { status: 'aborted' } };
    }, ctx);
  },
});
export const tickEconomicExtension = defineExtension({ name: 'qz.tick-economic-replay', tasks: [TickEconomicReplay] });

function timestamp(value: unknown): string | null {
  return typeof value === 'string' && Number.isFinite(Date.parse(value)) ? value : null;
}
export function replayMetadata(stored: ReplayFact, task: TaskRecord<unknown, unknown, unknown> | undefined): TickEconomicReplayMetadata {
  const input = task?.input && typeof task.input === 'object' && !Array.isArray(task.input) ? task.input as Record<string, unknown> : null;
  const linked = task?.kind === TickEconomicReplay.definition.name && input?.id === stored.view.id;
  const state = linked ? task.state : null;
  const checkpoint = state && 'checkpoint' in state && state.checkpoint && typeof state.checkpoint === 'object' ? state.checkpoint as Record<string, unknown> : null;
  const candidate = checkpoint?.progress && typeof checkpoint.progress === 'object' && !Array.isArray(checkpoint.progress) ? checkpoint.progress as Record<string, unknown> : null;
  let progress: TickEconomicReplayProgress | null = null;
  if (candidate && typeof candidate.phase === 'string' && candidate.phase && timestamp(candidate.updatedAt)) {
    progress = { phase: candidate.phase, updatedAt: candidate.updatedAt as string };
    if (Number.isSafeInteger(candidate.completed) && Number.isSafeInteger(candidate.total)
      && (candidate.total as number) > 0 && (candidate.completed as number) >= 0 && (candidate.completed as number) <= (candidate.total as number)) {
      progress.completed = candidate.completed as number; progress.total = candidate.total as number;
    }
  }
  const scenario = stored.original?.request?.scenario;
  const executionModel = stored.view.result?.model ?? (!scenario ? null : scenario.execution_model === undefined ? 'SEGREGATED_COHORT_MARK_ESTIMATE_V1'
    : scenario.execution_model === 'SYNTHETIC_QUOTE_IOC_V1' ? 'SEGREGATED_SYNTHETIC_QUOTE_IOC_V1'
      : scenario.execution_model === 'OBSERVED_L2_IOC_V1' ? 'SEGREGATED_OBSERVED_L2_IOC_V1'
        : scenario.execution_model === 'OBSERVED_L2_IOC_V2' ? 'SEGREGATED_OBSERVED_L2_IOC_V2' : null);
  return { createdAt: linked ? timestamp(input?.createdAt) : null, executionModel, progress };
}

export function tickEconomicWorkbench(host: ResearchHost, config: ServiceConfig) {
  async function get(id: string): Promise<TickEconomicReplayDetail> {
    const runs = (await host.harness.snapshot(TickEconomicReplays, context))?.runs;
    if (!runs || !Object.hasOwn(runs, id)) throw new Problem('tick_economic_replay_not_found', 404);
    const stored = runs[id]!;
    const replay = fact(stored); const task = await host.harness.getTask(replay.view.taskId as TaskId, context);
    return { ...replay.view, ...replayMetadata(replay, task) };
  }
  async function list(): Promise<TickEconomicReplaySummary[]> {
    const stored = (await host.harness.snapshot(TickEconomicReplays, context))?.runs ?? {};
    return Promise.all(Object.values(stored).map(async value => {
      const replay = fact(value); const { result: _result, ...summary } = replay.view;
      const task = await host.harness.getTask(replay.view.taskId as TaskId, context);
      return { ...summary, ...replayMetadata(replay, task) };
    }));
  }
  async function start(value: unknown) {
    const body = object(value);
    if (Object.keys(body).some(key => !['requestId', 'datasetId', 'factorId', 'members', 'fundingCatalog', 'request'].includes(key))) throw new Problem('tick_economic_unknown_field');
    let members: TickEconomicReplayMember[];
    if (body.members === undefined) members = [{ factorId: text(body.factorId, 'factor_id'), weight: '1' }];
    else {
      if (body.factorId !== undefined || !Array.isArray(body.members) || body.members.length < 2) throw new Problem('tick_economic_members_invalid');
      members = body.members.map(value => {
        const member = object(value);
        if (Object.keys(member).some(key => !['factorId', 'weight'].includes(key))) throw new Problem('tick_economic_members_invalid');
        return { factorId: text(member.factorId, 'factor_id'), weight: text(member.weight, 'member_weight') };
      });
      if (new Set(members.map(member => member.factorId)).size !== members.length) throw new Problem('tick_economic_duplicate_factor');
      exactWeights(members.map(member => member.weight));
    }
    const request = validateNativeTickEconomicRequest(body.request);
    const weighted = validateNativeTickEconomicRequest({ ...request, signal_weights: members.map(member => member.weight) });
    if (request.signal_weights && canonical(request.signal_weights) !== canonical(weighted.signal_weights!)) throw new Problem('tick_economic_signal_weights_mismatch');
    members = members.map((member, index) => ({ ...member, weight: weighted.signal_weights![index]! }));
    const input: TickEconomicReplayStart = { requestId: text(body.requestId, 'request_id'), datasetId: text(body.datasetId, 'dataset_id'),
      ...(body.members === undefined ? { factorId: members[0]!.factorId, request } : { members, request: weighted }),
      ...(body.fundingCatalog === undefined ? {} : { fundingCatalog: text(body.fundingCatalog, 'funding_catalog') }) };
    return host.harness.commit(async tx => {
      const replays = await tx.doc(TickEconomicReplays);
      const existing = Object.values(replays.runs).map(fact).find(row => row.original.requestId === input.requestId);
      if (existing) {
        if (canonical(json(existing.original)) !== canonical(json(input))) throw new Problem('request_id_conflict', 409);
        return json(existing.view) as unknown as TickEconomicReplayView;
      }
      const dataset = config.datasets.find(row => row.id === input.datasetId);
      if (!dataset) throw new Problem('dataset_not_found', 404);
      if (!dataset.ready || !dataset.preparation?.nativePrepared) throw new Problem('dataset_not_ready', 409);
      const application = await tx.doc(Application); assertBusinessAdmission(application);
      const requests = await tx.doc(Requests);
      const retained = [];
      for (const member of members) {
        const reference = factorReference(member.factorId);
        const study = await tx.doc(Study, reference.researchId as ConversationId);
        if (!study.input || study.deletedAt || study.deletedFactors?.[member.factorId]) throw new Problem('factor_not_found', 404);
        const admitted = Object.entries(requests.requests).some(([key, row]) => row.conversationId === reference.researchId
          && object(application.researchRequests[key]?.dataset ?? {}).id === input.datasetId);
        if (!admitted) throw new Problem('tick_economic_factor_dataset_mismatch', 409);
        const trial = study.trials.find(row => row.taskId === reference.trialId);
        if (!trial || trial.status !== 'COMPUTED' || !trial.result) throw new Problem('tick_economic_computed_factor_required', 422);
        const models = strategyMembers(object(trial.proposal));
        if (models.length !== 1 || reference.memberIndex !== 0) throw new Problem('tick_economic_single_factor_required', 422);
        const source = matchingBinding(study.input.data, dataset.data, input.request.research)
          ?? (study.input.data.sealed && dataset.data.sealed
            ? matchingBinding(object(study.input.data.sealed), object(dataset.data.sealed), input.request.research) : null);
        if (!source) throw new Problem('tick_economic_original_request_mismatch', 409);
        if (input.fundingCatalog !== undefined && input.fundingCatalog !== source.fundingCatalog) throw new Problem('tick_economic_funding_catalog_mismatch', 409);
        const native = object(trial.result);
        const coverage = object(native.coverage, 'native_coverage');
        if (canonical(withoutExecutionQuotas(native.request!)) !== canonical(withoutExecutionQuotas(iterationRequest(study.input.data))) || coverage.modality !== 'TRADE_TICK'
          || coverage.source_revision !== source.request.input.source_revision || typeof coverage.trade_count !== 'string' || !/^[1-9]\d*$/.test(coverage.trade_count)
          || typeof object(native.native_versions, 'native_versions')['nautilus-model'] !== 'string') throw new Problem('tick_economic_native_evidence_mismatch', 409);
        const target = boundScienceTarget(config, study.input.data);
        if (retained.length && (source.catalog !== retained[0]!.source.catalog || source.fundingCatalog !== retained[0]!.source.fundingCatalog
          || canonical(target) !== canonical(retained[0]!.target))) throw new Problem('tick_economic_member_binding_mismatch', 409);
        retained.push({ reference, source, target, model: models[0]!.model, factorId: member.factorId });
      }
      const { reference, source, target } = retained[0]!;
      const id = randomUUID();
      const bookCores = settings(application.settings).tickBookCores ?? 1;
      const taskId = await tx.createTask(TickEconomicReplay, { id, createdAt: new Date().toISOString(), dataDir: config.dataDir, catalog: source.catalog, fundingCatalog: source.fundingCatalog,
        model: retained.length === 1 ? retained[0]!.model : retained.map(member => member.model), request: input.request, target, bookCores },
      { conversationId: reference.researchId as ConversationId, ownership: { kind: 'conversation' } });
      const view: TickEconomicReplayView = { id, requestId: input.requestId, datasetId: input.datasetId,
        ...(input.members ? { members: input.members } : { factorId: input.factorId }), taskId, fundingCatalog: source.fundingCatalog,
        sources: retained.map(member => ({ factorId: member.factorId, researchId: member.reference.researchId, trialId: member.reference.trialId,
          memberIndex: member.reference.memberIndex, sourceRevision: member.source.request.input.source_revision })),
        status: 'QUEUED', scientificQualification: 'UNQUALIFIED_EXPLORATORY', result: null, error: null, bookCores };
      replays.runs[id] = json({ original: input, view });
      return view;
    }, context);
  }
  return { start, get, list };
}

/** Register after the existing owner authentication and maintenance hooks. */
export function registerTickEconomicRoutes(app: FastifyInstance, work: ReturnType<typeof tickEconomicWorkbench>) {
  app.get(`${API}/tick-economic-replays`, () => work.list());
  app.post(`${API}/tick-economic-replays`, async (request, reply) => reply.code(202).send(await work.start(request.body)));
  app.get<{ Params: { id: string } }>(`${API}/tick-economic-replays/:id`, request => work.get(request.params.id));
}
