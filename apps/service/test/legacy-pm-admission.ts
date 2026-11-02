import assert from 'node:assert/strict';
import { BACKGROUND_CONTEXT } from '@earendil-works/chord/context';
import { configure } from '@earendil-works/pi-durable';
import { researchInstructions } from '../../../packages/contracts/src/research.ts';
import { canonical, Requests, Study } from '../../research/src/documents.ts';
import { researchInput } from '../../research/src/input.ts';
import { CampaignResearchDoc } from '../../research/src/pm-campaign.ts';
import { object } from '../src/facts.ts';
import type { Service } from '../src/service.ts';

/** Retained native PM regressions resume an exact pre-review admission gap.
 * New Review admission/qualification is tested separately. Never remove a
 * frozen policy, manufacture a report, change campaign budgets or alter input.
 */
export function preserveLegacyPmAdmission(service: Service) {
  const start = service.work.startResearch;
  service.work.startResearch = async (value, target) => {
    const body = object(value); const input = researchInput(body.input);
    assert.ok(input.campaign, 'Only the original PM monitor campaign may use this legacy fixture.');
    assert.equal(body.requestId, `pm:${input.campaign.campaignId}`);
    assert.equal(input.policy.reviewPolicy, undefined);
    assert.equal(input.policy.reviewPolicies, undefined);
    await service.research.harness.commit(async tx => {
      const requests = await tx.doc(Requests); const key = `request:${body.requestId}`;
      const previous = requests.requests[key];
      if (previous) {
        assert.equal(canonical(previous.input), canonical(input));
        const original = (await tx.doc(Study, previous.conversationId)).input!;
        assert.equal(original.policy.reviewPolicy, undefined);
        assert.equal(original.policy.reviewPolicies, undefined);
        return;
      }
      const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
      await configure(tx, conversation.id, { model: input.model, thinkingLevel: input.thinkingLevel ?? 'medium', instructions: researchInstructions(input) });
      const original = await tx.doc(Study, conversation.id);
      assert.equal(original.trials.length, 0); assert.equal(original.final, null);
      original.input = structuredClone(input);
      const campaign = await tx.doc(CampaignResearchDoc, conversation.id);
      campaign.binding = structuredClone(input.campaign!);
      campaign.orchestration = { policyVersion: 'qz.research.resources/1',
        maximumModelTurns: Math.min(Number.MAX_SAFE_INTEGER, Math.max(24, input.campaign!.budget.maximumTrials * 12)), generationTaskIds: [] };
      requests.requests[key] = { input, conversationId: conversation.id };
    }, BACKGROUND_CONTEXT);
    // Exercise the original service body capture and immutable-request replay.
    return start(value, target);
  };
}
