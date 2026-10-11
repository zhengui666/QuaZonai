// A real process crash probe using official model/storage/tool execution.
import { appendFile } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT, awaitWithContext } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';

const directory = process.argv[2]!;
const replay = process.argv[3] === 'safe' ? 'safe' : 'unsafe';
const faux = fauxProvider({ models: [{ id: 'crash-probe' }], tokensPerSecond: 100_000 });
const models = createModels(); models.setProvider(faux.provider);
faux.setResponses([fauxAssistantMessage(fauxToolCall('crash_probe', {}, { id: 'original-call' }), { stopReason: 'toolUse' })]);
const plugin = defineExtension({ name: 'crash-probe', tools: [defineTool({
  name: 'crash_probe', description: 'Protocol test only', parameters: Type.Object({}), replay,
  async execute(_args, api, context) {
    await appendFile(join(directory, 'calls'), `${api.taskId}\n`);
    process.stdout.write(`${JSON.stringify({ entered: api.taskId })}\n`);
    await awaitWithContext(new Promise<void>(() => {}), context);
    return {};
  },
})] });
const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [plugin] });
const receipt = await host.start('original-request', { goal: 'Crash recovery protocol test', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
process.stdout.write(`${JSON.stringify(receipt)}\n`);
await (await host.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
await host.close();
