import assert from 'node:assert/strict';
import test from 'node:test';
import { createServer } from 'node:http';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { BACKGROUND_CONTEXT as context } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { openResearch } from '../src/host.ts';
import { Study } from '../src/documents.ts';
import { ResearchConfiguration } from '../src/research-configuration.ts';
import { nativeCommands } from '../src/native-execution.ts';
import { isTickNativeCommand } from '../../../packages/contracts/src/nautilus.ts';
import { isNativeTable, readNativeTable } from '../src/native-table.ts';
import { NativeEvidencePart } from '../plugins/native-evidence.ts';
import { nativeResultView } from '../src/native-result-view.ts';

// Official Pi + HTTP protocol, not evidence that a custom Rust strategy was compiled or profitable.
for (const retainCanonical of [false, true]) test(`typed Rust factory tool retains original results and reopens without rerunning (canonical=${retainCanonical})`, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-configured-'));
  const request = { schema_version: 1, name: 'owner::MultiInstrument', config: { instruments: ['BTC.SIM', 'ETH.SIM'], exact: '0.000000000000000019', nested: [null, true] } };
  const report = { schema: 'nautilus-backtest-result/v1', orders: retainCanonical ? Array.from({ length: 1200 }, (_, i) => ({ index: i, quantity: '0.000000000000000019', detail: 'original'.repeat(32) })) : [], fills: [], positions: [], accounts: [], portfolio_snapshots: [], position_snapshots: [], statistics: {} };
  const result = { schema_version: 1, request, native_version: '0.69.0', native_results: [{ run_id: 'official-run', total_orders: 0 }], canonical_result: retainCanonical ? report : null, canonical_unavailable_reason: retainCanonical ? null : 'OMITTED_BY_DISPOSE_ON_COMPLETION_CONFIG' };
  const posts: any[] = [];
  const server = createServer(async (incoming, response) => {
    response.setHeader('content-type', 'application/json');
    if (incoming.url === '/v1/capabilities') { response.end(JSON.stringify({ schemaVersion: 1, implementationDigest: `sha256:${'a'.repeat(64)}`, commands: [...nativeCommands] })); return; }
    let body = ''; for await (const part of incoming) body += part;
    const operation = JSON.parse(body); posts.push(operation);
    response.end(JSON.stringify({ schemaVersion: 1, requestId: operation.requestId, status: 'COMPLETED', result }));
  });
  await new Promise<void>(resolve => server.listen(0, '127.0.0.1', resolve));
  const address = server.address(); assert.ok(address && typeof address !== 'string');
  const models = createModels(), faux = fauxProvider(); models.setProvider(faux.provider);
  faux.setResponses([fauxAssistantMessage(fauxToolCall('native_configured_backtest', { request, finishResearch: true }), { stopReason: 'toolUse' })]);
  const options = { database: join(directory, 'research.sqlite'), models };
  let host = await openResearch(options);
  const input = { goal: 'Run the installed owner Rust multi-instrument factory, no live trading.', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: { catalog: directory, nautilusTarget: { id: 'factory', url: `http://127.0.0.1:${address.port}` } }, policy: {} };
  try {
    const admission = await host.start('factory-protocol', input);
    await (await host.harness.submission(admission.submissionId, context))!.wait(context);
    const study = (await host.harness.snapshot(Study, admission.conversationId, context))!;
    assert.equal(study.final?.status, 'COMPUTED');
    const stored = study.final!.result as any;
    if (retainCanonical) {
      const table = stored.canonical_result.orders; assert.ok(isNativeTable(table)); assert.equal(table.rowCount, 1200);
      let offset = 0;
      while (offset < table.rowCount) {
        const rows = await readNativeTable(table, offset, 1200, async key => (await host.harness.snapshot(NativeEvidencePart, table.conversationId, key, context))!.data);
        assert.deepEqual(rows, report.orders.slice(offset, offset + rows.length)); assert.ok(rows.length); offset += rows.length;
      }
      assert.deepEqual(stored, { ...result, canonical_result: { ...report, orders: table } });
    } else assert.deepEqual(stored, result);
    assert.equal(posts.length, 1); assert.equal(posts[0].command, 'native-configured-backtest'); assert.deepEqual(posts[0].input, request); assert.deepEqual(posts[0].args, ['--catalog', directory]);
    assert.equal(isTickNativeCommand(posts[0].command), true); assert.equal(nativeCommands.has('native-walk-forward'), false);
    const executions = await host.harness.commit(tx => tx.scanTasks({ kind: 'qz.native.execution' }, 10), context);
    assert.equal(executions.items.length, 1);
    assert.equal((executions.items[0]!.input as any).receiptRequired, true);
    const configuration = await host.harness.snapshot(ResearchConfiguration, admission.conversationId, context);
    const view = nativeResultView(stored, configuration!.evaluationTaskId!);
    assert.equal(view.canonical_result === null, !retainCanonical); assert.equal('equity_performance' in view, false);
    await host.close(); host = await openResearch(options);
    assert.deepEqual(await host.start('factory-protocol', input), admission);
    assert.deepEqual((await host.harness.snapshot(Study, admission.conversationId, context))!.final?.result, stored);
    assert.equal(posts.length, 1);
  } finally { await host.close(); await new Promise<void>(resolve => server.close(() => resolve())); await rm(directory, { recursive: true, force: true }); }
});
