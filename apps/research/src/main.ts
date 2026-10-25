import { readFile } from 'node:fs/promises';
import { dirname, resolve } from 'node:path';
import { openResearch } from './host.ts';
import type { HarnessOptions } from '@earendil-works/pi-durable';
import type { ModelThinkingLevel } from '@earendil-works/pi-ai';
import { researchServer } from './http.ts';
import { record } from './input.ts';

// Every application entry point is remote-only. Direct local execution is
// reserved for the independent worker and explicitly configured science tests.
process.env.QZ_REQUIRE_REMOTE_NAUTILUS = '1';
delete process.env.QZ_JOB_BINARY;

const path = resolve(process.env.QZ_RESEARCH_CONFIG ?? 'research.json');
const config = record(JSON.parse(await readFile(path, 'utf8')), 'research_config');
if (typeof config.database !== 'string') throw new Error('database_required');
function modules(key: string): string[] {
  const values = config[key] ?? [];
  if (!Array.isArray(values) || !values.every((value) => typeof value === 'string')) throw new Error(`${key}_required`);
  return values.map((value) => resolve(dirname(path), value));
}
const port = config.port ?? 8092;
if (!Number.isInteger(port) || (port as number) < 1 || (port as number) > 65535) throw new Error('invalid_research_port');
const host = await openResearch({ database: resolve(dirname(path), config.database), modules: modules('modules'), providerModules: modules('providerModules'), settings: config.settings as HarnessOptions['settings'], thinkingLevel: config.thinkingLevel as ModelThinkingLevel | undefined });
const server = researchServer(host);
server.listen(port as number, '127.0.0.1');
let closing = false;
async function close() {
  if (closing) return;
  closing = true;
  const stopped = new Promise<void>((done, reject) => server.close((error) => error ? reject(error) : done()));
  await host.close();
  await stopped;
}
process.once('SIGTERM', () => { void close(); });
process.once('SIGINT', () => { void close(); });
