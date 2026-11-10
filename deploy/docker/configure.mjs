#!/usr/bin/env node
// Creates only absent configuration. Never opens or initializes a research DB.
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
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
  if ([join(researchDir, 'config.json'), join(data, 'research.json')].some(existsSync)) {
    throw new Error('Prior research configuration is unsupported; use a fresh installation.');
  }
  const research = { database: join(researchDir, 'research.sqlite'),
    modules: ['research.ts', 'factor-wasm.ts', 'heldout-wasm.ts'].map(name => `/opt/quazonai/apps/research/plugins/${name}`) };
  const config = { host: '0.0.0.0', port: 8080, dataDir: data, research,
    nautilus: {}, staticDir: '/opt/quazonai/web',
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
