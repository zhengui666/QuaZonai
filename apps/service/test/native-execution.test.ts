import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT, withAbortSignal } from '@earendil-works/chord/context';
import { nativeJob } from '../src/native.ts';

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
