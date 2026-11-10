import { readResearchAdmissions } from '../src/admissions.ts';
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
import { crashFixtureService, crashResponses } from './pm-start-crash-fixture.ts';

const context = BACKGROUND_CONTEXT;
for (const mode of ['create', 'settled']) for (const changed of [false, true]) {
  test(`PM SQLite crash recovery resumes official ${mode} intake boundary${changed ? ' and rejects a changed objective' : ''}`, { timeout: 40_000 }, async () => {
    const directory = await mkdtemp(join(tmpdir(), 'qz-pm-start-crash-'));
    let opened: Awaited<ReturnType<typeof crashFixtureService>> | undefined;
    try {
      const child = new URL('./pm-start-crash-fixture.ts', import.meta.url).pathname;
      await assert.rejects(promisify(execFile)(process.execPath, ['--experimental-strip-types', child, '--crash', directory, mode, changed ? 'changed' : 'same']),
        (error: unknown) => { assert.equal((error as { code: number }).code, 73, String(error)); return true; });
      const saved = JSON.parse(await readFile(join(directory, 'crash.json'), 'utf8'));
      opened = await crashFixtureService(directory, mode === 'create');
      const { service, faux } = opened;
      const intake = await service.research.harness.waitForTask(saved.native.taskId, context);
      assert.equal(intake.state.outcome.status, 'completed');
      if (intake.state.outcome.status !== 'completed') throw new Error('intake_did_not_complete');
      const actualReceipt = intake.state.outcome.result as { conversationId: number; submissionId: number };
      await (await service.research.harness.submission(actualReceipt.submissionId as never, context))!.wait(context);
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
      assert.equal(native.taskId, saved.native.taskId); assert.equal(native.inputDigest, saved.native.inputDigest);
      assert.deepEqual(native.input, saved.native.input); assert.deepEqual(native.metadata, saved.native.metadata);
      assert.equal(native.submissionId, actualReceipt.submissionId);
      const study = (await service.research.harness.snapshot(Study, saved.receipt.conversationId, context))!;
      assert.deepEqual(study.input, saved.study.input); assert.equal(study.trials.length, 1);
      if (mode === 'settled') assert.deepEqual(study, saved.study);
      assert.equal(faux.state.callCount, mode === 'create' ? 3 : 0, 'only unfinished original work runs after recovery');
      assert.deepEqual(record.budget, saved.budget, 'the original scientific budget remains intact');
      if (changed) assert.equal(record.campaigns[saved.campaignId]!.state, 'PAUSED');
      else {
        assert.notEqual(record.campaigns[saved.campaignId]!.conclusionReason, 'request_id_conflict');
        const admission = (await readResearchAdmissions(service.research))[key]!;
        assert.deepEqual(admission, saved.admission);
        assert.deepEqual(admission.original, saved.body);
        const replay = await service.work.startResearch(saved.body); assert.deepEqual(replay, actualReceipt);
        await assert.rejects(service.work.startResearch({ ...saved.body, input: { ...saved.body.input, goal: 'Another objective' } }), /request_id_conflict/);
        await assert.rejects(service.work.startResearch({ ...saved.body, input: { ...saved.body.input, policy: { ...saved.body.input.policy, evidenceMode: 'HISTORICAL_REASSESSMENT' } } }), /request_id_conflict/,
          'The public API keeps the exact original scientific request identity');
        assert.equal(Object.keys((await service.research.harness.snapshot(Requests, context))!.requests).filter(id => id.startsWith('request:pm:')).length, 1);
      }
    } finally { await opened?.service.close(); await rm(directory, { recursive: true, force: true }); }
  });
}
