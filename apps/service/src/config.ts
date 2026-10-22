import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import type { Extension, HarnessSettings, Storage } from '@earendil-works/pi-durable';
import type { Models } from '@earendil-works/pi-ai/models';
import type { DatasetView, JsonObject, ModelRef, ObservationMode, ServiceTier, ThinkingLevel } from '../../../packages/contracts/src/index.ts';

import type { StrategyPolicy } from '../../research/src/strategy.ts';
export type { StrategyPolicy, ThresholdPolicy } from '../../research/src/strategy.ts';
export interface DatasetConfig extends DatasetView {
  data: JsonObject;
  policy?: JsonObject;
  strategy?: StrategyPolicy;
  /** Trusted source producer atomically publishes immutable catalog revisions here. */
  revisionManifest?: string;
}
export interface ConnectionConfig {
  id: string; name: string; mode: ObservationMode;
  url: string; clientId: string; accountId: string;
  tokenEnv?: string;
}
export interface ExtensionConfig { name: string; module: string; enabled: boolean; version?: string; kind?: string }
export interface ServiceConfig {
  host: string; port: number; dataDir: string;
  research: { database: string; modules?: string[]; providerModules?: string[]; settings?: HarnessSettings };
  datasets: DatasetConfig[]; connections: ConnectionConfig[]; extensions: ExtensionConfig[];
  name: string; defaultModel: ModelRef | null; thinkingLevel: ThinkingLevel | null; serviceTier: ServiceTier | null; jobBinary: string;
  passwordHash?: string; secureCookies: boolean; publicOrigin?: string; staticDir?: string;
  archiveDatabase?: string;
}
export interface ServiceOptions {
  config: ServiceConfig;
  models?: Models; storage?: Storage; extensions?: readonly Extension[];
}
export function configDefaults(value: Partial<ServiceConfig> = {}, base = process.cwd()): ServiceConfig {
  const dataDir = resolve(base, value.dataDir ?? 'data');
  return {
    host: '127.0.0.1', port: 8080, name: 'QuaZonai', defaultModel: null, thinkingLevel: null, serviceTier: null, secureCookies: false,
    datasets: [], connections: [], extensions: [], ...value, dataDir,
    jobBinary: resolve(base, value.jobBinary ?? process.env.QZ_JOB_BINARY ?? 'job'),
    research: { ...value.research, database: resolve(base, value.research?.database ?? `${dataDir}/research.sqlite`) },
  };
}
export async function loadConfig(path: string): Promise<ServiceConfig> {
  const absolute = resolve(path);
  const config = configDefaults(JSON.parse(await readFile(absolute, 'utf8')), dirname(absolute));
  const base = dirname(absolute);
  config.research.modules = config.research.modules?.map((path) => resolve(base, path));
  config.research.providerModules = config.research.providerModules?.map((path) => resolve(base, path));
  config.extensions = config.extensions.map((extension) => ({ ...extension, module: resolve(base, extension.module) }));
  if (config.staticDir) config.staticDir = resolve(base, config.staticDir);
  // Native catalogs are trusted immutable server-side inputs, not browser paths.
  for (const dataset of config.datasets) {
    if (dataset.revisionManifest) dataset.revisionManifest = resolve(base, dataset.revisionManifest);
    for (const data of [dataset.data, dataset.data.sealed]) {
      if (data && typeof data === 'object' && !Array.isArray(data)) {
        for (const key of ['catalog', 'catalog_partitions']) if (typeof data[key] === 'string') data[key] = resolve(base, data[key]);
      }
    }
  }
  return config;
}
