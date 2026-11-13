import { isNativeTable, readNativeTable } from '../src/native-table.ts';
import { NativeEvidencePart } from '../plugins/native-evidence.ts';
import { Study } from '../src/documents.ts';
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
import { ResearchAssessment, nativeAssessmentMetric } from '../plugins/research-assessment.ts';
import { nativeWalkForwardResult, walkForwardScope } from '../src/native-walk-forward-result.ts';
import { nativeCommands, NativeRemoteUncertainty } from '../src/native-execution.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';

const source: JsonObject = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }],
  event_start_ns: '1790000000000000001', event_end_ns: '1790000000000000100', available_end_ns: '1790000000000000100',
  venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['10000 USD'], base_currency: null, leverage: '1', maker_fee: '0', taker_fee: '0.001' },
  strategy: { kind: 'EMA_CROSS', trade_size: '0.001', fast_period: 2, slow_period: 3 } };
const request: JsonObject = { schema_version: 1, candidates: [source], split: { n_splits: 1, gap: 0 }, objective: { native_key: 'Sharpe Ratio', direction: 'MAXIMIZE' } };
function instrumentBasis(candidate: JsonObject): JsonObject {
  const selected = candidate.instrument_definition as JsonObject | undefined;
  return selected?.mode === 'STATIC_SCENARIO' ? { ...selected, scope: 'HYPOTHETICAL_STATIC_RULES_NOT_HISTORICALLY_VERIFIED' } : { mode: 'HISTORICAL_AS_OF_START', ts_init_ns: '0', ts_event_ns: '0', scope: 'CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START' };
}
function equityWire(available = false): JsonObject {
  return { source: 'NAUTILUS_PORTFOLIO_SNAPSHOTS', status: available ? 'AVAILABLE' : 'UNAVAILABLE', reason: available ? null : 'OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE', currency: available ? 'USD' : null,
    returns: available ? [{ ts_event_ns: '1790000000000000001', value: 0.01 }] : [], statistics: [252, 365].flatMap(annualization_days => ['CAGR', 'Sharpe Ratio'].map(name => ({ native_key: `${name} (${annualization_days} days)`, annualization_days, value: available ? annualization_days === 365 ? 3 : 2 : null }))),
    first_snapshot_ns: available ? '1790000000000000000' : null, last_snapshot_ns: available ? '1790000000000000001' : null, snapshot_count: available ? '2' : '0', observed_utc_days: available ? '1' : '0', unpriced_snapshot_count: '0', return_days: available ? '1' : '0', notes: ['Protocol fixture, not measured performance'] };
}
// Protocol-only native wire. Numerical native execution is tested in the Rust job tests, not mocked here.
function wire(walkRequest: JsonObject = request, outcome = 'completed', selected = 0, trainOutcome = outcome, large = false) {
  const native = (start: string, end: string, candidate: JsonObject = source, runOutcome = outcome) => ({ schema_version: 1, native_version: '0.69.0', request: { ...candidate, available_start_ns: start, available_end_ns: end },
    instrument_definition: instrumentBasis(candidate), equity_performance: equityWire((walkRequest.objective as JsonObject).metric_source === 'EQUITY_PERFORMANCE'), canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: runOutcome }, accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: large ? Array.from({ length: 1000 }, (_, index) => ({ quantity: String(index), proof: 'UNPROJECTED_ROW'.repeat(80) })) : [{ quantity: '1.0000000001' }, { quantity: '2.0000000002' }, { quantity: '3.0000000003' }], statistics: { returns: { 'Sharpe Ratio': '3ff0000000000000' } }, components: {} }, data_rows_by_type: { QuoteTick: '2' }, notes: [] });
  return { schema_version: 1, request: walkRequest, native_version: '0.69.0', splitter_version: 'solow-cv 0.7.3 TimeSeriesSplit', performance_scope: walkForwardScope, notes: ['Independent cold-start windows; not a comprehensive overfitting score.'], folds: [{ index: 0, train_start_ns: '1790000000000000001', train_end_ns: '1790000000000000002', test_start_ns: '1790000000000000003', test_end_ns: '1790000000000000004',
    trials: (walkRequest.candidates as JsonObject[]).map((candidate, candidate_index) => ({ candidate_index, metric_value: selected === 1 ? candidate_index + 1 : 1, error: null, result: native('1790000000000000001', '1790000000000000002', candidate, trainOutcome) })), selected_candidate_index: selected, test_result: native('1790000000000000003', '1790000000000000004', (walkRequest.candidates as JsonObject[])[selected]), status: outcome === 'stopped' ? 'TEST_STOPPED' : 'COMPLETED', reason: outcome === 'stopped' ? 'WALK_FORWARD_TEST_WINDOW_STOPPED' : null }] };
}

