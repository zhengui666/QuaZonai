import assert from 'node:assert/strict';
import { test } from 'node:test';
import { createHash } from 'node:crypto';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { createServer } from 'node:http';
import type { Server } from 'node:http';
import { once } from 'node:events';
import type { AddressInfo } from 'node:net';
import type { JsonObject } from '@earendil-works/pi-durable';
import { canonical, Requests } from '../../research/src/documents.ts';
import { ReviewSourceEvidence, ReviewSourceValidationTask, trustedSourceChecks } from '../../research/src/review-source.ts';
import { Application, json } from '../src/facts.ts';
import { assertCurrentReviewSource } from '../src/review-source.ts';
import { downstream, isPreTransportRejection } from '../src/downstream.ts';
import type { ObservationTaskInput } from '../src/downstream.ts';
import type { CapturedReviewSource } from '../../research/src/review-source.ts';
import { context, setup, fauxAssistantMessage } from './support.ts';

const hash = (text: string) => createHash('sha256').update(text).digest('hex');
// Actual service/owner HTTP, official Durable, and a controlled subprocess wire
// protocol. This does not establish market PIT or a real Rust validation result.
test('source evidence crosses original admission and repair with exact bytes and one native receipt', { timeout: 30_000 }, async () => {
  const state = await setup(); const previous = process.env.QZ_JOB_BINARY; let receiver: Server | undefined;
  try {
    const dataset = state.options.config.datasets[0]!;
    const selection = { schema_version: 1, maximum_rows: 100, event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] };
    const request = { forecast: { selection }, train_end_ns: '140', validation_end_ns: '180' };
    const metadata = { registered_ref: dataset.id, native_snapshot_ref: 'snapshot:source-admission', partition: 'VALIDATION', origin: 'REAL',
      pit_status: 'VERIFIED', revision_policy: 'AS_KNOWN_THEN', provenance_reference: 'fixture-source-owner', availability_provenance: 'controlled source-owner assertion',
      quality: { datasets: [{ dataset_revision_id: 'dataset:source-admission', instrument_ids: ['TEST.SIM'], selection: { ...selection, event_start_ns: '140', event_end_ns: '180' } }] } };
    const license = { schemaVersion: 1, datasetId: dataset.id, sourceId: dataset.source, sourceRevision: 'snapshot:source-admission', datasetRevisionId: 'dataset:source-admission',
      grantId: 'fixture:source-grant', issuer: 'fixture-source-owner', reference: 'fixture-only-license', status: 'ACTIVE', purposes: ['RESEARCH'], validFromNs: '0', validUntilNs: '9223372036854775807' };
    const files: JsonObject[] = [];
    for (const [path, text] of Object.entries({ 'catalog/bars.parquet': 'CONTROLLED_PROTOCOL_ONLY', 'metadata.json': JSON.stringify(metadata), 'license.json': JSON.stringify(license) })) {
      await writeFile(join(state.directory, path), text); files.push({ path, sha256: hash(text), bytes: String(Buffer.byteLength(text)) });
    }
    const manifest = { schemaVersion: 1, datasetId: dataset.id, sourceRevision: 'snapshot:source-admission', availableAtNs: '250', revisionKind: 'APPEND',
      data: { catalog: 'catalog', request }, files };
    await writeFile(join(state.directory, 'revision.json'), JSON.stringify({ ...manifest, contentDigest: hash(canonical(manifest)) }));
    dataset.data = { catalog: join(state.directory, 'catalog'), request };
    dataset.revisionManifest = join(state.directory, 'revision.json');
    dataset.reviewSourceEvidence = { metadata: { path: 'metadata.json', sha256: hash(JSON.stringify(metadata)) }, license: { path: 'license.json', sha256: hash(JSON.stringify(license)) } };
    const sourceBinary = join(state.directory, 'source-job.mjs');
    await writeFile(sourceBinary, `#!${process.execPath}\nlet raw='';for await(const chunk of process.stdin)raw+=chunk;const input=JSON.parse(raw);if(process.argv[2]!=='review-source')throw new Error('unexpected_native_command');console.log(JSON.stringify({schema_version:1,method_version:'qz.review-source.v1',validated:true,metadata:input.metadata}));\n`, { mode: 0o700 });
    process.env.QZ_JOB_BINARY = sourceBinary;
    await state.restart();
    const body = { requestId: 'source-admission-original', goal: 'Source receipt protocol only.', model: state.model, datasetId: dataset.id };
    const admit = (payload: object) => state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload });
    const answer = () => fauxAssistantMessage('Source capture protocol; native validation is explicitly invoked by this test.');
    state.faux.setResponses([answer()]);
    const [response, concurrent] = await Promise.all([admit(body), admit(body)]);
    assert.equal(response.statusCode, 202, response.body); assert.equal(concurrent.statusCode, 202, concurrent.body);
    const receipt = response.json(); assert.deepEqual(concurrent.json(), receipt);
    await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const capturedSource = (await state.service.research.harness.snapshot(ReviewSourceEvidence, receipt.conversationId, context))!.captured as unknown as CapturedReviewSource;
    const taskId = await state.service.research.harness.commit(tx => tx.createTask(ReviewSourceValidationTask,
      { metadata: capturedSource.metadata, observedAt: capturedSource.receivedAt, target: sourceBinary },
      { conversationId: receipt.conversationId, ownership: { kind: 'conversation' } }), context);
    const settled = await state.service.research.harness.waitForTask(taskId, context);
    assert.equal(settled.state.outcome.status, 'completed');
    const native = settled.state.outcome.status === 'completed' ? settled.state.outcome.result : null;
    assert.equal(trustedSourceChecks(capturedSource, native, dataset.data, (BigInt(Date.now()) * 1_000_000n).toString()).find(check => check.id === 'provenance')!.status, 'PASS');
    await state.service.research.harness.commit(async tx => {
      const doc = await tx.doc(ReviewSourceEvidence, receipt.conversationId); doc.taskId = taskId; doc.native = native;
    }, context);
    const original = await state.service.research.harness.snapshot(ReviewSourceEvidence, receipt.conversationId, context);
    assert.ok(original!.captured); assert.equal(original!.native!.validated, true); assert.ok(original!.taskId);
    assert.equal(original!.captured!.metadataDigest, dataset.reviewSourceEvidence.metadata.sha256);
    const requests = await state.service.research.harness.snapshot(Requests, context);
    assert.deepEqual(requests!.requests[`request:${body.requestId}`]!.sourceEvidence, original!.captured);
    assert.equal((await admit(body)).json().submissionId, receipt.submissionId);
    assert.equal(state.faux.state.callCount, 1);
    await state.restart();
    assert.deepEqual(await state.service.research.harness.snapshot(ReviewSourceEvidence, receipt.conversationId, context), original);
    assert.deepEqual((await admit(body)).json(), receipt); assert.equal(state.faux.state.callCount, 1);
    await writeFile(join(state.directory, 'metadata.json'), JSON.stringify({ ...metadata, origin: 'CHANGED' }));
    assert.deepEqual((await admit(body)).json(), receipt);
    const changed = await admit({ ...body, requestId: 'source-admission-new-after-mutation' });
    assert.equal(changed.statusCode, 400, changed.body); assert.match(changed.json().error, /source_manifest_file_(?:digest_)?mismatch/);
    assert.equal(state.faux.state.callCount, 1);
    // A later source-owner revocation is a current consumption fence. Its
    // changed files cannot rewrite the historical captured grant or receipt.
    await writeFile(join(state.directory, 'metadata.json'), JSON.stringify(metadata));
    const captured = original!.captured as unknown as CapturedReviewSource;
    assert.equal((await assertCurrentReviewSource(dataset, captured)).license.status, 'ACTIVE');
    let posts = 0; const reads: string[] = [];
    receiver = createServer((request, response) => {
      if (request.method === 'POST') posts++; else reads.push(request.url!);
      response.setHeader('content-type', 'application/json'); response.end(JSON.stringify({ id: 'retained-receipt', status: 'ACKNOWLEDGED' }));
    });
    receiver.listen(0, '127.0.0.1'); await once(receiver, 'listening');
    const delivery: ObservationTaskInput = { id: 'fixture-observation', receiptId: 'retained-receipt',
      connection: { id: 'fixture-connection', name: 'Controlled receiver', mode: 'paper', clientId: 'fixture-client', accountId: 'fixture-account', url: `http://127.0.0.1:${(receiver.address() as AddressInfo).port}` },
      target: { schema_version: 1, valid_until_ns: (BigInt(Date.now()) * 1_000_000n + 60_000_000_000n).toString(), targets: [], cash_weight: '1' },
      reviewSources: [{ configured: structuredClone(dataset), captured }] };
    const signal = new AbortController().signal;
    assert.equal((await downstream(delivery, 'deliver', signal)).status, 'ACKNOWLEDGED'); assert.equal(posts, 1);
    const revokedBody = JSON.stringify({ ...license, status: 'REVOKED' });
    await writeFile(join(state.directory, 'license.json'), revokedBody);
    const revokedFiles = files.map(file => file.path === 'license.json' ? { ...file, sha256: hash(revokedBody), bytes: String(Buffer.byteLength(revokedBody)) } : file);
    const revokedManifest = { ...manifest, files: revokedFiles };
    await writeFile(join(state.directory, 'revision.json'), JSON.stringify({ ...revokedManifest, contentDigest: hash(canonical(revokedManifest)) }));
    dataset.reviewSourceEvidence.license.sha256 = hash(revokedBody);
    await assert.rejects(assertCurrentReviewSource(dataset, captured), /review_current_source_authorization_inactive/);
    await assert.rejects(downstream(delivery, 'deliver', signal), error => {
      assert.equal((error as Error).message, 'review_source_delivery_authorization_unavailable');
      assert.equal(isPreTransportRejection(error), true); return true;
    });
    assert.equal(posts, 1, 'revoked original source cannot produce another POST');
    assert.equal((await downstream(delivery, 'receipt', signal)).id, 'retained-receipt');
    assert.equal((await downstream(delivery, 'account', signal)).status, 'ACKNOWLEDGED');
    assert.deepEqual(reads, ['/targets/retained-receipt', '/account']); assert.equal(posts, 1);
    assert.equal(captured.license.status, 'ACTIVE');
    assert.deepEqual((await admit(body)).json(), receipt);
    // A fresh source revision must not get past admission merely because its
    // revoked grant has not appeared in the existing snapshot ledger before.
    const nextMetadataBody = JSON.stringify({ ...metadata, native_snapshot_ref: 'snapshot:revoked-before-admission' });
    const nextLicenseBody = JSON.stringify({ ...license, sourceRevision: 'snapshot:revoked-before-admission', status: 'REVOKED' });
    await writeFile(join(state.directory, 'metadata.json'), nextMetadataBody);
    await writeFile(join(state.directory, 'license.json'), nextLicenseBody);
    const nextFiles = files.map(file => file.path === 'metadata.json' ? { ...file, sha256: hash(nextMetadataBody), bytes: String(Buffer.byteLength(nextMetadataBody)) }
      : file.path === 'license.json' ? { ...file, sha256: hash(nextLicenseBody), bytes: String(Buffer.byteLength(nextLicenseBody)) } : file);
    const nextManifest = { ...manifest, sourceRevision: 'snapshot:revoked-before-admission', files: nextFiles };
    await writeFile(join(state.directory, 'revision.json'), JSON.stringify({ ...nextManifest, contentDigest: hash(canonical(nextManifest)) }));
    dataset.reviewSourceEvidence.metadata.sha256 = hash(nextMetadataBody); dataset.reviewSourceEvidence.license.sha256 = hash(nextLicenseBody);
    await state.restart();
    const revokedAdmission = await admit({ ...body, requestId: 'source-revoked-first-admission' });
    assert.equal(revokedAdmission.statusCode, 409, revokedAdmission.body);
    assert.match(revokedAdmission.json().error, /source.*authorization_inactive/);
    assert.equal(state.faux.state.callCount, 1);
  } finally { if (previous === undefined) delete process.env.QZ_JOB_BINARY; else process.env.QZ_JOB_BINARY = previous; await state.cleanup(); if (receiver) await new Promise<void>((resolve, reject) => receiver!.close(error => error ? reject(error) : resolve())); }
});
