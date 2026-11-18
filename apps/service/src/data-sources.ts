import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc, defineTask, defineExtension, type JsonObject as DurableJson, type TaskId } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import { readFile, realpath } from 'node:fs/promises';
import { dirname, resolve, relative, isAbsolute } from 'node:path';
import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import type { JsonObject } from '../../../packages/contracts/src/index.ts';
import type { DataSourceView } from '../../../packages/contracts/src/data-sources.ts';
import { bindScienceData, resolveDatasetPaths, scienceDataWithoutBinding, validateDatasetInput } from './config.ts';
import type { DatasetConfig, ServiceConfig } from './config.ts';
import { Application, object, Problem } from './facts.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';
import { canonical } from '../../research/src/documents.ts';
import { validateNumbers } from '../../research/src/input.ts';

const registeredName = 'qz.data.registered';
interface Registration { input: { expectedRevision: number; path: string }; receipt: { requestId: string; revision: number; datasetIds: string[] } }
interface SavedSource {
  registrations?: Record<string, Registration>;
  acquisition?: { manifestPath: string; planId: string }; revision?: number; taskId?: number; config: JsonObject; enabled: boolean; datasets: DatasetConfig[]; retainedDatasets?: DatasetConfig[]; lastLoadedAt: string | null; error: string | null;
}
interface Source extends SavedSource { name: string; version: string | null }
export async function readDatasetConfig(file: string): Promise<DatasetConfig[]> {
  const parsed: unknown = JSON.parse(await readFile(file, 'utf8'));
  return datasets(Array.isArray(parsed) ? parsed : [parsed]).map(value => resolveDatasetPaths(value, dirname(file)));
}
function datasets(value: unknown): DatasetConfig[] {
  validateNumbers(value);
  if (!Array.isArray(value)) throw new Problem('data_source_datasets_invalid');
  const ids = new Set<string>();
  for (const item of value) {
    const row = object(item, 'data_source_dataset');
    if (['id', 'name', 'assetClass', 'currency', 'source'].some(key => typeof row[key] !== 'string' || !String(row[key]).trim())
      || ['start', 'end'].some(key => row[key] !== null && (typeof row[key] !== 'string' || !Number.isFinite(Date.parse(String(row[key])))))
      || typeof row.ready !== 'boolean' || row.revisionManifest !== undefined && typeof row.revisionManifest !== 'string') throw new Problem('data_source_dataset_invalid');
    object(row.data, 'data_source_dataset_data');
    validateDatasetInput(row as unknown as DatasetConfig);
    if (row.policy !== undefined) object(row.policy, 'data_source_dataset_policy');
    if (ids.has(String(row.id))) throw new Problem('data_source_dataset_id_conflict', 409);
    ids.add(String(row.id));
  }
  return structuredClone(value) as DatasetConfig[];
}
export const DataSourceState = defineDoc<{ sources: DurableJson }>({
  kind: 'qz.data-sources', version: 1, scope: 'session', initial: () => ({ sources: {} }),
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 32,
});

