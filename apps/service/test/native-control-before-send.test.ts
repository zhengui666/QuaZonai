import assert from 'node:assert/strict';
import test from 'node:test';
import { spawn } from 'node:child_process';
import { mkdtemp, rm, writeFile, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import { defineTask, defineExtension } from '@earendil-works/pi-durable';
import { openResearch } from '../../research/src/host.ts';
import { Application, json } from '../src/facts.ts';
import { nativeStrategyControlPhases, admitNativeControlTask, type NativeControlInput } from '../src/native-strategy-control-phases.ts';
import { nativeStrategyControl } from '../../../packages/contracts/src/native-strategy-control.ts';
const action = process.env.QZ_CONTROL_BEFORE_SEND_ACTION === 'CANCEL' ? 'CANCEL' : 'STOP';
const originalFor = (action: 'STOP' | 'CANCEL') => nativeStrategyControl({ schemaVersion: 1, requestId: 'crash-request', action, ...(action === 'CANCEL' ? { order: { traderId: 'TRADER-001', accountId: 'VENUE-001', instrumentId: 'BTC-USDC-SPOT.HYPERLIQUID', clientOrderId: 'ORIGINAL-CLOID' } } : {}), strategyId: 'ORIGINAL-001', binding: { sourceId: 'source', environmentId: 'environment', buildId: 'build', nativeSessionId: 'session' } });
const original = originalFor(action);
let attemptedResends = 0, reads = 0;
async function open(directory: string, worker: boolean, original = originalFor(action)) {
  const models = createModels(); models.setProvider(fauxProvider().provider);
  const phases = nativeStrategyControlPhases({
    async send() { attemptedResends++; await writeFile(join(directory, "unexpected-send"), "called"); throw Error("MUST_NOT_DISPATCH"); },
    async read(command) { reads++; assert.deepEqual(command, original); return json({command,status:"UNKNOWN",error:"owner killed before transport invocation"}); },
  });
  const Task = defineTask<NativeControlInput, { phase: 'send' | 'read' }, null>({
    name: 'qz.account-strategy-command', version: 1, initial: () => ({ phase: 'send' }),
    phases: { ...phases, async send(task, runtime, ctx) {
      if (!worker) return phases.send(task, runtime, ctx);
      // Only this test pauses AFTER the real durable commit resolves and BEFORE
      // the production phase can call transport.send. No production hook.
      const paused = new Proxy(runtime, { get(target, key) {
        if (key === 'commit') return async (...args: unknown[]) => {
          await (target.commit as Function).apply(target, args);
          await writeFile(join(directory, 'checkpoint-committed'), JSON.stringify(original), {flag:'wx'});
          await new Promise(() => {});
        };
        const value=Reflect.get(target,key,target);return typeof value==='function'?value.bind(target):value;
      }});
      return phases.send(task, paused, ctx);
    }},
    abort: (_task, runtime, ctx) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx),
  });
  const host = await openResearch({ database: join(directory, 'original.sqlite'), models, deferResume: true, extensions: [defineExtension({ name: 'original-account-crash-test', tasks: [Task] })] });
  return { host, Task };
}
if (process.env.QZ_CONTROL_BEFORE_SEND_DIRECTORY) {
  const directory = process.env.QZ_CONTROL_BEFORE_SEND_DIRECTORY;
  const { host, Task } = await open(directory, true);
  const root = await host.harness.root(context);
  await root.commit(tx => admitNativeControlTask(tx, Task, { key: 'original', original }, { conversationId: root.id, ownership: { kind: 'conversation' } }), context);
  host.harness.resume();
  // Keep only the test owner process alive until its parent kills it.
  setInterval(() => {}, 1000);
} else for (const action of ['STOP', 'CANCEL'] as const) test(`SIGKILL before ${action} transport invocation preserves original task checkpoint and never resends`, { timeout: 30000 }, async () => {
  const original = originalFor(action); attemptedResends = 0; reads = 0;
  const directory = await mkdtemp(join(tmpdir(), 'qz-control-before-send-'));
  const child = spawn(process.execPath, ['--experimental-strip-types', fileURLToPath(import.meta.url)], { env: { ...process.env, QZ_CONTROL_BEFORE_SEND_DIRECTORY: directory, QZ_CONTROL_BEFORE_SEND_ACTION: action }, stdio: ['ignore', 'pipe', 'pipe'] });
  let output = ''; child.stdout.on('data', value => output += value); child.stderr.on('data', value => output += value);
  const exited = new Promise<{ code: number | null; signal: string | null }>(resolve => child.once('exit', (code, signal) => resolve({ code, signal })));
  try {
    let committed = '';
    for (let i = 0; i < 500; i++) {
      committed = await readFile(join(directory, 'checkpoint-committed'), 'utf8').catch(() => '');
      if (committed || child.exitCode !== null) break;
      await delay(10);
    }
    assert.deepEqual(JSON.parse(committed || '{}'), original, output);
    assert.equal(child.kill('SIGKILL'), true);
    assert.equal((await exited).signal, 'SIGKILL');
    const { host } = await open(directory, false, original);
    try {
      const before = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.account-strategy-command' }, 10), context);
      assert.equal(before.items.length, 1);
      assert.equal((before.items[0]!.state as unknown as { checkpoint: { phase: string } }).checkpoint.phase, 'read');
      const retained = (await host.harness.snapshot(Application, context))!.strategyCommands!.original!;
      assert.equal(retained.status, 'UNCERTAIN'); assert.deepEqual(retained.original, original);
      host.harness.resume(); await host.harness.waitForTask(before.items[0]!.id, context);
      assert.equal((await host.harness.snapshot(Application, context))!.strategyCommands!.original!.status, 'UNCERTAIN');
      assert.equal(attemptedResends, 0);assert.equal(reads,1);
      await assert.rejects(readFile(join(directory,'unexpected-send')),/ENOENT/);
      assert.equal(await readFile(join(directory, 'checkpoint-committed'), 'utf8'), committed);
    } finally { await host.close(); }
  } finally { if (child.exitCode === null) child.kill('SIGKILL'); await exited; await rm(directory, { recursive: true, force: true }); }
});
