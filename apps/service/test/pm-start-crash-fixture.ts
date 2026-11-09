import { readResearchAdmissions } from '../src/admissions.ts';
import { obj } from '@earendil-works/pi-schema';
import assert from 'node:assert/strict';
import { writeFile } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import research from '../../research/plugins/research.ts';
import { ResearchIntakeTask } from '../../research/src/intake.ts';
import { Study, Requests, type ResearchInput } from '../../research/src/documents.ts';
import { registerHypothesis } from '../../research/src/pm-ledger.ts';
import { evaluateTrial } from '../../research/src/trials.ts';
import { configDefaults } from '../src/config.ts';
import { Application, json, object } from '../src/facts.ts';
import { PmDocument, pmState } from '../src/pm-facts.ts';
import { createService } from '../src/service.ts';

const context = BACKGROUND_CONTEXT;
const probe = defineExtension({ name: 'test.pm-start-crash', tools: [defineTool({
  name: 'pm_start_crash_probe', description: 'TEST_ONLY original trial receipt', parameters: obj({}), replay: 'safe',
  async execute(_args, api, ctx) {
    const hypothesis = await registerHypothesis(api, ctx, { economicFamilyId: 'test:pm-start-crash', mechanism: 'Original admission survives process death.',
      prediction: 'Recovery reconnects the existing study.', falsification: 'A second submission or changed ledger appears.', requiredDataCapabilities: [], incrementalQuestion: 'Is the original receipt retained?' });
    await evaluateTrial(api, ctx, { hypothesisId: hypothesis.id, hypothesis: 'TEST_ONLY recovery protocol.' }, async () => ({ protocolOnly: true }));
    return { output: [{ type: 'text', text: 'Original protocol trial recorded.' }] };
  },
})] });

export async function crashFixtureService(directory: string, resumeOriginal = false) {
  const faux = fauxProvider({ provider: 'test-pm-start-crash', tokensPerSecond: 100_000 });
  const models = createModels(); models.setProvider(faux.provider);
  const model = { provider: faux.provider.id, modelId: faux.getModel().id };
  if (resumeOriginal) faux.setResponses(crashResponses());
  const config = configDefaults({ dataDir: directory, research: { database: join(directory, 'research.sqlite') }, jobBinary: '/must-not-run',
    datasets: [{ id: 'controlled', name: 'TEST_ONLY source', assetClass: 'TEST', currency: 'USD', source: 'TEST_ONLY protocol', start: null, end: null, ready: true,
      data: { catalog: '/TEST_ONLY_NOT_MARKET_DATA', request: { train_end_ns: '140', validation_end_ns: '180', forecast: {
        schema_version: 1, selection: { event_start_ns: '100', event_end_ns: '200', decision_cutoff_ns: '200', bar_types: ['TEST.SIM-1-MINUTE-LAST-EXTERNAL'] }, parameters: {} } } } }] });
  return { service: await createService({ config, models, extensions: [research, probe] }), faux, model };
}

export function crashResponses() {
  return [
    fauxAssistantMessage(fauxToolCall('strategy_intake', { name: 'Crash recovery protocol', archetype: 'PREDICTIVE_ALPHA',
      economicMechanism: 'Exercise original admission and task ownership.', falsificationConditions: ['Original receipts change.'],
      requiredCapabilities: [], intendedHoldingHorizonRef: 'REGISTERED_NATIVE_LABEL_HORIZON', knownFailureModes: ['TEST_ONLY'], ambiguities: [], ownerBoundaries: [] }), { stopReason: 'toolUse' }),
    fauxAssistantMessage(fauxToolCall('pm_start_crash_probe', {}), { stopReason: 'toolUse' }),
    fauxAssistantMessage('', { stopReason: 'error', errorMessage: '400 controlled provider unavailable' }),
  ];
}

// This child exits after real workbench/host admission and before returning its
// receipt to the real monitor. No scheduler/route or successful science is mocked.
async function crash(directory: string, mode: string, changedGoal: boolean) {
  const { service, faux, model } = await crashFixtureService(directory);
  const start = service.work.startResearch.bind(service.work);
  service.work.startResearch = async (value, target) => {
    const body = json(value); const input = object(body.input) as ResearchInput;
    async function capture(receipt: { conversationId: number; submissionId: number | null }) {
      const study = await service.research.harness.snapshot(Study, receipt.conversationId as never, context);
      const native = (await service.research.harness.snapshot(Requests, context))!.requests[`request:${body.requestId}`]!;
      const admission = (await readResearchAdmissions(service.research))[`request:${body.requestId}`];
      if (changedGoal) await service.research.harness.commit(async tx => {
        pmState(await tx.doc(PmDocument)).mandates[input.campaign!.mandateRevisionId]!.input.goal = 'A different scientific objective';
      }, context);
      const budget = pmState(await service.research.harness.snapshot(PmDocument, context)).mandates[input.campaign!.mandateRevisionId]!.budget;
      await writeFile(join(directory, 'crash.json'), JSON.stringify({ body, receipt, study, native, admission, budget,
        mandateId: input.campaign!.mandateRevisionId, campaignId: input.campaign!.campaignId }));
      process.exit(73);
    }
    if (mode === 'create') {
      const original = ResearchIntakeTask.definition.phases.create;
      Object.defineProperty(ResearchIntakeTask.definition.phases, 'create', { value: async (...args: Parameters<typeof original>) => {
        await original(...args);
        if (args[0].input.requestId === body.requestId) {
          const binding = (await service.research.harness.snapshot(Requests, context))!.requests[`request:${body.requestId}`]!;
          await capture({ conversationId: binding.conversationId!, submissionId: null });
        }
      } });
    }
    const receipt = await start(body, target);
    await (await service.research.harness.submission(receipt.submissionId as never, context))!.wait(context);
    assert.equal((await service.research.harness.snapshot(Study, receipt.conversationId as never, context))!.trials.length, 1);
    await capture(receipt);
    return receipt;
  };
  faux.setResponses(crashResponses());
  const owner = await service.app.inject({ method: 'POST', url: '/api/v3/session/setup', payload: { password: 'test-crash-owner' } });
  assert.equal(owner.statusCode, 200, owner.body);
  const headers = { cookie: owner.cookies.map(cookie => `${cookie.name}=${cookie.value}`).join('; ') };
  const response = await service.app.inject({ method: 'POST', url: '/api/v3/mandates', headers,
    payload: { requestId: 'durable-pm-start', goal: 'Original crash recovery objective', datasetId: 'controlled', model,
      funding: { kind: 'RESEARCH_SCENARIO', capital: { amount: '3000', currency: 'USD' } } } });
  assert.equal(response.statusCode, 202, response.body);
  await new Promise(() => { setTimeout(() => process.exit(99), 20_000); });
}

if (process.argv[2] === '--crash') await crash(process.argv[3]!, process.argv[4]!, process.argv[5] === 'changed');
