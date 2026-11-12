import { obj } from '@earendil-works/pi-schema';
// A real process crash probe using official model/storage/tool execution.
import { appendFile } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT, awaitWithContext } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall } from '@earendil-works/pi-ai';
import { configure, defineExtension, defineTool } from '@earendil-works/pi-durable';
import { openRetainedResearch as openResearch } from './frozen-input.ts';
import { ResearchIntakeTask } from '../src/intake.ts';
import { Requests, Study } from '../src/documents.ts';

const directory = process.argv[2]!;
const failed = process.argv[3] === 'failed';
const replay = process.argv[3] === 'unsafe' ? 'unsafe' : 'safe';
const faux = fauxProvider({ models: [{ id: 'crash-probe' }], tokensPerSecond: 100_000 });
const models = createModels(); models.setProvider(faux.provider);
faux.setResponses([fauxAssistantMessage(fauxToolCall('crash_probe', {}, { id: 'original-call' }), { stopReason: 'toolUse' })]);
const plugin = defineExtension({ name: 'qz.research', tools: [defineTool({
  name: 'crash_probe', description: 'Protocol test only', parameters: obj({}), replay,
  async execute(_args, api, context) {
    await appendFile(join(directory, 'calls'), `${api.taskId}\n`);
    process.stdout.write(`${JSON.stringify({ entered: api.taskId })}\n`);
    await awaitWithContext(new Promise<void>(() => {}), context);
    return {};
  },
})] });
const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [plugin] });
if (process.argv[3] === 'admission') {
  const input = { goal: 'Recover the original admitted objective', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: { } };
  const create = ResearchIntakeTask.definition.phases.create;
  Object.defineProperty(ResearchIntakeTask.definition.phases, 'create', { value: async (...args: Parameters<typeof create>) => {
    await create(...args);
    const request = (await host.harness.snapshot(Requests, BACKGROUND_CONTEXT))!.requests['request:admission-gap']!;
    process.stdout.write(`${JSON.stringify({ entered: true, ids: [request.conversationId], input })}\n`);
    await awaitWithContext(new Promise<void>(() => {}), args[2]);
  } });
  void host.start('admission-gap', input);
  await new Promise<void>(() => {});
}
const receipt = await host.start('original-request', { goal: 'Crash recovery protocol test', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
process.stdout.write(`${JSON.stringify(receipt)}\n`);
await (await host.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
await host.close();
