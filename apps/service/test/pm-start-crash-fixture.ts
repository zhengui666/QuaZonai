import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import research from '../../research/plugins/research.ts';
import { Study, Requests, type ResearchInput } from '../../research/src/documents.ts';
import { registerHypothesis } from '../../research/src/pm-ledger.ts';
import { evaluateTrial } from '../../research/src/trials.ts';
import { configDefaults } from '../src/config.ts';
import { Application, json, object } from '../src/facts.ts';
import { PmDocument, pmState } from '../src/pm-facts.ts';
import { createService } from '../src/service.ts';

const context = BACKGROUND_CONTEXT;
const probe = defineExtension({ name: 'test.pm-start-crash', tools: [defineTool({
  name: 'pm_start_crash_probe', description: 'TEST_ONLY original trial receipt', parameters: Type.Object({}), replay: 'safe',
  async execute(_args, api, ctx) {
    const hypothesis = await registerHypothesis(api, ctx, { economicFamilyId: 'test:pm-start-crash', mechanism: 'Original admission survives process death.',
      prediction: 'Recovery reconnects the existing study.', falsification: 'A second submission or changed ledger appears.', requiredDataCapabilities: [], incrementalQuestion: 'Is the original receipt retained?' });
    await evaluateTrial(api, ctx, { hypothesisId: hypothesis.id, hypothesis: 'TEST_ONLY recovery protocol.' }, async () => ({ protocolOnly: true }));
    return { content: [{ type: 'text', text: 'Original protocol trial recorded.' }] };
  },
})] });

export async function crashFixtureService(directory: string) {
  const faux = fauxProvider({ provider: 'test-pm-start-crash', tokensPerSecond: 100_000 });
  const models = createModels(); models.setProvider(faux.provider);
  const model = { provider: faux.provider.id, modelId: faux.getModel().id };
  const config = configDefaults({ dataDir: directory, research: { database: join(directory, 'research.sqlite') }, jobBinary: '/must-not-run',
    datasets: [{ id: 'controlled', name: 'TEST_ONLY source', assetClass: 'TEST', currency: 'USD', source: 'TEST_ONLY protocol', start: null, end: null, ready: true,
      data: { catalog: '/TEST_ONLY_NOT_MARKET_DATA', request: { train_end_ns: '140', validation_end_ns: '180', forecast: {
        schema_version: 1, selection: { event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] }, parameters: {} } } } }] });
  return { service: await createService({ config, models, extensions: [research, probe] }), faux, model };
}

// This child exits after real workbench/host admission and before returning its
// receipt to the real monitor. No scheduler/route or successful science is mocked.
async function crash(directory: string, mode: string, changedGoal: boolean) {
  const { service, faux, model } = await crashFixtureService(directory);
  const start = service.work.startResearch.bind(service.work);
  service.work.startResearch = async (value, target) => {
    const body = json(value); const input = object(body.input) as ResearchInput;
    input.policy.max_trials = 2; input.campaign!.budget.maximumTrials = 2;
    await service.research.harness.commit(async tx => {
      pmState(await tx.doc(PmDocument)).mandates[input.campaign!.mandateRevisionId]!.budget.maximumTrials = 2;
    }, context);
    const receipt = mode === 'application' ? await start(body, target) : await service.research.start(String(body.requestId), input);
    await (await service.research.harness.submission(receipt.submissionId, context))!.wait(context);
    const study = await service.research.harness.snapshot(Study, receipt.conversationId, context);
    assert.equal(study!.trials.length, 1);
    const native = (await service.research.harness.snapshot(Requests, context))!.requests[`request:${body.requestId}`]!;
    const admission = (await service.work.snapshot())!.researchRequests[`request:${body.requestId}`];
    assert.equal(Boolean(admission), mode === 'application');
    if (changedGoal) await service.research.harness.commit(async tx => {
      pmState(await tx.doc(PmDocument)).mandates[input.campaign!.mandateRevisionId]!.input.goal = 'A different scientific objective';
    }, context);
    await writeFile(join(directory, 'crash.json'), JSON.stringify({ body, receipt, study, native, admission,
      mandateId: input.campaign!.mandateRevisionId, campaignId: input.campaign!.campaignId }));
    process.exit(73);
  };
  faux.setResponses([
    fauxAssistantMessage(fauxToolCall('strategy_intake', { name: 'Crash recovery protocol', archetype: 'PREDICTIVE_ALPHA',
      economicMechanism: 'Exercise original admission and task ownership.', falsificationConditions: ['Original receipts change.'],
      requiredCapabilities: [], intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['TEST_ONLY'], ambiguities: [], ownerBoundaries: [] }), { stopReason: 'toolUse' }),
    fauxAssistantMessage(fauxToolCall('pm_start_crash_probe', {}), { stopReason: 'toolUse' }),
    fauxAssistantMessage('', { stopReason: 'error', errorMessage: '400 controlled provider unavailable' }),
  ]);
  const owner = await service.app.inject({ method: 'POST', url: '/api/v3/session/setup', payload: { password: 'test-crash-owner' } });
  assert.equal(owner.statusCode, 200, owner.body);
  const headers = { cookie: owner.cookies.map(cookie => `${cookie.name}=${cookie.value}`).join('; ') };
  const response = await service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers,
    payload: { requestId: 'legacy-pm-start', goal: 'Original crash recovery objective', datasetId: 'controlled', model,
      funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } }, maxTrials: 2 } });
  assert.equal(response.statusCode, 202, response.body);
  await new Promise(() => { setTimeout(() => process.exit(99), 20_000); });
}

if (process.argv[2] === '--crash') await crash(process.argv[3]!, process.argv[4]!, process.argv[5] === 'changed');
