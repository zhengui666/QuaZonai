import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { Study, Requests } from '../../research/src/documents.ts';
import { retainLegacyResearchAdmission } from '../../research/test/frozen-input.ts';
import { Application, json } from '../src/facts.ts';
import { configDefaults } from '../src/config.ts';
import { createService } from '../src/service.ts';
import type { Service } from '../src/service.ts';
import type { Extension, Storage, TaskId } from '@earendil-works/pi-durable';
import type { PortfolioView } from '../../../packages/contracts/src/index.ts';

export const context = BACKGROUND_CONTEXT;
export { fauxAssistantMessage, fauxToolCall };
export const wat = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))';
export async function setup(owner = true, staticDir?: string, storage?: Storage) {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-service-test-'));
  const catalog = join(directory, 'catalog'); await mkdir(catalog);
  const binary = join(directory, 'controlled-job');
  const fixture = new URL('./controlled-job.mjs', import.meta.url);
  await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${fixture.pathname}' "$@"\n`, { mode: 0o700 });
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const model = { provider: faux.provider.id, modelId: faux.getModel().id };
  const request = { protocol_version: 1, train_end_ns: '1', validation_end_ns: '2', forecast: { schema_version: 1, selection: {}, parameters: {} } };
  const config = configDefaults({ dataDir: directory, staticDir, research: { database: join(directory, 'original-research.sqlite') }, jobBinary: binary,
    datasets: [{ id: 'controlled', name: 'Protocol dataset', assetClass: 'TEST', currency: 'USD', source: 'test only', start: null, end: null, ready: true,
      data: { catalog, request }, strategy: { name: 'fixed-before-trials', kind: 'factor-threshold', instrumentId: 'TEST.SIM', threshold: 0, investedWeight: '0.250000000000000001', targetTtlNs: '60000000000', simulationSettings: { base_currency: 'USD' } } }] });
  const options = { config, models, extensions: [] as Extension[], storage };
  let service = await createService(options);
  const response = owner ? await service.app.inject({ method: 'POST', url: '/api/v3/session/setup', payload: { password: 'local-owner-test' } }) : undefined;
  if (response) assert.equal(response.statusCode, 200, response.body);
  const headers = { cookie: response?.cookies.map((cookie) => `${cookie.name}=${cookie.value}`).join('; ') ?? '' };
  return { directory, options, model, faux, headers, get service() { return service; },
    async restart() { await service.close(); service = await createService(options); },
    async cleanup() { await service.close(); await rm(directory, { recursive: true, force: true }); } };
}
/** Reconstruct one explicitly historical, already-admitted request before its
 * normal HTTP/host replay. Does not invent results or change production admission.
 * Tests for genuinely new research must not call this helper. */
export function retainNextResearchAdmission(state: Awaited<ReturnType<typeof setup>>) {
  const host = state.service.research;
  const originalStart = host.start;
  let retained: Promise<void> | undefined;
  let retainedRequestId: string | undefined;
  host.start = async (...args: Parameters<typeof host.start>) => {
    const [requestId, input, , sourceEvidence, metadata] = args;
    if (retainedRequestId !== undefined && requestId !== retainedRequestId) return originalStart(...args);
    retainedRequestId = requestId;
    retained ??= retainLegacyResearchAdmission(host, requestId, input, sourceEvidence, metadata);
    try { await retained; } finally { host.start = originalStart; }
    return originalStart(...args);
  };
}
export async function studied(state: Awaited<ReturnType<typeof setup>>, requestId = 'idea') {
  state.faux.setResponses([fauxAssistantMessage('Deletion protocol setup only; no scientific execution claimed.')]);
  const input = { requestId, goal: 'Controlled model protocol', model: state.model, datasetId: 'controlled' };
  const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: input }); assert.equal(response.statusCode, 202, response.body);
  const receipt = response.json(); await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
  const diagnostics = await state.service.app.inject({ url: `/api/v3/research/${receipt.conversationId}/diagnostics`, headers: state.headers });
  const generationTaskId = diagnostics.json().generation.taskId;
  assert.equal(typeof generationTaskId, 'number');
  // Persist explicit test-only records through the official Doc API. These are
  // deletion/reference inputs, not a replacement scientific evaluator.
  await state.service.research.harness.commit(async tx => {
    const study = await tx.doc(Study, receipt.conversationId);
    const taskId = generationTaskId;
    const result = { request: study.input!.data.request!, partitions: [{ partition: 'HELDOUT', metric: { counts: { included: '2' }, pearson_ic: { value: null, status: 'CONSTANT_SCORE' } } }] };
    study.trials.push({ taskId, proposal: { hypothesis: 'TEST_ONLY deletion protocol', model: wat }, status: 'COMPUTED', result, error: null });
    study.final = { taskId, trialId: taskId, status: 'COMPUTED', result, reason: 'TEST_ONLY deletion protocol' };
  }, context);
  return { input, receipt };
}
export async function built(state: Awaited<ReturnType<typeof setup>>) {
  const { receipt } = await studied(state);
  const input = { requestId: 'portfolio', name: 'Controlled shared-account protocol', researchIds: [receipt.conversationId], datasetId: 'controlled', capital: '1000.000000000000000001' };
  const requests = await state.service.research.harness.snapshot(Requests, context);
  const taskId = requests!.requests['request:idea']!.taskId!;
  const study = (await state.service.research.state(receipt.conversationId)).research!;
  const view: PortfolioView = { id: 'test-saved-portfolio', name: input.name, researchIds: input.researchIds, datasetId: input.datasetId,
    status: 'COMPUTED', capital: input.capital, currency: 'USD', createdAt: new Date().toISOString(), taskId,
    metrics: [], curve: [], targets: [], error: null };
  await state.service.research.harness.commit(async tx => {
    (await tx.doc(Application)).portfolios[view.id] = json({ input, view, evidence: [{ researchId: receipt.conversationId, input: study.input, selected: study.trials[0] }],
      policy: {}, native: { protocolOnly: true }, target: { valid_until_ns: '9223372036854775807', targets: [] } });
  }, context);
  return { input, view };
}
export async function settledObservation(service: Service, id: string) {
  const fact = await service.work.getObservation(id); await service.research.harness.waitForTask(fact.taskId as TaskId, context);
  return (await service.work.getObservation(id)).view;
}
