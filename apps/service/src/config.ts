import { randomUUID } from 'node:crypto';
import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import type { Extension, HarnessSettings, Storage } from '@earendil-works/pi-durable';
import type { Models } from '@earendil-works/pi-ai/models';
import type { DatasetView, JsonObject, ModelRef, NautilusConnection, ServiceTier, ThinkingLevel } from '../../../packages/contracts/src/index.ts';
import type { NativeTarget, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { setRemoteNativeCredential } from '../../research/src/native-execution.ts';

import type { StrategyPolicy } from '../../research/src/strategy.ts';
import { nativeResearchInput } from '../../../packages/contracts/src/tick-input.ts';
export type { StrategyPolicy, ThresholdPolicy } from '../../research/src/strategy.ts';
export interface DatasetConfig extends DatasetView {
  data: JsonObject;
  policy?: JsonObject;
  strategy?: StrategyPolicy;
  /** Trusted source producer atomically publishes immutable catalog revisions here. */
  revisionManifest?: string;
}
export type ConnectionConfig = NautilusConnection & { credentialRef?: string };
type BacktestConfig = RemoteNativeTarget & { token?: string };
export interface ExtensionConfig { name: string; module: string; enabled: boolean; version?: string; kind?: string; config?: JsonObject }
export interface ServiceConfig {
  host: string; port: number; dataDir: string;
  research: { database: string; modules?: string[]; providerModules?: string[]; settings?: HarnessSettings };
  /** Runtime source snapshots for existing mandates; excluded from new admissions. */
  retainedDatasets?: DatasetConfig[];
  datasets: DatasetConfig[]; connections: ConnectionConfig[]; extensions: ExtensionConfig[];
  nautilus?: { backtest?: BacktestConfig };
  name: string; defaultModel: ModelRef | null; thinkingLevel: ThinkingLevel | null; serviceTier: ServiceTier | null; jobBinary: string;
  passwordHash?: string; secureCookies: boolean; staticDir?: string;
  archiveDatabase?: string;
}
export interface ServiceOptions {
  config: ServiceConfig;
  models?: Models; storage?: Storage; extensions?: readonly Extension[];
}
export function configDefaults(value: Partial<ServiceConfig> = {}, base = process.cwd()): ServiceConfig {
  const dataDir = resolve(base, value.dataDir ?? 'data');
  const target = value.nautilus?.backtest ? remoteNativeBinding(value.nautilus.backtest) : undefined;
  return {
    host: '127.0.0.1', port: 8080, name: 'QuaZonai', defaultModel: null, thinkingLevel: null, serviceTier: null, secureCookies: false,
    datasets: [], connections: [], extensions: [], ...value, dataDir,
    ...(target ? { datasets: (value.datasets ?? []).map(dataset => ({ ...dataset, data: bindScienceData(dataset.data, target) })) } : {}),
    // Only explicit in-process test/worker callers may supply a local executable.
    // File-based application configuration never enables embedded execution.
    jobBinary: value.jobBinary ? resolve(base, value.jobBinary) : '',
    research: { ...value.research, database: resolve(base, value.research?.database ?? `${dataDir}/research.sqlite`) },
  };
}
export async function loadConfig(path: string): Promise<ServiceConfig> {
  const absolute = resolve(path);
  const raw = JSON.parse(await readFile(absolute, 'utf8'));
  if (raw.jobBinary !== undefined) throw new Error('embedded_nautilus_removed_configure_remote_backtest');
  const config = configDefaults(raw, dirname(absolute));
  validateNautilusConfiguration(config);
  const base = dirname(absolute);
  config.research.modules = config.research.modules?.map((path) => resolve(base, path));
  config.research.providerModules = config.research.providerModules?.map((path) => resolve(base, path));
  config.extensions = config.extensions.map((extension) => ({ ...extension, module: resolve(base, extension.module) }));
  if (config.staticDir) config.staticDir = resolve(base, config.staticDir);
  // Native catalogs are trusted immutable server-side inputs, not browser paths.
  for (const dataset of config.datasets) resolveDatasetPaths(dataset, base);
  return config;
}

export function resolveDatasetPaths(dataset: DatasetConfig, base: string): DatasetConfig {
  validateDatasetInput(dataset);
  if (dataset.revisionManifest) dataset.revisionManifest = resolve(base, dataset.revisionManifest);
  for (const data of [dataset.data, dataset.data.sealed]) {
    if (data && typeof data === 'object' && !Array.isArray(data)) {
      for (const key of ['catalog', 'catalog_partitions']) if (typeof data[key] === 'string') data[key] = resolve(base, data[key]);
    }
  }
  return dataset;
}

export function validateDatasetInput(dataset: DatasetConfig): void {
  const tagged = nativeResearchInput(dataset.data);
  if (dataset.nativeModality !== undefined && !['BAR', 'TRADE_TICK'].includes(dataset.nativeModality)) throw new Error('dataset_modality_invalid');
  if (tagged && dataset.nativeModality !== undefined && tagged.modality !== dataset.nativeModality) throw new Error('dataset_modality_mismatch');
  if (tagged?.modality === 'TRADE_TICK' && dataset.strategy !== undefined) throw new Error('tick_portfolio_unsupported');
  const prepared = dataset.preparation;
  if (prepared !== undefined && (!prepared || typeof prepared.archiveLoaded !== 'boolean' || typeof prepared.nativePrepared !== 'boolean'
    || !['UNVERIFIED', 'NOT_ASSESSED'].includes(prepared.scientificQualification))) throw new Error('dataset_preparation_invalid');
}

/** Backtest/PM computations and PAPER/LIVE transports have separate bindings. */
export function scienceTarget(config: { nautilus?: ServiceConfig['nautilus']; jobBinary: NativeTarget }): NativeTarget {
  return config.nautilus?.backtest ? remoteNativeBinding(config.nautilus.backtest) : config.jobBinary;
}

/** Register the secret outside research facts and pass only its private reference onward. */
export function remoteNativeBinding(target: BacktestConfig, reference?: string): RemoteNativeTarget {
  if (target.credentialRef) {
    if (target.token) setRemoteNativeCredential(target.credentialRef, target.token);
    return { id: target.id, url: target.url, credentialRef: target.credentialRef };
  }
  if (Object.hasOwn(target, 'token')) {
    if (typeof target.token !== 'string' || !target.token) return { id: target.id, url: target.url };
    const credentialRef = reference ?? randomUUID(); setRemoteNativeCredential(credentialRef, target.token);
    return { id: target.id, url: target.url, credentialRef };
  }
  return { id: target.id, url: target.url, ...(target.tokenEnv ? { tokenEnv: target.tokenEnv } : {}) };
}

function remoteUrl(value: unknown) {
  if (typeof value !== 'string') throw new Error('nautilus_url_required');
  let url: URL;
  try { url = new URL(value); } catch { throw new Error('nautilus_url_invalid'); }
  if (!['http:', 'https:'].includes(url.protocol) || url.username || url.password || url.search || url.hash) throw new Error('nautilus_url_invalid');
}
function tokenEnvironment(value: unknown) {
  if (value !== undefined && (typeof value !== 'string' || !/^[A-Za-z_][A-Za-z0-9_]*$/.test(value))) throw new Error('nautilus_token_env_invalid');
}
function instanceToken(value: unknown) {
  if (value !== undefined && typeof value !== 'string') throw new Error('nautilus_token_invalid');
}
export function validateNautilusConfiguration(config: Pick<ServiceConfig, 'nautilus' | 'connections'>) {
  if (config.nautilus !== undefined) {
    if (!config.nautilus || typeof config.nautilus !== 'object' || Array.isArray(config.nautilus)
      || Object.keys(config.nautilus).some(key => key !== 'backtest')) throw new Error('nautilus_configuration_invalid');
    const backtest = config.nautilus.backtest;
    if (backtest !== undefined) {
      if (!backtest || typeof backtest !== 'object' || Array.isArray(backtest) || typeof backtest.id !== 'string' || !backtest.id.trim()
        || Object.keys(backtest).some(key => !['id', 'url', 'token', 'tokenEnv'].includes(key))) throw new Error('nautilus_backtest_instance_invalid');
      remoteUrl(backtest.url); instanceToken(backtest.token); tokenEnvironment(backtest.tokenEnv);
    }
  }
  const ids = new Set<string>();
  if (!Array.isArray(config.connections)) throw new Error('nautilus_connections_invalid');
  for (const connection of config.connections) {
    if (!connection || typeof connection !== 'object' || !['paper', 'live'].includes(connection.mode)
      || ['id', 'name', 'clientId', 'accountId'].some(key => typeof connection[key as keyof ConnectionConfig] !== 'string' || !String(connection[key as keyof ConnectionConfig]).trim())) throw new Error('nautilus_connection_invalid');
    if (ids.has(connection.id)) throw new Error('nautilus_connection_id_conflict');
    ids.add(connection.id); remoteUrl(connection.url); instanceToken(connection.token); tokenEnvironment(connection.tokenEnv);
  }
}

/** Freeze the transport identity with newly admitted data, never rewrite stored requests. */
export function bindScienceData(data: JsonObject, target: RemoteNativeTarget): JsonObject {
  const bound: JsonObject = { ...data, nautilusTarget: { ...target } };
  if (data.sealed && typeof data.sealed === 'object' && !Array.isArray(data.sealed)) {
    bound.sealed = { ...data.sealed, nautilusTarget: { ...target } };
  }
  return bound;
}

export function scienceDataWithoutBinding(data: JsonObject): JsonObject {
  // Stored Durable documents can be proxies and cannot be structuredClone'd.
  const copy = { ...data }; delete copy.nautilusTarget;
  if (copy.sealed && typeof copy.sealed === 'object' && !Array.isArray(copy.sealed)) {
    copy.sealed = { ...copy.sealed }; delete copy.sealed.nautilusTarget;
  }
  return copy;
}

/** Existing mandates continue observing their source after it leaves the admission list. */
export function existingDataset(config: ServiceConfig, id: string): DatasetConfig | undefined {
  return config.datasets.find(dataset => dataset.id === id) ?? config.retainedDatasets?.find(dataset => dataset.id === id);
}
