import assert from 'node:assert/strict';
import { EventEmitter } from 'node:events';
import { test } from 'node:test';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import Fastify from 'fastify';
import { Watch } from '@earendil-works/chord';
import { awaitWithContext, BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels } from '@earendil-works/pi-ai';
import { createRegistry, defineExtension, defineTask, Harness } from '@earendil-works/pi-durable';
import { openNodeSqliteStorage } from '@earendil-works/pi-durable/storage/sqlite/node';
import { eventStream } from '../src/event-stream.ts';

const context = BACKGROUND_CONTEXT;
const Hold = defineTask<null, { phase: 'hold' }, null>({
  name: 'test.stream.independent-work', version: 1, initial: () => ({ phase: 'hold' }),
  phases: { async hold(_task, _runtime, ctx) { await awaitWithContext(new Promise<void>(() => {}), ctx); } },
  abort: (_task, runtime, ctx) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx),
});

test('a permanent official watch failure closes SSE and its peer watch while leaving the durable task running', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-event-close-'));
  const registry = createRegistry(); registry.install(defineExtension({ name: 'test.stream', tasks: [Hold] }));
  const harness = await Harness.open(await openNodeSqliteStorage(join(directory, 'durable.sqlite')), { registry, models: createModels() }, context);
  const streams = new Set<() => Promise<void>>(); const app = Fastify();
  const controller = new AbortController(); let detachCount = 0;
  try {
    const owner = await harness.root(context);
    const id = await owner.commit(tx => tx.createTask(Hold, null, { ownership: { kind: 'conversation' } }), context);
    harness.resume();
    const graph = await harness.watchTaskGraph(context);
    const failed = new Watch({ source: 'initial' }, () => { detachCount++; }, error => assert.fail(String(error)));
    app.get('/events', (_request, reply) => eventStream(reply, 'test', async () => ({ id, source: failed.value.source }), [failed, graph], streams));
    app.addHook('preClose', async () => { await Promise.all([...streams].map(close => close())); });
    const base = await app.listen({ host: '127.0.0.1', port: 0 });
    const response = await fetch(`${base}/events`, { signal: AbortSignal.any([controller.signal, AbortSignal.timeout(10_000)]) });
    assert.equal(response.status, 200);
    const reader = response.body!.getReader(); const first = await reader.read();
    assert.equal(first.done, false); assert.match(new TextDecoder().decode(first.value), /"source":"initial"/);
    // The official Watch sends terminal failure through .closed, without a value callback.
    failed.end({ kind: 'failed', error: new Error('private-source-failure') });
    while (!(await reader.read()).done) { /* Drain any committed event already in flight. */ }
    assert.equal((await failed.closed).kind, 'failed');
    assert.equal((await graph.closed).kind, 'stopped');
    assert.equal(detachCount, 1); assert.equal(streams.size, 0);
    const task = await harness.getTask(id, context);
    assert.ok(task && task.state.status !== 'terminal'); assert.equal(task.abortRequested, false);
    await harness.abortTask(id, context);
    assert.equal((await harness.waitForTask(id, context)).state.outcome.status, 'aborted');
  } finally {
    controller.abort(); await app.close(); await harness.close(context); await rm(directory, { recursive: true, force: true });
  }
});

