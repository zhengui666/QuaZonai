import { createHash } from 'node:crypto';
import type { Context } from '@earendil-works/chord';
import type { ConversationId, DocumentReader, JsonObject, TaskRuntime } from '@earendil-works/pi-durable';
import type { EvidenceRef } from '../../../packages/contracts/src/pm.ts';
import { NativeEvidencePart, readNativeEvidenceParts, retainNativeEvidence } from '../../research/plugins/native-evidence.ts';
import type { PmScienceCheckpoint } from './pm-science.ts';
import { object, Problem } from './facts.ts';

/** Same canonical JSON SHA-256 as business digest, without duplicating a whole
 * native tape into an intermediate string or parsed object. */
export function pmArtifactDigest(value: JsonObject): string {
  const hash = createHash('sha256');
  function visit(item: JsonObject[string] | undefined): void {
    if (item === null || typeof item !== 'object') { hash.update(JSON.stringify(item) ?? 'null'); return; }
    if (Array.isArray(item)) {
      hash.update('['); item.forEach((entry, index) => { if (index) hash.update(','); visit(entry); }); hash.update(']'); return;
    }
    hash.update('{'); let first = true;
    for (const key of Object.keys(item).filter(key => item[key] !== undefined).sort()) {
      if (!first) hash.update(','); first = false; hash.update(JSON.stringify(key)); hash.update(':'); visit(item[key]);
    }
    hash.update('}');
  }
  visit(value); return hash.digest('hex');
}

type Runtime<I, S, R, H extends object> = TaskRuntime<I, S, R, H>;
async function save<I, S, R, H extends object>(runtime: Runtime<I, S, R, H>, namespace: string, value: JsonObject, ctx: Context) {
  return retainNativeEvidence(value, async (index, data) => {
    await runtime.commit(async tx => { (await tx.doc(NativeEvidencePart, runtime.conversationId, `${runtime.taskId}:${namespace}:${index}`, null)).data = data; return undefined; }, ctx);
  });
}
async function load(reader: DocumentReader, marker: JsonObject, ctx: Context): Promise<JsonObject> {
  if (typeof marker.conversationId !== 'number' || typeof marker.taskId !== 'number' || typeof marker.namespace !== 'string') throw new Problem('pm_native_bundle_reference_invalid');
  return readNativeEvidenceParts(object(marker.native, 'native_bundle_descriptor'), async index => {
    const part = await reader.snapshot(NativeEvidencePart, marker.conversationId as ConversationId, `${marker.taskId}:${marker.namespace}:${index}`, ctx);
    if (!part) throw new Problem('pm_native_bundle_part_missing'); return part.data;
  });
}

/** One final artifact bundle per official decision task. Original keys and raw
 * content remain owner-readable; no additional store or task identity exists. */
export async function retainPmArtifacts<I, S, R, H extends object>(runtime: Runtime<I, S, R, H>, artifacts: Record<string, JsonObject>, ctx: Context, inlineKeys: readonly string[] = []): Promise<Record<string, JsonObject>> {
  if (!Object.keys(artifacts).length) return artifacts;
  const namespace = 'pm-result-artifacts'; const native = await save(runtime, namespace, artifacts, ctx);
  if (native.format !== 'qz.native-evidence/gzip-json/v1') return artifacts;
  return Object.fromEntries(Object.keys(artifacts).map(key => [key, inlineKeys.includes(key) ? artifacts[key]! : { kind: 'PM_NATIVE_ARTIFACT_BUNDLE', conversationId: runtime.conversationId,
    taskId: runtime.taskId, namespace, native, key }]));
}
export async function readPmArtifact(reader: DocumentReader, reference: EvidenceRef, payload: JsonObject, ctx: Context): Promise<JsonObject> {
  if (payload.kind !== 'PM_NATIVE_ARTIFACT_BUNDLE') return payload;
  if (payload.key !== reference.objectKey || payload.taskId !== reference.runtime?.taskId || payload.conversationId !== reference.runtime?.conversationId) throw new Problem('pm_native_bundle_identity_mismatch');
  const bundle = await load(reader, payload, ctx); const artifact = object(bundle[String(payload.key)], 'native_bundle_artifact');
  if (reference.contentDigest !== null && pmArtifactDigest(artifact) !== reference.contentDigest) throw new Problem('pm_native_bundle_digest_mismatch');
  return artifact;
}

/** Scientific checkpoint namespaces are separate from final output bundles.
 * Official memo keeps the original key; gzip parts retain the exact original
 * fingerprint/value consumed by the scientific adapter after recovery. */
export function pmScienceCheckpoint<I, S, R, H extends object>(runtime: Runtime<I, S, R, H>, prefix: string, ctx: Context): PmScienceCheckpoint {
  return {
    async read(key) {
      const saved = await runtime.memo<JsonObject>(`${prefix}:${key}`, ctx);
      if (!saved || saved.kind !== 'PM_COMPRESSED_SCIENCE_CHECKPOINT') return saved;
      if (saved.taskId !== runtime.taskId || saved.conversationId !== runtime.conversationId || saved.namespace !== `pm-checkpoint:${prefix}:${key}`) throw new Problem('pm_science_checkpoint_identity_mismatch');
      return load(runtime, saved, ctx);
    },
    async write(key, value) {
      const namespace = `pm-checkpoint:${prefix}:${key}`; const native = await save(runtime, namespace, value, ctx);
      await runtime.memo(`${prefix}:${key}`, native.format === 'qz.native-evidence/gzip-json/v1'
        ? { kind: 'PM_COMPRESSED_SCIENCE_CHECKPOINT', conversationId: runtime.conversationId, taskId: runtime.taskId, namespace, native } : value, ctx);
    },
  };
}
