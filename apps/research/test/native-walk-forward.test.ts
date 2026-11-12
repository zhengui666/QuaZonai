import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import type { JsonObject } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { NativeWalkForward } from '../plugins/native-walk-forward.ts';
import { ResearchAssessment } from '../plugins/research-assessment.ts';
import { nativeWalkForwardResult, walkForwardScope } from '../src/native-walk-forward-result.ts';
import { nativeCommands, NativeRemoteUncertainty } from '../src/native-execution.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';

const source: JsonObject = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }],
  event_start_ns: '1790000000000000001', event_end_ns: '1790000000000000100', available_end_ns: '1790000000000000100',
  venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['10000 USD'], base_currency: null, leverage: '1', maker_fee: '0', taker_fee: '0.001' },
  strategy: { kind: 'EMA_CROSS', trade_size: '0.001', fast_period: 2, slow_period: 3 } };
const request: JsonObject = { schema_version: 1, candidates: [source], split: { n_splits: 1, gap: 0 }, objective: { native_key: 'Sharpe Ratio', direction: 'MAXIMIZE' } };
// Protocol-only native wire. Numerical native execution is tested in the Rust job tests, not mocked here.
function wire() {
  const native = (start: string, end: string) => ({ schema_version: 1, native_version: '0.69.0', request: { ...source, available_start_ns: start, available_end_ns: end },
    canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'completed' }, accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: [], statistics: { returns: { 'Sharpe Ratio': '3ff0000000000000' } }, components: {} }, data_rows_by_type: { QuoteTick: '2' }, notes: [] });
  return { schema_version: 1, request, native_version: '0.69.0', splitter_version: 'solow-cv 0.7.3 TimeSeriesSplit', performance_scope: walkForwardScope, notes: ['Independent cold-start windows; not a comprehensive overfitting score.'], folds: [{ index: 0, train_start_ns: '1790000000000000001', train_end_ns: '1790000000000000002', test_start_ns: '1790000000000000003', test_end_ns: '1790000000000000004',
    trials: [{ candidate_index: 0, metric_value: 1, error: null, result: native('1790000000000000001', '1790000000000000002') }], selected_candidate_index: 0, test_result: native('1790000000000000003', '1790000000000000004'), status: 'COMPLETED', reason: null }] };
}

test('walk-forward evidence binds original strategies, chronology, availability and untouched native statistics', () => {
  const result = nativeWalkForwardResult(wire(), request);
  assert.equal((result.folds as any)[0].test_result.canonical_result.statistics.returns['Sharpe Ratio'], '3ff0000000000000');
  const failed = structuredClone(wire()); failed.folds[0]!.trials[0] = { candidate_index: 0, metric_value: null, error: 'missing native statistic', result: null } as any;
  Object.assign(failed.folds[0]!, { selected_candidate_index: null, test_result: null, status: 'NOT_EVALUATED', reason: 'NO_COMPARABLE_TRAINING_CANDIDATE' });
  assert.equal((nativeWalkForwardResult(failed, request).folds as any)[0].test_result, null, 'no successful tests or zero metric are invented');
  for (const mutate of [(r: any) => r.request.candidates[0].strategy.fast_period++, (r: any) => r.folds[0].test_start_ns = r.folds[0].train_end_ns,
    (r: any) => r.folds[0].test_result = null, (r: any) => r.folds[0].test_result.request.available_start_ns = '1790000000000000001',
    (r: any) => r.performance_scope = 'FULL_SELECTION_INCLUDING_PRE_DECISION_CASH']) {
    const changed = structuredClone(wire()); mutate(changed); assert.throws(() => nativeWalkForwardResult(changed, request));
  }
});

