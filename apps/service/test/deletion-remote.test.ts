import assert from 'node:assert/strict';
import { createServer, type ServerResponse } from 'node:http';
import { test } from 'node:test';
import type { ConversationId, TaskId } from '@earendil-works/pi-durable';
import type { NativeExecutionReceipt, RemoteNativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import { NativeRemoteUncertainty } from '../../research/src/native-execution.ts';
import { Study } from '../../research/src/documents.ts';
import { portfolio } from '../src/facts.ts';
import { context, setup, studied, built } from './support.ts';

type State = Awaited<ReturnType<typeof setup>>;
type Reply = NativeExecutionReceipt['status'] | 'not_found' | 'unavailable' | 'wrong_identity' | 'malformed';
function respond(response: ServerResponse, value: unknown, status = 200) {
  response.writeHead(status, { 'content-type': 'application/json' }); response.end(JSON.stringify(value));
}
async function peer(requestId: string) {
  const requests: string[] = []; let result: Reply = 'RUNNING';
  const server = createServer((request, response) => {
    requests.push(`${request.method} ${request.url}`);
    if (request.method !== 'GET' || request.url !== `/v1/executions/${requestId}`) return respond(response, { error: 'unexpected_request' }, 405);
    if (result === 'not_found') return respond(response, { error: 'missing_original_receipt' }, 404);
    if (result === 'unavailable') return respond(response, { error: 'original_instance_unavailable' }, 503);
    if (result === 'wrong_identity') return respond(response, { schemaVersion: 1, requestId: 'f'.repeat(64), status: 'CANCELLED' });
    if (result === 'malformed') return respond(response, { schemaVersion: 1, requestId, status: 'COMPLETED' });
    respond(response, { schemaVersion: 1, requestId, status: result,
      ...(result === 'UNKNOWN' || result === 'FAILED' ? { error: 'controlled_original_execution' } : {}),
      ...(result === 'COMPLETED' ? { result: { protocolOnly: true } } : {}) });
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target: RemoteNativeTarget = { id: 'original-deletion-receipt-instance', url: `http://127.0.0.1:${address.port}` };
  return { target, requests, set(value: Reply) { result = value; },
    async close() { server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); } };
}

// These are explicit saved transport-reference fixtures on real, already
// terminal Durable tasks. The real HTTP/Durable checks establish deletion and
// original-receipt reconciliation, not a scientific run or a remote cancellation.
async function retain(state: State, taskId: number, requestId: string, target: RemoteNativeTarget, status = 'PENDING') {
  const task = await state.service.research.harness.getTask(taskId as TaskId, context);
  assert.ok(task); assert.equal(task.state.status, 'terminal');
  const reference = { taskId, requestId, target, status, error: 'nautilus_execution_unknown' };
  await state.service.research.harness.commit(async tx => {
    (await tx.doc(NativeRemoteUncertainty, task.conversationId, String(taskId), null)).requests = [reference];
  }, context);
  return { conversationId: task.conversationId, taskId, reference };
}
async function references(state: State, task: { conversationId: number; taskId: number }) {
  return state.service.research.harness.snapshot(NativeRemoteUncertainty, task.conversationId as ConversationId, String(task.taskId), context);
}
async function remove(state: State, collection: string, id: string | number) {
  return state.service.app.inject({ method: 'DELETE', url: `/api/v3/${collection}/${encodeURIComponent(String(id))}`, headers: state.headers });
}
function unsettled(response: Awaited<ReturnType<typeof remove>>) {
  assert.equal(response.statusCode, 409, response.body); assert.equal(response.json().error, 'native_remote_execution_unsettled');
}
function onlyOriginalGets(requests: string[], requestId: string) {
  assert.ok(requests.length > 0); assert.ok(requests.every(request => request === `GET /v1/executions/${requestId}`), JSON.stringify(requests));
}

test('research deletion retains terminal-local original remote uncertainty through failed reads and restart until the exact receipt is terminal', async () => {
  const state = await setup(); const requestId = 'a'.repeat(64); const source = await peer(requestId);
  const replacement = await peer(requestId);
  try {
    const { input, receipt } = await studied(state);
    const original = structuredClone((await state.service.research.harness.snapshot(Study, receipt.conversationId, context))!);
    const task = await retain(state, Number(original.trials[0]!.taskId), requestId, source.target);
    for (const result of ['RUNNING', 'UNKNOWN', 'not_found', 'unavailable', 'wrong_identity', 'malformed'] as const) {
      source.set(result); unsettled(await remove(state, 'research', receipt.conversationId));
      assert.deepEqual((await references(state, task))!.requests, [task.reference]);
      assert.equal((await state.service.research.harness.snapshot(Study, receipt.conversationId, context))!.deletedAt, undefined);
    }
    state.options.config.nautilus = { backtest: { ...replacement.target, id: 'newly-configured-different-instance' } };
    await state.restart();
    assert.deepEqual((await references(state, task))!.requests, [task.reference]);
    source.set('RUNNING'); unsettled(await remove(state, 'research', receipt.conversationId));
    source.set('CANCELLED'); const deleted = await remove(state, 'research', receipt.conversationId);
    assert.equal(deleted.statusCode, 200, deleted.body);
    const stored = (await references(state, task))!.requests[0]!;
    assert.equal(stored.status, 'CANCELLED'); assert.equal(stored.requestId, requestId); assert.deepEqual(stored.target, source.target);
    const after = (await state.service.research.harness.snapshot(Study, receipt.conversationId, context))!;
    assert.deepEqual(after.trials, original.trials); assert.deepEqual(after.final, original.final);
    const replay = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: input });
    assert.equal(replay.statusCode, 409); assert.equal(replay.json().error, 'research_deleted');
    const reads = source.requests.length;
    assert.deepEqual((await remove(state, 'research', receipt.conversationId)).json(), deleted.json());
    assert.equal(source.requests.length, reads, 'a known settled original receipt is reused without a replacement call');
    onlyOriginalGets(source.requests, requestId);
    assert.deepEqual(replacement.requests, [], 'new service configuration cannot redirect an original retained execution');
  } finally { await source.close(); await replacement.close(); await state.cleanup(); }
});