test('walk-forward evidence binds original strategies, chronology, availability and untouched native statistics', () => {
  const result = nativeWalkForwardResult(wire(), request);
  assert.equal((result.folds as any)[0].test_result.canonical_result.statistics.returns['Sharpe Ratio'], '3ff0000000000000');
  const failed = structuredClone(wire()); failed.folds[0]!.trials[0] = { candidate_index: 0, metric_value: null, error: 'missing native statistic', result: null } as any;
  Object.assign(failed.folds[0]!, { selected_candidate_index: null, test_result: null, status: 'NOT_EVALUATED', reason: 'NO_COMPARABLE_TRAINING_CANDIDATE' });
  assert.equal((nativeWalkForwardResult(failed, request).folds as any)[0].test_result, null, 'no successful tests or zero metric are invented');
  for (const mutate of [(r: any) => r.request.candidates[0].strategy.fast_period++, (r: any) => r.folds[0].test_start_ns = r.folds[0].train_end_ns,
    (r: any) => r.folds[0].test_result = null, (r: any) => r.folds[0].test_result.request.available_start_ns = '1790000000000000001',
    (r: any) => r.folds[0].test_result.instrument_definition.mode = 'STATIC_SCENARIO', (r: any) => r.performance_scope = 'FULL_SELECTION_INCLUDING_PRE_DECISION_CASH']) {
    const changed = structuredClone(wire()); mutate(changed); assert.throws(() => nativeWalkForwardResult(changed, request));
  }
});

test('stopped native test windows retain the original evidence but cannot claim completed OOS', () => {
  const stopped = wire(request, 'stopped', 0, 'completed');
  const original = structuredClone(stopped);
  assert.deepEqual(nativeWalkForwardResult(stopped, request), original);
  assert.equal(stopped.folds[0]!.status, 'TEST_STOPPED');
  const mislabeled = structuredClone(stopped);
  Object.assign(mislabeled.folds[0]!, { status: 'COMPLETED', reason: null });
  assert.throws(() => nativeWalkForwardResult(mislabeled, request), /walk_forward_test_outcome_mismatch/);
  const missing = structuredClone(wire());
  delete (missing.folds[0]!.test_result!.canonical_result.run as any).outcome;
  assert.throws(() => nativeWalkForwardResult(missing, request), /native_strategy_report_invalid/);
});

test('assessment decodes only unambiguous finite original native metrics without manufacturing zeros', () => {
  const objective = { native_key: 'metric' };
  assert.equal(nativeAssessmentMetric({ returns: { metric: '0000000000000000' } }, objective), 0);
  assert.equal(nativeAssessmentMetric({ returns: { metric: '3ff0000000000000' } }, objective), 1);
  for (const statistics of [{}, { returns: { metric: '7ff8000000000000' } }, { returns: { metric: '7ff0000000000000' } }, { returns: { metric: '1.0' } }, { general: { metric: '3ff0000000000000' }, returns: { metric: '3ff0000000000000' } }]) assert.equal(nativeAssessmentMetric(statistics as JsonObject, objective), null);
  assert.equal(nativeAssessmentMetric({ pnls: { USD: { metric: '3ff0000000000000' }, EUR: { metric: '4000000000000000' } } }, { ...objective, currency: 'EUR' }), 2);
});

