import { configure, defineTask, ProviderDoc } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, SubmissionId } from '@earendil-works/pi-durable';
import type { ModelThinkingLevel } from '@earendil-works/pi-ai';
import { Requests, Study } from './documents.ts';
import type { ResearchInput } from './documents.ts';
import { freezeReviewPolicies } from './review-workflow.ts';
import { bindReviewSourceEvidence } from './review-source.ts';
import type { CapturedReviewSource } from './review-source.ts';
import { CampaignResearchDoc } from './pm-campaign.ts';
import { researchInstructions } from '../../../packages/contracts/src/research.ts';
import { ModelTiers } from './models.ts';

export type ResearchAdmission = { conversationId: ConversationId; submissionId: SubmissionId };
type IntakeInput = { requestId: string; input: ResearchInput; thinkingLevel: ModelThinkingLevel; sourceEvidence?: JsonObject };
type IntakeCheckpoint = { phase: 'create' } | { phase: 'submit'; conversationId: ConversationId };

// Creation writes tables; submit(tx, draft) must precede table writes. Two
// official phases make that boundary durable without a startup repair scan.
export const ResearchIntakeTask = defineTask<IntakeInput, IntakeCheckpoint, ResearchAdmission>({
  name: 'qz.research.intake', version: 1, initial: () => ({ phase: 'create' }),
  phases: {
    async create(task, runtime, context) {
      await runtime.commit(async tx => {
        const conversation = await tx.createConversation({ ownership: { kind: 'ownerless' } });
        const input = task.input.input;
        await configure(tx, conversation.id, { model: input.model, thinkingLevel: task.input.thinkingLevel, instructions: researchInstructions(input) });
        if (input.serviceTier) (await tx.doc(ModelTiers)).tiers[(await tx.doc(ProviderDoc, conversation.id)).sessionId] = input.serviceTier;
        (await tx.doc(Study, conversation.id)).input = freezeReviewPolicies(input);
        if (task.input.sourceEvidence) await bindReviewSourceEvidence(tx, conversation.id, input.data, task.input.sourceEvidence as unknown as CapturedReviewSource);
        if (input.campaign) {
          const campaign = await tx.doc(CampaignResearchDoc, conversation.id);
          campaign.binding = input.campaign;
          campaign.orchestration = { policyVersion: 'qz.research.resources/1', generationTaskIds: [] };
        }
        (await tx.doc(Requests)).requests[`request:${task.input.requestId}`]!.conversationId = conversation.id;
        return { status: 'running', checkpoint: { phase: 'submit', conversationId: conversation.id } };
      }, context);
    },
    async submit(task, runtime, context) {
      const conversationId = task.state.checkpoint.conversationId;
      const handle = await runtime.conversation(conversationId, context);
      if (!handle) throw new Error('research_conversation_missing');
      await runtime.commit(async tx => {
        const study = await tx.doc(Study, conversationId);
        if (study.final || study.deletedAt) throw new Error('research_already_final');
        const submissionId = await handle.submit(tx, { type: 'input', content: researchInstructions(task.input.input), requestId: task.input.requestId });
        (await tx.doc(Requests)).requests[`request:${task.input.requestId}`]!.submissionId = submissionId;
        return { status: 'terminal', outcome: { status: 'completed', result: { conversationId, submissionId } } };
      }, context);
    },
  },
  abort: (_task, runtime, context) => runtime.commit(() => ({ status: 'terminal', outcome: { status: 'aborted' } }), context),
});
