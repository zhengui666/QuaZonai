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

test('parameter scan commands preserve original writes, opaque identities and exact query values', async () => {
  const requests: { method: string; path: string; body: string }[] = [];
  let drop = true;
  const server = createServer(async (request, response) => {
    let body = ''; for await (const chunk of request) body += chunk;
    requests.push({ method: request.method!, path: request.url!, body });
    if (request.method === 'POST' && request.url === '/api/v3/parameter-scans' && drop) {
      drop = false; request.socket.destroy(); return;
    }
    response.writeHead(request.method === 'POST' ? 202 : 200, { 'content-type': 'application/json' });
    response.end(JSON.stringify({ received: true }));
  });
  server.listen(0, '127.0.0.1'); await once(server, 'listening');
  const directory = await mkdtemp(join(tmpdir(), 'qz-parameter-scan-cli-'));
  const previous = process.env.XDG_CONFIG_HOME; process.env.XDG_CONFIG_HOME = directory;
  const file = join(directory, 'original.json');
  const bytes = '{\n "requestId":"original-scan", "name":"EMA parameter scan", "source":{"kind":"factor","factorId":"r2.t3.m0"}, "mode":"STRATEGY", "axes":[{"key":"investedWeight","domain":{"kind":"values","values":["0.100000000000000001"]}}], "objective":{"metricId":"account.sharpe252","partition":"VALIDATION","direction":"maximize"}, "execution":{"concurrency":1}\n}\n';
  await writeFile(file, bytes);
  try {
    const address = server.address() as { port: number };
    await saveProfile({ schema_version: 1, origin: `http://127.0.0.1:${address.port}`, token: 'qzc.synthetic-scan-cli', development_http: true, ca_certificate: null });
    await assert.rejects(run(['parameter-scan', 'create', '--file', file], () => {}), /write_outcome_unknown_retain_original_request/);
    assert.equal(requests.length, 1);
    await run(['parameter-scan', 'create', '--file', file, '--request-id', 'original-scan'], () => {});
    assert.deepEqual(requests.slice(0, 2), [
      { method: 'POST', path: '/api/v3/parameter-scans', body: bytes },
      { method: 'POST', path: '/api/v3/parameter-scans', body: bytes },
    ]);
    await assert.rejects(run(['parameter-scan', 'create', '--file', file, '--request-id', 'different'], () => {}), /request_id_conflict/);
    await run(['parameter-scan', 'preview', '--file', file], () => {});
    assert.equal(requests.at(-1)!.body, bytes);
    await run(['parameter-scan', 'list'], () => {});
    await run(['parameter-scan', 'get', 'scan:one/two'], () => {});
    await run(['parameter-scan', 'stop', 'scan:one/two'], () => {});
    await run(['parameter-scan', 'options'], () => {});
    await writeFile(file, '{"sourceKind":"factor","sourceId":"r2.t3.m0"}');
    await run(['parameter-scan', 'options', '--file', file], () => {});
    await writeFile(file, '{"page":1,"pageSize":20,"status":"FAILED","sort":"metric"}');
    await run(['parameter-scan', 'list', '--file', file], () => {});
    await run(['parameter-scan', 'trials', 'scan:one/two', '--file', file], () => {});
    await writeFile(file, '{"x":"fast_period","y":"slow_period","fixed":{"investedWeight":"0.100000000000000001"},"metricId":"account.sharpe252","xOffset":"9007199254740993","limit":20}');
    await run(['parameter-scan', 'surface', 'scan:one/two', '--file', file], () => {});
    assert.deepEqual(requests.slice(2).map(({ method, path }) => [method, path]), [
      ['POST', '/api/v3/parameter-scans/preview'],
      ['GET', '/api/v3/parameter-scans'],
      ['GET', '/api/v3/parameter-scans/scan%3Aone%2Ftwo'],
      ['POST', '/api/v3/parameter-scans/scan%3Aone%2Ftwo/stop'],
      ['GET', '/api/v3/parameter-scans/options'],
      ['GET', '/api/v3/parameter-scans/options?sourceKind=factor&sourceId=r2.t3.m0'],
      ['GET', '/api/v3/parameter-scans?page=1&pageSize=20&status=FAILED&sort=metric'],
      ['GET', '/api/v3/parameter-scans/scan%3Aone%2Ftwo/trials?page=1&pageSize=20&status=FAILED&sort=metric'],
      ['GET', '/api/v3/parameter-scans/scan%3Aone%2Ftwo/surface?x=fast_period&y=slow_period&fixed=%7B%22investedWeight%22%3A%220.100000000000000001%22%7D&metricId=account.sharpe252&xOffset=9007199254740993&limit=20'],
    ]);
    const admitted = requests.length;
    await assert.rejects(run(['parameter-scan', 'get']), /resource_id_required/);
    await writeFile(file, '{"datasetId":{"nested":"unsupported"}}');
    await assert.rejects(run(['parameter-scan', 'options', '--file', file]), /query_scalar_required/);
    await writeFile(file, '{"datasetId":9007199254740993}');
    await assert.rejects(run(['parameter-scan', 'options', '--file', file]), /invalid_json_number_use_decimal_string/);
    assert.equal(requests.length, admitted);
  } finally {
    if (previous === undefined) delete process.env.XDG_CONFIG_HOME; else process.env.XDG_CONFIG_HOME = previous;
    await rm(directory, { recursive: true, force: true });
    server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
  }
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