for (const behavior of ['recover', 'cancel'] as const) test(`native sensitivity ${behavior} retains owned official Tasks and original receipts`, { timeout: 20_000 }, async () => {
  const { NativeSensitivity } = await import('../plugins/native-sensitivity.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-sensitivity-'));
  const posts: NativeExecutionRequest[] = []; const cancelled: string[] = []; let complete = false;
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    if (incoming.method === 'POST') { let body = ''; for await (const part of incoming) body += part; posts.push(JSON.parse(body)); }
    const original = incoming.method === 'POST' ? posts.at(-1)! : posts.find(post => incoming.url?.endsWith(post.requestId))!;
    assert.equal(original.command, 'native-strategy-backtest');
    if (incoming.method === 'DELETE') { cancelled.push(original.requestId); response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: 'CANCELLED' })); return; }
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: complete ? 'COMPLETED' : 'UNKNOWN', ...(complete ? { result: { ...wire().folds[0]!.trials[0]!.result, request: original.input, instrument_definition: instrumentBasis(original.input as JsonObject), equity_performance: equityWire(), canonical_result: { ...wire().folds[0]!.trials[0]!.result!.canonical_result, run: { outcome: 'completed' } } } } : { error: 'original_native_work_unresolved' }) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_sensitivity', { neighbourStrategies: [{ ...(source.strategy as JsonObject), fast_period: 3, slow_period: 5 }], costs: [{ maker_fee: '0', taker_fee: '0.002' }] }), { stopReason: 'toolUse' }), fauxAssistantMessage('Actual cases completed; no OOS claim.')]);
  const options = { database: join(directory, 'research.sqlite'), models }; let host = await openResearch(options);
  try {
    const receipt = await host.start(`sensitivity-${behavior}`, { goal: 'Check real task lifecycle', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, nautilusTarget: { id: 'sensitivity-protocol', url: `http://127.0.0.1:${address.port}` }, native_strategy_backtest_request: source }, policy: {} });
    let run;
    for (let tries = 0; ; tries++) {
      run = (await host.harness.snapshot(NativeSensitivity, receipt.conversationId, context))?.runs[0];
      if (run && posts.length === 3 && (await Promise.all(run.tasks.map(item => host.harness.getTask(item.taskId, context)))).every(task => task?.state.status === 'sleeping')) break;
      assert.ok(tries < 500); await delay(10);
    }
    const ids = run.tasks.map(item => item.taskId);
    if (behavior === 'recover') {
      await host.close(); complete = true; host = await openResearch(options);
      await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
      for (const id of ids) assert.equal((await host.harness.getTask(id, context))?.state.outcome?.status, 'completed', 'original native child resumes, without replacement work');
    } else {
      await host.harness.abortTask(run.toolTaskId as typeof ids[number], context);
      for (const id of ids) assert.equal((await host.harness.waitForTask(id, context)).state.outcome.status, 'aborted');
      assert.deepEqual([...new Set(cancelled)].sort(), posts.map(post => post.requestId).sort());
    }
    assert.equal(posts.length, 3, 'restart/cancellation never posts replacement native work');
    assert.deepEqual((await host.harness.snapshot(NativeSensitivity, receipt.conversationId, context))!.runs[0]!.tasks.map(item => item.taskId), ids);
  } finally { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); }
});

