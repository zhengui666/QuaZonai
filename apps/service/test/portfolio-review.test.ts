import assert from 'node:assert/strict';
import test from 'node:test';
import { createHash } from 'node:crypto';
import { mkdir, readFile, writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { setTimeout } from 'node:timers/promises';
import type { JsonObject, TaskId } from '@earendil-works/pi-durable';
import { setup, context, fauxAssistantMessage, fauxToolCall, wat } from './support.ts';
import { canonical, Study } from '../../research/src/documents.ts';
import { iterationRequest } from '../../research/plugins/native-wasm.ts';
import { Exposures } from '../../research/plugins/native-exposure.ts';
import { ReviewSourceEvidence, ReviewSourceValidationTask } from '../../research/src/review-source.ts';
import { createReviewPolicy } from '../../../packages/contracts/src/review-scoring.ts';
import { prepareReviewSourceEvidence } from '../src/review-source.ts';
import { Application, json, portfolio } from '../src/facts.ts';
import type { DatasetConfig } from '../src/config.ts';
import type { PortfolioTaskInput } from '../src/native.ts';
import { BuildPortfolio } from '../src/science.ts';
import { PortfolioReviewScience } from '../src/portfolio-review.ts';

const DAY = 86_400_000_000_000n;
const hash = (value: string) => createHash('sha256').update(value).digest('hex');
async function sourceFixture(root: string, id: string, start: bigint, train: bigint, validation: bigint, end: bigint, heldout = false): Promise<DatasetConfig> {
  await mkdir(join(root, 'catalog'), { recursive: true });
  const selection = { schema_version: 1, bar_types: ['TEST.SIM-1-DAY-LAST-EXTERNAL'], event_start_ns: String(start*DAY), event_end_ns: String(end*DAY), decision_cutoff_ns: String(end*DAY), maximum_rows: 1000 };
  const request = { schema_version: 1, train_end_ns: String(train*DAY), validation_end_ns: String(validation*DAY), forecast: { schema_version: 1, selection, parameters: { schema_version: 1, fast_period: 2, slow_period: 3, label_horizon_observations: 1, ...(heldout ? {} : { total_fuel: '1' }) } } };
  const metadata = { registered_ref: id, native_snapshot_ref: `TEST_ONLY:${id}`, partition: heldout ? 'SEALED' : 'VALIDATION', origin: 'REAL', pit_status: 'VERIFIED', revision_policy: 'AS_KNOWN_THEN', provenance_reference: 'TEST_ONLY_SOURCE_OWNER', availability_provenance: 'TEST_ONLY_HISTORICAL_CLOCK_CLAIM', quality: { datasets: [{ dataset_revision_id: `TEST_ONLY_NATIVE:${id}`, instrument_ids: ['TEST.SIM'], selection: { ...selection, event_start_ns: String((heldout ? validation : train)*DAY), event_end_ns: String((heldout ? end : validation)*DAY) } }] } };
  const license = { schemaVersion: 1, datasetId: id, sourceId: `owner:${id}`, sourceRevision: `TEST_ONLY:${id}`, datasetRevisionId: `TEST_ONLY_NATIVE:${id}`, grantId: `TEST_ONLY_GRANT:${id}`, issuer: 'TEST_ONLY_OWNER', reference: 'TEST_ONLY_PROTOCOL', status: 'ACTIVE', purposes: ['RESEARCH'], validFromNs: '0', validUntilNs: '9223372036854775807' };
  const files: JsonObject[] = [];
  for (const [path, body] of Object.entries({ 'catalog/bars.parquet': 'TEST_ONLY_NOT_MARKET_DATA', 'metadata.json': JSON.stringify(metadata), 'license.json': JSON.stringify(license) })) {
    await writeFile(join(root, path), body); files.push({ path, sha256: hash(body), bytes: String(Buffer.byteLength(body)) });
  }
  const manifest = { schemaVersion: 1, datasetId: id, sourceRevision: `TEST_ONLY:${id}`, availableAtNs: '0', revisionKind: 'APPEND', data: { catalog: 'catalog', request }, files };
  await writeFile(join(root, 'revision.json'), JSON.stringify({ ...manifest, contentDigest: hash(canonical(json(manifest))) }));
  return { id, name: id, source: `owner:${id}`, assetClass: 'TEST', currency: 'USD', start: null, end: null, ready: true, data: { catalog: join(root, 'catalog'), request }, revisionManifest: join(root, 'revision.json'), reviewSourceEvidence: { metadata: { path: 'metadata.json', sha256: hash(JSON.stringify(metadata)) }, heldoutMetadata: { path: 'metadata.json', sha256: hash(JSON.stringify(metadata)) }, license: { path: 'license.json', sha256: hash(JSON.stringify(license)) } } };
}

// Real official Durable/Faux/child-process data flow. Controlled source-owner and
// native receipts do not establish Rust estimator accuracy, market PIT or profitability.
for (const mode of ['positive', 'negative', 'missing', 'pending'] as const) test(`portfolio independent outcome and durable receipts: ${mode}`, { timeout: 60_000 }, async () => {
  const state = await setup(); const log = join(state.directory, 'portfolio-wire.log');
  try {
    if (mode === 'negative') await writeFile(log + '.negative', 'negative independent account');
    if (mode === 'missing') await writeFile(log + '.insufficient', 'missing independent confidence evidence');
    const binary = join(state.directory, 'portfolio-job');
    await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${new URL('./portfolio-review-job.mjs', import.meta.url).pathname}' '${log}' "$@"\n`, { mode: 0o700 });
    const validation = await sourceFixture(join(state.directory, 'validation'), 'validation', 0n, 150n, 300n, 350n);
    const independent = await sourceFixture(join(state.directory, 'independent'), 'fresh', 350n, 375n, 400n, 600n, true);
    const captured = await prepareReviewSourceEvidence(validation); const independentSource = await prepareReviewSourceEvidence(independent, 'HELDOUT');
    assert.ok(captured); assert.ok(independentSource);
    const settings = { base_currency: 'USD', starting_capital: '1000', fill_model: { adapter_kind: 'NAUTILUS_DEFAULT_FILL', parameters: { prob_slippage: '0' } }, latency_model: { adapter_kind: 'NAUTILUS_STATIC_LATENCY', parameters: { base_latency_ns: '0' } } };
    const strategy = { name: 'TEST_ONLY', kind: 'factor-threshold', instrumentId: 'TEST.SIM', threshold: 0, investedWeight: '1', targetTtlNs: String(2n*DAY), simulationSettings: settings };
    const source = await state.service.research.harness.commit(async tx => {
      // Reconstruct an existing source study for this consumer test. Source
      // discovery/qualification and HTTP portfolio admission are outside this fixture.
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      const study = await tx.doc(Study, conversation.id);
      study.input = { goal: 'Controlled source factor protocol', model: state.model, data: validation.data, policy: { max_trials: 1, strategy } };
      const selected = { taskId: 1, status: 'COMPUTED', proposal: { model: wat, hypothesis: 'Controlled original source' }, result: { request: iterationRequest(validation.data), partitions: [{ partition: 'VALIDATION', metric: { counts: { included: '150' }, pearson_ic: { value: 1, status: 'AVAILABLE' } } }] } };
      study.trials = [selected];
      const sourceDoc = await tx.doc(ReviewSourceEvidence, conversation.id); sourceDoc.captured = json(captured);
      const taskId = await tx.createTask(ReviewSourceValidationTask, { metadata: captured.metadata, observedAt: captured.receivedAt, target: binary }, { conversationId: conversation.id, ownership: { kind: 'conversation' } });
      sourceDoc.taskId = taskId;
      return { conversationId: conversation.id, selected: json(selected), input: json(study.input), sourceTaskId: taskId };
    }, context);
    const validated = await state.service.research.harness.waitForTask(source.sourceTaskId, context);
    assert.equal(validated.state.outcome.status, 'completed');
    if (validated.state.outcome.status !== 'completed') return;
    const validationReceipt = validated.state.outcome.result; assert.ok(validationReceipt);
    await state.service.research.harness.commit(async tx => { (await tx.doc(ReviewSourceEvidence, source.conversationId)).native = validationReceipt; }, context);
    const policy = createReviewPolicy('PORTFOLIO', 'TEST_ONLY_POLICY', '100');
    policy.calibration = { rootCount: 30, endNs: '90', evidenceRefs: ['TEST_ONLY_PRIOR_COHORT'] };
    const taskInput: PortfolioTaskInput = { id: 'reviewed-portfolio', dataDir: state.directory, catalog: '/NEVER_READ_COMPONENT_SEALED', binary, capital: '1000', currency: 'USD', evidence: [{ researchId: source.conversationId, input: json(source.input), selected: source.selected, final: { result: { request: { FORBIDDEN_COMPONENT_HELDOUT: true } } } }], weights: ['1'], policy: strategy as PortfolioTaskInput['policy'], capacityPolicy: { participation: '0.1', maximumAgeSeconds: 3600, scenarios: ['1000'] }, review: { policy, model: state.model, createdAt: '2026-11-02T00:00:00Z', maximumScientificCalls: 16, stressMethod: 'QZ_REVIEW_ONE_BAR_NATIVE_SLIPPAGE_V1', independentData: independent.data, independentSource } };
    const call = (name: string, args: JsonObject = {}) => fauxAssistantMessage(fauxToolCall(name, args), { stopReason: 'toolUse' });
    const reference = `research:${source.conversationId}:trial:1:validation`;
    state.faux.setResponses([...['context', 'provenance', 'factor', 'selection', 'stress', 'portfolio'].map(kind => call(`review_${kind}`)), call('review_finish', { narrative: 'Complete controlled protocol evidence.', findings: [], mechanism: ['mechanism', 'availability', 'direction', 'implementation', 'complexity'].map(criterion => ({ criterion, supported: true, evidenceRefs: [reference] })) })]);
    if (mode === 'pending') await writeFile(log + '.pause', 'pause unresolved native replay before its result checkpoint');
    const taskId = await state.service.research.harness.commit(async tx => {
      const facts = await tx.doc(Application);
      const taskId = await tx.createTask(BuildPortfolio, taskInput, { conversationId: source.conversationId, ownership: { kind: 'conversation' } });
      facts.portfolios[taskInput.id] = json({ input: { requestId: taskInput.id, name: taskInput.id, datasetId: validation.id, capital: '1000', researchIds: [source.conversationId] }, evidence: taskInput.evidence, weights: ['1'], policy: strategy, native: null, target: null,
        view: { id: taskInput.id, name: taskInput.id, datasetId: validation.id, researchIds: [source.conversationId], status: 'QUEUED', capital: '1000', currency: 'USD', createdAt: taskInput.review!.createdAt, taskId, metrics: [], curve: [], targets: [], error: null } });
      return taskId;
    }, context);
    if (mode === 'pending') {
      let entered = false;
      for (let n = 0; n < 300; n++) { try { await readFile(log + '.entered'); entered = true; break; } catch { await setTimeout(10); } }
      assert.equal(entered, true, 'Validation shared account actually started');
      await state.restart();
    }
    const settled = await state.service.research.harness.waitForTask(taskId, context);
    assert.equal(settled.state.outcome.status, 'completed', JSON.stringify(settled.state));
    const facts = await state.service.research.harness.snapshot(Application, context); const result = portfolio(facts!.portfolios[taskInput.id]);
    if (mode !== 'positive') {
      assert.equal(result.view.review?.decision, mode === 'negative' ? 'REJECT' : 'INCONCLUSIVE', JSON.stringify(result.view.review));
      assert.equal(result.view.status, 'REVIEW_BLOCKED'); assert.equal(result.target, null); assert.deepEqual(result.view.targets, []);
      const recorded = await state.service.research.harness.snapshot(PortfolioReviewScience, String(taskId), context);
      if (mode === 'pending') {
        assert.equal(recorded!.reservedCalls, 4);
        const wire = (await readFile(log, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
        assert.equal(wire.filter(row => row.command === 'simulate').length, 1, 'unresolved invocation must never be posted again under the same budget');
        assert.ok(result.view.review!.reasonCodes.some(reason => reason.includes('missing')));
      } else {
        assert.ok(result.native?.independentQualification, 'original independent results and cash oracle remain owner-visible');
        const check = result.view.review!.checks.find(check => check.id === 'independent_outcome');
        assert.equal(check?.status, mode === 'negative' ? 'FAIL' : 'UNKNOWN');
        assert.equal(state.faux.state.callCount, 7, 'independent outcome never enters another Agent turn');
      }
      return;
    }
    assert.equal(result.view.review?.decision, 'PASS', JSON.stringify({ reasons: result.view.review?.reasonCodes, analyses: result.view.review?.analyses, stages: await state.service.research.harness.snapshot(PortfolioReviewScience, String(taskId), context) }));
    assert.equal(result.view.status, 'COMPUTED', result.view.error ?? '');
    const stages = await state.service.research.harness.snapshot(PortfolioReviewScience, String(taskId), context);
    assert.equal(stages!.reservedCalls, 16); assert.equal(Object.keys(stages!.reservations).length, 9);
    assert.ok(stages!.results.independent);
    const exposure = await state.service.research.harness.snapshot(Exposures, context);
    const final = exposure!.windows.filter(window => window.kind === 'FINAL' && window.taskId === taskId);
    assert.equal(final.length, 1); assert.equal(final[0]!.catalog, independent.data.catalog); assert.equal(final[0]!.start, String(400n*DAY));
    const args = (await readFile(log + '.args', 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    const forecasts = args.filter(row => row.command === 'forecast');
    assert.ok(forecasts.some(row => row.args.includes(validation.data.catalog))); assert.ok(forecasts.some(row => row.args.includes(independent.data.catalog)));
    assert.equal(args.some(row => row.args.includes('/NEVER_READ_COMPONENT_SEALED')), false);
    const calls = state.faux.state.callCount; const prior = json(stages);
    await state.restart(); await state.service.research.harness.waitForTask(taskId as TaskId, context);
    assert.deepEqual(json(await state.service.research.harness.snapshot(PortfolioReviewScience, String(taskId), context)), prior);
    assert.equal(state.faux.state.callCount, calls);
    // The first task is terminal and the service has restarted. Its Validation
    // result must remain available to the next actual configuration's cohort.
    const later = await sourceFixture(join(state.directory, 'later-independent'), 'fresh-later', 650n, 675n, 700n, 900n, true);
    const laterSource = await prepareReviewSourceEvidence(later, 'HELDOUT'); assert.ok(laterSource);
    const next: PortfolioTaskInput = { ...taskInput, id: 'reviewed-portfolio-two', review: { ...taskInput.review!, independentData: later.data, independentSource: laterSource } };
    state.faux.setResponses([...['context', 'provenance', 'factor', 'selection', 'stress', 'portfolio'].map(kind => call(`review_${kind}`)), call('review_finish', { narrative: 'Second complete controlled configuration.', findings: [], mechanism: ['mechanism', 'availability', 'direction', 'implementation', 'complexity'].map(criterion => ({ criterion, supported: true, evidenceRefs: [reference] })) })]);
    const secondId = await state.service.research.harness.commit(async tx => {
      const app = await tx.doc(Application);
      const secondId = await tx.createTask(BuildPortfolio, next, { conversationId: source.conversationId, ownership: { kind: 'conversation' } });
      app.portfolios[next.id] = json({ input: { requestId: next.id, name: next.id, datasetId: validation.id, capital: '1000', researchIds: [source.conversationId] }, evidence: next.evidence, weights: ['1'], policy: strategy, native: null, target: null,
        view: { id: next.id, name: next.id, datasetId: validation.id, researchIds: [source.conversationId], status: 'QUEUED', capital: '1000', currency: 'USD', createdAt: next.review!.createdAt, taskId: secondId, metrics: [], curve: [], targets: [], error: null } });
      return secondId;
    }, context);
    const second = await state.service.research.harness.waitForTask(secondId, context);
    assert.equal(second.state.outcome.status, 'completed', JSON.stringify(second.state));
    const updated = await state.service.research.harness.snapshot(Application, context); const secondResult = portfolio(updated!.portfolios[next.id]);
    assert.equal(secondResult.view.review?.decision, 'PASS', JSON.stringify(secondResult.view.review));
    assert.equal(secondResult.view.status, 'COMPUTED', secondResult.view.error ?? '');
    const commands = (await readFile(log, 'utf8')).trim().split('\n').map(line => JSON.parse(line));
    const selection = commands.findLast(row => row.command === 'review-science' && row.input.analysis.kind === 'SELECTION');
    assert.equal(selection.input.analysis.total_trials, 2); assert.equal(selection.input.analysis.ledger_complete, true);
    assert.deepEqual(selection.input.analysis.trials.map((row: { id: string }) => row.id).sort(), [taskInput.id, next.id].sort());
  } finally { await state.cleanup(); }
});
