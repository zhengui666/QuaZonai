import test from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { AddressInfo } from 'node:net';
import { createHash, randomUUID } from 'node:crypto';
import { createNautilusWorker, executionId, type WorkerOptions } from '../src/server.ts';
import type { NativeExecutionRequest, NativeExecutionReceipt } from '../../../packages/contracts/src/nautilus.ts';

test('request hashing preserves original canonical identities across buffered Unicode and attachment boundaries', () => {
  const request: Omit<NativeExecutionRequest, 'requestId'> = {
    schemaVersion: 1, invocationId: '11111111-1111-4111-8111-111111111111',
    implementationDigest: `sha256:${'a'.repeat(64)}`, command: 'allocate',
    input: { z: [null, true, false, -0, 1e-10, '饭饭🐋', '\ud800'], a: {
      '10': '1790000000000000001', '2': '0.000000000000000001', escape: '"\\\n',
    } }, args: ['--model', '@file:model'], files: { model: 'Zml4dHVyZQ==' },
  };
  // Frozen from the original whole-string canonical encoder, not the new writer.
  assert.equal(executionId(request), '5225f6bb7e658b183cc4fde48e96087178c4e306b8a1659a6eeb4baa31fe69c8');
  const large = { ...request, input: ['x'.repeat(65_535) + '🐋', ...Array.from({ length: 10_000 }, (_, index) => index)],
    files: { model: Buffer.alloc(100_000, 7).toString('base64') } };
  // This fixture's nested values have no object-key ordering ambiguity.
  const original = `{${Object.keys(large).sort().map(key => `${JSON.stringify(key)}:${JSON.stringify(large[key as keyof typeof large])}`).join(',')}}`;
  assert.equal(executionId(large), createHash('sha256').update(original).digest('hex'));
  assert.equal(executionId({ ...large, files: { ...large.files }, input: [...large.input] }), executionId(large));
});

async function fixture() {
  const directory = await mkdtemp(join(tmpdir(), 'qz-remote-worker-'));
  const catalog = join(directory, 'catalog'); await mkdir(catalog);
  const count = join(directory, 'calls'); const binary = join(directory, 'job');
  const script = join(directory, 'job.mjs');
  await writeFile(script, `import fs from 'node:fs';
fs.appendFileSync(${JSON.stringify(count)}, process.pid + '\\n');
let text=''; for await (const chunk of process.stdin) text+=chunk;
const input=JSON.parse(text); const args=process.argv.slice(3);
if(input.progress && process.env.QZ_PROGRESS_PATH) fs.writeFileSync(process.env.QZ_PROGRESS_PATH, JSON.stringify({snapshot:input.progress}) + '\\n' + '{\"snapshot\":');
if(input.wait) { console.error('waiting'); setInterval(()=>{},1000); }
else if(input.fail) { console.error('NATIVE_CONTROLLED_FAILURE'); process.exitCode=2; }
else if(input.invalid) { console.log('invalid'); }
else { const contents={}; for(let i=0;i<args.length;i++) if(['--model','--scope'].includes(args[i])) contents[args[i]]=fs.readFileSync(args[++i],'utf8');
const objects=args.indexOf('--objects'); if(objects>=0) contents.objects=fs.readdirSync(args[objects+1]).sort();
console.log(JSON.stringify({input,args,contents,authLeaked:!!process.env.QZ_NAUTILUS_TOKEN})); }
`);
  await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${script}' "$@"\n`, { mode: 0o700 });
  const options: WorkerOptions = { binary, dataDir: join(directory, 'state'), catalogRoots: [catalog], token: 'test-only-worker-token' };
  let worker = await createNautilusWorker(options);
  async function listen() { worker.server.listen(0, '127.0.0.1'); await once(worker.server, 'listening'); return `http://127.0.0.1:${(worker.server.address() as AddressInfo).port}`; }
  let url = await listen();
  const headers = { authorization: `Bearer ${options.token}`, 'content-type': 'application/json' };
  function request(input: NativeExecutionRequest['input'], args: string[] = [], files: Record<string, string> = {}): NativeExecutionRequest {
    const body = { schemaVersion: 1 as const, invocationId: randomUUID(), implementationDigest: worker.implementationDigest, command: 'review-source', input, args, files };
    return { ...body, requestId: executionId(body) };
  }
  return { directory, catalog, count, options, headers, request, get worker() { return worker; }, get url() { return url; },
    async post(body: NativeExecutionRequest) {
      let response = await fetch(`${url}/v1/execute`, { method: 'POST', headers, body: JSON.stringify(body) });
      if ((body.input as { wait?: boolean })?.wait || !response.ok) return response;
      for (let attempt = 0; attempt < 500 && response.status === 202; attempt++) {
        await response.body?.cancel(); await new Promise(resolve => setTimeout(resolve, 10));
        response = await fetch(`${url}/v1/executions/${body.requestId}`, { headers });
      }
      return response;
    },
    async restart() { await worker.close(); worker = await createNautilusWorker(options); url = await listen(); },
    async cleanup() { await worker.close(); await rm(directory, { recursive: true, force: true }); },
  };
}

