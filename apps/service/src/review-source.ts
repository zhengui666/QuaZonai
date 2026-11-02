import { createHash } from 'node:crypto';
import { readFile, realpath, lstat } from 'node:fs/promises';
import { dirname, isAbsolute, relative, resolve } from 'node:path';
import type { ConversationId, JsonObject, Tx } from '@earendil-works/pi-durable';
import type { DatasetConfig } from './config.ts';
import type { ReviewSourceLicense } from '../../../packages/contracts/src/review-source.ts';
import { readSourceRevision } from './pm-source.ts';
import { canonical } from '../../research/src/documents.ts';
import { scienceDataWithoutBinding } from './config.ts';
import { ReviewSourceEvidence } from '../../research/src/review-source.ts';
import type { CapturedReviewSource } from '../../research/src/review-source.ts';
import { Problem, counter, object, text } from './facts.ts';

/** Reads only explicitly configured files already covered by the original source manifest's byte hashes. */
export async function prepareReviewSourceEvidence(configured: DatasetConfig, purpose: 'VALIDATION' | 'HELDOUT' = 'VALIDATION'): Promise<CapturedReviewSource | null> {
  const descriptors = configured.reviewSourceEvidence;
  if (!descriptors) return null;
  const source = await readSourceRevision(configured);
  if (!source || !configured.revisionManifest) throw new Problem('review_source_manifest_required');
  if (source.revisionKind === 'RETRACTION') throw new Problem('review_source_retracted');
  if (canonical(scienceDataWithoutBinding(configured.data)) !== canonical(scienceDataWithoutBinding(source.configured.data))) throw new Problem('review_source_snapshot_not_registered');
  const root = await realpath(dirname(configured.revisionManifest));
  const read = async (descriptor: { path: string; sha256: string }, name: string) => {
    if (!descriptor || typeof descriptor.path !== 'string' || !descriptor.path || isAbsolute(descriptor.path)
      || descriptor.path.split(/[\\/]/).includes('..') || !/^[a-f0-9]{64}$/.test(descriptor.sha256)) throw new Problem('review_source_descriptor_invalid');
    const path = resolve(root, descriptor.path); const stat = await lstat(path);
    if (relative(root, path) !== descriptor.path || !stat.isFile() || stat.isSymbolicLink() || await realpath(path) !== path || stat.size > 8 * 1024 * 1024) throw new Problem('review_source_file_invalid');
    const registered = (source.manifest.files as JsonObject[]).find(file => file.path === descriptor.path);
    const bytes = await readFile(path); const digest = createHash('sha256').update(bytes).digest('hex');
    if (!registered || registered.sha256 !== descriptor.sha256 || String(bytes.length) !== registered.bytes || digest !== descriptor.sha256) throw new Problem('review_source_file_not_bound');
    return { document: object(JSON.parse(bytes.toString('utf8')), name), digest };
  };
  const metadataDescriptor = purpose === 'HELDOUT' ? descriptors.heldoutMetadata : descriptors.metadata;
  if (!metadataDescriptor) throw new Problem('review_source_heldout_metadata_required');
  const metadata = await read(metadataDescriptor, 'native_source_metadata');
  const grant = await read(purpose === 'HELDOUT' ? descriptors.heldoutLicense ?? descriptors.license : descriptors.license, 'source_license');
  const license = grant.document as unknown as ReviewSourceLicense;
  if (license.schemaVersion !== 1 || license.datasetId !== configured.id || license.sourceId !== configured.source || license.sourceRevision !== source.sourceRevision
    || !['ACTIVE', 'REVOKED'].includes(license.status) || !Array.isArray(license.purposes) || !license.purposes.includes('RESEARCH')
    || license.purposes.some(value => !['RESEARCH', 'REPORT'].includes(value))) throw new Problem('review_source_license_scope_invalid');
  for (const key of ['datasetRevisionId', 'grantId', 'issuer', 'reference'] as const) text(license[key], `review_license_${key}`);
  counter(license.validFromNs, 'review_license_start'); counter(license.validUntilNs, 'review_license_end');
  if (BigInt(license.validFromNs) >= BigInt(license.validUntilNs)) throw new Problem('review_source_license_interval_invalid');
  const quality = object(metadata.document.quality, 'native_quality');
  const dataset = Array.isArray(quality.datasets) && quality.datasets.length === 1 ? object(quality.datasets[0]) : null;
  if (metadata.document.registered_ref !== configured.id || metadata.document.native_snapshot_ref !== source.sourceRevision
    || dataset?.dataset_revision_id !== license.datasetRevisionId || metadata.document.partition !== (purpose === 'HELDOUT' ? 'SEALED' : 'VALIDATION')) throw new Problem('review_source_metadata_identity_mismatch');
  const selectedData = purpose === 'HELDOUT' && configured.data.sealed ? object(configured.data.sealed) : configured.data;
  const raw = object(selectedData.request ?? selectedData.catalog_scan_request ?? selectedData.catalog_ensemble_request, 'review_source_request');
  const selection = raw.forecast ? object(object(raw.forecast).selection) : raw;
  const from = counter(purpose === 'HELDOUT' ? raw.validation_end_ns ?? selectedData.validation_end_ns : raw.train_end_ns ?? selectedData.train_end_ns, 'review_source_start');
  const to = counter(purpose === 'HELDOUT' ? selection.event_end_ns : raw.validation_end_ns ?? selectedData.validation_end_ns, 'review_source_end');
  const measured = object(dataset.selection, 'quality_selection');
  if (selection.schema_version !== 1 || !Array.isArray(selection.bar_types) || !selection.bar_types.length
    || !selection.bar_types.every(value => typeof value === 'string') || !Number.isSafeInteger(selection.maximum_rows) || Number(selection.maximum_rows) < 1) throw new Problem('review_source_explicit_bar_selection_required');
  // Bind every original selection field, including BAR identity/order and row limit. Equal clocks alone do not bind a market.
  const expected = { ...selection, event_start_ns: from, event_end_ns: to };
  if (canonical(measured) !== canonical(expected)) throw new Problem('review_source_validation_scope_mismatch');
  const instrumentIds = [...new Set((selection.bar_types as string[]).map(type => {
    const parsed = /^(.+)-[1-9]\d*-[A-Z_]+-[A-Z_]+-(?:EXTERNAL|INTERNAL)$/.exec(type);
    if (!parsed) throw new Problem('review_source_bar_identity_unsupported');
    return parsed[1]!;
  }))].sort();
  if (!Array.isArray(dataset.instrument_ids) || canonical(dataset.instrument_ids) !== canonical(instrumentIds)) throw new Problem('review_source_instrument_coverage_mismatch');
  const evidence: CapturedReviewSource = {
    schemaVersion: 1, partition: purpose, datasetId: configured.id, sourceId: configured.source, sourceRevision: source.sourceRevision,
    snapshotDigest: source.contentDigest, inputDataDigest: createHash('sha256').update(canonical(scienceDataWithoutBinding(selectedData))).digest('hex'),
    metadataDigest: metadata.digest, licenseDigest: grant.digest, metadata: metadata.document,
    license: structuredClone(license), receivedAt: new Date().toISOString(), validationStartNs: from, validationEndNs: to,
  };
  return evidence;
}

