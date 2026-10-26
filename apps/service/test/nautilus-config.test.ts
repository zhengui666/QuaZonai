import assert from 'node:assert/strict';
import { test } from 'node:test';
import { once } from 'node:events';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { configDefaults, loadConfig, scienceTarget, validateNautilusConfiguration } from '../src/config.ts';

test('file configuration separates remote backtest, paper and live and freezes research binding', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-remote-config-'));
  try {
    const path = join(directory, 'service.json');
    const backtest = { id: 'research', url: 'https://research.example/nautilus', tokenEnv: 'RESEARCH_TOKEN' };
    const connections = ['paper', 'live'].map(mode => ({ id: mode, name: mode, mode, url: `https://${mode}.example`, clientId: 'qz', accountId: mode, tokenEnv: `${mode.toUpperCase()}_TOKEN` }));
    await writeFile(path, JSON.stringify({ nautilus: { backtest }, connections, datasets: [{ id: 'data', data: { catalog: './catalog', sealed: { catalog: './heldout' } } }] }));
    const config = await loadConfig(path);
    assert.deepEqual(scienceTarget(config), backtest);
    assert.equal(config.jobBinary, '');
    assert.deepEqual(config.connections, connections);
    assert.deepEqual(config.datasets[0]!.data.nautilusTarget, backtest);
    assert.deepEqual((config.datasets[0]!.data.sealed as any).nautilusTarget, backtest);
    assert.equal(config.datasets[0]!.data.catalog, join(directory, 'catalog'));
    config.nautilus!.backtest!.url = 'https://replacement.example';
    assert.equal((config.datasets[0]!.data.nautilusTarget as any).url, backtest.url, 'admitted binding is a snapshot');
    await writeFile(path, JSON.stringify({ jobBinary: '/old/embedded/job' }));
    await assert.rejects(loadConfig(path), /embedded_nautilus_removed_configure_remote_backtest/);
    await writeFile(path, '{}');
    assert.equal(scienceTarget(await loadConfig(path)), '', 'unconfigured app can start without an implicit local engine');
  } finally { await rm(directory, { recursive: true, force: true }); }
});

test('invalid or ambiguous remote bindings fail instead of selecting another instance', () => {
  const valid = { id: 'one', url: 'https://worker.example', tokenEnv: 'QZ_WORKER_TOKEN' };
  for (const url of ['file:///tmp/job', 'https://user:password@worker.example', 'https://worker.example/?token=secret', 'invalid']) {
    assert.throws(() => validateNautilusConfiguration(configDefaults({ nautilus: { backtest: { ...valid, url } } })), /nautilus_url_invalid/);
  }
  assert.throws(() => validateNautilusConfiguration(configDefaults({ nautilus: { backtest: { ...valid, tokenEnv: 'literal-token!' } } })), /nautilus_token_env_invalid/);
  const connection = { id: 'same', name: 'instance', mode: 'paper' as const, url: 'http://worker:8090', clientId: 'qz', accountId: 'one' };
  assert.throws(() => validateNautilusConfiguration(configDefaults({ connections: [connection, { ...connection, mode: 'live' }] })), /nautilus_connection_id_conflict/);
});


test('new portfolio accepts completed research when only its configured transport changed', async () => {
  const { createNautilusWorker } = await import('../../nautilus/src/server.ts');
  const { setup, studied, context } = await import('./support.ts');
  const state = await setup();
  const worker = await createNautilusWorker({ binary: state.options.config.jobBinary, dataDir: join(state.directory, 'remote'), catalogRoots: [state.directory] });
  worker.server.listen(0, '127.0.0.1'); await once(worker.server, 'listening');
  try {
    const { receipt } = await studied(state, 'completed-before-remote');
    const target = { id: 'new-backtest', url: `http://127.0.0.1:${(worker.server.address() as import('node:net').AddressInfo).port}` };
    const saved = await state.service.app.inject({ method: 'PUT', url: '/api/v3/settings/instances', headers: state.headers,
      payload: { expectedRevision: 0, backtest: target, connections: [] } });
    assert.equal(saved.statusCode, 200, saved.body);
    const dataset = state.service.config.datasets[0]!;
    const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: {
      requestId: 'new-remote-backtest', name: 'Existing evidence via selected remote worker', researchIds: [receipt.conversationId], datasetId: dataset.id, capital: '1000' } });
    assert.equal(response.statusCode, 202, response.body);
    await state.service.research.harness.waitForTask(response.json().taskId, context);
    const result = await state.service.work.getPortfolio(response.json().id);
    assert.equal(result.view.status, 'COMPUTED', result.view.error ?? '');
  } finally { await state.cleanup(); await worker.close(); }
});
