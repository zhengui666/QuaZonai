import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import type { Extension, HarnessSettings, Storage } from '@earendil-works/pi-durable';
import type { Models } from '@earendil-works/pi-ai/models';
import type { DatasetView, JsonObject, ModelRef, ObservationMode, ThinkingLevel } from '../../../packages/contracts/src/index.ts';

import type { ThresholdPolicy } from '../../research/src/strategy.ts';
export type { ThresholdPolicy } from '../../research/src/strategy.ts';
export interface DatasetConfig extends DatasetView {
  data: JsonObject;
  policy?: JsonObject;
  strategy?: ThresholdPolicy;
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
  name: string; defaultModel: ModelRef | null; thinkingLevel: ThinkingLevel | null; jobBinary: string;
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
    host: '127.0.0.1', port: 8080, name: 'QuaZonai', defaultModel: null, thinkingLevel: null, secureCookies: false,
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
    for (const data of [dataset.data, dataset.data.sealed]) {
      if (data && typeof data === 'object' && !Array.isArray(data) && typeof data.catalog === 'string') data.catalog = resolve(base, data.catalog);
    }
  }
  return config;
}
