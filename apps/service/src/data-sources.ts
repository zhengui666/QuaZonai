import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { defineDoc, defineTask, defineExtension, type JsonObject as DurableJson, type TaskId } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import type { ExtensionView, JsonObject } from '../../../packages/contracts/src/index.ts';
import type { DataSourceField, DataSourceView } from '../../../packages/contracts/src/data-sources.ts';
import { bindScienceData, resolveDatasetPaths, scienceDataWithoutBinding, validateDatasetInput } from './config.ts';
import type { DatasetConfig, ServiceConfig } from './config.ts';
import { Application, object, Problem } from './facts.ts';
import { assertBusinessAdmission } from './pm-maintenance.ts';
import { canonical } from '../../research/src/documents.ts';
import { validateNumbers } from '../../research/src/input.ts';
import cryptohftdata from './providers/cryptohftdata.ts';

export interface DataSourcePlugin {
  name: string; version?: string; fields: DataSourceField[];
  load(config: JsonObject, context: { dataDir: string; signal?: AbortSignal }): Promise<DatasetConfig[]>;
}
interface SavedSource {
  acquisition?: { manifestPath: string; planId: string }; revision?: number; taskId?: number; config: JsonObject; enabled: boolean; datasets: DatasetConfig[]; retainedDatasets?: DatasetConfig[]; lastLoadedAt: string | null; error: string | null;
}
interface Source extends SavedSource { name: string; version: string | null; fields: DataSourceField[]; plugin?: DataSourcePlugin }
const local: DataSourcePlugin = {
  name: 'qz.data.local-config', version: '1',
  fields: [{ key: 'path', label: '数据集配置文件', type: 'text', required: true, description: '现有数据工具导出的 DatasetConfig JSON 文件；相对数据目录解析。' }],
  async load(config, context) {
    const file = resolve(context.dataDir, String(config.path));
    return readDatasetConfig(file);
  },
};
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
function validateConfig(plugin: DataSourcePlugin, value: unknown, requireFields = true): JsonObject {
  const config = { ...object(value, 'data_source_config') };
  if (Object.keys(config).some(key => !plugin.fields.some(field => field.key === key))) throw new Problem('data_source_config_field_invalid');
  for (const field of plugin.fields) {
    const value = config[field.key];
    if (value === undefined || value === null || value === '') { if (field.required && requireFields) throw new Problem('data_source_config_required'); delete config[field.key]; continue; }
    if (typeof value !== (['text', 'password'].includes(field.type) ? 'string' : field.type) || typeof value === 'number' && !Number.isFinite(value)) throw new Problem('data_source_config_invalid');
    if (field.choices && !field.choices.some(choice => choice.value === value)) throw new Problem('data_source_config_invalid');
  }
  return structuredClone(config);
}
function validatePlugin(value: unknown, name: string): DataSourcePlugin {
  const plugin = value as DataSourcePlugin;
  if (!plugin || plugin.name !== name || typeof plugin.load !== 'function' || !Array.isArray(plugin.fields)
    || plugin.version !== undefined && typeof plugin.version !== 'string'
    || plugin.fields.some(field => !field || typeof field.key !== 'string' || !field.key || typeof field.label !== 'string'
      || !['text', 'password', 'number', 'boolean'].includes(field.type)
      || field.choices !== undefined && (field.type !== 'text' || !Array.isArray(field.choices) || !field.choices.length
        || field.choices.some(choice => !choice || typeof choice.value !== 'string' || !choice.value || typeof choice.label !== 'string' || !choice.label)
        || new Set(field.choices.map(choice => choice.value)).size !== field.choices.length))
    || new Set(plugin.fields.map(field => field.key)).size !== plugin.fields.length) throw new Problem('data_source_plugin_invalid');
  return plugin;
}

