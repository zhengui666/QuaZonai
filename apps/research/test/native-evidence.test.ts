import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { gunzipSync } from 'node:zlib';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { ConversationId, DocumentReader, JsonObject, TaskId } from '@earendil-works/pi-durable';
import { NativeEvidencePart, readNativeEvidence, retainNativeEvidence } from '../plugins/native-evidence.ts';

const taskId = 1 as TaskId;
const conversationId = 2 as ConversationId;

test('small native evidence retains its original inline representation', async () => {
  const native = { members: [{ model: 'original', weight: '1' }], simulation: { cash: '3000.000000' } };
  const stored = await retainNativeEvidence(native, async () => { assert.fail('Small evidence must stay inline'); });
  assert.equal(stored, native);
  assert.equal(await readNativeEvidence({} as DocumentReader, conversationId, taskId, stored, BACKGROUND_CONTEXT), native);
});

test('complete compressed native evidence retains its original-child association and reads back losslessly', async () => {
  const native: JsonObject = { original: randomBytes(2 * 1024 * 1024).toString('base64'),
    members: [{ model: '原生🙂', weight: '0.000000000000000001' }], clock: '1791849600000000000',
    rows: Array.from({ length: 2048 }, (_, index) => ({ index, snapshot: { balances: [
      { total: { amount: '3000.000000000000000001', currency: 'USDC' } },
    ], events: [{ source: { native: { clock: '1791849600000000001', text: '原生🐋' } } }] } })),
    simulation: { cash: '3000.000000', fee: '0.000001', missing: null } };
  const parts = new Map<string, { data: string }>();
  const stored = await retainNativeEvidence(native, async (index, data) => { parts.set(`${taskId}:${index}`, { data }); });
  assert.equal(stored.format, 'qz.native-evidence/gzip-json/v1');
  assert.equal(stored.parts, parts.size);
  assert.ok(parts.size > 1);
  assert.equal(gunzipSync(Buffer.concat([...parts.values()].map(part => Buffer.from(part.data, 'base64')))).toString('utf8'),
    JSON.stringify(native), 'stored gzip retains the original standard JSON bytes independently of the reader');
  const reader = { async snapshot(token: unknown, id: ConversationId, key: string) {
    assert.equal(token, NativeEvidencePart); assert.equal(id, conversationId); return parts.get(key);
  } } as unknown as DocumentReader;
  assert.deepEqual(await readNativeEvidence(reader, conversationId, taskId, stored, BACKGROUND_CONTEXT), native);
  parts.delete(`${taskId}:${parts.size - 1}`);
  await assert.rejects(readNativeEvidence(reader, conversationId, taskId, stored, BACKGROUND_CONTEXT), /native_evidence_part_missing/);
});

test('a failed native evidence part save cannot publish a completed representation', async () => {
  const failure = new Error('original-store-write-failed');
  await assert.rejects(retainNativeEvidence({ raw: 'x'.repeat(2 * 1024 * 1024) }, async () => { throw failure; }), failure);
});

test('a late serialization failure in the compressed stream cannot publish a completed representation', async () => {
  const failure = new Error('original-late-native-evidence-error');
  const native = { frames: Array.from({ length: 8 }, () => ({ original: '原生🐋'.repeat(100_000) })),
    get tail(): never { throw failure; } };
  await assert.rejects(retainNativeEvidence(native, async () => {}), failure);
});

test('native receipt tables retain original rows in bounded pages without duplicating them in the Task', async () => {
  const { NativeTableReader, isNativeTable, nativeRowCount, readNativeTable } = await import('../src/native-table.ts');
  const parts = new Map<string, string>();
  const parser = new NativeTableReader({ conversationId, taskId }, async (key, data) => { parts.set(key, data); });
  const rows = Array.from({ length: 1000 }, (_, i) => ({ quantity: '0.12345678901234567890', index: i, detail: '原始盘口📈'.repeat(200) }));
  const original = { status: 'COMPLETED', result: { canonical_result: { run: { outcome: 'completed' }, statistics: { returns: { Sharpe: null } }, fills: rows, accounts: [] } } };
  const encoded = JSON.stringify(original);
  for (let i = 0; i < encoded.length; i += 777) await parser.write(encoded.slice(i, i + 777));
  const result = await parser.finish() as typeof original;
  const table = result.result.canonical_result.fills;
  assert.ok(isNativeTable(table));
  assert.equal(nativeRowCount(table), rows.length);
  assert.ok(JSON.stringify(result).length < 15000);
  assert.deepEqual(result.result.canonical_result.accounts, []);
  const restored: unknown[] = [];
  for (let offset = 0; offset < rows.length;) {
    const page = await readNativeTable(table, offset, 1000000, async key => parts.get(key)!);
    assert.ok(page.length > 0 && page.length < rows.length);
    restored.push(...page); offset += page.length;
  }
  assert.deepEqual(restored, rows);
});


test('canonical-table receiver consumes a 200 MB result under a 64 MB V8 heap', async () => {
  const { execFile } = await import('node:child_process');
  const { promisify } = await import('node:util');
  const run = promisify(execFile);
  const script = `import { NativeTableReader } from ${JSON.stringify(new URL('../src/native-table.ts', import.meta.url).href)};
    let bytes=0, pages=0;
    const {gunzipSync}=await import('node:zlib');
    const parser = new NativeTableReader({taskId:1,conversationId:2}, async (_key,data)=>{bytes+=gunzipSync(Buffer.from(data,'base64')).length;pages++;});
    await parser.write('{"result":{"canonical_result":{"orders":[');
    const row=JSON.stringify({price:'123.45678901234567890',detail:'original'.repeat(128)});
    for(let i=0;i<220000;i++) await parser.write((i?',':'')+row);
    await parser.write(']}}}');
    const result=await parser.finish();
    if(bytes<200*1024*1024 || result.result.canonical_result.orders.rowCount!==220000) throw Error('incomplete');
    console.log(JSON.stringify({bytes,pages,rows:result.result.canonical_result.orders.rowCount}));`;
  const result = await run(process.execPath, ['--max-old-space-size=64', '--experimental-strip-types', '--input-type=module', '-e', script], { timeout: 120000 });
  const observed = JSON.parse(result.stdout);
  assert.ok(observed.bytes > 200 * 1024 * 1024); assert.equal(observed.rows, 220000);
});
