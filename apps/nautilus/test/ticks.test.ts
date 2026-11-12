import test from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createHash, randomUUID } from 'node:crypto';
import type { AddressInfo } from 'node:net';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';
import { createNautilusWorker, executionId, type WorkerOptions } from '../src/server.ts';

// Controlled processes establish native process/receipt behavior, not numerical or PIT evidence.
async function fixture(overrides: Partial<WorkerOptions> = {}, companion = false) {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-worker-'));
  const catalog = join(directory, 'catalog'); await mkdir(catalog);
  const calls = join(directory, 'calls'); const binary = join(directory, 'job');
  await writeFile(binary, `#!/usr/bin/python3
import json, mmap, os, resource, sys, time
with open(${JSON.stringify(calls)}, 'a') as stream: stream.write(str(os.getpid())+'\\n')
request=json.load(sys.stdin)
if request.get('wait'):
    while True: time.sleep(0.05)
if request.get('large'):
    sys.stdout.write('"'+'x'*1000000+'"'); sys.stdout.flush()
    sys.exit(0)
limits=resource.getrlimit(resource.RLIMIT_AS)
raised=False
if request.get('raiseLimit'):
    try: resource.setrlimit(resource.RLIMIT_AS, (limits[1]+4096,limits[1]+4096)); raised=True
    except (ValueError, OSError): pass
allocation='not_requested'
if request.get('allocateBeyondLimit'):
    try:
        region=mmap.mmap(-1,32*1024*1024); region.close(); allocation='allowed'
    except (OSError, OverflowError): allocation='rejected'
print(json.dumps({'input':request,'limits':list(limits),'raised':raised,'allocation':allocation,'authLeaked':'QZ_NAUTILUS_TOKEN' in os.environ,'executable':os.path.basename(sys.argv[0])}))
`, { mode: 0o700 });
  const tickBinary = join(directory, 'job-high-precision');
  if (companion) await writeFile(tickBinary, await readFile(binary, 'utf8') + '\n# controlled high-format companion\n', { mode: 0o700 });
  const options: WorkerOptions = { binary, dataDir: join(directory, 'state'), catalogRoots: [catalog], ...(companion ? { tickBinary } : {}), ...overrides };
  let worker = await createNautilusWorker(options);
  async function listen() {
    worker.server.listen(0, '127.0.0.1'); await once(worker.server, 'listening');
    return `http://127.0.0.1:${(worker.server.address() as AddressInfo).port}`;
  }
  let url = await listen();
  const headers = { 'content-type': 'application/json' };
  function request(input: NativeExecutionRequest['input'], command = 'simulate', args: string[] = [], files: Record<string, string> = {}) {
    const body = { schemaVersion: 1 as const, invocationId: randomUUID(), implementationDigest: worker.implementationDigest, command, input, args, files };
    return { ...body, requestId: executionId(body) };
  }
  async function started(count = 1) {
    for (let attempt = 0; attempt < 200; attempt++) {
      try {
        const pids = (await readFile(calls, 'utf8')).trim().split('\n').map(Number);
        if (pids.length >= count) return pids[count - 1]!;
      } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; }
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    assert.fail('controlled native process did not start');
  }
  return { directory, calls, options, request, started, get url() { return url; },
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

test('native execution inherits process limits and preserves original receipts', async () => {
  const f = await fixture();
  try {
    const caps = await (await fetch(`${f.url}/v1/capabilities`)).json();
    assert.ok(caps.commands.includes('simulate'));
    const body = f.request({ allocateBeyondLimit: true });
    const original = await (await f.post(body)).json();
    assert.equal(original.status, 'COMPLETED');
    const bar = await (await f.post(f.request({}, 'review-source'))).json();
    assert.deepEqual(original.result.limits, bar.result.limits);
    assert.equal(original.result.allocation, 'allowed');
    assert.equal(original.result.authLeaked, false); assert.deepEqual(original.result.input, body.input);
    await f.restart();
    assert.deepEqual(await (await f.post(body)).json(), original);
    assert.equal((await readFile(f.calls, 'utf8')).trim().split('\n').length, 2);
  } finally { await f.cleanup(); }
});

test('native active execution keeps exact replay, independent commands and confirmed cancellation', async () => {
  const f = await fixture();
  try {
    const completed = f.request({ earlier: true }); const original = await (await f.post(completed)).json();
    const waiting = f.request({ wait: true }); const first = f.post(waiting); const pid = await f.started(2);
    const duplicate = f.post(waiting); const next = f.request({ next: true });
    assert.equal((await (await f.post(next)).json()).status, 'COMPLETED', 'fixed-target historical replay is independent of tick execution slots');
    assert.deepEqual(await (await f.post(completed)).json(), original);
    const batch = await (await f.post(f.request({}, 'review-source'))).json();
    assert.equal(batch.status, 'COMPLETED');
    const cancelled = await (await fetch(`${f.url}/v1/executions/${waiting.requestId}`, { method: 'DELETE' })).json();
    assert.equal(cancelled.status, 'CANCELLED'); assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    assert.equal((await (await first).json()).status, 'RUNNING'); assert.equal((await (await duplicate).json()).status, 'RUNNING');
    const count = (await readFile(f.calls, 'utf8')).trim().split('\n').length;
    assert.equal((await (await f.post(next)).json()).status, 'COMPLETED');
    assert.equal((await readFile(f.calls, 'utf8')).trim().split('\n').length, count, 'replay never starts another process');
  } finally { await f.cleanup(); }
});

test('large native output keeps the completed receipt across retry', async () => {
  const f = await fixture();
  try {
    const body = f.request({ large: true });
    const original = await (await f.post(body)).json();
    assert.equal(original.status, 'COMPLETED'); assert.equal(original.result.length, 1_000_000);
    await f.restart();
    assert.deepEqual(await (await f.post(body)).json(), original);
    assert.equal((await readFile(f.calls, 'utf8')).trim().split('\n').length, 1);
  } finally { await f.cleanup(); }
});

test('the configured native executable alone binds implementation identity and cannot borrow old receipts', async () => {
  const f = await fixture();
  try {
    const before = await (await fetch(`${f.url}/v1/capabilities`)).json();
    assert.equal(before.implementationDigest, `sha256:${createHash('sha256').update(await readFile(f.options.binary)).digest('hex')}`);
    const body = f.request({ timestamp_ns: '1790000000000000001', amount: '9007199254740993.123456789012345678' });
    const original = await (await f.post(body)).json();
    assert.equal(original.status, 'COMPLETED'); assert.deepEqual(original.result.input, body.input); assert.equal(original.result.executable, 'job');
    await writeFile(f.options.binary, await readFile(f.options.binary, 'utf8') + '\n# changed official executable bytes\n');
    const changed = await (await f.post(f.request({}))).json();
    assert.equal(changed.status, 'FAILED'); assert.equal(changed.error, 'nautilus_implementation_changed');
    assert.equal((await readFile(f.calls, 'utf8')).trim().split('\n').length, 1);
    await f.restart();
    const after = await (await fetch(`${f.url}/v1/capabilities`)).json();
    assert.notEqual(after.implementationDigest, before.implementationDigest);
    assert.equal((await f.post(body)).status, 409);
    assert.deepEqual(await (await fetch(`${f.url}/v1/executions/${body.requestId}`)).json(), original, 'original receipt stays readable');
    assert.equal((await (await f.post(f.request({}))).json()).status, 'COMPLETED');
  } finally { await f.cleanup(); }
});

test('native catalog mounts remain restricted without a retired economic replay worker', async () => {
  const f = await fixture();
  try {
    const mounted = ['--catalog', f.options.catalogRoots[0]!];
    const result = await (await f.post(f.request({}, 'simulate', mounted))).json();
    assert.equal(result.status, 'COMPLETED'); assert.equal(result.result.executable, 'job');
    const outside = await (await f.post(f.request({}, 'simulate', ['--catalog', f.directory]))).json();
    assert.equal(outside.status, 'FAILED'); assert.equal(outside.error, 'nautilus_catalog_outside_configured_roots');
    const outsideFunding = await (await f.post(f.request({}, 'simulate', ['--catalog', f.options.catalogRoots[0]!, '--funding-catalog', f.directory]))).json();
    assert.equal(outsideFunding.error, 'nautilus_argument_unsupported');
    const waiting = f.request({ wait: true }, 'simulate', mounted);
    const active = f.post(waiting); await f.started(2);
    assert.equal((await (await fetch(`${f.url}/v1/executions/${waiting.requestId}`, { method: 'DELETE' })).json()).status, 'CANCELLED');
    assert.equal((await (await active).json()).status, 'RUNNING');
  } finally { await f.cleanup(); }
});

test('tick capacity rejects before creating a receipt; exact replay and BAR operations still work', async () => {
  const f = await fixture();
  try {
    const completed = f.request({ earlier: true }, 'native-strategy-backtest'); const original = await (await f.post(completed)).json();
    const waiting = f.request({ wait: true }, 'native-strategy-backtest'); const first = f.post(waiting); const pid = await f.started(2);
    const duplicate = f.post(waiting); const next = f.request({ next: true }, 'native-strategy-backtest');
    const busy = await f.post(next);
    assert.equal(busy.status, 429); assert.deepEqual(await busy.json(), { schemaVersion: 1, requestId: next.requestId, admission: 'REJECTED', error: 'nautilus_tick_capacity_exhausted' });
    assert.equal((await fetch(`${f.url}/v1/executions/${next.requestId}`)).status, 404);
    assert.deepEqual(await (await f.post(completed)).json(), original);
    const bar = await (await f.post(f.request({}, 'simulate'))).json();
    assert.equal(bar.status, 'COMPLETED');
    assert.notEqual(bar.result.limits[1], 2 * 1024 ** 3, 'tick-only limit does not change BAR process limits');
    const cancelled = await (await fetch(`${f.url}/v1/executions/${waiting.requestId}`, { method: 'DELETE' })).json();
    assert.equal(cancelled.status, 'CANCELLED'); assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    assert.equal((await (await first).json()).status, 'RUNNING'); assert.equal((await (await duplicate).json()).status, 'RUNNING');
    assert.equal((await (await f.post(next)).json()).status, 'COMPLETED');
  } finally { await f.cleanup(); }
});

test('worker capacity configuration still rejects nonpositive concurrency', async () => {
  await assert.rejects(createNautilusWorker({ binary: '/does-not-exist', dataDir: '/does-not-exist', catalogRoots: [], maximumConcurrentTickExecutions: 0 }), /nautilus_tick_concurrency_invalid/);
});

test('explicit precision companions route tick-only and bind both native files and preserve single-binary identity', async () => {
  const single = await fixture();
  try {
    const caps = await (await fetch(`${single.url}/v1/capabilities`)).json();
    assert.equal(caps.implementationDigest, `sha256:${createHash('sha256').update(await readFile(single.options.binary)).digest('hex')}`);
  } finally { await single.cleanup(); }
  const f = await fixture({}, true);
  try {
    const before = await (await fetch(`${f.url}/v1/capabilities`)).json();
    const tick = await (await f.post(f.request({}, 'native-strategy-backtest'))).json();
    assert.equal(tick.status, 'COMPLETED'); assert.equal(tick.result.executable, 'job-high-precision');
    const barBody = f.request({}, 'simulate'); const bar = await (await f.post(barBody)).json();
    assert.equal(bar.status, 'COMPLETED'); assert.equal(bar.result.executable, 'job');
    await writeFile(f.options.tickBinary!, await readFile(f.options.tickBinary!, 'utf8') + '\n# changed companion bytes\n');
    const changed = await (await f.post(f.request({}, 'simulate'))).json();
    assert.equal(changed.status, 'FAILED'); assert.equal(changed.error, 'nautilus_implementation_changed');
    assert.equal((await readFile(f.calls, 'utf8')).trim().split('\n').length, 2, 'a changed companion cannot execute even an old standard identity');
    await f.restart();
    const after = await (await fetch(`${f.url}/v1/capabilities`)).json();
    assert.notEqual(after.implementationDigest, before.implementationDigest);
    assert.equal((await f.post(barBody)).status, 409, 'the new executable pair cannot borrow an old POST receipt identity');
    assert.deepEqual(await (await fetch(`${f.url}/v1/executions/${barBody.requestId}`)).json(), bar, 'historical original receipts remain readable');
    assert.equal((await (await f.post(f.request({}, 'native-strategy-backtest'))).json()).status, 'COMPLETED');
    await writeFile(f.options.binary, await readFile(f.options.binary, 'utf8') + '\n# changed standard bytes\n');
    const standardChanged = await (await f.post(f.request({}, 'native-strategy-backtest'))).json();
    assert.equal(standardChanged.status, 'FAILED'); assert.equal(standardChanged.error, 'nautilus_implementation_changed');
  } finally { await f.cleanup(); }
});


test('upstream strategy replay shares tick precision, mounted catalogs and admission slots', async () => {
  const f = await fixture({}, true);
  try {
    const caps = await (await fetch(`${f.url}/v1/capabilities`)).json();
    assert.ok(caps.commands.includes('native-strategy-backtest'));
    const mounted = ['--catalog', f.options.catalogRoots[0]!];
    const files = {};
    const result = await (await f.post(f.request({}, 'native-strategy-backtest', mounted, files))).json();
    assert.equal(result.status, 'COMPLETED'); assert.equal(result.result.executable, 'job-high-precision');
    const bar = await (await f.post(f.request({}, 'simulate'))).json();
    assert.deepEqual(result.result.limits, bar.result.limits);
    const outside = await (await f.post(f.request({}, 'native-strategy-backtest', ['--catalog', f.directory]))).json();
    assert.equal(outside.status, 'FAILED'); assert.equal(outside.error, 'nautilus_catalog_outside_configured_roots');
    const outsideFunding = await (await f.post(f.request({}, 'native-strategy-backtest', ['--catalog', f.options.catalogRoots[0]!, '--funding-catalog', f.directory]))).json();
    assert.equal(outsideFunding.error, 'nautilus_argument_unsupported');
    const waiting = f.request({ wait: true }, 'native-strategy-backtest', mounted, files);
    const active = f.post(waiting); await f.started(3);
    const refused = await f.post(f.request({}, 'native-strategy-backtest'));
    assert.equal(refused.status, 429); assert.equal((await refused.json()).admission, 'REJECTED');
    assert.equal((await (await fetch(`${f.url}/v1/executions/${waiting.requestId}`, { method: 'DELETE' })).json()).status, 'CANCELLED');
    assert.equal((await (await active).json()).status, 'RUNNING');
  } finally { await f.cleanup(); }
});