test('removed scientific commands are absent from capabilities and rejected before native execution', async () => {
  const f = await fixture();
  try {
    const capabilities = await (await fetch(`${f.url}/v1/capabilities`, { headers: f.headers })).json();
    for (const command of ['review-science', 'review-allocate', 'fit-ensemble', 'apply-ensemble', 'freeze-ensemble-calibrations', 'simulate-indicator-strategy', 'simulate-indicator-strategy-batch', 'forecast', 'forecast-factor', 'allocate', 'study-portfolio', 'build-observed-spot', 'study-observed-spot', 'evaluate-experiment', 'replay-tick-economic']) {
      assert.equal(capabilities.commands.includes(command), false);
      const body = f.request({}); body.command = command;
      const { requestId: _previous, ...input } = body; body.requestId = executionId(input);
      const response = await f.post(body);
      assert.equal(response.status, 400);
      assert.deepEqual(await response.json(), { error: 'nautilus_request_invalid' });
      assert.equal((await fetch(`${f.url}/v1/executions/${body.requestId}`, { headers: f.headers })).status, 404);
    }
    await assert.rejects(readFile(f.count), { code: 'ENOENT' });
    assert.equal(capabilities.commands.includes('review-source'), true);
    const body = f.request({}); body.command = 'review-source';
    const { requestId: _previous, ...input } = body; body.requestId = executionId(input);
    assert.equal((await (await f.post(body)).json()).status, 'COMPLETED');
    assert.equal((await readFile(f.count, 'utf8')).trim().split('\n').length, 1);
  } finally { await f.cleanup(); }
});

test('retired artifact and solver arguments cannot launch native execution', async () => {
  const f = await fixture();
  try {
    for (const option of ['--model', '--scope', '--features', '--calibration', '--objects', '--funding-catalog', '--model-artifact-id', '--feature-artifact-id', '--book-cores', '--summary-only', '--plan-only']) {
      const result = await (await f.post(f.request({}, [option, 'retired']))).json();
      assert.equal(result.status, 'FAILED');
      assert.equal(result.error, 'nautilus_argument_unsupported');
    }
    const attached = await (await f.post(f.request({}, [], { model: 'eA==' }))).json();
    assert.equal(attached.status, 'FAILED');
    assert.equal(attached.error, 'nautilus_attachments_unsupported');
    await assert.rejects(readFile(f.count), { code: 'ENOENT' });
  } finally { await f.cleanup(); }
});

test('capabilities expose the verified build manifest of the actual executable', async () => {
  const f = await fixture();
  try {
    const path = join(f.directory, 'native-manifest.json');
    const manifest = { schemaVersion: 1, nautilusVersion: 'test', nautilusRevision: 'a'.repeat(40),
      binaries: { job: { precision: 'standard', digest: `sha256:${createHash('sha256').update(await readFile(f.options.binary)).digest('hex')}` } } };
    await writeFile(path, JSON.stringify(manifest)); f.options.manifestPath = path;
    await f.restart();
    const capabilities = await (await fetch(`${f.url}/v1/capabilities`, { headers: f.headers })).json();
    assert.deepEqual(capabilities.buildIdentity, manifest);
    manifest.binaries.job.digest = `sha256:${'b'.repeat(64)}`;
    await writeFile(path, JSON.stringify(manifest));
    await assert.rejects(createNautilusWorker(f.options), /nautilus_build_identity_mismatch/);
  } finally { await f.cleanup(); }
});

