import assert from 'node:assert/strict';
import { test } from 'node:test';
import { Study } from '../../research/src/documents.ts';
import { ResearchConfiguration } from '../../research/src/research-configuration.ts';
import { setup, context, fauxAssistantMessage, fauxToolCall } from './support.ts';

test('authenticated natural-language research configures only its registered catalog through Pi', async () => {
  const state = await setup();
  try {
    const registered = state.options.config.datasets[0]!;
    const catalog = registered.data.catalog;
    registered.data = { catalog }; delete registered.strategy;
    await state.restart();
    const request = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }],
      event_start_ns: '1790000000000000001', event_end_ns: '1790000000000000101', available_end_ns: '1790000000000000101',
      venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['5000 USD'], base_currency: null,
        leverage: '1', maker_fee: '0', taker_fee: '0.001' }, strategy: { kind: 'EMA_CROSS', trade_size: '1', fast_period: 2, slow_period: 3 } };
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('native_strategy_configure', { request }), { stopReason: 'toolUse' }),
      fauxAssistantMessage('Configured only; native validation and historical execution have not run.')]);
    const payload = { requestId: 'natural-catalog', datasetId: registered.id, model: state.model, goal: '使用已注册数据配置官方 EMA，5000 USD，手续费 .001，不执行交易。' };
    const rejected = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', payload });
    assert.equal(rejected.statusCode, 401);
    const admitted = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload });
    assert.equal(admitted.statusCode, 202, admitted.body);
    const receipt = admitted.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = (await state.service.research.harness.snapshot(Study, receipt.conversationId, context))!;
    assert.deepEqual(study.input!.data.native_strategy_backtest_request, request);
    assert.equal(study.input!.data.catalog, catalog); assert.equal(study.final, null);
    const configuration = (await state.service.research.harness.snapshot(ResearchConfiguration, receipt.conversationId, context))!;
    assert.ok(configuration.configuredByTaskId); assert.equal(configuration.evaluationTaskId, null);
    const repeated = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload });
    assert.equal(repeated.statusCode, 202, repeated.body); assert.deepEqual(repeated.json(), receipt);
    assert.equal(state.faux.state.callCount, 2);
  } finally { await state.cleanup(); }
});
