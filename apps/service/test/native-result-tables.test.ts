import assert from 'node:assert/strict';
import { test } from 'node:test';
import type { JsonObject, ConversationId, TaskId } from '@earendil-works/pi-durable';
import type { ResearchHost } from '../../research/src/host.ts';
import { NativeTableReader } from '../../research/src/native-table.ts';
import { nativeResultTableReferences, nativeWebTablePage, researchNativeTables } from '../src/native-result-tables.ts';
import { setup } from './support.ts';

test('native table references preserve nested paths, empty tables and original field values', () => {
  const original: JsonObject = { canonical_result: { fills: [{ price: '9007199254740993.000000001' }], positions: [] }, equity_performance: { returns: [{ ts_event_ns: '1776124800000000001', value: null }] } };
  const before = JSON.stringify(original);
  const rows = nativeResultTableReferences(original, 17);
  assert.deepEqual(rows, [
    { name: 'fills', path: '$.canonical_result.fills', rowCount: 1, resultPath: '$' },
    { name: 'positions', path: '$.canonical_result.positions', rowCount: 0, resultPath: '$' },
    { name: 'equity_returns', path: '$.equity_performance.returns', rowCount: 1, resultPath: '$' },
  ]);
  const nested: JsonObject = { folds: [{ index: 0, trials: [{ candidate_index: 0, result: original }], test_result: null }] };
  assert.equal(nativeResultTableReferences(nested, 17)[0]!.path, '$.folds[0].trials[0].result.canonical_result.fills');
  assert.equal(JSON.stringify(original), before);
});

test('HTTP table page has a fixed byte budget, follows actual offsets and does not fill missing values', () => {
  const rows = Array.from({ length: 30 }, (_, ordinal) => ({ ordinal, exact: '0.000000000000000001', missing: null, note: '值'.repeat(1000) }));
  const first = nativeWebTablePage(rows, 17, '$.canonical_result.fills', 0, 1000);
  assert.ok(first.rows.length > 0 && first.rows.length < rows.length);
  assert.ok(Buffer.byteLength(JSON.stringify(first)) <= 50 * 1024);
  assert.equal(first.nextOffset, first.rows.length);
  const second = nativeWebTablePage(rows, 17, '$.canonical_result.fills', first.nextOffset!, 1000);
  assert.deepEqual([...first.rows, ...second.rows], rows);
  assert.equal(second.nextOffset, null);
  assert.deepEqual(nativeWebTablePage([], 17, '$.canonical_result.positions', 0, 200).rows, []);
  assert.throws(() => nativeWebTablePage([{ note: 'x'.repeat(60 * 1024) }], 17, '$.canonical_result.fills', 0, 1), /native_table_row_exceeds_page_budget/);
});

test('table read uses original stored pages and rejects unrelated tasks, paths and incomplete results', async () => {
  const parts = new Map<string, string>();
  const sourceRows = Array.from({ length: 180 }, (_, ordinal) => ({ ordinal, ts_event: '1776124800000000001', exact: '0.000000000000000001', description: 'x'.repeat(1000) }));
  const parser = new NativeTableReader({ conversationId: 7 as ConversationId, taskId: 12 as TaskId }, async (key, data) => { parts.set(key, data); });
  await parser.write(JSON.stringify({ canonical_result: { fills: sourceRows, positions: [] }, equity_performance: { returns: [] } }));
  const result = await parser.finish() as JsonObject;
  let source: any = { id: 11, conversationId: 7, kind: 'qz.native-strategy.backtest', state: { status: 'terminal', outcome: { status: 'completed', result } } };
  let producer: any = { id: 12, owner: 11, conversationId: 7, kind: 'qz.native.execution' };
  const original = JSON.stringify(source);
  let reads = 0;
  const host = { harness: { commit: async (operation: (tx: unknown) => unknown) => operation({
    task: async (id: number) => id === 11 ? source : id === 12 ? producer : null,
    doc: async (_definition: unknown, conversation: number, key?: string) => {
      assert.equal(conversation, 7);
      if (key === undefined) return { input: { goal: 'Stored table read only' }, deletedAt: null };
      reads++; return { data: parts.get(key) ?? '' };
    },
  }) } } as unknown as ResearchHost;
  const manifest = await researchNativeTables(host, 7, 11, undefined);
  assert.equal(manifest.kind, 'TABLES'); assert.equal(reads, 0);
  const received: unknown[] = []; let offset: number | null = 0;
  while (offset !== null) {
    const page = await researchNativeTables(host, 7, 11, '$.canonical_result.fills', offset, 1000);
    assert.equal(page.totalRows, sourceRows.length);
    assert.ok(page.endExclusive > offset);
    assert.ok(Buffer.byteLength(JSON.stringify(page)) <= 50 * 1024);
    received.push(...page.rows); offset = page.nextOffset;
  }
  assert.deepEqual(received, sourceRows); assert.equal(JSON.stringify(source), original);
  const empty = await researchNativeTables(host, 7, 11, '$.canonical_result.positions');
  assert.equal(empty.totalRows, 0); assert.equal(empty.nextOffset, null);
  await assert.rejects(researchNativeTables(host, 8, 11, '$.canonical_result.fills'), /research_evidence_not_found/);
  await assert.rejects(researchNativeTables(host, 7, 11, '$.request.private'), /native_table_path_invalid/);
  await assert.rejects(researchNativeTables(host, 7, 11, '$.canonical_result.fills', 181), /native_table_page_invalid/);
  for (const count of [0, 1001, Infinity, 1.5]) await assert.rejects(researchNativeTables(host, 7, 11, undefined, 0, count), /native_table_page_invalid/);
  producer = { ...producer, owner: 99 };
  await assert.rejects(researchNativeTables(host, 7, 11, '$.canonical_result.fills'), /research_evidence_not_found/);
  producer = { ...producer, owner: 11, conversationId: 99 };
  await assert.rejects(researchNativeTables(host, 7, 11, '$.canonical_result.fills'), /research_evidence_not_found/);
  source = { ...source, state: { status: 'terminal', outcome: { status: 'failed', error: { message: 'original_failure' } } } };
  await assert.rejects(researchNativeTables(host, 7, 11, undefined), /native_result_not_completed/);
  source = { ...source, kind: 'qz.research.assessment' };
  await assert.rejects(researchNativeTables(host, 7, 11, undefined), /research_evidence_not_found/);
});

test('native table HTTP route retains owner authentication and validates pagination', async () => {
  const state = await setup();
  try {
    const url = '/api/v3/research/1/native-tables?taskId=1';
    assert.equal((await state.service.app.inject({ url })).statusCode, 401);
    assert.equal((await state.service.app.inject({ url: `${url}&pageSize=1001`, headers: state.headers })).statusCode, 422);
    assert.equal((await state.service.app.inject({ url, headers: state.headers })).statusCode, 404);
  } finally { await state.cleanup(); }
});
