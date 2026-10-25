import { loadConfig } from './config.ts';
import { createService } from './service.ts';

const config = await loadConfig(process.env.QZ_CONFIG ?? 'service.json');
// Application processes never run an embedded engine, including old persisted tasks.
delete process.env.QZ_JOB_BINARY;
delete process.env.QZ_NAUTILUS_BACKTEST;
process.env.QZ_REQUIRE_REMOTE_NAUTILUS = '1';
if (config.nautilus?.backtest) process.env.QZ_NAUTILUS_BACKTEST = JSON.stringify(config.nautilus.backtest);
const service = await createService({ config });
try { await service.app.listen({ host: config.host, port: config.port }); }
catch (error) { await service.close(); throw error; }
let closing = false;
for (const signal of ['SIGTERM', 'SIGINT'] as const) process.on(signal, () => {
  if (closing) return;
  closing = true;
  service.close().catch(() => { process.exitCode = 1; });
});