test('remote worker retains actual execution and original receipt across retries and restart', async () => {
  const f = await fixture();
  try {
    assert.equal((await fetch(`${f.url}/v1/capabilities`)).status, 401);
    const caps = await (await fetch(`${f.url}/v1/capabilities`, { headers: f.headers })).json();
    assert.match(caps.implementationDigest, /^sha256:[a-f0-9]{64}$/);
    assert.ok(caps.commands.includes('simulate')); assert.ok(!caps.commands.includes('paper'));
    const body = f.request({ decimal: '1.000000000000000001', ns: '1790000000000000000' },
      ['--catalog', f.catalog]);
    const responses = await Promise.all([f.post(body), f.post(body)]);
    const [first, duplicate] = await Promise.all(responses.map(response => response.json()));
    assert.equal(first.status, 'COMPLETED'); assert.deepEqual(duplicate, first);
    assert.deepEqual(first.result.input, body.input);
    assert.deepEqual(first.result.contents, {});
    assert.equal(first.result.authLeaked, false);
    assert.equal((await readFile(f.count, 'utf8')).trim().split('\n').length, 1);
    await f.restart();
    assert.deepEqual(await (await f.post(body)).json(), first);
    assert.deepEqual(await (await fetch(`${f.url}/v1/executions/${body.requestId}`, { headers: f.headers })).json(), first);
    assert.equal((await readFile(f.count, 'utf8')).trim().split('\n').length, 1);
    assert.equal((await f.post({ ...body, input: { changed: true } })).status, 409);
    assert.equal((await f.post({ ...body, implementationDigest: `sha256:${'0'.repeat(64)}` })).status, 409);
  } finally { await f.cleanup(); }
});

test('worker rejects arbitrary commands and unregistered paths; failures remain failed receipts', async () => {
  const f = await fixture();
  try {
    const forbidden = f.request({}); forbidden.command = 'paper';
    assert.equal((await f.post(forbidden)).status, 400);
    const outside = await (await f.post(f.request({}, ['--catalog', f.directory]))).json();
    assert.equal(outside.status, 'FAILED'); assert.equal(outside.error, 'nautilus_catalog_outside_configured_roots');
    const missing = await (await f.post(f.request({}, ['--model', '/etc/passwd']))).json();
    assert.equal(missing.status, 'FAILED'); assert.equal(missing.error, 'nautilus_argument_unsupported');
    const traversal = f.request({}, [], { '../escape': 'eA==' });
    assert.equal((await f.post(traversal)).status, 400);
    const failure = f.request({ fail: true });
    const first = await (await f.post(failure)).json();
    assert.equal(first.status, 'FAILED'); assert.equal(first.error, 'NATIVE_CONTROLLED_FAILURE');
    assert.deepEqual(await (await f.post(failure)).json(), first);
    const invalid = await (await f.post(f.request({ invalid: true }))).json();
    assert.equal(invalid.status, 'FAILED');
  } finally { await f.cleanup(); }
});

test('cancellation joins the native process and never reexecutes the same request', async () => {
  const f = await fixture();
  try {
    const body = f.request({ wait: true });
    const pending = f.post(body);
    let pid: number | undefined;
    for (let attempt = 0; attempt < 100; attempt++) {
      try { pid = Number((await readFile(f.count, 'utf8')).trim()); break; } catch { await new Promise(resolve => setTimeout(resolve, 10)); }
    }
    assert.ok(pid, 'native process actually started');
    const cancelled = await (await fetch(`${f.url}/v1/executions/${body.requestId}`, { method: 'DELETE', headers: f.headers })).json();
    assert.equal(cancelled.status, 'CANCELLED');
    assert.throws(() => process.kill(pid!, 0), { code: 'ESRCH' });
    assert.equal((await (await pending).json()).status, 'RUNNING');
    assert.deepEqual(await (await f.post(body)).json(), cancelled);
    assert.equal((await readFile(f.count, 'utf8')).trim().split('\n').length, 1);
  } finally { await f.cleanup(); }
});