test('SSE folds simultaneous latest-state invalidations but reads again after an in-flight snapshot', async () => {
  const raw = new EventEmitter() as EventEmitter & { writeHead: () => void; write: (data: string) => boolean; end: () => void };
  const output: string[] = []; let blocked = false;
  raw.writeHead = () => {};
  raw.write = data => { output.push(data); return !blocked; };
  raw.end = () => { raw.emit('close'); };
  const reply = { hijack() {}, raw } as unknown as import('fastify').FastifyReply;
  const streams = new Set<() => Promise<void>>();
  const watches = [new Watch(0, () => {}, error => assert.fail(String(error))), new Watch(0, () => {}, error => assert.fail(String(error)))];
  let version = 0; let reads = 0;
  const tick = () => new Promise<void>(resolve => setImmediate(resolve));
  const change = (watch: Watch<number>, value: number) => watch.advance({ kind: 'value', value, ops: [['r', value]], retired: false }, context);
  try {
    // Both sources may change between watch acquisition and subscription start.
    change(watches[0]!, 0); change(watches[1]!, 0);
    await eventStream(reply, 'test', async () => { reads++; return { version }; }, watches, streams);
    await tick();
    assert.equal(reads, 1, 'two initial watch callbacks and explicit first read share one projection');
    blocked = true; version = 1;
    change(watches[0]!, 1);
    await tick();
    assert.equal(reads, 2);
    version = 2; change(watches[1]!, 2);
    await tick();
    assert.equal(reads, 2, 'backpressure prevents reading ahead of the pending output');
    blocked = false; raw.emit('drain');
    await tick(); await tick();
    assert.equal(reads, 3, 'invalidation during drain schedules a fresh read');
    assert.deepEqual(output.map(data => JSON.parse(data.split('data: ')[1]!.trim()).version), [0, 1, 2]);
    await Promise.all([...streams].map(close => close()));
    version = 3; await tick();
    assert.equal(reads, 3); assert.equal(streams.size, 0);
    assert.equal((await watches[0]!.closed).kind, 'stopped');
    assert.equal((await watches[1]!.closed).kind, 'stopped');
  } finally { await Promise.all([...streams].map(close => close())); }
});

for (const stage of ['read', 'drain'] as const) {
  for (const disconnect of [false, true]) {
    test(`SSE ${disconnect ? 'closes safely' : 'keeps the newest state'} with invalidation during ${stage}`, { timeout: 5000 }, async () => {
      const raw = new EventEmitter() as EventEmitter & { writeHead: () => void; write: (data: string) => boolean; end: () => void };
      const output: number[] = []; let writesBlocked = stage === 'drain';
      raw.writeHead = () => {};
      raw.write = data => { output.push(JSON.parse(data.split('data: ')[1]!.trim()).version); return !writesBlocked; };
      raw.end = () => { raw.emit('close'); };
      const reply = { hijack() {}, raw } as unknown as import('fastify').FastifyReply;
      const streams = new Set<() => Promise<void>>(); let detached = 0;
      const watches = [new Watch(0, () => { detached++; }, error => assert.fail(String(error))), new Watch(0, () => { detached++; }, error => assert.fail(String(error)))];
      const tick = () => new Promise<void>(resolve => setImmediate(resolve));
      let releaseRead!: () => void;
      const blockedRead = new Promise<void>(resolve => { releaseRead = resolve; });
      let reads = 0; let version = 0;
      const serving = eventStream(reply, 'test', async () => {
        reads++; const captured = version;
        if (reads === 1 && stage === 'read') await blockedRead;
        return { version: captured };
      }, watches, streams);
      try {
        await tick();
        assert.equal(reads, 1);
        for (const [index, watch] of watches.entries()) {
          version = index + 1;
          watch.advance({ kind: 'value', value: version, ops: [['r', version]], retired: false }, context);
        }
        await tick();
        assert.equal(reads, 1, 'no concurrent projection or read ahead of backpressure');
        if (disconnect) {
          raw.emit('close');
          await tick();
          assert.equal(detached, 2); assert.equal(streams.size, 0);
        }
        releaseRead(); writesBlocked = false;
        if (!disconnect) raw.emit('drain');
        await serving; await tick(); await tick();
        if (disconnect) {
          assert.equal(reads, 1, 'queued reads do not run after disconnection');
          assert.deepEqual(output, stage === 'read' ? [] : [0], 'in-flight reads do not write after disconnection');
        } else {
          assert.equal(reads, 2, 'both invalidations share one subsequent latest-state projection');
          assert.deepEqual(output, [0, 2], 'newest state follows the captured older snapshot');
        }
        assert.equal(raw.listenerCount('drain'), 0);
        await Promise.all([...streams].map(close => close()));
        assert.equal(detached, 2); assert.equal(streams.size, 0);
        for (const watch of watches) assert.equal((await watch.closed).kind, 'stopped');
      } finally {
        releaseRead(); writesBlocked = false; raw.emit('drain');
        await Promise.all([...streams].map(close => close())); await serving;
      }
    });
  }
}
