import { randomUUID } from 'node:crypto';
import { readFile, rename, rm, writeFile } from 'node:fs/promises';
import { dirname, join, resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import type { FastifyInstance } from 'fastify';
import { API } from '../../../packages/contracts/src/index.ts';
import type { ExtensionView, JsonObject } from '../../../packages/contracts/src/index.ts';
import type { DataSourceField, DataSourceView } from '../../../packages/contracts/src/data-sources.ts';
import { bindScienceData, resolveDatasetPaths, scienceDataWithoutBinding } from './config.ts';
import type { DatasetConfig, ServiceConfig } from './config.ts';
import { object, Problem } from './facts.ts';
import { validateNumbers } from '../../research/src/input.ts';
import hyperliquid from './providers/hyperliquid.ts';

export interface DataSourcePlugin {
  name: string; version?: string; fields: DataSourceField[];
  load(config: JsonObject, context: { dataDir: string; signal?: AbortSignal }): Promise<DatasetConfig[]>;
}
interface SavedSource {
  config: JsonObject; enabled: boolean; datasets: DatasetConfig[]; retainedDatasets?: DatasetConfig[]; lastLoadedAt: string | null; error: string | null;
}
interface Source extends SavedSource { name: string; version: string | null; fields: DataSourceField[]; plugin?: DataSourcePlugin }
const local: DataSourcePlugin = {
  name: 'qz.data.local-config', version: '1',
  fields: [{ key: 'path', label: '数据集配置文件', type: 'text', required: true, description: '现有数据工具导出的 DatasetConfig JSON 文件；相对数据目录解析。' }],
  async load(config, context) {
    const file = resolve(context.dataDir, String(config.path));
    const parsed: unknown = JSON.parse(await readFile(file, 'utf8'));
    return datasets(Array.isArray(parsed) ? parsed : [parsed]).map(value => resolveDatasetPaths(value, dirname(file)));
  },
};
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
  }
  return structuredClone(config);
}
function validatePlugin(value: unknown, name: string): DataSourcePlugin {
  const plugin = value as DataSourcePlugin;
  if (!plugin || plugin.name !== name || typeof plugin.load !== 'function' || !Array.isArray(plugin.fields)
    || plugin.version !== undefined && typeof plugin.version !== 'string'
    || plugin.fields.some(field => !field || typeof field.key !== 'string' || !field.key || typeof field.label !== 'string'
      || !['text', 'password', 'number', 'boolean'].includes(field.type))
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
  const configured = [{ name: 'qz.data.config', version: '1', enabled: true, config: {} as JsonObject, module: '', kind: 'data' }, ...[local, hyperliquid].map(plugin => ({ name: plugin.name, version: plugin.version, enabled: false, config: {} as JsonObject, module: '', kind: 'data' })), ...config.extensions.filter(item => item.kind === 'data')];
  for (const entry of configured) {
    if (sources.has(entry.name)) throw new Problem('extension_name_conflict');
    const saved = stored[entry.name];
    const source: Source = { name: entry.name, version: entry.version ?? null, fields: [], enabled: saved?.enabled ?? entry.enabled,
      config: structuredClone(saved?.config ?? entry.config ?? {}), datasets: entry.name === 'qz.data.config' ? base : saved ? datasets(saved.datasets) : [], retainedDatasets: saved ? datasets(saved.retainedDatasets ?? saved.datasets) : [], lastLoadedAt: saved?.lastLoadedAt ?? null, error: saved?.error ?? null };
    try {
      source.plugin = validatePlugin(entry.name === 'qz.data.config' ? { name: entry.name, version: '1', fields: [], async load() { return base; } } : entry.name === local.name ? local : entry.name === hyperliquid.name ? hyperliquid : (await import(pathToFileURL(entry.module).href)).default, entry.name);
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
  const active = new Map<string, AbortController>();
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
    const temporary = `${file}.${randomUUID()}.tmp`;
    try { await writeFile(temporary, JSON.stringify(saved, null, 2) + '\n', { flag: 'wx', mode: 0o600 }); await rename(temporary, file); }
    catch { await rm(temporary, { force: true }).catch(() => undefined); throw new Problem('data_source_save_failed', 503); }
    stored = saved; sources.set(next.name, next); apply();
    return viewOne(next);
  }
  const save = async (name: string, body: unknown) => {
    const update = object(body, 'data_source');
    if (update.enabled === false && Object.keys(update).every(key => ['config', 'enabled'].includes(key))) {
      const source = found(name);
      if (source.plugin) validateConfig(source.plugin, update.config, false);
      active.get(name)?.abort();
    }
    return serial(async () => {
      const source = found(name); const value = object(body, 'data_source');
      if (Object.keys(value).some(key => !['config', 'enabled'].includes(key)) || typeof value.enabled !== 'boolean') throw new Problem('data_source_update_invalid');
      if (!source.plugin) throw new Problem('data_source_plugin_unavailable');
      return persist({ ...source, config: validateConfig(source.plugin, value.config, value.enabled), enabled: value.enabled, error: null });
    });
  };
  const toggle = (name: string, enabled: boolean) => {
    if (enabled === false) active.get(name)?.abort();
    return serial(async () => {
      if (typeof enabled !== 'boolean') throw new Problem('enabled_boolean_required');
      return persist({ ...found(name), enabled });
    });
  };
  const refresh = (name: string) => serial(async () => {
    const source = found(name);
    if (!source.enabled) throw new Problem('data_source_disabled', 409);
    if (!source.plugin) throw new Problem('data_source_plugin_unavailable');
    let loaded: DatasetConfig[];
    const controller = new AbortController(); active.set(name, controller);
    try {
      loaded = datasets(await source.plugin.load(validateConfig(source.plugin, source.config), { dataDir: config.dataDir, signal: controller.signal }));
      controller.signal.throwIfAborted();
      merged({ ...source, datasets: loaded });
    } catch {
      await persist({ ...source, error: 'data_source_refresh_failed' });
      throw new Problem('data_source_refresh_failed', 400);
    } finally {
      active.delete(name);
    }
    return persist({ ...source, datasets: loaded, lastLoadedAt: new Date().toISOString(), error: null });
  });
  return { view: () => [...sources.values()].map(viewOne), has: (name: string) => sources.has(name), save, toggle, refresh,
    extensions: (): ExtensionView[] => [...sources.values()].map(source => ({ name: source.name, version: source.version, enabled: source.enabled,
      kind: 'data', state: source.error ? 'error' : source.enabled ? 'loaded' : 'disabled', error: source.error })),
    async close() { closing = true; for (const controller of active.values()) controller.abort(); await tail; } };
}
export function registerDataSources(app: FastifyInstance, sources: Awaited<ReturnType<typeof dataSources>>) {
  app.get(`${API}/data-sources`, sources.view);
  app.put<{ Params: { name: string } }>(`${API}/data-sources/:name`, request => sources.save(request.params.name, request.body));
  app.post<{ Params: { name: string } }>(`${API}/data-sources/:name/refresh`, request => sources.refresh(request.params.name));
}
