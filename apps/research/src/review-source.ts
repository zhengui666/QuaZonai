import { createHash } from 'node:crypto';
import { defineDoc, defineTask } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, TaskId } from '@earendil-works/pi-durable';
import type { Context } from '@earendil-works/chord';
import type { NativeTarget } from '../../../packages/contracts/src/nautilus.ts';
import type { ReviewSourceLicense } from '../../../packages/contracts/src/review-source.ts';
import type { ReviewCheck } from '../../../packages/contracts/src/review.ts';
import { canonical } from './documents.ts';
import { executeNativeValue, nativeTargetForData, retainNativeRemoteUncertainty, withNativeRemoteTracking } from './native-execution.ts';
import type { ResearchRuntime } from './research-runtime.ts';
import { mkdtemp, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';

export interface CapturedReviewSource {
  schemaVersion: 1; partition: 'VALIDATION' | 'HELDOUT'; datasetId: string; sourceId: string; sourceRevision: string; snapshotDigest: string; inputDataDigest: string;
  metadataDigest: string; licenseDigest: string; metadata: JsonObject; license: ReviewSourceLicense;
  receivedAt: string; validationStartNs: string; validationEndNs: string;
  independent?: CapturedReviewSource;
}
/** Only the service's source consumer writes captured; model tools never have a write route. */
export const ReviewSourceEvidence = defineDoc<{ captured: JsonObject | null; native: JsonObject | null; taskId: TaskId<JsonObject> | null }>({
  kind: 'qz.review-source-evidence', version: 1, scope: 'conversation', history: 'latest', fork: 'initial',
  initial: () => ({ captured: null, native: null, taskId: null }),
});
const SourceSnapshots = defineDoc<{ digests: Record<string, string> }>({ kind: 'qz.review-source-snapshots', version: 1, scope: 'session', initial: () => ({ digests: {} }) });
export const ReviewIndependentSourceEvidence = defineDoc<{ captured: JsonObject | null; native: JsonObject | null; taskId: TaskId<JsonObject> | null }>({
  kind: 'qz.review-independent-source-evidence', version: 1, scope: 'conversation', history: 'latest', fork: 'initial',
  initial: () => ({ captured: null, native: null, taskId: null }),
});
const object = (value: unknown): JsonObject | null => value !== null && typeof value === 'object' && !Array.isArray(value) ? value as JsonObject : null;
/** Binding ignores only connection credentials; all scientific source inputs remain part of the digest. */
export function reviewSourceDataDigest(data: JsonObject): string {
  const { nautilusTarget: _target, ...rest } = data;
  const sealed = object(rest.sealed); if (sealed) { const { nautilusTarget: _nested, ...source } = sealed; rest.sealed = source; }
  return createHash('sha256').update(canonical(rest)).digest('hex');
}
export async function bindReviewSourceEvidence(tx: import('@earendil-works/pi-durable').Tx, id: ConversationId, input: JsonObject, source: CapturedReviewSource) {
  if (source.partition !== 'VALIDATION') throw new Error('review_primary_source_partition_mismatch');
  if (source.inputDataDigest !== reviewSourceDataDigest(input)) throw new Error('review_source_input_mismatch');
  const snapshots = await tx.doc(SourceSnapshots);
  const key = canonical({ datasetId: source.datasetId, sourceId: source.sourceId, sourceRevision: source.sourceRevision });
  if (snapshots.digests[key] && snapshots.digests[key] !== source.snapshotDigest) throw new Error('review_source_revision_conflict');
  snapshots.digests[key] ??= source.snapshotDigest;
  const doc = await tx.doc(ReviewSourceEvidence, id);
  const { independent } = source;
  const captured = JSON.parse(JSON.stringify(source)) as JsonObject;
  if (doc.captured && canonical(doc.captured) !== canonical(captured)) throw new Error('review_source_identity_conflict');
  doc.captured ??= captured;
  if (independent) {
    const sealed = object(input.sealed);
    if (!sealed || independent.partition !== 'HELDOUT' || independent.independent !== undefined || independent.inputDataDigest !== reviewSourceDataDigest(sealed)) throw new Error('review_independent_source_input_mismatch');
    const independentKey = canonical({ datasetId: independent.datasetId, sourceId: independent.sourceId, sourceRevision: independent.sourceRevision });
    if (snapshots.digests[independentKey] && snapshots.digests[independentKey] !== independent.snapshotDigest) throw new Error('review_independent_source_revision_conflict');
    snapshots.digests[independentKey] ??= independent.snapshotDigest;
    const independentDoc = await tx.doc(ReviewIndependentSourceEvidence, id);
    const body = JSON.parse(JSON.stringify(independent)) as JsonObject;
    if (independentDoc.captured && canonical(independentDoc.captured) !== canonical(body)) throw new Error('review_independent_source_identity_conflict');
    independentDoc.captured ??= body;
  }
}


export const ReviewSourceValidationTask = defineTask<{ metadata: JsonObject; observedAt: string; target: NativeTarget }, { phase: 'validate' }, JsonObject>({
  name: 'qz.review.source-validate', version: 1, initial: () => ({ phase: 'validate' }),
  phases: { async validate(task, runtime, context) {
    return withNativeRemoteTracking(task.id, runtime, context, async () => {
      const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-review-source-'));
      try {
        const value = await executeNativeValue(task.input.target, 'review-source', { schema_version: 1,
          metadata: task.input.metadata, observed_at: task.input.observedAt }, [], directory, context);
        const result = object(value);
        if (result?.schema_version !== 1 || result.method_version !== 'qz.review-source.v1' || result.validated !== true
          || canonical(object(result.metadata)!) !== canonical(task.input.metadata)) throw new Error('review_source_native_binding_mismatch');
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'completed', result } }), context);
      } catch (error) {
        await retainNativeRemoteUncertainty(error, task.id, runtime, context);
        if (runtime.signal.aborted) throw error;
        await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'failed', error: { message: 'review_source_native_validation_failed' } } }), context);
      } finally { await rm(directory, { recursive: true, force: true }); }
    });
  } },
  async abort(_task, runtime, context) { await runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context); },
});

