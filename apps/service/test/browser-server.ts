// Disposable browser-test process, never installed or started by the application.
import { resolve } from 'node:path';
import { fauxAssistantMessage, fauxToolCall, setup } from './support.ts';
const state = await setup(false, process.env.QZ_BROWSER_STATIC_DIR ?? resolve('../web/dist'));
state.options.config.datasets[0]!.strategy!.targetTtlNs = '300000000000';
state.faux.setResponses(Array.from({ length: 2 }, () => fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })));
if (process.env.QZ_BROWSER_DOWNSTREAM) state.options.config.connections.push({ id: 'browser-paper', name: 'Protocol-only paper connection', mode: 'paper', clientId: 'browser-client', accountId: 'browser-account', url: process.env.QZ_BROWSER_DOWNSTREAM });
await state.restart(); // createService captures configuration; apply only this test's connection/TTL.
const url = await state.service.app.listen({ host: '127.0.0.1', port: 0 });
const port = Number(new URL(url).port);
let closing = false;
async function close() { if (closing) return; closing = true; await state.cleanup(); process.exit(0); }
process.on('message', async (message: { type: string }) => {
  try {
    if (message.type === 'restart') { await state.restart(); await state.service.app.listen({ host: '127.0.0.1', port }); process.send?.({ type: 'restart' }); }
    else if (message.type === 'stats') process.send?.({ type: 'stats', calls: state.faux.state.callCount });
    else if (message.type === 'close') await close();
  } catch { process.send?.({ type: 'error' }); await close(); }
});
process.once('SIGTERM', () => { void close(); }); process.once('disconnect', () => { void close(); });
process.send?.({ type: 'ready', url });
