import { NativeStrategyBacktestTask } from '../../research/plugins/native-strategy-backtest.ts';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Study } from '../../research/src/documents.ts';
import type { ConversationId, TaskId } from '@earendil-works/pi-durable';
import { setup, context, fauxAssistantMessage, fauxToolCall } from './support.ts';

// Actual official Pi model Tasks and SQLite persistence; no numerical backtest is claimed.
test('research API preserves original advisory reports, scoped evidence links and configuration history across restart', async () => {
  const state = await setup();
  const native = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }], event_start_ns: '1', event_end_ns: '100', available_end_ns: '100',
    venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['5000 USD'], base_currency: null, leverage: '1', maker_fee: '0', taker_fee: '0.001' },
    strategy: { kind: 'EMA_CROSS', trade_size: '0.500000000000000001', fast_period: 2, slow_period: 3 } };
  try {
    const dataset = state.options.config.datasets[0]!;
    dataset.data = { catalog: dataset.data.catalog!, nautilusTarget: { id: 'read-only-review-test', url: 'http://127.0.0.1:1' }, native_strategy_backtest_request: native };
    delete dataset.strategy; await state.restart();
    state.faux.setResponses([
      fauxAssistantMessage(fauxToolCall('native_strategy_logic_review', { hypothesis: 'Economic hypothesis only.' }), { stopReason: 'toolUse' }),
      fauxAssistantMessage(JSON.stringify({ stars: 3, rationale: 'Test logic judgment', risks: ['Costs'], uncertainties: ['No OOS'] })),
      fauxAssistantMessage(fauxToolCall('native_strategy_assessment_report', {}), { stopReason: 'toolUse' }),
      fauxAssistantMessage(JSON.stringify({ stars: 5, rationale: 'Evidence remains missing', risks: ['No native validation'], uncertainties: ['Sample'] })),
      fauxAssistantMessage('No independent data rating is available.'),
    ]);
    const admission = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { requestId: 'review-view', goal: 'Review the bound request', datasetId: dataset.id, model: state.model } });
    assert.equal(admission.statusCode, 202, admission.body); const receipt = admission.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const get = () => state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers: state.headers });
    const detail = (await get()).json(); const reports = detail.assessments;
    assert.equal(reports.length, 2); assert.deepEqual(reports.map((row: any) => row.mode), ['SYNTHESIS', 'LOGIC']);
    assert.ok(reports.every((row: any) => row.configuration === 'CURRENT' && row.status === 'completed'));
    assert.equal(reports[0].result.judgment.stars, null); assert.equal(reports[1].result.judgment.stars, 3);
    assert.equal(reports[0].result.evidence.dataValidation.status, 'NOT_RUN');
    assert.equal(reports[0].result.request.strategy.trade_size, native.strategy.trade_size);
    assert.ok(reports[0].evidenceLinks.some((link: any) => link.taskId === reports[1].taskId));
    for (const link of reports[0].evidenceLinks) {
      const response = await state.service.app.inject({ url: link.href, headers: state.headers }); assert.equal(response.statusCode, 200, response.body);
      const value = response.json(); assert.equal(value.taskId, link.taskId);
      const task = await state.service.research.harness.getTask(link.taskId as TaskId, context);
      assert.deepEqual(value.state, task!.state); assert.deepEqual(value.input, task!.input);
    }
    const conversation = await state.service.research.harness.conversation(receipt.conversationId, context);
    const failedNative = await conversation!.commit(tx => tx.createTask(NativeStrategyBacktestTask, { request: native, catalog: state.directory, target: '/missing-native-evidence-binary' }, { ownership: { kind: 'conversation' } }), context);
    assert.equal((await state.service.research.harness.waitForTask(failedNative, context)).state.outcome.status, 'failed');
    const nativeEvidence = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}/evidence/${failedNative}`, headers: state.headers });
    assert.equal(nativeEvidence.statusCode, 200); assert.equal(nativeEvidence.json().state.outcome.status, 'failed'); assert.deepEqual(nativeEvidence.json().input.request, native);
    const calls = state.faux.state.callCount; await state.restart();
    assert.deepEqual((await get()).json().assessments, reports); assert.equal(state.faux.state.callCount, calls);
    await state.service.research.harness.commit(async tx => { const study = await tx.doc(Study, receipt.conversationId as ConversationId); study.input!.data.native_strategy_backtest_request = { ...native, strategy: { ...native.strategy, trade_size: '2' } }; }, context);
    const changed = (await get()).json().assessments;
    assert.ok(changed.every((row: any) => row.configuration === 'HISTORICAL'));
    assert.deepEqual(changed.map((row: any) => row.result), reports.map((row: any) => row.result));
    const task = await state.service.research.harness.getTask(reports[0].taskId as TaskId, context);
    assert.equal((await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}/evidence/${task!.owner}`, headers: state.headers })).statusCode, 404, 'private tool inputs are not evidence endpoints');
    assert.equal((await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId + 1}/evidence/${reports[0].taskId}`, headers: state.headers })).statusCode, 404, 'cross-conversation task access is rejected');
    assert.equal((await state.service.app.inject({ url: reports[0].evidenceLinks[0].href })).statusCode, 401, 'existing owner session is still required');
  } finally { await state.cleanup(); }
});
