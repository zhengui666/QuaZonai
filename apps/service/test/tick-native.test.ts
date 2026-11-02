import assert from 'node:assert/strict';
import { test } from 'node:test';
import { mkdtemp, readFile, rm } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { configure } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject } from '@earendil-works/pi-durable';
import { researchInstructions } from '../../../packages/contracts/src/research.ts';
import type { ReviewReport } from '../../../packages/contracts/src/review.ts';
import { canonical, Requests, Study } from '../../research/src/documents.ts';
import { researchInput } from '../../research/src/input.ts';
import { ReviewSession } from '../../research/src/review-documents.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { configDefaults } from '../src/config.ts';
import { createService } from '../src/service.ts';
import { exportDataset } from '../src/dataset-export.ts';
import { createNautilusWorker } from '../../nautilus/src/server.ts';
import factorWasm from '../../research/plugins/factor-wasm.ts';
import heldoutWasm from '../../research/plugins/heldout-wasm.ts';

const fixture = process.env.QZ_TICK_NATIVE_FIXTURE_DIR;
const standard = process.env.QZ_JOB_BINARY; const high = process.env.QZ_TICK_JOB_BINARY;

const cases = [
  { representation: 'native', legacy: true }, { representation: 'cache', legacy: true }, { representation: 'flow', legacy: true },
  { representation: 'native', legacy: false },
] as const;
for (const { representation, legacy } of cases) test(legacy
  ? `HTTP and Durable evaluate actual high-precision ticks (${representation}) with separate BAR kernel and owner-only final evidence`
  : 'Fresh high-precision tick admission completes independent Review as INCONCLUSIVE without consuming held-out data',
{ skip: !fixture || !standard || !high, timeout: 120_000 }, async () => {
  const directory = await mkdtemp(join(tmpdir(), 'qz-tick-native-http-'));
  const worker = await createNautilusWorker({ binary: standard!, tickBinary: high!, dataDir: join(directory, 'worker'), catalogRoots: [fixture!, directory] });
  try {
    await new Promise<void>(resolve => worker.server.listen(0, '127.0.0.1', resolve));
    const address = worker.server.address(); assert.ok(address && typeof address !== 'string');
    const prefix = representation === 'native' ? '' : representation === 'flow' ? 'cohort-flow-' : 'cohort-';
    const dataset = await exportDataset({ prepared: join(fixture!, `${prefix}prepared/catalog-metadata.json`), request: join(fixture!, `${prefix}request.json`),
      output: join(directory, 'dataset.json'), id: 'native-ticks', name: 'Controlled high-precision ticks, not market evidence', ready: true });
    const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
    const config = configDefaults({ dataDir: join(directory, 'service'), research: { database: join(directory, 'research.sqlite') }, datasets: [dataset],
      nautilus: { backtest: { id: 'native-tick-test', url: `http://127.0.0.1:${address.port}` } } });
    let service = await createService({ config, models, extensions: [factorWasm, heldoutWasm] });
    try {
      const session = await service.app.inject({ method: 'POST', url: '/api/v3/session/setup', payload: { password: 'controlled-tick-native-only' } });
      assert.equal(session.statusCode, 200, session.body);
      const headers = { cookie: session.cookies.map(cookie => `${cookie.name}=${cookie.value}`).join('; ') };
      const model = await readFile(join(fixture!, representation === 'flow' ? 'cohort-flow-model.wat' : 'model.wat'), 'utf8');
      faux.setResponses([
        fauxAssistantMessage(fauxToolCall('factor_evaluate', { hypothesis: 'Controlled cohort-price factor; no profitability or PIT claim', model }), { stopReason: 'toolUse' }),
        async () => {
          const studies = await service.research.list(); const id = studies.items[0]!.id;
          const trial = (await service.research.view(id)).research!.trials[0]!;
          assert.equal(trial.status, 'COMPUTED', JSON.stringify(trial));
          return fauxAssistantMessage(fauxToolCall('factor_finalize', { trialId: Number(trial.taskId), reason: 'Fixed controlled selection before held-out calculation' }), { stopReason: 'toolUse' });
        },
        ...(!legacy ? [
          ...['context', 'provenance', 'factor', 'selection', 'stress', 'portfolio'].map(kind => fauxAssistantMessage(fauxToolCall(`review_${kind}`, {}), { stopReason: 'toolUse' })),
          async (transcript: { messages: readonly unknown[] }) => {
            const read = transcript.messages.find(message => {
              const row = message as { role?: string; toolName?: string };
              return row.role === 'toolResult' && row.toolName === 'review_context';
            }) as { content: { type: string; text?: string }[] } | undefined;
            assert.ok(read, 'The reviewer must cite evidence returned by its real context tool.');
            const returned = JSON.parse(read.content.filter(item => item.type === 'text').map(item => item.text).join(''));
            const ref = returned.evidenceRefs[0]; assert.equal(typeof ref, 'string');
            return fauxAssistantMessage(fauxToolCall('review_finish', {
              narrative: 'Unverified event-time proxy tick coverage supplies no historical source or BAR-first statistical qualification.', findings: [],
              mechanism: ['mechanism', 'availability', 'direction', 'implementation', 'complexity'].map(criterion => ({ criterion, supported: false, evidenceRefs: [ref] })),
            }), { stopReason: 'toolUse' });
          },
        ] : []),
      ]);
      const original = { requestId: 'actual-tick-native-http', goal: 'Controlled high-precision tick pipeline', model: { provider: faux.provider.id, modelId: faux.getModel().id }, datasetId: dataset.id, maxTrials: 2 };
      // Legacy cases reconstruct a pre-review native protocol fixture. This source has
      // EVENT_TIME_PROXY_UNVERIFIED coverage, not modern Review qualification.
      // Prebuild the original admission gap without starting a model or trial;
      // the unchanged HTTP request below captures its body and submits it.
      const input = researchInput({ goal: original.goal, model: original.model, data: config.datasets[0]!.data,
        policy: { ...config.datasets[0]!.policy, max_trials: original.maxTrials } });
      assert.equal(input.policy.reviewPolicy, undefined); assert.equal(input.policy.reviewPolicies, undefined);
      const frozenConversationId = legacy ? await service.research.harness.commit(async tx => {
        const requests = await tx.doc(Requests); const key = `request:${original.requestId}`;
        assert.equal(requests.requests[key], undefined);
        const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
        await configure(tx, conversation.id, { model: input.model, thinkingLevel: 'medium', instructions: researchInstructions(input) });
        const study = await tx.doc(Study, conversation.id);
        assert.equal(study.trials.length, 0); assert.equal(study.final, null);
        study.input = structuredClone(input);
        requests.requests[key] = { input, conversationId: conversation.id };
        return conversation.id;
      }, BACKGROUND_CONTEXT) : null;
      const start = await service.app.inject({ method: 'POST', url: '/api/v3/research', headers, payload: original });
      assert.equal(start.statusCode, 202, start.body); const receipt = start.json();
      if (legacy) assert.equal(receipt.conversationId, frozenConversationId);
      await (await service.research.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
      assert.equal(canonical((await service.research.harness.snapshot(Requests, BACKGROUND_CONTEXT))!.requests[`request:${original.requestId}`]!.input), canonical(input));
      if (legacy) assert.equal(canonical((await service.research.harness.snapshot(Study, frozenConversationId!, BACKGROUND_CONTEXT))!.input!), canonical(input));
      const detail = await service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers });
      assert.equal(detail.statusCode, 200, detail.body); const result = detail.json();
      assert.equal(result.trialRecords.length, 1); assert.equal(result.trialRecords[0].status, 'COMPUTED', detail.body);
      assert.equal(result.trialRecords[0].metrics.filter((metric: { key: string }) => metric.key.startsWith('HELDOUT.pearson_ic'))[0].value, null);
      if (legacy) {
        assert.equal(result.final.status, 'COMPUTED', detail.body);
        assert.equal(result.final.inputCoverage.modality, 'TRADE_TICK'); assert.equal(result.final.inputCoverage.availability, 'EVENT_TIME_PROXY_UNVERIFIED');
        assert.equal(result.final.inputCoverage.trade_count, '32');
        assert.equal(result.final.inputCoverage.representation, representation === 'flow' ? 'EVENT_TIME_COHORT_FLOW_CACHE_V1' : representation === 'cache' ? 'EVENT_TIME_COHORT_CACHE_V1' : undefined);
        assert.ok(result.final.metrics.some((metric: { key: string }) => metric.key === 'HELDOUT.pearson_ic'));
      } else {
        // The new-admission path uses the production policies and actual review
        // tools. Missing tick/PIT evidence must remain missing after the report.
        assert.ok(result.input.policy.reviewPolicies);
        assert.equal(result.final.status, 'STOPPED'); assert.equal(result.final.reason, 'independent_review_not_passed');
        assert.deepEqual(result.final.metrics, []);
        const stopped = (await service.research.harness.snapshot(Study, receipt.conversationId, BACKGROUND_CONTEXT))!.final!;
        assert.equal(stopped.result, null);
        assert.equal(result.trialRecords[0].inputCoverage.availability, 'EVENT_TIME_PROXY_UNVERIFIED');
        const reports = result.reviews as ReviewReport[];
        assert.equal(reports.length, 1); const report = reports[0]!;
        assert.equal(stopped.reviewReportId, report.id);
        assert.equal(report.decision, 'INCONCLUSIVE'); assert.equal(report.qualityCoefficient, 0);
        assert.equal(report.checks.find(check => check.id === 'provenance')!.status, 'UNKNOWN');
        assert.ok(report.missingEvidence.length > 0);
        assert.equal(report.dimensions.find(dimension => dimension.id === 'STATISTICS')!.status, 'INCOMPLETE');
        assert.equal((await service.research.harness.snapshot(Exposures, BACKGROUND_CONTEXT))!.windows.filter(window => window.kind === 'FINAL').length, 0);
        const nativeTasks = await service.research.harness.commit(tx => tx.scanTasks({ kind: 'qz.factor.wasm' }, 20), BACKGROUND_CONTEXT);
        assert.equal(nativeTasks.items.length, 1, 'Review refusal cannot schedule a second native factor task for held-out data.');
        const reviewTasks = await service.research.harness.commit(tx => tx.scanTasks({ kind: 'qz.review.independent' }, 20), BACKGROUND_CONTEXT);
        assert.equal(reviewTasks.items.length, 1); const review = reviewTasks.items[0]!;
        if (review.state.status !== 'terminal' || review.state.outcome.status !== 'completed') assert.fail('The original isolated review must complete.');
        const reviewResult = review.state.outcome.result as JsonObject;
        const session = await service.research.harness.snapshot(ReviewSession, reviewResult.reviewerConversationId as ConversationId, BACKGROUND_CONTEXT);
        assert.equal(session!.finished, true); assert.equal(session!.calls, 7);
        assert.deepEqual(session!.reads, ['context', 'provenance', 'factor', 'selection', 'stress', 'portfolio']);
        const reviewContext = session!.evidence!.context as JsonObject;
        assert.equal('sealed' in (reviewContext.data as JsonObject), false);
        const feedback = (reviewContext.candidate as JsonObject).feedback as JsonObject;
        assert.deepEqual((feedback.partitions as JsonObject[]).map(partition => partition.partition), ['VALIDATION']);
        assert.equal(faux.state.callCount, 9);
      }
      const factors = await service.app.inject({ url: '/api/v3/factors', headers });
      assert.equal(factors.json()[0].scientificQualification, 'UNQUALIFIED_EXPLORATORY'); assert.equal(factors.json()[0].combinable, false);
      const replay = await service.app.inject({ method: 'POST', url: '/api/v3/research', headers, payload: original });
      assert.equal(replay.json().conversationId, receipt.conversationId);
      const calls = faux.state.callCount;
      await service.close(); service = await createService({ config, models, extensions: [factorWasm, heldoutWasm] });
      const restored = await service.app.inject({ method: 'POST', url: '/api/v3/research', headers, payload: original });
      assert.equal(restored.statusCode, 202, restored.body); assert.deepEqual(restored.json(), receipt);
      const restoredDetail = await service.app.inject({ url: `/api/v3/research/${receipt.conversationId}`, headers });
      assert.equal(restoredDetail.statusCode, 200, restoredDetail.body);
      assert.deepEqual(restoredDetail.json().final, result.final);
      assert.deepEqual(restoredDetail.json().trialRecords, result.trialRecords);
      assert.deepEqual(restoredDetail.json().reviews, result.reviews);
      assert.equal(faux.state.callCount, calls, 'Restart must reuse the original admission and evidence without another model turn.');
    } finally { await service.close(); }
  } finally { await worker.close(); await rm(directory, { recursive: true, force: true }); }
});
