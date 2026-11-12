import { test } from 'node:test';
import assert from 'node:assert/strict';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { openResearch } from '../src/host.ts';
import { ResearchAssessment, ResearchAssessmentTask, assessmentJudgment } from '../plugins/research-assessment.ts';
import { ResearchConfiguration } from '../src/research-configuration.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';

const request = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }],
  event_start_ns: '1790000000000000001', event_end_ns: '1790000100000000001', available_end_ns: '1790000100000000001',
  venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['10000 USD'], base_currency: null, leverage: '1', maker_fee: '0', taker_fee: '0.001' },
  strategy: { kind: 'EMA_CROSS', trade_size: '0.500000000000000001', fast_period: 2, slow_period: 3 } };
const judgment = (stars: number | null) => ({ stars, rationale: 'Trend persistence is a hypothesis, not established profitability.', risks: ['Parameter selection bias'], uncertainties: ['No out-of-sample evidence'] });

test('assessment schema keeps missing evidence distinct from zero and rejects invented numeric ranges', () => {
  assert.equal(assessmentJudgment(judgment(null)).stars, null);
  assert.equal(assessmentJudgment(judgment(0)).stars, 0);
  for (const stars of [-1, 6, 1.5, undefined, '5']) assert.throws(() => assessmentJudgment({ ...judgment(null), stars }), /stars_invalid/);
  assert.throws(() => assessmentJudgment({ ...judgment(3), rationale: '' }), /rationale_required/);
});

test('official child Agent tasks isolate logic context, retain evidence identity and report missing OOS without averaging', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-assessment-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  let logicCalls = 0; let synthesisCalls = 0;
  faux.setResponses([
    fauxAssistantMessage(fauxToolCall('native_strategy_logic_review', { hypothesis: 'Trend persistence after transaction costs.' }), { stopReason: 'toolUse' }),
    (input) => {
      logicCalls++;
      const wire = JSON.stringify(input);
      assert.ok(!wire.includes('PRIVATE_CONVERSATION_SENTINEL'));
      assert.ok(wire.includes('NO_TRANSCRIPT_OR_NATIVE_RESULTS'));
      assert.ok(wire.includes('1790000000000000001')); assert.ok(wire.includes('0.500000000000000001'));
      return fauxAssistantMessage(JSON.stringify(judgment(3)));
    },
    fauxAssistantMessage(fauxToolCall('native_strategy_assessment_report', {}), { stopReason: 'toolUse' }),
    (input) => {
      synthesisCalls++;
      const wire = JSON.stringify(input);
      assert.ok(wire.includes('NOT_RUN')); assert.ok(wire.includes('INDEPENDENT_OUT_OF_SAMPLE'));
      return fauxAssistantMessage(JSON.stringify(judgment(5))); // A model cannot override missing-evidence semantics.
    },
    fauxAssistantMessage('Logic reviewed; independent data validation remains absent.'),
  ]);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  try {
    const receipt = await host.start('assessment', { goal: 'PRIVATE_CONVERSATION_SENTINEL', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, native_strategy_backtest_request: request }, policy: {} });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const doc = await host.harness.snapshot(ResearchAssessment, receipt.conversationId, context);
    assert.ok(doc?.logicTaskId); assert.equal(doc.reportTaskIds.length, 1);
    const logic = await host.harness.getTask(doc.logicTaskId, context);
    const report = await host.harness.getTask(doc.reportTaskIds[0]!, context);
    assert.equal(logic?.state.outcome?.status, 'completed'); assert.equal(report?.state.outcome?.status, 'completed');
    assert.ok(logic?.owner); assert.ok(report?.owner); assert.notEqual(logic!.id, report!.id);
    if (logic?.state.outcome?.status !== 'completed' || report?.state.outcome?.status !== 'completed') assert.fail('actual agent child tasks did not complete');
    assert.equal((logic.state.outcome.result.judgment as any).stars, 3);
    assert.equal((report.state.outcome.result.judgment as any).stars, null);
    assert.equal((report.state.outcome.result.evidence as any).logic.taskId, logic.id);
    assert.deepEqual(report.state.outcome.result.request, request);
    assert.equal(report.state.outcome.result.tradingPermission, false);
    const all = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 20), context);
    assert.equal(all.items.length, 0, 'review must not execute data tests or silently place orders');
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.harness.snapshot(ResearchAssessment, receipt.conversationId, context), doc);
    assert.equal(logicCalls, 1); assert.equal(synthesisCalls, 1);
    await host.harness.commit(async tx => {
      const configuration = await tx.doc(ResearchConfiguration, receipt.conversationId);
      configuration.request = { ...request, long_weight: '0.25' }; configuration.configuredByTaskId = 123;
    }, context);
    faux.setResponses([
      fauxAssistantMessage(fauxToolCall('native_strategy_assessment_report', {}), { stopReason: 'toolUse' }),
      (input) => { assert.ok(!JSON.stringify(input).includes('MISSING_OR_DIFFERENT_CONFIGURATION')); assert.ok(!JSON.stringify(input).includes('0.25')); assert.ok(JSON.stringify(input).includes('0.500000000000000001')); return fauxAssistantMessage(JSON.stringify(judgment(null))); },
      fauxAssistantMessage('Retired metadata does not replace the actual registered strategy.'),
    ]);
    const continuation = await (await host.harness.conversation(receipt.conversationId, context))!.submit({ type: 'input', content: 'Review this changed configuration', requestId: 'changed-assessment' }, context);
    await continuation.wait(context);
    const changed = await host.harness.snapshot(ResearchAssessment, receipt.conversationId, context);
    assert.equal(changed!.reportTaskIds.length, 2); assert.equal(changed!.logicTaskId, doc.logicTaskId);
    assert.equal((await host.harness.getTask(doc.reportTaskIds[0]!, context))!.state.outcome!.status, 'completed', 'previous report is preserved');

  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

