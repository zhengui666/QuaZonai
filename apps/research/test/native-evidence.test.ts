import assert from 'node:assert/strict';
import { randomBytes } from 'node:crypto';
import { test } from 'node:test';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import type { DocumentReader, JsonObject, TaskId } from '@earendil-works/pi-durable';
import { NativeEvidencePart, readNativeEvidence, retainNativeEvidence } from '../plugins/native-evidence.ts';

const taskId = 1 as TaskId;

test('small native evidence retains its original inline representation', async () => {
  const native = { members: [{ model: 'original', weight: '1' }], simulation: { cash: '3000.000000' } };
  const stored = await retainNativeEvidence(native, async () => { assert.fail('Small evidence must stay inline'); });
  assert.equal(stored, native);
  assert.equal(await readNativeEvidence({} as DocumentReader, taskId, stored, BACKGROUND_CONTEXT), native);
});

test('complete compressed native evidence spans original-child documents and reads back losslessly', async () => {
  const native: JsonObject = { original: randomBytes(2 * 1024 * 1024).toString('base64'),
    members: [{ model: '原生🙂', weight: '0.000000000000000001' }], clock: '1791849600000000000',
    simulation: { cash: '3000.000000', fee: '0.000001', missing: null } };
  const parts = new Map<string, { data: string }>();
  const stored = await retainNativeEvidence(native, async (index, data) => { parts.set(String(index), { data }); });
  assert.equal(stored.format, 'qz.native-evidence/gzip-json/v1');
  assert.equal(stored.parts, parts.size);
  assert.ok(parts.size > 1);
  const reader = { async snapshot(token: unknown, id: TaskId, key: string) {
    assert.equal(token, NativeEvidencePart); assert.equal(id, taskId); return parts.get(key);
  } } as unknown as DocumentReader;
  assert.deepEqual(await readNativeEvidence(reader, taskId, stored, BACKGROUND_CONTEXT), native);
  parts.delete(String(parts.size - 1));
  await assert.rejects(readNativeEvidence(reader, taskId, stored, BACKGROUND_CONTEXT), /native_evidence_part_missing/);
});

test('a failed native evidence part save cannot publish a completed representation', async () => {
  const failure = new Error('original-store-write-failed');
  await assert.rejects(retainNativeEvidence({ raw: 'x'.repeat(2 * 1024 * 1024) }, async () => { throw failure; }), failure);
});