/** Dataset references only; existing research retains its captured native catalog. */
export async function dataSources(config: ServiceConfig) {
  const base = structuredClone(config.datasets);
  let stored: Record<string, SavedSource> = {};
  const sources = new Map<string, Source>();
  // Persisted provider records are now ordinary dataset references. Never import
  // old modules or reread their source descriptors while opening the registry.
  for (const name of ['qz.data.config', registeredName]) {
    sources.set(name, { name, version: '1', enabled: true, config: {},
      datasets: name === 'qz.data.config' ? base : [], retainedDatasets: [], lastLoadedAt: null, error: null });
  }
  function merged(): DatasetConfig[] {
    const owners = new Map<string, string>();
    for (const source of [
      ...Object.entries(stored).map(([name, value]) => ({ name, datasets: value.datasets, retainedDatasets: value.retainedDatasets })),
      ...sources.values(),
    ]) {
      for (const dataset of [...(source.retainedDatasets ?? []), ...source.datasets]) {
        const owner = owners.get(dataset.id);
        if (owner !== undefined && owner !== source.name) throw new Problem('data_source_dataset_id_conflict', 409);
        owners.set(dataset.id, source.name);
      }
    }
    const result = [...sources.values()].flatMap(source => source.enabled ? source.datasets : []);
    const ids = new Set<string>();
    for (const dataset of result) {
      if (ids.has(dataset.id)) throw new Problem('data_source_dataset_id_conflict', 409);
      ids.add(dataset.id);
    }
    return result.map(dataset => ({ ...structuredClone(dataset), data: (config.nativeProcess ?? config.nautilus?.backtest)
      ? bindScienceData(scienceDataWithoutBinding(dataset.data), (config.nativeProcess ?? config.nautilus?.backtest)!) : scienceDataWithoutBinding(dataset.data) }));
  }
  function apply() {
    config.datasets = merged();
    const retained = new Map<string, DatasetConfig>();
    for (const source of [...Object.values(stored), ...sources.values()]) {
      for (const dataset of [...(source.retainedDatasets ?? []), ...source.datasets]) retained.set(dataset.id, dataset);
    }
    config.retainedDatasets = [...retained.values()].map(dataset => ({ ...structuredClone(dataset), data: (config.nativeProcess ?? config.nautilus?.backtest)
      ? bindScienceData(scienceDataWithoutBinding(dataset.data), (config.nativeProcess ?? config.nautilus?.backtest)!) : scienceDataWithoutBinding(dataset.data) }));
  }
  apply();
  let host: ResearchHost | undefined;
  let closing = false;
  let detach: (() => void) | undefined;
  const context = BACKGROUND_CONTEXT;
  const State = DataSourceState;
  function connected() { if (!host || closing) throw new Problem('data_source_runtime_unavailable', 503); return host; }
  function hydrate(saved: Record<string, SavedSource>) {
    stored = structuredClone(saved);
    for (const [name, value] of Object.entries(stored)) {
      const source = sources.get(name);
      sources.set(name, { name, version: source?.version ?? null, ...structuredClone(value) });
    }
    apply();
  }
  async function synchronize() {
    const value = await connected().harness.snapshot(State, context);
    if (value) hydrate(value.sources as unknown as Record<string, SavedSource>);
  }
  const CancelRefresh = defineTask<{ target: number }, { phase: 'cancel' }, null>({
    name: 'qz.data-source.cancel', version: 1, initial: () => ({ phase: 'cancel' }),
    phases: { async cancel(task, runtime, ctx) {
      if (!host) throw new Error('data_source_runtime_unavailable');
      await host.harness.abortTask(task.input.target as TaskId, ctx);
      await host.harness.waitForTask(task.input.target as TaskId, ctx);
      await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result: null } }), ctx);
    } },
    abort: (_task, runtime, ctx) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx),
  });
  function found(name: string): Source {
    const value = sources.get(name); if (!value) throw new Problem('data_source_not_found', 404); return value;
  }
  function viewOne(source: Source): DataSourceView {
    return structuredClone({ name: source.name, version: source.version, enabled: source.enabled,
      state: source.error ? 'error' : source.enabled ? 'loaded' : 'disabled', fields: [], config: {}, ...(source.name === registeredName ? { revision: source.revision ?? 1 } : {}),
      datasetIds: source.datasets.map(value => value.id), lastLoadedAt: source.lastLoadedAt, error: source.error, ...(source.acquisition ? { acquisition: source.acquisition } : {}) });
  }
  function assertOwnership(saved: Record<string, SavedSource>, name: string, replacement: SavedSource) {
    const ids = new Set([...(replacement.retainedDatasets ?? []), ...replacement.datasets].map(row => row.id));
    for (const [owner, value] of Object.entries(saved)) {
      if (owner === name) continue;
      if ([...(value.retainedDatasets ?? []), ...value.datasets].some(row => ids.has(row.id))) throw new Problem('data_source_dataset_id_conflict', 409);
    }
  }
  const Refresh = defineTask<{ name: string; revision: number; pluginVersion: string | null },
    { phase: 'load' } | { phase: 'publish'; loaded: DurableJson[] }, DurableJson>({
    name: 'qz.data-source.refresh', version: 1, initial: () => ({ phase: 'load' }),
    phases: {
      async load(_task, runtime, ctx) {
        // Compatibility for an original pending task, not a new plugin runtime.
        // No provider code or filesystem acquisition is executed after retirement.
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: 'data_source_plugins_retired' } } }), ctx);
      },
      async publish(task, runtime, ctx) {
        if (task.state.checkpoint.phase !== 'publish') throw new Error('data_source_checkpoint_invalid');
        const loaded = datasets(task.state.checkpoint.loaded);
        await runtime.commit(async tx => {
          const saved = (await tx.doc(State)).sources[task.input.name] as unknown as SavedSource;
          if (!saved.enabled || saved.revision !== task.input.revision) return { status: 'terminal', outcome: { status: 'failed', error: { message: 'data_source_refresh_superseded' } } };
          const state = await tx.doc(State);
          try { assertOwnership(state.sources as unknown as Record<string, SavedSource>, task.input.name, { ...saved, datasets: loaded }); }
          catch {
            saved.error = 'data_source_dataset_id_conflict';
            return { status: 'terminal', outcome: { status: 'failed', error: { message: 'data_source_dataset_id_conflict' } } };
          }
          saved.datasets = loaded;
          saved.retainedDatasets = [...new Map([...(saved.retainedDatasets ?? []), ...loaded].map(dataset => [dataset.id, dataset])).values()];
          saved.lastLoadedAt = new Date(runtime.now()).toISOString(); saved.error = null;
          return { status: 'terminal', outcome: { status: 'completed', result: { name: task.input.name } } };
        }, ctx);
      },
    },
    abort: (_task, runtime, ctx) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx),
  });
  const extension = defineExtension({ name: 'qz.data-source-runtime', tasks: [Refresh, CancelRefresh] });
  // Register a new identity only. File contents are captured once on successful publication;
  // retrying the original request returns its receipt without reopening the source file.
  async function registerDatasets(value: unknown, descriptorWorkspace?: string) {
    const body = object(value, 'dataset_registration');
    if (Object.keys(body).some(key => !['requestId', 'expectedRevision', 'path'].includes(key))
      || typeof body.requestId !== 'string' || !body.requestId.trim()
      || typeof body.path !== 'string' || !body.path.trim()
      || !Number.isSafeInteger(body.expectedRevision) || Number(body.expectedRevision) < 1) throw new Problem('dataset_registration_invalid', 422);
    const requestId = body.requestId;
    const input = { expectedRevision: Number(body.expectedRevision), path: body.path };
    const runtime = connected();
    const replay = (saved: SavedSource) => {
      const previous = saved.registrations?.[`request:${requestId}`];
      if (!previous) return undefined;
      if (canonical(previous.input) !== canonical(input)) throw new Problem('request_id_conflict', 409);
      return { requestId: previous.receipt.requestId, revision: previous.receipt.revision, datasetIds: [...previous.receipt.datasetIds] };
    };
    const initial = (await runtime.harness.snapshot(State, context))!.sources[registeredName] as unknown as SavedSource;
    const prior = replay(initial); if (prior) return prior;
    if (initial.revision !== input.expectedRevision) throw new Problem('data_source_revision_conflict', 409);
    const descriptorPath=resolve(config.dataDir,input.path);
    if(descriptorWorkspace!==undefined){
      const root=await realpath(descriptorWorkspace), path=await realpath(descriptorPath);
      const within=relative(root,path);
      if(within==='..'||within.startsWith('../')||isAbsolute(within))throw new Problem('research_descriptor_outside_workspace',422);
    }
    let loaded: DatasetConfig[];
    try { loaded = await readDatasetConfig(descriptorPath); }
    catch { throw new Problem('dataset_registration_file_invalid', 422); }
    if (!loaded.length) throw new Problem('dataset_registration_empty', 422);
    const receipt = await runtime.harness.commit(async tx => {
      const state = await tx.doc(State);
      const saved = state.sources[registeredName] as unknown as SavedSource;
      const prior = replay(saved); if (prior) return prior;
      assertBusinessAdmission(await tx.doc(Application));
      if (saved.revision !== input.expectedRevision) throw new Problem('data_source_revision_conflict', 409);
      if (!Number.isSafeInteger(input.expectedRevision + 1)) throw new Problem('data_source_revision_exhausted', 409);
      const ids = new Set(loaded.map(dataset => dataset.id));
      for (const source of Object.values(state.sources) as unknown as SavedSource[]) {
        if ([...source.datasets, ...(source.retainedDatasets ?? [])].some(dataset => ids.has(dataset.id))) throw new Problem('data_source_dataset_id_conflict', 409);
      }
      const receipt = { requestId, revision: input.expectedRevision + 1, datasetIds: loaded.map(dataset => dataset.id) };
      saved.datasets = [...saved.datasets, ...loaded];
      saved.retainedDatasets = [...(saved.retainedDatasets ?? []), ...loaded];
      saved.revision = receipt.revision;
      saved.registrations = { ...saved.registrations, [`request:${requestId}`]: { input, receipt } };
      return receipt;
    }, context);
    await synchronize();
    return receipt;
  }
  async function taskStatus(name: string, requestedId?: number) {
    const source = found(name); const id = requestedId ?? source.taskId;
    if (!id) return { taskId: null, status: 'idle', source: viewOne(source) };
    if (!Number.isSafeInteger(id) || id <= 0) throw new Problem('data_source_task_invalid');
    const task = await connected().harness.getTask(id as TaskId, context);
    if (!task || task.kind !== Refresh.definition.name || (task.input as DurableJson).name !== name) throw new Problem('data_source_task_not_found', 404);
    return { taskId: id, status: task.state.status,
      ...(task.state.status === 'terminal' ? { outcome: task.state.outcome.status } : {}), source: viewOne(found(name)) };
  }
  return { extension, taskStatus, registerDatasets,
    async connect(value: ResearchHost) {
      host = value;
      await host.harness.commit(async tx => {
        const state = await tx.doc(State);
        for (const source of sources.values()) {
          const previous = state.sources[source.name] as unknown as SavedSource | undefined;
          const staticChanged = source.name === 'qz.data.config' && previous && canonical(previous.datasets as unknown as DurableJson[]) !== canonical(source.datasets as unknown as DurableJson[]);
          if (!previous || staticChanged) state.sources[source.name] = {
            config: source.config, enabled: previous?.enabled ?? source.enabled, datasets: source.datasets,
            retainedDatasets: [...new Map([...(previous?.retainedDatasets ?? []), ...(previous?.datasets ?? []), ...(source.retainedDatasets ?? [])].map(dataset => [dataset.id, dataset])).values()],
            lastLoadedAt: null, error: source.error, revision: (previous?.revision ?? 0) + 1,
          } as unknown as DurableJson;
        }
        for (const [name, saved] of Object.entries(state.sources)) assertOwnership(state.sources as unknown as Record<string, SavedSource>, name, saved as unknown as SavedSource);
      }, context);
      const observation = await host.harness.observeDoc(State, {
        advance(call) { if (call.kind === 'value') hydrate(call.value.sources as unknown as Record<string, SavedSource>); },
        end() {},
      }, context);
      if (observation) { hydrate(observation.value.sources as unknown as Record<string, SavedSource>); detach = () => observation.detach(); }
    },
    view: () => [...sources.values()].map(viewOne),
    registry: () => ({ revision: found(registeredName).revision ?? 1, datasetIds: config.datasets.map(row => row.id) }),
    async close() { closing = true; detach?.(); },
  };
}
export function registerDataSources(app: FastifyInstance, sources: Awaited<ReturnType<typeof dataSources>>) {
  app.get(`${API}/datasets/registry`, sources.registry);
  app.post(`${API}/datasets/register`, request => sources.registerDatasets(request.body));
  // Historical read and registration addresses remain thin aliases, not plugins.
  app.get(`${API}/data-sources`, sources.view);
  app.post(`${API}/data-sources/qz.data.registered/datasets`, request => sources.registerDatasets(request.body));
  app.put(`${API}/data-sources/:name`, () => { throw new Problem('data_source_plugins_retired', 410); });
  app.post(`${API}/data-sources/:name/refresh`, () => { throw new Problem('data_source_plugins_retired', 410); });
}
