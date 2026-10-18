import { defineDocFamily } from '@earendil-works/pi-durable';
import type { ConversationId, DocumentReader, JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import { Readable } from 'node:stream';
import { StringDecoder } from 'node:string_decoder';
import { createGzip, createGunzip } from 'node:zlib';
import { JsonReader, jsonChunks } from '../src/native-json.ts';
import { object } from '../src/trials.ts';

// Persistent original-study facts, keyed by native child ID and part index in the same official store.
export const NativeEvidencePart = defineDocFamily<{ data: string }, null>({
  kind: 'qz.native-evidence-part', version: 1, scope: 'conversation', history: 'latest', fork: 'initial', family: true, initial: () => ({ data: '' }),
});
const format = 'qz.native-evidence/gzip-json/v1';
const partSize = 1024 * 1024;

export async function retainNativeEvidence(evidence: JsonObject, save: (index: number, data: string) => Promise<void>): Promise<JsonObject> {
  const chunks = jsonChunks(evidence, partSize);
  const first = chunks.next(); const second = chunks.next();
  if (second.done) return evidence;
  const compressed = Readable.from((function* () { yield first.value!; yield second.value; yield* chunks; })()).compose(createGzip());
  let buffers: Buffer[] = []; let bytes = 0; let parts = 0;
  const flush = async () => {
    await save(parts++, Buffer.concat(buffers, bytes).toString('base64'));
    buffers = []; bytes = 0;
  };
  try {
    for await (const chunk of compressed) {
      const buffer = chunk as Buffer;
      buffers.push(buffer); bytes += buffer.length;
      if (bytes >= partSize) await flush();
    }
    if (bytes) await flush();
    return { format, parts };
  } finally { compressed.destroy(); }
}

/** Owner-side read of the original native child; never model-facing final feedback. */
export async function readNativeEvidence(reader: DocumentReader, conversationId: ConversationId, taskId: TaskId, native: JsonObject, context: Context): Promise<JsonObject> {
  if (native.format !== format) return native;
  const count = native.parts;
  if (typeof count !== 'number' || !Number.isInteger(count) || count < 1) throw new Error('native_evidence_parts_invalid');
  async function* parts(count: number) {
    for (let index = 0; index < count; index++) {
      const part = await reader.snapshot(NativeEvidencePart, conversationId, `${taskId}:${index}`, context);
      if (!part) throw new Error('native_evidence_part_missing');
      yield Buffer.from(part.data, 'base64');
    }
  }
  const decoded = Readable.from(parts(count)).compose(createGunzip());
  const parser = new JsonReader(); const decoder = new StringDecoder('utf8');
  try {
    for await (const chunk of decoded) parser.write(decoder.write(chunk as Buffer));
    parser.write(decoder.end());
    return object(parser.finish(), 'native_evidence');
  } finally { decoded.destroy(); }
}
