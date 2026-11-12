import { readSourceRevision } from '../src/pm-source.ts';
import type { Task } from '@earendil-works/pi-durable';
import { createPmPlatform } from '../src/pm-platform.ts';
import { captureDataset } from '../src/pm-datasets.ts';
import type { PmRecord } from '../src/pm-facts.ts';
import assert from 'node:assert/strict';
import { test } from 'node:test';
import { readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import type { TaskId } from '@earendil-works/pi-durable';
import type * as Pm from '../../../packages/contracts/src/pm.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { publishDatasetRevision } from '../src/dataset-revision-publish.ts';
import { fact, PmDocument, pmRecord, pmState } from '../src/pm-facts.ts';
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
    const id = 'same-source-recovery';
    const originalSource = await readSourceRevision(dataset);
    await state.service.research.harness.commit(async tx => {
      const stored = pmState(await tx.doc(PmDocument));
      const registered = captureDataset(dataset, stored, String(BigInt(Date.now()) * 1_000_000n), originalSource);
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const record = {
        input: { datasetId: dataset.id }, conversationId: conversation.id,
        mandate: { ...fact(id), capitalSource: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } } },
        policy: { id: 'existing-policy', validDomain: {} }, datasets: [registered.revision.id],
        campaigns: { original: { id: 'original', state: 'CONCLUDED', datasetRevisionIds: [registered.revision.id], runtime: { conversationId: conversation.id, taskId: null } } },
        strategies: {}, assessments: {}, events: {}, cycles: {}, decisions: {}, comparisons: {}, capacities: {}, ensembles: {}, evidence: {}, artifacts: {},
        outcomes: {}, attributions: {}, retirements: {}, commands: {}, gaps: {}, derivedTrials: {}, budgetMovements: {}, searchFamily: {},
        latestStrategyId: 'existing-strategy', currentDecisionId: null, latestCycleId: null, basisRevision: 'existing-basis', monitorTaskId: null,
      } as unknown as PmRecord;
      stored.mandates[id] = record;
      stored.mandates[id]!.monitorTaskId = await tx.createTask(createPmPlatform(state.options.config).extension.tasks!.find(task => task.definition.name === 'qz.pm.mandate-monitor')! as Task<{ mandateId: string }, { phase: 'start' }, null, object>, { mandateId: id },
        { conversationId: conversation.id, ownership: { kind: 'conversation' }, background: true });
    }, context);
    const read = async () => {
      const response = await state.service.app.inject({ url: `/api/v3/mandates/${encodeURIComponent(id)}`, headers: state.headers });
      assert.equal(response.statusCode, 200, response.body); return response.json<Pm.PmMandateView>();
    };
    const before = await read(); assert.equal(before.currentDecision, null, 'Source recovery does not invent a research decision.');
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