for (const uncertain of [false, true]) test(`owner stop after historical result resumes only settled sensitivity receipts (unknown=${uncertain})`, { timeout: 20_000 }, async () => {
  const { NativeSensitivity } = await import('../plugins/native-sensitivity.ts');
  const { CampaignResearchDoc } = await import('../src/pm-campaign.ts');
  const directory = await mkdtemp(join(tmpdir(), 'qz-owner-native-continuation-'));
  const posts: NativeExecutionRequest[] = []; let resolved = !uncertain; let stopping = false;
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    if (incoming.method === 'POST') { let body = ''; for await (const part of incoming) body += part; posts.push(JSON.parse(body)); }
    const original = incoming.method === 'POST' ? posts.at(-1)! : posts.find(post => incoming.url?.endsWith(post.requestId))!;
    if (original === posts[0]) { response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: 'COMPLETED', result: { ...wire().folds[0]!.trials[0]!.result, request: original.input } })); return; }
    const cancelled = stopping && resolved && incoming.method !== 'POST';
    response.end(JSON.stringify({ schemaVersion: 1, requestId: original.requestId, status: cancelled ? 'CANCELLED' : 'UNKNOWN', ...(!cancelled ? { error: 'original_receipt_unresolved' } : {}) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve)); const address = server.address(); assert.ok(address && typeof address !== 'string');
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_backtest', {}), { stopReason: 'toolUse' }), fauxAssistantMessage(fauxToolCall('native_strategy_sensitivity', { neighbourStrategies: [{ ...(source.strategy as JsonObject), fast_period: 3, slow_period: 5 }], costs: [{ maker_fee: '0', taker_fee: '0.002' }] }), { stopReason: 'toolUse' })]);
  const options = { database: join(directory, 'research.sqlite'), models }; let host = await openResearch(options);
  try {
    const receipt = await host.start(`stop-resume-${uncertain}`, { goal: 'Historical result then validation', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, nautilusTarget: { id: 'owner-native-protocol', url: `http://127.0.0.1:${address.port}` }, native_strategy_backtest_request: source }, policy: {} });
    let run;
    for (let tries = 0; ; tries++) {
      run = (await host.harness.snapshot(NativeSensitivity, receipt.conversationId, context))?.runs[0];
      if (run && posts.length === 4 && (await Promise.all(run.tasks.map(item => host.harness.getTask(item.taskId, context)))).every(task => task?.state.status === 'sleeping')) break;
      assert.ok(tries < 500); await delay(10);
    }
    stopping = true; await host.stop(receipt.conversationId);
    const stopped = (await host.harness.snapshot(Study, receipt.conversationId, context))!;
    assert.equal(stopped.final!.status, 'COMPUTED'); assert.deepEqual((stopped.final!.result as any).request, source);
    const originalStop = (await host.harness.snapshot(CampaignResearchDoc, receipt.conversationId, context))!.ownerStopTaskId;
    assert.equal((await host.submission(receipt.submissionId)).status, 'unanswered');
    await host.close(); host = await openResearch(options);
    const instruction = 'Continue from retained evidence without repeating native runs.';
    if (uncertain) {
      await assert.rejects(host.resume(receipt.conversationId, receipt.submissionId, instruction), /native_remote_execution_unsettled/);
      assert.equal(posts.length, 4); assert.deepEqual((await host.harness.snapshot(Study, receipt.conversationId, context))!.final, stopped.final);
      resolved = true;
    }
    faux.setResponses([fauxAssistantMessage('Historical result retained; cancelled validation remains cancelled, not successful.')]);
    const resumed = await host.resume(receipt.conversationId, receipt.submissionId, instruction);
    await (await host.harness.submission(resumed.submissionId, context))!.wait(context);
    assert.deepEqual(await host.resume(receipt.conversationId, receipt.submissionId, instruction), resumed);
    assert.equal(posts.length, 4, 'continuation and reconciliation never rePOST the stopped native attempts');
    const study = (await host.harness.snapshot(Study, receipt.conversationId, context))!;
    assert.equal(study.final, null); assert.deepEqual(study.previousNativeResults!.at(-1)!.final, stopped.final);
    for (const item of run.tasks) assert.equal((await host.harness.getTask(item.taskId, context))!.state.outcome!.status, 'aborted');
    assert.equal((await host.harness.snapshot(CampaignResearchDoc, receipt.conversationId, context))!.ownerStopTaskId, undefined);
    await host.stop(receipt.conversationId);
    assert.notEqual((await host.harness.snapshot(CampaignResearchDoc, receipt.conversationId, context))!.ownerStopTaskId, originalStop, 'later submissions receive their own stop task');
  } finally { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); }
});

