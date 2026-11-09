import { createHash, randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { atomicWriteFile } from '../../research/src/durable-files.ts';
import type { ResearchHost } from '../../research/src/host.ts';
import { canonical } from '../../research/src/documents.ts';
import { createDataSourceTasks } from './data-source-tasks.ts';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import type { ExtensionView, JsonObject } from '../../../packages/contracts/src/index.ts';
import type { DataSourceField, DataSourceView } from '../../../packages/contracts/src/data-sources.ts';
import { bindScienceData, resolveDatasetPaths, scienceDataWithoutBinding, validateDatasetInput } from './config.ts';
import type { DatasetConfig, ServiceConfig } from './config.ts';
import { object, Problem } from './facts.ts';
import { validateNumbers } from '../../research/src/input.ts';
import cryptohftdata from './providers/cryptohftdata.ts';

export interface DataSourcePlugin {
  name: string; version?: string; fields: DataSourceField[];
  /** Explicit declaration: load performs only repeatable reads. Long conversions
   * must use a checkpoint-capable managed worker rather than this Promise API. */
  recovery?: 'read-only';
  load(config: JsonObject, context: { dataDir: string; signal?: AbortSignal }): Promise<DatasetConfig[]>;
}
interface SavedSource {
  config: JsonObject; enabled: boolean; datasets: DatasetConfig[]; retainedDatasets?: DatasetConfig[]; lastLoadedAt: string | null; error: string | null;
}
interface Source extends SavedSource { name: string; version: string | null; fields: DataSourceField[]; plugin?: DataSourcePlugin }
const local: DataSourcePlugin = {
  name: 'qz.data.local-config', version: '1', recovery: 'read-only',
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
    || plugin.recovery !== undefined && plugin.recovery !== 'read-only'
    || plugin.fields.some(field => !field || typeof field.key !== 'string' || !field.key || typeof field.label !== 'string'
      || !['text', 'password', 'number', 'boolean'].includes(field.type)
      || field.choices !== undefined && (field.type !== 'text' || !Array.isArray(field.choices) || !field.choices.length
        || field.choices.some(choice => !choice || typeof choice.value !== 'string' || !choice.value || typeof choice.label !== 'string' || !choice.label)
        || new Set(field.choices.map(choice => choice.value)).size !== field.choices.length))
    || new Set(plugin.fields.map(field => field.key)).size !== plugin.fields.length) throw new Problem('data_source_plugin_invalid');
  return plugin;
}

