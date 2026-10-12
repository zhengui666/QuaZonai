import { loadConfig } from './config.ts';
import { createService } from './service.ts';

const config = await loadConfig(process.env.QZ_CONFIG ?? 'service.json');
// Select one configured kernel before the sole research host loads its factor extensions.
process.env.QZ_JOB_BINARY = config.jobBinary;
const service = await createService({ config });
try { await service.app.listen({ host: config.host, port: config.port }); }
catch (error) { await service.close(); throw error; }
let closing = false;
for (const signal of ['SIGTERM', 'SIGINT'] as const) process.on(signal, () => {
  if (closing) return;
  closing = true;
  service.close().catch(() => { process.exitCode = 1; });
});
