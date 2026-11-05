import assert from 'node:assert/strict';
import { test } from 'node:test';
import { execFile } from 'node:child_process';
import { promisify } from 'node:util';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { join } from 'node:path';
import { tmpdir } from 'node:os';
import { setTimeout as delay } from 'node:timers/promises';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { Requests, Study } from '../../research/src/documents.ts';
import { PmDocument, pmState } from '../src/pm-facts.ts';
import { crashFixtureService } from './pm-start-crash-fixture.ts';

const context = BACKGROUND_CONTEXT;
for (const mode of ['application', 'native']) for (const changed of [false, true]) {
  test(`PM SQLite crash recovery reuses legacy ${mode} admission${changed ? ' and rejects a changed objective' : ''}`, { timeout: 40_000 }, async () => {
    const directory = await mkdtemp(join(tmpdir(), 'qz-pm-start-crash-'));
    let opened: Awaited<ReturnType<typeof crashFixtureService>> | undefined;
    try {
      const child = new URL('./pm-start-crash-fixture.ts', import.meta.url).pathname;
      await assert.rejects(promisify(execFile)(process.execPath, ['--experimental-strip-types', child, '--crash', directory, mode, changed ? 'changed' : 'same']),
        (error: unknown) => { assert.equal((error as { code: number }).code, 73, String(error)); return true; });
      const saved = JSON.parse(await readFile(join(directory, 'crash.json'), 'utf8'));
      opened = await crashFixtureService(directory);
      const { service, faux } = opened;
      let record = pmState(await service.research.harness.snapshot(PmDocument, context)).mandates[saved.mandateId]!;
      const deadline = Date.now() + 20_000;
      for (;;) {
        record = pmState(await service.research.harness.snapshot(PmDocument, context)).mandates[saved.mandateId]!;
        const campaign = record.campaigns[saved.campaignId]!;
        if (changed ? campaign.conclusionReason === 'request_id_conflict' : campaign.runtime.conversationId === saved.receipt.conversationId) break;
        assert.ok(Date.now() < deadline, JSON.stringify(campaign)); await delay(15);
      }
      const key = `request:${saved.body.requestId}`;
      const native = (await service.research.harness.snapshot(Requests, context))!.requests[key]!;
      assert.deepEqual(native, saved.native);
      assert.deepEqual(await service.research.harness.snapshot(Study, saved.receipt.conversationId, context), saved.study);
      assert.equal(faux.state.callCount, 0, 'Reconnecting cannot create a new model submission or trial');
      assert.equal(record.budget.maximumTrials, 2, 'Historical budget facts remain intact');
      if (changed) assert.equal(record.campaigns[saved.campaignId]!.state, 'PAUSED');
      else {
        assert.notEqual(record.campaigns[saved.campaignId]!.conclusionReason, 'request_id_conflict');
        const admission = (await service.work.snapshot())!.researchRequests[key]!;
        if (mode === 'application') assert.deepEqual(admission, saved.admission);
        assert.deepEqual(admission.original, saved.body);
        const replay = await service.work.startResearch(saved.body); assert.deepEqual(replay, saved.receipt);
        await assert.rejects(service.work.startResearch({ ...saved.body, input: { ...saved.body.input, goal: 'Another objective' } }), /request_id_conflict/);
        await assert.rejects(service.work.startResearch({ ...saved.body, input: { ...saved.body.input, policy: { ...saved.body.input.policy, max_trials: 100 } } }), /request_id_conflict/,
          'The public API keeps strict original-body identity, even for retired quota fields');
        assert.equal(Object.keys((await service.research.harness.snapshot(Requests, context))!.requests).filter(id => id.startsWith('request:pm:')).length, 1);
      }
    } finally { await opened?.service.close(); await rm(directory, { recursive: true, force: true }); }
  });
}
