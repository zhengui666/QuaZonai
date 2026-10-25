#!/usr/bin/env node
// Creates only absent configuration. Never opens or initializes a research DB.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export function configure(root, origin) {
  const data = join(resolve(root), 'data');
  mkdirSync(join(data, 'scratch'), { recursive: true, mode: 0o700 });
  const servicePath = join(data, 'service.json');
  if (existsSync(servicePath)) {
    const config = JSON.parse(readFileSync(servicePath, 'utf8'));
    if (!config.research?.database) throw new Error(`Research database path missing in ${servicePath}; original state was not changed.`);
    return servicePath;
  }
  const researchDir = join(data, 'research');
  mkdirSync(join(data, 'home'), { recursive: true, mode: 0o700 });
  mkdirSync(researchDir, { recursive: true, mode: 0o700 });
  const oldPath = [join(researchDir, 'config.json'), join(data, 'research.json')].find(existsSync);
  const old = oldPath ? JSON.parse(readFileSync(oldPath, 'utf8')) : null;
  if (old && typeof old.database !== 'string') throw new Error(`Research database path missing in ${oldPath}; refusing a fresh database.`);
  const defaults = ['research.ts', 'factor-wasm.ts', 'heldout-wasm.ts'];
  const shipped = [...defaults, 'factor-volume.ts', 'validation-compare.ts'];
  const modulePath = (value) => {
    const absolute = resolve(oldPath ? dirname(oldPath) : researchDir, value);
    // Only exact former shipped modules move. Private extensions keep their paths.
    const name = shipped.find((name) => absolute === `/opt/quazonai/research/plugins/${name}`);
    return name ? `/opt/quazonai/apps/research/plugins/${name}` : absolute;
  };
  const database = old ? resolve(dirname(oldPath), old.database) : join(researchDir, 'research.sqlite');
  if (old && !existsSync(database)) throw new Error(`Original database unavailable: ${database}; refusing a fresh database.`);
  const research = { ...(old ?? {}), database,
    modules: old?.modules ? old.modules.map(modulePath) : defaults.map((name) => `/opt/quazonai/apps/research/plugins/${name}`) };
  if (research.providerModules) research.providerModules = research.providerModules.map(modulePath);
  const config = { host: '0.0.0.0', port: 8080, dataDir: data, research,
    nautilus: {}, staticDir: '/opt/quazonai/web', publicOrigin: origin,
    datasets: [], connections: [], extensions: [], secureCookies: origin.startsWith('https:') };
  writeFileSync(servicePath, JSON.stringify(config, null, 2) + '\n', { flag: 'wx', mode: 0o600 });
  const provider = join(researchDir, 'provider.env');
  if (!existsSync(provider)) writeFileSync(provider, '# Official pi-ai provider environment; keep private.\n', { flag: 'wx', mode: 0o600 });
  return servicePath;
}
if (process.argv[1] && resolve(process.argv[1]) === fileURLToPath(import.meta.url)) {
  if (!process.argv[2] || !process.argv[3]) throw new Error('Use configure ROOT PUBLIC_ORIGIN');
  configure(process.argv[2], process.argv[3]);
}
