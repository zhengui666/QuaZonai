import { resolve } from 'node:path';
import { createNautilusWorker } from './server.ts';

const roots = JSON.parse(process.env.QZ_NAUTILUS_CATALOG_ROOTS ?? '[]') as unknown;
if (!Array.isArray(roots) || !roots.every(root => typeof root === 'string' && root.startsWith('/'))) throw new Error('QZ_NAUTILUS_CATALOG_ROOTS must be a JSON array of absolute paths');
const worker = await createNautilusWorker({
  binary: process.env.QZ_JOB_BINARY ?? '/opt/quazonai/bin/job',
  dataDir: resolve(process.env.QZ_NAUTILUS_DATA_DIR ?? 'data/nautilus'),
  catalogRoots: roots,
  token: process.env.QZ_NAUTILUS_TOKEN,
});
const port = Number(process.env.QZ_NAUTILUS_PORT ?? '8090');
if (!Number.isInteger(port) || port < 1 || port > 65_535) throw new Error('QZ_NAUTILUS_PORT is invalid');
worker.server.listen(port, process.env.QZ_NAUTILUS_HOST ?? '127.0.0.1');
let stopping = false;
for (const signal of ['SIGTERM', 'SIGINT']) process.on(signal, () => {
  if (stopping) return;
  stopping = true;
  void worker.close().catch(error => { console.error(error instanceof Error ? error.message : 'nautilus_shutdown_failed'); process.exitCode = 1; });
});
