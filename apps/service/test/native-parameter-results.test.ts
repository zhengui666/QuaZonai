import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import test from 'node:test';
import { NativeStrategyBacktestTask } from '../../research/plugins/native-strategy-backtest.ts';
import { setup, context, fauxAssistantMessage, fauxToolCall } from './support.ts';

test('owner HTTP research exposes grid tools and pages every original native child across restart', { timeout: 60_000 }, async () => {
  const state = await setup();
  try {
    const binary = join(state.directory, 'native-protocol.mjs');
    await writeFile(binary, `#!${process.execPath}\nlet body='';for await(const c of process.stdin)body+=c;const request=JSON.parse(body);process.stdout.write(JSON.stringify({schema_version:1,request,native_version:'protocol',native_results:[{run_id:String(request.config.period)}],canonical_result:null,canonical_unavailable_reason:'PROTOCOL_TEST_ONLY'}));`, { mode: 0o755 });
    const dataset = state.options.config.datasets[0]!;
    dataset.data = { catalog: dataset.data.catalog!, nautilusTarget: { kind: 'RUST_PROCESS', id: 'parameter-test', binary, capabilities: { schemaVersion: 1, protocol: 'quazonai-native-process-v1', identity: { nautilusVersion: 'protocol', precision: 'high' }, research: { singleStrategyRegistration: true, configuredRunFactory: true }, account: { compiled: false, registration: false, command: 'account-runtime', requiresExplicitStart: true }, running: false } } };
    state.options.config.nativeProcess = dataset.data.nautilusTarget as unknown as import('../../../packages/contracts/src/nautilus.ts').ProcessNativeTarget;
    delete dataset.strategy; await state.restart();
    const grid = { configured: true, request: { schema_version: 1, name: 'test', config: { period: 1 } }, axes: [{ path: ['config', 'period'], values: Array.from({length: 21}, (_, i) => i + 1) }], offset: 0, pageSize: 21 };
    state.faux.setResponses([fauxAssistantMessage(fauxToolCall('native_parameter_grid', grid), { stopReason: 'toolUse' }), transcript => { const message = transcript.messages.filter(item => item.role === 'toolResult' && item.toolName === 'native_parameter_grid').at(-1); assert.ok(message?.role === 'toolResult'); assert.equal(message.isError, false); const generated = JSON.parse(message.content.filter(part => part.type === 'text').map(part => part.text).join('')); assert.equal(generated.totalCandidates, 21); return fauxAssistantMessage(fauxToolCall('native_parameter_batch', { configured: true, requests: generated.requests }), { stopReason: 'toolUse' }); }, fauxAssistantMessage('Protocol batch complete, not OOS evidence.')]);
    const admission = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: { requestId: 'grid-http', goal: 'Generate parameter values 1 through 21, then run native requests', datasetId: dataset.id, model: state.model } });
    assert.equal(admission.statusCode, 202, admission.body);
    const receipt = admission.json();
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const base = `/api/v3/research/${receipt.conversationId}/native-backtests`;
    const page = await state.service.app.inject({ url: base, headers: state.headers });
    assert.equal(page.statusCode, 200, page.body); assert.equal(page.json().items.length, 20, JSON.stringify(await state.service.research.state(receipt.conversationId)));
    assert.ok(page.json().items.every((x: any) => x.status === 'completed'));
    const secondUrl = `${base}?cursor=${encodeURIComponent(page.json().nextCursor)}`;
    const second = await state.service.app.inject({ url: secondUrl, headers: state.headers });
    assert.equal(second.json().items.length, 1); assert.equal(second.json().nextCursor, null);
    const rows = [...page.json().items, ...second.json().items];
    assert.equal(new Set(rows.map(x => x.taskId)).size, 21);
    for (const [index, row] of rows.entries()) {
      const original = await state.service.app.inject({ url: row.evidenceHref, headers: state.headers });
      assert.equal(original.statusCode, 200, original.body);
      assert.equal(original.json().input.request.config.period, index + 1);
      assert.deepEqual(original.json().state.outcome.result.request, original.json().input.request);
    }
    assert.equal((await state.service.app.inject({ url: base })).statusCode, 401);
    assert.equal((await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId + 100}/native-backtests?cursor=${encodeURIComponent(page.json().nextCursor)}`, headers: state.headers })).statusCode, 422);
    const badCursor = Buffer.from(JSON.stringify({ conversationId: receipt.conversationId, cursor: { after: 'bad', order: 'wrong' } })).toString('base64url');
    assert.equal((await state.service.app.inject({ url: `${base}?cursor=${badCursor}`, headers: state.headers })).statusCode, 422);
    assert.equal((await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId + 100}/evidence/${rows[0].taskId}`, headers: state.headers })).statusCode, 404);
    await state.restart();
    assert.deepEqual((await state.service.app.inject({ url: secondUrl, headers: state.headers })).json(), second.json());
    const conversation = await state.service.research.harness.conversation(receipt.conversationId, context);
    const failed = await conversation!.commit(tx => tx.createTask(NativeStrategyBacktestTask, { configured: true, request: grid.request, catalog: state.directory, target: '/missing-native-test' }, { ownership: { kind: 'conversation' } }), context);
    assert.equal((await state.service.research.harness.waitForTask(failed, context)).state.outcome.status, 'failed');
    const uncertainBinary = join(state.directory, 'uncertain-native-protocol.mjs');
    await writeFile(uncertainBinary, `#!${process.execPath}\nfor await(const c of process.stdin){}process.exit(1);`, {mode: 0o755});
    const uncertain = await conversation!.commit(tx => tx.createTask(NativeStrategyBacktestTask, { configured: true, request: grid.request, catalog: state.directory, target: {...state.options.config.nativeProcess!, binary: uncertainBinary} }, { ownership: {kind: 'conversation'} }), context);
    const deadline = Date.now() + 5000;
    while ((await state.service.research.harness.getTask(uncertain, context))!.state.status !== 'sleeping') { assert.ok(Date.now() < deadline); await new Promise(resolve => setTimeout(resolve, 10)); }
    const expanded = (await state.service.app.inject({url: secondUrl, headers: state.headers})).json().items;
    assert.deepEqual(expanded.map((row: any) => [row.taskId, row.status]), [[rows[20].taskId, 'completed'], [failed, 'failed'], [uncertain, 'sleeping']]);
    assert.equal((await state.service.app.inject({url: expanded[2].evidenceHref, headers: state.headers})).json().state.status, 'sleeping');
  } finally { await state.cleanup(); }
});
