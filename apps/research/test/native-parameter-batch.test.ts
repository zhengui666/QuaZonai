import assert from 'node:assert/strict';
import test from 'node:test';
import { mkdtemp, rm, writeFile, readFile } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { openResearch } from '../src/host.ts';
import { nativeParameterGridTool, nativeParameterGridPage, prepareNativeParameterGrid, prepareNativeParameterBatch, nativeParameterBatchReadTool } from '../plugins/native-strategy-backtest.ts';
import { Study } from '../src/documents.ts';
import { ResearchConfiguration } from '../src/research-configuration.ts';
import type { ProcessNativeTarget } from '../../../packages/contracts/src/nautilus.ts';

const requests = [2, 3, 4].map(period => ({ schema_version: 1, name: 'owner::Installed', config: { period, precision: '0.000000000000000019' } }));
test('batch requires explicit complete candidates, never expands a grid', () => {
  assert.deepEqual(prepareNativeParameterBatch({ configured: true, requests }), { configured: true, requests });
  assert.throws(() => prepareNativeParameterBatch({ configured: true, requests: [] }), /requires_explicit/);
  assert.throws(() => prepareNativeParameterBatch({ configured: true, grid: { period: [2, 3] } }), /requires_explicit/);
  assert.throws(() => prepareNativeParameterBatch({ configured: false, requests }), /schema_invalid/);
  assert.throws(() => prepareNativeParameterBatch({ configured: true, requests: [requests[0], { config: {} }] }), /schema_invalid/);
});

const grid = { configured: true, request: requests[0]!, axes: [{ path: ['config', 'period'], values: [2, 3, 4] }], offset: 0, pageSize: 3 };
test('QZ grid preserves exact values and deterministic Cartesian order across pages', () => {
  const twoAxes = { ...grid, axes: [...grid.axes, { path: ['config', 'precision'], values: ['0.000000000000000019', '0.000000000000000023'] }], pageSize: 2 };
  const all = [0, 2, 4].flatMap(offset => nativeParameterGridPage({ ...twoAxes, offset }).requests as any[]);
  assert.deepEqual(all.map(x => [x.config.period, x.config.precision]), [2, 3, 4].flatMap(x => twoAxes.axes[1]!.values.map(y => [x, y])));
  assert.equal(nativeParameterGridPage(twoAxes).nextOffset, 2);
  assert.equal(nativeParameterGridPage({ ...twoAxes, offset: 4 }).nextOffset, null);
  assert.deepEqual(nativeParameterGridPage(grid).requests, requests);
  assert.equal(grid.request.config.period, 2, 'base request remains unchanged');
  assert.throws(() => prepareNativeParameterGrid({ ...grid, axes: [] }), /invalid/);
  assert.throws(() => prepareNativeParameterGrid({ ...grid, axes: [{ path: ['config'], values: [] }] }), /empty_axis/);
  assert.throws(() => prepareNativeParameterGrid({ ...grid, axes: [{ path: ['config', '__proto__'], values: [1] }] }), /path_missing/);
  assert.throws(() => prepareNativeParameterGrid({ ...grid, axes: [{ path: ['missing'], values: [1] }] }), /path_missing/);
  assert.throws(() => prepareNativeParameterGrid({ ...grid, axes: [...grid.axes, { path: ['config'], values: [{}] }] }), /overlapping/);
  assert.throws(() => nativeParameterGridPage({ ...grid, axes: [{ path: ['schema_version'], values: [2] }] }), /schema_invalid/);
  assert.throws(() => prepareNativeParameterGrid({ ...grid, offset: Number.MAX_SAFE_INTEGER + 1 }), /invalid/);
});

test('grid tool prepare-to-execute preserves the strict public argument schema', async () => {
  const prepared = nativeParameterGridTool.prepareArguments!(grid);
  assert.equal(Object.hasOwn(prepared, 'total'), false);
  const result = await nativeParameterGridTool.execute(prepared, {} as any, context);
  assert.deepEqual(JSON.parse((result.output![0] as any).text).requests, requests);
});

