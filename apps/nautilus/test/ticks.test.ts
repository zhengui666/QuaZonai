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

// Controlled processes establish resource/protocol behavior, not factor or PIT evidence.
async function fixture(overrides: Partial<WorkerOptions> = {}, companion = false) {
  const directory = await mkdtemp(join(tmpdir(), 'qz-tick-worker-'));
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
    while True: time.sleep(0.05)
limits=resource.getrlimit(resource.RLIMIT_AS)
raised=False
if request.get('raiseLimit'):
    try: resource.setrlimit(resource.RLIMIT_AS, (limits[1]+4096,limits[1]+4096)); raised=True
    except (ValueError, OSError): pass
allocation='not_requested'
if request.get('allocateBeyondLimit'):
    try:
        region=mmap.mmap(-1,limits[1]+4096); region.close(); allocation='allowed'
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
  function request(input: NativeExecutionRequest['input'], command = 'evaluate-tick-factor', args: string[] = [], files: Record<string, string> = {}) {
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
    post(body: NativeExecutionRequest) { return fetch(`${url}/v1/execute`, { method: 'POST', headers, body: JSON.stringify(body) }); },
    async restart() { await worker.close(); worker = await createNautilusWorker(options); url = await listen(); },
    async cleanup() { await worker.close(); await rm(directory, { recursive: true, force: true }); },
  };
}

test('tick command inherits the hard owner memory bound; request JSON cannot raise it', async () => {
  const f = await fixture();
  try {
    const caps = await (await fetch(`${f.url}/v1/capabilities`)).json();
    assert.ok(caps.commands.includes('evaluate-tick-factor'));
    const body = f.request({ raiseLimit: true, allocateBeyondLimit: true, tickMaximumMemoryBytes: 16 * 1024 ** 3 });
    const original = await (await f.post(body)).json();
    assert.equal(original.status, 'COMPLETED');
    assert.deepEqual(original.result.limits, [2 * 1024 ** 3, 2 * 1024 ** 3]);
    assert.equal(original.result.raised, false); assert.equal(original.result.allocation, 'rejected');
    assert.equal(original.result.authLeaked, false); assert.deepEqual(original.result.input, body.input);
    await f.restart();
    assert.deepEqual(await (await f.post(body)).json(), original);
    assert.equal((await readFile(f.calls, 'utf8')).trim().split('\n').length, 1);
  } finally { await f.cleanup(); }
});

test('tick capacity rejects before creating a receipt; exact replay and BAR operations still work', async () => {
  const f = await fixture();
  try {
    const completed = f.request({ earlier: true }); const original = await (await f.post(completed)).json();
    const waiting = f.request({ wait: true }); const first = f.post(waiting); const pid = await f.started(2);
    const duplicate = f.post(waiting); const next = f.request({ next: true });
    const busy = await f.post(next);
    assert.equal(busy.status, 429); assert.deepEqual(await busy.json(), { schemaVersion: 1, requestId: next.requestId, admission: 'REJECTED', error: 'nautilus_tick_capacity_exhausted' });
    assert.equal((await fetch(`${f.url}/v1/executions/${next.requestId}`)).status, 404);
    assert.deepEqual(await (await f.post(completed)).json(), original);
    const bar = await (await f.post(f.request({}, 'evaluate-factor'))).json();
    assert.equal(bar.status, 'COMPLETED');
    assert.notEqual(bar.result.limits[1], 2 * 1024 ** 3, 'tick-only limit does not change BAR process limits');
    const cancelled = await (await fetch(`${f.url}/v1/executions/${waiting.requestId}`, { method: 'DELETE' })).json();
    assert.equal(cancelled.status, 'CANCELLED'); assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    assert.deepEqual(await (await first).json(), cancelled); assert.deepEqual(await (await duplicate).json(), cancelled);
    assert.equal((await (await f.post(next)).json()).status, 'COMPLETED');
  } finally { await f.cleanup(); }
});

test('tick output limits stop and join the controlled child and retain the failed receipt', async () => {
  const f = await fixture({ tickMaximumOutputBytes: 1024, tickMaximumMemoryBytes: 256 * 1024 ** 2 });
  try {
    const body = f.request({ large: true }); const response = f.post(body); const pid = await f.started();
    const failure = await (await response).json();
    assert.equal(failure.status, 'FAILED'); assert.equal(failure.error, 'nautilus_tick_output_limit_exceeded');
    assert.throws(() => process.kill(pid, 0), { code: 'ESRCH' });
    assert.deepEqual(await (await f.post(body)).json(), failure);
    const next = await (await f.post(f.request({}))).json();
    assert.equal(next.status, 'COMPLETED'); assert.deepEqual(next.result.limits, [256 * 1024 ** 2, 256 * 1024 ** 2]);
  } finally { await f.cleanup(); }
});

