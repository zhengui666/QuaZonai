import assert from 'node:assert/strict';
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
