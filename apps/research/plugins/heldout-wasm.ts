import { finalizeFactor } from './final-evaluation.ts';
import { Type } from '@earendil-works/pi-ai';
import { defineExtension, defineTool } from '@earendil-works/pi-durable';
import { finalizeGroup } from './heldout-group.ts';

export default defineExtension({
  name: 'qz.heldout-wasm',
  tools: [defineTool({
    name: 'factor_finalize', description: 'Select one recorded Wasm factor before a single unseen held-out assessment. End research; final metrics are only published to the owner, never returned for tuning.',
    parameters: Type.Object({ reason: Type.String(), trialId: Type.Integer() }),
    replay: 'safe', executionMode: 'sequential',
    async execute(args, api, context) {
      await finalizeFactor(args, api, context);
      return { content: [{ type: 'text', text: 'Research ended. Selection, exposure and final outcome are recorded for the owner; no final tuning feedback is returned.' }], control: { terminate: true } };
    },
  }), finalizeGroup],
});