test('worker resource configuration rejects nonpositive and unsafe limits', async () => {
  for (const overrides of [{ tickMaximumMemoryBytes: 0 }, { maximumConcurrentTickExecutions: 0 }, { tickMaximumOutputBytes: Number.MAX_SAFE_INTEGER + 1 }]) {
    await assert.rejects(createNautilusWorker({ binary: '/does-not-exist', dataDir: '/does-not-exist', catalogRoots: [], ...overrides }), /nautilus_tick_.*invalid/);
  }
});

test('explicit precision companions route tick-only and bind both native files without changing legacy identity', async () => {
  const single = await fixture();
  try {
    const caps = await (await fetch(`${single.url}/v1/capabilities`)).json();
    assert.equal(caps.implementationDigest, `sha256:${createHash('sha256').update(await readFile(single.options.binary)).digest('hex')}`);
  } finally { await single.cleanup(); }
  const f = await fixture({}, true);
  try {
    const before = await (await fetch(`${f.url}/v1/capabilities`)).json();
    const tick = await (await f.post(f.request({}))).json();
    assert.equal(tick.status, 'COMPLETED'); assert.equal(tick.result.executable, 'job-high-precision');
    const barBody = f.request({}, 'evaluate-factor'); const bar = await (await f.post(barBody)).json();
    assert.equal(bar.status, 'COMPLETED'); assert.equal(bar.result.executable, 'job');
    await writeFile(f.options.tickBinary!, await readFile(f.options.tickBinary!, 'utf8') + '\n# changed companion bytes\n');
    const changed = await (await f.post(f.request({}, 'evaluate-factor'))).json();
    assert.equal(changed.status, 'FAILED'); assert.equal(changed.error, 'nautilus_implementation_changed');
    assert.equal((await readFile(f.calls, 'utf8')).trim().split('\n').length, 2, 'a changed companion cannot execute even an old standard identity');
    await f.restart();
    const after = await (await fetch(`${f.url}/v1/capabilities`)).json();
    assert.notEqual(after.implementationDigest, before.implementationDigest);
    assert.equal((await f.post(barBody)).status, 409, 'the new executable pair cannot borrow an old POST receipt identity');
    assert.deepEqual(await (await fetch(`${f.url}/v1/executions/${barBody.requestId}`)).json(), bar, 'historical original receipts remain readable');
    assert.equal((await (await f.post(f.request({}))).json()).status, 'COMPLETED');
    await writeFile(f.options.binary, await readFile(f.options.binary, 'utf8') + '\n# changed standard bytes\n');
    const standardChanged = await (await f.post(f.request({}))).json();
    assert.equal(standardChanged.status, 'FAILED'); assert.equal(standardChanged.error, 'nautilus_implementation_changed');
  } finally { await f.cleanup(); }
});


test('economic replay shares tick precision, mounted catalogs, hard limits and admission slots', async () => {
  const f = await fixture({}, true);
  try {
    const caps = await (await fetch(`${f.url}/v1/capabilities`)).json();
    assert.ok(caps.commands.includes('replay-tick-economic'));
    const mounted = ['--catalog', f.options.catalogRoots[0]!, '--funding-catalog', f.options.catalogRoots[0]!, '--model', '@file:model'];
    const files = { model: Buffer.from('controlled model artifact').toString('base64') };
    const result = await (await f.post(f.request({}, 'replay-tick-economic', mounted, files))).json();
    assert.equal(result.status, 'COMPLETED'); assert.equal(result.result.executable, 'job-high-precision');
    assert.deepEqual(result.result.limits, [2 * 1024 ** 3, 2 * 1024 ** 3]);
    const outside = await (await f.post(f.request({}, 'replay-tick-economic', ['--catalog', f.directory]))).json();
    assert.equal(outside.status, 'FAILED'); assert.equal(outside.error, 'nautilus_catalog_outside_configured_roots');
    const outsideFunding = await (await f.post(f.request({}, 'replay-tick-economic', ['--catalog', f.options.catalogRoots[0]!, '--funding-catalog', f.directory]))).json();
    assert.equal(outsideFunding.error, 'nautilus_catalog_outside_configured_roots');
    const waiting = f.request({ wait: true }, 'replay-tick-economic', mounted, files);
    const active = f.post(waiting); await f.started(2);
    const refused = await f.post(f.request({}));
    assert.equal(refused.status, 429); assert.equal((await refused.json()).admission, 'REJECTED');
    assert.equal((await (await fetch(`${f.url}/v1/executions/${waiting.requestId}`, { method: 'DELETE' })).json()).status, 'CANCELLED');
    assert.equal((await (await active).json()).status, 'CANCELLED');
  } finally { await f.cleanup(); }
});
