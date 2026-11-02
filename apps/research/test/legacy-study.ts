import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { configure } from '@earendil-works/pi-durable';
import { researchInstructions } from '../../../packages/contracts/src/research.ts';
import { canonical, Requests, Study } from '../src/documents.ts';
import { researchInput } from '../src/input.ts';
import type { ResearchHost } from '../src/host.ts';

/** Reconstruct a pre-review immutable admission gap in real Durable storage.
 * Only legacy scientific/protocol regressions use this helper; new admissions
 * and quantitative-review tests exercise host.start's production defaults.
 */
export async function startLegacyResearch(host: ResearchHost, requestId: string, value: unknown) {
  const input = researchInput(value);
  await host.harness.commit(async tx => {
    const requests = await tx.doc(Requests); const key = `request:${requestId}`;
    const previous = requests.requests[key];
    if (previous) {
      if (canonical(previous.input) !== canonical(input)) throw new Error('request_id_conflict');
      return;
    }
    const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
    await configure(tx, conversation.id, { model: input.model, thinkingLevel: input.thinkingLevel ?? 'medium', instructions: researchInstructions(input) });
    (await tx.doc(Study, conversation.id)).input = structuredClone(input);
    requests.requests[key] = { input, conversationId: conversation.id };
  }, BACKGROUND_CONTEXT);
  return host.start(requestId, input);
}
