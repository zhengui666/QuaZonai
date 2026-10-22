import assert from 'node:assert/strict';
import { test } from 'node:test';
import Fastify from 'fastify';
import type { TaskId } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { Application, json, object } from '../src/facts.ts';
import { captureAccount } from '../src/pm-account.ts';
import { campaignAccount, campaignAccountKey, freezeCampaignAccount } from '../src/pm-campaign-account.ts';
import { PmDocument, pmState } from '../src/pm-facts.ts';
import { accountSourceFixture } from './pm-account-fixture.ts';
import { context, setup } from './support.ts';

test('campaign account: real current source 503 stays unknown while original historical cycle basis survives restart', { timeout: 30_000 }, async () => {
  const state = await setup(); const f = accountSourceFixture(); const remote = Fastify();
  let accountReads = 0; let capabilityReads = 0;
  remote.get('/account', () => { accountReads++; return f.snapshot; });
  remote.get('/capabilities', (_req, reply) => { capabilityReads++; return reply.code(503).send({ error: 'test_current_source_unavailable' }); });
  const url = await remote.listen({ host: '127.0.0.1', port: 0 });
  const connection = { ...f.connection, url };
  try {
    const dataset = state.options.config.datasets[0]!; dataset.currency = 'USDC';
    dataset.data.request = { train_end_ns: '140', validation_end_ns: '180', forecast: { schema_version: 1, parameters: {}, selection: {
      event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], maximum_rows: 100 } } };
    state.options.config.connections = [connection]; await state.restart();
    const original = await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application);
      app.observations[connection.id] = json({ ...f.observation, connection });
      app.accountControls = json(f.application.accountControls) as typeof app.accountControls;
      return captureAccount(app, connection.id);
    }, context);
    const admission = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers,
      payload: { requestId: 'frozen-account-start', goal: 'Test historical source isolation, no financial performance evidence.', datasetId: dataset.id,
        funding: { kind: 'ACCOUNT', accountBindingId: connection.id }, model: state.model, maxTrials: 1 } });
    assert.equal(admission.statusCode, 202, admission.body); const view = admission.json<Pm.PmMandateView>();
    const campaignId = view.campaigns[0]!.id; const id = view.mandate.id; const key = campaignAccountKey(campaignId);
    let record = pmState(await state.service.research.harness.snapshot(PmDocument, context)).mandates[id]!;
    assert.deepEqual(campaignAccount(record, record.campaigns[campaignId]!), original);
    const originalMarker = structuredClone(record.artifacts[key]);
    const refresh = await state.service.app.inject({ method: 'POST', url: `/api/v3/observations/${encodeURIComponent(connection.id)}/refresh`, headers: state.headers });
    assert.equal(refresh.statusCode, 400, refresh.body); assert.equal(refresh.json().error, 'downstream_control_503');
    assert.ok(accountReads > 0 && capabilityReads > 0, 'The real source read was performed, not skipped to preserve old health.');
    const app = (await state.service.research.harness.snapshot(Application, context))!;
    const current = captureAccount(app, connection.id); assert.equal(current.basis.reserved, null);
    assert.equal(object(current.artifact.capitalControlFailure).code, 'downstream_control_503');
    assert.notEqual(current.evidence.id, original.evidence.id);
    const recompute = await state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(id)}/recompute`, headers: state.headers,
      payload: { requestId: 'after-real-source-failure', expectedRevision: view.mandate.revision } });
    assert.equal(recompute.statusCode, 202, recompute.body); const cycle = recompute.json<Pm.PmMandateView>().lastCycle!;
    await state.service.research.harness.waitForTask(cycle.runtime.taskId as TaskId, context);
    record = pmState(await state.service.research.harness.snapshot(PmDocument, context)).mandates[id]!;
    assert.deepEqual(record.cycles[cycle.id]!.accountBasis, original.basis);
    assert.deepEqual(record.artifacts[key], originalMarker);
    assert.equal(record.cycles[cycle.id]!.state, 'WAITING_EVIDENCE');
    assert.equal(Object.values(record.commands).length, 0, 'Retaining historical evidence does not grant target authority.');
    await state.restart();
    record = pmState(await state.service.research.harness.snapshot(PmDocument, context)).mandates[id]!;
    assert.deepEqual(campaignAccount(record, record.campaigns[campaignId]!), original);
    assert.equal(captureAccount((await state.service.research.harness.snapshot(Application, context))!, connection.id).basis.reserved, null);

    // Recovery has no live-source fallback. Only this campaign's original
    // admitted historical cycle can establish its missing legacy marker.
    const legacy = structuredClone(record); delete legacy.artifacts[key];
    assert.deepEqual(campaignAccount(legacy, legacy.campaigns[campaignId]!), original);
    assert.deepEqual(legacy.artifacts[key], originalMarker);
    const absent = structuredClone(record); delete absent.artifacts[key]; absent.cycles = {};
    assert.throws(() => campaignAccount(absent, absent.campaigns[campaignId]!), /campaign_account_start_missing/);
    const wrongCampaign = structuredClone(record); delete wrongCampaign.artifacts[key]; wrongCampaign.artifacts[cycle.id]!.campaignId = 'another-campaign';
    assert.throws(() => campaignAccount(wrongCampaign, wrongCampaign.campaigns[campaignId]!), /campaign_account_start_missing/);
    const forwardOnly = structuredClone(record); delete forwardOnly.artifacts[key]; forwardOnly.artifacts[cycle.id]!.operatingForward = true;
    assert.throws(() => campaignAccount(forwardOnly, forwardOnly.campaigns[campaignId]!), /campaign_account_start_missing/);
    const changedDataset = structuredClone(record); changedDataset.campaigns[campaignId]!.datasetRevisionIds = ['dataset:changed'];
    assert.throws(() => campaignAccount(changedDataset, changedDataset.campaigns[campaignId]!), /campaign_account_start_dataset_mismatch/);
    const ambiguous = structuredClone(record); delete ambiguous.artifacts[key];
    ambiguous.evidence[current.evidence.id] = current.evidence; ambiguous.artifacts[current.evidence.objectKey] = current.artifact;
    ambiguous.cycles['cycle:other-captured-start'] = { ...ambiguous.cycles[cycle.id]!, id: 'cycle:other-captured-start', accountBasis: current.basis };
    ambiguous.artifacts['cycle:other-captured-start'] = { ...ambiguous.artifacts[cycle.id]! };
    assert.throws(() => campaignAccount(ambiguous, ambiguous.campaigns[campaignId]!), /campaign_account_start_ambiguous/);
    const corrupt = structuredClone(record); object(corrupt.artifacts[original.evidence.objectKey]!.snapshot).sequence = '1';
    assert.throws(() => campaignAccount(corrupt, corrupt.campaigns[campaignId]!), /campaign_account_original_evidence_mismatch/);
    assert.throws(() => freezeCampaignAccount(record, record.campaigns[campaignId]!, current), /campaign_account_start_immutable/);
    const newCampaign = { ...record.campaigns[campaignId]!, id: 'campaign:new-current-unknown', datasetRevisionIds: ['dataset:next'] };
    freezeCampaignAccount(record, newCampaign, current);
    assert.equal(campaignAccount(record, newCampaign).basis.reserved, null, 'A new campaign cannot borrow the old campaign\'s healthy control.');
    assert.deepEqual(record.artifacts[key], originalMarker);
  } finally { await state.cleanup(); await remote.close(); }
});
