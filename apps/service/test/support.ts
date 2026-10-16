import assert from 'node:assert/strict';
import { mkdtemp, mkdir, rm, writeFile } from 'node:fs/promises';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { Study } from '../../research/src/documents.ts';
import { evaluateTrial } from '../../research/src/trials.ts';
import { configDefaults } from '../src/config.ts';
import { createService } from '../src/service.ts';
import type { Service } from '../src/service.ts';
import type { TaskId } from '@earendil-works/pi-durable';
import type { JsonObject } from '../../../packages/contracts/src/index.ts';

export const context = BACKGROUND_CONTEXT;
export const wat = '(module (func (export "predict") (param f64 f64 f64 f64 f64 f64 f64 f64) (result f64) f64.const 1))';
// Only protocol facts, NOT native evidence; production modules never install this extension.
export const controlledResearch = defineExtension({ name: 'test.protocol-research', tools: [defineTool({
  name: 'controlled_trial', description: 'Test-only model/science protocol', parameters: Type.Object({}), replay: 'safe',
  async execute(_args, api, ctx) {
    const study = (await api.snapshot(Study, api.conversationId, ctx))!;
    const result: JsonObject = { request: study.input!.data.request!, partitions: [{ partition: 'HELDOUT', metric: { counts: { included: '2' }, pearson_ic: { value: null, status: 'CONSTANT_SCORE' } } }] };
    await evaluateTrial(api, ctx, { hypothesis: 'Controlled protocol', model: wat }, async () => result);
    await api.commit(async (tx) => { (await tx.doc(Study, api.conversationId)).final = { taskId: api.taskId, trialId: api.taskId, status: 'COMPUTED', result, reason: 'Protocol only' }; }, ctx);
    return { content: [{ type: 'text', text: 'Protocol test ended, not scientific evidence.' }], control: { terminate: true } };
  },
})] });

export async function setup() {
  const directory = await mkdtemp(join(process.env.PI_SCRATCH_DIR ?? tmpdir(), 'qz-service-test-'));
  const catalog = join(directory, 'catalog'); await mkdir(catalog);
  const binary = join(directory, 'controlled-job');
  const fixture = new URL('./controlled-job.mjs', import.meta.url);
  await writeFile(binary, `#!/bin/sh\nexec '${process.execPath}' '${fixture.pathname}' "$@"\n`, { mode: 0o700 });
  const faux = fauxProvider({ tokensPerSecond: 100_000 }); const models = createModels(); models.setProvider(faux.provider);
  const model = { provider: faux.provider.id, modelId: faux.getModel().id };
  const request = { schema_version: 1, train_end_ns: '1', validation_end_ns: '2', forecast: { schema_version: 1, selection: {}, parameters: {} } };
  const config = configDefaults({ dataDir: directory, research: { database: join(directory, 'original-research.sqlite') }, jobBinary: binary,
    datasets: [{ id: 'controlled', name: 'Protocol dataset', assetClass: 'TEST', currency: 'USD', source: 'test only', start: null, end: null, ready: true,
      data: { catalog, request }, strategy: { name: 'fixed-before-trials', kind: 'factor-threshold', instrumentId: 'TEST.SIM', threshold: 0, investedWeight: '0.250000000000000001', targetTtlNs: '60000000000', simulationSettings: { base_currency: 'USD' } } }] });
  const options = { config, models, extensions: [controlledResearch] };
  let service = await createService(options);
  const response = await service.app.inject({ method: 'POST', url: '/api/v3/session/setup', payload: { password: 'local-owner-test' } }); assert.equal(response.statusCode, 200, response.body);
  const headers = { cookie: response.cookies.map((cookie) => `${cookie.name}=${cookie.value}`).join('; ') };
  return { directory, options, model, faux, headers, get service() { return service; },
    async restart() { await service.close(); service = await createService(options); },
    async cleanup() { await service.close(); await rm(directory, { recursive: true, force: true }); } };
}
export async function studied(state: Awaited<ReturnType<typeof setup>>, requestId = 'idea') {
  state.faux.setResponses([fauxAssistantMessage(fauxToolCall('controlled_trial', {}), { stopReason: 'toolUse' })]);
  const input = { requestId, goal: 'Controlled model protocol', model: state.model, datasetId: 'controlled', maxTrials: 1 };
  const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/research', headers: state.headers, payload: input }); assert.equal(response.statusCode, 202, response.body);
  const receipt = response.json(); await (await state.service.research.harness.submission(receipt.submissionId, context))!.wait(context);
  return { input, receipt };
}
export async function built(state: Awaited<ReturnType<typeof setup>>) {
  const { receipt } = await studied(state);
  const input = { requestId: 'portfolio', name: 'Controlled shared-account protocol', researchIds: [receipt.conversationId], datasetId: 'controlled', capital: '1000.000000000000000001' };
  const response = await state.service.app.inject({ method: 'POST', url: '/api/v3/portfolios', headers: state.headers, payload: input }); assert.equal(response.statusCode, 202, response.body);
  const view = response.json(); await state.service.research.harness.waitForTask(view.taskId as TaskId, context);
  return { input, view: (await state.service.work.getPortfolio(view.id)).view };
}
export async function settledObservation(service: Service, id: string) {
  const fact = await service.work.getObservation(id); await service.research.harness.waitForTask(fact.taskId as TaskId, context);
  return (await service.work.getObservation(id)).view;
}
