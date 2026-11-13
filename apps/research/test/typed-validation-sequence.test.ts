// Official Harness + transport regression only; controlled peer data is not scientific evidence.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { openResearch } from '../src/host.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { ResearchAssessment } from '../plugins/research-assessment.ts';
import { nativeCommands } from '../src/native-execution.ts';
import type { NativeExecutionRequest } from '../../../packages/contracts/src/nautilus.ts';

const A = { schema_version: 1, name: 'owner::MultiInstrument', config: { instruments: ['BTC.SIM', 'ETH.SIM'], window: 'TRAIN_DECLARED', exact: '0.000000000000000019' } };
const B = { ...A, config: { ...A.config, window: 'OOS_DECLARED' } };
const judgment = (stars: number | null) => ({ stars, rationale: 'Protocol-only model response, not a scientific claim.', risks: ['Independent source audit absent'], uncertainties: ['No measured trading evidence in this test'] });
for (const [declaredB, sufficient] of [[true, true], [false, true], [true, false]]) test(`typed factory A/review/A/backtest/B/backtest/synthesis preserves declared request binding (${declaredB}, sufficient=${sufficient})`, { timeout: 20000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-explicit-validation-'));
  const posts: NativeExecutionRequest[] = [];
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    assert.equal(incoming.method, 'POST');
    let body = ''; for await (const chunk of incoming) body += chunk;
    const operation = JSON.parse(body) as NativeExecutionRequest; posts.push(operation);
    assert.equal(operation.command, 'native-configured-backtest');
    const result = { schema_version: 1, native_version: '0.69.0', request: operation.input, native_results: [{ run_id: 'protocol-only' }], canonical_result: null, canonical_unavailable_reason: 'OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG' };
    response.end(JSON.stringify({ schemaVersion: 1, requestId: operation.requestId, status: 'COMPLETED', result }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const models = createModels(); const faux = fauxProvider({ tokensPerSecond: 100000 }); models.setProvider(faux.provider);
  const call = (name: string, args = {}) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
  let synthesisSeen = false;
  faux.setResponses([
    call('native_configured_configure', { request: A }),
    call('native_strategy_logic_review', { hypothesis: 'Explicit separated windows, protocol test only.', validationMethod: { requests: declaredB ? [A, B] : [A], purposes: declaredB ? ['TRAINING_PROTOCOL_CASE', 'HELD_OUT_PROTOCOL_CASE'] : ['TRAINING_PROTOCOL_CASE'], protocol: 'Read separately; do not derive folds or select winners.' } }),
    fauxAssistantMessage(JSON.stringify(judgment(3))),
    call('native_configured_backtest'), call('native_configured_configure', { request: B }), call('native_configured_backtest'),
    fauxAssistantMessage('Explicit runs finished.'),
  ]);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models });
  const reportResponses: Parameters<typeof faux.setResponses>[0] = [
    call('native_strategy_assessment_report'),
    input => {
      synthesisSeen = true; const wire = JSON.stringify(input);
      assert.ok(wire.includes('EXPLICIT_PI_CONFIGURATIONS_NO_QZ_SPLITTER_OR_WINNER'));
      assert.ok(wire.includes(A.config.window)); assert.ok(wire.includes(B.config.window));
      return fauxAssistantMessage(JSON.stringify({ ...judgment(4), evidenceSufficient: sufficient, dataJudgment: judgment(2) }));
    },
    fauxAssistantMessage('Original explicit results reviewed; no automatic splitting or selection.'),
  ];
  try {
    const receipt = await host.start(`explicit-${declaredB}`, { goal: 'Protocol-only workflow validation', model: { provider: faux.provider.id, modelId: faux.getModel().id },
      data: { catalog: directory, nautilusTarget: { id: 'explicit-peer', url: `http://127.0.0.1:${address.port}` }, native_configured_backtest_request: A }, policy: {} });
    const settled = await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    assert.equal(settled.status, 'done');
    const conversation = await host.harness.conversation(receipt.conversationId, context); assert.ok(conversation);
    // Persist a retired invocation in the official store, after the valid pre-review.
    // Its old request shape must remain history, never become an explicit pre-reviewed run.
    const historicalId = await conversation.commit(tx => tx.createTask(NativeStrategyBacktestTask, {
      request: { candidates: [A], split: { n_splits: 2 }, objective: 'SHARPE' }, catalog: directory,
      target: { id: 'explicit-peer', url: `http://127.0.0.1:${address.port}` }, walkForward: true,
    }, { ownership: { kind: 'conversation' } }), context);
    assert.equal((await host.harness.waitForTask(historicalId, context)).state.outcome.status, 'failed');
    faux.setResponses(reportResponses);
    await (await conversation.submit({ type: 'input', content: 'Review explicit runs with historical records visible.', requestId: 'synthesize-history' }, context)).wait(context);
    assert.equal(synthesisSeen, true);
    assert.deepEqual(posts.map(post => post.input), [A, B]);
    assert.notEqual(posts[0]!.requestId, posts[1]!.requestId, 'distinct backtests keep distinct invocation identities');
    const assessment = await host.harness.snapshot(ResearchAssessment, receipt.conversationId, context); assert.ok(assessment);
    const logic = await host.harness.getTask(assessment.logicTaskId!, context); const report = await host.harness.getTask(assessment.reportTaskIds[0]!, context);
    assert.equal(logic?.state.outcome?.status, 'completed'); assert.equal(report?.state.outcome?.status, 'completed');
    if (logic?.state.outcome?.status !== 'completed' || report?.state.outcome?.status !== 'completed') assert.fail('missing actual assessment Tasks');
    assert.equal((logic.state.outcome.result as any).judgment.stars, 3);
    const result = report.state.outcome.result as any;
    for (const original of [logic.state.outcome.result, result]) {
      assert.deepEqual((original as any).instrumentDefinitionAssumption, { mode: 'FACTORY_CONFIGURATION_NOT_INDEPENDENTLY_VERIFIED', qualification: 'FACTORY_CONFIGURATION_NOT_INDEPENDENTLY_VERIFIED' });
    }
    assert.equal(result.judgment.stars, declaredB && sufficient ? 4 : null, JSON.stringify(result.evidence));
    assert.equal(result.dataJudgment.stars, declaredB && sufficient ? 2 : null);
    assert.equal(result.evidence.logic.taskId ?? null, declaredB ? logic.id : null);
    assert.equal(result.evidence.trialHistory.runs.length, 3);
    const historical = result.evidence.trialHistory.runs.find((run: any) => run.taskId === historicalId);
    assert.equal(historical.execution, 'HISTORICAL_RETIRED_QZ_WALK_FORWARD');
    assert.equal(historical.preReviewedRequest, false);
    assert.deepEqual(historical.request, { candidates: [A], split: { n_splits: 2 }, objective: 'SHARPE' });
    assert.equal(result.tradingPermission, false);
  } finally { await host.close(); server.closeAllConnections(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); }
});
