// A real process crash probe using official model/storage/tool execution.
import { appendFile } from 'node:fs/promises';
import { join } from 'node:path';
import { BACKGROUND_CONTEXT, awaitWithContext } from '@earendil-works/chord/context';
import { createModels, fauxProvider, fauxAssistantMessage, fauxToolCall, Type } from '@earendil-works/pi-ai';
import { configure, defineExtension, defineTool } from '@earendil-works/pi-durable';
import { openResearch } from '../src/host.ts';
import { evaluateTrial } from '../src/trials.ts';
import { Requests, Study } from '../src/documents.ts';

const directory = process.argv[2]!;
const failed = process.argv[3] === 'failed';
const replay = process.argv[3] === 'unsafe' ? 'unsafe' : 'safe';
const faux = fauxProvider({ models: [{ id: 'crash-probe' }], tokensPerSecond: 100_000 });
const models = createModels(); models.setProvider(faux.provider);
faux.setResponses([fauxAssistantMessage(fauxToolCall('crash_probe', {}, { id: 'original-call' }), { stopReason: 'toolUse' })]);
const plugin = defineExtension({ name: 'crash-probe', tools: [defineTool({
  name: 'crash_probe', description: 'Protocol test only', parameters: Type.Object({}), replay,
  async execute(_args, api, context) {
    if (failed) {
      try {
        await evaluateTrial(api, context, { hypothesis: 'Original scientific failure' }, async () => {
          await appendFile(join(directory, 'calls'), `${api.taskId}\n`);
          throw new Error('original_native_failure');
        });
      } catch (error) {
        if (!(error instanceof Error) || error.message !== 'original_native_failure') throw error;
      }
    } else await appendFile(join(directory, 'calls'), `${api.taskId}\n`);
    process.stdout.write(`${JSON.stringify({ entered: api.taskId })}\n`);
    await awaitWithContext(new Promise<void>(() => {}), context);
    return {};
  },
})] });
const host = await openResearch({ database: join(directory, 'research.sqlite'), models, extensions: [plugin] });
if (process.argv[3] === 'admission') {
  const input = { goal: 'Recover the original admitted objective', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: { max_trials: 3 } };
  const ids = await host.harness.commit(async (tx) => {
    const requests = await tx.doc(Requests);
    const ids = [];
    for (const requestId of ['admission-gap', 'stopped-gap', 'final-gap']) {
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      await configure(tx, conversation.id, { model: input.model, instructions: input.goal });
      const study = await tx.doc(Study, conversation.id); study.input = input;
      if (requestId !== 'admission-gap') study.final = { status: requestId === 'stopped-gap' ? 'STOPPED' : 'COMPUTED', reason: requestId };
      requests.requests[`request:${requestId}`] = { conversationId: conversation.id, input };
      ids.push(conversation.id);
    }
    return ids;
  }, BACKGROUND_CONTEXT);
  setInterval(() => {}, 1000); // Keep the crash probe alive; parent kills at the committed stdout boundary.
  process.stdout.write(`${JSON.stringify({ entered: true, ids, input })}\n`);
  await new Promise<void>(() => {});
}
const receipt = await host.start('original-request', { goal: 'Crash recovery protocol test', model: { provider: faux.provider.id, modelId: faux.getModel().id }, data: {}, policy: {} });
process.stdout.write(`${JSON.stringify(receipt)}\n`);
await (await host.harness.submission(receipt.submissionId, BACKGROUND_CONTEXT))!.wait(BACKGROUND_CONTEXT);
await host.close();