for (const behavior of ['recover', 'stop', 'tools', 'history'] as const) test(`native walk-forward ${behavior} uses original Durable receipt and actual evidence Tasks`, { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-walk-forward-'));
  const posts: NativeExecutionRequest[] = []; const deletes: string[] = []; let complete = behavior === 'tools' || behavior === 'history';
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    if (incoming.method === 'POST') { let body = ''; for await (const part of incoming) body += part; posts.push(JSON.parse(body)); }
    const original = posts.at(-1)!; assert.ok(['native-walk-forward', 'native-strategy-backtest'].includes(original.command));
    if (incoming.method === 'DELETE') { deletes.push(incoming.url!); response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: 'CANCELLED' })); return; }
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: complete ? 'COMPLETED' : 'UNKNOWN', ...(complete ? { result: original.command === 'native-walk-forward' ? wire() : { ...wire().folds[0]!.trials[0]!.result, request: source } } : { error: 'pending_original_receipt' }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'native-walk-forward-protocol', url: `http://127.0.0.1:${address.port}` };
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  try {
    if (behavior === 'tools' || behavior === 'history') {
      let receipt;
      if (behavior === 'history') {
        faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_backtest', {}), { stopReason: 'toolUse' })]);
        receipt = await host.start('walk-forward-tools', { goal: 'Check chronology, not profitability claims', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, nautilusTarget: target, native_strategy_backtest_request: source }, policy: {} });
        await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
      }
      faux.setResponses([
        fauxAssistantMessage(fauxToolCall('native_strategy_walk_forward', { split: request.split, objective: request.objective }), { stopReason: 'toolUse' }),
        fauxAssistantMessage(fauxToolCall('native_strategy_assessment_report', {}), { stopReason: 'toolUse' }),
        (input) => { const prompt = JSON.stringify(input); assert.ok(prompt.includes('PARTIAL_WALK_FORWARD_EVIDENCE')); assert.ok(prompt.includes('3ff0000000000000')); return fauxAssistantMessage(JSON.stringify({ stars: 5, rationale: 'Chronological tests exist, other checks remain missing.', risks: ['Search selection'], uncertainties: ['Costs and neighbourhood untested'] })); },
        fauxAssistantMessage('Walk-forward done; comprehensive rating remains unavailable.'),
      ]);
      if (receipt) {
        const submission = await (await host.harness.conversation(receipt.conversationId, context))!.submit({ type: 'input', content: 'Now run walk-forward and report existing facts', requestId: 'walk-forward-after-history' }, context);
        await submission.wait(context);
      } else {
        receipt = await host.start('walk-forward-tools', { goal: 'Check chronology, not profitability claims', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, nautilusTarget: target, native_strategy_backtest_request: source }, policy: {} });
        await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
      }
      const validation = await host.harness.snapshot(NativeWalkForward, receipt.conversationId, context); assert.equal(validation?.taskIds.length, 1);
      const assessment = await host.harness.snapshot(ResearchAssessment, receipt.conversationId, context); assert.equal(assessment?.reportTaskIds.length, 1);
      const task = await host.harness.getTask(assessment!.reportTaskIds[0]!, context);
      assert.equal(task?.state.outcome?.status, 'completed');
      if (task?.state.outcome?.status !== 'completed') assert.fail('report did not complete');
      assert.equal((task.state.outcome.result.judgment as any).stars, null);
      assert.equal((task.state.outcome.result.evidence as any).dataValidation.taskId, validation!.taskIds[0]);
      if (behavior === 'history') assert.equal((task.state.outcome.result.evidence as any).historicalNativeExecution.status, 'completed', 'newer walk-forward does not hide a real ordinary backtest');
    } else {
      const root = await host.harness.root(context);
      const id = await root.commit(tx => tx.createTask(NativeStrategyBacktestTask, { request, sourceRequest: source, walkForward: true, catalog: directory, target }, { ownership: { kind: 'conversation' } }), context);
      for (let count = 0; ; count++) {
        const task = await host.harness.getTask(id, context);
        if (task?.state.status === 'sleeping' && (task.state.checkpoint as any).phase === 'reconcile') break;
        assert.ok(count < 500); await delay(10);
      }
      if (behavior === 'stop') {
        await host.harness.abortTask(id, context); assert.equal((await host.harness.waitForTask(id, context)).state.outcome.status, 'aborted');
        assert.ok(deletes.length); assert.ok(deletes.every(path => path === `/v1/executions/${posts[0]!.requestId}`));
        assert.equal((await host.harness.snapshot(NativeRemoteUncertainty, root.id, String(id), context))!.requests[0]!.status, 'CANCELLED');
      } else {
        await host.close(); complete = true; host = await openResearch(options);
        const result = await host.harness.waitForTask(id, context); assert.equal(result.state.outcome.status, 'completed');
      }
    }
    assert.equal(posts.length, behavior === 'history' ? 2 : 1); assert.deepEqual(posts.at(-1)!.input, request);
  } finally { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); }
});
