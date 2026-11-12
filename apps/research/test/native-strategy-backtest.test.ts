import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { nativeCommands, NativeRemoteUncertainty } from '../src/native-execution.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';

// Exercise official Durable/HTTP ownership and receipts, not numerical backtest results.
for (const behavior of ['recover', 'stop', 'mismatch', 'tool', 'tool-finish'] as const) test(`native upstream strategy ${behavior} retains a single original task and invocation`, { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-indicator-batch-'));
  const posts: NativeExecutionRequest[] = []; const deletes: string[] = []; let completed = behavior === 'mismatch' || behavior === 'tool' || behavior === 'tool-finish';
  const request: JsonObject = { schema_version: 1, instrument_id: 'TEST.SIM',
    selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }],
    event_start_ns: '1790000000000000001', event_end_ns: '1790000000000000101', available_end_ns: '1790000000000000101',
    venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['10000 USD'], base_currency: null, leverage: '1', maker_fee: '0', taker_fee: '0.0001' },
    strategy: { kind: 'EMA_CROSS', base: { strategy_id: 'OWNER-001', order_id_tag: 'native' }, trade_size: '0.001', fast_period: 2, slow_period: 3 } };
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
    // Protocol-only payload: verifies binding/recovery, not numerical engine behavior.
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: completed ? 'COMPLETED' : 'UNKNOWN',
      ...(completed ? { result: { schema_version: 1, native_version: '0.69.0',
        request: behavior === 'mismatch' ? { ...original.input as JsonObject, instrument_id: 'OTHER.SIM' } : original.input,
        canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'completed' },
          accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: [], statistics: {}, components: {} },
        data_rows_by_type: { QuoteTick: '2' }, notes: [] } } : { error: 'original_receipt_temporarily_unknown' }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const models = createModels(); const faux = fauxProvider(); models.setProvider(faux.provider);
  const options = { database: join(directory, 'research.sqlite'), models };
  if (behavior === 'tool' || behavior === 'tool-finish') faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_backtest', behavior === 'tool-finish' ? { finishResearch: true } : {}), { stopReason: 'toolUse' }), fauxAssistantMessage('Historical result saved; no independent validation was requested.')]);
  let host = await openResearch(options);
  try {
    const root = await host.harness.root(context);
    if (behavior === 'tool' || behavior === 'tool-finish') {
      const { Study } = await import('../src/documents.ts');
      const admission = await host.start('exact-upstream', { goal: 'Evaluate this bound upstream strategy', model: { provider: faux.provider.id, modelId: faux.getModel().id },
        data: { catalog: directory, native_strategy_backtest_request: request, nautilusTarget: { id: 'strategy-protocol', url: `http://127.0.0.1:${address.port}` } }, policy: {} });
      await (await host.harness.submission(admission.submissionId, context))!.wait(context);
      const study = (await host.harness.snapshot(Study, admission.conversationId, context))!;
      assert.equal(study.final?.status, 'COMPUTED');
      assert.equal(study.final?.scientificClaim, 'HISTORICAL_NATIVE_BACKTEST_NOT_INDEPENDENT_QUALIFICATION');
      assert.equal(study.trials.length, 0);
      assert.equal(faux.state.callCount, behavior === 'tool-finish' ? 1 : 2, 'default continues the official loop; explicit standalone finish terminates');
      assert.equal(posts.length, 1); assert.deepEqual(posts[0]!.input, request);
      const originalPosts = structuredClone(posts); await host.close(); host = await openResearch(options);
      assert.deepEqual(await host.start('exact-upstream', study.input!), admission);
      assert.deepEqual(posts, originalPosts);
      return;
    }
    const id = await root.commit(tx => tx.createTask(NativeStrategyBacktestTask, {
      request, catalog: directory, target: { id: 'batch-protocol', url: `http://127.0.0.1:${address.port}` },
    }, { ownership: { kind: 'conversation' } }), context);
    if (behavior === 'mismatch') {
      const task = await host.harness.waitForTask(id, context);
      assert.equal(task.state.outcome.status, 'failed');
      assert.equal(task.state.outcome.status === 'failed' && task.state.outcome.error.message, 'native_strategy_request_mismatch');
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
        assert.equal(task.state.outcome.status, 'completed');
        assert.deepEqual(posts, originalPosts, 'restart must GET the original receipt, not POST another batch');
        assert.equal(deletes.length, 0);
      } else {
        await host.harness.abortTask(id, context); await host.harness.waitForTask(id, context);
        assert.deepEqual(deletes, [`/v1/executions/${posts[0]!.requestId}`]);
        const retained = await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context);
        assert.equal(retained!.requests[0]!.status, 'CANCELLED');
      }
    }
    assert.equal(posts.length, 1); assert.equal(posts[0]!.command, 'native-strategy-backtest');
    assert.deepEqual(posts[0]!.args, ['--catalog', directory]);
    assert.deepEqual(posts[0]!.input, request); assert.equal(faux.state.callCount, 0);
  } finally {
    await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve()));
    await rm(directory, { recursive: true, force: true });
  }
});

test('bound upstream research exposes only its native tool and never translates a sealed or mixed binding', async () => {
  const { fauxAssistantMessage, fauxToolCall } = await import('@earendil-works/pi-ai');
  const { Study } = await import('../src/documents.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-strategy-intake-'));
  const models = createModels(); const faux = fauxProvider(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_backtest', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('Cannot use a sealed historical binding.')]);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models });
  try {
    await assert.rejects(host.start('bound-upstream', { goal: 'Reject mixed historical binding without translating it', model: { provider: faux.provider.id, modelId: faux.getModel().id },
      data: { catalog: '/registered/catalog', native_strategy_backtest_request: {}, indicator_strategy_request: {} }, policy: {} }), /custom_indicator_strategy_removed/);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
