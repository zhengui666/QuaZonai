// Official Harness + transport regression only; controlled peer data is not scientific evidence.
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, rm, writeFile, readFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { openResearch } from '../src/host.ts';
import { NativeStrategyBacktestTask } from '../plugins/native-strategy-backtest.ts';
import { ResearchAssessment } from '../plugins/research-assessment.ts';
import { nativeCommands } from '../src/native-execution.ts';
import type { ProcessNativeTarget } from '../../../packages/contracts/src/nautilus.ts';

const A = { schema_version: 1, instrument_id: 'TEST.SIM', selectors: [{ data_type: 'QuoteTick', identifier: 'TEST.SIM' }],
  event_start_ns: '1790000000000000001', event_end_ns: '1790000000000000100', available_end_ns: '1790000000000000100',
  venue: { account_type: 'MARGIN', oms_type: 'NETTING', book_type: 'L1_MBP', starting_balances: ['5000 USD'], base_currency: null, leverage: '1', maker_fee: '0', taker_fee: '0.001' },
  strategy: { kind: 'EMA_CROSS', trade_size: '0.001', fast_period: 2, slow_period: 3 } };
const B = { ...A, event_start_ns: '1790000000000000200', event_end_ns: '1790000000000000300', available_end_ns: '1790000000000000300' };
const judgment = (stars: number | null) => ({ stars, rationale: 'Protocol-only model response, not a scientific claim.', risks: ['Independent source audit absent'], uncertainties: ['No measured trading evidence in this test'] });
for (const declaredB of [true, false]) test(`explicit A/review/A/backtest/B/backtest/synthesis preserves declared request binding (${declaredB})`, { timeout: 20000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-explicit-validation-'));
  const binary=join(directory,'controlled-native.mjs'), marker=join(directory,'calls.jsonl');
  await writeFile(binary, `#!${process.execPath}\nimport{appendFileSync}from'node:fs';let text='';for await(const part of process.stdin)text+=part;const input=JSON.parse(text);appendFileSync(${JSON.stringify(marker)},JSON.stringify({command:process.argv[2],args:process.argv.slice(3),input})+'\\n');
const result = { schema_version: 1, native_version: '0.69.0', request: input,
      instrument_definition: { mode: 'HISTORICAL_AS_OF_START', ts_init_ns: '0', ts_event_ns: '0', scope: 'CATALOG_DEFINITION_AVAILABLE_AT_WINDOW_START' },
      equity_performance: { source: 'NAUTILUS_PORTFOLIO_SNAPSHOTS', status: 'UNAVAILABLE', reason: 'OFFICIAL_PORTFOLIO_RETURNS_UNAVAILABLE', currency: null, returns: [], statistics: [252, 365].flatMap(annualization_days => ['CAGR', 'Sharpe Ratio'].map(name => ({ native_key: name+' ('+annualization_days+' days)', annualization_days, value: null }))), first_snapshot_ns: null, last_snapshot_ns: null, snapshot_count: '0', observed_utc_days: '0', unpriced_snapshot_count: '0', return_days: '0', notes: ['Protocol-only response; not market evidence.'] },
      canonical_result: { schema: 'nautilus-backtest-result/v1', run: { outcome: 'completed' }, accounts: [], diagnostics: [], fills: [], orders: [], portfolio_snapshots: [], position_snapshots: [], positions: [], statistics: {}, components: {} },
      data_rows_by_type: { QuoteTick: '2' }, notes: ['Protocol-only response; not market evidence.'] };
process.stdout.write(JSON.stringify(result));`,{mode:0o755});
  const target:ProcessNativeTarget={kind:'RUST_PROCESS',id:'controlled-process',binary,capabilities:{schemaVersion:1,protocol:'quazonai-native-process-v1',identity:{nautilusVersion:'protocol-only',precision:'high'},research:{singleStrategyRegistration:true,configuredRunFactory:true},account:{compiled:false,registration:false,command:'account-runtime',requiresExplicitStart:true},running:false}};
  const models = createModels(); const faux = fauxProvider({ tokensPerSecond: 100000 }); models.setProvider(faux.provider);
  const call = (name: string, args = {}) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
  let synthesisSeen = false;
  faux.setResponses([
    input => {
      const wire = JSON.stringify(input);
      assert.equal(wire.includes('native_strategy_walk_forward'), false, 'active guidance must not request a retired tool');
      assert.ok(wire.includes('native_strategy_backtest'));
      return call('native_strategy_configure', { request: A });
    },
    call('native_strategy_logic_review', { hypothesis: 'Explicit separated windows, protocol test only.', validationMethod: { requests: declaredB ? [A, B] : [A], purposes: declaredB ? ['TRAINING_PROTOCOL_CASE', 'HELD_OUT_PROTOCOL_CASE'] : ['TRAINING_PROTOCOL_CASE'], protocol: 'Read separately; do not derive folds or select winners.' } }),
    fauxAssistantMessage(JSON.stringify(judgment(3))),
    call('native_strategy_backtest'), call('native_strategy_configure', { request: B }), call('native_strategy_backtest'),
    fauxAssistantMessage('Explicit runs finished.'),
  ]);
  const host = await openResearch({ database: join(directory, 'research.sqlite'), models });
  const reportResponses: Parameters<typeof faux.setResponses>[0] = [
    call('native_strategy_assessment_report'),
    input => {
      synthesisSeen = true; const wire = JSON.stringify(input);
      assert.ok(wire.includes('EXPLICIT_PI_CONFIGURATIONS_NO_QZ_SPLITTER_OR_WINNER'));
      assert.ok(wire.includes(A.event_start_ns)); assert.ok(wire.includes(B.event_start_ns));
      return fauxAssistantMessage(JSON.stringify({ ...judgment(4), evidenceSufficient: true, dataJudgment: judgment(2) }));
    },
    fauxAssistantMessage('Original explicit results reviewed; no automatic splitting or selection.'),
  ];
  try {
    const receipt = await host.start(`explicit-${declaredB}`, { goal: 'Protocol-only workflow validation', model: { provider: faux.provider.id, modelId: faux.getModel().id },
      data: { catalog: directory, nautilusTarget: target, native_strategy_backtest_request: A }, policy: {} });
    const settled = await (await host.harness.submission(receipt.submissionId, context))!.wait(context);
    assert.equal(settled.status, 'done');
    const conversation = await host.harness.conversation(receipt.conversationId, context); assert.ok(conversation);
    // Persist a retired invocation in the official store, after the valid pre-review.
    // Its old request shape must remain history, never become an explicit pre-reviewed run.
    const historicalId = await conversation.commit(tx => tx.createTask(NativeStrategyBacktestTask, {
      request: { candidates: [A], split: { n_splits: 2 }, objective: 'SHARPE' }, catalog: directory,
      target, walkForward: true,
    }, { ownership: { kind: 'conversation' } }), context);
    assert.equal((await host.harness.waitForTask(historicalId, context)).state.outcome.status, 'failed');
    faux.setResponses(reportResponses);
    await (await conversation.submit({ type: 'input', content: 'Review explicit runs with historical records visible.', requestId: 'synthesize-history' }, context)).wait(context);
    assert.equal(synthesisSeen, true);
    const posts=(await readFile(marker,'utf8')).trim().split('\n').map(line=>JSON.parse(line));
    assert.deepEqual(posts.map(post => post.input), [A, B]);
    assert.ok(posts.every(post=>post.command==='native-strategy-backtest'));
    assert.ok(posts.every(post=>JSON.stringify(post.args)===JSON.stringify(['--catalog',directory])));
    const executions=(await host.harness.commit(tx=>tx.scanTasks({kind:'qz.native.execution'},10),context)).items;
    assert.equal(executions.length,2);
    assert.notEqual((executions[0]!.input as any).requestId,(executions[1]!.input as any).requestId,'distinct backtests keep distinct persisted invocation identities');
    const assessment = await host.harness.snapshot(ResearchAssessment, receipt.conversationId, context); assert.ok(assessment);
    const logic = await host.harness.getTask(assessment.logicTaskId!, context); const report = await host.harness.getTask(assessment.reportTaskIds[0]!, context);
    assert.equal(logic?.state.outcome?.status, 'completed'); assert.equal(report?.state.outcome?.status, 'completed');
    if (logic?.state.outcome?.status !== 'completed' || report?.state.outcome?.status !== 'completed') assert.fail('missing actual assessment Tasks');
    assert.equal((logic.state.outcome.result as any).judgment.stars, 3);
    const result = report.state.outcome.result as any;
    assert.equal(result.judgment.stars, declaredB ? 4 : null, JSON.stringify(result.evidence));
    assert.equal(result.dataJudgment.stars, declaredB ? 2 : null);
    assert.equal(result.evidence.logic.taskId ?? null, declaredB ? logic.id : null);
    assert.equal(result.evidence.trialHistory.runs.length, 3);
    const historical = result.evidence.trialHistory.runs.find((run: any) => run.taskId === historicalId);
    assert.equal(historical.execution, 'HISTORICAL_RETIRED_QZ_WALK_FORWARD');
    assert.equal(historical.preReviewedRequest, false);
    assert.deepEqual(historical.request, { candidates: [A], split: { n_splits: 2 }, objective: 'SHARPE' });
    assert.equal(result.tradingPermission, false);
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});