test('a persisted uncertain run is readable but is neither repeated nor falsely cancelled', async () => {
  const f = await fixture();
  try {
    const body = f.request({ interrupted: true });
    const directory = join(f.options.dataDir, 'executions', body.requestId); await mkdir(directory);
    const running: NativeExecutionReceipt = { schemaVersion: 1, requestId: body.requestId, status: 'RUNNING' };
    await writeFile(join(directory, 'receipt.json'), JSON.stringify(running));
    await f.restart();
    for (const method of ['GET', 'DELETE']) {
      const receipt = await (await fetch(`${f.url}/v1/executions/${body.requestId}`, { method, headers: f.headers })).json();
      assert.equal(receipt.status, 'UNKNOWN');
    }
    assert.equal((await (await f.post(body)).json()).status, 'UNKNOWN');
    await assert.rejects(readFile(f.count), { code: 'ENOENT' });
  } finally { await f.cleanup(); }
});

test('admission detaches from HTTP and only explicit cancellation stops the child', async () => {
  const f = await fixture();
  try {
    const first = f.request({ sameInput: true });
    const second = f.request({ sameInput: true });
    assert.notEqual(first.requestId, second.requestId);
    assert.equal((await (await f.post(first)).json()).status, 'COMPLETED');
    assert.equal((await (await f.post(second)).json()).status, 'COMPLETED');
    assert.equal((await readFile(f.count, 'utf8')).trim().split('\n').length, 2);

    const body = f.request({ wait: true }); const controller = new AbortController();
    const pending = fetch(`${f.url}/v1/execute`, { method: 'POST', headers: f.headers, body: JSON.stringify(body), signal: controller.signal });
    let pid: number | undefined;
    for (let attempt = 0; attempt < 100; attempt++) {
      const ids = (await readFile(f.count, 'utf8')).trim().split('\n');
      if (ids.length === 3) { pid = Number(ids[2]); break; }
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    assert.ok(pid);
    const admitted = await pending; assert.equal(admitted.status, 202);
    assert.equal((await admitted.json()).status, 'RUNNING');
    controller.abort();
    assert.doesNotThrow(() => process.kill(pid!, 0));
    let receipt: NativeExecutionReceipt | undefined;
    for (let attempt = 0; attempt < 100; attempt++) {
      receipt = await (await fetch(`${f.url}/v1/executions/${body.requestId}`, { headers: f.headers })).json();
      if (receipt?.status === 'RUNNING') break;
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    assert.equal(receipt?.status, 'RUNNING');
    const duplicate = await f.post(body); assert.equal(duplicate.status, 202);
    assert.equal((await duplicate.json()).status, 'RUNNING');
    assert.equal((await readFile(f.count, 'utf8')).trim().split('\n').length, 3);
    const cancelled = await (await fetch(`${f.url}/v1/executions/${body.requestId}`, { method: 'DELETE', headers: f.headers })).json();
    assert.equal(cancelled.status, 'CANCELLED');
    assert.throws(() => process.kill(pid!, 0), { code: 'ESRCH' });
  } finally { await f.cleanup(); }
});


test('running receipt exposes original snapshot previews without waiting for or repeating execution', async () => {
  const f = await fixture();
  try {
    const snapshot = { ts_event: '1790000000000000001', total_equity: ['1000.000000000000000001 USD'] };
    const body = f.request({ wait: true, progress: snapshot }); body.command = 'simulate';
    const { requestId: _originalId, ...content } = body; body.requestId = executionId(content);
    const pending = f.post(body);
    let running: NativeExecutionReceipt | undefined;
    for (let attempt = 0; attempt < 100; attempt++) {
      const response = await fetch(`${f.url}/v1/executions/${body.requestId}`, { headers: f.headers });
      if (response.ok) {
        running = await response.json() as NativeExecutionReceipt;
        if (running.status === 'RUNNING' && running.progress?.snapshots.length) break;
      }
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    assert.equal(running?.status, 'RUNNING');
    assert.deepEqual(running?.status === 'RUNNING' && running.progress?.snapshots, [snapshot], 'incomplete final line is ignored, decimals and clocks are preserved');
    const cancelled = await (await fetch(`${f.url}/v1/executions/${body.requestId}`, { method: 'DELETE', headers: f.headers })).json();
    assert.equal(cancelled.status, 'CANCELLED'); await pending;
    assert.equal((await readFile(f.count, 'utf8')).trim().split('\n').length, 1);
  } finally { await f.cleanup(); }
});

test('catalog coverage uses the existing read-only catalog execution route without rounding nanoseconds', async () => {
  const f = await fixture();
  try {
    for (const selector of [{ bar_types: ['BTC/USDT.BINANCE-1-MINUTE-LAST-EXTERNAL'] },
      { selectors: [{ data_type: 'TradeTick', identifier: 'BTC/USDT.BINANCE' }, { data_type: 'QuoteTick', identifier: 'BTC/USDT.BINANCE' }] }] as Record<string, NativeExecutionRequest['input']>[]) {
    const request = f.request({ ...selector,
      start_ns: '1790000000000000001', end_ns: '1790000000000000002' }, ['--catalog', f.catalog]);
    request.command = 'catalog-coverage';
    const { requestId: _old, ...input } = request;
    request.requestId = executionId(input);
    const response = await f.post(request);
    const receipt = await response.json();
    assert.equal(receipt.status, 'COMPLETED');
    assert.deepEqual(receipt.result.input, request.input);
    assert.deepEqual(receipt.result.args, ['--catalog', f.catalog]);
    }
  } finally { await f.cleanup(); }
});

test('stopping an active upstream strategy cancels its one process and never publishes a result', async () => {
  const f = await fixture();
  try {
    const body = f.request({ wait: true, requests: [{ ordinal: '0' }, { ordinal: '1' }] });
    body.command = 'native-strategy-backtest';
    const { requestId: _previous, ...input } = body; body.requestId = executionId(input);
    const response = await f.post(body); assert.equal(response.status, 202); await response.body?.cancel();
    for (let attempt = 0; ; attempt++) {
      try { await readFile(f.count, 'utf8'); break; }
      catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT' || attempt === 100) throw error; }
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    const stopped = await (await fetch(`${f.url}/v1/executions/${body.requestId}`, { method: 'DELETE', headers: f.headers })).json();
    assert.equal(stopped.status, 'CANCELLED'); assert.equal('result' in stopped, false);
    const readback = await (await fetch(`${f.url}/v1/executions/${body.requestId}`, { headers: f.headers })).json();
    assert.equal(readback.status, 'CANCELLED'); assert.equal('result' in readback, false);
    const replay = await (await f.post(body)).json(); assert.equal(replay.status, 'CANCELLED');
    assert.equal((await readFile(f.count, 'utf8')).trim().split('\n').length, 1);
  } finally { await f.cleanup(); }
});

test('upstream native strategy command preserves exact clocks and only registered catalog paths', async () => {
  const f = await fixture();
  try {
    const input = { event_start_ns: '1790000000000000001', strategy: { kind: 'EMA_CROSS', trade_size: '0.000000000000000001' } };
    const body = f.request(input, ['--catalog', f.catalog]); body.command = 'native-strategy-backtest';
    const { requestId: _old, ...wire } = body; body.requestId = executionId(wire);
    const receipt = await (await f.post(body)).json() as NativeExecutionReceipt;
    assert.equal(receipt.status, 'COMPLETED');
    assert.deepEqual((receipt.result as { input: unknown }).input, input);
    const outside = f.request(input, ['--catalog', f.directory]); outside.command = 'native-strategy-backtest'; const { requestId: _outsideOld, ...outsideWire } = outside; outside.requestId = executionId(outsideWire);
    const rejected = await (await f.post(outside)).json();
    assert.equal(rejected.status, 'FAILED'); assert.equal(rejected.error, 'nautilus_catalog_outside_configured_roots');
  } finally { await f.cleanup(); }
});

test('catalog inspection forwards an empty read-only request only within the configured root', async () => {
  const f = await fixture();
  try {
    const capabilities = await (await fetch(`${f.url}/v1/capabilities`, { headers: f.headers })).json();
    assert.equal(capabilities.commands.includes('catalog-inspect'), true);
    const request = f.request({}, ['--catalog', f.catalog]);
    request.command = 'catalog-inspect';
    const { requestId: _old, ...input } = request;
    request.requestId = executionId(input);
    const receipt = await (await f.post(request)).json();
    assert.equal(receipt.status, 'COMPLETED');
    assert.deepEqual(receipt.result.input, {});
    assert.deepEqual(receipt.result.args, ['--catalog', f.catalog]);
    const outside = { ...input, args: ['--catalog', tmpdir()] };
    const denied = await f.post({ ...outside, requestId: executionId(outside) });
    const failure = await denied.json();
    assert.equal(failure.status, "FAILED");
    assert.match(failure.error, /nautilus_catalog_outside_configured_roots/);
    assert.equal((await readFile(f.count, "utf8")).trim().split("\n").length, 1);
  } finally { await f.cleanup(); }
});