for (const uncertain of [false, true]) test(`model error after native result resumes original evidence only after receipts settle (unknown=${uncertain})`, { timeout: 20_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-model-error-native-'));
  const posts: NativeExecutionRequest[] = []; let resolved = false;
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    if (incoming.method === 'POST') { let body = ''; for await (const part of incoming) body += part; posts.push(JSON.parse(body)); }
    const original = incoming.method === 'POST' ? posts.at(-1)! : posts.find(post => incoming.url?.endsWith(post.requestId))!;
    response.end(JSON.stringify(original === posts[0] ? { schemaVersion: 1, requestId: original.requestId, status: 'COMPLETED', result: { ...wire().folds[0]!.trials[0]!.result, request: original.input } }
      : { schemaVersion: 1, requestId: original.requestId, status: resolved ? 'CANCELLED' : 'UNKNOWN', ...(!resolved ? { error: 'original_receipt_unresolved' } : {}) }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve)); const address = server.address(); assert.ok(address && typeof address !== 'string');
  const target = { id: 'model-error-protocol', url: `http://127.0.0.1:${address.port}` };
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_backtest', {}), { stopReason: 'toolUse' }), fauxAssistantMessage('', { stopReason: 'error', errorMessage: '400 unsupported response test-only' })]);
  const options = { database: join(directory, 'research.sqlite'), models }; let host = await openResearch(options);
  try {
    const receipt = await host.start(`model-failure-${uncertain}`, { goal: 'Continue validation after model failure', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, nautilusTarget: target, native_strategy_backtest_request: source }, policy: {} });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const submission = await host.submission(receipt.submissionId); assert.equal(submission.status, 'unanswered'); assert.equal(submission.reason, 'model_error');
    const original = (await host.harness.snapshot(Study, receipt.conversationId, context))!.final!; assert.equal(original.status, 'COMPUTED');
    if (uncertain) {
      const conversation = await host.harness.conversation(receipt.conversationId, context);
      const id = await conversation!.commit(tx => tx.createTask(NativeStrategyBacktestTask, { request: source, catalog: directory, target }, { ownership: { kind: 'conversation' } }), context);
      for (let i = 0; ; i++) { const task = await host.harness.getTask(id, context); if (task?.state.status === 'sleeping') break; assert.ok(i < 500); await delay(10); }
      await host.harness.abortTask(id, context); await host.harness.waitForTask(id, context);
    }
    await host.close(); host = await openResearch(options);
    if (uncertain) {
      await assert.rejects(host.resume(receipt.conversationId, receipt.submissionId), /native_remote_execution_unsettled/);
      assert.deepEqual((await host.harness.snapshot(Study, receipt.conversationId, context))!.final, original);
      resolved = true;
    }
    const originalPosts = structuredClone(posts);
    faux.setResponses([fauxAssistantMessage('Continue using retained evidence, no repeated native execution.')]);
    const resumed = await host.resume(receipt.conversationId, receipt.submissionId);
    await (await host.harness.submission(resumed.submissionId, context))!.wait(context);
    assert.deepEqual(await host.resume(receipt.conversationId, receipt.submissionId), resumed);
    assert.deepEqual(posts, originalPosts, 'resume only reconciles original receipts and never repeats native POST');
    const study = (await host.harness.snapshot(Study, receipt.conversationId, context))!;
    assert.equal(study.final, null); assert.deepEqual(study.previousNativeResults!.at(-1)!.final, original);
    assert.equal((await host.submission(receipt.submissionId)).reason, 'model_error', 'original failed submission is not rewritten');
  } finally { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); }
});
