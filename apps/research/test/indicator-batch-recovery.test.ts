import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider } from '@earendil-works/pi-ai';
import type { JsonObject } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { IndicatorStrategyTask } from '../plugins/indicator-strategy.ts';
import { nativeCommands, NativeRemoteUncertainty } from '../src/native-execution.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';

// Exercise official Durable/HTTP ownership and receipts, not numerical backtest results.
for (const behavior of ['recover', 'stop', 'partial'] as const) test(`native indicator batch ${behavior} retains a single original task and invocation`, { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-indicator-batch-'));
  const posts: NativeExecutionRequest[] = []; const deletes: string[] = []; let completed = behavior === 'partial';
  const request: JsonObject = { schema_version: 1, requests: [
    { selection: { bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] }, settings: {}, signal: { kind: 'EMA_CROSS', fast_period: 2, slow_period: 3 } },
    { selection: { bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] }, settings: {}, signal: { kind: 'EMA_CROSS', fast_period: 3, slow_period: 4 } },
  ] };
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    if (incoming.method === 'POST') {
      let body = ''; for await (const chunk of incoming) body += chunk;
      posts.push(JSON.parse(body));
    }
    const original = posts.at(-1)!;
    if (incoming.method === 'DELETE') {
      deletes.push(incoming.url!); response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: 'CANCELLED' })); return;
    }
    const requests = (original.input as JsonObject).requests as JsonObject[];
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: completed ? 'COMPLETED' : 'UNKNOWN',
      ...(completed ? { result: { schema_version: 1, native_version: '0.69.0', reuse: 'NAUTILUS_ENGINE_RESET',
        results: (behavior === 'partial' ? requests.slice(0, 1) : requests).map(request => ({ request, simulation: {}, observations: [] })) } } : { error: 'original_receipt_temporarily_unknown' }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const models = createModels(); const faux = fauxProvider(); models.setProvider(faux.provider);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  try {
    const root = await host.harness.root(context);
    const id = await root.commit(tx => tx.createTask(IndicatorStrategyTask, {
      request, batch: true, catalog: directory, target: { id: 'batch-protocol', url: `http://127.0.0.1:${address.port}` },
    }, { ownership: { kind: 'conversation' } }), context);
    if (behavior === 'partial') {
      const task = await host.harness.waitForTask(id, context);
      assert.equal(task.state.outcome.status, 'failed');
      assert.equal(task.state.outcome.status === 'failed' && task.state.outcome.error.message, 'indicator_strategy_batch_result_mismatch');
    } else {
      const deadline = Date.now() + 5000;
      for (;;) {
        const task = await host.harness.getTask(id, context); assert.ok(task);
        if (task.state.status === 'sleeping' && (task.state.checkpoint as { phase: string }).phase === 'reconcile') break;
        assert.notEqual(task.state.status, 'terminal', JSON.stringify(task.state));
        assert.ok(Date.now() < deadline, 'batch did not retain original receipt'); await delay(5);
      }
      if (behavior === 'recover') {
        const originalPosts = structuredClone(posts); await host.close(); completed = true;
        host = await openResearch(options);
        const task = await host.harness.waitForTask(id, context);
        // A recovered transport receipt is not evidence of valid science. The
        // deliberately incomplete response must fail the same per-case validator.
        assert.equal(task.state.outcome.status, 'failed');
        assert.equal(task.state.outcome.status === 'failed' && task.state.outcome.error.message, 'indicator_strategy_result_invalid');
        assert.deepEqual(posts, originalPosts, 'restart must GET the original receipt, not POST another batch');
        assert.equal(deletes.length, 0);
      } else {
        await host.harness.abortTask(id, context); await host.harness.waitForTask(id, context);
        assert.deepEqual(deletes, [`/v1/executions/${posts[0]!.requestId}`]);
        const retained = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context);
        assert.equal(retained!.requests[0]!.status, 'CANCELLED');
      }
    }
    assert.equal(posts.length, 1); assert.equal(posts[0]!.command, 'simulate-indicator-strategy-batch');
    assert.deepEqual(posts[0]!.input, request); assert.equal(faux.state.callCount, 0);
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});
