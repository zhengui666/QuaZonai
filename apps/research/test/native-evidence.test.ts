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
