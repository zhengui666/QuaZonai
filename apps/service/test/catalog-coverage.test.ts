import assert from 'node:assert/strict';
import test from 'node:test';
import fs from 'node:fs';
import { syncBuiltinESMExports } from 'node:module';
import { createServer } from 'node:http';
import type { AddressInfo } from 'node:net';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';
import { setup } from './support.ts';

// Controlled transport identity test; these responses are not market coverage evidence.
async function backend(id: string) {
  const calls: NativeExecutionRequest[] = []; let fail = false;
  const server = createServer(async (request, response) => {
    response.setHeader('content-type', 'application/json');
    if (request.url === '/v1/capabilities') {
      response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: ['catalog-coverage'] })); return;
    }
    let body = ''; for await (const chunk of request) body += chunk;
    const original = JSON.parse(body) as NativeExecutionRequest; calls.push(original);
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: fail ? 'FAILED' : 'COMPLETED',
      ...(fail ? { error: 'CONTROLLED_DATASET_BACKEND_FAILURE' } : { result: { backend: id, request: original.input } }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  return { target: { id, url: `http://127.0.0.1:${(server.address() as AddressInfo).port}` }, calls, fail() { fail = true; },
    async close() { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); } };
}

for (const failing of [false, true]) test(`coverage freezes the original dataset backend across an asynchronous instance switch (failure=${failing})`, { timeout: 20000 }, async () => {
  const state = await setup(); const dataset = await backend('dataset-A'); const other = await backend('global-B');
  const originalMkdtemp = fs.promises.mkdtemp;
  let release!: () => void; let entered!: () => void;
  const barrier = new Promise<void>(resolve => { release = resolve; });
  const waiting = new Promise<void>(resolve => { entered = resolve; });
  try {
    state.options.config.nautilus = { backtest: dataset.target };
    await state.restart();
    if (failing) dataset.fail();
    const catalog = state.options.config.datasets[0]!.data.catalog;
    // Pause the actual existing I/O boundary, not the target resolver or backend.
    // The native route has already read its dataset/catalog but has not dispatched.
    fs.promises.mkdtemp = (async (...args: Parameters<typeof originalMkdtemp>) => {
      if (String(args[0]).includes('catalog-coverage-')) { entered(); await barrier; }
      return originalMkdtemp(...args);
    }) as typeof originalMkdtemp;
    syncBuiltinESMExports();
    const payload = { selectors: [{ data_type: 'TradeTick', identifier: 'BTC/USDT.BINANCE' }], start_ns: '9007199254740993', end_ns: '9007199254740995' };
    const pending = state.service.app.inject({ method: 'POST', url: '/api/v3/datasets/controlled/coverage', headers: state.headers, payload });
    await waiting;
    const current = (await state.service.app.inject({ method: 'GET', url: '/api/v3/settings/instances', headers: state.headers })).json();
    const switched = await state.service.app.inject({ method: 'PUT', url: '/api/v3/settings/instances', headers: state.headers, payload: { expectedRevision: current.revision, backtest: other.target, connections: [] } });
    assert.equal(switched.statusCode, 200, switched.body);
    release();
    const response = await pending;
    if (failing) assert.ok(response.statusCode >= 400, response.body);
    else { assert.equal(response.statusCode, 200, response.body); assert.equal(response.json().backend, 'dataset-A'); assert.deepEqual(response.json().request, payload); }
    assert.equal(dataset.calls.length, 1, 'the in-flight query remains with its captured dataset');
    assert.equal(other.calls.length, 0, 'neither switching nor failure redirects an existing query');
    assert.equal(dataset.calls[0]!.command, 'catalog-coverage'); assert.ok(dataset.calls[0]!.args.includes(String(catalog)));
  } finally {
    release(); fs.promises.mkdtemp = originalMkdtemp; syncBuiltinESMExports();
    await state.cleanup(); await dataset.close(); await other.close();
  }
});
