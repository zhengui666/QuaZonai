import nativeWalkForward from '../plugins/native-walk-forward.ts';
import researchAssessment from '../plugins/research-assessment.ts';
import nativeStrategyBacktest from '../plugins/native-strategy-backtest.ts';
import { configure, defineTask, ProviderDoc } from '@earendil-works/pi-durable';
import type { ConversationId, JsonObject, SubmissionId } from '@earendil-works/pi-durable';
import type { ModelThinkingLevel } from '@earendil-works/pi-ai';
import { Requests, Study } from './documents.ts';
import type { ResearchInput } from './documents.ts';
import { bindReviewSourceEvidence } from './review-source.ts';
import type { CapturedReviewSource } from './review-source.ts';
import { researchInstructions } from '../../../packages/contracts/src/research.ts';
import { ModelTiers } from './models.ts';
import indicatorStrategy from '../plugins/indicator-strategy.ts';
import { nativeResearchExtension } from '../plugins/research.ts';

export type ResearchAdmission = { conversationId: ConversationId; submissionId: SubmissionId };
type IntakeInput = { requestId: string; input: ResearchInput; thinkingLevel: ModelThinkingLevel; toolPolicy?: 'NAUTILUS_ONLY'; sourceEvidence?: JsonObject };
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
        const upstreamBinding = input.data.native_strategy_backtest_request !== undefined;
        const nativeBinding = input.data.indicator_strategy_request !== undefined || input.data.indicator_strategy_batch_request !== undefined;
        await configure(tx, conversation.id, { model: input.model, thinkingLevel: task.input.thinkingLevel, instructions: upstreamBinding ? `${researchInstructions(input)}\nUse native_strategy_backtest for the exact bound historical upstream strategy. Do not infer paper/live deployment or trading permission.` : nativeBinding
          ? `${researchInstructions(input)}\nUse native_indicator_strategy_evaluate for the exact bound historical strategy. Do not call legacy factor, review, fitting or optimization tools. A backtest is not trading permission.` : researchInstructions(input),
          extensions: upstreamBinding ? [nativeStrategyBacktest, nativeWalkForward, researchAssessment] : nativeBinding ? [indicatorStrategy, researchAssessment] : [nativeResearchExtension(), indicatorStrategy, researchAssessment], tools: null,
        });
        if (input.serviceTier) (await tx.doc(ModelTiers)).tiers[(await tx.doc(ProviderDoc, conversation.id)).sessionId] = input.serviceTier;
        (await tx.doc(Study, conversation.id)).toolPolicy = 'NAUTILUS_ONLY';
        (await tx.doc(Study, conversation.id)).input = JSON.parse(JSON.stringify(input));
        if (task.input.sourceEvidence) await bindReviewSourceEvidence(tx, conversation.id, input.data, task.input.sourceEvidence as unknown as CapturedReviewSource);
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