test('factor deletion cannot hide an unknown remote invocation on its locally terminal study and accepts original completed/failed receipts', async () => {
  for (const terminal of ['COMPLETED', 'FAILED'] as const) {
    const state = await setup(); const requestId = (terminal === 'COMPLETED' ? 'b' : 'c').repeat(64); const source = await peer(requestId);
    try {
      const { receipt } = await studied(state);
      const factors = (await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers })).json();
      assert.equal(factors.length, 1); const factor = factors[0]!;
      const task = await retain(state, factor.trialId, requestId, source.target, 'UNKNOWN');
      source.set('UNKNOWN'); unsettled(await remove(state, 'factors', factor.id));
      assert.deepEqual((await references(state, task))!.requests, [task.reference]);
      await state.restart();
      source.set('RUNNING'); unsettled(await remove(state, 'factors', factor.id));
      source.set(terminal); const deleted = await remove(state, 'factors', factor.id);
      assert.equal(deleted.statusCode, 200, deleted.body);
      assert.equal((await references(state, task))!.requests[0]!.status, terminal);
      assert.equal((await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers })).statusCode, 200);
      assert.deepEqual((await state.service.app.inject({ url: '/api/v3/factors', headers: state.headers })).json(), []);
      assert.deepEqual((await remove(state, 'factors', factor.id)).json(), deleted.json());
      onlyOriginalGets(source.requests, requestId);
    } finally { await source.close(); await state.cleanup(); }
  }
});

test('portfolio deletion fences retained original build uncertainty across restart without resubmitting or cancelling native work', async () => {
  const state = await setup(); const requestId = 'd'.repeat(64); const source = await peer(requestId);
  try {
    const { view } = await built(state); const original = structuredClone(await state.service.work.getPortfolio(view.id));
    const task = await retain(state, view.taskId!, requestId, source.target);
    source.set('UNKNOWN'); unsettled(await remove(state, 'portfolios', view.id));
    assert.deepEqual((await references(state, task))!.requests, [task.reference]);
    assert.equal(portfolio((await state.service.work.snapshot())!.portfolios[view.id]).deletedAt, undefined);
    await state.restart();
    source.set('unavailable'); unsettled(await remove(state, 'portfolios', view.id));
    assert.deepEqual((await references(state, task))!.requests, [task.reference]);
    source.set('COMPLETED'); const deleted = await remove(state, 'portfolios', view.id);
    assert.equal(deleted.statusCode, 200, deleted.body);
    const retained = portfolio((await state.service.work.snapshot())!.portfolios[view.id]);
    assert.deepEqual(retained.evidence, original.evidence); assert.deepEqual(retained.target, original.target);
    assert.equal((await references(state, task))!.requests[0]!.status, 'COMPLETED');
    await state.restart();
    assert.deepEqual((await remove(state, 'portfolios', view.id)).json(), deleted.json());
    onlyOriginalGets(source.requests, requestId);
  } finally { await source.close(); await state.cleanup(); }
});