test('grid pages are byte bounded without materializing the entire product', () => {
  const values = Array.from({ length: 2000 }, (_, i) => String(i).padEnd(100, 'x'));
  const large = { ...grid, axes: [{ path: ['config', 'precision'], values }], pageSize: 2000 };
  let offset = 0;
  const seen: unknown[] = [];
  for (;;) {
    const page = nativeParameterGridPage({ ...large, offset });
    assert.ok(Buffer.byteLength(JSON.stringify(page), 'utf8') <= 50 * 1024);
    seen.push(...(page.requests as any[]).map(x => x.config.precision));
    if (page.nextOffset === null) break;
    assert.ok(Number(page.nextOffset) > offset);
    offset = Number(page.nextOffset);
  }
  assert.deepEqual(seen, values);
});

// Official Pi orchestration with an actual local protocol process. This verifies
// dispatch/recovery, not numerical Nautilus behavior or strategy profitability.
for (const failSecond of [false, true]) test(`explicit batch retains independent child results and stops after failure=${failSecond}`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-parameter-test-'));
  const binary = join(directory, 'protocol.mjs'), calls = join(directory, 'calls.jsonl');
  await writeFile(binary, `#!${process.execPath}\nimport{appendFileSync}from'node:fs';let body='';for await(const c of process.stdin)body+=c;const request=JSON.parse(body);appendFileSync(${JSON.stringify(calls)},JSON.stringify({request,command:process.argv[2],args:process.argv.slice(3)})+'\\n');const echoed=${failSecond}&&request.config.period===3?{...request,name:'wrong::Factory'}:request;process.stdout.write(JSON.stringify({schema_version:1,request:echoed,native_version:'0.69.0',native_results:[{run_id:String(request.config.period)}],canonical_result:null,canonical_unavailable_reason:'PROTOCOL_TEST_ONLY'}));`, { mode: 0o755 });
  const target: ProcessNativeTarget = { kind: 'RUST_PROCESS', id: 'batch-protocol', binary, capabilities: { schemaVersion: 1, protocol: 'quazonai-native-process-v1', identity: { nautilusVersion: 'protocol-only', precision: 'high' }, research: { singleStrategyRegistration: true, configuredRunFactory: true, multiCatalogBindings: true }, account: { compiled: false, registration: false, command: 'account-runtime', requiresExplicitStart: true }, running: false } };
  const models = createModels(), faux = fauxProvider(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_parameter_grid', grid), { stopReason: 'toolUse' }), fauxAssistantMessage(fauxToolCall('native_parameter_batch', { configured: true, requests: nativeParameterGridPage(grid).requests }), { stopReason: 'toolUse' }), fauxAssistantMessage('Explicit batch finished; no automatic ranking or OOS claim.')]);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  const input = { goal: 'Run the explicit installed Rust factory candidates, no live trading.', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, catalogBindings:[{datasetId:"primary",catalog:directory},{datasetId:"book",catalog:directory+"-book"}], nautilusTarget: target }, policy: {} };
  try {
    const admission = await host.start('explicit-batch', input);
    await (await host.harness.submission(admission.submissionId, context))!.wait(context);
    const launches = (await readFile(calls, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    assert.deepEqual(launches.map(row => row.request), requests.slice(0, failSecond ? 2 : 3));
    for (const launch of launches) { assert.equal(launch.command, 'native-configured-backtest'); assert.deepEqual(launch.args, ['--catalog', directory, '--additional-catalog', directory+'-book']); }
    const tasks = (await host.harness.commit(tx => tx.scanTasks({ conversationId: admission.conversationId, kind: 'qz.native-strategy.backtest' }, 20), context)).items;
    assert.equal(tasks.length, launches.length);
    for (const task of tasks) {
      assert.equal(task.state.status, 'terminal');
      if (task.state.status !== 'terminal') continue;
      const request = (task.input as any).request;
      assert.equal(task.state.outcome.status, failSecond && request.config.period === 3 ? 'failed' : 'completed');
      if (task.state.outcome.status === 'completed') assert.deepEqual((task.state.outcome.result as any).request, request);
    }
    const final = (await host.harness.snapshot(Study, admission.conversationId, context))!.final;
    if (failSecond) assert.equal(final, null);
    else { assert.equal(final?.status, 'COMPUTED'); assert.match(String(final!.reason), /last_case_not_best/); assert.deepEqual((final!.result as any).request, requests[2]); }
    const configuration = await host.harness.snapshot(ResearchConfiguration, admission.conversationId, context);
    assert.equal(configuration!.previousTaskIds!.length, launches.length - 1);
    assert.deepEqual(configuration!.request, requests[launches.length - 1]);
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.start('explicit-batch', input), admission);
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, launches.length, 'reopening retains original Tasks without resubmitting');
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});

test('an uncertain process result retains its original child and never starts the next candidate', { timeout: 15_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-native-parameter-unknown-'));
  const binary = join(directory, 'uncertain.mjs'), calls = join(directory, 'calls.jsonl');
  await writeFile(binary, `#!${process.execPath}\nimport{appendFileSync}from'node:fs';let body='';for await(const c of process.stdin)body+=c;appendFileSync(${JSON.stringify(calls)},body+'\\n');process.exit(1);`, { mode: 0o755 });
  const target: ProcessNativeTarget = { kind: 'RUST_PROCESS', id: 'uncertain-protocol', binary, capabilities: { schemaVersion: 1, protocol: 'quazonai-native-process-v1', identity: { nautilusVersion: 'protocol-only', precision: 'high' }, research: { singleStrategyRegistration: true, configuredRunFactory: true, multiCatalogBindings: true }, account: { compiled: false, registration: false, command: 'account-runtime', requiresExplicitStart: true }, running: false } };
  const models = createModels(), faux = fauxProvider(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_parameter_batch', { configured: true, requests }), { stopReason: 'toolUse' })]);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  try {
    const admission = await host.start('uncertain-batch', { goal: 'Explicit batch protocol test', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, nautilusTarget: target }, policy: {} });
    const deadline = Date.now() + 5000;
    let originalId: number | undefined;
    for (;;) {
      const tasks = (await host.harness.commit(tx => tx.scanTasks({ conversationId: admission.conversationId, kind: 'qz.native-strategy.backtest' }, 10), context)).items;
      assert.ok(tasks.length <= 1, 'later candidate must not start while original result is unknown');
      if (tasks[0]?.state.status === 'sleeping') { originalId = tasks[0].id; break; }
      assert.ok(Date.now() < deadline, 'uncertain task was not retained');
      await new Promise(resolve => setTimeout(resolve, 10));
    }
    await host.close(); host = await openResearch(options);
    await new Promise(resolve => setTimeout(resolve, 1100));
    const tasks = (await host.harness.commit(tx => tx.scanTasks({ conversationId: admission.conversationId, kind: 'qz.native-strategy.backtest' }, 10), context)).items;
    assert.equal(tasks.length, 1); assert.equal(tasks[0]!.id, originalId);
    assert.equal((await readFile(calls, 'utf8')).trim().split('\n').length, 1);
    const final = (await host.harness.snapshot(Study, admission.conversationId, context))!.final;
    assert.equal(final, null, 'uncertainty cannot be reported as a completed batch');
  } finally { await host.close(); await rm(directory, { recursive: true, force: true }); }
});


