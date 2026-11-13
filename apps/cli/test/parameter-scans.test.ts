import assert from 'node:assert/strict';
import { once } from 'node:events';
import { createServer } from 'node:http';
import { mkdtemp, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { test } from 'node:test';
import { run } from '../src/commands.ts';
import { saveProfile } from '../src/profile.ts';
import { schemas } from '../src/schema.ts';
import type { ParameterScanStatus } from '../../../packages/contracts/src/parameter-scans.ts';

test('retired scan construction commands cannot send requests',async()=>{
 for(const command of ['create','preview','options','surface'])await assert.rejects(run(['parameter-scan',command,'old'],()=>{}),/unknown_parameter_scan_command/);
});

test('parameter scan observers keep remote uncertainty unsettled and detach without stopping work', async () => {
  let status: ParameterScanStatus = 'RUNNING'; let unknown = '0';
  const requests: string[] = [];
  const server = createServer((request, response) => {
    requests.push(`${request.method} ${request.url}`);
    response.writeHead(200, { 'content-type': 'text/event-stream' });
    response.write(`event: parameter-scan\ndata: ${JSON.stringify({ status, counts: { unknown } })}\n\n`);
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const directory = await mkdtemp(join(tmpdir(), 'qz-parameter-scan-watch-'));
  const previous = process.env.XDG_CONFIG_HOME; process.env.XDG_CONFIG_HOME = directory;
  try {
    const address = server.address() as { port: number };
    await saveProfile({ schema_version: 1, origin: `http://127.0.0.1:${address.port}`, token: 'qzc.synthetic-scan-cli', development_http: true, ca_certificate: null });
    let result: { settled: boolean; cancellationRequested?: boolean; watchEnded?: string } | undefined;
    const emit = (value: unknown) => { result = value as typeof result; };
    for (const pending of ['RUNNING', 'STOPPING', 'RECONCILING'] as const) {
      status = pending;
      assert.equal(await run(['parameter-scan', 'wait', 'scan:original', '--timeout', '0.04'], emit), 124);
      assert.equal(result!.settled, false); assert.equal(result!.cancellationRequested, false);
    }
    status = 'STOPPED'; unknown = '1';
    assert.equal(await run(['parameter-scan', 'wait', 'scan:original', '--timeout', '0.04'], emit), 124);
    assert.equal(result!.settled, false);
    unknown = '0';
    for (const [terminal, exitCode] of [['COMPLETED', 0], ['FAILED', 2], ['STOPPED', 2]] as const) {
      status = terminal;
      assert.equal(await run(['parameter-scan', 'wait', 'scan:original', '--timeout', '1'], emit), exitCode);
      assert.equal(result!.settled, true);
    }
    status = 'RUNNING';
    const controller = new AbortController();
    assert.equal(await run(['parameter-scan', 'watch', 'scan:original'], value => { emit(value); controller.abort(); }, controller.signal), 130);
    assert.equal(result!.watchEnded, 'detached'); assert.equal(result!.cancellationRequested, false);
    assert.ok(requests.every(request => request === 'GET /api/v3/parameter-scans/scan%3Aoriginal/events'));
  } finally {
    if (previous === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previous;
    await rm(directory, { recursive: true, force: true });
    server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

test('parameter scan source contracts are discoverable without a service connection', async () => {
  const value = await schemas('ParameterScanStart');
  assert.equal(value.entry, 'ParameterScanStart');
  assert.match(value.source!, /interface ParameterScanPlan/);
  assert.match(value.source!, /interface ParameterScanSurface/);
  assert.match(value.source!, /scientificQualification: 'UNQUALIFIED_EXPLORATORY'/);
  assert.ok((await schemas()).names!.includes('ParameterScanView'));
});

test('dataset coverage CLI preserves exact inclusive nanoseconds and dataset identity', async () => {
  const calls: { path: string; body: string }[] = [];
  const server = createServer(async (request, response) => {
    let body = ''; for await (const chunk of request) body += chunk;
    calls.push({ path: request.url!, body });
    response.writeHead(200, { 'content-type': 'application/json' }); response.end('{}');
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const directory = await mkdtemp(join(tmpdir(), 'qz-coverage-cli-'));
  const previous = process.env.XDG_CONFIG_HOME; process.env.XDG_CONFIG_HOME = directory;
  const file = join(directory, 'request.json');
  const bytes = '{"bar_types":["BTC/USDT.BINANCE-1-MINUTE-LAST-EXTERNAL"],"start_ns":"1790000000000000001","end_ns":"1790000000000000002"}';
  try {
    await writeFile(file, bytes);
    const address = server.address() as { port: number };
    await saveProfile({ schema_version: 1, origin: `http://127.0.0.1:${address.port}`, token: 'qzc.test-coverage', development_http: true, ca_certificate: null });
    await run(['datasets', 'coverage', 'dataset:one/two', '--file', file], () => {});
    assert.deepEqual(calls, [{ path: '/api/v3/datasets/dataset%3Aone%2Ftwo/coverage', body: bytes }]);
  } finally {
    if (previous === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previous;
    await rm(directory, { recursive: true, force: true });
    server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});

test('observation sizing and order preview CLI preserve exact inputs without a trading command', async () => {
  const calls: { path: string; body: string }[] = [];
  const server = createServer(async (request, response) => {
    let body = ''; for await (const chunk of request) body += chunk;
    assert.equal(request.method, 'POST'); calls.push({ path: request.url!, body });
    response.writeHead(200, { 'content-type': 'application/json' }); response.end('{"preview_only":true}');
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-preview-cli-'));
  const previous = process.env.XDG_CONFIG_HOME; process.env.XDG_CONFIG_HOME = directory;
  const file = join(directory, 'request.json');
  const sizing = '{"sizing":{"instrument_id":"BTC.HYPERLIQUID","equity":"1000.000000000000000001"}}';
  const order = '{"instrument_id":"BTC.HYPERLIQUID","order":{"kind":"STOP_LOSS","quantity":"0.00000001","trigger_price":"95.00000001","side":"SELL"}}';
  try {
    const address = server.address() as { port: number };
    await saveProfile({ schema_version: 1, origin: `http://127.0.0.1:${address.port}`, token: 'qzc.test-preview', development_http: true, ca_certificate: null });
    await writeFile(file, sizing); await run(['observation', 'size', 'original:account/one', '--file', file], () => {});
    await writeFile(file, order); await run(['observation', 'preview-order', 'original:account/one', '--file', file], () => {});
    assert.deepEqual(calls, [
      { path: '/api/v3/observations/original%3Aaccount%2Fone/sizing/fixed-risk', body: sizing },
      { path: '/api/v3/observations/original%3Aaccount%2Fone/orders/preview', body: order },
    ]);
  } finally {
    if (previous === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previous;
    await rm(directory, { recursive: true, force: true });
    server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
});