/** Data providers publish new admissions; captured research inputs keep their original catalogs. */
export async function dataSources(config: ServiceConfig) {
  const file = join(config.dataDir, 'data-sources.json');
  const base = structuredClone(config.datasets);
  let stored: Record<string, SavedSource> = {};
  try { stored = object(JSON.parse(await readFile(file, 'utf8')), 'data_sources') as unknown as typeof stored; }
  catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
  const sources = new Map<string, Source>();
  const configured = [{ name: 'qz.data.config', version: '1', enabled: true, config: {} as JsonObject, module: '', kind: 'data' }, ...[local, cryptohftdata].map(plugin => ({ name: plugin.name, version: plugin.version, enabled: false, config: {} as JsonObject, module: '', kind: 'data' })), ...config.extensions.filter(item => item.kind === 'data')];
  for (const entry of configured) {
    if (sources.has(entry.name)) throw new Problem('extension_name_conflict');
    const saved = stored[entry.name];
    const source: Source = { name: entry.name, version: entry.version ?? null, fields: [], enabled: saved?.enabled ?? entry.enabled,
      config: structuredClone(saved?.config ?? entry.config ?? {}), datasets: entry.name === 'qz.data.config' ? base : saved ? datasets(saved.datasets) : [], retainedDatasets: saved ? datasets(saved.retainedDatasets ?? saved.datasets) : [], lastLoadedAt: saved?.lastLoadedAt ?? null, error: saved?.error ?? null };
    try {
      source.plugin = validatePlugin(entry.name === 'qz.data.config' ? { name: entry.name, version: '1', recovery: 'read-only', fields: [], async load() { return base; } } : entry.name === local.name ? local : entry.name === cryptohftdata.name ? cryptohftdata : (await import(pathToFileURL(entry.module).href)).default, entry.name);
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
  let tail: Promise<unknown> = Promise.resolve(); let closing = false;
  const active = new Map<string, Set<AbortController>>();
  let connected = false;
  async function cancel(name: string) { for (const controller of active.get(name) ?? []) controller.abort(); await managed.cancel(name); }
  function serial<T>(run: () => Promise<T>): Promise<T> {
    const result = tail.then(() => { if (closing) throw new Problem('service_closing', 503); return run(); });
    tail = result.catch(() => undefined); return result;
  }
  function found(name: string): Source {
    const value = sources.get(name); if (!value) throw new Problem('data_source_not_found', 404); return value;
  }
  function viewOne(source: Source): DataSourceView {
    return structuredClone({ name: source.name, version: source.version, enabled: source.enabled,
      state: source.error ? 'error' : source.enabled ? 'loaded' : 'disabled', fields: source.fields, config: source.config,
      datasetIds: source.datasets.map(value => value.id), lastLoadedAt: source.lastLoadedAt, error: source.error });
  }
  async function persist(next: Source) {
    merged(next);
    next = { ...next, retainedDatasets: [...new Map([...(next.retainedDatasets ?? []), ...next.datasets].map(dataset => [dataset.id, dataset])).values()] };
    const saved = { ...stored, [next.name]: { config: next.config, enabled: next.enabled, datasets: next.datasets, retainedDatasets: next.retainedDatasets, lastLoadedAt: next.lastLoadedAt, error: next.error } };
    try { await atomicWriteFile(file, JSON.stringify(saved, null, 2) + '\n', 0o600); }
    catch { throw new Problem('data_source_save_failed', 503); }
    stored = saved; sources.set(next.name, next); apply();
    return viewOne(next);
  }
  const save = async (name: string, body: unknown) => {
    const update = object(body, 'data_source');
    if (update.enabled === false && Object.keys(update).every(key => ['config', 'enabled'].includes(key))) {
      const source = found(name);
      if (source.plugin) validateConfig(source.plugin, update.config, false);
      await cancel(name);
    }
    return serial(async () => {
      const source = found(name); const value = object(body, 'data_source');
      if (Object.keys(value).some(key => !['config', 'enabled'].includes(key)) || typeof value.enabled !== 'boolean') throw new Problem('data_source_update_invalid');
      if (!source.plugin) throw new Problem('data_source_plugin_unavailable');
      return persist({ ...source, config: validateConfig(source.plugin, value.config, value.enabled), enabled: value.enabled, error: null });
    });
  };
  const toggle = async (name: string, enabled: boolean) => {
    if (enabled === false) await cancel(name);
    return serial(async () => {
      if (typeof enabled !== 'boolean') throw new Problem('enabled_boolean_required');
      return persist({ ...found(name), enabled });
    });
  };
  const legacyRefresh = (name: string) => {
    const controller = new AbortController();
    const controllers = active.get(name) ?? new Set<AbortController>();
    controllers.add(controller); active.set(name, controllers);
    return serial(async () => {
      try {
        if (controller.signal.aborted) throw new Problem('data_source_refresh_cancelled', 503);
        const source = found(name);
        if (!source.enabled) throw new Problem('data_source_disabled', 409);
        if (!source.plugin) throw new Problem('data_source_plugin_unavailable');
        let loaded: DatasetConfig[];
        try {
          loaded = datasets(await source.plugin.load(validateConfig(source.plugin, source.config), { dataDir: config.dataDir, signal: controller.signal }));
          controller.signal.throwIfAborted();
          merged({ ...source, datasets: loaded });
        } catch {
          if (controller.signal.aborted) throw new Problem('data_source_refresh_cancelled', 503);
          await persist({ ...source, error: 'data_source_refresh_failed' });
          throw new Problem('data_source_refresh_failed', 400);
        }
        return persist({ ...source, datasets: loaded, lastLoadedAt: new Date().toISOString(), error: null });
      } finally {
        controllers.delete(controller);
        if (!controllers.size) active.delete(name);
      }
    });
  };
  async function moduleIdentity(source: Source): Promise<string> {
    const configured = config.extensions.find(item => item.kind === 'data' && item.name === source.name);
    const files = configured?.module ? [configured.module] : [new URL('./data-sources.ts', import.meta.url), ...(source.name === cryptohftdata.name ? [new URL('./providers/cryptohftdata.ts', import.meta.url)] : [])];
    const hashes = await Promise.all(files.map(async path => createHash('sha256').update(await readFile(path)).digest('hex')));
    return canonical({ name: source.name, version: source.version, hashes });
  }
  const managed = createDataSourceTasks(config.dataDir, {
    async capture(name) {
      return serial(async () => {
        const source = found(name);
        if (!source.enabled) throw new Problem('data_source_disabled', 409);
        if (!source.plugin) throw new Problem('data_source_plugin_unavailable');
        if (source.name !== cryptohftdata.name && source.plugin.recovery !== 'read-only') throw new Problem('data_source_recovery_capability_required', 409);
        const privateInput = { config: validateConfig(source.plugin, source.config), pluginIdentity: await moduleIdentity(source) };
        return { identity: createHash('sha256').update(canonical(privateInput)).digest('hex'), privateInput };
      });
    },
    async load(name, captured, signal) {
      const source = found(name);
      if (!source.enabled) throw new Problem('data_source_disabled', 409);
      if (!source.plugin || await moduleIdentity(source) !== captured.pluginIdentity) throw new Problem('data_source_plugin_version_unavailable', 409);
      const loaded = datasets(await source.plugin.load(object(captured.config), { dataDir: config.dataDir, signal }));
      signal.throwIfAborted();
      return { datasets: loaded } as unknown as JsonObject;
    },
    async publish(name, captured, result) {
      return serial(async () => {
        const source = found(name);
        // A later owner edit/disable cannot be replaced by a late refresh. The
        // original task still retains its loaded result as its own evidence.
        if (!source.enabled || canonical(source.config) !== canonical(captured.config!)) throw new Problem('data_source_refresh_configuration_changed', 409);
        const loaded = datasets(result.datasets);
        const { config: _privateConfig, ...published } = await persist({ ...source, datasets: loaded, lastLoadedAt: String(result.completedAt), error: null });
        return published as unknown as JsonObject;
      });
    },
    async failed(name, captured) {
      await serial(async () => {
        const source = found(name);
        if (source.enabled && canonical(source.config) === canonical(captured.config!)) await persist({ ...source, error: 'data_source_refresh_failed' });
      });
    },
  });
  const refresh = async (name: string, requestId?: string): Promise<DataSourceView> => {
    if (!connected) return legacyRefresh(name);
    const result = await managed.refresh(name, requestId ?? randomUUID());
    if (result.state === 'superseded') throw new Problem('data_source_refresh_configuration_changed', 409);
    // Owner-only settings are returned directly from private host state, never
    // copied into the official Task outcome or its model-readable documents.
    return { ...result, config: structuredClone(found(name).config) } as unknown as DataSourceView;
  };
  return { extension: managed.extension, connect(host: ResearchHost) { managed.connect(host); connected = true; }, view: () => [...sources.values()].map(viewOne), has: (name: string) => sources.has(name), save, toggle, refresh,
    extensions: (): ExtensionView[] => [...sources.values()].map(source => ({ name: source.name, version: source.version, enabled: source.enabled,
      kind: 'data', state: source.error ? 'error' : source.enabled ? 'loaded' : 'disabled', error: source.error })),
    async close() { closing = true; if (!connected) for (const controllers of active.values()) for (const controller of controllers) controller.abort(); await tail; } };
}
export function registerDataSources(app: FastifyInstance, sources: Awaited<ReturnType<typeof dataSources>>) {
  app.get(`${API}/data-sources`, sources.view);
  app.put<{ Params: { name: string } }>(`${API}/data-sources/:name`, request => sources.save(request.params.name, request.body));
  app.post<{ Params: { name: string } }>(`${API}/data-sources/:name/refresh`, request => {
    const body = request.body === undefined || request.body === null ? {} : object(request.body);
    if (Object.keys(body).some(key => key !== 'requestId') || body.requestId !== undefined && (typeof body.requestId !== 'string' || !body.requestId.trim())) throw new Problem('data_source_request_id_invalid');
    return sources.refresh(request.params.name, body.requestId as string | undefined);
  });
}
