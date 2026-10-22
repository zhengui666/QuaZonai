import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { TaskId } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { publishDatasetRevision } from '../src/dataset-revision-publish.ts';
import { pmRecord, pmState } from '../src/pm-facts.ts';
import { context, setup } from './support.ts';

test('same-source recovery records each failure episode once across HTTP observation and Durable restart without new scientific attempts', { timeout: 75_000 }, async () => {
  const state = await setup();
  try {
    const dataset = state.options.config.datasets[0]!;
    dataset.data.request = { train_end_ns: '100', validation_end_ns: '200', forecast: { schema_version: 1, parameters: {}, selection: {
      event_start_ns: '0', event_end_ns: '300', decision_cutoff_ns: '300', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'], maximum_rows: 100 } } };
    await writeFile(join(String(dataset.data.catalog), 'protocol.txt'), 'Source availability protocol only. No investment result.');
    dataset.revisionManifest = join(state.directory, 'published-source', 'current.json');
    const published = await publishDatasetRevision(dataset, { scope: 'TEST_SOURCE_RECOVERY_PROTOCOL' }); dataset.data = published.configured.data;
    const originalManifest = await readFile(dataset.revisionManifest, 'utf8'); await state.restart();
    const started = await state.service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers: state.headers,
      payload: { requestId: 'same-source-recovery', goal: 'Disclose unavailable research evidence without manufacturing an investment conclusion.', datasetId: dataset.id,
        funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, model: state.model, maxTrials: 1 } });
    assert.equal(started.statusCode, 202, started.body); const first = started.json<Pm.PmMandateView>(); const id = first.mandate.id;
    const read = async () => {
      const response = await state.service.app.inject({ url: `/api/v3/mandates/${encodeURIComponent(id)}`, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body); return response.json<Pm.PmMandateView>();
    };
    const recompute = await state.service.app.inject({ method: 'POST', url: `/api/v3/mandates/${encodeURIComponent(id)}/recompute`, headers: state.headers,
      payload: { requestId: 'original-protocol-report', expectedRevision: first.mandate.revision } });
    assert.equal(recompute.statusCode, 202, recompute.body);
    await state.service.research.harness.waitForTask(recompute.json<Pm.PmMandateView>().lastCycle!.runtime.taskId as TaskId, context);
    const before = await read(); assert.equal(before.currentDecision!.action, 'NEEDS_EVIDENCE');
    const originalDataset = before.datasetRevisions[0]!;
    const originalTrials = structuredClone(before.researchTrials); const originalBudget = structuredClone(before.budgetMovements);
    const exposures = structuredClone((await state.service.research.harness.snapshot(Exposures, context))?.windows ?? []);
    const waitFor = async (predicate: (view: Pm.PmMandateView) => Promise<boolean>) => {
      const deadline = Date.now() + 15_000; let view = await read();
      while (!await predicate(view) && Date.now() < deadline) { await new Promise(resolve => setTimeout(resolve, 40)); view = await read(); }
      assert.ok(await predicate(view), JSON.stringify({ gaps: view.gaps, cycle: view.lastCycle, events: pmRecord(pmState(await state.service.pm.snapshot()), id).events }));
      return view;
    };
    const recoveredIds: string[] = [];
    for (let episode = 1; episode <= 2; episode++) {
      // An actual transient unreadable manifest. Original market bytes,
      // revision identity and availability are never changed or republished.
      await writeFile(dataset.revisionManifest, '{unreadable source manifest');
      await waitFor(async view => view.gaps.some(gap => gap.requiredCapability === 'source_revision' && gap.state === 'OPEN'));
      let record = pmRecord(pmState(await state.service.pm.snapshot()), id);
      const failure = Object.values(record.events).filter(event => event.kind === 'EVIDENCE_INVALIDATED');
      assert.equal(failure.length, episode, 'Repeated failed polls are one episode; the same error after recovery is a new episode.');
      if (episode === 2) await state.restart();
      await writeFile(dataset.revisionManifest, originalManifest);
      const recovered = await waitFor(async () => Object.values(pmRecord(pmState(await state.service.pm.snapshot()), id).events)
        .filter(event => event.kind === 'SOURCE_RECOVERED').length === episode);
      record = pmRecord(pmState(await state.service.pm.snapshot()), id);
      const event = Object.values(record.events).filter(event => event.kind === 'SOURCE_RECOVERED').at(-1)!;
      assert.equal(event.supersedesEventId, failure.at(-1)!.id);
      assert.ok(!recoveredIds.includes(event.id)); recoveredIds.push(event.id);
      const evidence = record.evidence[event.sourceRevisionRef]!; assert.ok(evidence);
      const artifact = record.artifacts[evidence.objectKey]!;
      assert.equal(artifact.outcome, 'NEEDS_EVIDENCE', 'Source recovery cannot make the original incomplete protocol report scientifically complete.');
      assert.equal(artifact.datasetRevisionId, originalDataset.id); assert.equal(artifact.contentRef, originalDataset.contentRef);
      assert.ok(BigInt(event.availableAtNs) >= BigInt(failure.at(-1)!.availableAtNs));
      assert.deepEqual(recovered.datasetRevisions, before.datasetRevisions);
      assert.deepEqual(recovered.researchTrials, originalTrials); assert.deepEqual(recovered.budgetMovements, originalBudget);
      assert.deepEqual((await state.service.research.harness.snapshot(Exposures, context))?.windows ?? [], exposures);
      assert.ok(recovered.gaps.filter(gap => gap.requiredCapability === 'source_revision').every(gap => gap.state === 'RESOLVED'));
      assert.ok(recovered.gaps.some(gap => gap.requiredCapability === 'source_recovery_evidence' && gap.state !== 'RESOLVED'));
      assert.equal(recovered.targetCommands.length, 0);
      await state.restart();
      const afterRestart = pmRecord(pmState(await state.service.pm.snapshot()), id);
      assert.deepEqual(Object.values(afterRestart.events).filter(event => event.kind === 'SOURCE_RECOVERED').map(event => event.id), recoveredIds);
    }
    await new Promise(resolve => setTimeout(resolve, 5500));
    assert.deepEqual(Object.values(pmRecord(pmState(await state.service.pm.snapshot()), id).events).filter(event => event.kind === 'SOURCE_RECOVERED').map(event => event.id), recoveredIds);
  } finally { await state.cleanup(); }
});