/** Called in the original admission transaction, before any model may run. */
export async function freezeReviewSourceEvidence(tx: Tx, conversationId: ConversationId, captured: CapturedReviewSource): Promise<void> {
  const doc = await tx.doc(ReviewSourceEvidence, conversationId);
  if (doc.captured && canonical(doc.captured) !== canonical(JSON.parse(JSON.stringify(captured)) as JsonObject)) throw new Problem('review_source_identity_conflict');
  doc.captured ??= JSON.parse(JSON.stringify(captured)) as JsonObject;
}

/** A saved PASS is historical evidence; every new consumer rechecks the current source/grant bytes. */
export async function assertCurrentReviewSource(configured: DatasetConfig, captured: CapturedReviewSource): Promise<CapturedReviewSource> {
  if (!configured.ready || configured.id !== captured.datasetId || configured.source !== captured.sourceId) throw new Problem('review_current_source_unavailable', 409);
  const current = await prepareReviewSourceEvidence(configured, captured.partition);
  if (!current) throw new Problem('review_current_source_evidence_missing', 409);
  if (captured.independent) await assertCurrentReviewSource(configured, captured.independent);
  const atNs = BigInt(Date.now()) * 1_000_000n;
  if (current.license.status !== 'ACTIVE' || atNs < BigInt(current.license.validFromNs) || atNs >= BigInt(current.license.validUntilNs)) throw new Problem('review_current_source_authorization_inactive', 409);
  if (current.sourceRevision !== captured.sourceRevision || current.snapshotDigest !== captured.snapshotDigest || current.metadataDigest !== captured.metadataDigest
    || current.licenseDigest !== captured.licenseDigest || current.inputDataDigest !== captured.inputDataDigest || current.license.grantId !== captured.license.grantId) throw new Problem('review_current_source_changed_requires_new_evidence', 409);
  return current;
}