test('batch reference reader pages large candidate lists without truncating or ranking', async () => {
  const ids = Array.from({ length: 2000 }, (_, i) => i + 10);
  // Reader-only API boundary: no market data or numerical results are fabricated.
  const api = { conversationId: 1,
    commit: async (f: any) => f({ task: async (id: number) => id === 1 ? { conversationId: 1 } : { state: { status: 'terminal', outcome: { status: 'completed' } } } }),
    snapshot: async () => ({ taskIds: ids }),
  };
  const read = nativeParameterBatchReadTool.execute as any;
  let offset = 0; const seen: number[] = [];
  for (;;) {
    const result = await read({ batchTaskId: 1, offset, pageSize: 10000 }, api, context);
    const text = result.output![0].text;
    assert.ok(Buffer.byteLength(text) <= 50 * 1024);
    const page = JSON.parse(text);
    assert.equal(page.totalStarted, ids.length);
    seen.push(...page.rows.map((row: any) => row.taskId));
    if (page.nextOffset === null) break;
    assert.ok(page.nextOffset > offset); offset = page.nextOffset;
  }
  assert.deepEqual(seen, ids);
  await assert.rejects(() => read({ batchTaskId: 1, offset: 0, pageSize: 1 }, { ...api, conversationId: 2 }, context), /not_found/);
});