export function trustedSourceChecks(captured: CapturedReviewSource | null, native: JsonObject | null, inputData: JsonObject, atNs: string): ReviewCheck[] {
  if (!captured) return [];
  const ref = `source:${captured.datasetId}:${captured.metadataDigest}`;
  const check = (id: string, status: ReviewCheck['status'], reason: string): ReviewCheck => ({ id, status, reason, evidenceRefs: [ref] });
  if (captured.inputDataDigest !== reviewSourceDataDigest(inputData)) return [check('provenance', 'FAIL', 'trusted_source_input_binding_changed')];
  if (!native || native.validated !== true || native.method_version !== 'qz.review-source.v1' || canonical(object(native.metadata)!) !== canonical(captured.metadata)) return [check('provenance', 'UNKNOWN', 'native_source_report_validation_missing')];
  const metadata = captured.metadata; const license = captured.license;
  if (metadata.partition !== (captured.partition === 'HELDOUT' ? 'SEALED' : 'VALIDATION')) return [check('provenance', 'FAIL', 'source_metadata_partition_binding_changed')];
  if (license.status === 'REVOKED' || BigInt(atNs) >= BigInt(license.validUntilNs)) return [check('provenance', 'FAIL', 'source_authorization_revoked_or_expired')];
  if (BigInt(atNs) < BigInt(license.validFromNs)) return [check('provenance', 'UNKNOWN', 'source_authorization_not_yet_valid')];
  if (metadata.origin !== 'REAL') return [check('provenance', 'UNKNOWN', 'source_origin_not_real')];
  if (metadata.pit_status === 'INVALID') return [check('provenance', 'FAIL', 'source_owner_declares_invalid_pit')];
  if (metadata.pit_status !== 'VERIFIED' || metadata.revision_policy !== 'AS_KNOWN_THEN') return [check('provenance', 'UNKNOWN', 'source_owner_pit_evidence_unverified')];
  if (!license.purposes.includes('RESEARCH') || !metadata.availability_provenance || !metadata.provenance_reference) return [check('provenance', 'UNKNOWN', 'source_authority_or_availability_reference_missing')];
  return [check('provenance', 'PASS', 'manifest_snapshot_bytes_native_metadata_and_source_owner_research_grant_bound'),
    check('no_leakage', 'PASS', 'native_validated_as_known_then_source_owner_pit_evidence')];
}