test('a real native execution attempt cannot be relabelled as pre-execution logic review', async () => {
  const previousBinary = process.env.QZ_JOB_BINARY; process.env.QZ_JOB_BINARY = '/nonexistent-native-binary';
  const directory = await mkdtemp(join(tmpdir(), 'qz-assessment-after-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage('Please choose a hypothesis.')]);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models });
  try {
    const receipt = await host.start('after', { goal: 'Review', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, native_strategy_backtest_request: request }, policy: {} });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const conversation = await host.harness.conversation(receipt.conversationId, context);
    const execution = await conversation!.commit(tx => tx.createTask(NativeStrategyBacktestTask, { request: { ...request, available_start_ns: null, instrument_definition: { mode: 'HISTORICAL_AS_OF_START' }, strategy: { ...request.strategy, base: null } }, catalog: directory, target: '/nonexistent-native-binary' }, { ownership: { kind: 'conversation' } }), context);
    assert.equal((await host.harness.waitForTask(execution, context)).state.outcome.status, 'failed');
    faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_logic_review', { hypothesis: 'Already observed.' }), { stopReason: 'toolUse' }), fauxAssistantMessage('Cannot label this pre-execution.')]);
    const submission = await conversation!.submit({ type: 'input', content: 'Review now', requestId: 'after-review' }, context);
    await submission.wait(context);
    assert.equal((await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.research.assessment' }, 20), context)).items.length, 0);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); if (previousBinary === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previousBinary; }
});


test('aborting an official assessment child cancels its model call and never persists a fabricated rating', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-assessment-cancel-'));
  const faux = fauxProvider(); const models = createModels(); models.setProvider(faux.provider);
  let started!: () => void; const called = new Promise<void>(resolve => { started = resolve; });
  faux.setResponses([async (_input, options) => {
    started();
    await new Promise<void>(resolve => { if (options?.signal?.aborted) resolve(); else options?.signal?.addEventListener('abort', () => resolve(), { once: true }); });
    return fauxAssistantMessage('', { stopReason: 'aborted' });
  }]);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  try {
    const root = await host.harness.root(context);
    const id = await root.commit(tx => tx.createTask(ResearchAssessmentTask, { mode: 'LOGIC' as const, model: { provider: faux.provider.id, modelId: faux.getModel().id }, request, configurationTaskId: null, evidence: {} }, { ownership: { kind: 'conversation' } }), context);
    await called; await host.harness.abortTask(id, context);
    assert.equal((await host.harness.waitForTask(id, context)).state.outcome.status, 'aborted');
    await host.close(); host = await openResearch(options);
    assert.equal((await host.harness.getTask(id, context))!.state.outcome?.status, 'aborted');
    assert.equal(faux.state.callCount, 1);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

test('upstream native strategy binding exposes real logic and synthesis child Agents', async () => {
  const request = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }],
    event_start_ns: '1790000000000000001', event_end_ns: '1790000100000000001', available_end_ns: '1790000100000000001',
    venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['10000 USD'], base_currency: null, leverage: '1', maker_fee: '0', taker_fee: '0.001' },
    strategy: { kind: 'EMA_CROSS', trade_size: '0.500000000000000001', fast_period: 2, slow_period: 3 } };

  const directory = await mkdtemp(join(tmpdir(), 'qz-assessment-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  let logicCalls = 0; let synthesisCalls = 0;
  faux.setResponses([
    fauxAssistantMessage(fauxToolCall('native_strategy_logic_review', { hypothesis: 'Trend persistence after transaction costs.' }), { stopReason: 'toolUse' }),
    (input) => {
      logicCalls++;
      const wire = JSON.stringify(input);
      assert.ok(!wire.includes('PRIVATE_CONVERSATION_SENTINEL'));
      assert.ok(wire.includes('NO_TRANSCRIPT_OR_NATIVE_RESULTS'));
      assert.ok(wire.includes('1790000000000000001')); assert.ok(wire.includes('0.500000000000000001'));
      return fauxAssistantMessage(JSON.stringify(judgment(3)));
    },
    fauxAssistantMessage(fauxToolCall('native_strategy_assessment_report', {}), { stopReason: 'toolUse' }),
    (input) => {
      synthesisCalls++;
      const wire = JSON.stringify(input);
      assert.ok(wire.includes('NOT_RUN')); assert.ok(wire.includes('INDEPENDENT_OUT_OF_SAMPLE'));
      return fauxAssistantMessage(JSON.stringify(judgment(5))); // A model cannot override missing-evidence semantics.
    },
    fauxAssistantMessage('Logic reviewed; independent data validation remains absent.'),
  ]);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  try {
    const receipt = await host.start('assessment', { goal: 'PRIVATE_CONVERSATION_SENTINEL', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, native_strategy_backtest_request: request }, policy: {} });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    const doc = await host.harness.snapshot(ResearchAssessment, receipt.conversationId, context);
    assert.ok(doc?.logicTaskId); assert.equal(doc.reportTaskIds.length, 1);
    const logic = await host.harness.getTask(doc.logicTaskId, context);
    const report = await host.harness.getTask(doc.reportTaskIds[0]!, context);
    assert.equal(logic?.state.outcome?.status, 'completed'); assert.equal(report?.state.outcome?.status, 'completed');
    assert.ok(logic?.owner); assert.ok(report?.owner); assert.notEqual(logic!.id, report!.id);
    if (logic?.state.outcome?.status !== 'completed' || report?.state.outcome?.status !== 'completed') assert.fail('actual agent child tasks did not complete');
    assert.equal((logic.state.outcome.result.judgment as any).stars, 3);
    assert.equal((report.state.outcome.result.judgment as any).stars, null);
    assert.equal((report.state.outcome.result.evidence as any).logic.taskId, logic.id);
    assert.deepEqual(report.state.outcome.result.request, request);
    assert.equal(report.state.outcome.result.tradingPermission, false);
    const all = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 20), context);
    assert.equal(all.items.length, 0, 'review must not execute data tests or silently place orders');
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.harness.snapshot(ResearchAssessment, receipt.conversationId, context), doc);
    assert.equal(logicCalls, 1); assert.equal(synthesisCalls, 1);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

test('retired configuration metadata cannot supply an absent official execution binding', async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-assessment-retired-'));
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage('Choose an officially registered strategy.')]);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models });
  try {
    const receipt = await host.start('unbound', { goal: 'Research without an execution binding', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
    await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    await host.harness.commit(async tx => {
      const metadata = await tx.doc(ResearchConfiguration, receipt.conversationId);
      metadata.request = request; metadata.configuredByTaskId = 123;
    }, context);
    faux.setResponses([
      fauxAssistantMessage(fauxToolCall('native_strategy_logic_review', { hypothesis: 'Do not execute metadata.' }), { stopReason: 'toolUse' }),
      fauxAssistantMessage(fauxToolCall('native_strategy_assessment_report', {}), { stopReason: 'toolUse' }),
      fauxAssistantMessage('The original official strategy request is missing; no rating exists.'),
    ]);
    await (await (await host.harness.conversation(receipt.conversationId, context))!.submit({ type: 'input', content: 'Review', requestId: 'review-unbound' }, context)).wait(context);
    assert.equal((await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.research.assessment' }, 20), context)).items.length, 0);
    assert.equal(faux.state.callCount, 4, 'only ordinary model turns run, no invented reviewer input');
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
