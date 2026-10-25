import test from 'node:test';
import assert from 'node:assert/strict';
import { once } from 'node:events';
import { mkdtemp, mkdir, readFile, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import type { AddressInfo } from 'node:net';
import { randomUUID } from 'node:crypto';
import { createNautilusWorker, executionId } from '../src/server.ts';
import type { NativeExecutionRequest, NativeExecutionReceipt } from '../../../packages/contracts/src/nautilus.ts';

async function fixture() {
  const directory = await mkdtemp(join(tmpdir(), 'qz-remote-worker-'));
  const catalog = join(directory, 'catalog'); await mkdir(catalog);
  const count = join(directory, 'calls'); const binary = join(directory, 'job');
  const script = join(directory, 'job.mjs');
  await writeFile(script, `import fs from 'node:fs';
fs.appendFileSync(${JSON.stringify(count)}, process.pid + '\\n');
let text=''; for await (const chunk of process.stdin) text+=chunk;
const input=JSON.parse(text); const args=process.argv.slice(3);
if(input.wait) { console.error('waiting'); setInterval(()=>{},1000); }
else if(input.fail) { console.error('NATIVE_CONTROLLED_FAILURE'); process.exitCode=2; }
else if(input.invalid) { console.log('invalid'); }
else { const contents={}; for(let i=0;i<args.length;i++) if(['--model','--scope'].includes(args[i])) contents[args[i]]=fs.readFileSync(args[++i],'utf8');
const objects=args.indexOf('--objects'); if(objects>=0) contents.objects=fs.readdirSync(args[objects+1]).sort();
console.log(JSON.stringify({input,args,contents,authLeaked:!!process.env.QZ_NAUTILUS_TOKEN})); }
`);
  await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${script}' "$@"\n`, { mode: 0o700 });
  const options = { binary, dataDir: join(directory, 'state'), catalogRoots: [catalog], token: 'test-only-worker-token' };
  let worker = await createNautilusWorker(options);
  async function listen() { worker.server.listen(0, '127.0.0.1'); await once(worker.server, 'listening'); return `http://127.0.0.1:${(worker.server.address() as AddressInfo).port}`; }
  let url = await listen();
  const headers = { authorization: `Bearer ${options.token}`, 'content-type': 'application/json' };
  function request(input: NativeExecutionRequest['input'], args: string[] = [], files: Record<string, string> = {}): NativeExecutionRequest {
    const body = { schemaVersion: 1 as const, invocationId: randomUUID(), implementationDigest: worker.implementationDigest, command: 'allocate', input, args, files };
    return { ...body, requestId: executionId(body) };
  }
  return { directory, catalog, count, options, headers, request, get worker() { return worker; }, get url() { return url; },
    async post(body: NativeExecutionRequest) { return fetch(`${url}/v1/execute`, { method: 'POST', headers, body: JSON.stringify(body) }); },
    async restart() { await worker.close(); worker = await createNautilusWorker(options); url = await listen(); },
    async cleanup() { await worker.close(); await rm(directory, { recursive: true, force: true }); },
  };
}

test('remote worker retains actual execution, attachments and original receipt across retries and restart', async () => {
  const f = await fixture();
  try {
    assert.equal((await fetch(`${f.url}/v1/capabilities`)).status, 401);
    const caps = await (await fetch(`${f.url}/v1/capabilities`, { headers: f.headers })).json();
    assert.match(caps.implementationDigest, /^sha256:[a-f0-9]{64}$/);
    assert.ok(caps.commands.includes('simulate')); assert.ok(!caps.commands.includes('paper'));
    const body = f.request({ decimal: '1.000000000000000001', ns: '1790000000000000000' },
      ['--catalog', f.catalog, '--model', '@file:model-0', '--scope', '@file:scope-0', '--objects', '@objects:objects-0'],
      { 'model-0': Buffer.from('wasm-bytes').toString('base64'), 'scope-0': Buffer.from('["instrument"]').toString('base64'), 'objects-0/uuid': Buffer.from('calibration').toString('base64') });
    const responses = await Promise.all([f.post(body), f.post(body)]);
    const [first, duplicate] = await Promise.all(responses.map(response => response.json()));
    assert.equal(first.status, 'COMPLETED'); assert.deepEqual(duplicate, first);
    assert.deepEqual(first.result.input, body.input);
    assert.deepEqual(first.result.contents, { '--model': 'wasm-bytes', '--scope': '["instrument"]', objects: ['uuid'] });
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
    assert.equal(missing.status, 'FAILED'); assert.equal(missing.error, 'nautilus_attachment_required');
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
    assert.deepEqual(await (await pending).json(), cancelled);
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

test('disconnect stops the actual child and independent invocations do not reuse prior input results', async () => {
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
    controller.abort(); await assert.rejects(pending, { name: 'AbortError' });
    let receipt: NativeExecutionReceipt | undefined;
    for (let attempt = 0; attempt < 100; attempt++) {
      receipt = await (await fetch(`${f.url}/v1/executions/${body.requestId}`, { headers: f.headers })).json();
      if (receipt?.status !== 'RUNNING') break;
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    assert.equal(receipt?.status, 'CANCELLED');
    assert.throws(() => process.kill(pid!, 0), { code: 'ESRCH' });
  } finally { await f.cleanup(); }
});