/** Resolve the source report from the official conversation, never caller-supplied review arguments. */
export async function sourceReviewEvidence(api: ResearchRuntime, context: Context, data: JsonObject, partition: 'VALIDATION' | 'HELDOUT' = 'VALIDATION'): Promise<{ checks: ReviewCheck[]; evidenceRefs: string[]; summary: JsonObject | null }> {
  const document = partition === 'HELDOUT' ? ReviewIndependentSourceEvidence : ReviewSourceEvidence;
  const source = await api.snapshot(document, api.conversationId, context);
  if (!source?.captured) return { checks: [], evidenceRefs: [], summary: null };
  const captured = source.captured as unknown as CapturedReviewSource;
  if (captured.partition !== partition) return { checks: [{ id: 'provenance', status: 'FAIL', reason: 'source_evidence_purpose_mismatch', evidenceRefs: [`source:${captured.datasetId}:${captured.metadataDigest}`] }], evidenceRefs: [`source:${captured.datasetId}:${captured.metadataDigest}`], summary: null };
  let id: TaskId<JsonObject> | undefined;
  await api.commit(async tx => {
    const doc = await tx.doc(document, api.conversationId);
    doc.taskId ??= await tx.createTask(ReviewSourceValidationTask, { metadata: captured.metadata, observedAt: captured.receivedAt, target: nativeTargetForData(data) }, { ownership: { kind: 'task', taskId: api.taskId } });
    id = doc.taskId;
  }, context);
  const settled = await api.waitForTask(id!, context);
  const native = settled.state.outcome.status === 'completed' ? settled.state.outcome.result : null;
  await api.commit(async tx => { (await tx.doc(document, api.conversationId)).native = native; }, context);
  const checks = trustedSourceChecks(captured, native, data, (BigInt(Date.now()) * 1_000_000n).toString());
  return { checks, evidenceRefs: [...new Set(checks.flatMap(check => check.evidenceRefs))], summary: {
    datasetId: captured.datasetId, sourceId: captured.sourceId, sourceRevision: captured.sourceRevision, snapshotDigest: captured.snapshotDigest,
    metadataDigest: captured.metadataDigest, licenseDigest: captured.licenseDigest, grantId: captured.license.grantId,
    partition: captured.partition, origin: captured.metadata.origin!, pitStatus: captured.metadata.pit_status!, revisionPolicy: captured.metadata.revision_policy!,
    nativeValidation: native ? 'VALIDATED' : 'FAILED', checks: JSON.parse(JSON.stringify(checks)),
    limitation: 'Source-owner PIT and authorization evidence is attributed to the configured producer. Byte hashes and native consistency checks do not independently reconstruct historical receive clocks.',
  } };
}

/** Optional consumers read the source's original settled validation receipt; they cannot upload a replacement. */
export async function readTrustedSourceChecks(reader: import('@earendil-works/pi-durable').DocumentReader, researchId: ConversationId, inputData: JsonObject, context: Context): Promise<ReviewCheck[]> {
  const source = await reader.snapshot(ReviewSourceEvidence, researchId, context);
  return source?.captured ? trustedSourceChecks(source.captured as unknown as CapturedReviewSource, source.native, inputData,
    (BigInt(Date.now()) * 1_000_000n).toString()) : [];
}