export const DataSourceState = defineDoc<{ sources: DurableJson }>({
  kind: 'qz.data-sources', version: 1, scope: 'session', initial: () => ({ sources: {} }),
  checkpointWhen: (_value, _ops, info) => info.deltasSinceBase >= 32,
});

/** Data providers publish new admissions; captured research inputs keep their original catalogs. */
export async function dataSources(config: ServiceConfig) {
  const base = structuredClone(config.datasets);
  let stored: Record<string, SavedSource> = {};
  const sources = new Map<string, Source>();
  const configured = [{ name: 'qz.data.config', version: '1', enabled: true, config: {} as JsonObject, module: '', kind: 'data' }, ...[local, cryptohftdata].map(plugin => ({ name: plugin.name, version: plugin.version, enabled: false, config: {} as JsonObject, module: '', kind: 'data' })), ...config.extensions.filter(item => item.kind === 'data')];
  for (const entry of configured) {
    if (sources.has(entry.name)) throw new Problem('extension_name_conflict');
    const saved = stored[entry.name];
    const source: Source = { name: entry.name, version: entry.version ?? null, fields: [], enabled: saved?.enabled ?? entry.enabled,
      config: structuredClone(saved?.config ?? entry.config ?? {}), datasets: entry.name === 'qz.data.config' ? base : saved ? datasets(saved.datasets) : [], retainedDatasets: saved ? datasets(saved.retainedDatasets ?? saved.datasets) : [], lastLoadedAt: saved?.lastLoadedAt ?? null, error: saved?.error ?? null };
    try {
      source.plugin = validatePlugin(entry.name === 'qz.data.config' ? { name: entry.name, version: '1', fields: [], async load() { return base; } } : entry.name === local.name ? local : entry.name === cryptohftdata.name ? cryptohftdata : (await import(pathToFileURL(entry.module).href)).default, entry.name);
      source.fields = source.plugin.fields; source.version = source.plugin.version ?? source.version;
    } catch { source.error = 'data_source_plugin_load_failed'; }
    sources.set(entry.name, source);
  }
  function merged(replacement?: Source): DatasetConfig[] {
    const owners = new Map<string, string>();
    for (const source of [
      ...Object.entries(stored).map(([name, value]) => ({ name, datasets: value.datasets, retainedDatasets: value.retainedDatasets })),
      ...sources.values(), ...(replacement ? [replacement] : []),
    ]) {
      for (const dataset of [...(source.retainedDatasets ?? []), ...source.datasets]) {
        const owner = owners.get(dataset.id);
        if (owner !== undefined && owner !== source.name) throw new Problem('data_source_dataset_id_conflict', 409);
        owners.set(dataset.id, source.name);
      }
    }
    const result = [...sources.values()].flatMap(source => { const selected = source.name === replacement?.name ? replacement : source; return selected.enabled ? selected.datasets : []; });
    const ids = new Set<string>();
    for (const dataset of result) {
      if (ids.has(dataset.id)) throw new Problem('data_source_dataset_id_conflict', 409);
      ids.add(dataset.id);
    }
    return result.map(dataset => ({ ...structuredClone(dataset), data: config.nautilus?.backtest
      ? bindScienceData(scienceDataWithoutBinding(dataset.data), config.nautilus.backtest) : scienceDataWithoutBinding(dataset.data) }));
  }
  function apply() {
    config.datasets = merged();
    const retained = new Map<string, DatasetConfig>();
    for (const source of [...Object.values(stored), ...sources.values()]) {
      for (const dataset of [...(source.retainedDatasets ?? []), ...source.datasets]) retained.set(dataset.id, dataset);
    }
    config.retainedDatasets = [...retained.values()].map(dataset => ({ ...structuredClone(dataset), data: config.nautilus?.backtest
      ? bindScienceData(scienceDataWithoutBinding(dataset.data), config.nautilus.backtest) : scienceDataWithoutBinding(dataset.data) }));
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
      if (source) sources.set(name, { name: source.name, version: source.version, fields: source.fields, plugin: source.plugin, ...structuredClone(value) });
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
      state: source.error ? 'error' : source.enabled ? 'loaded' : 'disabled', fields: source.fields, config: source.config,
      datasetIds: source.datasets.map(value => value.id), lastLoadedAt: source.lastLoadedAt, error: source.error, ...(source.acquisition ? { acquisition: source.acquisition } : {}) });
  }
  function assertOwnership(saved: Record<string, SavedSource>, name: string, replacement: SavedSource) {
    const ids = new Set([...(replacement.retainedDatasets ?? []), ...replacement.datasets].map(row => row.id));
    for (const [owner, value] of Object.entries(saved)) {
      if (owner === name) continue;
      if ([...(value.retainedDatasets ?? []), ...value.datasets].some(row => ids.has(row.id))) throw new Problem('data_source_dataset_id_conflict', 409);
    }
  }
  async function persist(next: Source, changeConfig = true) {
    const runtime = connected(); const owner = await runtime.harness.root(context);
    const cancellation = await owner.commit(async tx => {
      const state = await tx.doc(State);
      const previous = state.sources[next.name] as unknown as SavedSource | undefined;
      const settings = !changeConfig && previous ? previous.config : next.config;
      const changed = !previous || previous.enabled !== next.enabled || canonical(previous.config) !== canonical(settings);
      const updated: SavedSource = { config: settings, enabled: next.enabled, datasets: previous?.datasets ?? next.datasets,
        retainedDatasets: [...new Map([...(previous?.retainedDatasets ?? next.retainedDatasets ?? []), ...(previous?.datasets ?? next.datasets)].map(dataset => [dataset.id, dataset])).values()],
        lastLoadedAt: previous?.lastLoadedAt ?? next.lastLoadedAt, error: changeConfig ? next.error : previous?.error ?? next.error,
        revision: changed ? (previous?.revision ?? 0) + 1 : previous?.revision ?? 1,
        ...(previous?.taskId ? { taskId: previous.taskId } : {}),
        ...(previous?.acquisition ? { acquisition: previous.acquisition } : {}),
      };
      assertOwnership(state.sources as unknown as Record<string, SavedSource>, next.name, updated);
      state.sources[next.name] = updated as unknown as DurableJson;
      return (changed || !next.enabled) && previous?.taskId
        ? await tx.createTask(CancelRefresh, { target: previous.taskId }, { ownership: { kind: 'conversation' }, background: true }) : undefined;
    }, context);
    await synchronize();
    if (cancellation) {
      const settled = await runtime.harness.waitForTask(cancellation, context);
      if (settled.state.outcome.status !== 'completed') throw new Problem('data_source_cancellation_unconfirmed', 503);
    }
    return viewOne(found(next.name));
  }
  const save = async (name: string, body: unknown) => {
    const source = found(name); const value = object(body, 'data_source');
    if (Object.keys(value).some(key => !['config', 'enabled'].includes(key)) || typeof value.enabled !== 'boolean') throw new Problem('data_source_update_invalid');
    if (!source.plugin) throw new Problem('data_source_plugin_unavailable');
    const settings = validateConfig(source.plugin, value.config, value.enabled);
    return persist({ ...found(name), config: settings, enabled: value.enabled, error: null });
  };
  const toggle = async (name: string, enabled: boolean) => {
    if (typeof enabled !== 'boolean') throw new Problem('enabled_boolean_required');
    return persist({ ...found(name), enabled }, false);
  };
  const Refresh = defineTask<{ name: string; revision: number; pluginVersion: string | null },
    { phase: 'load' } | { phase: 'publish'; loaded: DurableJson[] }, DurableJson>({
    name: 'qz.data-source.refresh', version: 1, initial: () => ({ phase: 'load' }),
    phases: {
      async load(task, runtime, ctx) {
        try {
          const state = await runtime.snapshot(State, ctx);
          const saved = state?.sources[task.input.name] as unknown as SavedSource | undefined;
          const source = found(task.input.name);
          if (!saved?.enabled || saved.revision !== task.input.revision) throw new Error('data_source_refresh_superseded');
          if (!source.plugin || source.version !== task.input.pluginVersion) throw new Error('data_source_plugin_changed');
          const loaded = datasets(await source.plugin.load(validateConfig(source.plugin, saved.config), { dataDir: config.dataDir, signal: runtime.signal }));
          merged({ ...source, datasets: loaded });
          await runtime.commit(() => ({ status: 'running', checkpoint: { phase: 'publish', loaded: loaded as unknown as DurableJson[] } }), ctx);
        } catch (error) {
          if (runtime.signal.aborted) throw error;
          const message = error instanceof Error && ['data_source_refresh_superseded', 'data_source_plugin_changed'].includes(error.message) ? error.message : 'data_source_refresh_failed';
          await runtime.commit(async tx => {
            const saved = (await tx.doc(State)).sources[task.input.name] as unknown as SavedSource;
            if (saved.revision === task.input.revision) saved.error = message;
            return { status: 'terminal', outcome: { status: 'failed', error: { message } } };
          }, ctx);
        }
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
  const refresh = async (name: string) => {
    const source = found(name); if (!source.enabled) throw new Problem('data_source_disabled', 409);
    if (!source.plugin) throw new Problem('data_source_plugin_unavailable');
    const runtime = connected();
    const owner = await runtime.harness.root(context);
    const taskId = await owner.commit(async tx => {
      assertBusinessAdmission(await tx.doc(Application));
      const saved = (await tx.doc(State)).sources[name] as unknown as SavedSource;
      if (saved.revision !== source.revision || !saved.enabled) throw new Problem('data_source_refresh_superseded', 409);
      if (saved.taskId) {
        const previous = await tx.task(saved.taskId as TaskId);
        if (previous && previous.state.status !== 'terminal' && (previous.input as DurableJson).revision === saved.revision) return previous.id;
      }
      const id = await tx.createTask(Refresh, { name, revision: saved.revision ?? 0, pluginVersion: source.version }, { ownership: { kind: 'conversation' } });
      saved.taskId = id as number; return id;
    }, context);
    await synchronize(); return { taskId: taskId as number };
  };
  async function taskStatus(name: string, requestedId?: number) {
    const source = found(name); const id = requestedId ?? source.taskId;
    if (!id) return { taskId: null, status: 'idle', source: viewOne(source) };
    if (!Number.isSafeInteger(id) || id <= 0) throw new Problem('data_source_task_invalid');
    const task = await connected().harness.getTask(id as TaskId, context);
    if (!task || task.kind !== Refresh.definition.name || (task.input as DurableJson).name !== name) throw new Problem('data_source_task_not_found', 404);
    return { taskId: id, status: task.state.status,
      ...(task.state.status === 'terminal' ? { outcome: task.state.outcome.status } : {}), source: viewOne(found(name)) };
  }
  return { extension, taskStatus,
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
    view: () => [...sources.values()].map(viewOne), has: (name: string) => sources.has(name), save, toggle, refresh,
    extensions: (): ExtensionView[] => [...sources.values()].map(source => ({ name: source.name, version: source.version, enabled: source.enabled,
      kind: 'data', state: source.error ? 'error' : source.enabled ? 'loaded' : 'disabled', error: source.error })),
    async close() { closing = true; detach?.(); },
  };
}
export function registerDataSources(app: FastifyInstance, sources: Awaited<ReturnType<typeof dataSources>>) {
  app.get(`${API}/data-sources`, sources.view);
  app.put<{ Params: { name: string } }>(`${API}/data-sources/:name`, request => sources.save(request.params.name, request.body));
  app.post<{ Params: { name: string } }>(`${API}/data-sources/:name/refresh`, async (request, reply) => reply.code(202).send(await sources.refresh(request.params.name)));
}
