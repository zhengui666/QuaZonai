import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT, withAbortSignal } from '@earendil-works/chord/context';
import type { TaskId } from '@earendil-works/pi-durable';
import { nativeJob } from '../src/native.ts';
import { json, Problem } from '../src/facts.ts';
import { classifyExecutionError, ExecutionFailure } from '../../research/src/execution-errors.ts';
import { once } from 'node:events';
import type { AddressInfo } from 'node:net';
import { createNautilusWorker } from '../../nautilus/src/server.ts';
import { withNativeProgress } from '../../research/src/native-execution.ts';
import { built, setup, wat } from './support.ts';

test('native execution has no application deadline and owner abort joins the real child', { timeout: 10_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-execution-test-')); const binary = join(directory, 'controlled-child'); const script = join(directory, 'child.mjs');
  try {
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${script}' "$@"\n`, { mode: 0o700 });
    await writeFile(script, `setTimeout(() => console.log(JSON.stringify({completed:true, secret:process.env.QZ_TEST_PROVIDER_SECRET ?? null})), 120);\n`);
    process.env.QZ_TEST_PROVIDER_SECRET = 'synthetic-child-isolation-only';
    assert.deepEqual(await nativeJob(binary, 'forecast', {}, [], directory, BACKGROUND_CONTEXT), { completed: true, secret: null });
    const pidPath = join(directory, 'child.pid');
    await writeFile(script, `import {writeFileSync} from 'node:fs'; writeFileSync(${JSON.stringify(pidPath)}, String(process.pid)); setInterval(() => {}, 1000);\n`);
    const controller = new AbortController();
    const pending = nativeJob(binary, 'forecast', {}, [], directory, withAbortSignal(controller.signal, BACKGROUND_CONTEXT));
    const rejected = assert.rejects(pending);
    let pid: number;
    for (;;) { try { pid = Number(await readFile(pidPath, 'utf8')); break; } catch (error) { if ((error as NodeJS.ErrnoException).code !== 'ENOENT') throw error; await new Promise(resolve => setTimeout(resolve, 5)); } }
    controller.abort(); await rejected;
    assert.throws(() => process.kill(pid, 0), (error: unknown) => (error as NodeJS.ErrnoException).code === 'ESRCH');
  } finally { delete process.env.QZ_TEST_PROVIDER_SECRET; await rm(directory, { recursive: true, force: true }); }
});


test('remote live observation reads the original attached invocation and preview failure cannot fail final science', { timeout: 10_000 }, async () => {
  const host = await setup();
  const directory = await mkdtemp(join(tmpdir(), 'qz-live-remote-')); const binary = join(directory, 'job'); const script = join(directory, 'job.mjs');
  const snapshot = { ts_event: '1790000000000000001', total_equity: ['1000.000000000000000001 USD'] };
  let worker;
  try {
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${script}' "$@"\n`, { mode: 0o700 });
    await writeFile(script, `import {writeFileSync} from 'node:fs';
writeFileSync(process.env.QZ_PROGRESS_PATH, JSON.stringify({snapshot:${JSON.stringify(snapshot)}})+'\\n');
setTimeout(()=>console.log(JSON.stringify({completed:true})),1800);`);
    worker = await createNautilusWorker({ binary, dataDir: join(directory, 'worker'), catalogRoots: [directory] });
    worker.server.listen(0, '127.0.0.1'); await once(worker.server, 'listening');
    const target = { id: 'live-original', url: `http://127.0.0.1:${(worker.server.address() as AddressInfo).port}` };
    let completed = false; let observations = 0;
    const result = await withNativeProgress(async snapshots => {
      assert.equal(completed, false); assert.deepEqual(snapshots, [snapshot]); observations++;
      throw new Error('optional preview unavailable');
    }, () => host.service.research.withNativeHost(() => nativeJob(target, 'simulate', {}, [], directory, BACKGROUND_CONTEXT)));
    completed = true; assert.deepEqual(result, { completed: true }); assert.ok(observations > 0);
    // The local explicit test executable uses the same optional observation policy.
    observations = 0;
    assert.deepEqual(await withNativeProgress(async () => { observations++; throw new Error('optional preview unavailable'); },
      () => nativeJob(binary, 'simulate', {}, [], directory, BACKGROUND_CONTEXT)), { completed: true });
    assert.ok(observations > 0);
  } finally { await worker?.close(); await host.cleanup(); await rm(directory, { recursive: true, force: true }); }
});
