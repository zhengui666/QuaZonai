import assert from 'node:assert/strict';
import { test } from 'node:test';
import { awaitWithContext } from '@earendil-works/chord/context';
import { defineExtension, defineTask, type ConversationId } from '@earendil-works/pi-durable';
import { setup, studied, built, context } from './support.ts';
import { Application, json } from '../src/facts.ts';

const Hold = defineTask<{ label: string }, { phase: 'hold' }, null>({
  name: 'test.events.hold', version: 1, initial: () => ({ phase: 'hold' }),
  phases: { async hold(_task, _runtime, ctx) { await awaitWithContext(new Promise<void>(() => {}), ctx); } },
  async abort(_task, runtime, ctx) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), ctx); },
});
function events(response: Response) {
  assert.equal(response.status, 200); assert.match(response.headers.get('content-type')!, /text\/event-stream/);
  const reader = response.body!.getReader(); const decoder = new TextDecoder(); let buffer = '';
  return { async until(predicate: (value: any) => boolean) {
    for (;;) {
      const boundary = buffer.indexOf('\n\n');
      if (boundary !== -1) {
        const packet = buffer.slice(0, boundary); buffer = buffer.slice(boundary + 2);
        const data = packet.split('\n').find(line => line.startsWith('data: '));
        if (data) { const value = JSON.parse(data.slice(6)); if (predicate(value)) return value; }
        continue;
      }
      const next = await reader.read(); assert.equal(next.done, false, 'the subscription must remain open'); buffer += decoder.decode(next.value, { stream: true });
    }
  }, async close() { await reader.cancel(); } };
}

test('official research watches stream committed task changes and HTTP disconnect leaves durable work running', { timeout: 20_000 }, async () => {
  const state = await setup();
  state.options.extensions.push(defineExtension({ name: 'test.events', tasks: [Hold] }));
  await state.restart();
  try {
    const { receipt } = await studied(state);
    const base = await state.service.app.listen({ host: '127.0.0.1', port: 0 });
    const response = await fetch(`${base}/api/v3/research/${receipt.conversationId}/events`, { headers: state.headers, signal: AbortSignal.timeout(15_000) });
    const stream = events(response);
    assert.equal((await stream.until(value => value.id === receipt.conversationId)).id, receipt.conversationId);
    const taskId = await state.service.research.harness.commit(tx => tx.createTask(Hold, { label: 'independently owned work' },
      { conversationId: receipt.conversationId as ConversationId, ownership: { kind: 'conversation' }, background: true }), context);
    const changed = await stream.until(value => value.tasks.some((task: { id: number; status: string }) => task.id === taskId && task.status === 'running'));
    assert.ok(changed.tasks.some((task: { id: number }) => task.id === taskId));
    await stream.close();
    assert.notEqual((await state.service.research.harness.getTask(taskId, context))!.state.status, 'terminal');
    await state.service.research.harness.abortTask(taskId, context);
  } finally { await state.cleanup(); }
});

test('portfolio watches stream the original public view and committed presentation changes', { timeout: 20_000 }, async () => {
  const state = await setup();
  try {
    const { view } = await built(state);
    const base = await state.service.app.listen({ host: '127.0.0.1', port: 0 });
    const response = await fetch(`${base}/api/v3/portfolios/${view.id}/events`, { headers: state.headers, signal: AbortSignal.timeout(15_000) });
    const stream = events(response);
    assert.equal((await stream.until(value => value.id === view.id)).status, 'COMPUTED');
    await state.service.research.harness.commit(async tx => {
      const facts = await tx.doc(Application); const fact = json(facts.portfolios[view.id]!);
      (fact.view as Record<string, unknown>).name = 'Updated presentation'; facts.portfolios[view.id] = fact;
    }, context);
    assert.equal((await stream.until(value => value.name === 'Updated presentation')).id, view.id);
    await stream.close();
  } finally { await state.cleanup(); }
});

test('data source watches stream committed configuration through the public task-status envelope', { timeout: 15_000 }, async () => {
  const state = await setup();
  try {
    const base = await state.service.app.listen({ host: '127.0.0.1', port: 0 });
    const response = await fetch(`${base}/api/v3/data-sources/qz.data.local-config/events`, { headers: state.headers, signal: AbortSignal.timeout(10_000) });
    const stream = events(response);
    const initial = await stream.until(value => value.source?.name === 'qz.data.local-config');
    assert.equal(initial.status, 'idle'); assert.equal(initial.taskId, null);
    const saved = await state.service.app.inject({ method: 'PUT', url: '/api/v3/data-sources/qz.data.local-config', headers: state.headers,
      payload: { enabled: false, config: { path: 'next-owner-source.json' } } });
    assert.equal(saved.statusCode, 200, saved.body);
    const changed = await stream.until(value => value.source?.config.path === 'next-owner-source.json');
    assert.equal(changed.source.enabled, false); assert.equal(changed.status, 'idle');
    await stream.close();
  } finally { await state.cleanup(); }
});
